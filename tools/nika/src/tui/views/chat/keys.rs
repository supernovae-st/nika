//! Key Handler Methods for Chat View
//!
//! Specialized key event handlers for overlay modes.
//! Extracted from mod.rs as part of Phase A1 refactoring.

use crossterm::event::{KeyCode, KeyEvent};

use super::ChatView;
use crate::tui::command::{Command, ExportFormat, HELP_TEXT};
use crate::tui::views::ViewAction;
use crate::tui::widgets::provider_modal::ModalEventHandler;

use super::hints::{detect_verb_in_input, verb_placeholder};

// ═══════════════════════════════════════════════════════════════════════════════
// Key Handler Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Handle key events when help overlay is visible
    pub(super) fn handle_help_overlay_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            // Escape, ?, F1 = Close help
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => {
                self.help_overlay.hide();
                ViewAction::None
            }
            // j/Down = Scroll down
            KeyCode::Char('j') | KeyCode::Down => {
                // Max scroll based on content (HELP_SECTIONS has ~30 lines)
                self.help_overlay.scroll_down(30);
                ViewAction::None
            }
            // k/Up = Scroll up
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_overlay.scroll_up();
                ViewAction::None
            }
            // g = Scroll to top
            KeyCode::Char('g') => {
                self.help_overlay.scroll = 0;
                ViewAction::None
            }
            // G = Scroll to bottom
            KeyCode::Char('G') => {
                self.help_overlay.scroll = 30;
                ViewAction::None
            }
            // PageUp
            KeyCode::PageUp => {
                self.help_overlay.scroll_page_up(10);
                ViewAction::None
            }
            // PageDown
            KeyCode::PageDown => {
                self.help_overlay.scroll_page_down(30, 10);
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    /// Handle key events when command palette is visible
    pub(super) fn handle_palette_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                self.command_palette.close();
                ViewAction::None
            }
            KeyCode::Enter => {
                if let Some(cmd_id) = self.command_palette.execute_selected() {
                    // Execute the selected command
                    self.input = tui_input::Input::new(format!("/{}", cmd_id));
                    self.input.handle(tui_input::InputRequest::GoToEnd);
                    // Trigger submit with the command
                    if let Some(message) = self.submit() {
                        let cmd = Command::parse(&message);
                        return match cmd {
                            Command::Help => {
                                self.add_nika_message(HELP_TEXT.to_string(), None);
                                ViewAction::None
                            }
                            Command::Clear => ViewAction::ChatClear,
                            Command::Exec { command } => ViewAction::ChatExec(command),
                            Command::Fetch { url, method } => ViewAction::ChatFetch(url, method),
                            Command::FetchError {
                                error,
                                hint,
                                example,
                            } => {
                                // v0.8.2: Show helpful error message
                                self.add_nika_message(
                                    format!("{}\n{}\n{}", error, hint, example),
                                    None,
                                );
                                ViewAction::None
                            }
                            Command::Invoke {
                                tool,
                                server,
                                params,
                            } => ViewAction::ChatInvoke(tool, server, params),
                            Command::Agent {
                                goal,
                                max_turns,
                                mcp_servers,
                            } => ViewAction::ChatAgent(
                                goal,
                                max_turns,
                                self.deep_thinking,
                                mcp_servers,
                            ),
                            Command::Mcp { action } => ViewAction::ChatMcp(action),
                            Command::Model { provider } => ViewAction::ChatModelSwitch(provider),
                            Command::Export { format, path } => {
                                // v0.13: Export chat to JSON or YAML file
                                let result = match format {
                                    ExportFormat::Json => self.export_session(path.as_deref()),
                                    ExportFormat::Yaml => self.export_session_yaml(path.as_deref()),
                                };
                                match result {
                                    Ok(filepath) => {
                                        let format_name = match format {
                                            ExportFormat::Json => "JSON",
                                            ExportFormat::Yaml => "YAML workflow",
                                        };
                                        self.add_system_message(format!(
                                            "📤 Chat exported as {} to: {}",
                                            format_name, filepath
                                        ));
                                    }
                                    Err(e) => {
                                        self.add_system_message(format!("❌ Export failed: {}", e));
                                    }
                                }
                                ViewAction::None
                            }
                            Command::Infer { prompt } | Command::Chat { message: prompt } => {
                                ViewAction::ChatInfer(prompt)
                            }
                        };
                    }
                }
                ViewAction::None
            }
            KeyCode::Up => {
                self.command_palette.select_prev();
                ViewAction::None
            }
            KeyCode::Down => {
                self.command_palette.select_next();
                ViewAction::None
            }
            KeyCode::Char(c) => {
                self.command_palette.input_char(c);
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.command_palette.backspace();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    /// Handle key events for Provider Modal (full management)
    pub(super) fn handle_provider_modal_key(&mut self, key: KeyEvent) -> ViewAction {
        let result = ModalEventHandler::handle(&mut self.provider_modal, key);

        // If the modal requested to close
        if let Some(action) = &result.action {
            use crate::tui::widgets::ModalAction;
            match action {
                ModalAction::Close => {
                    self.provider_modal.visible = false;
                }
                ModalAction::RefreshProviders => {
                    return ViewAction::VerifyProviders;
                }
                ModalAction::RefreshOllamaModels => {
                    self.add_system_message("🔄 Refreshing Ollama models...".to_string());
                    return ViewAction::RefreshOllamaModels;
                }
                ModalAction::TestApiKey { provider } => {
                    self.add_system_message(format!("🔑 Testing {} API key...", provider));
                    return ViewAction::VerifyProviders;
                }
                ModalAction::PullModel { model } => {
                    self.add_system_message(format!("📥 Pulling model: {}...", model));
                    return ViewAction::PullOllamaModel(model.clone());
                }
                ModalAction::DeleteModel { model } => {
                    self.add_system_message(format!("🗑️ Deleting model: {}...", model));
                    return ViewAction::DeleteOllamaModel(model.clone());
                }
                ModalAction::SaveAndTestApiKey { provider, key } => {
                    use crate::tui::widgets::provider_modal::{validate_key_format, SpnKeyring};
                    // Validate format first
                    if let Err(e) = validate_key_format(provider, key) {
                        self.add_system_message(format!("❌ Invalid key format: {}", e));
                        return ViewAction::None;
                    }
                    // Save to keyring
                    match SpnKeyring::set(provider, key) {
                        Ok(()) => {
                            self.add_system_message(format!(
                                "✅ Saved {} API key to keychain",
                                provider
                            ));
                            // Trigger verification
                            return ViewAction::VerifyProviders;
                        }
                        Err(e) => {
                            self.add_system_message(format!("❌ Failed to save key: {}", e));
                        }
                    }
                }
                ModalAction::SaveApiKey { provider, key } => {
                    use crate::tui::widgets::provider_modal::{validate_key_format, SpnKeyring};
                    // Validate format first
                    if let Err(e) = validate_key_format(provider, key) {
                        self.add_system_message(format!("❌ Invalid key format: {}", e));
                        return ViewAction::None;
                    }
                    // Save silently (no verification trigger)
                    match SpnKeyring::set(provider, key) {
                        Ok(()) => {
                            self.add_system_message(format!(
                                "✅ Saved {} API key to keychain",
                                provider
                            ));
                            self.provider_modal.visible = false;
                        }
                        Err(e) => {
                            self.add_system_message(format!("❌ Failed to save key: {}", e));
                        }
                    }
                }
                ModalAction::SelectProvider { provider, model } => {
                    // v0.12.2: Actually switch provider via ChatModelSwitch
                    use crate::tui::command::ModelProvider;
                    self.provider_modal.visible = false;
                    if let Some(model_provider) = ModelProvider::from_name(provider) {
                        self.add_system_message(format!(
                            "🔄 Switching to {} ({})",
                            provider, model
                        ));
                        return ViewAction::ChatModelSwitch(model_provider);
                    } else {
                        self.add_system_message(format!(
                            "❌ Unknown provider: {} - available: claude, openai, mistral, groq, deepseek, ollama",
                            provider
                        ));
                    }
                }
                ModalAction::CheckProvider { provider } => {
                    self.add_system_message(format!("🔍 Checking {} connection...", provider));
                    return ViewAction::VerifyProviders;
                }
            }
        }

        ViewAction::None
    }

    /// v0.8.2: Try to autocomplete verb command with Tab
    /// Returns Some(new_input) if autocomplete happened, None otherwise
    pub(super) fn try_verb_autocomplete(&self) -> Option<String> {
        let input = self.input.value();

        if let Some((_verb_len, verb_color, is_complete, full_verb)) = detect_verb_in_input(input) {
            if !is_complete {
                // Partial match → complete the verb name
                return Some(format!("/{} ", full_verb));
            } else {
                // Complete verb → check if we should insert placeholder
                let rest = if input.len() > full_verb.len() + 1 {
                    &input[full_verb.len() + 1..] // +1 for /
                } else {
                    ""
                };
                if rest.trim().is_empty() {
                    // Insert first placeholder example
                    let placeholder = verb_placeholder(&verb_color, 0);
                    return Some(format!("/{} {}", full_verb, placeholder));
                }
            }
        }
        None
    }
}
