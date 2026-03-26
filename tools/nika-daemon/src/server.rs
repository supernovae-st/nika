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
#[allow(dead_code)]
struct ServerState {
    started_at: Instant,
    secret_service: SecretService,
    job_service: Option<JobService>,
    cache_service: CacheService,
    event_bus: EventBus,
    active_watch: tokio::sync::Mutex<Option<ActiveWatch>>,
    shutdown_tx: watch::Sender<bool>,
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

        // Set socket permissions to owner-only (0o600)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(socket_path, perms).await?;
        }

        info!(path = %socket_path.display(), "daemon listening");

        // Initialize job storage
        let db_path = crate::daemon_dir().join("jobs.db");
        let job_service = match Storage::open(&db_path) {
            Ok(storage) => {
                info!(path = %db_path.display(), "job storage opened");
                Some(JobService::new(storage))
            }
            Err(e) => {
                warn!(error = %e, "failed to open job storage — jobs disabled");
                None
            }
        };

        let event_bus = EventBus::new();
        event_bus.publish(crate::events::DaemonEvent::DaemonStarted {
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        });

        let state = Arc::new(ServerState {
            started_at: Instant::now(),
            secret_service: SecretService::new(),
            job_service,
            cache_service: CacheService::new(),
            event_bus,
            active_watch: tokio::sync::Mutex::new(None),
            shutdown_tx: self.shutdown_tx.clone(),
        });

        let mut shutdown_rx = self.shutdown_rx;
        // H1 fix: enforce max_connections via semaphore
        let conn_semaphore = Arc::new(Semaphore::new(self.config.max_connections));

        loop {
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
                            tokio::spawn(async move {
                                let _permit = match sem.acquire().await {
                                    Ok(p) => p,
                                    Err(_) => return, // Semaphore closed
                                };
                                if let Err(e) = handle_connection(stream, &state).await {
                                    debug!(error = %e, "connection handler error");
                                }
                                // Permit dropped here → slot released
                            });
                        }
                        Err(e) => {
                            warn!(error = %e, "accept failed");
                        }
                    }
                }
            }
        }

        // Cleanup socket
        let _ = tokio::fs::remove_file(socket_path).await;
        info!("daemon stopped");
        Ok(())
    }
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
            let mut services = vec!["secrets".to_string(), "cache".into()];
            if state.job_service.is_some() {
                services.push("jobs".into());
            }
            DaemonResponse::StatusInfo {
                pid: std::process::id(),
                uptime_secs: state.started_at.elapsed().as_secs(),
                services,
            }
        }

        DaemonRequest::GetSecret { provider } => {
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

        // ── Jobs ────────────────────────────────────────────────────────
        DaemonRequest::JobSubmit {
            workflow,
            name,
            args,
            cron,
            max_retries,
        } => match &state.job_service {
            Some(svc) => match svc
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
            None => DaemonResponse::Error {
                code: "JOB-000".into(),
                message: "job service not available".into(),
            },
        },

        DaemonRequest::JobList { state: filter } => match &state.job_service {
            Some(svc) => {
                let job_state = filter.map(|s| JobState::parse(&s));
                match svc.list_jobs(job_state).await {
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
            None => DaemonResponse::Error {
                code: "JOB-000".into(),
                message: "job service not available".into(),
            },
        },

        DaemonRequest::JobStatus { id } => match &state.job_service {
            Some(svc) => match svc.get_job(&id).await {
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
            None => DaemonResponse::Error {
                code: "JOB-000".into(),
                message: "job service not available".into(),
            },
        },

        DaemonRequest::JobCancel { id } => match &state.job_service {
            Some(svc) => match svc.cancel(&id).await {
                Ok(()) => DaemonResponse::Ok,
                Err(e) => DaemonResponse::Error {
                    code: "JOB-003".into(),
                    message: e.to_string(),
                },
            },
            None => DaemonResponse::Error {
                code: "JOB-000".into(),
                message: "job service not available".into(),
            },
        },

        DaemonRequest::JobRetry { id } => match &state.job_service {
            Some(svc) => match svc.retry(&id).await {
                Ok(new_id) => DaemonResponse::JobCreated { id: new_id },
                Err(e) => DaemonResponse::Error {
                    code: "JOB-005".into(),
                    message: e.to_string(),
                },
            },
            None => DaemonResponse::Error {
                code: "JOB-000".into(),
                message: "job service not available".into(),
            },
        },

        DaemonRequest::JobHistory { id } => match &state.job_service {
            Some(svc) => match svc.get_history(&id).await {
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
            None => DaemonResponse::Error {
                code: "JOB-000".into(),
                message: "job service not available".into(),
            },
        },

        // ── Watch ───────────────────────────────────────────────────────
        DaemonRequest::WatchStart { dir, patterns } => {
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

        // ── Lifecycle ────────────────────────────────────────────────────
        DaemonRequest::Shutdown => {
            info!("shutdown requested via IPC");
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
    use std::time::Duration;

    fn test_config(socket_path: PathBuf) -> DaemonConfig {
        DaemonConfig {
            socket_path,
            max_connections: 4,
        }
    }

    #[tokio::test]
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
    async fn server_shutdown_via_ipc() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");
        let config = test_config(sock.clone());

        let server = DaemonServer::new(config);
        let server_handle = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send Shutdown request via client
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
    }

    #[test]
    fn default_config_has_sensible_values() {
        let config = DaemonConfig::default();
        assert!(config.socket_path.ends_with("nika.sock"));
        assert_eq!(config.max_connections, 64);
    }
}
