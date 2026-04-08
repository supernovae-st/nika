// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command Handling for Chat View
//!
//! Contains submit, system command processing, and history navigation.

use tui_input::{Input, InputRequest};

use super::{ChatView, ParsedInput, SystemCommand};

// ═══════════════════════════════════════════════════════════════════════════════
// Command Handling
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Submit current input
    /// Returns Some(message) if it should be sent to the agent,
    /// or None if it was a system command handled internally
    pub fn submit(&mut self) -> Option<String> {
        if self.input.value().trim().is_empty() {
            return None;
        }
        let message = self.input.value().to_string();

        // Check for system commands (handled internally)
        match ParsedInput::parse(&message) {
            ParsedInput::System(cmd) => {
                self.handle_system_command(cmd);
                self.input.reset();
                self.update_mode_from_input(); // Preserve mode after clear
                return None;
            }
            ParsedInput::PartialPrefix(_) => {
                // User typing a command prefix, don't submit
                return None;
            }
            ParsedInput::Verb { .. } => {
                // Verb command - check if explicit (starts with /)
                // If explicit, TaskBox will display the command visually,
                // so we don't need a user message bubble (avoids duplication)
            }
            _ => {
                // Empty or other - don't process
                return None;
            }
        }

        // Only add user message bubble for implicit messages (no / prefix)
        // Explicit verb commands like /exec, /infer etc. are shown in TaskBox
        let is_explicit_verb_command = message.starts_with('/');
        if !is_explicit_verb_command {
            self.add_user_message(message.clone());
        }
        self.input.reset();
        self.update_mode_from_input(); // Preserve mode after submit
        Some(message)
    }

    /// Handle system commands (internal, not sent to agent)
    fn handle_system_command(&mut self, cmd: SystemCommand) {
        match cmd {
            SystemCommand::Clear => {
                self.messages.clear();
                self.add_system_message("Conversation cleared.".to_string());
            }
            SystemCommand::Help => {
                self.add_system_message(
                    "Commands:\n\
                     /clear - Clear conversation\n\
                     /help - Show this help\n\
                     /yaml - Toggle YAML view\n\
                     /thinking - Toggle deep thinking mode\n\
                     /model <name> - Change model\n\
                     /provider <name> - Change provider\n\n\
                     Verbs:\n\
                     /infer <prompt> - LLM generation (default)\n\
                     /exec <cmd> - Shell command\n\
                     /fetch <url> - HTTP request\n\
                     /invoke <tool> - MCP tool call\n\
                     /agent <prompt> - Agentic loop"
                        .to_string(),
                );
            }
            SystemCommand::Yaml => {
                self.show_yaml = !self.show_yaml;
                let status = if self.show_yaml { "ON" } else { "OFF" };
                self.add_system_message(format!("YAML view: {}", status));
            }
            SystemCommand::Thinking => {
                self.deep_thinking = !self.deep_thinking;
                let status = if self.deep_thinking { "ON" } else { "OFF" };
                self.add_system_message(format!("Deep thinking: {}", status));
            }
            SystemCommand::Model => {
                // Model change would need argument parsing
                self.add_system_message("Use ⌘P to select a model.".to_string());
            }
            SystemCommand::Provider => {
                // Provider change would need argument parsing
                self.add_system_message("Use ⌘P to select a provider.".to_string());
            }
        }
    }

    /// Navigate history up
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Safe: already checked history is not empty above
                self.history_index = Some(self.history.len().saturating_sub(1));
            }
            Some(i) if i > 0 => {
                self.history_index = Some(i.saturating_sub(1));
            }
            _ => {}
        }
        if let Some(i) = self.history_index {
            // Safe bounds check with .get()
            if let Some(entry) = self.history.get(i) {
                self.input = Input::new(entry.clone());
                self.input.handle(InputRequest::GoToEnd);
                self.update_mode_from_input(); // Sync mode indicator
            }
        }
    }

    /// Navigate history down
    pub fn history_down(&mut self) {
        // Early return if history is empty (prevents underflow on len() - 1)
        if self.history.is_empty() {
            self.history_index = None;
            return;
        }
        let last_idx = self.history.len().saturating_sub(1);
        match self.history_index {
            Some(i) if i < last_idx => {
                let next_idx = i.saturating_add(1);
                self.history_index = Some(next_idx);
                // Safe bounds check with .get()
                if let Some(entry) = self.history.get(next_idx) {
                    self.input = Input::new(entry.clone());
                    self.input.handle(InputRequest::GoToEnd);
                    self.update_mode_from_input(); // Sync mode indicator
                }
            }
            Some(_) => {
                self.history_index = None;
                self.input.reset();
                self.update_mode_from_input(); // Sync mode indicator
            }
            None => {}
        }
    }
}
