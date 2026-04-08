// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Footer Stats Bar for Provider Modal
//!
//! WOW Cosmic Edition
//! Displays session metrics: provider count, tokens used, MCP status

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Status: all connected (success)
const COLOR_STATUS_SUCCESS: Color = Color::Rgb(34, 197, 94); // Emerald-500

/// Status: partial (warning)
const COLOR_STATUS_PARTIAL: Color = Color::Rgb(251, 191, 36); // Amber-400

/// Status: none connected (error)
const COLOR_STATUS_ERROR: Color = Color::Rgb(239, 68, 68); // Red-500

/// Token usage indicator
const COLOR_TOKENS: Color = Color::Rgb(99, 102, 241); // Indigo-500

/// MCP indicator
const COLOR_MCP: Color = Color::Rgb(59, 130, 246); // Blue-500

/// Separator
const COLOR_SEPARATOR: Color = Color::Rgb(75, 85, 99); // Gray-600

/// Session statistics for footer display
#[derive(Debug, Clone, Default)]
pub struct SessionStats {
    /// Number of connected providers
    pub connected_providers: usize,
    /// Total number of providers
    pub total_providers: usize,
    /// Total tokens used in session
    pub tokens_used: u64,
    /// Number of active MCP connections
    pub mcp_connections: usize,
    /// Average latency across connected providers (ms)
    pub avg_latency_ms: Option<u64>,
}

impl SessionStats {
    /// Format tokens with K/M suffix for display
    pub fn format_tokens(&self) -> String {
        if self.tokens_used >= 1_000_000 {
            format!("{:.1}M", self.tokens_used as f64 / 1_000_000.0)
        } else if self.tokens_used >= 1_000 {
            format!("{:.1}K", self.tokens_used as f64 / 1_000.0)
        } else {
            self.tokens_used.to_string()
        }
    }
}

/// Footer stats bar widget
pub struct FooterStatsBar<'a> {
    stats: &'a SessionStats,
}

impl<'a> FooterStatsBar<'a> {
    pub fn new(stats: &'a SessionStats) -> Self {
        Self { stats }
    }
}

impl Widget for FooterStatsBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 {
            return;
        }

        // Build stats string: "✓ 3/6 │ 🔤 12.5K │ 🔌 2 MCP │ ⏱ 150ms"
        let mut parts = Vec::new();

        // Provider status
        let provider_color = if self.stats.connected_providers == self.stats.total_providers {
            COLOR_STATUS_SUCCESS
        } else if self.stats.connected_providers > 0 {
            COLOR_STATUS_PARTIAL
        } else {
            COLOR_STATUS_ERROR
        };
        parts.push((
            format!(
                "✓ {}/{}",
                self.stats.connected_providers, self.stats.total_providers
            ),
            provider_color,
        ));

        // Token usage
        if self.stats.tokens_used > 0 {
            let tokens_str = format!("🔤 {}", self.stats.format_tokens());
            parts.push((tokens_str, COLOR_TOKENS));
        }

        // MCP connections
        if self.stats.mcp_connections > 0 {
            let mcp_str = format!("🔌 {} MCP", self.stats.mcp_connections);
            parts.push((mcp_str, COLOR_MCP));
        }

        // Average latency
        if let Some(avg) = self.stats.avg_latency_ms {
            let latency_color = if avg < 200 {
                COLOR_STATUS_SUCCESS
            } else if avg < 500 {
                COLOR_STATUS_PARTIAL
            } else {
                COLOR_STATUS_ERROR
            };
            parts.push((format!("⏱ {}ms", avg), latency_color));
        }

        // Render from right side
        let separator = " │ ";
        let separator_style = Style::default().fg(COLOR_SEPARATOR);

        // Calculate total width needed
        let total_width: usize = parts.iter().map(|(s, _)| s.chars().count()).sum::<usize>()
            + (parts.len().saturating_sub(1) * separator.len());

        if total_width as u16 > area.width {
            return;
        }

        let start_x = area.right().saturating_sub(total_width as u16 + 1);
        let mut x = start_x;

        for (i, (text, color)) in parts.iter().enumerate() {
            if i > 0 {
                buf.set_string(x, area.y, separator, separator_style);
                x += separator.len() as u16;
            }
            buf.set_string(x, area.y, text, Style::default().fg(*color));
            x += text.chars().count() as u16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens_small() {
        let stats = SessionStats {
            tokens_used: 500,
            ..Default::default()
        };
        assert_eq!(stats.format_tokens(), "500");
    }

    #[test]
    fn test_format_tokens_thousands() {
        let stats = SessionStats {
            tokens_used: 12_500,
            ..Default::default()
        };
        assert_eq!(stats.format_tokens(), "12.5K");
    }

    #[test]
    fn test_format_tokens_millions() {
        let stats = SessionStats {
            tokens_used: 1_500_000,
            ..Default::default()
        };
        assert_eq!(stats.format_tokens(), "1.5M");
    }

    #[test]
    fn test_session_stats_default() {
        let stats = SessionStats::default();
        assert_eq!(stats.connected_providers, 0);
        assert_eq!(stats.total_providers, 0);
        assert_eq!(stats.tokens_used, 0);
        assert_eq!(stats.mcp_connections, 0);
        assert!(stats.avg_latency_ms.is_none());
    }

    #[test]
    fn test_footer_stats_renders_without_panic() {
        let stats = SessionStats {
            connected_providers: 3,
            total_providers: 7,
            tokens_used: 15_000,
            mcp_connections: 2,
            avg_latency_ms: Some(150),
        };
        let bar = FooterStatsBar::new(&stats);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        bar.render(Rect::new(0, 0, 80, 1), &mut buf);

        // Should have rendered provider count
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("3/7"));
    }

    #[test]
    fn test_footer_stats_small_area_no_panic() {
        let stats = SessionStats::default();
        let bar = FooterStatsBar::new(&stats);

        // Should not panic on small area
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        bar.render(Rect::new(0, 0, 10, 1), &mut buf);
    }

    #[test]
    fn test_footer_stats_empty_tokens_not_shown() {
        let stats = SessionStats {
            connected_providers: 2,
            total_providers: 7,
            tokens_used: 0,
            mcp_connections: 0,
            avg_latency_ms: None,
        };
        let bar = FooterStatsBar::new(&stats);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        bar.render(Rect::new(0, 0, 80, 1), &mut buf);

        // Should have provider count but no token indicator
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("2/7"));
        assert!(!content.contains("🔤"));
    }

    #[test]
    fn test_footer_stats_all_connected() {
        let stats = SessionStats {
            connected_providers: 7,
            total_providers: 7,
            ..Default::default()
        };
        let bar = FooterStatsBar::new(&stats);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 1));
        bar.render(Rect::new(0, 0, 80, 1), &mut buf);

        // Should render without panic
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("7/7"));
    }
}
