// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared CLI formatting utilities for consistent command output.
//!
//! Provides reusable building blocks for all `nika` subcommands:
//! - Panels with box-drawing characters
//! - Aligned key-value display
//! - Status indicators with consistent icons
//! - Section headers and separators
//!
//! These utilities ensure visual consistency across all CLI commands
//! (doctor, provider list, model list, etc.).

use colored::Colorize;

// ═══════════════════════════════════════════════════════════════════════════
// STATUS ICONS
// ═══════════════════════════════════════════════════════════════════════════

/// Standard status icons for CLI output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusIcon {
    /// Green checkmark — success/pass.
    Ok,
    /// Red cross — failure/error.
    Fail,
    /// Yellow tilde — warning/partial.
    Warn,
    /// Cyan info — informational.
    Info,
    /// Dimmed skip — skipped/not applicable.
    Skip,
    /// Cyan arrow down — downloading/in progress.
    Download,
    /// Right arrow — action hint.
    Hint,
}

impl StatusIcon {
    /// Render the icon as a colored string.
    pub fn render(self) -> String {
        match self {
            Self::Ok => "✓".green().bold().to_string(),
            Self::Fail => "✗".red().bold().to_string(),
            Self::Warn => "⚠".yellow().to_string(),
            Self::Info => "ℹ".cyan().to_string(),
            Self::Skip => "⊘".dimmed().to_string(),
            Self::Download => "⬇".cyan().to_string(),
            Self::Hint => "→".dimmed().to_string(),
        }
    }
}

impl std::fmt::Display for StatusIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.render())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PANEL — Box-drawing panels for section framing
// ═══════════════════════════════════════════════════════════════════════════

/// Render a panel with rounded corners and a title.
///
/// ```text
/// ╭─────────────────────────╮
/// │  Title                  │
/// ╰─────────────────────────╯
/// ```
pub fn panel(title: &str, width: usize) -> String {
    let inner = width.saturating_sub(4); // 2 for borders + 2 for padding
    let bar = "─".repeat(inner + 2);
    format!(
        "  ╭{bar}╮\n  │ {:<inner$} │\n  ╰{bar}╯",
        title.bold(),
        inner = inner,
        bar = bar,
    )
}

/// Render a panel with title and content lines.
///
/// ```text
/// ╭─────────────────────────╮
/// │  Title                  │
/// ├─────────────────────────┤
/// │  line 1                 │
/// │  line 2                 │
/// ╰─────────────────────────╯
/// ```
pub fn panel_with_content(title: &str, lines: &[String], width: usize) -> String {
    let inner = width.saturating_sub(4);
    let bar = "─".repeat(inner + 2);
    let mut out = format!("  ╭{bar}╮\n  │ {:<inner$} │\n  ├{bar}┤\n", title.bold());
    for line in lines {
        out.push_str(&format!("  │ {:<inner$} │\n", line));
    }
    out.push_str(&format!("  ╰{bar}╯"));
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION HEADERS
// ═══════════════════════════════════════════════════════════════════════════

/// Print a section header with a separator line.
///
/// ```text
///   Providers
///   ────────────────────
/// ```
pub fn section_header(title: &str) -> String {
    let sep = "─".repeat(title.len().max(40));
    format!("\n  {}\n  {}", title.bold(), sep.dimmed())
}

/// Print a section header with a subtitle (e.g., count).
///
/// ```text
///   Providers (4/7 configured)
///   ────────────────────────────
/// ```
pub fn section_header_with_subtitle(title: &str, subtitle: &str) -> String {
    let full = format!("{title} ({subtitle})");
    let sep = "─".repeat(full.len().max(40));
    format!(
        "\n  {} ({})\n  {}",
        title.bold(),
        subtitle.dimmed(),
        sep.dimmed()
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// KEY-VALUE PAIRS
// ═══════════════════════════════════════════════════════════════════════════

/// Format a key-value pair with aligned label.
///
/// ```text
///     Provider:   anthropic
///     Status:     ✓ configured
/// ```
pub fn key_value(label: &str, value: &str) -> String {
    format!("    {:<12} {}", format!("{label}:").dimmed(), value)
}

/// Format a key-value pair with custom label width.
pub fn key_value_width(label: &str, value: &str, width: usize) -> String {
    format!(
        "    {:<width$} {}",
        format!("{label}:").dimmed(),
        value,
        width = width
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// STATUS LINES
// ═══════════════════════════════════════════════════════════════════════════

/// Format a status line with icon and message.
///
/// ```text
///     ✓ anthropic    sk-ant-a... (vault)
///     ✗ mistral      → nika keys set mistral
/// ```
pub fn status_line(icon: StatusIcon, message: &str) -> String {
    format!("    {} {message}", icon.render())
}

/// Format a status line with a hint (dimmed right-side text).
pub fn status_line_with_hint(icon: StatusIcon, message: &str, hint: &str) -> String {
    format!("    {} {message}  {}", icon.render(), hint.dimmed())
}

// ═══════════════════════════════════════════════════════════════════════════
// TREE CONNECTORS
// ═══════════════════════════════════════════════════════════════════════════

/// Tree connector for non-last items.
pub const TREE_BRANCH: &str = "├──";

/// Tree connector for the last item.
pub const TREE_LAST: &str = "└──";

/// Tree continuation for nested items.
pub const TREE_PIPE: &str = "│  ";

/// Get the appropriate tree connector for an item.
pub fn tree_connector(is_last: bool) -> &'static str {
    if is_last {
        TREE_LAST
    } else {
        TREE_BRANCH
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════════════════════════════════════════

/// Format a separator line (dimmed).
pub fn separator(width: usize) -> String {
    format!("  {}", "─".repeat(width).dimmed())
}

/// Format a hint line (dimmed, indented).
pub fn hint(message: &str) -> String {
    format!("  {}", message.dimmed())
}

/// Get the terminal width, falling back to 80.
pub fn terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_icon_ok_contains_checkmark() {
        let s = StatusIcon::Ok.render();
        assert!(s.contains('✓'), "Ok icon must contain ✓");
    }

    #[test]
    fn status_icon_fail_contains_cross() {
        let s = StatusIcon::Fail.render();
        assert!(s.contains('✗'), "Fail icon must contain ✗");
    }

    #[test]
    fn status_icon_display_works() {
        // Display trait delegates to render()
        let s = format!("{}", StatusIcon::Info);
        assert!(s.contains('ℹ'));
    }

    #[test]
    fn all_status_icons_render() {
        let icons = [
            StatusIcon::Ok,
            StatusIcon::Fail,
            StatusIcon::Warn,
            StatusIcon::Info,
            StatusIcon::Skip,
            StatusIcon::Download,
            StatusIcon::Hint,
        ];
        for icon in icons {
            let rendered = icon.render();
            assert!(!rendered.is_empty(), "{icon:?} must render non-empty");
        }
    }

    #[test]
    fn panel_has_correct_structure() {
        let p = panel("Test Title", 40);
        assert!(p.contains("╭"), "panel must have top-left corner");
        assert!(p.contains("╰"), "panel must have bottom-left corner");
        assert!(p.contains("Test Title"), "panel must contain title");
    }

    #[test]
    fn panel_with_content_has_separator() {
        let p = panel_with_content("Header", &["line1".into(), "line2".into()], 40);
        assert!(p.contains("├"), "panel must have separator");
        assert!(p.contains("line1"));
        assert!(p.contains("line2"));
    }

    #[test]
    fn section_header_has_separator() {
        let h = section_header("Providers");
        assert!(h.contains("Providers"));
        assert!(h.contains("─"));
    }

    #[test]
    fn section_header_with_subtitle_works() {
        let h = section_header_with_subtitle("Providers", "4/7 configured");
        assert!(h.contains("Providers"));
        assert!(h.contains("4/7 configured"));
    }

    #[test]
    fn key_value_alignment() {
        let kv = key_value("Provider", "anthropic");
        assert!(kv.contains("Provider:"));
        assert!(kv.contains("anthropic"));
    }

    #[test]
    fn key_value_width_custom() {
        let kv = key_value_width("Provider", "anthropic", 20);
        assert!(kv.contains("Provider:"));
        assert!(kv.contains("anthropic"));
    }

    #[test]
    fn status_line_format() {
        let sl = status_line(StatusIcon::Ok, "anthropic configured");
        assert!(sl.contains('✓'));
        assert!(sl.contains("anthropic configured"));
    }

    #[test]
    fn status_line_with_hint_format() {
        let sl = status_line_with_hint(StatusIcon::Fail, "mistral", "nika keys set mistral");
        assert!(sl.contains('✗'));
        assert!(sl.contains("mistral"));
    }

    #[test]
    fn tree_connectors_are_correct() {
        assert_eq!(tree_connector(false), "├──");
        assert_eq!(tree_connector(true), "└──");
    }

    #[test]
    fn separator_produces_dashes() {
        let s = separator(40);
        assert!(s.contains("─"));
    }

    #[test]
    fn hint_produces_dimmed_text() {
        let h = hint("Use nika keys set <name>");
        assert!(h.contains("nika keys set"));
    }

    #[test]
    fn terminal_width_returns_positive() {
        let w = terminal_width();
        assert!(w > 0, "terminal width must be positive");
    }

    #[test]
    fn status_icon_eq() {
        assert_eq!(StatusIcon::Ok, StatusIcon::Ok);
        assert_ne!(StatusIcon::Ok, StatusIcon::Fail);
    }

    #[test]
    fn panel_narrow_width_doesnt_panic() {
        // Even with very narrow width, shouldn't panic
        let p = panel("X", 4);
        assert!(p.contains("X"));
    }
}
