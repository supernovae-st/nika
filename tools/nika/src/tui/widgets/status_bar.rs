//! Unified status bar widget showing contextual keybindings
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ [Enter] Send  [Up/Down] History     Claude | 1.2k tokens | MCP: 2 connected │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui::keybindings::{format_key, keybindings_for_context, KeyCategory, Keybinding};
use crate::tui::mode::InputMode;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Key hint for status bar
#[derive(Debug, Clone)]
pub struct KeyHint {
    pub key: &'static str,
    pub action: &'static str,
}

impl KeyHint {
    pub const fn new(key: &'static str, action: &'static str) -> Self {
        Self { key, action }
    }
}

/// LLM Provider indicator (v0.7.0: 6 providers)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Provider {
    #[default]
    None,
    Claude,
    OpenAI,
    Mistral,
    Ollama,
    Groq,
    DeepSeek,
    Mock,
}

impl Provider {
    /// Get provider icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::None => "  ",
            Self::Claude => "🧠",   // Brain for Claude
            Self::OpenAI => "🤖",   // Robot for OpenAI
            Self::Mistral => "🌬️",  // Wind for Mistral (mistral wind)
            Self::Ollama => "🦙",   // Llama for Ollama
            Self::Groq => "⚡",     // Lightning for Groq (fast inference)
            Self::DeepSeek => "🔍", // Magnifying glass for DeepSeek
            Self::Mock => "🧪",     // Test tube for mock
        }
    }

    /// Get provider display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::None => "---",
            Self::Claude => "Claude",
            Self::OpenAI => "OpenAI",
            Self::Mistral => "Mistral",
            Self::Ollama => "Ollama",
            Self::Groq => "Groq",
            Self::DeepSeek => "DeepSeek",
            Self::Mock => "Mock",
        }
    }
}

/// MCP Connection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Error,
}

impl ConnectionStatus {
    /// Get status icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Disconnected => "○",
            Self::Connecting => "◔",
            Self::Connected => "●",
            Self::Error => "⊗",
        }
    }
}

/// Metrics to display on the right side of status bar
#[derive(Debug, Clone, Default)]
pub struct StatusMetrics {
    /// Current LLM provider
    pub provider: Provider,
    /// Total tokens used in session
    pub tokens: Option<u64>,
    /// Number of connected MCP servers
    pub mcp_connected: usize,
    /// Total MCP servers configured
    pub mcp_total: usize,
    /// Connection status
    pub connection: ConnectionStatus,
}

impl StatusMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn provider(mut self, provider: Provider) -> Self {
        self.provider = provider;
        self
    }

    pub fn tokens(mut self, tokens: u64) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn mcp(mut self, connected: usize, total: usize) -> Self {
        self.mcp_connected = connected;
        self.mcp_total = total;
        self
    }

    pub fn connection(mut self, status: ConnectionStatus) -> Self {
        self.connection = status;
        self
    }

    /// Format token count for display
    fn format_tokens(&self) -> Option<String> {
        self.tokens.map(|t| {
            if t >= 1_000_000 {
                format!("{:.1}M", t as f64 / 1_000_000.0)
            } else if t >= 1_000 {
                format!("{:.1}k", t as f64 / 1_000.0)
            } else {
                format!("{}", t)
            }
        })
    }
}

/// Status bar configuration
pub struct StatusBar<'a> {
    /// Current view (determines which hints to show)
    pub view: TuiView,
    /// Optional custom hints (overrides defaults)
    pub hints: Option<Vec<KeyHint>>,
    /// Theme for colors
    pub theme: &'a Theme,
    /// Metrics to display on the right
    pub metrics: Option<StatusMetrics>,
    /// Current input mode (Normal, Insert, Command, Search)
    pub input_mode: Option<InputMode>,
    /// Custom status text from the view's status_line() method
    pub custom_text: Option<String>,
}

impl<'a> StatusBar<'a> {
    pub fn new(view: TuiView, theme: &'a Theme) -> Self {
        Self {
            view,
            hints: None,
            theme,
            metrics: None,
            input_mode: None,
            custom_text: None,
        }
    }

    pub fn hints(mut self, hints: Vec<KeyHint>) -> Self {
        self.hints = Some(hints);
        self
    }

    pub fn metrics(mut self, metrics: StatusMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn mode(mut self, mode: InputMode) -> Self {
        self.input_mode = Some(mode);
        self
    }

    /// Set custom status text from the view's status_line() method
    pub fn custom_text(mut self, text: String) -> Self {
        if !text.is_empty() {
            self.custom_text = Some(text);
        }
        self
    }

    fn default_hints(&self) -> Vec<KeyHint> {
        // If input_mode is set, use keybindings_for_context for dynamic hints
        if let Some(mode) = self.input_mode {
            return self.hints_from_keybindings(keybindings_for_context(self.view, mode));
        }

        // Fallback to static hints (for backwards compatibility)
        match self.view {
            TuiView::Chat => vec![
                KeyHint::new("Enter", "Send"),
                KeyHint::new("Up/Down", "History"),
                KeyHint::new("Tab", "Views"),
                KeyHint::new("Ctrl+L", "Clear"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Home => vec![
                KeyHint::new("Up/Down", "Navigate"),
                KeyHint::new("Enter", "Run"),
                KeyHint::new("e", "Edit"),
                KeyHint::new("n", "New"),
                KeyHint::new("/", "Search"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Studio => vec![
                KeyHint::new("i", "Insert"),
                KeyHint::new("Esc", "Normal"),
                KeyHint::new("F5", "Run"),
                KeyHint::new("Ctrl+S", "Save"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Back"),
            ],
            TuiView::Monitor => vec![
                KeyHint::new("1-4", "Focus"),
                KeyHint::new("Tab", "Cycle"),
                KeyHint::new("Space", "Pause"),
                KeyHint::new("r", "Restart"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Stop"),
            ],
        }
    }

    /// Convert keybindings to key hints for display
    ///
    /// Selects the most relevant keybindings based on priority:
    /// - View-specific actions first
    /// - Mode switching actions
    /// - Global actions (quit, help)
    fn hints_from_keybindings(&self, keybindings: Vec<Keybinding>) -> Vec<KeyHint> {
        // Prioritize categories: view-specific > mode > scroll > global
        let priority = |kb: &Keybinding| -> u8 {
            match kb.category {
                KeyCategory::Chat | KeyCategory::Monitor => 0,
                KeyCategory::Action => 1,
                KeyCategory::Mode => 2,
                KeyCategory::Scroll => 3,
                KeyCategory::PanelNav => 4,
                KeyCategory::ViewNav => 5,
                KeyCategory::Global => 6,
            }
        };

        // Sort by priority and take top 6 hints (to fit status bar)
        let mut sorted: Vec<_> = keybindings.iter().collect();
        sorted.sort_by_key(|kb| priority(kb));

        sorted
            .into_iter()
            .take(6)
            .map(|kb| {
                // Convert Keybinding to KeyHint using format_key
                let key_str = format_key(kb.code, kb.modifiers);
                // We need to leak the string to get a static lifetime - this is OK
                // because hints are recreated each frame anyway
                let key: &'static str = Box::leak(key_str.into_boxed_str());
                KeyHint {
                    key,
                    action: kb.description,
                }
            })
            .collect()
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Get default hints before potentially moving self.hints
        let default = self.default_hints();
        let hints = self.hints.unwrap_or(default);

        // Build left side (input mode indicator + key hints)
        let mut left_spans = vec![Span::raw(" ")];

        // Add input mode indicator if set
        if let Some(mode) = self.input_mode {
            let (mode_char, mode_color) = match mode {
                InputMode::Normal => ('N', self.theme.status_success), // Green for Normal
                InputMode::Insert => ('I', self.theme.status_running), // Amber for Insert
                InputMode::Search => ('/', self.theme.highlight),      // Cyan for Search
            };
            left_spans.push(Span::styled(
                format!("[{}]", mode_char),
                Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
            ));
            left_spans.push(Span::raw(" "));
        }

        // Add custom status text from view (if present)
        if let Some(ref text) = self.custom_text {
            left_spans.push(Span::styled(
                text.clone(),
                Style::default().fg(self.theme.text_primary),
            ));
            left_spans.push(Span::styled(
                " │ ",
                Style::default().fg(self.theme.text_muted),
            ));
        }

        for (i, hint) in hints.iter().enumerate() {
            if i > 0 || self.input_mode.is_some() || self.custom_text.is_some() {
                left_spans.push(Span::raw("  "));
            }
            left_spans.push(Span::styled(
                format!("[{}]", hint.key),
                Style::default()
                    .fg(self.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ));
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled(
                hint.action,
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        // Build right side (metrics) if available
        let mut right_spans: Vec<Span> = Vec::new();

        if let Some(ref metrics) = self.metrics {
            // Provider indicator
            if metrics.provider != Provider::None {
                right_spans.push(Span::raw(metrics.provider.icon()));
                right_spans.push(Span::raw(" "));
                right_spans.push(Span::styled(
                    metrics.provider.name(),
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            // Token counter
            if let Some(token_str) = metrics.format_tokens() {
                if !right_spans.is_empty() {
                    right_spans.push(Span::styled(
                        " | ",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }
                right_spans.push(Span::styled(
                    token_str,
                    Style::default().fg(self.theme.status_running), // Amber for attention
                ));
                right_spans.push(Span::styled(
                    " tokens",
                    Style::default().fg(self.theme.text_secondary),
                ));
            }

            // MCP connection status
            if metrics.mcp_total > 0 {
                if !right_spans.is_empty() {
                    right_spans.push(Span::styled(
                        " | ",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }

                // Connection status icon with color
                let conn_color = match metrics.connection {
                    ConnectionStatus::Connected => self.theme.status_success,
                    ConnectionStatus::Connecting => self.theme.status_running,
                    ConnectionStatus::Disconnected => self.theme.text_muted,
                    ConnectionStatus::Error => self.theme.status_failed,
                };

                right_spans.push(Span::styled(
                    metrics.connection.icon(),
                    Style::default().fg(conn_color),
                ));
                right_spans.push(Span::raw(" "));
                right_spans.push(Span::styled(
                    "MCP:",
                    Style::default().fg(self.theme.text_secondary),
                ));
                right_spans.push(Span::styled(
                    format!(" {}/{}", metrics.mcp_connected, metrics.mcp_total),
                    Style::default().fg(if metrics.mcp_connected == metrics.mcp_total {
                        self.theme.status_success
                    } else {
                        self.theme.text_primary
                    }),
                ));
            }

            right_spans.push(Span::raw(" "));
        }

        // Calculate widths for layout
        let left_width: usize = left_spans.iter().map(|s| s.content.len()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.content.len()).sum();

        // Render left side
        let left_line = Line::from(left_spans);
        let left_paragraph =
            Paragraph::new(left_line).style(Style::default().bg(self.theme.background));
        left_paragraph.render(area, buf);

        // Render right side (right-aligned)
        if !right_spans.is_empty() && area.width as usize > left_width + right_width {
            let right_x = area.x + area.width - right_width as u16;
            let right_line = Line::from(right_spans);

            for (i, span) in right_line.spans.iter().enumerate() {
                let x_offset: usize = right_line.spans[..i].iter().map(|s| s.content.len()).sum();
                buf.set_string(
                    right_x + x_offset as u16,
                    area.y,
                    span.content.as_ref(),
                    span.style,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_default_hints_home() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Home, &theme);
        let hints = bar.default_hints();
        assert!(hints.iter().any(|h| h.key == "Enter" && h.action == "Run"));
        assert!(hints.iter().any(|h| h.key == "e" && h.action == "Edit"));
    }

    #[test]
    fn test_status_bar_default_hints_studio() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Studio, &theme);
        let hints = bar.default_hints();
        assert!(hints.iter().any(|h| h.key == "F5" && h.action == "Run"));
        assert!(hints
            .iter()
            .any(|h| h.key == "Ctrl+S" && h.action == "Save"));
    }

    #[test]
    fn test_status_bar_custom_hints() {
        let theme = Theme::dark();
        let custom = vec![KeyHint::new("x", "Custom")];
        let bar = StatusBar::new(TuiView::Chat, &theme).hints(custom);
        assert!(bar.hints.is_some());
        assert_eq!(bar.hints.unwrap().len(), 1);
    }

    #[test]
    fn test_provider_icons() {
        assert_eq!(Provider::Claude.icon(), "🧠");
        assert_eq!(Provider::OpenAI.icon(), "🤖");
        assert_eq!(Provider::Mistral.icon(), "🌬️");
        assert_eq!(Provider::Ollama.icon(), "🦙");
        assert_eq!(Provider::Groq.icon(), "⚡");
        assert_eq!(Provider::DeepSeek.icon(), "🔍");
        assert_eq!(Provider::Mock.icon(), "🧪");
        assert_eq!(Provider::None.icon(), "  ");
    }

    #[test]
    fn test_provider_names() {
        assert_eq!(Provider::Claude.name(), "Claude");
        assert_eq!(Provider::OpenAI.name(), "OpenAI");
        assert_eq!(Provider::Mistral.name(), "Mistral");
        assert_eq!(Provider::Ollama.name(), "Ollama");
        assert_eq!(Provider::Groq.name(), "Groq");
        assert_eq!(Provider::DeepSeek.name(), "DeepSeek");
        assert_eq!(Provider::Mock.name(), "Mock");
        assert_eq!(Provider::None.name(), "---");
    }

    #[test]
    fn test_connection_status_icons() {
        assert_eq!(ConnectionStatus::Connected.icon(), "●");
        assert_eq!(ConnectionStatus::Connecting.icon(), "◔");
        assert_eq!(ConnectionStatus::Disconnected.icon(), "○");
        assert_eq!(ConnectionStatus::Error.icon(), "⊗");
    }

    #[test]
    fn test_status_metrics_token_formatting() {
        let m1 = StatusMetrics::new().tokens(500);
        assert_eq!(m1.format_tokens(), Some("500".to_string()));

        let m2 = StatusMetrics::new().tokens(1500);
        assert_eq!(m2.format_tokens(), Some("1.5k".to_string()));

        let m3 = StatusMetrics::new().tokens(1_500_000);
        assert_eq!(m3.format_tokens(), Some("1.5M".to_string()));

        let m4 = StatusMetrics::new();
        assert_eq!(m4.format_tokens(), None);
    }

    #[test]
    fn test_status_metrics_builder() {
        let metrics = StatusMetrics::new()
            .provider(Provider::Claude)
            .tokens(1234)
            .mcp(2, 3)
            .connection(ConnectionStatus::Connected);

        assert_eq!(metrics.provider, Provider::Claude);
        assert_eq!(metrics.tokens, Some(1234));
        assert_eq!(metrics.mcp_connected, 2);
        assert_eq!(metrics.mcp_total, 3);
        assert_eq!(metrics.connection, ConnectionStatus::Connected);
    }

    #[test]
    fn test_status_bar_with_metrics() {
        let theme = Theme::dark();
        let metrics = StatusMetrics::new().provider(Provider::Claude).tokens(5000);
        let bar = StatusBar::new(TuiView::Monitor, &theme).metrics(metrics);
        assert!(bar.metrics.is_some());
    }
}
