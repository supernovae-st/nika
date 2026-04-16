// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent checkpoint types — re-exported from `nika-error`.
//!
//! The checkpoint data types live in `nika-error` (L0) so they can be
//! shared across kernel module groups without circular dependencies.
//! This module re-exports them for backward compatibility.

pub use nika_error::checkpoint::{AgentCheckpoint, CheckpointMessage, ToolCallRecord};
