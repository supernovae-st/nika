//! CLI subcommand handlers
//!
//! Re-exports from the `nika-cli` crate, plus TUI-dependent handlers
//! that stay in the binary crate (provider).

// Re-export all non-TUI handlers from nika-cli
pub use nika_cli::config;
pub use nika_cli::course;
pub use nika_cli::daemon;
pub use nika_cli::doctor;
pub use nika_cli::init;
pub use nika_cli::jobs;
pub use nika_cli::machine;
pub use nika_cli::mcp;
pub use nika_cli::media;
pub use nika_cli::model_cloud;
pub use nika_cli::new_cmd;
pub use nika_cli::pkg;
pub use nika_cli::schema;
pub use nika_cli::showcase;
pub use nika_cli::trace;
pub use nika_cli::verbs;
pub use nika_cli::workflow;

#[cfg(feature = "native-inference")]
pub use nika_cli::model;

// Provider command has no TUI dependency — always available
pub mod provider;
