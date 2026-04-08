// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Theme-derived color palette for message rendering.
//!
//! Extracted once per frame to avoid repeated field accesses into Theme.

use ratatui::style::Color;

use crate::theme::Theme;

/// Pre-extracted color palette from Theme for message rendering.
///
/// Created once at the top of `render_messages_v2` and passed into sub-renderers
/// to keep the call sites clean.
pub(in crate::views::chat) struct RenderColors {
    pub thinking_header_color: Color,
    pub thinking_content_color: Color,
    pub muted_color: Color,
    pub mcp_box_color: Color,
    pub success_color: Color,
    pub error_color: Color,
    pub infer_box_color: Color,
    pub status_running_color: Color,
    pub selection_bg: Color,
}

impl RenderColors {
    /// Extract all rendering colors from the theme.
    pub fn from_theme(theme: &Theme) -> Self {
        Self {
            thinking_header_color: theme.status_running,
            thinking_content_color: theme.text_muted,
            muted_color: theme.text_muted,
            mcp_box_color: theme.status_success,
            success_color: theme.status_success,
            error_color: theme.status_failed,
            infer_box_color: theme.highlight,
            status_running_color: theme.status_running,
            selection_bg: theme.highlight,
        }
    }
}
