//! Wizard View for Nika Setup
//!
//! Full-screen setup wizard launched via `nika setup`.
//! NOT part of the 3-view TuiView navigation - runs standalone.
//!
//! # Option D Architecture (Hybrid)
//!
//! 1. `nika setup` → Launches wizard standalone
//! 2. First TUI run → Modal prompt to run setup if not completed
//! 3. Settings view → "Re-run Setup Wizard" button
//!
//! # Steps
//!
//! 0. Welcome - Introduction and overview
//! 1. Providers - Configure API keys for cloud LLMs
//! 2. Models - Download local models (mistral.rs)
//! 3. McpServers - Configure MCP server connections
//! 4. EditorSync - Enable editor integrations
//! 5. Verification - Run diagnostics
//! 6. Complete - Summary and exit

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::state::TuiState;
use crate::theme::solarized;
use crate::theme::Theme;
use crate::views::{View, ViewAction};
use crate::wizard::{WizardConfig, WizardState, WizardStep};

/// Provider item for the provider list
#[derive(Debug, Clone)]
struct ProviderItem {
    id: &'static str,
    name: &'static str,
    env_var: &'static str,
    configured: bool,
}

/// Editor item for the editor list
#[derive(Debug, Clone)]
struct EditorItem {
    #[cfg_attr(not(test), allow(dead_code))]
    id: &'static str,
    name: &'static str,
    enabled: bool,
}

/// Wizard View - Full-screen setup wizard
///
/// Standalone view launched by `nika setup`, not part of normal TUI navigation.
pub struct WizardView {
    /// Wizard state machine
    pub state: WizardState,
    /// List state for provider selection
    provider_list_state: ListState,
    /// List state for editor selection
    editor_list_state: ListState,
    /// List state for MCP server selection
    mcp_list_state: ListState,
    /// Available providers
    providers: Vec<ProviderItem>,
    /// Available editors
    editors: Vec<EditorItem>,
    /// Available MCP servers (subset of 100 aliases)
    mcp_servers: Vec<&'static str>,
    /// Input buffer for API key entry
    input_buffer: String,
    /// Whether we're in input mode
    input_mode: bool,
    /// Currently selected provider for input
    input_provider: Option<usize>,
}

impl Default for WizardView {
    fn default() -> Self {
        Self::new()
    }
}

impl WizardView {
    /// Create a new wizard view
    pub fn new() -> Self {
        let mut provider_list_state = ListState::default();
        provider_list_state.select(Some(0));

        let mut editor_list_state = ListState::default();
        editor_list_state.select(Some(0));

        let mut mcp_list_state = ListState::default();
        mcp_list_state.select(Some(0));

        Self {
            state: WizardState::new(),
            provider_list_state,
            editor_list_state,
            mcp_list_state,
            providers: vec![
                ProviderItem {
                    id: "anthropic",
                    name: "Anthropic (Claude)",
                    env_var: "ANTHROPIC_API_KEY",
                    configured: std::env::var("ANTHROPIC_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "openai",
                    name: "OpenAI (GPT-4)",
                    env_var: "OPENAI_API_KEY",
                    configured: std::env::var("OPENAI_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "mistral",
                    name: "Mistral AI",
                    env_var: "MISTRAL_API_KEY",
                    configured: std::env::var("MISTRAL_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "groq",
                    name: "Groq",
                    env_var: "GROQ_API_KEY",
                    configured: std::env::var("GROQ_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "deepseek",
                    name: "DeepSeek",
                    env_var: "DEEPSEEK_API_KEY",
                    configured: std::env::var("DEEPSEEK_API_KEY").is_ok(),
                },
                ProviderItem {
                    id: "gemini",
                    name: "Google Gemini",
                    env_var: "GEMINI_API_KEY",
                    configured: std::env::var("GEMINI_API_KEY").is_ok(),
                },
            ],
            editors: vec![
                EditorItem {
                    id: "claude-code",
                    name: "Claude Code",
                    enabled: false,
                },
                EditorItem {
                    id: "cursor",
                    name: "Cursor",
                    enabled: false,
                },
                EditorItem {
                    id: "windsurf",
                    name: "Windsurf",
                    enabled: false,
                },
                EditorItem {
                    id: "vscode",
                    name: "VS Code",
                    enabled: false,
                },
            ],
            mcp_servers: vec![
                "neo4j",
                "github",
                "slack",
                "perplexity",
                "firecrawl",
                "filesystem",
                "postgres",
                "sqlite",
            ],
            input_buffer: String::new(),
            input_mode: false,
            input_provider: None,
        }
    }

    /// Check if wizard has been completed before
    /// Used for first-run detection in main TUI
    pub fn was_completed() -> bool {
        let config_path = dirs::home_dir()
            .map(|h| h.join(".nika").join("wizard.json"))
            .unwrap_or_default();

        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(config) = serde_json::from_str::<WizardConfig>(&content) {
                    return config.completed;
                }
            }
        }
        false
    }

    /// Save wizard completion state
    pub fn save_completion(&self) -> std::io::Result<()> {
        let config_path = dirs::home_dir()
            .map(|h| h.join(".nika").join("wizard.json"))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No home dir"))?;

        // Create .nika directory if needed
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config = self.state.to_config();
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, json)?;
        Ok(())
    }

    /// Render the welcome step
    fn render_welcome(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let ascii_art = r#"
    ███╗   ██╗██╗██╗  ██╗ █████╗
    ████╗  ██║██║██║ ██╔╝██╔══██╗
    ██╔██╗ ██║██║█████╔╝ ███████║
    ██║╚██╗██║██║██╔═██╗ ██╔══██║
    ██║ ╚████║██║██║  ██╗██║  ██║
    ╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝  ╚═╝
        "#;

        let welcome_text = vec![
            Line::from(""),
            Line::from(Span::styled(
                ascii_art,
                Style::default()
                    .fg(solarized::CYAN)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Welcome to Nika Setup Wizard!",
                Style::default()
                    .fg(solarized::YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("This wizard will help you configure:"),
            Line::from(""),
            Line::from(vec![
                Span::styled("  1. ", Style::default().fg(solarized::BLUE)),
                Span::raw("Cloud LLM providers (API keys)"),
            ]),
            Line::from(vec![
                Span::styled("  2. ", Style::default().fg(solarized::BLUE)),
                Span::raw("Local models (mistral.rs)"),
            ]),
            Line::from(vec![
                Span::styled("  3. ", Style::default().fg(solarized::BLUE)),
                Span::raw("MCP server connections"),
            ]),
            Line::from(vec![
                Span::styled("  4. ", Style::default().fg(solarized::BLUE)),
                Span::raw("Editor integrations"),
            ]),
            Line::from(vec![
                Span::styled("  5. ", Style::default().fg(solarized::BLUE)),
                Span::raw("Verify your setup"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press [Enter] to begin or [Esc] to quit",
                Style::default().fg(solarized::BASE1),
            )),
        ];

        let paragraph = Paragraph::new(welcome_text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    /// Render the providers step
    fn render_providers(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(10),   // List
                Constraint::Length(3), // Help
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                "Cloud Providers",
                Style::default()
                    .fg(solarized::YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Configure API keys for cloud LLM providers",
                Style::default().fg(solarized::BASE1),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        // Provider list
        let items: Vec<ListItem> = self
            .providers
            .iter()
            .map(|p| {
                let status = if p.configured {
                    Span::styled("[✓] ", Style::default().fg(solarized::GREEN))
                } else {
                    Span::styled("[ ] ", Style::default().fg(solarized::BASE01))
                };
                let name = Span::styled(p.name, Style::default().fg(solarized::BASE0));
                let env = Span::styled(
                    format!(" ({})", p.env_var),
                    Style::default().fg(solarized::BASE01),
                );
                ListItem::new(Line::from(vec![status, name, env]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(solarized::BASE01))
                    .title(" Providers "),
            )
            .highlight_style(
                Style::default()
                    .bg(solarized::BASE02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, chunks[1], &mut self.provider_list_state);

        // Help text
        let help = if self.input_mode {
            Paragraph::new(vec![Line::from(vec![
                Span::styled("Enter API key: ", Style::default().fg(solarized::BASE1)),
                Span::styled(&self.input_buffer, Style::default().fg(solarized::CYAN)),
                Span::styled("█", Style::default().fg(solarized::CYAN)),
            ])])
        } else {
            Paragraph::new(vec![Line::from(vec![
                Span::styled("[Enter] ", Style::default().fg(solarized::BLUE)),
                Span::raw("Configure  "),
                Span::styled("[↑↓] ", Style::default().fg(solarized::BLUE)),
                Span::raw("Navigate  "),
                Span::styled("[Tab] ", Style::default().fg(solarized::BLUE)),
                Span::raw("Skip  "),
                Span::styled("[→] ", Style::default().fg(solarized::BLUE)),
                Span::raw("Next"),
            ])])
        }
        .alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    /// Render the models step
    fn render_models(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                "Local Models",
                Style::default()
                    .fg(solarized::YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Download models for local inference (optional)",
                Style::default().fg(solarized::BASE1),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        let models_info = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Recommended models:",
                Style::default().fg(solarized::BASE0),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  • ", Style::default().fg(solarized::CYAN)),
                Span::styled("llama3.2:1b", Style::default().fg(solarized::GREEN)),
                Span::raw(" - Fast, lightweight (1.3 GB)"),
            ]),
            Line::from(vec![
                Span::styled("  • ", Style::default().fg(solarized::CYAN)),
                Span::styled("qwen3:8b", Style::default().fg(solarized::YELLOW)),
                Span::raw(" - Balanced performance (4.9 GB)"),
            ]),
            Line::from(vec![
                Span::styled("  • ", Style::default().fg(solarized::CYAN)),
                Span::styled("mistral:7b", Style::default().fg(solarized::ORANGE)),
                Span::raw(" - Quality inference (4.1 GB)"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Download with: nika model pull <name>",
                Style::default().fg(solarized::BASE01),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Skip this step if you only use cloud providers.",
                Style::default()
                    .fg(solarized::BASE01)
                    .add_modifier(Modifier::ITALIC),
            )),
        ];

        let paragraph = Paragraph::new(models_info)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(solarized::BASE01))
                    .title(" Available Models "),
            )
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, chunks[1]);

        let help = Paragraph::new(vec![Line::from(vec![
            Span::styled("[Tab] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Skip  "),
            Span::styled("[→] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Next  "),
            Span::styled("[←] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Back"),
        ])])
        .alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    /// Render the MCP servers step
    fn render_mcp_servers(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                "MCP Servers",
                Style::default()
                    .fg(solarized::YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Configure Model Context Protocol servers",
                Style::default().fg(solarized::BASE1),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        let items: Vec<ListItem> = self
            .mcp_servers
            .iter()
            .map(|name| {
                let enabled = self.state.mcp_servers.contains(&name.to_string());
                let status = if enabled {
                    Span::styled("[✓] ", Style::default().fg(solarized::GREEN))
                } else {
                    Span::styled("[ ] ", Style::default().fg(solarized::BASE01))
                };
                let name_span = Span::styled(*name, Style::default().fg(solarized::BASE0));
                ListItem::new(Line::from(vec![status, name_span]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(solarized::BASE01))
                    .title(" MCP Servers (48 available) "),
            )
            .highlight_style(
                Style::default()
                    .bg(solarized::BASE02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, chunks[1], &mut self.mcp_list_state);

        let help = Paragraph::new(vec![Line::from(vec![
            Span::styled("[Space] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Toggle  "),
            Span::styled("[↑↓] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Navigate  "),
            Span::styled("[Tab] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Skip  "),
            Span::styled("[→] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Next"),
        ])])
        .alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    /// Render the editor sync step
    fn render_editor_sync(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                "Editor Sync",
                Style::default()
                    .fg(solarized::YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Enable Nika integration for your editors",
                Style::default().fg(solarized::BASE1),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        let items: Vec<ListItem> = self
            .editors
            .iter()
            .map(|e| {
                let status = if e.enabled {
                    Span::styled("[✓] ", Style::default().fg(solarized::GREEN))
                } else {
                    Span::styled("[ ] ", Style::default().fg(solarized::BASE01))
                };
                let name = Span::styled(e.name, Style::default().fg(solarized::BASE0));
                ListItem::new(Line::from(vec![status, name]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(solarized::BASE01))
                    .title(" Editors "),
            )
            .highlight_style(
                Style::default()
                    .bg(solarized::BASE02)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, chunks[1], &mut self.editor_list_state);

        let help = Paragraph::new(vec![Line::from(vec![
            Span::styled("[Space] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Toggle  "),
            Span::styled("[↑↓] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Navigate  "),
            Span::styled("[Tab] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Skip  "),
            Span::styled("[→] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Next"),
        ])])
        .alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    /// Render the verification step
    fn render_verification(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                "Verification",
                Style::default()
                    .fg(solarized::YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Checking your configuration...",
                Style::default().fg(solarized::BASE1),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(title, chunks[0]);

        let mut results = vec![Line::from("")];

        // Check providers
        let configured_count = self.providers.iter().filter(|p| p.configured).count();
        results.push(Line::from(vec![
            if configured_count > 0 {
                Span::styled("✓ ", Style::default().fg(solarized::GREEN))
            } else {
                Span::styled("⚠ ", Style::default().fg(solarized::YELLOW))
            },
            Span::styled(
                format!("Cloud Providers: {}/6 configured", configured_count),
                Style::default().fg(solarized::BASE0),
            ),
        ]));

        // Check MCP servers
        let mcp_count = self.state.mcp_servers.len();
        results.push(Line::from(vec![
            if mcp_count > 0 {
                Span::styled("✓ ", Style::default().fg(solarized::GREEN))
            } else {
                Span::styled("○ ", Style::default().fg(solarized::BASE01))
            },
            Span::styled(
                format!("MCP Servers: {} enabled", mcp_count),
                Style::default().fg(solarized::BASE0),
            ),
        ]));

        // Check editors
        let editor_count = self.editors.iter().filter(|e| e.enabled).count();
        results.push(Line::from(vec![
            if editor_count > 0 {
                Span::styled("✓ ", Style::default().fg(solarized::GREEN))
            } else {
                Span::styled("○ ", Style::default().fg(solarized::BASE01))
            },
            Span::styled(
                format!("Editor Sync: {} enabled", editor_count),
                Style::default().fg(solarized::BASE0),
            ),
        ]));

        results.push(Line::from(""));
        results.push(Line::from(Span::styled(
            if configured_count > 0 {
                "Ready to use Nika!"
            } else {
                "Configure at least one provider to use Nika."
            },
            Style::default()
                .fg(if configured_count > 0 {
                    solarized::GREEN
                } else {
                    solarized::YELLOW
                })
                .add_modifier(Modifier::BOLD),
        )));

        let paragraph = Paragraph::new(results)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(solarized::BASE01))
                    .title(" Results "),
            )
            .alignment(Alignment::Left);
        frame.render_widget(paragraph, chunks[1]);

        let help = Paragraph::new(vec![Line::from(vec![
            Span::styled("[→] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Complete  "),
            Span::styled("[←] ", Style::default().fg(solarized::BLUE)),
            Span::raw("Back"),
        ])])
        .alignment(Alignment::Center);
        frame.render_widget(help, chunks[2]);
    }

    /// Render the complete step
    fn render_complete(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let complete_text = vec![
            Line::from(""),
            Line::from(Span::styled(
                "🎉 Setup Complete!",
                Style::default()
                    .fg(solarized::GREEN)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Nika is ready to use.",
                Style::default().fg(solarized::BASE0),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Quick Start:",
                Style::default().fg(solarized::YELLOW),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  nika", Style::default().fg(solarized::CYAN)),
                Span::raw("              Launch TUI"),
            ]),
            Line::from(vec![
                Span::styled("  nika chat", Style::default().fg(solarized::CYAN)),
                Span::raw("         Start chatting"),
            ]),
            Line::from(vec![
                Span::styled("  nika studio", Style::default().fg(solarized::CYAN)),
                Span::raw("       Open workflow editor"),
            ]),
            Line::from(vec![
                Span::styled("  nika run <file>", Style::default().fg(solarized::CYAN)),
                Span::raw("   Execute workflow"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press [Enter] to exit wizard",
                Style::default().fg(solarized::BASE1),
            )),
        ];

        let paragraph = Paragraph::new(complete_text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    /// Render progress bar at bottom
    fn render_progress(&self, frame: &mut Frame, area: Rect) {
        let progress = self.state.progress_percentage();
        let step_num = self.state.current_step.number();
        let total = 6;

        let label = format!(
            "Step {} of {}: {}",
            step_num.min(total),
            total,
            self.state.current_step.title()
        );

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(solarized::CYAN).bg(solarized::BASE02))
            .percent(progress as u16)
            .label(Span::styled(label, Style::default().fg(solarized::BASE1)));

        frame.render_widget(gauge, area);
    }

    /// Handle key events for providers step
    fn handle_providers_key(&mut self, key: KeyEvent) -> ViewAction {
        if self.input_mode {
            match key.code {
                KeyCode::Esc => {
                    self.input_mode = false;
                    self.input_buffer.clear();
                    self.input_provider = None;
                }
                KeyCode::Enter => {
                    // Would store API key here (via keyring)
                    // For now, just exit input mode
                    if let Some(idx) = self.input_provider {
                        if idx < self.providers.len() {
                            // Mark as configured (in real impl, store to keychain)
                            self.providers[idx].configured = true;
                            self.state
                                .provider_keys
                                .insert(self.providers[idx].id.to_string(), true);
                        }
                    }
                    self.input_mode = false;
                    self.input_buffer.clear();
                    self.input_provider = None;
                }
                KeyCode::Backspace => {
                    self.input_buffer.pop();
                }
                KeyCode::Char(c) => {
                    self.input_buffer.push(c);
                }
                _ => {}
            }
            return ViewAction::None;
        }

        match key.code {
            KeyCode::Up | KeyCode::Char('k') if !self.providers.is_empty() => {
                let i = self.provider_list_state.selected().unwrap_or(0);
                let new_i = if i == 0 {
                    self.providers.len() - 1
                } else {
                    i - 1
                };
                self.provider_list_state.select(Some(new_i));
            }
            KeyCode::Down | KeyCode::Char('j') if !self.providers.is_empty() => {
                let i = self.provider_list_state.selected().unwrap_or(0);
                let new_i = (i + 1) % self.providers.len();
                self.provider_list_state.select(Some(new_i));
            }
            KeyCode::Enter => {
                // Enter input mode for selected provider
                self.input_mode = true;
                self.input_provider = self.provider_list_state.selected();
            }
            KeyCode::Right | KeyCode::Tab => {
                self.state.advance();
            }
            KeyCode::Left => {
                self.state.go_back();
            }
            _ => {}
        }
        ViewAction::None
    }

    /// Handle key events for MCP servers step
    fn handle_mcp_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if !self.mcp_servers.is_empty() => {
                let i = self.mcp_list_state.selected().unwrap_or(0);
                let new_i = if i == 0 {
                    self.mcp_servers.len() - 1
                } else {
                    i - 1
                };
                self.mcp_list_state.select(Some(new_i));
            }
            KeyCode::Down | KeyCode::Char('j') if !self.mcp_servers.is_empty() => {
                let i = self.mcp_list_state.selected().unwrap_or(0);
                let new_i = (i + 1) % self.mcp_servers.len();
                self.mcp_list_state.select(Some(new_i));
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                // Toggle selection
                if let Some(idx) = self.mcp_list_state.selected() {
                    if idx < self.mcp_servers.len() {
                        let server = self.mcp_servers[idx].to_string();
                        if self.state.mcp_servers.contains(&server) {
                            self.state.mcp_servers.retain(|s| s != &server);
                        } else {
                            self.state.mcp_servers.push(server);
                        }
                    }
                }
            }
            KeyCode::Right | KeyCode::Tab => {
                self.state.advance();
            }
            KeyCode::Left => {
                self.state.go_back();
            }
            _ => {}
        }
        ViewAction::None
    }

    /// Handle key events for editor sync step
    fn handle_editor_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if !self.editors.is_empty() => {
                let i = self.editor_list_state.selected().unwrap_or(0);
                let new_i = if i == 0 {
                    self.editors.len() - 1
                } else {
                    i - 1
                };
                self.editor_list_state.select(Some(new_i));
            }
            KeyCode::Down | KeyCode::Char('j') if !self.editors.is_empty() => {
                let i = self.editor_list_state.selected().unwrap_or(0);
                let new_i = (i + 1) % self.editors.len();
                self.editor_list_state.select(Some(new_i));
            }
            KeyCode::Char(' ') | KeyCode::Enter => {
                // Toggle selection
                if let Some(idx) = self.editor_list_state.selected() {
                    if idx < self.editors.len() {
                        self.editors[idx].enabled = !self.editors[idx].enabled;
                    }
                }
            }
            KeyCode::Right | KeyCode::Tab => {
                self.state.advance();
            }
            KeyCode::Left => {
                self.state.go_back();
            }
            _ => {}
        }
        ViewAction::None
    }
}

impl View for WizardView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, _theme: &Theme) {
        // Clear background
        frame.render_widget(Clear, area);

        // Main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Content
                Constraint::Length(2), // Progress
            ])
            .split(area);

        // Header
        let header = Paragraph::new(vec![Line::from(vec![
            Span::styled("🦋 ", Style::default()),
            Span::styled(
                "NIKA SETUP WIZARD",
                Style::default()
                    .fg(solarized::CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
        ])])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(solarized::BASE01)),
        );
        frame.render_widget(header, chunks[0]);

        // Content area with padding
        let content_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ])
            .split(chunks[1])[1];

        // Render step content
        match self.state.current_step {
            WizardStep::Welcome => self.render_welcome(frame, content_area, _theme),
            WizardStep::Providers => self.render_providers(frame, content_area, _theme),
            WizardStep::Models => self.render_models(frame, content_area, _theme),
            WizardStep::McpServers => self.render_mcp_servers(frame, content_area, _theme),
            WizardStep::EditorSync => self.render_editor_sync(frame, content_area, _theme),
            WizardStep::Verification => self.render_verification(frame, content_area, _theme),
            WizardStep::Complete => self.render_complete(frame, content_area, _theme),
        }

        // Progress bar
        self.render_progress(frame, chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // Global keys
        match key.code {
            KeyCode::Esc => {
                if self.input_mode {
                    self.input_mode = false;
                    self.input_buffer.clear();
                    return ViewAction::None;
                }
                return ViewAction::Quit;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return ViewAction::Quit;
            }
            _ => {}
        }

        // Step-specific handling
        match self.state.current_step {
            WizardStep::Welcome => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Right) {
                    self.state.advance();
                }
            }
            WizardStep::Providers => {
                return self.handle_providers_key(key);
            }
            WizardStep::Models => match key.code {
                KeyCode::Right | KeyCode::Tab | KeyCode::Enter => {
                    self.state.advance();
                }
                KeyCode::Left => {
                    self.state.go_back();
                }
                _ => {}
            },
            WizardStep::McpServers => {
                return self.handle_mcp_key(key);
            }
            WizardStep::EditorSync => {
                return self.handle_editor_key(key);
            }
            WizardStep::Verification => match key.code {
                KeyCode::Right | KeyCode::Enter => {
                    self.state.advance();
                }
                KeyCode::Left => {
                    self.state.go_back();
                }
                _ => {}
            },
            WizardStep::Complete => {
                if matches!(key.code, KeyCode::Enter) {
                    // Save completion state
                    let _ = self.save_completion();
                    return ViewAction::Quit;
                }
            }
        }

        ViewAction::None
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "[Wizard] Step {}: {} | Progress: {}%",
            self.state.current_step.number(),
            self.state.current_step.title(),
            self.state.progress_percentage()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_view_new() {
        let view = WizardView::new();
        assert_eq!(view.state.current_step, WizardStep::Welcome);
        assert_eq!(view.providers.len(), 6);
        assert_eq!(view.editors.len(), 4);
    }

    #[test]
    fn test_wizard_view_default() {
        let view = WizardView::default();
        assert_eq!(view.state.current_step, WizardStep::Welcome);
    }

    #[test]
    fn test_wizard_view_providers_count() {
        let view = WizardView::new();
        assert_eq!(view.providers.len(), 6);
        assert_eq!(view.providers[0].id, "anthropic");
        assert_eq!(view.providers[5].id, "gemini");
    }

    #[test]
    fn test_wizard_view_editors_count() {
        let view = WizardView::new();
        assert_eq!(view.editors.len(), 4);
        assert_eq!(view.editors[0].id, "claude-code");
    }

    #[test]
    fn test_wizard_view_mcp_servers() {
        let view = WizardView::new();
        assert!(!view.mcp_servers.is_empty());
        assert!(view.mcp_servers.contains(&"neo4j"));
    }

    #[test]
    fn test_wizard_view_status_line() {
        let view = WizardView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("Wizard"));
        assert!(status.contains("Welcome"));
    }

    #[test]
    fn test_wizard_view_input_mode_default_false() {
        let view = WizardView::new();
        assert!(!view.input_mode);
        assert!(view.input_buffer.is_empty());
    }

    #[test]
    fn test_wizard_view_list_states_initialized() {
        let view = WizardView::new();
        assert_eq!(view.provider_list_state.selected(), Some(0));
        assert_eq!(view.editor_list_state.selected(), Some(0));
        assert_eq!(view.mcp_list_state.selected(), Some(0));
    }
}
