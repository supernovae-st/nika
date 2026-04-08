// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

    #[error("watch error: {0}")]
    Watch(String),
}

/// Result type alias for daemon operations.
pub type DaemonResult<T> = std::result::Result<T, DaemonError>;

impl From<nika_storage::StorageError> for DaemonError {
    fn from(e: nika_storage::StorageError) -> Self {
        DaemonError::Lifecycle(e.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_error_display() {
        let err = DaemonError::Watch("directory not found".into());
        assert_eq!(err.to_string(), "watch error: directory not found");
    }

    #[test]
    fn all_variants_are_debug() {
        let errors: Vec<DaemonError> = vec![
            DaemonError::NotRunning {
                path: PathBuf::from("/tmp/test.sock"),
            },
            DaemonError::Protocol("bad frame".into()),
            DaemonError::Timeout { timeout_secs: 5 },
            DaemonError::AlreadyRunning { pid: 1234 },
            DaemonError::StalePid { pid: 5678 },
            DaemonError::RemoteError {
                code: "E001".into(),
                message: "fail".into(),
            },
            DaemonError::MessageTooLarge { size: 100, max: 50 },
            DaemonError::Lifecycle("shutdown".into()),
            DaemonError::Watch("notify error".into()),
        ];
        for err in &errors {
            // Ensure Display and Debug work
            let _ = format!("{err}");
            let _ = format!("{err:?}");
        }
    }
}
