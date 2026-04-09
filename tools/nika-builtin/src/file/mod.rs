// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin file tools — `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`.
//!
//! All tools implement the kernel `BuiltinTool` trait and use `FileToolContext`
//! for working-directory validation and the read-before-edit cache.
//!
//! `shield.rs` provides `check_path_readable()` which reads trust state from
//! `nika_kernel::task_local` — no dependency on nika-engine required.

pub mod context;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod shield;
pub mod write;

pub use context::FileToolContext;
pub use edit::EditTool;
pub use glob::GlobTool;
pub use grep::GrepTool;
pub use read::ReadTool;
pub use write::WriteTool;
