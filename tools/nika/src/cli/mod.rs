// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CLI subcommand handlers
//!
//! Re-exports from the `nika-cli` crate.
//! The `help` module remains in the binary crate because it uses `Cli::command()`.

// Re-export all handlers from nika-cli
pub use nika_cli::bench;
#[cfg(unix)]
pub use nika_cli::cache_cmd;
pub use nika_cli::check;
pub use nika_cli::clean;
pub use nika_cli::config;
pub use nika_cli::course;
#[cfg(unix)]
pub use nika_cli::daemon;
pub use nika_cli::discover;
pub use nika_cli::doctor;
pub use nika_cli::eval;
#[cfg(unix)]
#[cfg(unix)]
pub use nika_cli::every;
pub use nika_cli::explain;
pub use nika_cli::init;
#[cfg(unix)]
#[cfg(unix)]
pub use nika_cli::jobs;
pub use nika_cli::keys;
pub use nika_cli::lint;
pub use nika_cli::machine;
pub use nika_cli::mcp;
pub use nika_cli::media;
pub use nika_cli::model_cmd;
pub use nika_cli::new_cmd;
pub use nika_cli::onboarding;
pub use nika_cli::pkg;
pub use nika_cli::provider;
pub use nika_cli::run;
#[cfg(unix)]
pub use nika_cli::schedule;
pub use nika_cli::schema;
pub use nika_cli::showcase;
pub use nika_cli::switch;
pub use nika_cli::test_cmd;
pub use nika_cli::token;
pub use nika_cli::tools_cmd;
pub use nika_cli::trace;
pub use nika_cli::verbs;
pub use nika_cli::workflow;

// Custom help system — uses Cli::command(), must stay in the binary crate
pub mod help;
