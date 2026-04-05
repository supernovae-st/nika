// CLI uses unix-only daemon features; suppress unused warnings on Windows CI.
#![cfg_attr(not(unix), allow(unused_variables, unused_imports))]

//! CLI subcommand handlers for Nika
//!
//! Each module handles one `nika <subcommand>` group.
//! TUI-dependent handlers (provider, new_wizard) remain in the nika binary crate.

pub mod course;
pub mod showcase;
pub mod trace;

pub mod init;
pub mod mcp;
pub mod pkg;

pub mod model_cloud;
pub mod model_cmd;

#[cfg(feature = "native-inference")]
pub mod model;

#[cfg(unix)]
pub mod cache_cmd;
pub mod clean;
pub mod config;
#[cfg(unix)]
pub mod daemon;
pub mod doctor;
#[cfg(unix)]
pub mod jobs;
pub mod media;
pub mod schema;
pub mod workflow;

pub mod lint;
pub mod machine;
pub mod new_cmd;
pub mod onboarding;
pub mod provider;
pub mod switch;
pub mod tools_cmd;
pub mod vault;
pub mod verbs;
