#![cfg(unix)]
//! Daemon client — connects to the daemon over Unix socket.
//!
//! The client is used by `nika run`, `nika doctor`, and the TUI to communicate
//! with the background daemon. It connects to the Unix socket, sends a request,
//! and reads the response.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{ReadHalf, WriteHalf};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::error::{DaemonError, DaemonResult};
use crate::protocol::{decode_message, write_message, DaemonRequest, DaemonResponse};

/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Client for communicating with the nika daemon.
pub struct DaemonClient {
    socket_path: PathBuf,
    timeout: Duration,
}

impl DaemonClient {
    /// Create a new client targeting the given socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Create a client using the default daemon socket path (`~/.nika/daemon/nika.sock`).
    pub fn default_path() -> Self {
        Self::new(crate::daemon_socket_path())
    }

    /// Set the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Check if the daemon socket file exists (does NOT connect).
    pub fn socket_exists(&self) -> bool {
        self.socket_path.exists()
    }

    /// Check if the daemon is available (socket exists and responds to Ping).
    pub async fn is_available(&self) -> bool {
        self.ping().await.is_ok()
    }

    /// Ping the daemon and get version + uptime.
    pub async fn ping(&self) -> DaemonResult<(String, u64)> {
        match self.send(DaemonRequest::Ping).await? {
            DaemonResponse::Pong {
                version,
                uptime_secs,
            } => Ok((version, uptime_secs)),
            DaemonResponse::Error { code, message } => {
                Err(DaemonError::RemoteError { code, message })
            }
            other => Err(DaemonError::Protocol(format!(
                "unexpected response to Ping: {:?}",
                other
            ))),
        }
    }

    /// Get daemon status.
    pub async fn status(&self) -> DaemonResult<DaemonResponse> {
        self.send(DaemonRequest::Status).await
    }

    /// Get a secret for a provider. Requires auth token.
    pub async fn get_secret(&self, provider: &str) -> DaemonResult<Option<String>> {
        let auth_token = read_auth_token().ok();
        match self
            .send(DaemonRequest::GetSecret {
                provider: provider.to_string(),
                auth_token,
            })
            .await?
        {
            DaemonResponse::Secret { value } => Ok(value),
            DaemonResponse::AuthRequired => Err(DaemonError::RemoteError {
                code: "AUTH".into(),
                message: "GetSecret requires auth token".into(),
            }),
            DaemonResponse::Error { code, message } => {
                Err(DaemonError::RemoteError { code, message })
            }
            other => Err(DaemonError::Protocol(format!(
                "unexpected response to GetSecret: {:?}",
                other
            ))),
        }
    }

    /// Check if a secret exists for a provider.
    pub async fn has_secret(&self, provider: &str) -> DaemonResult<bool> {
        match self
            .send(DaemonRequest::HasSecret {
                provider: provider.to_string(),
            })
            .await?
        {
            DaemonResponse::SecretExists { exists } => Ok(exists),
            DaemonResponse::Error { code, message } => {
                Err(DaemonError::RemoteError { code, message })
            }
            other => Err(DaemonError::Protocol(format!(
                "unexpected response to HasSecret: {:?}",
                other
            ))),
        }
    }

    /// Store a secret for a provider via the daemon.
    ///
    /// Requires auth token from `~/.nika/daemon/.token`.
    pub async fn set_secret(&self, provider: &str, key: &str) -> DaemonResult<()> {
        let auth_token = read_auth_token().ok();
        match self
            .send(DaemonRequest::SetSecret {
                provider: provider.to_string(),
                key: key.to_string(),
                auth_token,
            })
            .await?
        {
            DaemonResponse::SecretStored => Ok(()),
            DaemonResponse::AuthRequired => Err(DaemonError::RemoteError {
                code: "AUTH-001".into(),
                message: "auth token required — restart daemon or check ~/.nika/daemon/.token"
                    .into(),
            }),
            DaemonResponse::Error { code, message } => {
                Err(DaemonError::RemoteError { code, message })
            }
            other => Err(DaemonError::Protocol(format!(
                "unexpected response to SetSecret: {:?}",
                other
            ))),
        }
    }

    /// Delete a secret for a provider via the daemon.
    pub async fn delete_secret(&self, provider: &str) -> DaemonResult<()> {
        let auth_token = read_auth_token().ok();
        match self
            .send(DaemonRequest::DeleteSecret {
                provider: provider.to_string(),
                auth_token,
            })
            .await?
        {
            DaemonResponse::SecretDeleted => Ok(()),
            DaemonResponse::AuthRequired => Err(DaemonError::RemoteError {
                code: "AUTH-001".into(),
                message: "auth token required — restart daemon or check ~/.nika/daemon/.token"
                    .into(),
            }),
            DaemonResponse::Error { code, message } => {
                Err(DaemonError::RemoteError { code, message })
            }
            other => Err(DaemonError::Protocol(format!(
                "unexpected response to DeleteSecret: {:?}",
                other
            ))),
        }
    }

    /// Send a raw request and get the response.
    pub async fn send(&self, request: DaemonRequest) -> DaemonResult<DaemonResponse> {
        // No exists() check here — it's a blocking syscall with a TOCTOU race.
        // send_inner maps NotFound/ConnectionRefused to DaemonError::NotRunning.
        let result = timeout(self.timeout, self.send_inner(request)).await;
        match result {
            Ok(inner) => inner,
            Err(_) => Err(DaemonError::Timeout {
                timeout_secs: self.timeout.as_secs(),
            }),
        }
    }

    async fn send_inner(&self, request: DaemonRequest) -> DaemonResult<DaemonResponse> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                DaemonError::NotRunning {
                    path: self.socket_path.clone(),
                }
            } else {
                DaemonError::Connection(e)
            }
        })?;
        let (mut reader, mut writer) = tokio::io::split(stream);

        write_message(&mut writer, &request).await?;
        decode_message(&mut reader).await
    }

    /// Request the daemon to shut down gracefully.
    /// Requires auth token (same as secret write operations).
    pub async fn shutdown(&self) -> DaemonResult<()> {
        let auth_token = read_auth_token().ok();
        match self.send(DaemonRequest::Shutdown { auth_token }).await? {
            DaemonResponse::ShuttingDown => Ok(()),
            DaemonResponse::AuthRequired => Err(DaemonError::RemoteError {
                code: "AUTH-001".into(),
                message: "auth token required — restart daemon or check ~/.nika/daemon/.token"
                    .into(),
            }),
            DaemonResponse::Error { code, message } => {
                Err(DaemonError::RemoteError { code, message })
            }
            other => Err(DaemonError::Protocol(format!(
                "unexpected response to Shutdown: {:?}",
                other
            ))),
        }
    }

    /// Open a persistent connection to the daemon for sending multiple requests.
    ///
    /// The returned [`ConnectedClient`] reuses a single Unix socket connection,
    /// avoiding the overhead of reconnecting for each request. The server supports
    /// pipelining, so multiple requests can be sent sequentially on one connection.
    pub async fn connect(&self) -> DaemonResult<ConnectedClient> {
        if !self.socket_path.exists() {
            return Err(DaemonError::NotRunning {
                path: self.socket_path.clone(),
            });
        }
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (reader, writer) = tokio::io::split(stream);
        Ok(ConnectedClient {
            reader,
            writer,
            timeout: self.timeout,
            poisoned: false,
        })
    }

    /// Get the socket path this client targets.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

/// Read the daemon auth token from `~/.nika/daemon/.token`.
///
/// Returns the token string or an error if the file doesn't exist or is unreadable.
pub fn read_auth_token() -> Result<String, std::io::Error> {
    let path = crate::daemon_token_path();
    let token = std::fs::read_to_string(&path)?;
    Ok(token.trim().to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// CONNECTED CLIENT
// ═══════════════════════════════════════════════════════════════════════════

/// A persistent connection to the daemon that supports sending multiple requests.
///
/// Created via [`DaemonClient::connect`]. Holds a single Unix socket connection
/// open and reuses it for all requests, avoiding per-request connection overhead.
///
/// **Important:** After any error (including timeout), the connection is poisoned
/// and all subsequent requests will return `DaemonError::Connection`. Create a
/// new `ConnectedClient` via `DaemonClient::connect()` to recover.
pub struct ConnectedClient {
    reader: ReadHalf<UnixStream>,
    writer: WriteHalf<UnixStream>,
    timeout: Duration,
    /// Poisoned after any error to prevent response desynchronization.
    poisoned: bool,
}

impl std::fmt::Debug for ConnectedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectedClient")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl ConnectedClient {
    /// Send a request and wait for the response.
    ///
    /// After any error (timeout, I/O, protocol), the connection is poisoned.
    /// Subsequent calls will return `DaemonError::Connection` immediately.
    pub async fn request(&mut self, req: DaemonRequest) -> DaemonResult<DaemonResponse> {
        if self.poisoned {
            return Err(DaemonError::Connection(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "connection poisoned after previous error — reconnect",
            )));
        }
        let result = timeout(self.timeout, self.request_inner(req)).await;
        match result {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(e)) => {
                self.poisoned = true;
                Err(e)
            }
            Err(_) => {
                self.poisoned = true;
                Err(DaemonError::Timeout {
                    timeout_secs: self.timeout.as_secs(),
                })
            }
        }
    }

    /// Check if connection is poisoned (any previous error occurred).
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    async fn request_inner(&mut self, req: DaemonRequest) -> DaemonResult<DaemonResponse> {
        write_message(&mut self.writer, &req).await?;
        decode_message(&mut self.reader).await
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tokio::net::UnixListener;

    /// Helper: start a mock server that responds to one request.
    async fn mock_server(socket_path: &Path, response: DaemonResponse) {
        let listener = UnixListener::bind(socket_path).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = tokio::io::split(stream);

            // Read request (consume it)
            let _req: DaemonRequest = decode_message(&mut reader).await.unwrap();

            // Send response
            write_message(&mut writer, &response).await.unwrap();
        });
    }

    #[test]
    fn client_new_sets_path() {
        let client = DaemonClient::new("/tmp/test.sock");
        assert_eq!(client.socket_path(), Path::new("/tmp/test.sock"));
    }

    #[test]
    fn client_with_timeout() {
        let client = DaemonClient::new("/tmp/test.sock").with_timeout(Duration::from_secs(10));
        assert_eq!(client.timeout, Duration::from_secs(10));
    }

    #[test]
    fn client_socket_exists_false_for_nonexistent() {
        let client = DaemonClient::new("/tmp/nonexistent_nika_daemon_test.sock");
        assert!(!client.socket_exists());
    }

    #[tokio::test]
    async fn client_connect_to_nonexistent_socket_returns_not_running() {
        let client = DaemonClient::new("/tmp/nonexistent_nika_daemon_test.sock");
        let result = client.send(DaemonRequest::Ping).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::NotRunning { .. }
        ));
    }

    #[tokio::test]
    async fn client_is_available_false_when_no_socket() {
        let client = DaemonClient::new("/tmp/nonexistent_nika_daemon_test.sock");
        assert!(!client.is_available().await);
    }

    #[tokio::test]
    async fn client_ping_pong_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::Pong {
            version: "0.46.1".into(),
            uptime_secs: 42,
        };
        mock_server(&sock, response).await;

        // Small delay for server to bind
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let (version, uptime) = client.ping().await.unwrap();
        assert_eq!(version, "0.46.1");
        assert_eq!(uptime, 42);
    }

    #[tokio::test]
    async fn client_get_secret_returns_value() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::Secret {
            value: Some("sk-ant-123".into()),
        };
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let secret = client.get_secret("anthropic").await.unwrap();
        assert_eq!(secret, Some("sk-ant-123".into()));
    }

    #[tokio::test]
    async fn client_get_secret_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::Secret { value: None };
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let secret = client.get_secret("unknown").await.unwrap();
        assert_eq!(secret, None);
    }

    #[tokio::test]
    async fn client_has_secret_true() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::SecretExists { exists: true };
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let exists = client.has_secret("anthropic").await.unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn client_remote_error_propagated() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::Error {
            code: "NIKA-500".into(),
            message: "internal error".into(),
        };
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let result = client.ping().await;
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::RemoteError { .. }
        ));
    }

    #[tokio::test]
    async fn client_timeout_on_slow_server() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        // Server that accepts but never responds
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            // Hold connection open but don't respond
            tokio::time::sleep(Duration::from_secs(30)).await;
            drop(stream);
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock).with_timeout(Duration::from_millis(100));
        let result = client.send(DaemonRequest::Ping).await;
        assert!(matches!(result.unwrap_err(), DaemonError::Timeout { .. }));
    }

    #[tokio::test]
    async fn client_shutdown_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::ShuttingDown;
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        client.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn connected_client_multiple_requests() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        // Mock server that handles multiple requests on one connection (pipelining)
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (mut reader, mut writer) = tokio::io::split(stream);

            // Handle 3 sequential requests
            for _ in 0..3 {
                let _req: DaemonRequest = decode_message(&mut reader).await.unwrap();
                let resp = DaemonResponse::Pong {
                    version: "0.48.0".into(),
                    uptime_secs: 1,
                };
                write_message(&mut writer, &resp).await.unwrap();
            }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let mut conn = client.connect().await.unwrap();

        // Send 3 requests on the same connection
        for _ in 0..3 {
            let resp = conn.request(DaemonRequest::Ping).await.unwrap();
            assert!(matches!(resp, DaemonResponse::Pong { .. }));
        }
    }

    #[tokio::test]
    async fn connected_client_not_running() {
        let client = DaemonClient::new("/tmp/nonexistent_nika_connected_test.sock");
        let result = client.connect().await;
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::NotRunning { .. }
        ));
    }

    #[tokio::test]
    async fn client_set_secret_returns_stored() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::SecretStored;
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        // set_secret reads auth token from file, which won't exist in test,
        // but the mock server ignores the request content
        let result = client.set_secret("anthropic", "sk-test").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn client_delete_secret_returns_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::SecretDeleted;
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let result = client.delete_secret("anthropic").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn client_set_secret_auth_required() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        let response = DaemonResponse::AuthRequired;
        mock_server(&sock, response).await;
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let result = client.set_secret("anthropic", "sk-test").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            DaemonError::RemoteError { .. }
        ));
    }

    #[test]
    #[serial]
    fn read_auth_token_nonexistent_returns_error() {
        // Set NIKA_HOME to a temp dir where no token exists
        let dir = tempfile::tempdir().unwrap();
        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let result = read_auth_token();
        assert!(result.is_err());

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[test]
    #[serial]
    fn read_auth_token_reads_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let daemon_dir = dir.path().join("daemon");
        std::fs::create_dir_all(&daemon_dir).unwrap();
        std::fs::write(daemon_dir.join(".token"), "test-token-abc123\n").unwrap();

        let orig = std::env::var("NIKA_HOME").ok();
        std::env::set_var("NIKA_HOME", dir.path());

        let result = read_auth_token();
        assert_eq!(result.unwrap(), "test-token-abc123");

        match orig {
            Some(v) => std::env::set_var("NIKA_HOME", v),
            None => unsafe { std::env::remove_var("NIKA_HOME") },
        }
    }

    #[tokio::test]
    async fn client_server_closes_connection() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("test.sock");

        // Server that accepts then immediately closes
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            drop(stream); // Close immediately
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = DaemonClient::new(&sock);
        let result = client.send(DaemonRequest::Ping).await;
        assert!(result.is_err());
    }
}
