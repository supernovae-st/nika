//! Daemon client — connects to the daemon over Unix socket.
//!
//! The client is used by `nika run`, `nika doctor`, and the TUI to communicate
//! with the background daemon. It connects to the Unix socket, sends a request,
//! and reads the response.

use std::path::{Path, PathBuf};
use std::time::Duration;

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

    /// Get a secret for a provider.
    pub async fn get_secret(&self, provider: &str) -> DaemonResult<Option<String>> {
        match self
            .send(DaemonRequest::GetSecret {
                provider: provider.to_string(),
            })
            .await?
        {
            DaemonResponse::Secret { value } => Ok(value),
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

    /// Send a raw request and get the response.
    pub async fn send(&self, request: DaemonRequest) -> DaemonResult<DaemonResponse> {
        if !self.socket_path.exists() {
            return Err(DaemonError::NotRunning {
                path: self.socket_path.clone(),
            });
        }

        let result = timeout(self.timeout, self.send_inner(request)).await;
        match result {
            Ok(inner) => inner,
            Err(_) => Err(DaemonError::Timeout {
                timeout_secs: self.timeout.as_secs(),
            }),
        }
    }

    async fn send_inner(&self, request: DaemonRequest) -> DaemonResult<DaemonResponse> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        let (mut reader, mut writer) = tokio::io::split(stream);

        write_message(&mut writer, &request).await?;
        decode_message(&mut reader).await
    }

    /// Get the socket path this client targets.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(matches!(result.unwrap_err(), DaemonError::NotRunning { .. }));
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
