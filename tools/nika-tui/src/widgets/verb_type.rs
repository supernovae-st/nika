// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb Type
//!
//! Shared enum for task verb classification across TUI widgets.
//! Extracted from the deprecated dag.rs widget.

use ratatui::style::Color;

use crate::theme::VerbColor;

/// Verb type for task icon display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerbType {
    #[default]
    Unknown,
    Infer,
    Exec,
    Fetch,
    Invoke,
    Agent,
}

impl VerbType {
    /// Get icon for this verb (matches CLAUDE.md canonical icons)
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Unknown => "📋",
            Self::Infer => "⚡",
            Self::Exec => "📟",
            Self::Fetch => "🛰️",
            Self::Invoke => "🔌",
            Self::Agent => "🐔",
        }
    }

    /// Get icon for subagent (spawned via spawn_agent)
    pub fn subagent_icon() -> &'static str {
        "🐤"
    }

    /// Get the color for this verb type.
    pub fn color(&self) -> Color {
        match self {
            Self::Unknown => Color::Rgb(107, 114, 128),
            Self::Infer => VerbColor::Infer.rgb(),
            Self::Exec => VerbColor::Exec.rgb(),
            Self::Fetch => VerbColor::Fetch.rgb(),
            Self::Invoke => VerbColor::Invoke.rgb(),
            Self::Agent => VerbColor::Agent.rgb(),
        }
    }

    /// Get the label for this verb type
    pub fn label(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Infer => "infer",
            Self::Exec => "exec",
            Self::Fetch => "fetch",
            Self::Invoke => "invoke",
            Self::Agent => "agent",
        }
    }

    /// Parse verb type from string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "infer" => Self::Infer,
            "exec" => Self::Exec,
            "fetch" => Self::Fetch,
            "invoke" => Self::Invoke,
            "agent" => Self::Agent,
            _ => Self::Unknown,
        }
    }

    /// Parse verb type from string, returning None for unknown
    pub fn try_parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "infer" => Some(Self::Infer),
            "exec" => Some(Self::Exec),
            "fetch" => Some(Self::Fetch),
            "invoke" => Some(Self::Invoke),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}
