//! Nika native daemon — background services for secrets, jobs, watch, and cache.
//!
//! The daemon is Nika's optional background brain. `nika run` works without it,
//! but the daemon adds persistent features: vault secrets, job scheduling,
//! file watching, and LLM response caching.
//!
//! ## Architecture
//!
//! - Single binary: `nika daemon start` uses the same binary
//! - Unix socket IPC: `~/.nika/daemon/nika.sock`
//! - Wire format: 4-byte big-endian length + JSON payload
//! - tokio-native async runtime
//!
//! ## Crate Independence
//!
//! This crate depends on `nika-core` only (for path constants and provider info).
//! It does NOT depend on `nika-engine` — the daemon is lightweight.

#[cfg(unix)]
pub mod client;
pub mod error;
pub mod events;
pub mod provider;
pub mod provider_embedded;
#[cfg(unix)]
pub mod install;
#[cfg(unix)]
pub mod lifecycle;
pub mod protocol;
#[cfg(unix)]
pub mod server;
pub mod services;
pub mod storage;

#[cfg(unix)]
pub use client::{read_auth_token, ConnectedClient, DaemonClient};
pub use error::{DaemonError, DaemonResult};
pub use protocol::{DaemonRequest, DaemonResponse};
pub use provider::{DaemonProvider, DisconnectedProvider};
pub use provider_embedded::EmbeddedDaemonProvider;

#[cfg(unix)]
pub use server::{DaemonConfig, DaemonServer};

// ═══════════════════════════════════════════════════════════════════════════
// DAEMON PATHS
// ═══════════════════════════════════════════════════════════════════════════

use std::path::PathBuf;

/// Environment variable to override the Nika home directory.
pub const NIKA_HOME_ENV: &str = "NIKA_HOME";

/// Returns the daemon directory (`~/.nika/daemon/`).
pub fn daemon_dir() -> PathBuf {
    nika_home().join("daemon")
}

/// Returns the daemon socket path (`~/.nika/daemon/nika.sock`).
pub fn daemon_socket_path() -> PathBuf {
    daemon_dir().join("nika.sock")
}

/// Returns the daemon PID file path (`~/.nika/daemon/nika.pid`).
pub fn daemon_pid_path() -> PathBuf {
    daemon_dir().join("nika.pid")
}

/// Returns the daemon log file path (`~/.nika/daemon/nika.log`).
pub fn daemon_log_path() -> PathBuf {
    daemon_dir().join("nika.log")
}

/// Returns the daemon auth token file path (`~/.nika/daemon/.token`).
pub fn daemon_token_path() -> PathBuf {
    daemon_dir().join(".token")
}

fn nika_home() -> PathBuf {
    if let Ok(custom) = std::env::var(NIKA_HOME_ENV) {
        return PathBuf::from(custom);
    }
    dirs::home_dir()
        .map(|h| h.join(".nika"))
        .unwrap_or_else(|| std::env::temp_dir().join(".nika"))
}
