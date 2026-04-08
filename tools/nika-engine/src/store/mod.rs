// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Store Module - state management
//!
//! Thread-safe storage for task execution results.
//! Uses DashMap for lock-free concurrent access.
//!
//! Key types:
//! - `RunContext`: Central storage for task results
//! - `TaskResult`: Execution result with status and output
//! - `TaskOutcome`: Success or failure status
//! - `LoadedContext`: Loaded workflow context files

pub mod context;
pub mod record_writer;
mod run_context;

// Re-export all public types
pub use context::LoadedContext;
pub use record_writer::RecordWriter;
pub use run_context::{RunContext, TaskOutcome, TaskResult};
