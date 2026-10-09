// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Supernovae butterfly, relocated with its renditions to
//! [`nika_tui_view::visual::logomark`] (ADR-143): this path keeps the
//! renderer's callers unchanged.

pub use nika_tui_view::visual::logomark::{
    REVEAL_AT, REVEAL_ENDS, SOURCE_PATH, SOURCE_SHA256, Size, revealing,
};
