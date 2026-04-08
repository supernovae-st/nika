// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
pub mod rules;

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
pub mod discover;
pub mod doctor;
#[cfg(unix)]
pub mod every;
#[cfg(unix)]
pub mod jobs;
pub mod media;
#[cfg(unix)]
pub mod schedule;
pub mod schema;
pub mod workflow;

pub mod eval;
pub mod inputs;
pub mod keys;
pub mod lint;
pub mod machine;
pub mod new_cmd;
pub mod onboarding;
pub mod provider;
pub mod run;
pub mod switch;
pub mod task_filter;
pub mod token;
pub mod tools_cmd;
pub mod verbs;
