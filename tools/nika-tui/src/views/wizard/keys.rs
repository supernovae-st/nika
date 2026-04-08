// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Key event handlers for wizard steps.

use crossterm::event::{KeyCode, KeyEvent};

use crate::views::ViewAction;

use super::WizardView;

impl WizardView {
    /// Handle key events for providers step
    pub(super) fn handle_providers_key(&mut self, key: KeyEvent) -> ViewAction {
        if self.input_mode {
            match key.code {
                KeyCode::Esc => {
                    self.input_mode = false;
                    self.input_buffer.clear();
                    self.input_provider = None;
                }
                KeyCode::Enter => {
                    // Would store API key here (via vault)
                    // For now, just exit input mode
                    if let Some(idx) = self.input_provider {
                        if idx < self.providers.len() {
                            // Mark as configured (in real impl, store to vault)
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
    pub(super) fn handle_mcp_key(&mut self, key: KeyEvent) -> ViewAction {
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
    pub(super) fn handle_editor_key(&mut self, key: KeyEvent) -> ViewAction {
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
