// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared Panel Components for TUI Views
//!
//! This module provides reusable panel widgets that can be composed into different views.
//! Each panel is self-contained and handles its own state, rendering, and input.
//!
//! ## Architecture
//!
//! ```text
//! widgets/panels/
//! ├── browser.rs    ← BrowserPanel (shared file browser for Studio/Runner)
//! ├── task_list.rs  ← TaskListPanel (task list with selection)
//! ├── task_flow.rs  ← TaskBoxFlow (scrollable TaskBox stack)
//! └── info.rs       ← InfoPanel (context-aware detail panel)
//! ```
//!
//! ## Usage
//!
//! Views compose these panels with different behaviors:
//!
//! - **StudioView**: BrowserPanel (on_select → open in editor)
//! - **RunnerView**: BrowserPanel (on_select → run workflow) + TaskListPanel + TaskBoxFlow

mod browser;
mod info;
mod task_flow;
mod task_list;

pub use browser::*;
pub use info::*;
pub use task_flow::*;
pub use task_list::*;
