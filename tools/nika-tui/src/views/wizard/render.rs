// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Render methods for each wizard step.

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::theme::solarized;
use crate::theme::Theme;

use super::WizardView;

impl WizardView {
    /// Render the welcome step
    pub(super) fn render_welcome(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
    pub(super) fn render_providers(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
    pub(super) fn render_models(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
    pub(super) fn render_mcp_servers(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
    pub(super) fn render_editor_sync(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
    pub(super) fn render_verification(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
    pub(super) fn render_complete(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
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
}
