//! Daemon server — accepts connections on Unix socket and routes requests.
//!
//! The server listens on `~/.nika/daemon/nika.sock`, accepts connections,
//! reads length-prefixed JSON requests, dispatches them to services, and
//! sends back responses.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::net::UnixListener;
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::error::DaemonResult;
use crate::protocol::{decode_message, write_message, DaemonRequest, DaemonResponse};
use crate::services::secrets::SecretService;

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

/// Shared state across all connection handlers.
struct ServerState {
    started_at: Instant,
    secret_service: SecretService,
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
            std::fs::set_permissions(socket_path, perms)?;
        }

        info!(path = %socket_path.display(), "daemon listening");

        let state = Arc::new(ServerState {
            started_at: Instant::now(),
            secret_service: SecretService::new(),
        });

        let mut shutdown_rx = self.shutdown_rx;

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
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, &state).await {
                                    debug!(error = %e, "connection handler error");
                                }
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

/// Handle a single client connection.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: &ServerState,
) -> DaemonResult<()> {
    let (mut reader, mut writer) = tokio::io::split(stream);

    let request: DaemonRequest = decode_message(&mut reader).await?;
    debug!(?request, "received request");

    let response = route_request(request, state).await;
    write_message(&mut writer, &response).await?;

    Ok(())
}

/// Route a request to the appropriate service.
async fn route_request(request: DaemonRequest, state: &ServerState) -> DaemonResponse {
    match request {
        DaemonRequest::Ping => DaemonResponse::Pong {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: state.started_at.elapsed().as_secs(),
        },

        DaemonRequest::Status => DaemonResponse::StatusInfo {
            pid: std::process::id(),
            uptime_secs: state.started_at.elapsed().as_secs(),
            services: vec!["secrets".into()],
        },

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

    #[test]
    fn default_config_has_sensible_values() {
        let config = DaemonConfig::default();
        assert!(config.socket_path.ends_with("nika.sock"));
        assert_eq!(config.max_connections, 64);
    }
}
