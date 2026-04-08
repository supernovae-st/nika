// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Key Handler Methods for Chat View
//!
//! Specialized key event handlers for overlay modes and main key handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_input::InputRequest;

use std::path::PathBuf;

use super::{is_cmd_pressed, ChatPanel, ChatView};
use crate::command::{Command, ExportFormat, ModelProvider, HELP_TEXT};
use crate::file_resolve::FileResolver;
use crate::views::{TuiView, ViewAction};
use crate::widgets::provider_modal::ModalEventHandler;
use crate::widgets::task_box::RenderMode;
use crate::widgets::ConfirmAction;

use super::hints::{detect_verb_in_input, verb_placeholder};

// ═══════════════════════════════════════════════════════════════════════════════
// Key Handler Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Handle key events when confirmation dialog is visible.
    ///
    /// Y or Enter = confirm the action, N or Escape = cancel.
    pub(super) fn handle_confirm_dialog_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            // Confirm: Y or Enter
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                let action = self.pending_confirm.take();
                match action {
                    Some(ConfirmAction::ClearChat) => ViewAction::ChatClear,
                    None => ViewAction::None,
                }
            }
            // Cancel: N, Escape, or any other key
            _ => {
                self.pending_confirm = None;
                ViewAction::None
            }
        }
    }

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
                let max = self.help_overlay.content_height();
                self.help_overlay.scroll_down(max);
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
                self.help_overlay.scroll = self.help_overlay.content_height();
                ViewAction::None
            }
            // PageUp
            KeyCode::PageUp => {
                self.help_overlay.scroll_page_up(10);
                ViewAction::None
            }
            // PageDown
            KeyCode::PageDown => {
                let max = self.help_overlay.content_height();
                self.help_overlay.scroll_page_down(max, 10);
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
                            Command::Clear => {
                                self.pending_confirm = Some(ConfirmAction::ClearChat);
                                ViewAction::None
                            }
                            Command::Exec { command } => ViewAction::ChatExec(command),
                            Command::Fetch { url, method } => ViewAction::ChatFetch(url, method),
                            Command::FetchError {
                                error,
                                hint,
                                example,
                            } => {
                                // Show helpful error message
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
                            Command::InvokeError {
                                error,
                                hint,
                                example,
                            } => {
                                self.add_nika_message(
                                    format!("{}\n{}\n{}", error, hint, example),
                                    None,
                                );
                                ViewAction::None
                            }
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
                                // Export chat to JSON or YAML file
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
                            Command::Run { path } => ViewAction::RunWorkflow(PathBuf::from(path)),
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
            use crate::widgets::ModalAction;
            match action {
                ModalAction::Close => {
                    self.provider_modal.visible = false;
                    self.focus_panel(ChatPanel::Input);
                }
                ModalAction::RefreshProviders => {
                    return ViewAction::VerifyProviders;
                }
                ModalAction::RefreshNativeModels => {
                    self.add_system_message("🔄 Refreshing native models...".to_string());
                    return ViewAction::RefreshNativeModels;
                }
                ModalAction::TestApiKey { provider } => {
                    self.add_system_message(format!("🔑 Testing {} API key...", provider));
                    return ViewAction::VerifyProviders;
                }
                ModalAction::PullModel { model } => {
                    self.add_system_message(format!("📥 Pulling model: {}...", model));
                    return ViewAction::PullNativeModel(model.clone());
                }
                ModalAction::DeleteModel { model } => {
                    self.add_system_message(format!("🗑️ Deleting model: {}...", model));
                    return ViewAction::DeleteNativeModel(model.clone());
                }
                ModalAction::SaveAndTestApiKey { provider, key } => {
                    use nika_engine::secrets::validate_key_format;
                    // Validate format first
                    if let Err(e) = validate_key_format(provider, key.as_str()) {
                        self.add_system_message(format!("❌ Invalid key format: {}", e));
                        return ViewAction::None;
                    }
                    // Save to vault
                    let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
                    let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
                    match vault.set(provider, key.as_str()) {
                        Ok(()) => {
                            self.add_system_message(format!(
                                "✅ Saved {} API key to vault",
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
                    use nika_engine::secrets::validate_key_format;
                    // Validate format first
                    if let Err(e) = validate_key_format(provider, key.as_str()) {
                        self.add_system_message(format!("❌ Invalid key format: {}", e));
                        return ViewAction::None;
                    }
                    // Save silently (no verification trigger)
                    let nika_home = dirs::home_dir().unwrap_or_default().join(".nika");
                    let vault = nika_vault::NikaVault::new(&nika_home.join("secrets"));
                    match vault.set(provider, key.as_str()) {
                        Ok(()) => {
                            self.add_system_message(format!(
                                "✅ Saved {} API key to vault",
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
                    // Actually switch provider via ChatModelSwitch
                    use crate::command::ModelProvider;
                    self.provider_modal.visible = false;
                    if let Some(model_provider) = ModelProvider::from_name(provider) {
                        self.add_system_message(format!(
                            "🔄 Switching to {} ({})",
                            provider, model
                        ));
                        return ViewAction::ChatModelSwitch(model_provider);
                    } else {
                        self.add_system_message(format!(
                            "❌ Unknown provider: {} - available: claude, openai, mistral, groq, deepseek, native",
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

    /// All known slash commands for tab completion
    const ALL_COMMANDS: &'static [&'static str] = &[
        "infer", "exec", "fetch", "invoke", "agent", "help", "model", "mcp", "clear", "export",
        "run",
    ];

    /// Try to autocomplete verb command with Tab
    /// Returns Some(new_input) if autocomplete happened, None otherwise
    pub(super) fn try_verb_autocomplete(&self) -> Option<String> {
        let input = self.input.value();

        // First try verb-specific autocomplete (with gradient + placeholder)
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

        // Fall through: try non-verb command completion (help, model, mcp, etc.)
        if input.starts_with('/') && !input.contains(' ') {
            let partial = &input[1..].to_lowercase();
            if partial.len() >= 2 {
                if let Some(cmd) = Self::ALL_COMMANDS
                    .iter()
                    .find(|c| c.starts_with(partial) && *c != partial)
                {
                    return Some(format!("/{} ", cmd));
                }
            }
        }

        None
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // Main Key Handler (extracted from mod.rs handle_key)
    // ═══════════════════════════════════════════════════════════════════════════════

    /// Handle main key events (after overlay dispatch)
    /// Returns Some(ViewAction) if key was handled, None to fall through
    pub(super) fn handle_main_keys(&mut self, key: KeyEvent) -> Option<ViewAction> {
        // ───────────────────────────────────────────────────────────────────────────
        // Global shortcuts (Cmd/Ctrl + key)
        // ───────────────────────────────────────────────────────────────────────────

        // Cmd/Ctrl+K = Command palette toggle
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('k') {
            self.toggle_command_palette();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+P = Provider modal toggle
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('p') {
            let opened = self.toggle_provider_modal();
            if opened {
                return Some(ViewAction::VerifyProviders);
            }
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+F = Start conversation search
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('f') {
            self.start_search();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+D = Toggle DAG panel
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('d') {
            self.toggle_dag_panel();
            let status = if self.show_dag_panel {
                "visible"
            } else {
                "hidden"
            };
            self.add_system_message(format!("🔀 DAG panel {}", status));
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+T = Toggle deep thinking
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('t') {
            self.toggle_deep_thinking();
            let status = if self.deep_thinking {
                "enabled"
            } else {
                "disabled"
            };
            self.add_system_message(format!("🧠 Deep thinking {}", status));
            return Some(ViewAction::None);
        }

        // Alt+T = Toggle all thinking sections visibility
        if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Char('t') {
            self.toggle_all_thinking();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+M = Toggle infer/agent mode
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('m') {
            self.toggle_mode();
            self.add_system_message(format!(
                "{} Switched to {} mode",
                self.chat_mode.icon(),
                self.chat_mode.label()
            ));
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+C = Copy selection or input
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('c') {
            if self.text_selection.is_some() {
                if self.copy_selection() {
                    self.add_system_message("📋 Selection copied to clipboard");
                    self.clear_selection();
                }
            } else {
                self.copy_to_clipboard();
            }
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+V = Paste from clipboard
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('v') {
            self.paste_from_clipboard();
            return Some(ViewAction::None);
        }

        // Ctrl+Z = Undo input edit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('z') {
            if let Some((text, cursor)) = self.edit_history.undo() {
                self.input = tui_input::Input::new(text);
                let pos = cursor.min(self.input.value().len());
                self.input.handle(InputRequest::GoToStart);
                for _ in 0..pos {
                    self.input.handle(InputRequest::GoToNextChar);
                }
            }
            return Some(ViewAction::None);
        }

        // Ctrl+Y = Redo input edit
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('y') {
            if let Some((text, cursor)) = self.edit_history.redo() {
                self.input = tui_input::Input::new(text);
                let pos = cursor.min(self.input.value().len());
                self.input.handle(InputRequest::GoToStart);
                for _ in 0..pos {
                    self.input.handle(InputRequest::GoToNextChar);
                }
            }
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+R = Retry last failed MCP call
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('r') {
            if let Some(failed) = self.last_failed_mcp.take() {
                self.add_system_message("🔄 Retrying MCP call...".to_string());
                return Some(ViewAction::ChatInvoke(
                    failed.tool,
                    Some(failed.server),
                    failed.params,
                ));
            } else {
                self.add_system_message("⚠️ No failed MCP call to retry".to_string());
            }
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+Left = Move to previous word
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Left {
            self.cursor_prev_word();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+Right = Move to next word
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Right {
            self.cursor_next_word();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+Backspace = Delete previous word
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Backspace {
            self.delete_prev_word();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+A = Go to start of input
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('a') {
            self.cursor_start();
            return Some(ViewAction::None);
        }

        // Cmd/Ctrl+E = Go to end of input
        if is_cmd_pressed(key.modifiers) && key.code == KeyCode::Char('e') {
            self.cursor_end();
            return Some(ViewAction::None);
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Ctrl+j/k for vim-style panel navigation
        // ───────────────────────────────────────────────────────────────────────────

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Char('j') {
                self.focus_next_panel();
                return Some(ViewAction::None);
            }
            if key.code == KeyCode::Char('k') {
                self.focus_prev_panel();
                return Some(ViewAction::None);
            }
        }

        // ───────────────────────────────────────────────────────────────────────────
        // F1 or ? = Toggle help overlay
        // ───────────────────────────────────────────────────────────────────────────

        if key.code == KeyCode::F(1)
            || (key.code == KeyCode::Char('?') && self.focused_panel != ChatPanel::Input)
        {
            self.help_overlay.toggle();
            return Some(ViewAction::None);
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Tab = Autocomplete verb in Input panel
        // ───────────────────────────────────────────────────────────────────────────

        if key.code == KeyCode::Tab && self.focused_panel == ChatPanel::Input {
            if let Some(completed) = self.try_verb_autocomplete() {
                self.input = tui_input::Input::new(completed);
                self.input.handle(InputRequest::GoToEnd);
            }
            return Some(ViewAction::None);
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Panel navigation: Left/Right when NOT in Input panel
        // ───────────────────────────────────────────────────────────────────────────

        if self.focused_panel != ChatPanel::Input {
            if key.code == KeyCode::Right {
                self.focus_next_panel();
                return Some(ViewAction::None);
            }
            if key.code == KeyCode::Left {
                self.focus_prev_panel();
                return Some(ViewAction::None);
            }
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Vim-style navigation when NOT in Input panel
        // ───────────────────────────────────────────────────────────────────────────

        if self.focused_panel != ChatPanel::Input {
            match key.code {
                // j/Down = Scroll down
                KeyCode::Char('j') | KeyCode::Down => {
                    self.scroll_down();
                    return Some(ViewAction::None);
                }
                // k/Up = Scroll up
                KeyCode::Char('k') | KeyCode::Up => {
                    self.scroll_up();
                    return Some(ViewAction::None);
                }
                // g = Go to top
                KeyCode::Char('g') => {
                    self.scroll_to_top();
                    return Some(ViewAction::None);
                }
                // G = Go to bottom
                KeyCode::Char('G') => {
                    self.scroll_to_bottom();
                    return Some(ViewAction::None);
                }
                // PageUp/PageDown
                KeyCode::PageUp => {
                    self.page_up();
                    return Some(ViewAction::None);
                }
                KeyCode::PageDown => {
                    self.page_down();
                    return Some(ViewAction::None);
                }
                // y = Copy full message
                KeyCode::Char('y') => {
                    if self.copy_selected_message(false) {
                        self.add_system_message("📋 Message copied to clipboard".to_string());
                    }
                    return Some(ViewAction::None);
                }
                // Y = Copy text only
                KeyCode::Char('Y') => {
                    if self.copy_selected_message(true) {
                        self.add_system_message("📋 Text copied to clipboard".to_string());
                    }
                    return Some(ViewAction::None);
                }
                // t = Toggle thinking for selected message
                KeyCode::Char('t') => {
                    let cursor = self.conversation_scroll.cursor;
                    if cursor < self.messages.len() && self.messages[cursor].thinking.is_some() {
                        self.toggle_thinking(cursor);
                        let state = if self.is_thinking_visible(cursor) {
                            "expanded"
                        } else {
                            "collapsed"
                        };
                        self.add_system_message(format!("🧠 Thinking {} for message", state));
                    }
                    return Some(ViewAction::None);
                }
                // Enter = Return focus to Input
                KeyCode::Enter => {
                    self.focus_panel(ChatPanel::Input);
                    return Some(ViewAction::None);
                }
                // Escape = Return to Input
                KeyCode::Esc => {
                    self.focus_panel(ChatPanel::Input);
                    return Some(ViewAction::None);
                }
                _ => {} // Fall through
            }
        }

        // ───────────────────────────────────────────────────────────────────────────
        // Normal key handling (Input panel active or no special handling above)
        // ───────────────────────────────────────────────────────────────────────────

        match key.code {
            // Ctrl+J scrolls down (vi-like)
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_down();
                Some(ViewAction::None)
            }
            // 's' when empty switches to Studio (only from non-Input panels)
            KeyCode::Char('s')
                if self.input.value().is_empty() && self.focused_panel != ChatPanel::Input =>
            {
                Some(ViewAction::SwitchView(TuiView::Studio))
            }
            // 'm' when empty cycles TaskBox render mode
            KeyCode::Char('m') if self.input.value().is_empty() => {
                self.cycle_task_box_render_mode();
                let mode_name = match self.task_box_render_mode {
                    RenderMode::Compact => "Compact (1 line)",
                    RenderMode::Expanded => "Expanded (default)",
                    RenderMode::Full => "Full (all details)",
                };
                self.add_system_message(format!("📦 TaskBox mode: {}", mode_name));
                Some(ViewAction::None)
            }
            // Shift+T or Ctrl+t toggles theme
            KeyCode::Char('T') => Some(ViewAction::ToggleTheme),
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(ViewAction::ToggleTheme)
            }
            // "/" at start triggers command palette with verbs
            KeyCode::Char('/') if self.input.value().is_empty() => {
                self.toggle_command_palette();
                self.command_palette.query = "/".to_string();
                self.command_palette.update_filter();
                Some(ViewAction::None)
            }
            // "@" triggers file mention
            KeyCode::Char('@') => {
                self.insert_char('@');
                if !self.shown_file_hint {
                    self.add_system_message(
                        "💡 Type @filename to include file content".to_string(),
                    );
                    self.shown_file_hint = true;
                }
                Some(ViewAction::None)
            }
            // Shift+Enter inserts newline
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.handle(InputRequest::InsertChar('\n'));
                Some(ViewAction::None)
            }
            // Enter = Submit
            KeyCode::Enter => self.handle_enter_key(),
            // Ctrl+Up/Down = History navigation
            KeyCode::Up if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.history_up();
                Some(ViewAction::None)
            }
            KeyCode::Down if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.history_down();
                Some(ViewAction::None)
            }
            // Up/Down = Scroll
            KeyCode::Up => {
                self.scroll_up();
                Some(ViewAction::None)
            }
            KeyCode::Down => {
                self.scroll_down();
                Some(ViewAction::None)
            }
            // Left/Right = Cursor movement
            KeyCode::Left => {
                self.cursor_left();
                Some(ViewAction::None)
            }
            KeyCode::Right => {
                self.cursor_right();
                Some(ViewAction::None)
            }
            // Backspace
            KeyCode::Backspace => {
                self.backspace();
                Some(ViewAction::None)
            }
            // Character input
            KeyCode::Char(c) => {
                self.insert_char(c);
                Some(ViewAction::None)
            }
            // Page scroll
            KeyCode::PageUp => {
                self.page_up();
                Some(ViewAction::None)
            }
            KeyCode::PageDown => {
                self.page_down();
                Some(ViewAction::None)
            }
            // Home/End
            KeyCode::Home => {
                self.scroll_to_top();
                Some(ViewAction::None)
            }
            KeyCode::End => {
                self.scroll_to_bottom();
                Some(ViewAction::None)
            }
            // Esc = always return focus to Input panel (Ctrl+W / shortcut key to leave view)
            KeyCode::Esc => {
                self.focus_panel(ChatPanel::Input);
                Some(ViewAction::None)
            }
            _ => None,
        }
    }

    /// Handle Enter key submission
    fn handle_enter_key(&mut self) -> Option<ViewAction> {
        if let Some(message) = self.submit() {
            let cmd = Command::parse(&message);

            match cmd {
                Command::Help => {
                    self.add_nika_message(HELP_TEXT.to_string(), None);
                    Some(ViewAction::None)
                }
                Command::Clear => {
                    self.pending_confirm = Some(ConfirmAction::ClearChat);
                    Some(ViewAction::None)
                }
                Command::Exec { command } => Some(ViewAction::ChatExec(command)),
                Command::Fetch { url, method } => Some(ViewAction::ChatFetch(url, method)),
                Command::FetchError {
                    error,
                    hint,
                    example,
                } => {
                    self.add_nika_message(format!("{}\n{}\n{}", error, hint, example), None);
                    Some(ViewAction::None)
                }
                Command::Invoke {
                    tool,
                    server,
                    params,
                } => Some(ViewAction::ChatInvoke(tool, server, params)),
                Command::InvokeError {
                    error,
                    hint,
                    example,
                } => {
                    self.add_nika_message(format!("{}\n{}\n{}", error, hint, example), None);
                    Some(ViewAction::None)
                }
                Command::Agent {
                    goal,
                    max_turns,
                    mcp_servers,
                } => Some(ViewAction::ChatAgent(
                    goal,
                    max_turns,
                    self.deep_thinking,
                    mcp_servers,
                )),
                Command::Mcp { action } => Some(ViewAction::ChatMcp(action)),
                Command::Model { provider } => {
                    if provider == ModelProvider::List {
                        let providers = [
                            ModelProvider::Claude,
                            ModelProvider::OpenAI,
                            ModelProvider::Mistral,
                            ModelProvider::Groq,
                            ModelProvider::DeepSeek,
                            ModelProvider::Native,
                        ];
                        let mut list_text =
                            String::from("Available providers (use /model <name>):\n");
                        for p in providers {
                            let status = if p.is_available() {
                                "available"
                            } else {
                                "missing API key"
                            };
                            list_text.push_str(&format!(
                                "  - {}: {} ({})\n",
                                p.command_name(),
                                p.name(),
                                status
                            ));
                        }
                        self.add_nika_message(list_text.trim_end().to_string(), None);
                        Some(ViewAction::None)
                    } else {
                        Some(ViewAction::ChatModelSwitch(provider))
                    }
                }
                Command::Export { format, path } => {
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
                    Some(ViewAction::None)
                }
                Command::Run { path } => Some(ViewAction::RunWorkflow(PathBuf::from(path))),
                Command::Infer { prompt } | Command::Chat { message: prompt } => {
                    let base_dir = std::env::current_dir().unwrap_or_default();
                    let expanded = FileResolver::resolve(&prompt, &base_dir);
                    Some(ViewAction::ChatInfer(expanded))
                }
            }
        } else {
            Some(ViewAction::None)
        }
    }
}
