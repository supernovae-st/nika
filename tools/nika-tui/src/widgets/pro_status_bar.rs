// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pro Status Bar - Claude Code-inspired comprehensive status display
//!
//! Two-line status bar showing all session metrics:
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ 🧠 Claude Opus 4.5 │ ⚡ Infer │ 🧠 Thinking: ON                             │
//! │ 💰 $0.42 │ 🔢 1.2k/200k (0.6%) ▓▓░░░░░░░░ │ ⏱ 3m 12s │ 📝 +42/-8 │ MCP:●   │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::time::{Duration, Instant};

use super::{McpServerInfo, McpStatus, Provider};
use crate::theme::{Theme, VerbColor};
use crate::utils::format_number_compact;

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks when no theme provided)
// ═══════════════════════════════════════════════════════════════════════════

const COLOR_GOLD: Color = Color::Rgb(250, 204, 21); // #facc15 - Headers/cost
const COLOR_CYAN: Color = Color::Rgb(34, 211, 238); // #22d3ee - Model name
const COLOR_LIME: Color = Color::Rgb(132, 204, 22); // #84cc16 - Positive/success
const COLOR_CORAL: Color = Color::Rgb(251, 113, 133); // #fb7185 - Negative/error
const COLOR_PINK: Color = Color::Rgb(236, 72, 153); // #ec4899 - Tokens highlight
const COLOR_GRAY: Color = Color::Rgb(107, 114, 128); // #6b7280 - Muted
const COLOR_TURQUOISE: Color = Color::Rgb(45, 212, 191); // #2dd4bf - Accents
const COLOR_VIOLET: Color = Color::Rgb(139, 92, 246); // #8b5cf6 - Agent mode
const COLOR_ORANGE: Color = Color::Rgb(251, 146, 60); // #fb923c - Warning level

/// Chat mode indicator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatModeIndicator {
    #[default]
    Infer,
    Agent,
}

impl ChatModeIndicator {
    pub fn icon(&self) -> &'static str {
        match self {
            ChatModeIndicator::Infer => "⚡",
            ChatModeIndicator::Agent => "🐔",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ChatModeIndicator::Infer => "Infer",
            ChatModeIndicator::Agent => "Agent",
        }
    }

    /// Get color for this mode (theme-aware with fallback)
    pub fn color(&self) -> Color {
        match self {
            ChatModeIndicator::Infer => COLOR_LIME,
            ChatModeIndicator::Agent => COLOR_VIOLET,
        }
    }

    /// Get color using theme (delegates to VerbColor for consistency)
    pub fn color_with_theme(&self, theme: Option<&Theme>) -> Color {
        match self {
            ChatModeIndicator::Infer => theme.map(|_| VerbColor::Infer.rgb()).unwrap_or(COLOR_LIME),
            ChatModeIndicator::Agent => theme
                .map(|_| VerbColor::Agent.rgb())
                .unwrap_or(COLOR_VIOLET),
        }
    }
}

/// Session metrics for the pro status bar
#[derive(Debug, Clone)]
pub struct SessionMetrics {
    /// Total cost in USD
    pub cost_usd: f64,
    /// Input tokens used
    pub input_tokens: u64,
    /// Output tokens used
    pub output_tokens: u64,
    /// Maximum context tokens
    pub max_tokens: u64,
    /// Session start time
    pub session_start: Instant,
    /// Lines added in this session
    pub lines_added: u32,
    /// Lines removed in this session
    pub lines_removed: u32,
    /// MCP servers
    pub mcp_servers: Vec<McpServerInfo>,
}

impl Default for SessionMetrics {
    fn default() -> Self {
        Self {
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            max_tokens: 200_000,
            session_start: Instant::now(),
            lines_added: 0,
            lines_removed: 0,
            mcp_servers: vec![],
        }
    }
}

impl SessionMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    pub fn context_percentage(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            (self.total_tokens() as f64 / self.max_tokens as f64) * 100.0
        }
    }

    pub fn session_duration(&self) -> Duration {
        self.session_start.elapsed()
    }

    pub fn mcp_connected_count(&self) -> usize {
        self.mcp_servers
            .iter()
            .filter(|s| s.status == McpStatus::Connected)
            .count()
    }
}

/// Pro status bar widget with two-line display
pub struct ProStatusBar<'a> {
    /// Current provider
    provider: Provider,
    /// Current model name
    model: &'a str,
    /// Chat mode (Infer or Agent)
    mode: ChatModeIndicator,
    /// Deep thinking enabled
    thinking_enabled: bool,
    /// Session metrics
    metrics: &'a SessionMetrics,
    /// Is currently streaming
    is_streaming: bool,
    /// Optional theme for colors
    theme: Option<&'a Theme>,
}

impl<'a> ProStatusBar<'a> {
    pub fn new(model: &'a str, metrics: &'a SessionMetrics) -> Self {
        Self {
            provider: Provider::from_model_name(model),
            model,
            mode: ChatModeIndicator::Infer,
            thinking_enabled: false,
            metrics,
            is_streaming: false,
            theme: None,
        }
    }

    /// Set the theme for colors
    pub fn with_theme(mut self, theme: &'a Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn mode(mut self, mode: ChatModeIndicator) -> Self {
        self.mode = mode;
        self
    }

    pub fn thinking(mut self, enabled: bool) -> Self {
        self.thinking_enabled = enabled;
        self
    }

    pub fn streaming(mut self, is_streaming: bool) -> Self {
        self.is_streaming = is_streaming;
        self
    }

    fn format_cost(cost: f64) -> String {
        if cost < 0.01 {
            format!("${:.3}", cost)
        } else {
            format!("${:.2}", cost)
        }
    }

    fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    fn create_progress_bar(percentage: f64, length: usize) -> String {
        let filled = ((percentage / 100.0) * length as f64).round() as usize;
        let empty = length.saturating_sub(filled);
        format!("{}{}", "▓".repeat(filled), "░".repeat(empty))
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // THEME-AWARE COLOR HELPERS
    // ═══════════════════════════════════════════════════════════════════════════

    fn color_cyan(&self) -> Color {
        self.theme.map(|t| t.highlight).unwrap_or(COLOR_CYAN)
    }

    fn color_muted(&self) -> Color {
        self.theme.map(|t| t.text_muted).unwrap_or(COLOR_GRAY)
    }

    fn color_gold(&self) -> Color {
        self.theme.map(|t| t.status_running).unwrap_or(COLOR_GOLD)
    }

    fn color_lime(&self) -> Color {
        self.theme.map(|t| t.status_success).unwrap_or(COLOR_LIME)
    }

    fn color_coral(&self) -> Color {
        self.theme.map(|t| t.status_failed).unwrap_or(COLOR_CORAL)
    }

    fn color_pink(&self) -> Color {
        self.theme.map(|t| t.highlight).unwrap_or(COLOR_PINK)
    }

    fn color_turquoise(&self) -> Color {
        self.theme.map(|t| t.highlight).unwrap_or(COLOR_TURQUOISE)
    }

    fn color_orange(&self) -> Color {
        self.theme.map(|t| t.status_running).unwrap_or(COLOR_ORANGE)
    }

    #[allow(clippy::vec_init_then_push)]
    fn render_line1(&self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![];

        // Extract colors once
        let cyan = self.color_cyan();
        let muted = self.color_muted();
        let lime = self.color_lime();

        // Provider icon and model name
        spans.push(Span::raw(self.provider.icon()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            self.model,
            Style::default().fg(cyan).add_modifier(Modifier::BOLD),
        ));

        // Separator
        spans.push(Span::styled(" │ ", Style::default().fg(muted)));

        // Mode indicator
        spans.push(Span::raw(self.mode.icon()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            self.mode.label(),
            Style::default().fg(self.mode.color_with_theme(self.theme)),
        ));

        // Thinking indicator
        if self.thinking_enabled || self.mode == ChatModeIndicator::Agent {
            spans.push(Span::styled(" │ ", Style::default().fg(muted)));
            spans.push(Span::raw("🧠 "));
            spans.push(Span::styled("Thinking: ", Style::default().fg(muted)));
            let (status, color) = if self.thinking_enabled {
                ("ON", lime)
            } else {
                ("OFF", muted)
            };
            spans.push(Span::styled(
                status,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        }

        // Streaming indicator
        if self.is_streaming {
            spans.push(Span::styled(" │ ", Style::default().fg(muted)));
            spans.push(Span::styled(
                "●",
                Style::default().fg(lime).add_modifier(Modifier::SLOW_BLINK),
            ));
            spans.push(Span::styled(" Streaming...", Style::default().fg(lime)));
        }

        let line = Line::from(spans);
        let para = Paragraph::new(line);
        para.render(area, buf);
    }

    #[allow(clippy::vec_init_then_push)]
    fn render_line2(&self, area: Rect, buf: &mut Buffer) {
        let mut spans = vec![];

        // Extract colors once
        let gold = self.color_gold();
        let muted = self.color_muted();
        let pink = self.color_pink();
        let lime = self.color_lime();
        let coral = self.color_coral();
        let turquoise = self.color_turquoise();
        let orange = self.color_orange();

        // Cost
        spans.push(Span::raw("💰 "));
        spans.push(Span::styled(
            Self::format_cost(self.metrics.cost_usd),
            Style::default().fg(gold),
        ));

        // Separator
        spans.push(Span::styled(" │ ", Style::default().fg(muted)));

        // Tokens
        spans.push(Span::raw("🔢 "));
        let total = format_number_compact(self.metrics.total_tokens());
        let max = format_number_compact(self.metrics.max_tokens);
        let pct = self.metrics.context_percentage();
        spans.push(Span::styled(
            total,
            Style::default().fg(pink).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("/{}", max),
            Style::default().fg(muted).add_modifier(Modifier::ITALIC),
        ));
        spans.push(Span::styled(
            format!(" ({:.1}%) ", pct),
            Style::default().fg(muted),
        ));

        // Progress bar
        let progress = Self::create_progress_bar(pct, 10);
        let bar_color = if pct < 50.0 {
            lime
        } else if pct < 75.0 {
            gold
        } else if pct < 90.0 {
            orange
        } else {
            coral
        };
        spans.push(Span::styled(progress, Style::default().fg(bar_color)));

        // Separator
        spans.push(Span::styled(" │ ", Style::default().fg(muted)));

        // Duration
        spans.push(Span::raw("⏱ "));
        spans.push(Span::styled(
            Self::format_duration(self.metrics.session_duration()),
            Style::default().fg(turquoise),
        ));

        // Lines changed (if any)
        if self.metrics.lines_added > 0 || self.metrics.lines_removed > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(muted)));
            spans.push(Span::raw("📝 "));
            spans.push(Span::styled(
                format!("+{}", self.metrics.lines_added),
                Style::default().fg(lime),
            ));
            spans.push(Span::styled(
                format!("/-{}", self.metrics.lines_removed),
                Style::default().fg(coral),
            ));
        }

        // MCP status
        let mcp_connected = self.metrics.mcp_connected_count();
        let mcp_total = self.metrics.mcp_servers.len();
        if mcp_total > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(muted)));
            spans.push(Span::raw("MCP:"));
            let mcp_color = if mcp_connected == mcp_total {
                lime
            } else if mcp_connected > 0 {
                gold
            } else {
                muted
            };
            spans.push(Span::styled(
                if mcp_connected > 0 { "●" } else { "○" },
                Style::default().fg(mcp_color),
            ));
            if mcp_total > 1 {
                spans.push(Span::styled(
                    format!(" {}/{}", mcp_connected, mcp_total),
                    Style::default().fg(muted),
                ));
            }
        }

        let line = Line::from(spans);
        let para = Paragraph::new(line);
        para.render(area, buf);
    }
}

impl Widget for ProStatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height < 2 {
            return;
        }

        // Split into two lines
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);

        self.render_line1(chunks[0], buf);
        self.render_line2(chunks[1], buf);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_cost() {
        assert_eq!(ProStatusBar::format_cost(0.001), "$0.001");
        assert_eq!(ProStatusBar::format_cost(0.05), "$0.05");
        assert_eq!(ProStatusBar::format_cost(1.50), "$1.50");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_number_compact(500), "500");
        assert_eq!(format_number_compact(1500), "1.5K");
        assert_eq!(format_number_compact(1_500_000), "1.5M");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(
            ProStatusBar::format_duration(Duration::from_secs(45)),
            "45s"
        );
        assert_eq!(
            ProStatusBar::format_duration(Duration::from_secs(125)),
            "2m 5s"
        );
        assert_eq!(
            ProStatusBar::format_duration(Duration::from_secs(3700)),
            "1h 1m"
        );
    }

    #[test]
    fn test_progress_bar() {
        assert_eq!(ProStatusBar::create_progress_bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(ProStatusBar::create_progress_bar(50.0, 10), "▓▓▓▓▓░░░░░");
        assert_eq!(ProStatusBar::create_progress_bar(100.0, 10), "▓▓▓▓▓▓▓▓▓▓");
    }

    #[test]
    fn test_session_metrics_percentage() {
        let mut metrics = SessionMetrics::new();
        metrics.input_tokens = 50_000;
        metrics.output_tokens = 50_000;
        metrics.max_tokens = 200_000;
        assert_eq!(metrics.context_percentage(), 50.0);
    }

    #[test]
    fn test_chat_mode_indicator() {
        assert_eq!(ChatModeIndicator::Infer.icon(), "⚡");
        assert_eq!(ChatModeIndicator::Agent.icon(), "🐔");
    }
}
