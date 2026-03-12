//! Daemon Server
//!
//! Unix socket server that handles IPC requests.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;

use crate::core::paths::{daemon_socket_path, ensure_nika_home};
use crate::error::NikaError;
use crate::error::Result;

use super::pid::{acquire_pid_lock, PidLock};
use super::protocol::{DaemonRequest, DaemonResponse};

/// Daemon server that listens on a Unix socket
pub struct DaemonServer {
    /// Start time for uptime calculation
    start_time: Instant,
    /// Request counter
    requests_handled: AtomicU64,
    /// PID lock guard
    _pid_lock: PidLock,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
}

impl DaemonServer {
    /// Create a new daemon server
    ///
    /// This acquires the PID lock and prepares the server for running.
    pub fn new() -> Result<Self> {
        // Ensure ~/.nika/ exists
        ensure_nika_home().map_err(|e| NikaError::DaemonError {
            message: format!("Failed to create nika home directory: {}", e),
        })?;

        // Acquire PID lock (ensures single instance)
        let pid_path = crate::core::paths::daemon_pid_path();
        let pid_lock = acquire_pid_lock(&pid_path)?;

        // Create shutdown channel
        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            start_time: Instant::now(),
            requests_handled: AtomicU64::new(0),
            _pid_lock: pid_lock,
            shutdown_tx,
        })
    }

    /// Run the daemon server
    ///
    /// This blocks until shutdown is requested via SIGTERM or the Shutdown command.
    pub async fn run(self) -> Result<()> {
        let server = Arc::new(self);

        // Remove old socket if it exists
        let socket_path = daemon_socket_path();
        if socket_path.exists() {
            std::fs::remove_file(&socket_path).map_err(|e| NikaError::IoPathError {
                path: socket_path.clone(),
                operation: "remove old socket".to_string(),
                source: e,
            })?;
        }

        // Create listener
        let listener = UnixListener::bind(&socket_path).map_err(|e| NikaError::IoPathError {
            path: socket_path.clone(),
            operation: "bind socket".to_string(),
            source: e,
        })?;

        // Set socket permissions (owner only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
                .map_err(|e| NikaError::IoPathError {
                    path: socket_path.clone(),
                    operation: "set socket permissions".to_string(),
                    source: e,
                })?;
        }

        tracing::info!("Daemon listening on {:?}", socket_path);

        // Set up signal handlers
        let mut sigterm = signal(SignalKind::terminate()).map_err(|e| NikaError::DaemonError {
            message: format!("Failed to set up SIGTERM handler: {}", e),
        })?;

        let mut sighup = signal(SignalKind::hangup()).map_err(|e| NikaError::DaemonError {
            message: format!("Failed to set up SIGHUP handler: {}", e),
        })?;

        // Subscribe to shutdown channel
        let mut shutdown_rx = server.shutdown_tx.subscribe();

        // Accept connections
        loop {
            tokio::select! {
                // New connection
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let server_clone = Arc::clone(&server);
                            tokio::spawn(async move {
                                if let Err(e) = server_clone.handle_connection(stream).await {
                                    tracing::error!("Connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Accept error: {}", e);
                        }
                    }
                }

                // SIGTERM received
                _ = sigterm.recv() => {
                    tracing::info!("Received SIGTERM, shutting down...");
                    break;
                }

                // SIGHUP received (reload config)
                _ = sighup.recv() => {
                    tracing::info!("Received SIGHUP, reloading config...");
                    // TODO: Reload config if needed
                }

                // Shutdown requested via protocol
                _ = shutdown_rx.recv() => {
                    tracing::info!("Shutdown requested, exiting...");
                    break;
                }
            }
        }

        // Cleanup
        if socket_path.exists() {
            let _ = std::fs::remove_file(&socket_path);
        }

        tracing::info!("Daemon stopped");
        Ok(())
    }

    /// Handle a single client connection
    async fn handle_connection(self: &Arc<Self>, mut stream: UnixStream) -> Result<()> {
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();

        // Read request
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| NikaError::DaemonConnectionFailed {
                message: format!("Failed to read request: {}", e),
            })?;

        // Parse request
        let request =
            DaemonRequest::from_json(&line).map_err(|e| NikaError::DaemonProtocolError {
                message: format!("Failed to parse request: {}", e),
            })?;

        // Increment request counter
        self.requests_handled.fetch_add(1, Ordering::Relaxed);

        // Handle request
        let response = self.handle_request(request).await;

        // Send response
        let json = response
            .to_json_line()
            .map_err(|e| NikaError::DaemonProtocolError {
                message: format!("Failed to serialize response: {}", e),
            })?;

        stream
            .write_all(json.as_bytes())
            .await
            .map_err(|e| NikaError::DaemonConnectionFailed {
                message: format!("Failed to send response: {}", e),
            })?;

        Ok(())
    }

    /// Handle a daemon request
    async fn handle_request(&self, request: DaemonRequest) -> DaemonResponse {
        match request {
            DaemonRequest::Ping => DaemonResponse::Pong,

            DaemonRequest::Shutdown => {
                // Signal shutdown
                let _ = self.shutdown_tx.send(());
                DaemonResponse::ShuttingDown
            }

            DaemonRequest::Status => DaemonResponse::StatusInfo {
                version: env!("CARGO_PKG_VERSION").to_string(),
                uptime_secs: self.start_time.elapsed().as_secs(),
                requests_handled: self.requests_handled.load(Ordering::Relaxed),
                pid: std::process::id(),
            },

            DaemonRequest::GetSecret { provider } => {
                match self.get_secret_from_keychain(&provider).await {
                    Ok(Some(value)) => DaemonResponse::Secret { value },
                    Ok(None) => DaemonResponse::not_found(&provider),
                    Err(e) => DaemonResponse::keychain_error(e.to_string()),
                }
            }

            DaemonRequest::SetSecret { provider, value } => {
                match self.set_secret_in_keychain(&provider, &value).await {
                    Ok(()) => DaemonResponse::SecretSet,
                    Err(e) => DaemonResponse::keychain_error(e.to_string()),
                }
            }

            DaemonRequest::DeleteSecret { provider } => {
                match self.delete_secret_from_keychain(&provider).await {
                    Ok(()) => DaemonResponse::SecretDeleted,
                    Err(e) => DaemonResponse::keychain_error(e.to_string()),
                }
            }

            DaemonRequest::ListProviders => match self.list_providers_from_keychain().await {
                Ok(providers) => DaemonResponse::ProviderList { providers },
                Err(e) => DaemonResponse::keychain_error(e.to_string()),
            },
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // KEYCHAIN OPERATIONS (requires native-keychain feature)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Get a secret from the OS keychain
    #[cfg(feature = "native-keychain")]
    async fn get_secret_from_keychain(&self, provider: &str) -> Result<Option<String>> {
        // Use keyring crate for cross-platform keychain access
        let entry = keyring::Entry::new("nika", provider).map_err(|e| NikaError::DaemonError {
            message: format!("Keychain entry creation failed: {}", e),
        })?;

        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(NikaError::DaemonError {
                message: format!("Keychain read failed: {}", e),
            }),
        }
    }

    /// Get a secret - stub when native-keychain is disabled
    #[cfg(not(feature = "native-keychain"))]
    async fn get_secret_from_keychain(&self, _provider: &str) -> Result<Option<String>> {
        Err(NikaError::DaemonError {
            message: "Keychain support not compiled (native-keychain feature disabled)".to_string(),
        })
    }

    /// Set a secret in the OS keychain
    #[cfg(feature = "native-keychain")]
    async fn set_secret_in_keychain(&self, provider: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new("nika", provider).map_err(|e| NikaError::DaemonError {
            message: format!("Keychain entry creation failed: {}", e),
        })?;

        entry
            .set_password(value)
            .map_err(|e| NikaError::DaemonError {
                message: format!("Keychain write failed: {}", e),
            })
    }

    /// Set a secret - stub when native-keychain is disabled
    #[cfg(not(feature = "native-keychain"))]
    async fn set_secret_in_keychain(&self, _provider: &str, _value: &str) -> Result<()> {
        Err(NikaError::DaemonError {
            message: "Keychain support not compiled (native-keychain feature disabled)".to_string(),
        })
    }

    /// Delete a secret from the OS keychain
    #[cfg(feature = "native-keychain")]
    async fn delete_secret_from_keychain(&self, provider: &str) -> Result<()> {
        let entry = keyring::Entry::new("nika", provider).map_err(|e| NikaError::DaemonError {
            message: format!("Keychain entry creation failed: {}", e),
        })?;

        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
            Err(e) => Err(NikaError::DaemonError {
                message: format!("Keychain delete failed: {}", e),
            }),
        }
    }

    /// Delete a secret - stub when native-keychain is disabled
    #[cfg(not(feature = "native-keychain"))]
    async fn delete_secret_from_keychain(&self, _provider: &str) -> Result<()> {
        Err(NikaError::DaemonError {
            message: "Keychain support not compiled (native-keychain feature disabled)".to_string(),
        })
    }

    /// List all providers with stored secrets
    ///
    /// Note: The keyring crate doesn't support listing all entries,
    /// so we check a known list of providers.
    #[cfg(feature = "native-keychain")]
    async fn list_providers_from_keychain(&self) -> Result<Vec<String>> {
        use crate::core::KNOWN_PROVIDERS;

        let mut providers_with_secrets = Vec::new();

        for provider in KNOWN_PROVIDERS.iter() {
            if let Ok(Some(_)) = self.get_secret_from_keychain(provider.id).await {
                providers_with_secrets.push(provider.id.to_string());
            }
        }

        Ok(providers_with_secrets)
    }

    /// List providers - stub when native-keychain is disabled
    #[cfg(not(feature = "native-keychain"))]
    async fn list_providers_from_keychain(&self) -> Result<Vec<String>> {
        Err(NikaError::DaemonError {
            message: "Keychain support not compiled (native-keychain feature disabled)".to_string(),
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_response_status() {
        let response = DaemonResponse::StatusInfo {
            version: "0.27.0".to_string(),
            uptime_secs: 100,
            requests_handled: 50,
            pid: 1234,
        };

        let json = response.to_json_line().unwrap();
        assert!(json.contains("StatusInfo"));
        assert!(json.contains("0.27.0"));
    }

    #[tokio::test]
    async fn test_request_counter() {
        let counter = AtomicU64::new(0);
        assert_eq!(counter.fetch_add(1, Ordering::Relaxed), 0);
        assert_eq!(counter.fetch_add(1, Ordering::Relaxed), 1);
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }
}
