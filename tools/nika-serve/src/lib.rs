//! Nika HTTP API server (`nika serve`).
//!
//! Exposes workflow execution over HTTP with Bearer token authentication,
//! SQLite-backed job persistence, and subprocess-based execution.
//!
//! V1 architecture: every workflow run spawns `nika run <workflow>` as a child
//! process (no embedded engine). This keeps the server lean and crash-isolated.
//!
//! ## Endpoints
//!
//! | Method | Path              | Description              |
//! |--------|-------------------|--------------------------|
//! | GET    | `/health`         | Health check (no auth)   |
//! | POST   | `/v1/run`         | Submit workflow           |
//! | GET    | `/v1/status/{id}` | Poll job status           |
//! | POST   | `/v1/cancel/{id}` | Cancel running job        |
//!
//! Configuration is loaded from `[serve]` section in `nika.toml`, with env var overrides.

pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod executor;
pub mod metrics;
pub mod openapi;
pub mod rate_limit;
pub mod request_id;
pub mod routes;
pub mod state;
pub mod webhook;
pub mod worker;

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use axum::middleware;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;
use tracing::info;

use crate::config::ServeConfig;
use crate::error::ServeError;
use crate::state::AppState;

/// Start the HTTP API server with the given configuration.
///
/// This is the main entry point called from `nika serve`.
/// Blocks until the server receives a shutdown signal (SIGTERM/SIGINT).
pub async fn run_server(config: ServeConfig) -> Result<(), ServeError> {
    // FIX-14: Prevent two nika serve instances sharing the same DB
    let _db_lock = acquire_db_lock(&config.db_path)?;

    // Open SQLite storage
    let storage = nika_storage::Storage::open(&config.db_path)?;

    // Reset any jobs stuck in "running" from a previous crash (ERRATA-10)
    let reset_count = storage.reset_stale_running("Server restarted").await?;
    if reset_count > 0 {
        info!(
            count = reset_count,
            "reset stale running jobs from previous session"
        );
    }

    // Shutdown channel
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Build shared state
    // Load webhook config once at startup (BUG-8) + resolve DNS and pin IP (M1)
    let webhook_config = match crate::webhook::WebhookConfig::from_env() {
        Some(mut wh) => {
            wh.resolve_and_pin().await;
            Some(wh)
        }
        None => None,
    };

    let exec = match config.executor_mode {
        config::ExecutorMode::Subprocess => executor::Executor::Subprocess,
        config::ExecutorMode::Embedded => {
            info!("using embedded executor (in-process Runner)");
            executor::Executor::Embedded
        }
    };

    if config.executor_mode == config::ExecutorMode::Embedded {
        info!("panic isolation enabled: workflow panics are caught at task boundary");
    }

    let state = AppState {
        storage,
        config: Arc::new(config.clone()),
        executor: exec,
        semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
        shutdown: shutdown_rx,
        workers: Arc::new(Mutex::new(HashMap::new())),
        active_jobs: Arc::new(AtomicUsize::new(0)),
        event_bus: events::EventBus::default(),
        webhook_config,
    };

    // Install Prometheus metrics recorder
    let metrics_handle = metrics::install_recorder();

    // Per-token rate limiter (configurable via env/nika.toml)
    let limiter =
        rate_limit::new_rate_limiter_with(config.rate_per_second as u32, config.rate_burst);

    // Build router with middleware
    // SSE route is separate — long-lived streams must NOT have the 30s TimeoutLayer (C1).
    // M3: Auth runs BEFORE rate-limit so unauthenticated requests don't grow DashMap.
    // Axum layers: outermost (added last) runs first in request path.
    // Execution order: request-id → timeout → body-limit → auth → rate-limit → handler
    let api_routes = routes::build_router(state.clone())
        .layer(middleware::from_fn_with_state(
            limiter.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // 10 MB body limit
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            std::time::Duration::from_secs(30),
        ))
        .layer(middleware::from_fn(request_id::request_id_middleware))
        .layer(middleware::from_fn(crate::metrics::http_metrics_middleware));

    // SSE router: auth → rate-limit, NO TimeoutLayer (C1)
    let sse_routes = routes::build_sse_router(state.clone())
        .layer(middleware::from_fn_with_state(
            limiter,
            rate_limit::rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .layer(middleware::from_fn(request_id::request_id_middleware))
        .layer(middleware::from_fn(crate::metrics::http_metrics_middleware));

    let mut app = api_routes.merge(sse_routes);

    // Merge /metrics endpoint (behind auth — metrics can leak sensitive info)
    if let Some(handle) = metrics_handle {
        app = app.merge(routes::build_metrics_router(handle).layer(
            middleware::from_fn_with_state(state.clone(), auth::require_auth),
        ));
    }

    // CORS layer — only when explicitly configured (default: no CORS headers)
    if let Some(origin) = &config.cors_origin {
        let header_value = origin.parse::<axum::http::HeaderValue>().map_err(|e| {
            ServeError::Config(format!("invalid NIKA_SERVE_CORS_ORIGIN '{origin}': {e}"))
        })?;
        let cors = CorsLayer::new()
            .allow_origin(header_value)
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
            ]);
        app = app.layer(cors);
    }

    // Bind listener
    let listener = TcpListener::bind(config.bind)
        .await
        .map_err(|e| ServeError::Config(format!("failed to bind {}: {e}", config.bind)))?;

    info!(bind = %config.bind, max_concurrent = config.max_concurrent, "nika serve starting");

    // Startup banner (visible to the user, not just tracing logs)
    print_startup_banner(&config);

    // Spawn background job GC (configurable interval + retention)
    let gc_storage = state.storage.clone();
    let gc_interval = std::time::Duration::from_secs(config.gc_interval_secs);
    let gc_retention = config.gc_retention_secs;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(gc_interval).await;
            match gc_storage.delete_old_jobs(gc_retention).await {
                Ok(0) => {}
                Ok(n) => info!(count = n, "job GC: deleted old jobs"),
                Err(e) => tracing::warn!(error = %e, "job GC failed"),
            }
        }
    });

    // Graceful shutdown signal — notify workers IMMEDIATELY on signal,
    // not after Axum finishes draining connections (BUG-3).
    let shutdown_signal = async move {
        // ERRATA-18: If signal setup fails, use pending (never completes => manual kill only)
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(_) => {
                    tracing::warn!("failed to install SIGTERM handler, using pending fallback");
                    std::future::pending::<()>().await;
                    return;
                }
            };
            let mut sigint = match signal(SignalKind::interrupt()) {
                Ok(s) => s,
                Err(_) => {
                    tracing::warn!("failed to install SIGINT handler, using pending fallback");
                    std::future::pending::<()>().await;
                    return;
                }
            };
            tokio::select! {
                _ = sigterm.recv() => info!("received SIGTERM"),
                _ = sigint.recv() => info!("received SIGINT"),
            }
        }

        #[cfg(not(unix))]
        {
            match tokio::signal::ctrl_c().await {
                Ok(()) => info!("received Ctrl+C"),
                Err(_) => std::future::pending::<()>().await,
            }
        }

        info!("shutdown signal received, notifying workers");
        let _ = shutdown_tx.send(true);
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .map_err(|e| ServeError::Internal(Box::new(e)))?;

    info!("server stopped, draining workers...");

    // Workers already notified via shutdown_tx in signal handler — just drain
    drain_workers(state.workers.clone()).await;

    info!("all workers drained, goodbye");
    Ok(())
}

/// Wait for all active workers to complete, with a 30-second timeout.
/// After timeout, abort remaining worker tasks (FIX-8).
async fn drain_workers(workers: Arc<Mutex<HashMap<String, state::WorkerHandle>>>) {
    let handles: Vec<_> = {
        let mut map = workers.lock().await;
        map.drain().collect()
    };

    if handles.is_empty() {
        return;
    }

    let ids: Vec<String> = handles.iter().map(|(id, _)| id.clone()).collect();
    let join_handles: Vec<tokio::task::JoinHandle<()>> =
        handles.into_iter().map(|(_, wh)| wh.join).collect();

    info!(
        count = join_handles.len(),
        "waiting for active workers to complete"
    );

    // Collect abort handles before consuming JoinHandles
    let abort_handles: Vec<tokio::task::AbortHandle> =
        join_handles.iter().map(|h| h.abort_handle()).collect();

    // H4: Drain all workers in parallel (not sequentially) so each gets the full 30s
    let ids_clone = ids.clone();
    let drain_future = async move {
        let results = futures_util::future::join_all(join_handles).await;
        for (i, result) in results.into_iter().enumerate() {
            if let Err(e) = result {
                tracing::warn!(job_id = %ids_clone[i], error = %e, "worker panicked during drain");
            }
        }
    };

    if tokio::time::timeout(std::time::Duration::from_secs(30), drain_future)
        .await
        .is_err()
    {
        tracing::warn!("drain timeout (30s) -- aborting remaining workers");
        for (i, ah) in abort_handles.into_iter().enumerate() {
            if !ah.is_finished() {
                tracing::warn!(job_id = %ids[i], "aborting worker");
                ah.abort();
            }
        }
    }
}

/// Opaque lock handle — held for the lifetime of the server.
struct DbLock {
    #[cfg(unix)]
    _flock: nix::fcntl::Flock<std::fs::File>,
    #[cfg(not(unix))]
    _file: std::fs::File,
}

/// Acquire an exclusive file lock on the database path to prevent two
/// `nika serve` instances from writing to the same SQLite file (FIX-14).
///
/// Returns the lock handle — the lock is held as long as it's alive.
fn acquire_db_lock(db_path: &std::path::Path) -> Result<DbLock, ServeError> {
    let lock_path = db_path.with_extension("lock");

    // Ensure parent directory exists
    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| ServeError::Config(format!("failed to open lock file: {e}")))?;

    #[cfg(unix)]
    {
        use nix::fcntl::{Flock, FlockArg};
        let flock = Flock::lock(lock_file, FlockArg::LockExclusiveNonblock).map_err(|_| {
            ServeError::Config(
                "Another nika serve instance is using this database. \
                 Use a different --db path or stop the other instance."
                    .into(),
            )
        })?;
        Ok(DbLock { _flock: flock })
    }

    #[cfg(not(unix))]
    Ok(DbLock { _file: lock_file })
}

/// Print a structured startup banner to stderr.
fn print_startup_banner(config: &ServeConfig) {
    use nika_engine::core::{provider_to_env_var, ProviderCategory, KNOWN_PROVIDERS};

    let version = env!("CARGO_PKG_VERSION");
    let executor = match config.executor_mode {
        config::ExecutorMode::Subprocess => "subprocess",
        config::ExecutorMode::Embedded => "embedded",
    };
    let workflow_count = std::fs::read_dir(&config.workflows_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .is_some_and(|n| n.ends_with(".nika.yaml") || n.ends_with(".nika.yml"))
                })
                .count()
        })
        .unwrap_or(0);
    let wf_dir = config.workflows_dir.display();
    let mut configured = Vec::new();
    let mut missing = Vec::new();
    for p in KNOWN_PROVIDERS
        .iter()
        .filter(|p| p.category == ProviderCategory::Llm)
    {
        let env_var = provider_to_env_var(p.id).unwrap_or("UNKNOWN");
        if std::env::var(env_var).is_ok_and(|v| !v.is_empty()) {
            configured.push(p.id);
        } else {
            missing.push(p.id);
        }
    }
    let providers_str = configured
        .iter()
        .map(|p| format!("{p} \u{2713}"))
        .chain(missing.iter().take(3).map(|p| format!("{p} \u{2717}")))
        .collect::<Vec<_>>()
        .join("  ");
    let token_len = config.auth_token.len();

    eprintln!();
    eprintln!("  \u{1f98b} Nika Serve v{version}");
    eprintln!();
    eprintln!(
        "  \u{251c}\u{2500}\u{2500} Listening    http://{}",
        config.bind
    );
    eprintln!(
        "  \u{251c}\u{2500}\u{2500} Workflows    {wf_dir} ({workflow_count} file{})",
        if workflow_count == 1 { "" } else { "s" }
    );
    eprintln!("  \u{251c}\u{2500}\u{2500} Executor     {executor}");
    eprintln!(
        "  \u{251c}\u{2500}\u{2500} Max jobs     {} concurrent",
        config.max_concurrent
    );
    eprintln!(
        "  \u{251c}\u{2500}\u{2500} Timeout      {}s per job",
        config.job_timeout_secs
    );
    eprintln!("  \u{251c}\u{2500}\u{2500} Auth         Bearer token ({token_len} chars)");
    eprintln!("  \u{2514}\u{2500}\u{2500} Providers    {providers_str}");
    eprintln!();
    eprintln!("  Ready. Ctrl+C to stop.");
    eprintln!();
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Build a test app with an in-memory storage backend.
    async fn test_app() -> (axum::Router, AppState) {
        let storage = nika_storage::Storage::open_memory().expect("open in-memory storage");
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);

        let config = ServeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workflows_dir: std::path::PathBuf::from("/tmp/nika-test-workflows"),
            max_concurrent: 4,
            job_timeout_secs: 60,
            max_output_bytes: 1024,
            db_path: std::path::PathBuf::from(":memory:"),
            auth_token: "test-token-1234567890abcdef1234567".into(), // >=32 chars
            cors_origin: None,
            executor_mode: config::ExecutorMode::Embedded,
            rate_per_second: 10,
            rate_burst: 30,
            gc_retention_secs: 7 * 24 * 3600,
            gc_interval_secs: 3600,
            project_root: None,
            working_dir_mode: None,
        };

        let state = AppState {
            storage,
            config: Arc::new(config.clone()),
            executor: executor::Executor::Subprocess,
            semaphore: Arc::new(Semaphore::new(4)),
            shutdown: shutdown_rx,
            workers: Arc::new(Mutex::new(HashMap::new())),
            active_jobs: Arc::new(AtomicUsize::new(0)),
            event_bus: events::EventBus::default(),
            webhook_config: None,
        };

        let limiter = rate_limit::new_rate_limiter();
        let app = routes::build_router(state.clone())
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            ))
            .layer(middleware::from_fn_with_state(
                limiter,
                rate_limit::rate_limit_middleware,
            ))
            .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
            .layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                std::time::Duration::from_secs(30),
            ))
            .layer(middleware::from_fn(request_id::request_id_middleware));

        (app, state)
    }

    #[tokio::test]
    async fn request_timeout_layer_present() {
        let (app, _state) = test_app().await;
        // Validates that the TimeoutLayer is wired in and requests complete normally
        let req = Request::get("/health").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_ok() {
        let (app, _state) = test_app().await;
        let req = Request::get("/health").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_bypasses_auth() {
        let (app, _state) = test_app().await;
        // No Authorization header -- should still return 200
        let req = Request::get("/health").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_auth_returns_401() {
        let (app, _state) = test_app().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"workflow":"test.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_token_returns_401() {
        let (app, _state) = test_app().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", "Bearer wrong-token")
            .body(Body::from(r#"{"workflow":"test.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn path_traversal_rejected() {
        let (app, _state) = test_app().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", "Bearer test-token-1234567890abcdef1234567")
            .body(Body::from(r#"{"workflow":"../../etc/passwd"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn nonexistent_job_404() {
        let (app, _state) = test_app().await;
        let req = Request::get("/v1/status/nope")
            .header("authorization", "Bearer test-token-1234567890abcdef1234567")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn valid_auth_with_invalid_workflow() {
        let (app, _state) = test_app().await;
        // Valid token but the workflow file doesn't exist on disk
        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", "Bearer test-token-1234567890abcdef1234567")
            .body(Body::from(r#"{"workflow":"nonexistent.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Should be 400 (InvalidWorkflow -- file not found) since the
        // workflows dir doesn't exist in the test environment
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn cancel_nonexistent_job_404() {
        let (app, _state) = test_app().await;
        let req = Request::builder()
            .method("POST")
            .uri("/v1/cancel/nonexistent")
            .header("authorization", "Bearer test-token-1234567890abcdef1234567")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn request_id_generated_when_absent() {
        let (app, _state) = test_app().await;
        let req = Request::get("/health").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let id = resp
            .headers()
            .get("x-request-id")
            .expect("x-request-id header must be present");
        let id_str = id.to_str().unwrap();
        assert_eq!(id_str.len(), 32, "generated ID should be 32 hex chars");
        assert!(id_str.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn request_id_echoed_when_provided() {
        let (app, _state) = test_app().await;
        let req = Request::get("/health")
            .header("x-request-id", "my-custom-id-123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let id = resp
            .headers()
            .get("x-request-id")
            .expect("x-request-id header must be present");
        assert_eq!(id.to_str().unwrap(), "my-custom-id-123");
    }

    // ── GET /v1/workflows/{name}/source ──────────────────────────────────

    const AUTH: &str = "Bearer test-token-1234567890abcdef1234567";

    /// Build a test app with a real workflows directory (tempdir).
    async fn test_app_with_dir(
        workflows_dir: std::path::PathBuf,
    ) -> (axum::Router, AppState) {
        let storage = nika_storage::Storage::open_memory().expect("open in-memory storage");
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);

        let config = ServeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workflows_dir,
            max_concurrent: 4,
            job_timeout_secs: 60,
            max_output_bytes: 1024,
            db_path: std::path::PathBuf::from(":memory:"),
            auth_token: "test-token-1234567890abcdef1234567".into(),
            cors_origin: None,
            executor_mode: config::ExecutorMode::Embedded,
            rate_per_second: 100,
            rate_burst: 100,
            gc_retention_secs: 7 * 24 * 3600,
            gc_interval_secs: 3600,
            project_root: None,
            working_dir_mode: None,
        };

        let state = AppState {
            storage,
            config: Arc::new(config),
            executor: executor::Executor::Subprocess,
            semaphore: Arc::new(Semaphore::new(4)),
            shutdown: shutdown_rx,
            workers: Arc::new(Mutex::new(HashMap::new())),
            active_jobs: Arc::new(AtomicUsize::new(0)),
            event_bus: events::EventBus::default(),
            webhook_config: None,
        };

        let limiter = rate_limit::new_rate_limiter();
        let app = routes::build_router(state.clone())
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            ))
            .layer(middleware::from_fn_with_state(
                limiter,
                rate_limit::rate_limit_middleware,
            ))
            .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
            .layer(TimeoutLayer::with_status_code(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                std::time::Duration::from_secs(30),
            ))
            .layer(middleware::from_fn(request_id::request_id_middleware));

        (app, state)
    }

    /// Helper: read response body as string.
    async fn body_string(resp: axum::http::Response<Body>) -> String {
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn source_returns_yaml_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: hello\n\ntasks:\n  - id: greet\n    infer: \"Say hello\"\n";
        std::fs::write(dir.path().join("hello.nika.yaml"), yaml).unwrap();

        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;
        let req = Request::get("/v1/workflows/hello.nika.yaml/source")
            .header("authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "text/plain; charset=utf-8"
        );
        assert_eq!(body_string(resp).await, yaml);
    }

    #[tokio::test]
    async fn source_returns_404_for_missing_workflow() {
        let dir = tempfile::TempDir::new().unwrap();
        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;

        let req = Request::get("/v1/workflows/nope.nika.yaml/source")
            .header("authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        // canonicalize fails → InvalidWorkflow → 400
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_string(resp).await;
        assert!(body.contains("workflow not found"), "body: {body}");
    }

    #[tokio::test]
    async fn source_rejects_traversal() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("ok.nika.yaml"), "schema: test").unwrap();
        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;

        let req = Request::get("/v1/workflows/..%2F..%2Fetc%2Fpasswd.nika.yaml/source")
            .header("authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn source_rejects_wrong_extension() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("secrets.txt"), "top secret").unwrap();
        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;

        let req = Request::get("/v1/workflows/secrets.txt/source")
            .header("authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_string(resp).await;
        assert!(body.contains(".nika.yaml"), "body: {body}");
    }

    #[tokio::test]
    async fn source_requires_auth() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.nika.yaml"), "schema: test").unwrap();
        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;

        let req = Request::get("/v1/workflows/test.nika.yaml/source")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn source_nested_subdirectory_workflow() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("pipelines/prod")).unwrap();
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: deploy\n";
        std::fs::write(
            dir.path().join("pipelines/prod/deploy.nika.yaml"),
            yaml,
        )
        .unwrap();

        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;
        // URL-encode the slashes for the path parameter
        let req = Request::get(
            "/v1/workflows/pipelines%2Fprod%2Fdeploy.nika.yaml/source",
        )
        .header("authorization", AUTH)
        .body(Body::empty())
        .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_string(resp).await, yaml);
    }
}
