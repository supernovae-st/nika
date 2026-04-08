// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Proc-macros for Nika workflow engine.
//!
//! Provides derive macros and attribute macros that eliminate boilerplate:
//!
//! - `#[derive(NikaErrorCode)]` — auto-generates `code()` method from `#[nika_code("NIKA-XXX")]`
//! - `#[derive(EventTaskId)]` — auto-generates `task_id()` from `#[has_task_id]` on variants
//! - `#[builtin_tool]` — generates `BuiltinTool` impl from an async function

extern crate proc_macro;
