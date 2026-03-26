//! Daemon error types.

use std::path::PathBuf;

/// Errors from the nika daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("daemon not running (socket not found: {path})")]
    NotRunning { path: PathBuf },

    #[error("connection failed: {0}")]
    Connection(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("request timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("daemon already running (pid {pid})")]
    AlreadyRunning { pid: u32 },

    #[error("stale PID file: process {pid} not alive")]
    StalePid { pid: u32 },

    #[error("daemon responded with error: [{code}] {message}")]
    RemoteError { code: String, message: String },

    #[error("message too large: {size} bytes (max {max})")]
    MessageTooLarge { size: usize, max: usize },

    #[error("lifecycle error: {0}")]
    Lifecycle(String),
}

/// Result type alias for daemon operations.
pub type DaemonResult<T> = std::result::Result<T, DaemonError>;
