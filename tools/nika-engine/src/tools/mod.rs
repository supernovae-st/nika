// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool Context — working directory + permission mode for the executor.
//!
//! File tool implementations (read, write, edit, glob, grep) and the Shield
//! Item 3b path reconnaissance check have moved to `nika-builtin`. This module
//! retains only:
//!
//! - [`ToolContext`] — working directory + permission mode for the executor
//! - [`PermissionMode`] — 4-level permission gate

mod context;

pub use context::{PermissionMode, ToolContext};
