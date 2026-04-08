// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cloud providers tab - 2x3 grid of provider cards

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Color,
    widgets::Widget,
};

use crate::unicode::truncate_to_width;

// =============================================================================
// SEMANTIC COLOR CONSTANTS
// =============================================================================

/// Dark background for overlay
const COLOR_BG_DARK: Color = Color::Rgb(17, 24, 39); // Gray-900

/// Header text (light blue)
const COLOR_HEADER: Color = Color::Rgb(147, 197, 253); // Blue-300

/// Selected model (amber)
const COLOR_SELECTED: Color = Color::Rgb(251, 191, 36); // Amber-400

/// Normal model text
const COLOR_TEXT: Color = Color::Rgb(209, 213, 219); // Gray-300

/// Context window size
const COLOR_MUTED: Color = Color::Rgb(107, 114, 128); // Gray-500

/// Hint text
const COLOR_HINT: Color = Color::Rgb(75, 85, 99); // Gray-600

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
    /// Available models for this provider
    pub models: Vec<ModelInfo>,
}

/// Model information for provider model list
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub context_k: u32, // Context window in thousands
}

impl ProviderInfo {
    /// Create all 7 cloud providers with default status
    pub fn all_providers() -> Vec<Self> {
        vec![
            ProviderInfo {
                name: "Claude",
                icon: "🍊",
                model: "claude-sonnet-4",
                env_var: "ANTHROPIC_API_KEY",
                features: vec!["🧠", "👁️", "🔧"],
                context_window: 200_000,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "claude-opus-4",
                        name: "Opus 4",
                        context_k: 200,
                    },
                    ModelInfo {
                        id: "claude-sonnet-4",
                        name: "Sonnet 4",
                        context_k: 200,
                    },
                    ModelInfo {
                        id: "claude-haiku-3.5",
                        name: "Haiku 3.5",
                        context_k: 200,
                    },
                ],
            },
            ProviderInfo {
                name: "OpenAI",
                icon: "🧿",
                model: "gpt-4o",
                env_var: "OPENAI_API_KEY",
                features: vec!["🧠", "👁️", "🔧"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "gpt-4o",
                        name: "GPT-4o",
                        context_k: 128,
                    },
                    ModelInfo {
                        id: "gpt-4o-mini",
                        name: "GPT-4o Mini",
                        context_k: 128,
                    },
                    ModelInfo {
                        id: "gpt-4.1",
                        name: "GPT-4.1",
                        context_k: 1000,
                    },
                    ModelInfo {
                        id: "o1",
                        name: "o1",
                        context_k: 200,
                    },
                    ModelInfo {
                        id: "o1-mini",
                        name: "o1 Mini",
                        context_k: 128,
                    },
                ],
            },
            ProviderInfo {
                name: "Mistral",
                icon: "⛵",
                model: "mistral-large",
                env_var: "MISTRAL_API_KEY",
                features: vec!["🧠", "🔧"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "mistral-large-latest",
                        name: "Large",
                        context_k: 128,
                    },
                    ModelInfo {
                        id: "mistral-medium-latest",
                        name: "Medium",
                        context_k: 32,
                    },
                    ModelInfo {
                        id: "mistral-small-latest",
                        name: "Small",
                        context_k: 32,
                    },
                    ModelInfo {
                        id: "codestral-latest",
                        name: "Codestral",
                        context_k: 32,
                    },
                ],
            },
            ProviderInfo {
                name: "Groq",
                icon: "👁️‍🗨️",
                model: "llama-3.3-70b",
                env_var: "GROQ_API_KEY",
                features: vec!["🧠", "⚡"],
                context_window: 128_000,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "llama-3.3-70b-versatile",
                        name: "Llama 3.3 70B",
                        context_k: 128,
                    },
                    ModelInfo {
                        id: "llama-3.1-8b-instant",
                        name: "Llama 3.1 8B",
                        context_k: 128,
                    },
                    ModelInfo {
                        id: "mixtral-8x7b-32768",
                        name: "Mixtral 8x7B",
                        context_k: 32,
                    },
                    ModelInfo {
                        id: "gemma2-9b-it",
                        name: "Gemma 2 9B",
                        context_k: 8,
                    },
                ],
            },
            ProviderInfo {
                name: "DeepSeek",
                icon: "🐋",
                model: "deepseek-chat",
                env_var: "DEEPSEEK_API_KEY",
                features: vec!["🧠", "💰"],
                context_window: 64_000,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "deepseek-chat",
                        name: "Chat",
                        context_k: 64,
                    },
                    ModelInfo {
                        id: "deepseek-coder",
                        name: "Coder",
                        context_k: 64,
                    },
                    ModelInfo {
                        id: "deepseek-reasoner",
                        name: "Reasoner (R1)",
                        context_k: 64,
                    },
                ],
            },
            ProviderInfo {
                name: "Gemini",
                icon: "💎",
                model: "gemini-2.0-flash",
                env_var: "GEMINI_API_KEY",
                features: vec!["🧠", "👁️", "🔧"],
                context_window: 1_000_000,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "gemini-2.0-flash",
                        name: "2.0 Flash",
                        context_k: 1000,
                    },
                    ModelInfo {
                        id: "gemini-2.5-pro",
                        name: "2.5 Pro",
                        context_k: 1000,
                    },
                    ModelInfo {
                        id: "gemini-2.5-flash",
                        name: "2.5 Flash",
                        context_k: 1000,
                    },
                ],
            },
            ProviderInfo {
                name: "xAI",
                icon: "𝕏",
                model: "grok-3-fast",
                env_var: "XAI_API_KEY",
                features: vec!["🧠", "🔧"],
                context_window: 131_072,
                status: ConnectionStatus::Unknown,
                models: vec![
                    ModelInfo {
                        id: "grok-3",
                        name: "Grok 3",
                        context_k: 131,
                    },
                    ModelInfo {
                        id: "grok-3-fast",
                        name: "Grok 3 Fast",
                        context_k: 131,
                    },
                    ModelInfo {
                        id: "grok-3-mini",
                        name: "Grok 3 Mini",
                        context_k: 131,
                    },
                    ModelInfo {
                        id: "grok-3-mini-fast",
                        name: "Grok 3 Mini Fast",
                        context_k: 131,
                    },
                ],
            },
        ]
    }

    /// Get model by index
    pub fn get_model(&self, idx: usize) -> Option<&ModelInfo> {
        self.models.get(idx)
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

        // Dynamic row count based on provider count (3 columns per row)
        let num_rows = self.providers.len().div_ceil(3);
        let row_constraints: Vec<Constraint> = (0..num_rows)
            .map(|_| Constraint::Ratio(1, num_rows as u32))
            .collect();

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(row_constraints)
            .split(area);

        for (row_idx, row_area) in rows.iter().enumerate() {
            // 3 providers per row
            let num_cols = 3;
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, num_cols as u32); num_cols])
                .split(*row_area);

            for (col_idx, col_area) in cols.iter().enumerate() {
                // Row 0 has 3 providers (0-2), row 1 has 3 providers (3-5)
                let provider_idx = row_idx * 3 + col_idx;
                if provider_idx < self.providers.len() {
                    let p = &self.providers[provider_idx];

                    // Check if this provider is expanded (showing model list)
                    let is_expanded = self.state.is_provider_idx_expanded(provider_idx);

                    // Determine card style based on selection AND active status
                    let is_selected = provider_idx == self.state.selected_idx;
                    let is_active = self.state.active_provider_idx == Some(provider_idx);

                    let style = match (is_selected, is_active) {
                        (true, true) => CardStyle::SelectedActive,
                        (true, false) => CardStyle::Selected,
                        (false, true) => CardStyle::Active,
                        (false, false) => CardStyle::Normal,
                    };

                    // Get latency history for sparkline
                    let latency_history = self.state.get_latency_history(provider_idx);

                    // Get verification entry for Matrix effect
                    let verify_entry = if self.state.verification_active {
                        self.state.verification_state.entries.get(provider_idx)
                    } else {
                        None
                    };

                    // Render provider card with animated indicator for active
                    let mut card = ProviderCard::new(p.icon, p.name, p.model, &p.status)
                        .style(style)
                        .features(p.features.clone())
                        .context_window(p.context_window)
                        .active_indicator(self.state.active_indicator())
                        .latency_history(&latency_history);

                    // Add verification entry if present
                    if let Some(entry) = verify_entry {
                        card = card.verify_entry(entry);
                    }

                    card.render(*col_area, buf);

                    // Render model list overlay if expanded
                    if is_expanded {
                        Self::render_model_list(
                            buf,
                            *col_area,
                            &p.models,
                            self.state.model_selection_idx,
                        );
                    }
                }
            }
        }
    }
}

impl CloudTab<'_> {
    /// Render model list overlay on top of provider card
    fn render_model_list(buf: &mut Buffer, area: Rect, models: &[ModelInfo], selected_idx: usize) {
        use ratatui::style::{Modifier, Style};

        // Calculate overlay area (inside the card, below header)
        let overlay_y = area.y + 1;
        let overlay_height = area.height.saturating_sub(2);
        let overlay_width = area.width.saturating_sub(2);
        let overlay_x = area.x + 1;

        // Semi-transparent background
        for y in overlay_y..overlay_y + overlay_height {
            for x in overlay_x..overlay_x + overlay_width {
                if y < buf.area.height && x < buf.area.width {
                    buf[(x, y)].set_bg(COLOR_BG_DARK);
                    buf[(x, y)].set_symbol(" ");
                }
            }
        }

        // Header
        let header = "▾ Models";
        if overlay_y < buf.area.height {
            buf.set_string(
                overlay_x,
                overlay_y,
                header,
                Style::default()
                    .fg(COLOR_HEADER)
                    .add_modifier(Modifier::BOLD),
            );
        }

        // Model list
        for (i, model) in models.iter().enumerate() {
            let y = overlay_y + 1 + i as u16;
            if y >= overlay_y + overlay_height || y >= buf.area.height {
                break;
            }

            let is_selected = i == selected_idx;
            let prefix = if is_selected { "▸ " } else { "  " };
            let context_str = format!(" ({}K)", model.context_k);

            let style = if is_selected {
                Style::default()
                    .fg(COLOR_SELECTED)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(COLOR_TEXT)
            };

            // Render prefix and model name
            let max_name_width = (overlay_width as usize).saturating_sub(context_str.len() + 3);
            let display_name = if model.name.len() > max_name_width {
                let truncated = truncate_to_width(model.name, max_name_width.saturating_sub(1));
                format!("{}…", truncated)
            } else {
                model.name.to_string()
            };

            buf.set_string(overlay_x, y, prefix, style);
            buf.set_string(overlay_x + 2, y, &display_name, style);

            // Context window on the right
            let context_x = overlay_x + overlay_width - context_str.len() as u16;
            if context_x > overlay_x + 2 {
                buf.set_string(context_x, y, &context_str, Style::default().fg(COLOR_MUTED));
            }
        }

        // Hint at bottom
        let hint = "↑↓ sel  ⏎ pick  ⎋ back";
        let hint_y = overlay_y + overlay_height.saturating_sub(1);
        if hint_y < buf.area.height && hint_y > overlay_y + 1 {
            let hint_x = overlay_x + (overlay_width.saturating_sub(hint.len() as u16)) / 2;
            buf.set_string(hint_x, hint_y, hint, Style::default().fg(COLOR_HINT));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_provider_info_all_providers_returns_7() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers.len(), 7);
    }

    #[test]
    fn test_provider_info_has_correct_names() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers[0].name, "Claude");
        assert_eq!(providers[1].name, "OpenAI");
        assert_eq!(providers[2].name, "Mistral");
        assert_eq!(providers[3].name, "Groq");
        assert_eq!(providers[4].name, "DeepSeek");
        assert_eq!(providers[5].name, "Gemini");
        assert_eq!(providers[6].name, "xAI");
    }

    #[test]
    fn test_provider_info_has_env_vars() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers[0].env_var, "ANTHROPIC_API_KEY");
        assert_eq!(providers[1].env_var, "OPENAI_API_KEY");
        assert_eq!(providers[5].env_var, "GEMINI_API_KEY");
        assert_eq!(providers[6].env_var, "XAI_API_KEY");
    }

    #[test]
    fn test_cloud_tab_provider_count() {
        let state = ProviderModalState::default();
        let tab = CloudTab::new(&state);
        assert_eq!(tab.provider_count(), 7);
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
        let mut buf = Buffer::empty(Rect::new(0, 0, 100, 25));
        tab.render(Rect::new(0, 0, 100, 25), &mut buf);
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Groq"));
        assert!(content.contains("DeepSeek"));
        assert!(content.contains("Gemini"));
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
        state.selected_idx = 5; // Gemini (last)
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
