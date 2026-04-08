// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Node Data Types
//!
//! Data structures and constants for DAG node rendering:
//! colors, animation frames, display modes, border styles, and node metadata.

use ratatui::style::Color;

use crate::theme::{TaskStatus, VerbColor};
use crate::tokens::compat;

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks for theme-aware rendering)
// ═══════════════════════════════════════════════════════════════════════════

/// Failed status color (red)
pub(super) const DEFAULT_FAILED_COLOR: Color = compat::RED_500;
/// Pending/muted content (gray-400)
pub(super) const DEFAULT_MUTED_CONTENT_COLOR: Color = compat::GRAY_400;
/// Active content (gray-100)
pub(super) const DEFAULT_ACTIVE_CONTENT_COLOR: Color = Color::Rgb(243, 244, 246);
/// Pending badge (gray-500)
pub(super) const DEFAULT_PENDING_BADGE_COLOR: Color = compat::GRAY_500;
/// Running badge (amber)
pub(super) const DEFAULT_RUNNING_BADGE_COLOR: Color = compat::AMBER_500;
/// Success badge (green)
pub(super) const DEFAULT_SUCCESS_BADGE_COLOR: Color = compat::GREEN_500;
/// Paused badge (cyan)
pub(super) const DEFAULT_PAUSED_BADGE_COLOR: Color = compat::CYAN_500;
/// For_each indicator (violet)
pub(super) const DEFAULT_FOR_EACH_COLOR: Color = compat::VIOLET_500;
/// Estimate text (gray-500)
pub(super) const DEFAULT_ESTIMATE_COLOR: Color = compat::GRAY_500;
/// Secondary text (gray-400)
pub(super) const DEFAULT_SECONDARY_TEXT_COLOR: Color = compat::GRAY_400;

// ═══════════════════════════════════════════════════════════════════════════
// ANIMATION CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════

/// Spinner animation frames for running tasks
pub(super) const SPINNER_FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];

/// Success celebration frames
pub(super) const SUCCESS_FRAMES: &[&str] = &["✓", "✔", "✓", "✔"];

// ═══════════════════════════════════════════════════════════════════════════
// NODE BOX MODE
// ═══════════════════════════════════════════════════════════════════════════

/// Display mode for node boxes
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NodeBoxMode {
    /// Compact display: icon + id + estimate + badge (3 lines)
    #[default]
    Minimal,
    /// Full display: adds prompt preview and model (5+ lines)
    Expanded,
}

// ═══════════════════════════════════════════════════════════════════════════
// NODE BOX DATA
// ═══════════════════════════════════════════════════════════════════════════

/// Node data for rendering
#[derive(Debug, Clone)]
pub struct NodeBoxData {
    /// Task ID
    pub id: String,
    /// Verb type
    pub verb: VerbColor,
    /// Current status
    pub status: TaskStatus,
    /// Estimated duration
    pub estimate: String,
    /// Prompt preview (for expanded mode)
    pub prompt_preview: Option<String>,
    /// Model name (for expanded mode)
    pub model: Option<String>,
    /// For_each items count
    pub for_each_count: Option<usize>,
    /// For_each item names (for mini-nodes)
    pub for_each_items: Vec<String>,
}

impl NodeBoxData {
    /// Create new node data with required fields
    pub fn new(id: impl Into<String>, verb: VerbColor) -> Self {
        Self {
            id: id.into(),
            verb,
            status: TaskStatus::Pending,
            estimate: String::new(),
            prompt_preview: None,
            model: None,
            for_each_count: None,
            for_each_items: Vec::new(),
        }
    }

    /// Set the task status
    pub fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Set the estimated duration
    pub fn with_estimate(mut self, estimate: impl Into<String>) -> Self {
        self.estimate = estimate.into();
        self
    }

    /// Set the prompt preview text
    pub fn with_prompt_preview(mut self, preview: impl Into<String>) -> Self {
        self.prompt_preview = Some(preview.into());
        self
    }

    /// Set the model name
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set for_each count
    pub fn with_for_each_count(mut self, count: usize) -> Self {
        self.for_each_count = Some(count);
        self
    }

    /// Set for_each item names
    pub fn with_for_each_items(mut self, items: Vec<String>) -> Self {
        self.for_each_items = items;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BORDER STYLE
// ═══════════════════════════════════════════════════════════════════════════

/// Border style for node boxes
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BorderStyle {
    /// Sharp corners (default)
    #[default]
    Sharp,
    /// Rounded corners using ╭╮╰╯
    Rounded,
}

// ═══════════════════════════════════════════════════════════════════════════
// BORDER CHARACTERS
// ═══════════════════════════════════════════════════════════════════════════

/// Border character set based on status
#[derive(Debug, Clone, Copy)]
pub(super) struct BorderChars {
    /// Top-left corner
    pub(super) tl: char,
    /// Top-right corner
    pub(super) tr: char,
    /// Bottom-left corner
    pub(super) bl: char,
    /// Bottom-right corner
    pub(super) br: char,
    /// Horizontal line
    pub(super) h: char,
    /// Vertical line
    pub(super) v: char,
}

impl BorderChars {
    /// Get border characters for a task status with optional rounded corners
    pub(super) fn for_status(status: TaskStatus, style: BorderStyle) -> Self {
        match (status, style) {
            // Queued: thin dashed borders - rounded
            (TaskStatus::Queued, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '┄',
                v: '┆',
            },
            // Queued: thin dashed borders - sharp
            (TaskStatus::Queued, BorderStyle::Sharp) => Self {
                tl: '┌',
                tr: '┐',
                bl: '└',
                br: '┘',
                h: '┄',
                v: '┆',
            },
            // Pending: dashed borders (light) - rounded
            (TaskStatus::Pending, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '┄',
                v: '┆',
            },
            // Pending: dashed borders (light) - sharp
            (TaskStatus::Pending, BorderStyle::Sharp) => Self {
                tl: '┌',
                tr: '┐',
                bl: '└',
                br: '┘',
                h: '┄',
                v: '┆',
            },
            // Running: bold/heavy borders - rounded
            (TaskStatus::Running, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '━',
                v: '┃',
            },
            // Running: bold/heavy borders - sharp
            (TaskStatus::Running, BorderStyle::Sharp) => Self {
                tl: '┏',
                tr: '┓',
                bl: '┗',
                br: '┛',
                h: '━',
                v: '┃',
            },
            // Success: double line borders - rounded
            (TaskStatus::Success, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '═',
                v: '║',
            },
            // Success: double line borders - sharp
            (TaskStatus::Success, BorderStyle::Sharp) => Self {
                tl: '╔',
                tr: '╗',
                bl: '╚',
                br: '╝',
                h: '═',
                v: '║',
            },
            // Failed: double line borders - rounded
            (TaskStatus::Failed, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '═',
                v: '║',
            },
            // Failed: double line borders - sharp
            (TaskStatus::Failed, BorderStyle::Sharp) => Self {
                tl: '╔',
                tr: '╗',
                bl: '╚',
                br: '╝',
                h: '═',
                v: '║',
            },
            // Paused: dotted borders - rounded
            (TaskStatus::Paused, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '┈',
                v: '┊',
            },
            // Paused: dotted borders - sharp
            (TaskStatus::Paused, BorderStyle::Sharp) => Self {
                tl: '┌',
                tr: '┐',
                bl: '└',
                br: '┘',
                h: '┈',
                v: '┊',
            },
            // Skipped: thin dashed borders - rounded
            (TaskStatus::Skipped, BorderStyle::Rounded) => Self {
                tl: '╭',
                tr: '╮',
                bl: '╰',
                br: '╯',
                h: '┄',
                v: '┆',
            },
            // Skipped: thin dashed borders - sharp
            (TaskStatus::Skipped, BorderStyle::Sharp) => Self {
                tl: '┌',
                tr: '┐',
                bl: '└',
                br: '┘',
                h: '┄',
                v: '┆',
            },
        }
    }
}
