#![cfg(unix)]
//! Daemon server — accepts connections on Unix socket and routes requests.
//!
//! The server listens on `~/.nika/daemon/nika.sock`, accepts connections,
//! reads length-prefixed JSON requests, dispatches them to services, and
//! sends back responses.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UnixListener;
use tokio::sync::{watch, Semaphore};
use tracing::{debug, info, warn};

use crate::error::{DaemonError, DaemonResult};
use crate::events::EventBus;
use crate::protocol::{decode_message, write_message, DaemonRequest, DaemonResponse};
use crate::services::cache::CacheService;
use crate::services::jobs::JobService;
use crate::services::secrets::SecretService;
use crate::services::watch::{WatchConfig, WatchService};
use crate::storage::{JobState, Storage};

/// Maximum response size accepted in CacheSet requests (256 KB).
/// Prevents a single client from allocating large amounts of heap.
const MAX_CACHE_RESPONSE_BYTES: usize = 256 * 1024;

/// Daemon server configuration.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Path to the Unix socket.
    pub socket_path: PathBuf,
    /// Maximum concurrent connections.
    pub max_connections: usize,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            socket_path: crate::daemon_socket_path(),
            max_connections: 64,
        }
    }
}

/// Tracks active watch session (dir + patterns + ability to stop it).
struct ActiveWatch {
    dir: String,
    patterns: Vec<String>,
    /// Send true to stop the watch background task.
    stop_tx: watch::Sender<bool>,
}

/// Shared state across all connection handlers.
struct ServerState {
    started_at: Instant,
    secret_service: SecretService,
    job_service: JobService,
    cache_service: CacheService,
    event_bus: EventBus,
    active_watch: tokio::sync::Mutex<Option<ActiveWatch>>,
    shutdown_tx: watch::Sender<bool>,
    /// Session auth token — required for secret write operations.
    /// Generated at startup, stored in `~/.nika/daemon/.token` (0o600).
    auth_token: String,
}

/// Drop guard that cleans up socket and token files on exit (success or error).
struct SocketGuard {
    socket_path: PathBuf,
    token_path: PathBuf,
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(&self.token_path);
    }
}

/// The daemon server.
pub struct DaemonServer {
    config: DaemonConfig,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

impl DaemonServer {
    /// Create a new server with the given config.
    pub fn new(config: DaemonConfig) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        Self {
            config,
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Create a server with default config.
    pub fn with_defaults() -> Self {
        Self::new(DaemonConfig::default())
    }

    /// Get a handle to trigger shutdown from outside.
    pub fn shutdown_handle(&self) -> watch::Sender<bool> {
        self.shutdown_tx.clone()
    }

    /// Run the server (blocks until shutdown).
    pub async fn run(self) -> DaemonResult<()> {
        let socket_path = &self.config.socket_path;

        // Ensure parent directory exists
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Remove stale socket
        if socket_path.exists() {
            tokio::fs::remove_file(socket_path).await?;
        }

        let listener = UnixListener::bind(socket_path)?;

        // Guard ensures socket + token are cleaned up on any exit path (success, error, panic)
        let _socket_guard = SocketGuard {
            socket_path: socket_path.clone(),
            token_path: crate::daemon_token_path(),
        };

        // Set socket permissions to owner-only (0o600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(socket_path, perms).await?;
        }

        // Persist current exe path for job spawning (VPS-01).
        // After binary upgrade, current_exe() returns the old path. This file
        // is read as fallback by JobService when current_exe() fails.
        if let Ok(exe) = std::env::current_exe() {
            let exe_path_file = crate::daemon_dir().join("nika-exe-path");
            let _ = tokio::fs::write(&exe_path_file, exe.to_string_lossy().as_bytes()).await;
        }

        info!(path = %socket_path.display(), "daemon listening");

        // Initialize job storage — fatal on failure so systemd Restart=always
        // will restart the daemon (SQLite is required for jobs + cron).
        let db_path = crate::daemon_dir().join("jobs.db");
        let storage = Storage::open(&db_path).map_err(|e| {
            DaemonError::Lifecycle(format!(
                "failed to open job storage at {}: {e}",
                db_path.display()
            ))
        })?;
        info!(path = %db_path.display(), "job storage opened");
        let job_service = JobService::new(storage);
        // Spawn cron scheduler — fires due cron jobs every minute.
        tokio::spawn(crate::services::jobs::run_cron_scheduler(
            job_service.clone(),
        ));

        let event_bus = EventBus::new();
        event_bus.publish(crate::events::DaemonEvent::DaemonStarted {
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        });

        // Generate session auth token for secret write operations
        let auth_token = generate_auth_token();
        if let Err(e) = write_auth_token(&auth_token).await {
            warn!(error = %e, "failed to write auth token file — secret writes will be rejected");
        }

        let cache_service = CacheService::new();

        // Periodic cache cleanup (every 5 minutes)
        let cache_for_reaper = cache_service.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
            interval.tick().await; // skip first immediate tick
            loop {
                interval.tick().await;
                let removed = cache_for_reaper.cleanup_expired();
                if removed > 0 {
                    tracing::debug!(removed, "cache reaper cleaned expired entries");
                }
            }
        });

        let state = Arc::new(ServerState {
            started_at: Instant::now(),
            secret_service: SecretService::new(),
            job_service,
            cache_service,
            event_bus,
            active_watch: tokio::sync::Mutex::new(None),
            shutdown_tx: self.shutdown_tx.clone(),
            auth_token,
        });

        // Signal systemd readiness (Linux only, no-op if NOTIFY_SOCKET is unset)
        #[cfg(target_os = "linux")]
        {
            let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);
        }

        let mut shutdown_rx = self.shutdown_rx;
        // H1 fix: enforce max_connections via semaphore
        let conn_semaphore = Arc::new(Semaphore::new(self.config.max_connections));
        let mut connection_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

        loop {
            // Periodically clean up completed handles to prevent memory buildup
            connection_handles.retain(|h| !h.is_finished());

            tokio::select! {
                biased;

                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("shutdown signal received");
                        break;
                    }
                }

                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let state = Arc::clone(&state);
                            let sem = Arc::clone(&conn_semaphore);
                            let handle = tokio::spawn(async move {
                                let _permit = match sem.acquire().await {
                                    Ok(p) => p,
                                    Err(_) => return, // Semaphore closed
                                };
                                if let Err(e) = handle_connection(stream, &state).await {
                                    debug!(error = %e, "connection handler error");
                                }
                                // Permit dropped here → slot released
                            });
                            connection_handles.push(handle);
                        }
                        Err(e) => {
                            warn!(error = %e, "accept failed");
                        }
                    }
                }
            }
        }

        // Drain active connections with a 5s timeout before exiting
        let active = connection_handles
            .iter()
            .filter(|h| !h.is_finished())
            .count();
        if active > 0 {
            info!(count = active, "draining active connections");
            let drain_timeout = std::time::Duration::from_secs(5);
            let _ = tokio::time::timeout(drain_timeout, async {
                for handle in connection_handles {
                    let _ = handle.await;
                }
            })
            .await;
        }

        // SocketGuard handles cleanup on drop (socket + token files)
        info!("daemon stopped");
        Ok(())
    }
}

/// Generate a cryptographically random auth token using CSPRNG.
///
/// Uses two UUID v4 values (backed by `getrandom`) concatenated for 256 bits of entropy.
fn generate_auth_token() -> String {
    // uuid v4 uses getrandom (CSPRNG) — 122 bits of entropy per UUID.
    // Two UUIDs = 244 bits, formatted as 64 hex chars.
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    format!("{}{}", a.as_simple(), b.as_simple())
}

/// Write the auth token to `~/.nika/daemon/.token` with 0o600 permissions.
///
/// Uses atomic file creation with mode 0o600 on Unix to avoid a TOCTOU window
/// where the file is briefly world-readable.
async fn write_auth_token(token: &str) -> std::io::Result<()> {
    let path = crate::daemon_token_path();
    // Ensure daemon dir exists (may differ from socket dir in tests)
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let token = token.to_string();
    let path_clone = path.clone();
    // Use spawn_blocking for OpenOptions (synchronous + sets mode atomically)
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path_clone)?;
            file.write_all(token.as_bytes())?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&path_clone, token.as_bytes())?;
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(std::io::Error::other)??;

    Ok(())
}

/// Validate a client auth token against the server's session token.
/// Read operations don't require auth (token=None is fine for reads).
///
/// Uses blake3 hashing to normalize both tokens to fixed 32 bytes
/// before XOR comparison, preventing length-based timing leaks.
fn validate_auth_token(client_token: &Option<String>, server_token: &str) -> bool {
    match client_token {
        Some(token) => {
            // Hash both to fixed-size output to prevent length-based timing leaks.
            // Without hashing, an early return on length mismatch would let an
            // attacker determine the server token length via timing.
            let client_hash = blake3::hash(token.as_bytes());
            let server_hash = blake3::hash(server_token.as_bytes());
            // Constant-time XOR comparison on fixed 32-byte hashes
            client_hash
                .as_bytes()
                .iter()
                .zip(server_hash.as_bytes())
                .fold(0u8, |acc, (a, b)| acc | (a ^ b))
                == 0
        }
        None => false,
    }
}

/// Check if a provider name is one of the known LLM providers.
fn is_known_provider(provider: &str) -> bool {
    use crate::services::secrets::PROVIDERS;
    PROVIDERS.iter().any(|&(p, _)| p == provider)
}

/// Handle a client connection — loops reading requests until EOF or error.
///
/// Normal requests get a response then the loop continues (pipelining).
/// `EventSubscribe` enters streaming mode and holds the connection open.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: &Arc<ServerState>,
) -> DaemonResult<()> {
    let (mut reader, mut writer) = tokio::io::split(stream);

    loop {
        let request: DaemonRequest = match decode_message(&mut reader).await {
            Ok(req) => req,
            Err(e) => {
                // Distinguish clean EOF from protocol errors for observability
                if !matches!(e, DaemonError::Connection(ref io) if io.kind() == std::io::ErrorKind::UnexpectedEof)
                {
                    debug!(error = %e, "connection decode error (not EOF)");
                }
                break;
            }
        };
        debug!(?request, "received request");

        // EventSubscribe enters streaming mode — holds connection open
        if matches!(request, DaemonRequest::EventSubscribe) {
            let mut rx = state.event_bus.subscribe();
            debug!("event subscriber connected");
            // 5 minute idle timeout — prevents connection slot exhaustion from idle clients
            let idle_timeout = std::time::Duration::from_secs(300);
            loop {
                match tokio::time::timeout(idle_timeout, rx.recv()).await {
                    Err(_) => {
                        debug!("event subscriber idle timeout (5min)");
                        break;
                    }
                    Ok(result) => match result {
                        Ok(event) => {
                            let json = match serde_json::to_value(&event) {
                                Ok(v) => v,
                                Err(e) => {
                                    warn!(error = %e, "failed to serialize daemon event");
                                    continue;
                                }
                            };
                            let resp = DaemonResponse::Event { event: json };
                            if write_message(&mut writer, &resp).await.is_err() {
                                debug!("event subscriber disconnected");
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!(lagged = n, "event subscriber lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            debug!("event bus closed");
                            break;
                        }
                    },
                }
            }
            return Ok(());
        }

        let response = route_request(request, state).await;
        if write_message(&mut writer, &response).await.is_err() {
            break; // client disconnected
        }
    }

    Ok(())
}

/// Route a request to the appropriate service.
async fn route_request(request: DaemonRequest, state: &Arc<ServerState>) -> DaemonResponse {
    match request {
        DaemonRequest::Ping => DaemonResponse::Pong {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: state.started_at.elapsed().as_secs(),
        },

        DaemonRequest::Status => {
            let services = vec!["secrets".to_string(), "cache".into(), "jobs".into()];
            DaemonResponse::StatusInfo {
                pid: std::process::id(),
                uptime_secs: state.started_at.elapsed().as_secs(),
                services,
            }
        }

        // GetSecret requires auth token to prevent same-UID process exfiltration.
        // Unix socket 0o600 + auth token = defense in depth.
        DaemonRequest::GetSecret {
            provider,
            auth_token,
        } => {
            if !validate_auth_token(&auth_token, &state.auth_token) {
                return DaemonResponse::AuthRequired;
            }
            match state.secret_service.get_secret(&provider).await {
                Some(value) => DaemonResponse::Secret { value: Some(value) },
                None => DaemonResponse::Secret { value: None },
            }
        }

        DaemonRequest::HasSecret { provider } => {
            let exists = state.secret_service.has_secret(&provider).await;
            DaemonResponse::SecretExists { exists }
        }

        DaemonRequest::ListSecrets => {
            let providers = state.secret_service.list_secrets().await;
            DaemonResponse::SecretList { providers }
        }

        DaemonRequest::SetSecret {
            provider,
            key,
            auth_token,
        } => {
            // Require valid auth token for write operations
            if !validate_auth_token(&auth_token, &state.auth_token) {
                return DaemonResponse::AuthRequired;
            }
            // Validate provider name to prevent arbitrary vault entries
            if !is_known_provider(&provider) {
                return DaemonResponse::Error {
                    code: "SECRET-004".into(),
                    message: format!("unknown provider: {provider}"),
                };
            }
            match state.secret_service.set_secret(&provider, &key).await {
                Ok(true) => DaemonResponse::SecretStored,
                Ok(false) => DaemonResponse::Error {
                    code: "SECRET-001".into(),
                    message: "vault not available".into(),
                },
                Err(e) => DaemonResponse::Error {
                    code: "SECRET-002".into(),
                    message: e,
                },
            }
        }

        DaemonRequest::DeleteSecret {
            provider,
            auth_token,
        } => {
            if !validate_auth_token(&auth_token, &state.auth_token) {
                return DaemonResponse::AuthRequired;
            }
            if !is_known_provider(&provider) {
                return DaemonResponse::Error {
                    code: "SECRET-004".into(),
                    message: format!("unknown provider: {provider}"),
                };
            }
            match state.secret_service.delete_secret(&provider).await {
                Ok(_) => DaemonResponse::SecretDeleted,
                Err(e) => DaemonResponse::Error {
                    code: "SECRET-003".into(),
                    message: e,
                },
            }
        }

        // ── Jobs ────────────────────────────────────────────────────────
        DaemonRequest::JobSubmit {
            workflow,
            name,
            args,
            cron,
            max_retries,
        } => match state
            .job_service
            .submit(
                &workflow,
                name.as_deref(),
                args.as_deref(),
                cron.as_deref(),
                max_retries.unwrap_or(0),
            )
            .await
        {
            Ok(id) => DaemonResponse::JobCreated { id },
            Err(e) => DaemonResponse::Error {
                code: "JOB-001".into(),
                message: e.to_string(),
            },
        },

        DaemonRequest::JobList { state: filter } => {
            let job_state = filter.map(|s| JobState::parse(&s));
            match state.job_service.list_jobs(job_state).await {
                Ok(jobs) => {
                    let json_jobs: Vec<serde_json::Value> = jobs
                        .iter()
                        .filter_map(|j| match serde_json::to_value(j) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                warn!(error = %e, "failed to serialize job");
                                None
                            }
                        })
                        .collect();
                    DaemonResponse::JobList { jobs: json_jobs }
                }
                Err(e) => DaemonResponse::Error {
                    code: "JOB-002".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::JobStatus { id } => match state.job_service.get_job(&id).await {
            Ok(Some(job)) => match serde_json::to_value(&job) {
                Ok(v) => DaemonResponse::JobDetail { job: v },
                Err(e) => {
                    warn!(error = %e, id, "failed to serialize job");
                    DaemonResponse::Error {
                        code: "JOB-002".into(),
                        message: format!("failed to serialize job: {e}"),
                    }
                }
            },
            Ok(None) => DaemonResponse::Error {
                code: "JOB-004".into(),
                message: format!("job not found: {id}"),
            },
            Err(e) => DaemonResponse::Error {
                code: "JOB-002".into(),
                message: e.to_string(),
            },
        },

        DaemonRequest::JobCancel { id } => match state.job_service.cancel(&id).await {
            Ok(()) => DaemonResponse::Ok,
            Err(e) => DaemonResponse::Error {
                code: "JOB-003".into(),
                message: e.to_string(),
            },
        },

        DaemonRequest::JobRetry { id } => match state.job_service.retry(&id).await {
            Ok(new_id) => DaemonResponse::JobCreated { id: new_id },
            Err(e) => DaemonResponse::Error {
                code: "JOB-005".into(),
                message: e.to_string(),
            },
        },

        DaemonRequest::JobHistory { id } => match state.job_service.get_history(&id).await {
            Ok(events) => {
                let json_events: Vec<serde_json::Value> = events
                    .iter()
                    .filter_map(|e| match serde_json::to_value(e) {
                        Ok(v) => Some(v),
                        Err(err) => {
                            warn!(error = %err, "failed to serialize job history event");
                            None
                        }
                    })
                    .collect();
                DaemonResponse::JobHistoryList {
                    events: json_events,
                }
            }
            Err(e) => DaemonResponse::Error {
                code: "JOB-006".into(),
                message: e.to_string(),
            },
        },

        // ── Watch ───────────────────────────────────────────────────────
        DaemonRequest::WatchStart { dir, patterns } => {
            // Validate: reject paths with ".." components and absolute paths
            // outside the user's home directory (prevents watching sensitive dirs).
            let watch_path = std::path::Path::new(&dir);
            if watch_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
            {
                return DaemonResponse::Error {
                    code: "WATCH-003".into(),
                    message: "watch dir must not contain '..' path components".into(),
                };
            }
            if watch_path.is_absolute() {
                // Use ~/.nika parent as a proxy for the home directory
                let home = crate::nika_home()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default();
                if !watch_path.starts_with(&home) {
                    return DaemonResponse::Error {
                        code: "WATCH-003".into(),
                        message: format!("watch dir '{}' must be under home directory", dir),
                    };
                }
            }

            let mut guard = state.active_watch.lock().await;
            if guard.is_some() {
                return DaemonResponse::Error {
                    code: "WATCH-001".into(),
                    message: "watch already active — stop first".into(),
                };
            }
            let (stop_tx, stop_rx) = watch::channel(false);
            let config = WatchConfig {
                dir: std::path::PathBuf::from(&dir),
                patterns: patterns.clone(),
                ..WatchConfig::default()
            };
            match WatchService::start(config, stop_rx) {
                Ok(mut svc) => {
                    let state_ref = Arc::clone(state);
                    let dir_clone = dir.clone();
                    // Spawn background task to forward watch events to EventBus
                    tokio::spawn(async move {
                        while let Some(event) = svc.next_event().await {
                            state_ref.event_bus.publish(
                                crate::events::DaemonEvent::WatchTriggered {
                                    path: event.path.display().to_string(),
                                },
                            );
                        }
                        debug!(dir = %dir_clone, "watch event loop ended");
                    });
                    *guard = Some(ActiveWatch {
                        dir: dir.clone(),
                        patterns: patterns.clone(),
                        stop_tx,
                    });
                    DaemonResponse::WatchActive { dir, patterns }
                }
                Err(e) => DaemonResponse::Error {
                    code: "WATCH-002".into(),
                    message: e.to_string(),
                },
            }
        }

        DaemonRequest::WatchStop => {
            let mut guard = state.active_watch.lock().await;
            if let Some(active) = guard.take() {
                let _ = active.stop_tx.send(true);
            }
            DaemonResponse::Ok
        }

        DaemonRequest::WatchStatus => {
            let guard = state.active_watch.lock().await;
            match &*guard {
                Some(active) => DaemonResponse::WatchActive {
                    dir: active.dir.clone(),
                    patterns: active.patterns.clone(),
                },
                None => DaemonResponse::WatchInactive,
            }
        }

        // ── Cache ───────────────────────────────────────────────────────
        DaemonRequest::CacheGet { key } => match state.cache_service.get(&key) {
            Some(entry) => DaemonResponse::CacheHit {
                response: entry.response,
            },
            None => DaemonResponse::CacheMiss,
        },

        DaemonRequest::CacheSet {
            key,
            provider,
            model,
            response,
            tokens_in,
            tokens_out,
            cost,
            ttl_secs,
        } => {
            if response.len() > MAX_CACHE_RESPONSE_BYTES {
                return DaemonResponse::Error {
                    code: "CACHE-001".into(),
                    message: format!(
                        "response too large: {} bytes > {} byte limit",
                        response.len(),
                        MAX_CACHE_RESPONSE_BYTES
                    ),
                };
            }
            use crate::services::cache::CacheSetParams;
            state.cache_service.set(CacheSetParams {
                key,
                provider,
                model,
                response,
                tokens_in,
                tokens_out,
                cost,
                ttl: ttl_secs.map(std::time::Duration::from_secs),
            });
            DaemonResponse::Ok
        }

        DaemonRequest::CacheClear => {
            state.cache_service.clear();
            DaemonResponse::Ok
        }

        DaemonRequest::CacheStats => {
            let stats = state.cache_service.stats();
            DaemonResponse::CacheStatsResult {
                entries: stats.entries,
                hits: stats.hits,
                misses: stats.misses,
                evictions: stats.evictions,
                total_tokens_saved: stats.total_tokens_saved,
                total_cost_saved: stats.total_cost_saved,
            }
        }

        // ── Events ──────────────────────────────────────────────────────
        DaemonRequest::EventSubscribe => {
            // Handled in handle_connection (streaming mode) — should not reach here
            DaemonResponse::Ok
        }

        // ── LSP Queries ────────────────────────────────────────────────
        DaemonRequest::ListProviderStatus => {
            use nika_core::catalogs::{providers::KNOWN_PROVIDERS, KeySource, ProviderStatusInfo};
            let mut providers = Vec::new();
            for p in KNOWN_PROVIDERS {
                let has_key = state.secret_service.has_secret(p.id).await;
                let source = if has_key {
                    // Check env first, then vault
                    if std::env::var(p.env_var)
                        .map(|v| !v.is_empty())
                        .unwrap_or(false)
                    {
                        KeySource::Env
                    } else {
                        KeySource::Vault
                    }
                } else {
                    KeySource::NotFound
                };
                providers.push(ProviderStatusInfo {
                    id: p.id.to_string(),
                    name: p.name.to_string(),
                    has_key,
                    source,
                    category: p.category,
                    env_var: p.env_var.to_string(),
                });
            }
            DaemonResponse::ProviderStatusList { providers }
        }

        DaemonRequest::EstimateCost {
            provider: _, // Redundant — cost catalog matches by model pattern. Reserved for custom endpoint routing.
            model,
            input_tokens,
            output_tokens,
        } => match nika_core::catalogs::estimate_cost(&model, input_tokens, output_tokens) {
            Some(estimate) => DaemonResponse::CostEstimateResult { estimate },
            None => DaemonResponse::Error {
                code: "COST-001".into(),
                message: format!("Unknown model for cost estimation: {}", model),
            },
        },

        DaemonRequest::GetWorkflowHistory { workflow } => {
            match state.job_service.list_jobs_for_workflow(&workflow).await {
                Ok(jobs) => {
                    let runs = jobs
                        .into_iter()
                        .map(|j| nika_core::catalogs::WorkflowRunInfo {
                            job_id: j.id,
                            state: j.state.as_str().to_string(),
                            workflow: j.workflow,
                            created_at: j.created_at,
                            started_at: j.started_at,
                            completed_at: j.completed_at,
                            exit_code: j.exit_code,
                        })
                        .collect();
                    DaemonResponse::WorkflowHistoryResult { runs }
                }
                Err(e) => DaemonResponse::Error {
                    code: "JOB-007".into(),
                    message: format!("Failed to query workflow history: {}", e),
                },
            }
        }

        DaemonRequest::GetDaemonCapabilities => {
            let uptime_secs = state.started_at.elapsed().as_secs();
            let cache_stats = state.cache_service.stats();
            let total_requests = cache_stats.hits + cache_stats.misses;
            let cache_hit_rate = if total_requests > 0 {
                cache_stats.hits as f64 / total_requests as f64
            } else {
                0.0
            };
            let active_jobs = state.job_service.running_count().await;
            let watch_active = state.active_watch.lock().await.is_some();
            DaemonResponse::DaemonCapabilitiesResult {
                capabilities: nika_core::catalogs::DaemonCapabilities {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    uptime_secs,
                    cache_entries: cache_stats.entries,
                    cache_hit_rate,
                    active_jobs,
                    watch_active,
                    total_cost_saved: cache_stats.total_cost_saved,
                },
            }
        }

        // ── Lifecycle ────────────────────────────────────────────────────
        DaemonRequest::Shutdown { auth_token } => {
            if !validate_auth_token(&auth_token, &state.auth_token) {
                return DaemonResponse::AuthRequired;
            }
            info!("shutdown requested via IPC (authenticated)");
            let _ = state.shutdown_tx.send(true);
            DaemonResponse::ShuttingDown
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::DaemonClient;
    use serial_test::serial;
    use std::time::Duration;

    fn test_config(socket_path: PathBuf) -> DaemonConfig {
        DaemonConfig {
            socket_path,
            max_connections: 4,
        }
    }

    #[tokio::test]
    #[serial]
    async fn server_starts_and_accepts_ping() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();

        let server_handle = tokio::spawn(server.run());

        // Wait for server to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Client connects and pings
        let client = DaemonClient::new(&sock);
        let (version, _uptime) = client.ping().await.unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));

        // Shutdown
        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    #[serial]
    async fn server_responds_to_status() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = DaemonClient::new(&sock);
        let resp = client.status().await.unwrap();
        match resp {
            DaemonResponse::StatusInfo { pid, services, .. } => {
                assert!(pid > 0);
                assert!(services.contains(&"secrets".to_string()));
            }
            other => panic!("unexpected response: {:?}", other),
        }

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    #[serial]
    async fn server_handles_concurrent_connections() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send 5 concurrent pings
        let mut handles = Vec::new();
        for _ in 0..5 {
            let sock = sock.clone();
            handles.push(tokio::spawn(async move {
                let client = DaemonClient::new(&sock);
                client.ping().await
            }));
        }

        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    #[serial]
    async fn server_graceful_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify socket exists
        assert!(sock.exists());

        // Signal shutdown
        shutdown.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), server_handle)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_ok());

        // Socket should be cleaned up
        assert!(!sock.exists());
    }

    #[tokio::test]
    #[serial]
    async fn server_removes_stale_socket_on_start() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        // Create a stale socket file
        std::fs::write(&sock, "stale").unwrap();
        assert!(sock.exists());

        let config = test_config(sock.clone());
        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Server should have replaced the stale socket
        let client = DaemonClient::new(&sock);
        let result = client.ping().await;
        assert!(result.is_ok());

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    #[serial]
    async fn server_socket_permissions_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check socket permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(&sock).unwrap();
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "socket should be owner-only (0o600)");
        }

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    #[serial]
    async fn server_pipelining_multiple_requests_one_connection() {
        use crate::protocol::{decode_message, write_message};
        use tokio::net::UnixStream;

        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Open a single connection and send multiple requests
        let stream = UnixStream::connect(&sock).await.unwrap();
        let (mut reader, mut writer) = tokio::io::split(stream);

        // Request 1: Ping
        write_message(&mut writer, &DaemonRequest::Ping)
            .await
            .unwrap();
        let resp: DaemonResponse = decode_message(&mut reader).await.unwrap();
        assert!(matches!(resp, DaemonResponse::Pong { .. }));

        // Request 2: Status
        write_message(&mut writer, &DaemonRequest::Status)
            .await
            .unwrap();
        let resp: DaemonResponse = decode_message(&mut reader).await.unwrap();
        assert!(matches!(resp, DaemonResponse::StatusInfo { .. }));

        // Request 3: Ping again
        write_message(&mut writer, &DaemonRequest::Ping)
            .await
            .unwrap();
        let resp: DaemonResponse = decode_message(&mut reader).await.unwrap();
        assert!(matches!(resp, DaemonResponse::Pong { .. }));

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    #[serial]
    async fn server_shutdown_via_ipc() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let config = test_config(sock.clone());
        let server = DaemonServer::new(config);
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send Shutdown request via client (reads auth token from NIKA_HOME)
        let client = DaemonClient::new(&sock);
        client.shutdown().await.unwrap();

        // Server should stop within 2 seconds
        let result = tokio::time::timeout(Duration::from_secs(2), server_handle)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_ok());

        // Socket should be cleaned up
        assert!(!sock.exists());

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn server_shutdown_requires_auth() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let config = test_config(sock.clone());
        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Shutdown without auth token -> AuthRequired
        let client = DaemonClient::new(&sock);
        let resp = client
            .send(DaemonRequest::Shutdown { auth_token: None })
            .await
            .unwrap();
        assert!(
            matches!(resp, DaemonResponse::AuthRequired),
            "Shutdown without auth must return AuthRequired"
        );

        // Shutdown with wrong token -> AuthRequired
        let resp = client
            .send(DaemonRequest::Shutdown {
                auth_token: Some("wrong-token".into()),
            })
            .await
            .unwrap();
        assert!(
            matches!(resp, DaemonResponse::AuthRequired),
            "Shutdown with wrong token must return AuthRequired"
        );

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[test]
    fn default_config_has_sensible_values() {
        let config = DaemonConfig::default();
        assert!(config.socket_path.ends_with("nika.sock"));
        assert_eq!(config.max_connections, 64);
    }

    // ── Auth token tests ─────────────────────────────────────────────

    #[test]
    fn generate_auth_token_is_64_hex_chars() {
        let token = generate_auth_token();
        assert_eq!(token.len(), 64, "auth token must be 64 hex chars");
        assert!(
            token.chars().all(|c| c.is_ascii_hexdigit()),
            "auth token must be hex"
        );
    }

    #[test]
    fn generate_auth_token_is_unique() {
        let t1 = generate_auth_token();
        let t2 = generate_auth_token();
        assert_ne!(t1, t2, "two tokens must differ");
    }

    #[test]
    fn validate_auth_token_valid() {
        let server = "abc123def456";
        assert!(validate_auth_token(&Some(server.into()), server));
    }

    #[test]
    fn validate_auth_token_invalid() {
        assert!(!validate_auth_token(&Some("wrong".into()), "correct"));
    }

    #[test]
    fn validate_auth_token_none_rejected() {
        assert!(!validate_auth_token(&None, "any-token"));
    }

    #[test]
    fn validate_auth_token_length_mismatch_rejected() {
        assert!(!validate_auth_token(
            &Some("short".into()),
            "much-longer-token"
        ));
    }

    #[test]
    fn validate_auth_token_empty_vs_nonempty() {
        assert!(!validate_auth_token(&Some(String::new()), "nonempty"));
        assert!(!validate_auth_token(&Some("nonempty".into()), ""));
    }

    #[test]
    fn validate_auth_token_same_prefix_different_suffix() {
        assert!(!validate_auth_token(
            &Some("abc123xxxxx".into()),
            "abc123yyyyy"
        ));
    }

    #[tokio::test]
    #[serial]
    async fn write_auth_token_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        // Create daemon dir
        tokio::fs::create_dir_all(crate::daemon_dir())
            .await
            .unwrap();

        let token = "test-token-12345";
        write_auth_token(token).await.unwrap();

        let path = crate::daemon_token_path();
        assert!(path.exists());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, token);

        // Check permissions (unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = tokio::fs::metadata(&path).await.unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "token file must be owner-only");
        }

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn server_set_secret_requires_auth() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let config = test_config(sock.clone());
        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // SetSecret without auth token -> AuthRequired
        let client = DaemonClient::new(&sock);
        let resp = client
            .send(DaemonRequest::SetSecret {
                provider: "anthropic".into(),
                key: "sk-test".into(),
                auth_token: None,
            })
            .await
            .unwrap();
        assert!(
            matches!(resp, DaemonResponse::AuthRequired),
            "SetSecret without auth must return AuthRequired"
        );

        // SetSecret with wrong token -> AuthRequired
        let resp = client
            .send(DaemonRequest::SetSecret {
                provider: "anthropic".into(),
                key: "sk-test".into(),
                auth_token: Some("wrong-token".into()),
            })
            .await
            .unwrap();
        assert!(
            matches!(resp, DaemonResponse::AuthRequired),
            "SetSecret with wrong token must return AuthRequired"
        );

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn server_delete_secret_requires_auth() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let config = test_config(sock.clone());
        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        let client = DaemonClient::new(&sock);
        let resp = client
            .send(DaemonRequest::DeleteSecret {
                provider: "anthropic".into(),
                auth_token: None,
            })
            .await
            .unwrap();
        assert!(matches!(resp, DaemonResponse::AuthRequired));

        shutdown.send(true).unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[tokio::test]
    #[serial]
    async fn server_cleanup_removes_token_file() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let config = test_config(sock.clone());
        let server = DaemonServer::new(config);
        let shutdown = server.shutdown_handle();
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Token file should exist while server is running
        let token_path = crate::daemon_token_path();
        assert!(token_path.exists(), "token file must exist while running");

        // Shutdown
        shutdown.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), server_handle)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_ok());

        // Token file should be cleaned up
        assert!(
            !token_path.exists(),
            "token file must be removed on shutdown"
        );

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }
}
