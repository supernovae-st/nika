//! Server configuration loaded from environment variables.
//!
//! All fields have sensible defaults except `auth_token` which is mandatory.
//! ERRATA-13: `from_env()` returns `Result`, never panics.

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::error::ServeError;

/// Configuration for the Nika HTTP API server.
#[derive(Clone, Debug)]
pub struct ServeConfig {
    /// Socket address to bind to (default: `0.0.0.0:3000`).
    pub bind: SocketAddr,

    /// Directory containing `.nika.yaml` workflow files.
    pub workflows_dir: PathBuf,

    /// Maximum number of concurrent workflow executions.
    pub max_concurrent: usize,

    /// Per-job timeout in seconds (default: 300).
    pub job_timeout_secs: u64,

    /// Maximum bytes to capture from subprocess stdout/stderr (default: 1 MB).
    pub max_output_bytes: usize,

    /// Path to the SQLite database file for job persistence.
    pub db_path: PathBuf,

    /// Bearer token for API authentication (constant-time comparison).
    pub auth_token: String,
}

impl ServeConfig {
    /// Build configuration from environment variables.
    ///
    /// Required: `NIKA_SERVE_TOKEN`
    /// Optional: `NIKA_SERVE_BIND`, `NIKA_SERVE_WORKFLOWS`, `NIKA_SERVE_MAX_CONCURRENT`,
    ///           `NIKA_SERVE_TIMEOUT`, `NIKA_SERVE_DB`
    pub fn from_env() -> Result<Self, ServeError> {
        let auth_token = std::env::var("NIKA_SERVE_TOKEN")
            .map_err(|_| ServeError::Config("NIKA_SERVE_TOKEN must be set".into()))?;

        let bind: SocketAddr = std::env::var("NIKA_SERVE_BIND")
            .unwrap_or_else(|_| "0.0.0.0:3000".into())
            .parse()
            .map_err(|e| ServeError::Config(format!("Invalid NIKA_SERVE_BIND: {e}")))?;

        let max_concurrent = std::env::var("NIKA_SERVE_MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(6);

        let job_timeout_secs = std::env::var("NIKA_SERVE_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);

        let workflows_dir: PathBuf = std::env::var("NIKA_SERVE_WORKFLOWS")
            .unwrap_or_else(|_| "./workflows".into())
            .into();

        let db_path: PathBuf = std::env::var("NIKA_SERVE_DB")
            .unwrap_or_else(|_| ".nika/serve.db".into())
            .into();

        Ok(Self {
            bind,
            workflows_dir,
            max_concurrent,
            job_timeout_secs,
            max_output_bytes: 1024 * 1024, // 1 MB
            db_path,
            auth_token,
        })
    }
}
