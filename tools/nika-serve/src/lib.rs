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
//! TODO(v0.57): Job GC -- cleanup completed/failed jobs > 7 days
//! TODO(v0.57): \[serve\] section in config.toml

pub mod auth;
pub mod config;
pub mod error;
pub mod routes;
pub mod state;
pub mod worker;

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use axum::middleware;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
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
    let state = AppState {
        storage,
        config: Arc::new(config.clone()),
        semaphore: Arc::new(Semaphore::new(config.max_concurrent)),
        shutdown: shutdown_rx,
        workers: Arc::new(Mutex::new(HashMap::new())),
        active_jobs: Arc::new(AtomicUsize::new(0)),
    };

    // Build router with middleware
    let mut app = routes::build_router(state.clone())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)); // 10 MB body limit

    // CORS layer — only when explicitly configured (default: no CORS headers)
    if let Some(origin) = &config.cors_origin {
        let cors = CorsLayer::new()
            .allow_origin(
                origin
                    .parse::<axum::http::HeaderValue>()
                    .expect("invalid CORS origin"),
            )
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

    // Graceful shutdown signal
    let shutdown_signal = async {
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
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .map_err(|e| ServeError::Internal(Box::new(e)))?;

    info!("server stopped, draining workers...");

    // Signal shutdown to workers
    let _ = shutdown_tx.send(true);

    // Drain active workers with a 30s timeout
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

    let drain_future = async {
        for (i, handle) in join_handles.into_iter().enumerate() {
            if let Err(e) = handle.await {
                tracing::warn!(job_id = %ids[i], error = %e, "worker panicked during drain");
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
            auth_token: "test-token-1234567".into(), // >=16 chars
            cors_origin: None,
        };

        let state = AppState {
            storage,
            config: Arc::new(config.clone()),
            semaphore: Arc::new(Semaphore::new(4)),
            shutdown: shutdown_rx,
            workers: Arc::new(Mutex::new(HashMap::new())),
            active_jobs: Arc::new(AtomicUsize::new(0)),
        };

        let app = routes::build_router(state.clone())
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            ));

        (app, state)
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
            .header("authorization", "Bearer test-token-1234567")
            .body(Body::from(r#"{"workflow":"../../etc/passwd"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn nonexistent_job_404() {
        let (app, _state) = test_app().await;
        let req = Request::get("/v1/status/nope")
            .header("authorization", "Bearer test-token-1234567")
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
            .header("authorization", "Bearer test-token-1234567")
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
            .header("authorization", "Bearer test-token-1234567")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
