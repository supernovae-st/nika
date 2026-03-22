//! CLI subcommand handlers for Nika
//!
//! Each module handles one `nika <subcommand>` group.
//! TUI-dependent handlers (provider, new_wizard) remain in the nika binary crate.

pub mod trace;

pub mod init;
pub mod mcp;
pub mod pkg;

#[cfg(feature = "native-inference")]
pub mod model;

pub mod config;
pub mod doctor;
pub mod media;
pub mod schema;
pub mod workflow;

pub mod new_cmd;
