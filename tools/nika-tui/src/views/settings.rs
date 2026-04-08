// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Settings View - Provider configuration, theme, preferences
//!
//! 5-section layout:
//!
//! ```text
//! ╭───────────────────────────────────────────────────────────────────────────────╮
//! │  SETTINGS                                                    [Esc] Back       │
//! ├───────────────────────────────────────────────────────────────────────────────┤
//! │                                                                               │
//! │  ┌─ 1. PROVIDERS ────────────────────────────────────────────────────────┐    │
//! │  │  LLM: 4/7 configured  |  MCP: 2/6 configured                          │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! │  ┌─ 2. MCP SERVERS ──────────────────────────────────────────────────────┐    │
//! │  │  Active: novanet, perplexity                                          │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! │  ┌─ 3. SECRETS ──────────────────────────────────────────────────────────┐    │
//! │  │  Source: daemon  |  Keychain: 5 entries                               │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! │  ┌─ 4. PACKAGES ─────────────────────────────────────────────────────────┐    │
//! │  │  Installed: @nika/core@1.0                                            │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! │  ┌─ 5. PREFERENCES ──────────────────────────────────────────────────────┐    │
//! │  │  Theme: Dark  |  [1] Light  [2] Dark  [3] Solarized                   │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! ╰───────────────────────────────────────────────────────────────────────────────╯
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::view_trait::View;
use super::{CosmicVariant, TuiView, ViewAction};
use crate::state::TuiState;
use crate::theme::Theme;

/// Settings section for navigation (5 sections for SPN integration)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    Providers, // LLM + MCP status summary
    McpServers,  // Active MCP servers
    Secrets,     // Daemon status + vault info
    Packages,    // Installed packages (future)
    Preferences, // Theme + shortcuts merged
}

impl SettingsSection {
    /// Get all sections in order
    pub const ALL: [SettingsSection; 5] = [
        SettingsSection::Providers,
        SettingsSection::McpServers,
        SettingsSection::Secrets,
        SettingsSection::Packages,
        SettingsSection::Preferences,
    ];

    /// Get next section
    pub fn next(&self) -> Self {
        match self {
            SettingsSection::Providers => SettingsSection::McpServers,
            SettingsSection::McpServers => SettingsSection::Secrets,
            SettingsSection::Secrets => SettingsSection::Packages,
            SettingsSection::Packages => SettingsSection::Preferences,
            SettingsSection::Preferences => SettingsSection::Providers,
        }
    }

    /// Get previous section
    pub fn prev(&self) -> Self {
        match self {
            SettingsSection::Providers => SettingsSection::Preferences,
            SettingsSection::McpServers => SettingsSection::Providers,
            SettingsSection::Secrets => SettingsSection::McpServers,
            SettingsSection::Packages => SettingsSection::Secrets,
            SettingsSection::Preferences => SettingsSection::Packages,
        }
    }

    /// Get section title
    pub fn title(&self) -> &'static str {
        match self {
            SettingsSection::Providers => "PROVIDERS",
            SettingsSection::McpServers => "MCP SERVERS",
            SettingsSection::Secrets => "SECRETS",
            SettingsSection::Packages => "PACKAGES",
            SettingsSection::Preferences => "PREFERENCES",
        }
    }

    /// Get section number (1-indexed for display)
    pub fn number(&self) -> u8 {
        match self {
            SettingsSection::Providers => 1,
            SettingsSection::McpServers => 2,
            SettingsSection::Secrets => 3,
            SettingsSection::Packages => 4,
            SettingsSection::Preferences => 5,
        }
    }
}

/// Secrets info for display
#[derive(Debug, Clone, Default)]
pub struct SecretsInfo {
    /// Source description (e.g., "env vars")
    pub source: String,
    /// Number of configured entries
    pub vault_count: usize,
}

/// Settings view state (5 sections for SPN integration)
pub struct SettingsView {
    /// Currently selected section
    pub section: SettingsSection,
    /// Provider name (cached for display)
    pub provider_name: String,
    /// Model name (cached for display)
    pub model_name: String,
    /// Current theme name
    pub theme_name: String,
    /// LLM providers configured count
    pub llm_configured: usize,
    /// MCP providers configured count
    pub mcp_configured: usize,
    /// Active MCP servers
    pub active_mcp_servers: Vec<String>,
    /// Secrets info
    pub secrets_info: SecretsInfo,
    /// Installed packages (future)
    pub installed_packages: Vec<String>,
}

impl SettingsView {
    /// Create a new SettingsView
    pub fn new() -> Self {
        Self {
            section: SettingsSection::Providers,
            provider_name: "Auto-detect".to_string(),
            model_name: "".to_string(),
            theme_name: "Dark".to_string(),
            llm_configured: 0,
            mcp_configured: 0,
            active_mcp_servers: Vec::new(),
            secrets_info: SecretsInfo {
                source: "checking...".to_string(),
                vault_count: 0,
            },
            installed_packages: Vec::new(),
        }
    }

    /// Update provider info from state
    ///
    /// Called from App when provider changes.
    pub fn update_provider(&mut self, provider: &str, model: &str) {
        self.provider_name = provider.to_string();
        self.model_name = model.to_string();
    }

    /// Update theme name from theme mode
    ///
    /// Called from App when theme changes.
    pub fn update_theme_name(&mut self, name: &str) {
        self.theme_name = name.to_string();
    }

    /// Update provider counts
    pub fn update_provider_counts(&mut self, llm: usize, mcp: usize) {
        self.llm_configured = llm;
        self.mcp_configured = mcp;
    }

    /// Update active MCP servers
    pub fn update_mcp_servers(&mut self, servers: Vec<String>) {
        self.active_mcp_servers = servers;
    }

    /// Update secrets info
    pub fn update_secrets_info(&mut self, source: &str, count: usize) {
        self.secrets_info = SecretsInfo {
            source: source.to_string(),
            vault_count: count,
        };
    }

    /// Update installed packages
    pub fn update_packages(&mut self, packages: Vec<String>) {
        self.installed_packages = packages;
    }

    /// Refresh all settings data by checking environment and system state
    ///
    /// Called when entering Settings view to update:
    /// - LLM provider count (checks env vars)
    /// - MCP provider count (checks env vars)
    /// - Secrets/daemon status
    pub fn refresh_data(&mut self) {
        use crate::providers::{env_var, llm_provider_ids, mcp_provider_ids};

        // Count LLM providers with configured API keys
        let llm_count = llm_provider_ids()
            .filter(|id| {
                let var = env_var(id);
                std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false)
            })
            .count();

        // Count MCP providers with configured credentials
        let mcp_count = mcp_provider_ids()
            .filter(|id| {
                let var = env_var(id);
                std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false)
            })
            .count();

        self.update_provider_counts(llm_count, mcp_count);

        // Secrets source: env vars + vault
        self.update_secrets_info("env vars", llm_count + mcp_count);

        // MCP servers: Will be updated when a workflow is loaded
        // For now, show current state (from state if any)

        // Packages: Future feature
    }

    /// Render a settings section box
    fn render_section(
        &self,
        frame: &mut Frame,
        area: Rect,
        section: SettingsSection,
        content: Vec<Line<'_>>,
        theme: &Theme,
    ) {
        let is_selected = self.section == section;
        let border_style = if is_selected {
            Style::default()
                .fg(theme.highlight)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" {} ", section.title()))
            .title_style(if is_selected {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_muted)
            });

        let paragraph = Paragraph::new(content)
            .block(block)
            .style(Style::default().fg(theme.text_primary));

        frame.render_widget(paragraph, area);
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for SettingsView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout: 5 sections with compact heights
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(3), // 1. Providers section
                Constraint::Min(3), // 2. MCP Servers section
                Constraint::Min(3), // 3. Secrets section
                Constraint::Min(3), // 4. Packages section
                Constraint::Min(3), // 5. Preferences section
                Constraint::Min(0), // Footer
            ])
            .split(area);

        // 1. PROVIDERS section - LLM + MCP status
        let providers_content = vec![
            Line::from(vec![
                Span::styled("  LLM: ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    format!("{}/7", self.llm_configured),
                    Style::default()
                        .fg(if self.llm_configured > 0 {
                            theme.highlight
                        } else {
                            theme.text_muted
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    " configured  │  MCP: ",
                    Style::default().fg(theme.text_muted),
                ),
                Span::styled(
                    format!("{}/6", self.mcp_configured),
                    Style::default()
                        .fg(if self.mcp_configured > 0 {
                            theme.highlight
                        } else {
                            theme.text_muted
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" configured", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("[Ctrl+P]", Style::default().fg(theme.highlight)),
                Span::styled(
                    " Configure providers",
                    Style::default().fg(theme.text_muted),
                ),
            ]),
        ];
        self.render_section(
            frame,
            chunks[0],
            SettingsSection::Providers,
            providers_content,
            theme,
        );

        // 2. MCP SERVERS section
        let mcp_servers_display = if self.active_mcp_servers.is_empty() {
            "none active".to_string()
        } else {
            self.active_mcp_servers.join(", ")
        };
        let mcp_content = vec![Line::from(vec![
            Span::styled("  Active: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                mcp_servers_display,
                Style::default()
                    .fg(if self.active_mcp_servers.is_empty() {
                        theme.text_muted
                    } else {
                        theme.highlight
                    })
                    .add_modifier(if self.active_mcp_servers.is_empty() {
                        Modifier::empty()
                    } else {
                        Modifier::BOLD
                    }),
            ),
        ])];
        self.render_section(
            frame,
            chunks[1],
            SettingsSection::McpServers,
            mcp_content,
            theme,
        );

        // 3. SECRETS section
        let secrets_content = vec![Line::from(vec![
            Span::styled("  Source: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                &self.secrets_info.source,
                Style::default()
                    .fg(theme.text_muted)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  │  Vault: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{} keys", self.secrets_info.vault_count),
                Style::default().fg(theme.text_primary),
            ),
        ])];
        self.render_section(
            frame,
            chunks[2],
            SettingsSection::Secrets,
            secrets_content,
            theme,
        );

        // 4. PACKAGES section
        let packages_display = if self.installed_packages.is_empty() {
            "none installed".to_string()
        } else {
            self.installed_packages.join(", ")
        };
        let packages_content = vec![Line::from(vec![
            Span::styled("  Installed: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                packages_display,
                Style::default().fg(if self.installed_packages.is_empty() {
                    theme.text_muted
                } else {
                    theme.text_primary
                }),
            ),
        ])];
        self.render_section(
            frame,
            chunks[3],
            SettingsSection::Packages,
            packages_content,
            theme,
        );

        // 5. PREFERENCES section - Theme + shortcuts
        let preferences_content = vec![
            Line::from(vec![
                Span::styled("  Theme: ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    &self.theme_name,
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  │  ", Style::default().fg(theme.text_muted)),
                Span::styled("[1]", Style::default().fg(theme.highlight)),
                Span::styled(" Light  ", Style::default().fg(theme.text_muted)),
                Span::styled("[2]", Style::default().fg(theme.highlight)),
                Span::styled(" Dark  ", Style::default().fg(theme.text_muted)),
                Span::styled("[3]", Style::default().fg(theme.highlight)),
                Span::styled(" Violet", Style::default().fg(theme.text_muted)),
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("[?]", Style::default().fg(theme.highlight)),
                Span::styled(" Help  ", Style::default().fg(theme.text_muted)),
                Span::styled("[Esc]", Style::default().fg(theme.highlight)),
                Span::styled(" Back", Style::default().fg(theme.text_muted)),
            ]),
        ];
        self.render_section(
            frame,
            chunks[4],
            SettingsSection::Preferences,
            preferences_content,
            theme,
        );

        // Footer with navigation hints
        if chunks[5].height > 0 {
            let footer = Paragraph::new(Line::from(vec![
                Span::styled("[Tab]", Style::default().fg(theme.highlight)),
                Span::styled(" Next  ", Style::default().fg(theme.text_muted)),
                Span::styled("[j/k]", Style::default().fg(theme.highlight)),
                Span::styled(" Navigate  ", Style::default().fg(theme.text_muted)),
                Span::styled("[1-3]", Style::default().fg(theme.highlight)),
                Span::styled(" Theme  ", Style::default().fg(theme.text_muted)),
                Span::styled("[Esc]", Style::default().fg(theme.highlight)),
                Span::styled(" Back", Style::default().fg(theme.text_muted)),
            ]))
            .style(Style::default().fg(theme.text_muted));
            frame.render_widget(footer, chunks[5]);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            // Escape returns to Studio
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Studio),

            // Tab/Shift+Tab cycles sections
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.section = self.section.prev();
                } else {
                    self.section = self.section.next();
                }
                ViewAction::None
            }
            KeyCode::BackTab => {
                self.section = self.section.prev();
                ViewAction::None
            }

            // j/k for vim-style navigation
            KeyCode::Char('j') | KeyCode::Down => {
                self.section = self.section.next();
                ViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.section = self.section.prev();
                ViewAction::None
            }

            // Theme shortcuts - direct selection
            KeyCode::Char('1') => ViewAction::SetTheme(CosmicVariant::CosmicLight),
            KeyCode::Char('2') => ViewAction::SetTheme(CosmicVariant::CosmicDark),
            KeyCode::Char('3') => ViewAction::SetTheme(CosmicVariant::CosmicViolet),

            // ? falls through to app-level Help mode

            // Enter on Providers opens provider verification modal
            KeyCode::Enter if self.section == SettingsSection::Providers => {
                ViewAction::VerifyProviders
            }

            // 'w' launches the setup wizard
            KeyCode::Char('w') => ViewAction::LaunchWizard,

            _ => ViewAction::None,
        }
    }

    fn on_enter(&mut self, _state: &mut TuiState) {
        // Refresh provider data when entering Settings view
        // This ensures data is fresh every time the user switches to Settings
        self.refresh_data();
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "Control • {} selected | [1-3] Theme • [w] Wizard",
            self.section.title()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    // ═══════════════════════════════════════════════════════════════════════════════
    // SettingsView Basic Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_settings_view_new() {
        let view = SettingsView::new();
        assert_eq!(view.section, SettingsSection::Providers);
        assert_eq!(view.provider_name, "Auto-detect");
        assert_eq!(view.llm_configured, 0);
        assert_eq!(view.mcp_configured, 0);
    }

    #[test]
    fn test_section_next() {
        // 5-section cycle: Providers → McpServers → Secrets → Packages → Preferences → Providers
        assert_eq!(
            SettingsSection::Providers.next(),
            SettingsSection::McpServers
        );
        assert_eq!(SettingsSection::McpServers.next(), SettingsSection::Secrets);
        assert_eq!(SettingsSection::Secrets.next(), SettingsSection::Packages);
        assert_eq!(
            SettingsSection::Packages.next(),
            SettingsSection::Preferences
        );
        assert_eq!(
            SettingsSection::Preferences.next(),
            SettingsSection::Providers
        );
    }

    #[test]
    fn test_section_prev() {
        // Reverse cycle: Providers → Preferences → Packages → Secrets → McpServers → Providers
        assert_eq!(
            SettingsSection::Providers.prev(),
            SettingsSection::Preferences
        );
        assert_eq!(
            SettingsSection::Preferences.prev(),
            SettingsSection::Packages
        );
        assert_eq!(SettingsSection::Packages.prev(), SettingsSection::Secrets);
        assert_eq!(SettingsSection::Secrets.prev(), SettingsSection::McpServers);
        assert_eq!(
            SettingsSection::McpServers.prev(),
            SettingsSection::Providers
        );
    }

    #[test]
    fn test_section_titles() {
        assert_eq!(SettingsSection::Providers.title(), "PROVIDERS");
        assert_eq!(SettingsSection::McpServers.title(), "MCP SERVERS");
        assert_eq!(SettingsSection::Secrets.title(), "SECRETS");
        assert_eq!(SettingsSection::Packages.title(), "PACKAGES");
        assert_eq!(SettingsSection::Preferences.title(), "PREFERENCES");
    }

    #[test]
    fn test_section_numbers() {
        assert_eq!(SettingsSection::Providers.number(), 1);
        assert_eq!(SettingsSection::McpServers.number(), 2);
        assert_eq!(SettingsSection::Secrets.number(), 3);
        assert_eq!(SettingsSection::Packages.number(), 4);
        assert_eq!(SettingsSection::Preferences.number(), 5);
    }

    #[test]
    fn test_section_all_constant() {
        assert_eq!(SettingsSection::ALL.len(), 5);
        assert_eq!(SettingsSection::ALL[0], SettingsSection::Providers);
        assert_eq!(SettingsSection::ALL[4], SettingsSection::Preferences);
    }

    #[test]
    fn test_update_provider() {
        let mut view = SettingsView::new();
        view.update_provider("Claude", "claude-sonnet-4-6");
        assert_eq!(view.provider_name, "Claude");
        assert_eq!(view.model_name, "claude-sonnet-4-6");
    }

    #[test]
    fn test_update_theme_name() {
        let mut view = SettingsView::new();
        view.update_theme_name("Light");
        assert_eq!(view.theme_name, "Light");
        view.update_theme_name("Solarized");
        assert_eq!(view.theme_name, "Solarized");
    }

    #[test]
    fn test_update_provider_counts() {
        let mut view = SettingsView::new();
        view.update_provider_counts(4, 2);
        assert_eq!(view.llm_configured, 4);
        assert_eq!(view.mcp_configured, 2);
    }

    #[test]
    fn test_update_mcp_servers() {
        let mut view = SettingsView::new();
        view.update_mcp_servers(vec!["novanet".into(), "perplexity".into()]);
        assert_eq!(view.active_mcp_servers.len(), 2);
        assert!(view.active_mcp_servers.contains(&"novanet".to_string()));
    }

    #[test]
    fn test_update_secrets_info() {
        let mut view = SettingsView::new();
        view.update_secrets_info("env vars", 5);
        assert_eq!(view.secrets_info.source, "env vars");
        assert_eq!(view.secrets_info.vault_count, 5);
    }

    #[test]
    fn test_update_packages() {
        let mut view = SettingsView::new();
        view.update_packages(vec!["@nika/core@1.0".into()]);
        assert_eq!(view.installed_packages.len(), 1);
    }

    #[test]
    fn test_status_line() {
        let view = SettingsView::new();
        let state = TuiState::new("test");
        assert!(view.status_line(&state).contains("PROVIDERS"));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // Key Handling Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_handle_key_escape_returns_studio() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Studio)));
    }

    #[test]
    fn test_handle_key_tab_cycles_sections() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        assert_eq!(view.section, SettingsSection::Providers);

        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);

        // Cycle through all 5 sections
        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::McpServers);

        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Secrets);

        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Packages);

        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Preferences);

        // Wraps back to Providers
        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Providers);
    }

    #[test]
    fn test_handle_key_shift_tab_cycles_backwards() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Preferences);
    }

    #[test]
    fn test_handle_key_question_does_nothing() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
    }

    #[test]
    fn test_handle_key_vim_navigation() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");

        // j moves to next section
        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        view.handle_key(key_j, &mut state);
        assert_eq!(view.section, SettingsSection::McpServers);

        // k moves to previous section
        let key_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        view.handle_key(key_k, &mut state);
        assert_eq!(view.section, SettingsSection::Providers);
    }

    #[test]
    fn test_handle_key_enter_on_providers() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        view.section = SettingsSection::Providers;
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::VerifyProviders));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // Theme Shortcut Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_handle_key_1_sets_light_theme() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(
            action,
            ViewAction::SetTheme(CosmicVariant::CosmicLight)
        ));
    }

    #[test]
    fn test_handle_key_2_sets_dark_theme() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(
            action,
            ViewAction::SetTheme(CosmicVariant::CosmicDark)
        ));
    }

    #[test]
    fn test_handle_key_3_sets_violet_theme() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(
            action,
            ViewAction::SetTheme(CosmicVariant::CosmicViolet)
        ));
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // SecretsInfo Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_secrets_info_default() {
        let info = SecretsInfo::default();
        assert!(info.source.is_empty());
        assert_eq!(info.vault_count, 0);
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // refresh_data Tests
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_refresh_data_updates_provider_counts() {
        let mut view = SettingsView::new();
        assert_eq!(view.llm_configured, 0, "Initial llm count should be 0");
        assert_eq!(view.mcp_configured, 0, "Initial mcp count should be 0");

        // refresh_data checks env vars - without any set, counts should remain 0
        view.refresh_data();

        // Without env vars, both should be 0
        // (actual count depends on what's in the environment)
        // Just verify it doesn't panic and updates secrets_info
        assert!(
            !view.secrets_info.source.is_empty(),
            "Source should be set after refresh"
        );
    }

    #[test]
    #[serial]
    fn test_refresh_data_with_env_var() {
        use std::env;

        let mut view = SettingsView::new();

        // Set a fake API key env var
        env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-key");

        view.refresh_data();

        // Should detect at least 1 LLM provider
        assert!(view.llm_configured >= 1, "Should detect ANTHROPIC_API_KEY");

        // Clean up
        env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_refresh_data_sets_secrets_source() {
        let mut view = SettingsView::new();
        view.refresh_data();

        assert_eq!(view.secrets_info.source, "env vars");
    }
}
