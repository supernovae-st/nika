// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TUI Standalone Mode
//!
//! File browser, history, and workflow preview for standalone TUI operation.
//!
//! # Layout
//!
//! ```text
//! ┌─────────────────────────────┬─────────────────────────────────────────┐
//! │ [1] WORKFLOW BROWSER        │ [2] HISTORY                             │
//! │ ─────────────────────────   │ ─────────────────────                   │
//! │ examples/                   │ 2026-02-20 01:02:03                     │
//! │ ├── workflow1.nika.yaml     │ ├── workflow.nika.yaml ✓                │
//! │ ├── workflow2.nika.yaml     │ └── 2.7s | 3 tasks                      │
//! │ └── ...                     │                                         │
//! ├─────────────────────────────┴─────────────────────────────────────────┤
//! │ [3] PREVIEW                                                           │
//! │ ─────────────────────────────────────────────────────────             │
//! │ schema: nika/workflow@0.12                                             │
//! │ tasks: ...                                                            │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Crates Used
//!
//! - `ignore`: .gitignore-aware directory traversal (from ripgrep author)
//! - `camino`: UTF-8 safe paths

mod browser;
mod history;
mod panel;
mod state;

pub use state::StandaloneState;
