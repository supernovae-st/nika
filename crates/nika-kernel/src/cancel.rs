// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cancellation context — re-exported from `nika-error`.
//!
//! The `CancelCtx` struct lives in `nika-error` (L0) so it can be shared
//! across kernel module groups without circular dependencies.
//! This module re-exports it for backward compatibility.

pub use nika_error::cancel::CancelCtx;
