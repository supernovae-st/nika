//! Nika Daemon Module
//!
//! Provides background services via Unix socket IPC:
//! - Keychain access (single accessor, no popup spam)
//! - Secret resolution
//! - Health monitoring
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  nika daemon start                                                          │
//! │  ├── DaemonServer (Unix socket: ~/.nika/daemon/nika.sock)                   │
//! │  ├── PID file with flock() (~/.nika/daemon/nika.pid)                        │
//! │  └── Signal handling (SIGTERM, SIGHUP)                                      │
//! │                                                                             │
//! │  Clients connect via:                                                       │
//! │  ├── nika provider get <name>  → GetSecret request                          │
//! │  ├── nika daemon status        → Ping request                               │
//! │  └── nika daemon stop          → Shutdown request                           │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Protocol
//!
//! JSON-over-Unix-socket with newline-delimited messages:
//!
//! ```json
//! {"type": "GetSecret", "provider": "anthropic"}
//! {"type": "Secret", "value": "sk-ant-..."}
//! ```

mod client;
mod pid;
mod protocol;
mod server;

pub use client::DaemonClient;
pub use pid::{acquire_pid_lock, release_pid_lock, PidLock};
pub use protocol::{DaemonRequest, DaemonResponse};
pub use server::DaemonServer;

/// Check if the daemon is running by attempting to connect
pub async fn is_daemon_running() -> bool {
    DaemonClient::connect().await.is_ok()
}

/// Start the daemon server (blocking)
pub async fn start_daemon() -> crate::error::Result<()> {
    let server = DaemonServer::new()?;
    server.run().await
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _ = std::mem::size_of::<DaemonRequest>();
        let _ = std::mem::size_of::<DaemonResponse>();
    }
}
