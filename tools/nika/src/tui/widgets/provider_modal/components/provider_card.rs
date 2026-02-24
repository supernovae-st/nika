//! Rich provider card with status, sparkline, and model info

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Widget},
};

use super::super::state::ConnectionStatus;

/// Visual style for provider cards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardStyle {
    #[default]
    Normal,
    Selected,
    Disabled,
}

impl CardStyle {
    pub fn border_color(&self) -> Color {
        match self {
            Self::Selected => Color::Rgb(99, 102, 241), // indigo
            Self::Normal => Color::Rgb(55, 65, 81),     // gray
            Self::Disabled => Color::Rgb(31, 41, 55),   // dark gray
        }
    }

    pub fn bg_color(&self) -> Color {
        match self {
            Self::Selected => Color::Rgb(30, 41, 59), // slate-800
            Self::Normal => Color::Rgb(17, 24, 39),   // gray-900
            Self::Disabled => Color::Rgb(17, 24, 39),
        }
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
}

impl Widget for ProviderCard<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        // Card border with rounded corners
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.style.border_color()))
            .style(Style::default().bg(self.style.bg_color()))
            .title(Span::styled(
                format!(" {} {} ", self.icon, self.name),
                Style::default()
                    .fg(Color::Rgb(229, 231, 235))
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        // Row 1: Model name
        let model_style = Style::default().fg(Color::Rgb(156, 163, 175));
        buf.set_string(inner.x + 1, inner.y, self.model, model_style);

        // Status on the right
        let status_text = self.status.display_text();
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

        // Row 2: Features + Context window
        if inner.height >= 2 {
            let features_str = self.features.join(" ");
            buf.set_string(
                inner.x + 1,
                inner.y + 1,
                &features_str,
                Style::default().fg(Color::Rgb(59, 130, 246)),
            );

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
        let card = ProviderCard::new("🧠", "Claude", "claude-sonnet-4", &status);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        card.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Should render without panic
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
    }

    #[test]
    fn test_card_renders_in_small_area() {
        let status = super::super::super::state::ConnectionStatus::Unknown;
        let card = ProviderCard::new("🧠", "Claude", "claude-sonnet-4", &status);

        // Should not panic with small area
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        card.render(Rect::new(0, 0, 10, 2), &mut buf);
    }
}
