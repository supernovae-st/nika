//! Cloud providers tab - 2x3 grid of provider cards

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::Widget,
};

use super::super::components::{CardStyle, ProviderCard};
use super::super::state::{ConnectionStatus, ProviderModalState};

/// Provider information for display
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: &'static str,
    pub icon: &'static str,
    pub model: &'static str,
    pub env_var: &'static str,
    pub features: Vec<&'static str>,
    pub context_window: u32,
    pub status: ConnectionStatus,
}

impl ProviderInfo {
    /// Create all 6 supported providers with default status
    pub fn all_providers() -> Vec<Self> {
        vec![
            ProviderInfo {
                name: "Claude",
                icon: "🧠",
                model: "claude-sonnet-4",
                env_var: "ANTHROPIC_API_KEY",
                features: vec!["🧠", "👁️", "🔧"],
                context_window: 200_000,
                status: ConnectionStatus::Unknown,
            },
            ProviderInfo {
                name: "OpenAI",
                icon: "🤖",
                model: "gpt-4o",
                env_var: "OPENAI_API_KEY",
                features: vec!["🧠", "👁️", "🔧"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
            },
            ProviderInfo {
                name: "Mistral",
                icon: "🌀",
                model: "mistral-large",
                env_var: "MISTRAL_API_KEY",
                features: vec!["🧠", "🔧"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
            },
            ProviderInfo {
                name: "Groq",
                icon: "⚡",
                model: "llama-3.3-70b",
                env_var: "GROQ_API_KEY",
                features: vec!["🧠", "⚡"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
            },
            ProviderInfo {
                name: "DeepSeek",
                icon: "🔬",
                model: "deepseek-chat",
                env_var: "DEEPSEEK_API_KEY",
                features: vec!["🧠", "💰"],
                context_window: 64_000,
                status: ConnectionStatus::Unknown,
            },
            ProviderInfo {
                name: "Ollama",
                icon: "🦙",
                model: "llama3.2",
                env_var: "OLLAMA_HOST",
                features: vec!["🏠", "🔒"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
            },
        ]
    }
}

/// Cloud providers tab with 2x3 grid of provider cards
pub struct CloudTab<'a> {
    state: &'a ProviderModalState,
    providers: Vec<ProviderInfo>,
}

impl<'a> CloudTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self {
            state,
            providers: ProviderInfo::all_providers(),
        }
    }

    /// Create with custom provider statuses
    pub fn with_statuses(state: &'a ProviderModalState, statuses: Vec<ConnectionStatus>) -> Self {
        let mut providers = ProviderInfo::all_providers();
        for (i, status) in statuses.into_iter().enumerate() {
            if i < providers.len() {
                providers[i].status = status;
            }
        }
        Self { state, providers }
    }

    /// Get total provider count
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

impl Widget for CloudTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 6 || area.width < 40 {
            return;
        }

        // 2x3 grid layout
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
            .split(area);

        for (row_idx, row_area) in rows.iter().enumerate() {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Ratio(1, 3),
                    Constraint::Ratio(1, 3),
                    Constraint::Ratio(1, 3),
                ])
                .split(*row_area);

            for (col_idx, col_area) in cols.iter().enumerate() {
                let provider_idx = row_idx * 3 + col_idx;
                if provider_idx < self.providers.len() {
                    let p = &self.providers[provider_idx];

                    // Determine card style based on selection AND active status
                    let is_selected = provider_idx == self.state.selected_idx;
                    let is_active = self.state.active_provider_idx == Some(provider_idx);

                    let style = match (is_selected, is_active) {
                        (true, true) => CardStyle::SelectedActive,
                        (true, false) => CardStyle::Selected,
                        (false, true) => CardStyle::Active,
                        (false, false) => CardStyle::Normal,
                    };

                    // Render provider card
                    ProviderCard::new(p.icon, p.name, p.model, &p.status)
                        .style(style)
                        .features(p.features.clone())
                        .context_window(p.context_window)
                        .render(*col_area, buf);
                }
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
    fn test_provider_info_all_providers_returns_6() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers.len(), 6);
    }

    #[test]
    fn test_provider_info_has_correct_names() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers[0].name, "Claude");
        assert_eq!(providers[1].name, "OpenAI");
        assert_eq!(providers[2].name, "Mistral");
        assert_eq!(providers[3].name, "Groq");
        assert_eq!(providers[4].name, "DeepSeek");
        assert_eq!(providers[5].name, "Ollama");
    }

    #[test]
    fn test_provider_info_has_env_vars() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers[0].env_var, "ANTHROPIC_API_KEY");
        assert_eq!(providers[1].env_var, "OPENAI_API_KEY");
        assert_eq!(providers[5].env_var, "OLLAMA_HOST");
    }

    #[test]
    fn test_cloud_tab_provider_count() {
        let state = ProviderModalState::default();
        let tab = CloudTab::new(&state);
        assert_eq!(tab.provider_count(), 6);
    }

    #[test]
    fn test_cloud_tab_renders_6_providers() {
        let mut state = ProviderModalState::default();
        state.item_count = 6;
        let tab = CloudTab::new(&state);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
        assert!(content.contains("OpenAI"));
        assert!(content.contains("Mistral"));
    }

    #[test]
    fn test_cloud_tab_renders_second_row() {
        let mut state = ProviderModalState::default();
        state.item_count = 6;
        let tab = CloudTab::new(&state);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Groq"));
        assert!(content.contains("DeepSeek"));
        assert!(content.contains("Ollama"));
    }

    #[test]
    fn test_cloud_tab_selection_at_index_0() {
        let mut state = ProviderModalState::default();
        state.selected_idx = 0;
        let tab = CloudTab::new(&state);
        // Should render without panic, first card selected
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
    }

    #[test]
    fn test_cloud_tab_selection_at_index_5() {
        let mut state = ProviderModalState::default();
        state.selected_idx = 5; // Ollama (last)
        let tab = CloudTab::new(&state);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
    }

    #[test]
    fn test_cloud_tab_with_custom_statuses() {
        let state = ProviderModalState::default();
        let statuses = vec![
            ConnectionStatus::Connected { latency_ms: 100 },
            ConnectionStatus::Failed {
                error: "No key".into(),
            },
            ConnectionStatus::NotConfigured,
            ConnectionStatus::Checking,
            ConnectionStatus::Unknown,
            ConnectionStatus::Connected { latency_ms: 50 },
        ];
        let tab = CloudTab::with_statuses(&state, statuses);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 20));
        tab.render(Rect::new(0, 0, 80, 20), &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        // Status indicators should render
        assert!(content.contains("100ms") || content.contains("●"));
    }

    #[test]
    fn test_cloud_tab_small_area_no_panic() {
        let state = ProviderModalState::default();
        let tab = CloudTab::new(&state);
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 4));
        // Should not panic, just not render
        tab.render(Rect::new(0, 0, 30, 4), &mut buf);
    }
}
