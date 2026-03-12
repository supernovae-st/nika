//! Daemon Client
//!
//! Connect to the nika daemon via Unix socket.

use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::core::paths::daemon_socket_path;
use crate::error::NikaError;
use crate::error::Result;

use super::protocol::{DaemonRequest, DaemonResponse};

/// Client for communicating with the nika daemon
#[derive(Debug)]
pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    /// Connect to the daemon at the default socket path
    pub async fn connect() -> Result<Self> {
        Self::connect_to(&daemon_socket_path()).await
    }

    /// Connect to the daemon at a specific socket path
    pub async fn connect_to(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                NikaError::DaemonNotRunning
            } else {
                NikaError::DaemonConnectionFailed {
                    message: e.to_string(),
                }
            }
        })?;

        Ok(Self { stream })
    }

    /// Send a request and receive a response
    pub async fn send(&mut self, request: &DaemonRequest) -> Result<DaemonResponse> {
        // Serialize request
        let json = request
            .to_json_line()
            .map_err(|e| NikaError::DaemonProtocolError {
                message: format!("Failed to serialize request: {}", e),
            })?;

        // Send request
        self.stream.write_all(json.as_bytes()).await.map_err(|e| {
            NikaError::DaemonConnectionFailed {
                message: format!("Failed to send request: {}", e),
            }
        })?;

        // Read response
        let mut reader = BufReader::new(&mut self.stream);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| NikaError::DaemonConnectionFailed {
                message: format!("Failed to read response: {}", e),
            })?;

        // Deserialize response
        DaemonResponse::from_json(&line).map_err(|e| NikaError::DaemonProtocolError {
            message: format!("Failed to parse response: {}", e),
        })
    }

    /// Ping the daemon to check if it's alive
    pub async fn ping(&mut self) -> Result<()> {
        let response = self.send(&DaemonRequest::Ping).await?;
        match response {
            DaemonResponse::Pong => Ok(()),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to Ping".to_string(),
            }),
        }
    }

    /// Get a secret from the daemon
    pub async fn get_secret(&mut self, provider: &str) -> Result<Option<String>> {
        let response = self
            .send(&DaemonRequest::GetSecret {
                provider: provider.to_string(),
            })
            .await?;

        match response {
            DaemonResponse::Secret { value } => Ok(Some(value)),
            DaemonResponse::Error { code, .. } if code == "NOT_FOUND" => Ok(None),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to GetSecret".to_string(),
            }),
        }
    }

    /// Set a secret in the daemon
    pub async fn set_secret(&mut self, provider: &str, value: &str) -> Result<()> {
        let response = self
            .send(&DaemonRequest::SetSecret {
                provider: provider.to_string(),
                value: value.to_string(),
            })
            .await?;

        match response {
            DaemonResponse::SecretSet => Ok(()),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to SetSecret".to_string(),
            }),
        }
    }

    /// Delete a secret from the daemon
    pub async fn delete_secret(&mut self, provider: &str) -> Result<()> {
        let response = self
            .send(&DaemonRequest::DeleteSecret {
                provider: provider.to_string(),
            })
            .await?;

        match response {
            DaemonResponse::SecretDeleted => Ok(()),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to DeleteSecret".to_string(),
            }),
        }
    }

    /// List all providers with stored secrets
    pub async fn list_providers(&mut self) -> Result<Vec<String>> {
        let response = self.send(&DaemonRequest::ListProviders).await?;

        match response {
            DaemonResponse::ProviderList { providers } => Ok(providers),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to ListProviders".to_string(),
            }),
        }
    }

    /// Get daemon status
    pub async fn status(&mut self) -> Result<DaemonStatus> {
        let response = self.send(&DaemonRequest::Status).await?;

        match response {
            DaemonResponse::StatusInfo {
                version,
                uptime_secs,
                requests_handled,
                pid,
            } => Ok(DaemonStatus {
                version,
                uptime_secs,
                requests_handled,
                pid,
            }),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to Status".to_string(),
            }),
        }
    }

    /// Request daemon shutdown
    pub async fn shutdown(&mut self) -> Result<()> {
        let response = self.send(&DaemonRequest::Shutdown).await?;

        match response {
            DaemonResponse::ShuttingDown => Ok(()),
            DaemonResponse::Error { code, message } => Err(NikaError::DaemonError {
                message: format!("{}: {}", code, message),
            }),
            _ => Err(NikaError::DaemonProtocolError {
                message: "Unexpected response to Shutdown".to_string(),
            }),
        }
    }
}

/// Daemon status information
#[derive(Debug, Clone)]
pub struct DaemonStatus {
    /// Daemon version
    pub version: String,
    /// Uptime in seconds
    pub uptime_secs: u64,
    /// Number of requests handled
    pub requests_handled: u64,
    /// PID of daemon process
    pub pid: u32,
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_no_daemon() {
        // Connecting to a non-existent socket should fail with DaemonNotRunning
        let temp = tempfile::tempdir().unwrap();
        let socket_path = temp.path().join("nonexistent.sock");

        let result = DaemonClient::connect_to(&socket_path).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            NikaError::DaemonNotRunning => {}
            e => panic!("Expected DaemonNotRunning, got: {:?}", e),
        }
    }

    #[test]
    fn test_daemon_status_fields() {
        let status = DaemonStatus {
            version: "0.27.0".to_string(),
            uptime_secs: 3600,
            requests_handled: 42,
            pid: 12345,
        };

        assert_eq!(status.version, "0.27.0");
        assert_eq!(status.uptime_secs, 3600);
        assert_eq!(status.requests_handled, 42);
        assert_eq!(status.pid, 12345);
    }
}
