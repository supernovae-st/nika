//! CLI subcommand handlers
//!
//! Each module handles one `nika <subcommand>` group.

pub mod trace;

#[cfg(feature = "tui")]
pub mod provider;

pub mod init;
pub mod mcp;
pub mod pkg;

#[cfg(feature = "native-inference")]
pub mod model;

pub mod config;
pub mod doctor;
pub mod schema;
pub mod workflow;

#[cfg(feature = "jobs")]
pub mod jobs;

pub mod new_cmd;
