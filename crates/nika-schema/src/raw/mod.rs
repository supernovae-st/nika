// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw AST types — the direct output of YAML parsing.
//!
//! Raw types preserve source spans for error reporting and do NOT perform
//! semantic validation. They are fed to the analyzer which produces
//! `AnalyzedWorkflow` types.

pub mod action;
pub mod mcp;
pub mod task;
pub mod workflow;

pub use action::{RawAction, RawAgentAction, RawExecAction, RawInferAction, RawInvokeAction};
pub use mcp::{RawMcpConfig, RawMcpServer};
pub use task::RawTask;
pub use workflow::RawWorkflow;
