//! Server configuration loaded from nika.toml [serve] section + environment variables.
//!
//! Merge order: env vars > nika.toml [serve] > hardcoded defaults.
//! `auth_token` is ALWAYS from env (secrets never in nika.toml).
//! ERRATA-13: `from_env()` returns `Result`, never panics.

use std::net::SocketAddr;
use std::path::PathBuf;

use crate::error::ServeError;

/// [serve] section from nika.toml (deserialized, all optional).
#[derive(Debug, Default, serde::Deserialize)]
struct NikaTomlServe {
    #[serde(default)]
    bind: Option<String>,
    #[serde(default)]
    workflows: Option<String>,
    #[serde(default)]
    max_concurrent: Option<usize>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    rate_limit: Option<u64>,
    #[serde(default)]
    rate_burst: Option<u32>,
    #[serde(default)]
    gc_retention: Option<u64>,
    #[serde(default)]
    gc_interval: Option<u64>,
}

/// Minimal nika.toml shape — only [serve] section.
#[derive(Debug, Default, serde::Deserialize)]
struct NikaTomlPartial {
    #[serde(default)]
    serve: Option<NikaTomlServe>,
}

/// Try to read [serve] from nika.toml by walking up from cwd.
fn load_serve_from_nika_toml() -> NikaTomlServe {
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return NikaTomlServe::default(),
    };
    // Walk up looking for nika.toml
    let mut dir = cwd.as_path();
    loop {
        let path = dir.join("nika.toml");
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(parsed) = toml::from_str::<NikaTomlPartial>(&content) {
                    return parsed.serve.unwrap_or_default();
                }
            }
            return NikaTomlServe::default();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    NikaTomlServe::default()
}

/// Execution mode for workflow processing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ExecutorMode {
    /// V1: spawn `nika run <workflow>` as a subprocess.
    Subprocess,
    /// V2: run workflow in-process via embedded nika-engine Runner (default).
    #[default]
    Embedded,
}

/// Configuration for the Nika HTTP API server.
#[derive(Clone, Debug)]
pub struct ServeConfig {
    /// Socket address to bind to (default: `127.0.0.1:3000`).
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

    /// Optional CORS allowed origin (e.g. `https://jungo.supernovae.studio`).
    /// When `None`, no CORS headers are sent (default — most secure).
    pub cors_origin: Option<String>,

    /// Execution backend (subprocess or embedded).
    pub executor_mode: ExecutorMode,

    /// Rate limit: requests per second per token (default: 10).
    pub rate_per_second: u64,

    /// Rate limit: burst capacity per token (default: 30).
    pub rate_burst: u32,

    /// Job GC: retention period in seconds (default: 7 days).
    pub gc_retention_secs: u64,

    /// Job GC: interval between GC runs in seconds (default: 1 hour).
    pub gc_interval_secs: u64,
}

impl ServeConfig {
    /// Build configuration from nika.toml [serve] section + environment variables.
    ///
    /// Merge: env vars > nika.toml [serve] > hardcoded defaults.
    /// Required: `NIKA_SERVE_TOKEN` (always from env — secrets never in nika.toml)
    /// Optional: `NIKA_SERVE_BIND`, `NIKA_SERVE_WORKFLOWS`, `NIKA_SERVE_MAX_CONCURRENT`,
    ///           `NIKA_SERVE_TIMEOUT`, `NIKA_SERVE_DB`
    pub fn from_env() -> Result<Self, ServeError> {
        // Layer 1: Read nika.toml [serve] as base
        let toml = load_serve_from_nika_toml();

        // auth_token: always from env (secrets never in nika.toml)
        let auth_token = std::env::var("NIKA_SERVE_TOKEN")
            .map_err(|_| ServeError::Config("NIKA_SERVE_TOKEN must be set".into()))?;

        if auth_token.len() < 32 {
            return Err(ServeError::Config(
                "NIKA_SERVE_TOKEN must be at least 32 characters. \
                 Generate one with: openssl rand -hex 32"
                    .into(),
            ));
        }

        let cors_origin = std::env::var("NIKA_SERVE_CORS_ORIGIN").ok();

        // Layer 2: env var > nika.toml > default
        let bind_str = std::env::var("NIKA_SERVE_BIND")
            .ok()
            .or(toml.bind)
            .unwrap_or_else(|| "127.0.0.1:3000".into());
        let bind: SocketAddr = bind_str
            .parse()
            .map_err(|e| ServeError::Config(format!("Invalid bind address: {e}")))?;

        let max_concurrent = std::env::var("NIKA_SERVE_MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(toml.max_concurrent)
            .unwrap_or(6);

        let job_timeout_secs = std::env::var("NIKA_SERVE_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(toml.timeout)
            .unwrap_or(300);

        // Default changed from "./workflows" to "." (recursive scan from project root)
        let workflows_dir: PathBuf = std::env::var("NIKA_SERVE_WORKFLOWS")
            .ok()
            .or(toml.workflows)
            .unwrap_or_else(|| ".".into())
            .into();

        let db_path: PathBuf = std::env::var("NIKA_SERVE_DB")
            .unwrap_or_else(|_| ".nika/serve.db".into())
            .into();

        let executor_raw = std::env::var("NIKA_SERVE_EXECUTOR")
            .unwrap_or_else(|_| "embedded".into());
        let executor_mode = match executor_raw.as_str() {
            "embedded" => ExecutorMode::Embedded,
            "subprocess" => ExecutorMode::Subprocess,
            other => {
                tracing::warn!(
                    value = other,
                    "unknown NIKA_SERVE_EXECUTOR value, defaulting to embedded"
                );
                ExecutorMode::Embedded
            }
        };

        let rate_per_second = std::env::var("NIKA_SERVE_RATE_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(toml.rate_limit)
            .unwrap_or(10);

        let rate_burst = std::env::var("NIKA_SERVE_RATE_BURST")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(toml.rate_burst)
            .unwrap_or(30);

        let gc_retention_secs = std::env::var("NIKA_SERVE_GC_RETENTION")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(toml.gc_retention)
            .unwrap_or(7 * 24 * 3600);

        let gc_interval_secs = std::env::var("NIKA_SERVE_GC_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .or(toml.gc_interval)
            .unwrap_or(3600);

        Ok(Self {
            bind,
            workflows_dir,
            max_concurrent,
            job_timeout_secs,
            max_output_bytes: 1024 * 1024, // 1 MB
            db_path,
            auth_token,
            cors_origin,
            executor_mode,
            rate_per_second,
            rate_burst,
            gc_retention_secs,
            gc_interval_secs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bind_address_is_localhost() {
        let default_bind: std::net::SocketAddr = "127.0.0.1:3000".parse().unwrap();
        let fallback: std::net::SocketAddr =
            "0.0.0.0:3000".parse::<std::net::SocketAddr>().unwrap();
        assert!(
            default_bind.ip().is_loopback(),
            "default must bind to loopback"
        );
        assert!(
            !fallback.ip().is_loopback(),
            "0.0.0.0 is NOT loopback — this is the bug"
        );
    }

    // Phase 3 TDD tests

    #[test]
    fn serve_config_parses_toml_section() {
        let toml_content = r#"
[serve]
bind = "0.0.0.0:8080"
workflows = "./my-workflows"
max_concurrent = 12
timeout = 600
"#;
        let parsed: NikaTomlPartial = toml::from_str(toml_content).unwrap();
        let serve = parsed.serve.unwrap();
        assert_eq!(serve.bind.as_deref(), Some("0.0.0.0:8080"));
        assert_eq!(serve.workflows.as_deref(), Some("./my-workflows"));
        assert_eq!(serve.max_concurrent, Some(12));
        assert_eq!(serve.timeout, Some(600));
    }

    #[test]
    fn serve_config_missing_section_uses_defaults() {
        let toml_content = r#"
[project]
name = "no-serve"
"#;
        let parsed: NikaTomlPartial = toml::from_str(toml_content).unwrap();
        assert!(parsed.serve.is_none());
        let serve = parsed.serve.unwrap_or_default();
        assert!(serve.bind.is_none());
        assert!(serve.workflows.is_none());
    }

    #[test]
    fn serve_config_workflows_default_is_dot() {
        // When no env var and no nika.toml, workflows defaults to "."
        let default = NikaTomlServe::default();
        assert!(
            default.workflows.is_none(),
            "TOML default should be None (falls through to \".\" in from_env)"
        );
    }
}
