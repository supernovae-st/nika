//! Jobs Daemon error types.
//!
//! Error codes: NIKA-200 to NIKA-299 (Jobs Daemon range)

use std::path::PathBuf;
use thiserror::Error;

/// Jobs Daemon errors.
#[derive(Debug, Error)]
pub enum JobsError {
    // === Daemon Lifecycle (200-209) ===
    /// Daemon is already running.
    #[error("[NIKA-200] Daemon already running (pid: {pid})")]
    DaemonAlreadyRunning { pid: u32 },

    /// Failed to start daemon.
    #[error("[NIKA-201] Failed to start daemon: {reason}")]
    DaemonStartFailed { reason: String },

    /// Failed to stop daemon.
    #[error("[NIKA-202] Failed to stop daemon: {reason}")]
    DaemonStopFailed { reason: String },

    /// Daemon not running.
    #[error("[NIKA-203] Daemon is not running")]
    DaemonNotRunning,

    /// PID file operation failed.
    #[error("[NIKA-204] PID file error: {reason}")]
    PidFileError { reason: String },

    /// Internal channel closed.
    #[error("[NIKA-205] Internal channel closed")]
    ChannelClosed,

    // === Job Definition (210-219) ===
    /// Job not found.
    #[error("[NIKA-210] Job not found: {name}")]
    JobNotFound { name: String },

    /// Invalid job definition.
    #[error("[NIKA-211] Invalid job definition '{name}': {reason}")]
    InvalidJobDefinition { name: String, reason: String },

    /// Duplicate job name.
    #[error("[NIKA-212] Duplicate job name: {name}")]
    DuplicateJobName { name: String },

    /// Workflow file not found.
    #[error("[NIKA-213] Workflow not found: {path:?}")]
    WorkflowNotFound { path: PathBuf },

    /// Job is already running (exclusive).
    #[error("[NIKA-214] Job '{name}' is already running")]
    JobAlreadyRunning { name: String },

    // === Trigger Errors (220-229) ===
    /// Invalid cron expression.
    #[error("[NIKA-220] Invalid cron expression: {expression}")]
    InvalidCronExpression { expression: String },

    /// Webhook server error.
    #[error("[NIKA-221] Webhook server error: {reason}")]
    WebhookServerError { reason: String },

    /// Watch path error.
    #[error("[NIKA-222] Watch path error: {path:?} - {reason}")]
    WatchPathError { path: PathBuf, reason: String },

    /// Trigger configuration error.
    #[error("[NIKA-223] Trigger configuration error: {reason}")]
    TriggerConfigError { reason: String },

    // === Execution Errors (230-239) ===
    /// Job execution failed.
    #[error("[NIKA-230] Job '{name}' execution failed: {reason}")]
    ExecutionFailed { name: String, reason: String },

    /// Job timed out.
    #[error("[NIKA-231] Job '{name}' timed out after {timeout_secs}s")]
    ExecutionTimeout { name: String, timeout_secs: u64 },

    /// Cannot cancel running job.
    #[error("[NIKA-232] Cannot cancel running execution: {execution_id}")]
    CannotCancelRunning { execution_id: String },

    /// Execution not found.
    #[error("[NIKA-233] Execution not found: {execution_id}")]
    ExecutionNotFound { execution_id: String },

    /// Max retries exceeded.
    #[error("[NIKA-234] Job '{name}' exceeded max retries ({max_attempts})")]
    MaxRetriesExceeded { name: String, max_attempts: u32 },

    // === State Persistence (240-249) ===
    /// SQLite database error.
    #[error("[NIKA-240] Database error: {reason}")]
    DatabaseError { reason: String },

    /// Failed to open database.
    #[error("[NIKA-241] Failed to open database: {path:?}")]
    DatabaseOpenFailed { path: PathBuf },

    /// State serialization error.
    #[error("[NIKA-242] State serialization error: {reason}")]
    SerializationError { reason: String },

    /// State migration error.
    #[error("[NIKA-243] Database migration error: {reason}")]
    MigrationError { reason: String },

    // === Notification Errors (250-259) ===
    /// Notification failed.
    #[error("[NIKA-250] Notification failed: {channel} - {reason}")]
    NotificationFailed { channel: String, reason: String },

    /// Invalid notification configuration.
    #[error("[NIKA-251] Invalid notification config: {reason}")]
    InvalidNotificationConfig { reason: String },

    // === Configuration Errors (260-269) ===
    /// Configuration parse error.
    #[error("[NIKA-260] Configuration parse error: {reason}")]
    ConfigParseError { reason: String },

    /// Configuration file not found.
    #[error("[NIKA-261] Configuration file not found: {path:?}")]
    ConfigNotFound { path: PathBuf },

    /// Invalid configuration value.
    #[error("[NIKA-262] Invalid configuration: {reason}")]
    InvalidConfig { reason: String },

    // === I/O Errors (270-279) ===
    /// I/O error.
    #[error("[NIKA-270] I/O error: {reason}")]
    IoError { reason: String },

    /// File system error.
    #[error("[NIKA-271] File system error: {path:?} - {reason}")]
    FileSystemError { path: PathBuf, reason: String },
}

impl From<std::io::Error> for JobsError {
    fn from(err: std::io::Error) -> Self {
        JobsError::IoError {
            reason: err.to_string(),
        }
    }
}

#[cfg(feature = "jobs")]
impl From<rusqlite::Error> for JobsError {
    fn from(err: rusqlite::Error) -> Self {
        JobsError::DatabaseError {
            reason: err.to_string(),
        }
    }
}

impl From<serde_json::Error> for JobsError {
    fn from(err: serde_json::Error) -> Self {
        JobsError::SerializationError {
            reason: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        let err = JobsError::DaemonAlreadyRunning { pid: 1234 };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-200"));
        assert!(msg.contains("1234"));
    }

    #[test]
    fn test_job_not_found() {
        let err = JobsError::JobNotFound {
            name: "test-job".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-210"));
        assert!(msg.contains("test-job"));
    }

    #[test]
    fn test_execution_failed() {
        let err = JobsError::ExecutionFailed {
            name: "my-job".to_string(),
            reason: "workflow error".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-230"));
        assert!(msg.contains("my-job"));
        assert!(msg.contains("workflow error"));
    }

    #[test]
    fn test_channel_closed() {
        let err = JobsError::ChannelClosed;
        let msg = err.to_string();
        assert!(msg.contains("NIKA-205"));
    }

    #[test]
    fn test_job_already_running() {
        let err = JobsError::JobAlreadyRunning {
            name: "exclusive-job".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NIKA-214"));
        assert!(msg.contains("exclusive-job"));
    }
}
