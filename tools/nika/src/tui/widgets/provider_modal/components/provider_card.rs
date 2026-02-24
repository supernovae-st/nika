//! Rich provider card with status, sparkline, and model info
//!
//! v0.8.9: Added latency sparkline visualization
//! v0.8.91: Fixed overflow - truncate error messages to fit card width

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Widget},
};

use super::super::state::ConnectionStatus;

/// Maximum width for status text to prevent overflow (chars)
const MAX_STATUS_WIDTH: usize = 14;

/// Truncate text to max_len with ellipsis if needed
fn truncate_status(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

/// Sparkline characters for latency visualization (8 levels)
/// Each bar represents relative latency: lower = faster = better
const SPARKLINE_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Convert latency values to sparkline string
/// Values are normalized to the max in the array, lower = better (shorter bar)
pub fn latency_to_sparkline(history: &[u64]) -> String {
    if history.is_empty() {
        return String::new();
    }

    let max = history.iter().copied().max().unwrap_or(1).max(1);
    let min = history.iter().copied().min().unwrap_or(0);
    let range = (max - min).max(1);

    history
        .iter()
        .map(|&val| {
            // Normalize to 0-7 range (8 levels)
            let normalized = ((val.saturating_sub(min)) * 7) / range;
            let idx = (normalized as usize).min(7);
            SPARKLINE_CHARS[idx]
        })
        .collect()
}

/// Visual style for provider cards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardStyle {
    #[default]
    Normal,
    /// Currently selected in UI (cursor is on this card)
    Selected,
    /// Disabled (e.g., no API key)
    Disabled,
    /// Active provider: currently in use for inference
    Active,
    /// Both selected and active
    SelectedActive,
}

impl CardStyle {
    pub fn border_color(&self) -> Color {
        match self {
            Self::Selected => Color::Rgb(99, 102, 241),      // indigo
            Self::Active => Color::Rgb(34, 197, 94),         // green (in use!)
            Self::SelectedActive => Color::Rgb(34, 197, 94), // green + bold
            Self::Normal => Color::Rgb(55, 65, 81),          // gray
            Self::Disabled => Color::Rgb(31, 41, 55),        // dark gray
        }
    }

    pub fn bg_color(&self) -> Color {
        match self {
            Self::Selected => Color::Rgb(30, 41, 59),       // slate-800
            Self::Active => Color::Rgb(22, 78, 48),         // green tint
            Self::SelectedActive => Color::Rgb(30, 60, 50), // selected + green
            Self::Normal => Color::Rgb(17, 24, 39),         // gray-900
            Self::Disabled => Color::Rgb(17, 24, 39),
        }
    }

    /// Whether this card shows the active indicator
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::SelectedActive)
    }
}

/// Provider card showing rich status info
pub struct ProviderCard<'a> {
    icon: &'a str,
    name: &'a str,
    model: &'a str,
    status: &'a ConnectionStatus,
    features: Vec<&'a str>,
    context_window: u32,
    style: CardStyle,
    /// Animated indicator for active state (cycling ASCII chars)
    active_indicator: &'a str,
    /// v0.8.9: Latency history for sparkline (up to 10 samples)
    latency_history: &'a [u64],
    /// v0.8.9: Optional verification entry for Matrix effect
    verify_entry: Option<&'a super::verification_effect::VerifyEntry>,
}

impl<'a> ProviderCard<'a> {
    pub fn new(icon: &'a str, name: &'a str, model: &'a str, status: &'a ConnectionStatus) -> Self {
        Self {
            icon,
            name,
            model,
            status,
            features: vec![],
            context_window: 0,
            style: CardStyle::Normal,
            active_indicator: "★",
            latency_history: &[],
            verify_entry: None,
        }
    }

    pub fn features(mut self, features: Vec<&'a str>) -> Self {
        self.features = features;
        self
    }

    pub fn context_window(mut self, size: u32) -> Self {
        self.context_window = size;
        self
    }

    pub fn style(mut self, style: CardStyle) -> Self {
        self.style = style;
        self
    }

    /// Set animated active indicator (cycling ASCII chars)
    pub fn active_indicator(mut self, indicator: &'a str) -> Self {
        self.active_indicator = indicator;
        self
    }

    /// v0.8.9: Set latency history for sparkline visualization
    pub fn latency_history(mut self, history: &'a [u64]) -> Self {
        self.latency_history = history;
        self
    }

    /// v0.8.9: Set verification entry for Matrix effect during Checking
    pub fn verify_entry(mut self, entry: &'a super::verification_effect::VerifyEntry) -> Self {
        self.verify_entry = Some(entry);
        self
    }
}

impl Widget for ProviderCard<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        // Build title with optional animated "IN USE" badge
        let title = if self.style.is_active() {
            format!(
                " {} {} {} IN USE ",
                self.icon, self.name, self.active_indicator
            )
        } else {
            format!(" {} {} ", self.icon, self.name)
        };

        let title_color = if self.style.is_active() {
            Color::Rgb(34, 197, 94) // Green for active
        } else {
            Color::Rgb(229, 231, 235) // Default white
        };

        // Card border with rounded corners
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.style.border_color()))
            .style(Style::default().bg(self.style.bg_color()))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(title_color)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        // Row 1: Model name
        let model_style = Style::default().fg(Color::Rgb(156, 163, 175));
        buf.set_string(inner.x + 1, inner.y, self.model, model_style);

        // Status on the right - v0.8.9: Use Matrix effect during verification
        if let Some(entry) = &self.verify_entry {
            // Show Matrix effect when not complete
            if !entry.is_complete() {
                // Render status indicator
                let indicator = entry.status.indicator();
                let indicator_x = inner.right().saturating_sub(15);
                buf.set_string(
                    indicator_x,
                    inner.y,
                    indicator,
                    Style::default().fg(entry.status.color()),
                );
                // Render Matrix-style scrambled text
                let chars = entry.render_text();
                let mut x = indicator_x + 2;
                for (ch, color) in chars.iter().take(10) {
                    if x >= inner.right() {
                        break;
                    }
                    buf.set_string(x, inner.y, ch.to_string(), Style::default().fg(*color));
                    x += 1;
                }
            } else {
                // Entry complete - show normal status
                // v0.8.91: Truncate to prevent overflow
                let status_text = truncate_status(&self.status.display_text(), MAX_STATUS_WIDTH);
                let status_color = entry.status.color();
                let status_x = inner.right().saturating_sub(status_text.len() as u16 + 1);
                buf.set_string(
                    status_x,
                    inner.y,
                    &status_text,
                    Style::default().fg(status_color),
                );
            }
        } else {
            // No verification entry - render normal status
            // v0.8.91: Truncate to prevent overflow
            let status_text = truncate_status(&self.status.display_text(), MAX_STATUS_WIDTH);
            let status_color = match self.status {
                ConnectionStatus::Connected { .. } => Color::Rgb(34, 197, 94),
                ConnectionStatus::Failed { .. } => Color::Rgb(239, 68, 68),
                ConnectionStatus::Checking => Color::Rgb(59, 130, 246),
                _ => Color::Rgb(107, 114, 128),
            };
            let status_x = inner.right().saturating_sub(status_text.len() as u16 + 1);
            buf.set_string(
                status_x,
                inner.y,
                &status_text,
                Style::default().fg(status_color),
            );
        }

        // Row 2: Features + Sparkline + Context window
        if inner.height >= 2 {
            let features_str = self.features.join(" ");
            buf.set_string(
                inner.x + 1,
                inner.y + 1,
                &features_str,
                Style::default().fg(Color::Rgb(59, 130, 246)),
            );

            // v0.8.9: Sparkline visualization (center)
            if !self.latency_history.is_empty() {
                let sparkline = latency_to_sparkline(self.latency_history);
                // Calculate center position between features and context window
                let features_end = inner.x + 1 + features_str.len() as u16 + 1;
                let sparkline_len = sparkline.chars().count() as u16;
                // Place sparkline in center of remaining space
                let remaining_width = inner.width.saturating_sub(features_end - inner.x);
                let sparkline_x = if remaining_width > sparkline_len + 6 {
                    features_end + (remaining_width - sparkline_len - 6) / 2
                } else {
                    features_end
                };
                // Gradient color from green (fast) to red (slow)
                let sparkline_color = if self.latency_history.iter().all(|&l| l < 200) {
                    Color::Rgb(34, 197, 94) // Green - all fast
                } else if self.latency_history.iter().any(|&l| l > 500) {
                    Color::Rgb(239, 68, 68) // Red - some slow
                } else {
                    Color::Rgb(251, 191, 36) // Yellow - medium
                };
                buf.set_string(
                    sparkline_x,
                    inner.y + 1,
                    &sparkline,
                    Style::default().fg(sparkline_color),
                );
            }

            if self.context_window > 0 {
                let ctx_str = format!("{}K", self.context_window / 1000);
                let ctx_x = inner.right().saturating_sub(ctx_str.len() as u16 + 1);
                buf.set_string(
                    ctx_x,
                    inner.y + 1,
                    &ctx_str,
                    Style::default().fg(Color::Rgb(107, 114, 128)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_card_style_colors() {
        assert_eq!(CardStyle::Selected.border_color(), Color::Rgb(99, 102, 241));
        assert_eq!(CardStyle::Normal.border_color(), Color::Rgb(55, 65, 81));
        assert_eq!(CardStyle::Disabled.border_color(), Color::Rgb(31, 41, 55));
    }

    #[test]
    fn test_card_renders_without_panic() {
        let status = super::super::super::state::ConnectionStatus::Connected { latency_ms: 100 };
        let card = ProviderCard::new("🍊", "Claude", "claude-sonnet-4", &status);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        card.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Should render without panic
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
    }

    #[test]
    fn test_card_renders_in_small_area() {
        let status = super::super::super::state::ConnectionStatus::Unknown;
        let card = ProviderCard::new("🍊", "Claude", "claude-sonnet-4", &status);

        // Should not panic with small area
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        card.render(Rect::new(0, 0, 10, 2), &mut buf);
    }

    // v0.8.9: Sparkline tests
    #[test]
    fn test_sparkline_empty_history() {
        let result = latency_to_sparkline(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_sparkline_single_value() {
        let result = latency_to_sparkline(&[100]);
        assert_eq!(result.chars().count(), 1);
        // Single value should be minimum bar (since min == max)
        assert_eq!(result.chars().next().unwrap(), '▁');
    }

    #[test]
    fn test_sparkline_uniform_values() {
        let result = latency_to_sparkline(&[100, 100, 100, 100]);
        // All same values should produce same bar height (min bar)
        assert!(result.chars().all(|c| c == '▁'));
    }

    #[test]
    fn test_sparkline_increasing_values() {
        let result = latency_to_sparkline(&[50, 100, 150, 200]);
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars.len(), 4);
        // Should show increasing heights
        assert_eq!(chars[0], '▁'); // lowest
        assert_eq!(chars[3], '█'); // highest
    }

    #[test]
    fn test_sparkline_varying_values() {
        let result = latency_to_sparkline(&[100, 200, 150, 300, 50]);
        let chars: Vec<char> = result.chars().collect();
        assert_eq!(chars.len(), 5);
        // min=50, max=300 → normalized
        // 50 → 0, 300 → 7, others in between
        assert_eq!(chars[4], '▁'); // 50 is min → lowest bar
        assert_eq!(chars[3], '█'); // 300 is max → highest bar
    }

    #[test]
    fn test_sparkline_renders_correct_chars() {
        // Test that all sparkline chars are in expected set
        let result = latency_to_sparkline(&[0, 50, 100, 150, 200, 250, 300, 350]);
        for c in result.chars() {
            assert!(SPARKLINE_CHARS.contains(&c), "Unexpected char: {}", c);
        }
    }

    #[test]
    fn test_card_with_latency_history() {
        let status = ConnectionStatus::Connected { latency_ms: 150 };
        let history = [100, 120, 150, 130, 140];
        let card = ProviderCard::new("🍊", "Claude", "claude-sonnet-4", &status)
            .latency_history(&history);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        card.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Should render without panic and include sparkline chars
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
        // Check that at least one sparkline char is present
        let has_sparkline = content.chars().any(|c| SPARKLINE_CHARS.contains(&c));
        assert!(has_sparkline, "Should render sparkline");
    }

    #[test]
    fn test_card_without_latency_history() {
        let status = ConnectionStatus::Connected { latency_ms: 150 };
        let card = ProviderCard::new("🍊", "Claude", "claude-sonnet-4", &status);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        card.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Should render without sparkline chars (except by coincidence)
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
    }

    // v0.8.91: Truncation tests
    #[test]
    fn test_truncate_status_short_text() {
        let result = truncate_status("● 100ms", 14);
        assert_eq!(result, "● 100ms");
    }

    #[test]
    fn test_truncate_status_exact_length() {
        let result = truncate_status("✗ Not config", 14);
        assert_eq!(result, "✗ Not config");
    }

    #[test]
    fn test_truncate_status_long_error() {
        let result = truncate_status("✗ Connection refused: HTTP 500", 14);
        assert_eq!(result.chars().count(), 14);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_status_very_long_error() {
        let result = truncate_status("✗ Authentication failed: Invalid API key", 14);
        assert_eq!(result.chars().count(), 14);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_card_with_long_error_no_overflow() {
        let status = ConnectionStatus::Failed {
            error: "Connection refused: HTTP 500 Internal Server Error with very long message".into(),
        };
        let card = ProviderCard::new("🍊", "Claude", "claude-sonnet-4", &status);

        // Small card area to test overflow protection
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 4));
        card.render(Rect::new(0, 0, 30, 4), &mut buf);

        // Should render without panic and stay within bounds
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
    }
}
