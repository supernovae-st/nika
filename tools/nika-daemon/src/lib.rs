//! Nika native daemon — background services for secrets, jobs, watch, and cache.
//!
//! The daemon is Nika's optional background brain. `nika run` works without it,
//! but the daemon adds persistent features: keychain secrets, job scheduling,
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

pub mod client;
pub mod error;
pub mod lifecycle;
pub mod protocol;
pub mod server;
pub mod services;

pub use error::{DaemonError, DaemonResult};
pub use protocol::{DaemonRequest, DaemonResponse};

// Re-exports added as types are implemented:
// pub use client::DaemonClient;
// pub use server::DaemonServer;
