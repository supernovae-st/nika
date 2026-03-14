//! Provider Selector Widget
//!
//! Popup for selecting LLM provider and model with streaming indicators.
//! Inspired by VS Code command palette style.
//!
//!
//! All providers check for non-empty API key in environment variables.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Clear, Widget},
};
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════════════════
// COLORS (VS Code-like dark theme)
// ═══════════════════════════════════════════════════════════════════════════

const BORDER_COLOR: Color = Color::Rgb(99, 102, 241); // indigo
const BG_COLOR: Color = Color::Rgb(17, 24, 39); // dark slate
const TEXT_COLOR: Color = Color::Rgb(229, 231, 235); // light text
const MUTED_COLOR: Color = Color::Rgb(107, 114, 128); // muted text
const SELECTED_BG: Color = Color::Rgb(55, 65, 81); // selection bg
const AVAILABLE_COLOR: Color = Color::Rgb(34, 197, 94); // green
const UNAVAILABLE_COLOR: Color = Color::Rgb(239, 68, 68); // red
const STREAMING_COLOR: Color = Color::Rgb(59, 130, 246); // blue

// ═══════════════════════════════════════════════════════════════════════════
// ═══════════════════════════════════════════════════════════════════════════

/// Connection verification status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyStatus {
    /// Not yet verified
    #[default]
    Unknown,
    /// Verification in progress
    Verifying,
    /// Successfully verified
    Verified,
    /// Verification failed
    Failed,
}

/// Selector section for navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectorSection {
    /// LLM providers section
    #[default]
    Providers,
    /// MCP servers section
    McpServers,
}

/// MCP server display information
#[derive(Debug, Clone)]
pub struct McpServerDisplay {
    /// Server name
    pub name: String,
    /// Verification status
    pub status: VerifyStatus,
    /// Response latency (if verified)
    pub latency: Option<Duration>,
    /// Number of available tools (if verified)
    pub tool_count: Option<usize>,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl McpServerDisplay {
    /// Create a new MCP server display entry
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: VerifyStatus::Unknown,
            latency: None,
            tool_count: None,
            error: None,
        }
    }

    /// Set status to verifying (pinging)
    pub fn set_verifying(&mut self) {
        self.status = VerifyStatus::Verifying;
        self.latency = None;
        self.tool_count = None;
        self.error = None;
    }

    /// Set status to verified with latency and tool count
    pub fn set_verified(&mut self, latency: Duration, tool_count: usize) {
        self.status = VerifyStatus::Verified;
        self.latency = Some(latency);
        self.tool_count = Some(tool_count);
        self.error = None;
    }

    /// Set status to failed with error message
    pub fn set_failed(&mut self, error: impl Into<String>) {
        self.status = VerifyStatus::Failed;
        self.latency = None;
        self.tool_count = None;
        self.error = Some(error.into());
    }
}

/// Provider information for display
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// Provider ID (e.g., "claude", "openai")
    pub id: String,
    /// Display name
    pub name: String,
    /// Icon emoji
    pub icon: &'static str,
    /// Default model
    pub default_model: &'static str,
    /// Available models
    pub models: Vec<ModelInfo>,
    /// Whether API key is configured
    pub available: bool,
    /// Environment variable name
    pub env_var: &'static str,
    /// Reason why provider is unavailable
    pub unavailable_reason: Option<String>,
    /// Connection verification status
    pub verify_status: VerifyStatus,
    /// Last measured latency
    pub latency: Option<Duration>,
    /// Verification error message
    pub verify_error: Option<String>,
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    /// Model ID
    pub id: String,
    /// Display name
    pub name: String,
    /// Supports streaming
    pub streaming: bool,
    /// Supports extended thinking (Claude only)
    pub thinking: bool,
    /// Context window size
    pub context_window: u32,
}

impl ProviderInfo {
    /// Create provider info for all supported providers
    ///
    /// All providers require non-empty API key in environment.
    pub fn all_providers() -> Vec<ProviderInfo> {
        // Helper to check API key availability with reason
        let check_api_key = |key: &str| -> (bool, Option<String>) {
            match std::env::var(key) {
                Ok(v) if !v.is_empty() => (true, None),
                Ok(_) => (false, Some(format!("{} is empty", key))),
                Err(_) => (false, Some(format!("{} not set", key))),
            }
        };

        // 6 API providers: Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini

        let (claude_available, claude_reason) = check_api_key("ANTHROPIC_API_KEY");
        let (openai_available, openai_reason) = check_api_key("OPENAI_API_KEY");
        let (mistral_available, mistral_reason) = check_api_key("MISTRAL_API_KEY");
        let (groq_available, groq_reason) = check_api_key("GROQ_API_KEY");
        let (deepseek_available, deepseek_reason) = check_api_key("DEEPSEEK_API_KEY");
        let (gemini_available, gemini_reason) = check_api_key("GEMINI_API_KEY");

        vec![
            ProviderInfo {
                id: "claude".to_string(),
                name: "Anthropic Claude".to_string(),
                icon: "🧠",
                default_model: "claude-sonnet-4-6",
                env_var: "ANTHROPIC_API_KEY",
                available: claude_available,
                unavailable_reason: claude_reason,
                verify_status: VerifyStatus::Unknown,
                latency: None,
                verify_error: None,
                models: vec![
                    ModelInfo {
                        id: "claude-sonnet-4-6".to_string(),
                        name: "Claude Sonnet 4".to_string(),
                        streaming: true,
                        thinking: true,
                        context_window: 200_000,
                    },
                    ModelInfo {
                        id: "claude-opus-4-5".to_string(),
                        name: "Claude Opus 4.5".to_string(),
                        streaming: true,
                        thinking: true,
                        context_window: 200_000,
                    },
                    ModelInfo {
                        id: "claude-3-5-haiku-latest".to_string(),
                        name: "Claude 3.5 Haiku".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 200_000,
                    },
                ],
            },
            ProviderInfo {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                icon: "🤖",
                default_model: "gpt-4o",
                env_var: "OPENAI_API_KEY",
                available: openai_available,
                unavailable_reason: openai_reason,
                verify_status: VerifyStatus::Unknown,
                latency: None,
                verify_error: None,
                models: vec![
                    ModelInfo {
                        id: "gpt-4o".to_string(),
                        name: "GPT-4o".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 128_000,
                    },
                    ModelInfo {
                        id: "gpt-4o-mini".to_string(),
                        name: "GPT-4o Mini".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 128_000,
                    },
                    ModelInfo {
                        id: "o1".to_string(),
                        name: "O1 (Reasoning)".to_string(),
                        streaming: true,
                        thinking: true,
                        context_window: 200_000,
                    },
                ],
            },
            ProviderInfo {
                id: "mistral".to_string(),
                name: "Mistral AI".to_string(),
                icon: "🌬️",
                default_model: "mistral-large-latest",
                env_var: "MISTRAL_API_KEY",
                available: mistral_available,
                unavailable_reason: mistral_reason,
                verify_status: VerifyStatus::Unknown,
                latency: None,
                verify_error: None,
                models: vec![
                    ModelInfo {
                        id: "mistral-large-latest".to_string(),
                        name: "Mistral Large".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 128_000,
                    },
                    ModelInfo {
                        id: "mistral-medium-latest".to_string(),
                        name: "Mistral Medium".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 32_000,
                    },
                ],
            },
            ProviderInfo {
                id: "groq".to_string(),
                name: "Groq".to_string(),
                icon: "⚡",
                default_model: "llama-3.3-70b-versatile",
                env_var: "GROQ_API_KEY",
                available: groq_available,
                unavailable_reason: groq_reason,
                verify_status: VerifyStatus::Unknown,
                latency: None,
                verify_error: None,
                models: vec![
                    ModelInfo {
                        id: "llama-3.3-70b-versatile".to_string(),
                        name: "Llama 3.3 70B".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 128_000,
                    },
                    ModelInfo {
                        id: "mixtral-8x7b-32768".to_string(),
                        name: "Mixtral 8x7B".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 32_000,
                    },
                ],
            },
            ProviderInfo {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                icon: "🔍",
                default_model: "deepseek-chat",
                env_var: "DEEPSEEK_API_KEY",
                available: deepseek_available,
                unavailable_reason: deepseek_reason,
                verify_status: VerifyStatus::Unknown,
                latency: None,
                verify_error: None,
                models: vec![
                    ModelInfo {
                        id: "deepseek-chat".to_string(),
                        name: "DeepSeek Chat".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 64_000,
                    },
                    ModelInfo {
                        id: "deepseek-reasoner".to_string(),
                        name: "DeepSeek Reasoner".to_string(),
                        streaming: true,
                        thinking: true,
                        context_window: 64_000,
                    },
                ],
            },
            ProviderInfo {
                id: "gemini".to_string(),
                name: "Google Gemini".to_string(),
                icon: "💎",
                default_model: "gemini-2.0-flash",
                env_var: "GEMINI_API_KEY",
                available: gemini_available,
                unavailable_reason: gemini_reason,
                verify_status: VerifyStatus::Unknown,
                latency: None,
                verify_error: None,
                models: vec![
                    ModelInfo {
                        id: "gemini-2.0-flash".to_string(),
                        name: "Gemini 2.0 Flash".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 1_000_000,
                    },
                    ModelInfo {
                        id: "gemini-1.5-pro".to_string(),
                        name: "Gemini 1.5 Pro".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 2_000_000,
                    },
                    ModelInfo {
                        id: "gemini-1.5-flash".to_string(),
                        name: "Gemini 1.5 Flash".to_string(),
                        streaming: true,
                        thinking: false,
                        context_window: 1_000_000,
                    },
                ],
            },
            // For local inference, use `provider: native` with mistral.rs
        ]
    }
}

/// State for the provider selector
#[derive(Debug, Clone)]
pub struct ProviderSelectorState {
    /// All providers
    pub providers: Vec<ProviderInfo>,
    /// Currently selected provider index
    pub selected_provider: usize,
    /// Currently selected model index (within provider)
    pub selected_model: usize,
    /// Whether we're in model selection mode
    pub model_mode: bool,
    /// Whether the selector is visible
    pub visible: bool,
    // ═══ MCP Section ═══
    /// MCP servers display information
    pub mcp_servers: Vec<McpServerDisplay>,
    /// Current section (Providers or McpServers)
    pub selected_section: SelectorSection,
    /// Currently selected MCP server index
    pub selected_mcp: usize,
}

impl Default for ProviderSelectorState {
    fn default() -> Self {
        Self {
            providers: ProviderInfo::all_providers(),
            selected_provider: 0,
            selected_model: 0,
            model_mode: false,
            visible: false,
            mcp_servers: Vec::new(),
            selected_section: SelectorSection::Providers,
            selected_mcp: 0,
        }
    }
}

impl ProviderSelectorState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.model_mode = false;
            self.selected_model = 0;
        }
    }

    /// Show the selector
    pub fn show(&mut self) {
        self.visible = true;
        self.model_mode = false;
        self.selected_model = 0;
    }

    /// Hide the selector
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Move selection up
    pub fn up(&mut self) {
        if self.model_mode {
            if self.selected_model > 0 {
                self.selected_model -= 1;
            }
        } else if self.selected_provider > 0 {
            self.selected_provider -= 1;
        }
    }

    /// Move selection down
    pub fn down(&mut self) {
        if self.model_mode {
            let models_count = self.providers[self.selected_provider].models.len();
            if self.selected_model < models_count.saturating_sub(1) {
                self.selected_model += 1;
            }
        } else if self.selected_provider < self.providers.len().saturating_sub(1) {
            self.selected_provider += 1;
        }
    }

    /// Enter model selection or confirm selection
    pub fn enter(&mut self) -> Option<(String, String)> {
        if self.model_mode {
            // Confirm model selection - clone IDs before borrowing mutably
            let provider_id = self.providers[self.selected_provider].id.clone();
            let model_id = self.providers[self.selected_provider].models[self.selected_model]
                .id
                .clone();
            self.hide();
            Some((provider_id, model_id))
        } else {
            // Enter model selection
            self.model_mode = true;
            self.selected_model = 0;
            None
        }
    }

    /// Go back (escape from model mode or close)
    pub fn back(&mut self) {
        if self.model_mode {
            self.model_mode = false;
        } else {
            self.hide();
        }
    }

    /// Get current provider
    pub fn current_provider(&self) -> &ProviderInfo {
        &self.providers[self.selected_provider]
    }

    /// Get current model (if in model mode)
    pub fn current_model(&self) -> Option<&ModelInfo> {
        if self.model_mode {
            Some(&self.providers[self.selected_provider].models[self.selected_model])
        } else {
            None
        }
    }

    // === Aliases for chat.rs compatibility ===

    /// Alias for hide() - close the selector
    pub fn close(&mut self) {
        self.hide();
    }

    /// Alias for up()
    pub fn move_up(&mut self) {
        self.up();
    }

    /// Alias for down()
    pub fn move_down(&mut self) {
        self.down();
    }

    /// Alias for current_provider()
    pub fn selected_provider(&self) -> &ProviderInfo {
        self.current_provider()
    }

    /// Get the currently highlighted model (always returns the highlighted model, not just in model_mode)
    pub fn selected_model(&self) -> Option<&ModelInfo> {
        let provider = &self.providers[self.selected_provider];
        if provider.models.is_empty() {
            None
        } else {
            Some(
                &provider.models[self
                    .selected_model
                    .min(provider.models.len().saturating_sub(1))],
            )
        }
    }

    /// Enter model selection mode
    pub fn enter_model_mode(&mut self) {
        self.model_mode = true;
        self.selected_model = 0;
    }

    /// Mark provider as verifying
    pub fn set_verifying(&mut self, provider_id: &str) {
        if let Some(p) = self.providers.iter_mut().find(|p| p.id == provider_id) {
            p.verify_status = VerifyStatus::Verifying;
        }
    }

    /// Update provider verification result
    pub fn set_verified(&mut self, provider_id: &str, latency: Duration) {
        if let Some(p) = self.providers.iter_mut().find(|p| p.id == provider_id) {
            p.verify_status = VerifyStatus::Verified;
            p.latency = Some(latency);
            p.verify_error = None;
            p.available = true;
            p.unavailable_reason = None;
        }
    }

    /// Mark provider verification as failed
    pub fn set_verify_failed(&mut self, provider_id: &str, error: String) {
        if let Some(p) = self.providers.iter_mut().find(|p| p.id == provider_id) {
            p.verify_status = VerifyStatus::Failed;
            p.verify_error = Some(error.clone());
            p.unavailable_reason = Some(error);
        }
    }

    /// Refresh provider list (re-check availability)
    pub fn refresh(&mut self) {
        self.providers = ProviderInfo::all_providers();
    }

    /// Get provider by ID
    pub fn get_provider(&self, id: &str) -> Option<&ProviderInfo> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Get mutable provider by ID
    pub fn get_provider_mut(&mut self, id: &str) -> Option<&mut ProviderInfo> {
        self.providers.iter_mut().find(|p| p.id == id)
    }

    // ═══ MCP Section Methods ═══

    /// Toggle between sections (Providers <-> McpServers)
    pub fn toggle_section(&mut self) {
        match self.selected_section {
            SelectorSection::Providers => {
                if !self.mcp_servers.is_empty() {
                    self.selected_section = SelectorSection::McpServers;
                    self.selected_mcp = 0;
                }
            }
            SelectorSection::McpServers => {
                self.selected_section = SelectorSection::Providers;
                self.model_mode = false;
            }
        }
    }

    /// Move up in current section
    pub fn move_up_in_section(&mut self) {
        match self.selected_section {
            SelectorSection::Providers => self.up(),
            SelectorSection::McpServers => {
                if self.selected_mcp > 0 {
                    self.selected_mcp -= 1;
                }
            }
        }
    }

    /// Move down in current section
    pub fn move_down_in_section(&mut self) {
        match self.selected_section {
            SelectorSection::Providers => self.down(),
            SelectorSection::McpServers => {
                if self.selected_mcp < self.mcp_servers.len().saturating_sub(1) {
                    self.selected_mcp += 1;
                }
            }
        }
    }

    /// Add or update MCP server display entry
    pub fn set_mcp_server(&mut self, name: impl Into<String>, entry: McpServerDisplay) {
        let name = name.into();
        if let Some(server) = self.mcp_servers.iter_mut().find(|s| s.name == name) {
            *server = entry;
        } else {
            self.mcp_servers.push(entry);
        }
    }

    /// Mark MCP server as pinging
    pub fn set_mcp_pinging(&mut self, name: &str) {
        if let Some(server) = self.mcp_servers.iter_mut().find(|s| s.name == name) {
            server.set_verifying();
        } else {
            let mut server = McpServerDisplay::new(name);
            server.set_verifying();
            self.mcp_servers.push(server);
        }
    }

    /// Mark MCP server as verified
    pub fn set_mcp_verified(&mut self, name: &str, latency: Duration, tool_count: usize) {
        if let Some(server) = self.mcp_servers.iter_mut().find(|s| s.name == name) {
            server.set_verified(latency, tool_count);
        }
    }

    /// Mark MCP server as failed
    pub fn set_mcp_failed(&mut self, name: &str, error: impl Into<String>) {
        if let Some(server) = self.mcp_servers.iter_mut().find(|s| s.name == name) {
            server.set_failed(error);
        }
    }

    /// Get MCP server by name
    pub fn get_mcp_server(&self, name: &str) -> Option<&McpServerDisplay> {
        self.mcp_servers.iter().find(|s| s.name == name)
    }

    /// Check if currently in MCP section
    pub fn is_mcp_section(&self) -> bool {
        self.selected_section == SelectorSection::McpServers
    }

    /// Get count of verified MCP servers
    pub fn verified_mcp_count(&self) -> usize {
        self.mcp_servers
            .iter()
            .filter(|s| s.status == VerifyStatus::Verified)
            .count()
    }
}

/// Provider Selector widget
pub struct ProviderSelector<'a> {
    state: &'a ProviderSelectorState,
}

impl<'a> ProviderSelector<'a> {
    pub fn new(state: &'a ProviderSelectorState) -> Self {
        Self { state }
    }

    fn render_provider_list(&self, area: Rect, buf: &mut Buffer) {
        let title = if self.state.model_mode {
            format!(
                " {} Select Model ",
                self.state.providers[self.state.selected_provider].icon
            )
        } else {
            " 🎛️ Select Provider ".to_string()
        };

        // Draw block with border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR))
            .style(Style::default().bg(BG_COLOR))
            .title(Span::styled(
                title,
                Style::default().fg(TEXT_COLOR).add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        if self.state.model_mode {
            self.render_models(inner, buf);
        } else {
            self.render_providers(inner, buf);
        }

        // Footer hint
        let hint = if self.state.model_mode {
            "↑↓ Select • Enter Confirm • Esc Back"
        } else {
            "↑↓ Select • Enter Models • Esc Close"
        };
        if area.height > 2 {
            let hint_y = area.y + area.height - 1;
            let hint_x = area.x + 2;
            buf.set_string(hint_x, hint_y, hint, Style::default().fg(MUTED_COLOR));
        }
    }

    fn render_providers(&self, area: Rect, buf: &mut Buffer) {
        for (i, provider) in self.state.providers.iter().enumerate() {
            if i as u16 >= area.height {
                break;
            }

            let y = area.y + i as u16;
            let is_selected = i == self.state.selected_provider;

            // Background for selected
            if is_selected {
                for x in area.x..area.x + area.width {
                    buf[(x, y)].set_bg(SELECTED_BG);
                }
            }

            // Icon
            let icon_style = if provider.available {
                Style::default().fg(TEXT_COLOR)
            } else {
                Style::default().fg(MUTED_COLOR)
            };
            buf.set_string(area.x + 1, y, provider.icon, icon_style);

            // Name
            let name_style = if provider.available {
                Style::default().fg(TEXT_COLOR)
            } else {
                Style::default().fg(MUTED_COLOR)
            };
            buf.set_string(area.x + 4, y, &provider.name, name_style);

            // Status indicator
            let status_x = area.x + area.width.saturating_sub(18);
            match (&provider.verify_status, provider.available) {
                (VerifyStatus::Verified, _) => {
                    // Show latency if verified
                    let latency_str = provider
                        .latency
                        .map(|d| format!("✓ {}ms", d.as_millis()))
                        .unwrap_or_else(|| "✓ OK".to_string());
                    buf.set_string(
                        status_x,
                        y,
                        &latency_str,
                        Style::default().fg(AVAILABLE_COLOR),
                    );
                }
                (VerifyStatus::Verifying, _) => {
                    buf.set_string(
                        status_x,
                        y,
                        "⟳ Checking...",
                        Style::default().fg(STREAMING_COLOR),
                    );
                }
                (VerifyStatus::Failed, _) => {
                    buf.set_string(
                        status_x,
                        y,
                        "✗ Failed",
                        Style::default().fg(UNAVAILABLE_COLOR),
                    );
                }
                (VerifyStatus::Unknown, true) => {
                    buf.set_string(status_x, y, "○ Ready", Style::default().fg(AVAILABLE_COLOR));
                }
                (VerifyStatus::Unknown, false) => {
                    buf.set_string(
                        status_x,
                        y,
                        "✗ No Key",
                        Style::default().fg(UNAVAILABLE_COLOR),
                    );
                }
            }

            // Streaming indicator
            let stream_x = area.x + area.width.saturating_sub(20);
            buf.set_string(stream_x, y, "⚡", Style::default().fg(STREAMING_COLOR));
        }
    }

    fn render_models(&self, area: Rect, buf: &mut Buffer) {
        let provider = &self.state.providers[self.state.selected_provider];

        for (i, model) in provider.models.iter().enumerate() {
            if i as u16 >= area.height {
                break;
            }

            let y = area.y + i as u16;
            let is_selected = i == self.state.selected_model;

            // Background for selected
            if is_selected {
                for x in area.x..area.x + area.width {
                    buf[(x, y)].set_bg(SELECTED_BG);
                }
            }

            // Model name
            buf.set_string(area.x + 2, y, &model.name, Style::default().fg(TEXT_COLOR));

            // Features
            let mut features = Vec::new();
            if model.streaming {
                features.push("⚡");
            }
            if model.thinking {
                features.push("🧠");
            }

            let features_str = features.join(" ");
            let features_x = area.x + area.width.saturating_sub(10);
            buf.set_string(
                features_x,
                y,
                &features_str,
                Style::default().fg(STREAMING_COLOR),
            );

            // Context window
            let ctx_str = format!("{}K", model.context_window / 1000);
            let ctx_x = area.x + area.width.saturating_sub(16);
            buf.set_string(ctx_x, y, &ctx_str, Style::default().fg(MUTED_COLOR));
        }
    }
}

impl Widget for ProviderSelector<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate popup size and position (centered)
        let popup_width = 50.min(area.width.saturating_sub(4));
        let popup_height = 10.min(area.height.saturating_sub(4));
        let popup_x = (area.width - popup_width) / 2 + area.x;
        let popup_y = (area.height - popup_height) / 2 + area.y;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area first
        Clear.render(popup_area, buf);

        // Render the provider list
        self.render_provider_list(popup_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_info_all_providers() {
        let providers = ProviderInfo::all_providers();
        assert_eq!(providers.len(), 6);
        assert_eq!(providers[0].id, "claude");
        assert_eq!(providers[1].id, "openai");
        assert_eq!(providers[2].id, "mistral");
        assert_eq!(providers[3].id, "groq");
        assert_eq!(providers[4].id, "deepseek");
        assert_eq!(providers[5].id, "gemini");
    }

    #[test]
    fn test_provider_selector_state_navigation() {
        let mut state = ProviderSelectorState::new();
        state.show();

        assert_eq!(state.selected_provider, 0);

        state.down();
        assert_eq!(state.selected_provider, 1);

        state.down();
        assert_eq!(state.selected_provider, 2);

        state.up();
        assert_eq!(state.selected_provider, 1);

        state.up();
        assert_eq!(state.selected_provider, 0);

        // Can't go below 0
        state.up();
        assert_eq!(state.selected_provider, 0);
    }

    #[test]
    fn test_provider_selector_model_mode() {
        let mut state = ProviderSelectorState::new();
        state.show();

        // Enter model mode
        assert!(!state.model_mode);
        state.enter();
        assert!(state.model_mode);

        // Navigate models
        assert_eq!(state.selected_model, 0);
        state.down();
        assert_eq!(state.selected_model, 1);

        // Back to provider mode
        state.back();
        assert!(!state.model_mode);
    }

    #[test]
    fn test_provider_selector_confirm_selection() {
        let mut state = ProviderSelectorState::new();
        state.show();

        // Select OpenAI
        state.down(); // -> openai
        state.enter(); // model mode
        state.down(); // -> gpt-4o-mini

        let result = state.enter();
        assert!(result.is_some());
        let (provider, model) = result.unwrap();
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-4o-mini");
        assert!(!state.visible);
    }


    #[test]
    fn test_all_models_have_streaming() {
        let providers = ProviderInfo::all_providers();
        for provider in &providers {
            for model in &provider.models {
                assert!(
                    model.streaming,
                    "Model {} should support streaming",
                    model.id
                );
            }
        }
    }
}
