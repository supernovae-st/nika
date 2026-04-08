// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Machine-level auto-setup for Nika.
//!
//! Detects installed editors/AI tools and configures them automatically.
//! Tracks setup state via `~/.nika/machine.toml` marker file.
//! Called by `nika init` before the project wizard (Phase 1).

pub mod install;
pub mod status;

pub use install::*;
pub use status::*;
