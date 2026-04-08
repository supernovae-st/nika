// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Course module — interactive learning path for Nika
//!
//! 12 levels with a Liberation theme, from basic shell commands
//! to full MCP orchestration. Each level has exercises, checks,
//! hints, and progress tracking.

pub mod checks;
pub mod exercises;
pub mod exercises_advanced;
pub mod generator;
pub mod hints;
pub mod levels;
pub mod missions;
pub mod progress;
pub mod showcase;
pub mod showcase_builtin;
pub mod showcase_exec;
pub mod showcase_llm;
pub mod templates;
