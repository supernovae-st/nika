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
pub mod token_store;
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
    // FIX-14: DB lock for SQLite (skipped for PostgreSQL which handles concurrency natively)
    let _db_lock = if config.storage_url.is_none() {
        Some(acquire_db_lock(&config.db_path)?)
    } else {
        None
    };

    // Open storage backend (PostgreSQL if URL set, otherwise SQLite)
    let storage = if let Some(ref url) = config.storage_url {
        #[cfg(feature = "postgres")]
        {
            info!(url = %url.split('@').last().unwrap_or("***"), "connecting to PostgreSQL");
            nika_storage::Storage::connect_postgres(url)
                .await
                .map_err(|e| ServeError::Config(format!("PostgreSQL: {e}")))?
        }
        #[cfg(not(feature = "postgres"))]
        {
            let _ = url;
            return Err(ServeError::Config(
                "NIKA_STORAGE_URL set but nika was built without the 'postgres' feature. \
                 Rebuild with: cargo build --features postgres"
                    .into(),
            ));
        }
    } else {
        nika_storage::Storage::open(&config.db_path)?
    };

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

    // Determine auth mode: MultiKey if tokens exist in DB, else Legacy
    let token_count = storage.count_tokens().await.unwrap_or(0);
    let auth_mode = if token_count > 0 {
        if !config.auth_token.is_empty() {
            tracing::warn!(
                "NIKA_SERVE_TOKEN is set but {} named tokens exist — using multi-key mode \
                 (env var ignored)",
                token_count
            );
        }
        info!(count = token_count, "multi-key auth mode");
        Arc::new(token_store::AuthMode::MultiKey {
            store: token_store::TokenStore::new(storage.clone()),
        })
    } else if config.auth_token.is_empty() {
        return Err(ServeError::Config(
            "No authentication configured. Either:\n  \
             1. Set NIKA_SERVE_TOKEN (legacy mode), or\n  \
             2. Create tokens with: nika token add <name>"
                .into(),
        ));
    } else {
        info!("legacy auth mode (NIKA_SERVE_TOKEN)");
        Arc::new(token_store::AuthMode::Legacy {
            expected_hash: token_store::hash_token(&config.auth_token),
        })
    };

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
        auth_mode: auth_mode.clone(),
    };

    // Install Prometheus metrics recorder
    let metrics_handle = metrics::install_recorder();

    // Per-token rate limiter (configurable via env/nika.toml)
    let rl_state =
        rate_limit::new_rate_limiter_with(config.rate_per_second as u32, config.rate_burst);
    // Clone limiter Arc before rl_state is moved into middleware layers
    let gc_limiter = Arc::clone(&rl_state.limiter);

    // Build router with middleware
    // SSE route is separate — long-lived streams must NOT have the 30s TimeoutLayer (C1).
    // M3: Auth runs BEFORE rate-limit so unauthenticated requests don't grow DashMap.
    // Axum layers: outermost (added last) runs first in request path.
    // Execution order: request-id → timeout → body-limit → auth → rate-limit → handler
    let api_routes = routes::build_router(state.clone())
        .layer(middleware::from_fn_with_state(
            rl_state.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            auth_mode.clone(),
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
            rl_state,
            rate_limit::rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            auth_mode.clone(),
            auth::require_auth,
        ))
        .layer(middleware::from_fn(request_id::request_id_middleware))
        .layer(middleware::from_fn(crate::metrics::http_metrics_middleware));

    let mut app = api_routes.merge(sse_routes);

    // Merge /metrics endpoint (behind auth — metrics can leak sensitive info)
    if let Some(handle) = metrics_handle {
        app = app.merge(routes::build_metrics_router(handle).layer(
            middleware::from_fn_with_state(auth_mode.clone(), auth::require_auth),
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
    print_startup_banner(&config, &auth_mode);

    // Reconcile YAML-declared schedules with DB on startup
    reconcile_yaml_schedules(&state.storage, &config.workflows_dir).await;

    // Spawn periodic re-scan (60s) for YAML schedule changes at runtime
    let recon_storage = state.storage.clone();
    let recon_dir = config.workflows_dir.clone();
    let recon_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            reconcile_yaml_schedules(&recon_storage, &recon_dir).await;
        }
    });

    // Spawn background job GC (configurable interval + retention).
    // S11: Store handle so we can abort on shutdown instead of leaking the task.
    let gc_storage = state.storage.clone();
    let gc_interval = std::time::Duration::from_secs(config.gc_interval_secs);
    let gc_retention = config.gc_retention_secs;
    let gc_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(gc_interval).await;
            match gc_storage.delete_old_jobs(gc_retention).await {
                Ok(0) => {}
                Ok(n) => info!(count = n, "job GC: deleted old jobs"),
                Err(e) => tracing::warn!(error = %e, "job GC failed"),
            }
            // Evict stale rate limiter entries for tokens that haven't been
            // seen recently — prevents unbounded DashMap growth with rotating tokens.
            gc_limiter.retain_recent();
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
        recon_handle.abort();
        gc_handle.abort();
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

/// Count `.nika.yaml` files recursively, skipping hidden directories.
fn count_workflow_files(dir: &std::path::Path) -> usize {
    let mut count = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // Skip hidden directories (.nika/, .git/, etc.)
        if name_str.starts_with('.') {
            continue;
        }
        let ft = entry.file_type();
        if ft.as_ref().is_ok_and(|t| t.is_dir()) {
            count += count_workflow_files(&entry.path());
        } else if name_str.ends_with(".nika.yaml") || name_str.ends_with(".nika.yml") {
            count += 1;
        }
    }
    count
}

/// Scan workflow files for `schedule:` field (header-only, fast).
/// Returns (total_with_schedule, active_count, paused_count).
fn scan_scheduled_workflows(dir: &std::path::Path) -> (usize, usize, usize) {
    let mut total = 0;
    let mut active = 0;
    let mut paused = 0;

    for path in collect_workflow_paths(dir) {
        // Read only the first 50 lines (header-only scan)
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let header: String = content.lines().take(50).collect::<Vec<_>>().join("\n");
        if header.contains("schedule:") {
            total += 1;
            if header.contains("paused: true") {
                paused += 1;
            } else {
                active += 1;
            }
        }
    }
    (total, active, paused)
}

/// Extract the `schedule:` value from a workflow header as a JSON value.
///
/// Handles both string form (`schedule: "@daily"`) and object form:
/// ```yaml
/// schedule:
///   cron: "0 9 * * 1-5"
///   timezone: "Europe/Paris"
///   overlap: queue
/// ```
fn extract_schedule_value(header: &str) -> Option<serde_json::Value> {
    let lines: Vec<&str> = header.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("schedule:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                // String form: schedule: "@daily" or schedule: "0 9 * * *"
                return Some(serde_json::Value::String(val.to_string()));
            }
            // Object form: collect indented lines after "schedule:"
            let mut obj = serde_json::Map::new();
            for next_line in &lines[i + 1..] {
                let next_trimmed = next_line.trim();
                // Stop at next top-level key or empty line
                if next_trimmed.is_empty()
                    || (!next_line.starts_with(' ') && !next_line.starts_with('\t'))
                {
                    break;
                }
                // Parse "key: value" pairs
                if let Some((key, value)) = next_trimmed.split_once(':') {
                    let k = key.trim();
                    let v = value.trim().trim_matches('"').trim_matches('\'');
                    if k == "paused" {
                        obj.insert(k.to_string(), serde_json::Value::Bool(v == "true"));
                    } else if !v.is_empty() {
                        obj.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                    }
                }
            }
            if obj.contains_key("cron") {
                return Some(serde_json::Value::Object(obj));
            }
            // Object form without cron: key — can't parse, skip
            return None;
        }
    }
    None
}

/// Parsed schedule entry from a YAML file (sync, no DB).
struct YamlScheduleEntry {
    name: String,
    rel_path: String,
    cron: String,
    timezone: Option<String>,
    paused: bool,
    overlap: String,
}

/// Scan workflow YAML files for schedule declarations (blocking I/O).
///
/// This is a pure sync function — safe to call from `spawn_blocking`.
fn scan_yaml_schedule_entries(workflows_dir: &std::path::Path) -> Vec<YamlScheduleEntry> {
    let mut entries = Vec::new();
    for path in collect_workflow_paths(workflows_dir) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let header: String = content.lines().take(50).collect::<Vec<_>>().join("\n");
        let Some(sched_val) = extract_schedule_value(&header) else {
            continue;
        };

        let config = match nika_core::ast::schedule::parse_schedule_value(
            &sched_val,
            nika_core::source::Span::dummy(),
        ) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "invalid schedule in YAML — skipping");
                continue;
            }
        };

        let rel = path
            .strip_prefix(workflows_dir)
            .unwrap_or(&path)
            .display()
            .to_string();
        let name = rel
            .trim_end_matches(".nika.yaml")
            .trim_end_matches(".nika.yml")
            .replace('/', "::");

        entries.push(YamlScheduleEntry {
            name,
            rel_path: rel,
            cron: config.cron,
            timezone: config.timezone,
            paused: config.paused,
            overlap: config.overlap.to_string(),
        });
    }
    entries
}

/// Reconcile YAML-declared schedules with the database.
///
/// Implements 5 rules:
/// 1. YAML has schedule, DB doesn't → INSERT
/// 2. YAML has schedule, DB cron differs → UPDATE
/// 3. DB source="yaml", YAML removed → DELETE
/// 4. DB source="cli", YAML removed → KEEP
/// 5. YAML paused differs → update paused state
async fn reconcile_yaml_schedules(
    storage: &nika_storage::Storage,
    workflows_dir: &std::path::Path,
) {
    use std::collections::HashMap;

    // Phase 1: blocking file scan off the async runtime
    let dir = workflows_dir.to_path_buf();
    let yaml_entries = tokio::task::spawn_blocking(move || scan_yaml_schedule_entries(&dir))
        .await
        .unwrap_or_default();

    // Phase 2: async DB reconciliation
    let mut yaml_schedules: HashMap<String, (String, bool)> = HashMap::new();
    for entry in &yaml_entries {
        yaml_schedules.insert(entry.name.clone(), (entry.cron.clone(), entry.paused));
    }

    for entry in &yaml_entries {
        let name = &entry.name;
        let config_cron = &entry.cron;
        let paused = entry.paused;

        // Rule 1 & 2: check DB
        match storage.get_schedule_by_name(name).await {
            Ok(Some(existing)) => {
                // Rule 2: cron or paused changed → update
                if existing.cron_expr != *config_cron || existing.paused != paused {
                    let next_run = config_cron
                        .parse::<croner::Cron>()
                        .ok()
                        .and_then(|c| c.find_next_occurrence(&chrono::Utc::now(), false).ok())
                        .map(|dt| dt.to_rfc3339());
                    if let Err(e) = storage
                        .update_schedule_cron(
                            &existing.id,
                            config_cron,
                            next_run.as_deref(),
                            paused,
                        )
                        .await
                    {
                        tracing::warn!(name = %name, error = %e, "reconcile: update failed");
                    } else {
                        info!(name = %name, cron = %config_cron, "reconcile: updated schedule");
                    }
                }
            }
            Ok(None) => {
                // Rule 1: insert new schedule
                let id = uuid::Uuid::new_v4().to_string();
                let now = chrono::Utc::now();
                let next_run = config_cron
                    .parse::<croner::Cron>()
                    .ok()
                    .and_then(|c| c.find_next_occurrence(&now, false).ok())
                    .map(|dt| dt.to_rfc3339());
                let sched = nika_storage::CronSchedule {
                    id,
                    name: name.clone(),
                    workflow: entry.rel_path.clone(),
                    cron_expr: config_cron.clone(),
                    timezone: entry.timezone.clone().unwrap_or_else(|| "UTC".to_string()),
                    paused,
                    source: "yaml".to_string(),
                    overlap: entry.overlap.clone(),
                    inputs_json: None,
                    last_run_at: None,
                    next_run_at: next_run,
                    run_count: 0,
                    last_job_id: None,
                    created_at: now.to_rfc3339(),
                    updated_at: now.to_rfc3339(),
                };
                if let Err(e) = storage.insert_schedule(sched).await {
                    tracing::warn!(name = %name, error = %e, "reconcile: insert failed");
                } else {
                    info!(name = %name, cron = %config_cron, "reconcile: registered YAML schedule");
                }
            }
            Err(e) => {
                tracing::warn!(name = %name, error = %e, "reconcile: DB query failed");
            }
        }
    }

    // Rule 3: delete DB schedules with source=yaml that no longer exist in YAML
    // Rule 4: keep source=cli schedules untouched
    if let Ok(all_schedules) = storage.list_schedules(false).await {
        for sched in all_schedules {
            if sched.source == "yaml" && !yaml_schedules.contains_key(&sched.name) {
                if let Err(e) = storage.delete_schedule(&sched.id).await {
                    tracing::warn!(name = %sched.name, error = %e, "reconcile: delete failed");
                } else {
                    info!(name = %sched.name, "reconcile: removed orphaned YAML schedule");
                }
            }
        }
    }
}

/// Collect all .nika.yaml paths recursively.
fn collect_workflow_paths(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        tracing::warn!(dir = %dir.display(), "cannot read directory during workflow scan");
        return paths;
    };
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(dir = %dir.display(), error = %e, "directory entry error during scan");
                continue;
            }
        };
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with('.') {
            continue;
        }
        let ft = entry.file_type();
        if ft.as_ref().is_ok_and(|t| t.is_dir()) {
            paths.extend(collect_workflow_paths(&entry.path()));
        } else if name_str.ends_with(".nika.yaml") || name_str.ends_with(".nika.yml") {
            paths.push(entry.path());
        }
    }
    paths
}

/// Print a structured startup banner to stderr.
fn print_startup_banner(config: &ServeConfig, auth_mode: &token_store::AuthMode) {
    use nika_engine::core::{provider_to_env_var, ProviderCategory, KNOWN_PROVIDERS};

    let version = env!("CARGO_PKG_VERSION");
    let executor = match config.executor_mode {
        config::ExecutorMode::Subprocess => "subprocess",
        config::ExecutorMode::Embedded => "embedded",
    };
    let workflow_count = count_workflow_files(&config.workflows_dir);
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
    let auth_desc = auth_mode.description();

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
    eprintln!("  \u{251c}\u{2500}\u{2500} Auth         {auth_desc}");
    eprintln!("  \u{251c}\u{2500}\u{2500} Providers    {providers_str}");

    // Scan for scheduled workflows
    let (sched_total, sched_active, sched_paused) = scan_scheduled_workflows(&config.workflows_dir);
    if sched_total > 0 {
        let details = if sched_paused > 0 {
            format!("{sched_active} active, {sched_paused} paused")
        } else {
            format!("{sched_active} active")
        };
        eprintln!(
            "  \u{2514}\u{2500}\u{2500} Scheduled    {sched_total} workflow{} ({details})",
            if sched_total == 1 { "" } else { "s" }
        );
    } else {
        eprintln!("  \u{2514}\u{2500}\u{2500} Scheduled    none");
    }

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
            storage_url: None,
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

        let auth_mode = Arc::new(token_store::AuthMode::Legacy {
            expected_hash: token_store::hash_token(&config.auth_token),
        });

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
            auth_mode: auth_mode.clone(),
        };

        let rl_state = rate_limit::new_rate_limiter();
        let app = routes::build_router(state.clone())
            .layer(middleware::from_fn_with_state(
                auth_mode,
                auth::require_auth,
            ))
            .layer(middleware::from_fn_with_state(
                rl_state,
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
    async fn test_app_with_dir(workflows_dir: std::path::PathBuf) -> (axum::Router, AppState) {
        let storage = nika_storage::Storage::open_memory().expect("open in-memory storage");
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);

        let config = ServeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workflows_dir,
            max_concurrent: 4,
            job_timeout_secs: 60,
            max_output_bytes: 1024,
            db_path: std::path::PathBuf::from(":memory:"),
            storage_url: None,
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

        let auth_mode = Arc::new(token_store::AuthMode::Legacy {
            expected_hash: token_store::hash_token(&config.auth_token),
        });

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
            auth_mode: auth_mode.clone(),
        };

        let rl_state = rate_limit::new_rate_limiter();
        let app = routes::build_router(state.clone())
            .layer(middleware::from_fn_with_state(
                auth_mode,
                auth::require_auth,
            ))
            .layer(middleware::from_fn_with_state(
                rl_state,
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
        std::fs::write(dir.path().join("pipelines/prod/deploy.nika.yaml"), yaml).unwrap();

        let (app, _) = test_app_with_dir(dir.path().to_path_buf()).await;
        // URL-encode the slashes for the path parameter
        let req = Request::get("/v1/workflows/pipelines%2Fprod%2Fdeploy.nika.yaml/source")
            .header("authorization", AUTH)
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(body_string(resp).await, yaml);
    }

    #[test]
    fn count_workflows_recursive() {
        let dir = tempfile::TempDir::new().unwrap();
        // Root-level workflow
        std::fs::write(
            dir.path().join("root.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        // Nested in subdirectory
        std::fs::create_dir_all(dir.path().join("jungo")).unwrap();
        std::fs::write(
            dir.path().join("jungo/api.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        // Deeply nested
        std::fs::create_dir_all(dir.path().join("dev/test")).unwrap();
        std::fs::write(
            dir.path().join("dev/test/mock.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();
        // Non-workflow file — must NOT be counted
        std::fs::write(dir.path().join("readme.md"), "# hello").unwrap();
        // Hidden dir — must NOT be counted
        std::fs::create_dir_all(dir.path().join(".nika")).unwrap();
        std::fs::write(
            dir.path().join(".nika/internal.nika.yaml"),
            "schema: nika/workflow@0.12",
        )
        .unwrap();

        let count = count_workflow_files(dir.path());
        assert_eq!(
            count, 3,
            "should find 3 workflows recursively, skipping hidden dirs"
        );
    }

    #[test]
    fn scan_discovers_scheduled_workflows() {
        let dir = tempfile::TempDir::new().unwrap();

        // Workflow with schedule (active)
        std::fs::write(
            dir.path().join("daily.nika.yaml"),
            "schema: \"nika/workflow@0.12\"\nschedule: \"@daily\"\ntasks:\n  - id: run\n    infer: hello\n",
        ).unwrap();

        // Workflow with schedule (paused)
        std::fs::write(
            dir.path().join("paused.nika.yaml"),
            "schema: \"nika/workflow@0.12\"\nschedule:\n  cron: \"0 9 * * *\"\n  paused: true\ntasks:\n  - id: run\n    infer: hello\n",
        ).unwrap();

        // Workflow without schedule
        std::fs::write(
            dir.path().join("normal.nika.yaml"),
            "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: run\n    infer: hello\n",
        )
        .unwrap();

        let (total, active, paused) = scan_scheduled_workflows(dir.path());
        assert_eq!(total, 2, "should find 2 scheduled workflows");
        assert_eq!(active, 1, "1 active");
        assert_eq!(paused, 1, "1 paused");
    }

    // ── L2 scope enforcement ────────────────────────────────────────────────

    /// Build a test app with MultiKey auth and a token with given scope and role.
    /// Returns (router, state, raw_token, _tempdir_guard).
    /// The tempdir guard keeps the workflows directory alive for the test.
    async fn test_app_multikey(
        scope: &str,
        role: nika_storage::Role,
    ) -> (axum::Router, AppState, String, tempfile::TempDir) {
        let storage = nika_storage::Storage::open_memory().expect("open in-memory storage");
        let (_, shutdown_rx) = tokio::sync::watch::channel(false);
        let tmpdir = tempfile::TempDir::new().unwrap();

        // Create a token in DB
        let raw_token = token_store::generate_token();
        let hash = token_store::hash_token(&raw_token);
        let entry = nika_storage::TokenEntry {
            id: "scoped-1".to_string(),
            name: "scoped-test".to_string(),
            token_hash: hash.to_vec(),
            role,
            scope: scope.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
            last_used_at: None,
            revoked: false,
        };
        storage.insert_token(entry).await.unwrap();

        let config = ServeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            workflows_dir: tmpdir.path().to_path_buf(),
            max_concurrent: 4,
            job_timeout_secs: 60,
            max_output_bytes: 1024,
            db_path: std::path::PathBuf::from(":memory:"),
            storage_url: None,
            auth_token: String::new(),
            cors_origin: None,
            executor_mode: config::ExecutorMode::Embedded,
            rate_per_second: 100,
            rate_burst: 100,
            gc_retention_secs: 7 * 24 * 3600,
            gc_interval_secs: 3600,
            project_root: None,
            working_dir_mode: None,
        };

        let auth_mode = Arc::new(token_store::AuthMode::MultiKey {
            store: token_store::TokenStore::new(storage.clone()),
        });

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
            auth_mode: auth_mode.clone(),
        };

        let rl_state = rate_limit::new_rate_limiter();
        let app = routes::build_router(state.clone())
            .layer(middleware::from_fn_with_state(
                auth_mode,
                auth::require_auth,
            ))
            .layer(middleware::from_fn_with_state(
                rl_state,
                rate_limit::rate_limit_middleware,
            ))
            .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
            .layer(middleware::from_fn(request_id::request_id_middleware));

        (app, state, raw_token, tmpdir)
    }

    #[tokio::test]
    async fn scope_rejects_out_of_scope_workflow() {
        let (app, _, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(r#"{"workflow":"project-b/pipeline.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn scope_allows_in_scope_workflow() {
        let (app, _, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(r#"{"workflow":"project-a/pipeline.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Scope passes → hits next check (workflow not found on disk) → 400
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn scope_wildcard_allows_everything() {
        let (app, _, token, _dir) = test_app_multikey("*", nika_storage::Role::Operator).await;

        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(r#"{"workflow":"any/workflow.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Wildcard scope passes → hits next check (workflow not found) → 400
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── L3 RBAC ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn rbac_viewer_rejected_on_run() {
        let (app, _, token, _dir) = test_app_multikey("*", nika_storage::Role::Viewer).await;

        let req = Request::builder()
            .method("POST")
            .uri("/v1/run")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(r#"{"workflow":"test.nika.yaml"}"#))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rbac_viewer_allowed_on_list_workflows() {
        let (app, _, token, _dir) = test_app_multikey("*", nika_storage::Role::Viewer).await;

        let req = Request::builder()
            .method("GET")
            .uri("/v1/workflows")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Viewer can list — 200 (empty list, but not 403)
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rbac_operator_rejected_on_reload() {
        let (app, _, token, _dir) = test_app_multikey("*", nika_storage::Role::Operator).await;

        let req = Request::builder()
            .method("POST")
            .uri("/v1/reload")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rbac_admin_allowed_on_reload() {
        let (app, _, token, _dir) = test_app_multikey("*", nika_storage::Role::Admin).await;

        let req = Request::builder()
            .method("POST")
            .uri("/v1/reload")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Admin can reload — 200 (empty dir but succeeds)
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── L2 scope enforcement: job-level endpoints ──────────────────────────

    /// Helper: create a job in storage for a given workflow.
    async fn create_test_job(storage: &nika_storage::Storage, workflow: &str) -> String {
        let job_id = uuid::Uuid::new_v4().simple().to_string();
        storage
            .create_job_with_tags(&job_id, workflow, None)
            .await
            .unwrap();
        job_id
    }

    #[tokio::test]
    async fn scope_rejects_status_for_out_of_scope_job() {
        let (app, state, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        let job_id = create_test_job(&state.storage, "project-b/pipeline.nika.yaml").await;

        let req = Request::builder()
            .method("GET")
            .uri(format!("/v1/status/{job_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn scope_allows_status_for_in_scope_job() {
        let (app, state, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        let job_id = create_test_job(&state.storage, "project-a/pipeline.nika.yaml").await;

        let req = Request::builder()
            .method("GET")
            .uri(format!("/v1/status/{job_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn scope_rejects_cancel_for_out_of_scope_job() {
        let (app, state, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        let job_id = create_test_job(&state.storage, "project-b/pipeline.nika.yaml").await;

        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/cancel/{job_id}"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn scope_filters_jobs_list() {
        let (app, state, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        create_test_job(&state.storage, "project-a/pipeline.nika.yaml").await;
        create_test_job(&state.storage, "project-b/other.nika.yaml").await;

        let req = Request::builder()
            .method("GET")
            .uri("/v1/jobs")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let jobs = json["jobs"].as_array().unwrap();
        // Only project-a job should be visible
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0]["workflow"]
            .as_str()
            .unwrap()
            .starts_with("project-a/"));
    }

    #[tokio::test]
    async fn scope_rejects_artifacts_for_out_of_scope_job() {
        let (app, state, token, _dir) =
            test_app_multikey("project-a/*", nika_storage::Role::Operator).await;

        let job_id = create_test_job(&state.storage, "project-b/pipeline.nika.yaml").await;

        let req = Request::builder()
            .method("GET")
            .uri(format!("/v1/jobs/{job_id}/artifacts"))
            .header("authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
