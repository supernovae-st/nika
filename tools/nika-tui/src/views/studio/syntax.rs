// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML syntax highlighting for the Studio editor.
//!
//! Provides Solarized-based color coding for YAML keys, values, comments,
//! Nika verbs (infer, exec, fetch, invoke, agent), and structure keywords.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// YAML syntax highlighting colors
pub(super) struct YamlHighlight;

impl YamlHighlight {
    /// Key color (Solarized blue)
    pub(crate) const KEY: Color = Color::Rgb(38, 139, 210);
    /// String value color (Solarized green)
    pub(crate) const STRING: Color = Color::Rgb(133, 153, 0);
    /// Number value color (Solarized orange)
    pub(crate) const NUMBER: Color = Color::Rgb(203, 75, 22);
    /// Boolean value color (Solarized violet)
    pub(crate) const BOOL: Color = Color::Rgb(108, 113, 196);
    /// Comment color (Solarized base01 - muted)
    pub(crate) const COMMENT: Color = Color::Rgb(88, 110, 117);
    /// Nika verbs (Solarized MAGENTA) - infer, exec, fetch, invoke, agent
    pub(crate) const VERB: Color = Color::Rgb(211, 54, 130);
    /// Nika structure keywords (Solarized yellow)
    pub(crate) const NIKA_KEYWORD: Color = Color::Rgb(181, 137, 0);

    /// Highlight a single YAML line into styled spans
    pub(super) fn highlight_line(line: &str, base_style: Style) -> Vec<Span<'static>> {
        let line_owned = line.to_string();
        let trimmed = line_owned.trim();

        // Empty line
        if trimmed.is_empty() {
            return vec![Span::styled(line_owned, base_style)];
        }

        // Full-line comment
        if trimmed.starts_with('#') {
            return vec![Span::styled(line_owned, base_style.fg(Self::COMMENT))];
        }

        // Try to parse as key: value
        if let Some(colon_pos) = line.find(':') {
            let (key_part, rest) = line.split_at(colon_pos);
            let key_trimmed = key_part.trim();

            // Check if it's a Nika verb (magenta) or Nika keyword (yellow)
            let key_color = if matches!(
                key_trimmed,
                "infer" | "exec" | "fetch" | "invoke" | "agent" | "decompose"
            ) {
                Self::VERB // Nika verbs in magenta
            } else if matches!(
                key_trimmed,
                "schema"
                    | "workflow"
                    | "tasks"
                    | "flows"
                    | "mcp"
                    | "servers"
                    | "with"
                    | "params"
                    | "context"
                    | "include"
                    | "skills"
                    | "for_each"
                    | "id"
                    | "model"
                    | "provider"
            ) {
                Self::NIKA_KEYWORD // Structure keywords in yellow
            } else {
                Self::KEY
            };

            let mut spans = vec![Span::styled(
                format!("{}:", key_part),
                base_style.fg(key_color),
            )];

            // Handle the value part (skip the colon we already included)
            let value = &rest[1..]; // Skip ':'
            if !value.is_empty() {
                spans.push(Self::highlight_value(value, base_style));
            }

            return spans;
        }

        // List item (- value)
        if trimmed.starts_with('-') {
            // SAFETY: starts_with('-') guarantees find('-') will succeed
            if let Some(dash_pos) = line.find('-') {
                let before_dash = &line[..dash_pos];
                let after_dash = &line[dash_pos + 1..];

                let mut spans = vec![
                    Span::styled(before_dash.to_string(), base_style),
                    Span::styled("-".to_string(), base_style.fg(Self::KEY)),
                ];

                if !after_dash.is_empty() {
                    spans.push(Self::highlight_value(after_dash, base_style));
                }

                return spans;
            }
        }

        // Default: return as-is
        vec![Span::styled(line_owned, base_style)]
    }

    /// Highlight a YAML value
    fn highlight_value(value: &str, base_style: Style) -> Span<'static> {
        let trimmed = value.trim();

        // String in quotes
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return Span::styled(value.to_string(), base_style.fg(Self::STRING));
        }

        // Boolean
        if matches!(trimmed, "true" | "false" | "yes" | "no" | "True" | "False") {
            return Span::styled(value.to_string(), base_style.fg(Self::BOOL));
        }

        // Number (integer or float)
        if trimmed.parse::<f64>().is_ok() || trimmed.parse::<i64>().is_ok() {
            return Span::styled(value.to_string(), base_style.fg(Self::NUMBER));
        }

        // Inline comment - for now, return as-is (would need multiple spans)
        if value.contains(" #") {
            return Span::styled(value.to_string(), base_style);
        }

        // Default
        Span::styled(value.to_string(), base_style)
    }
}
