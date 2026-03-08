//! App Routing and Action Dispatch
//!
//! Contains the apply_action method for routing user actions to appropriate handlers.

use crate::tui::InputMode;

use super::super::views::{TuiView, View, ViewAction};
use super::types::Action;
use super::App;

impl App {
    /// Apply an action resulting from user input
    ///
    /// Routes actions to the appropriate handler method.
    pub(crate) fn apply_action(&mut self, action: Action) {
        match action {
            // ═══ Application Control ═══
            Action::Quit => {
                self.should_quit = true;
            }

            // ═══ View Navigation ═══
            Action::SwitchView(view) => {
                self.switch_to_view(view);
            }

            // ═══ Theme ═══
            Action::ToggleTheme => {
                self.toggle_theme();
            }

            // ═══ Scrolling ═══
            Action::ScrollUp => {
                self.handle_scroll_up();
            }
            Action::ScrollDown => {
                self.handle_scroll_down();
            }
            Action::ScrollToTop => {
                self.handle_scroll_to_top();
            }
            Action::ScrollToBottom => {
                self.handle_scroll_to_bottom();
            }

            // ═══ Pause/Step ═══
            Action::TogglePause => {
                self.state.workflow.paused = !self.state.workflow.paused;
            }
            Action::Step => {
                // Step one event while paused
                // TODO: Add step_requested field to workflow state if needed
            }

            // ═══ Panel Focus ═══
            Action::FocusNext => {
                // Cycle to next panel
            }
            Action::FocusPrev => {
                // Cycle to previous panel
            }
            Action::FocusPanel(_n) => {
                // Focus specific panel by number
            }
            Action::CycleTab => {
                // Cycle tabs in focused panel
            }

            // ═══ Quick Actions ═══
            Action::CopyToClipboard => {
                // Copy current panel content
            }
            Action::RetryWorkflow => {
                self.retry_workflow();
            }
            Action::ExportTrace => {
                // Export trace to file
            }

            // ═══ Breakpoints ═══
            Action::ToggleBreakpoint => {
                // Toggle breakpoint on current task
            }

            // ═══ Mouse Actions ═══
            Action::MouseClickPanel(_panel_id) => {
                // Handle panel click
            }
            Action::MouseScrollUp => {
                self.handle_scroll_up();
            }
            Action::MouseScrollDown => {
                self.handle_scroll_down();
            }

            // ═══ Notifications ═══
            Action::DismissNotification => {
                self.state.notifications.pop();
            }
            Action::DismissAllNotifications => {
                self.state.notifications.clear();
            }
            Action::DismissError => {
                // Dismiss error message
            }

            // ═══ Filter/Search ═══
            Action::EnterFilter => {
                self.input_mode = InputMode::Search;
            }
            Action::ExitFilter => {
                self.input_mode = InputMode::Normal;
                self.state.filter_query.clear();
            }
            Action::FilterInput(c) => {
                self.state.filter_query.push(c);
            }
            Action::FilterBackspace => {
                self.state.filter_query.pop();
            }
            Action::FilterDelete | Action::FilterCursorLeft | Action::FilterCursorRight => {
                // Cursor movement in filter - simplified
            }
            Action::FilterClear => {
                self.state.filter_query.clear();
            }

            // ═══ Settings Overlay ═══
            Action::SettingsNextField
            | Action::SettingsPrevField
            | Action::SettingsToggleEdit
            | Action::SettingsInput(_)
            | Action::SettingsBackspace
            | Action::SettingsDelete
            | Action::SettingsCancelEdit
            | Action::SettingsSave
            | Action::SettingsCursorLeft
            | Action::SettingsCursorRight => {
                // Settings overlay actions - handled by SettingsView
            }

            // ═══ Chat Overlay ═══
            Action::ChatOverlayInput(_)
            | Action::ChatOverlayBackspace
            | Action::ChatOverlayDelete
            | Action::ChatOverlayCursorLeft
            | Action::ChatOverlayCursorRight
            | Action::ChatOverlayHistoryUp
            | Action::ChatOverlayHistoryDown
            | Action::ChatOverlaySend
            | Action::ChatOverlayClear
            | Action::ChatOverlayScrollUp
            | Action::ChatOverlayScrollDown => {
                // Chat overlay actions - handled by ChatView
            }

            // ═══ Mode Change ═══
            Action::SetMode(mode) => {
                self.state.mode = mode;
            }

            // ═══ Continue (no-op) ═══
            Action::Continue => {}

            // ═══ View-Specific Actions (v0.21 TUI Fix) ═══
            Action::ViewSpecific(view_action) => {
                self.apply_view_action(view_action);
            }
        }
    }

    /// Apply view-specific actions that need App-level orchestration
    ///
    /// v0.21 FIX: These actions come from view handle_key() methods
    /// and need access to App-level state (spawning tasks, MCP clients, etc.)
    fn apply_view_action(&mut self, action: ViewAction) {
        match action {
            // Chat commands - TODO: delegate to ChatAgent when implemented
            ViewAction::ChatInfer(prompt) => {
                self.chat_view
                    .add_user_message(format!("/infer {}", prompt));
                self.set_status("Infer command received");
                tracing::debug!("ChatInfer: {}", prompt);
            }
            ViewAction::ChatExec(cmd) => {
                self.chat_view.add_user_message(format!("/exec {}", cmd));
                self.set_status("Exec command received");
                tracing::debug!("ChatExec: {}", cmd);
            }
            ViewAction::ChatFetch(url, method) => {
                self.chat_view
                    .add_user_message(format!("/fetch {} {}", method, url));
                self.set_status("Fetch command received");
                tracing::debug!("ChatFetch: {} {}", method, url);
            }
            ViewAction::ChatInvoke(tool, server, _params) => {
                self.chat_view
                    .add_user_message(format!("/invoke {} {:?}", tool, server));
                self.set_status("Invoke command received");
                tracing::debug!("ChatInvoke: {} {:?}", tool, server);
            }
            ViewAction::ChatAgent(goal, max_turns, extended, servers) => {
                self.chat_view.add_user_message(format!("/agent {}", goal));
                self.set_status(&format!(
                    "Agent: {} (turns={:?}, ext={}, servers={:?})",
                    goal, max_turns, extended, servers
                ));
                tracing::debug!("ChatAgent: {}", goal);
            }
            ViewAction::ChatClear => {
                self.chat_view.messages.clear();
                self.set_status("Chat cleared");
            }
            ViewAction::ChatModelSwitch(provider) => {
                self.set_status(&format!("Switched to {:?}", provider));
                tracing::debug!("ChatModelSwitch: {:?}", provider);
            }
            ViewAction::ChatMcp(_mcp_action) => {
                self.set_status("MCP action received");
            }
            ViewAction::SendChatMessage(msg) => {
                self.chat_view.add_user_message(msg.clone());
                self.set_status("Message sent");
                tracing::debug!("SendChatMessage: {}", msg);
            }

            // Workflow actions
            ViewAction::RunWorkflow(path) => {
                self.set_status(&format!("Running: {}", path.display()));
                tracing::info!("RunWorkflow: {}", path.display());
            }
            ViewAction::OpenInStudio(path) => {
                let _ = self.studio_view.load_file(path.clone());
                self.switch_to_view(TuiView::Studio);
                self.set_status(&format!("Opened: {}", path.display()));
            }
            ViewAction::ValidateWorkflow(path) => {
                self.set_status(&format!("Validating: {}", path.display()));
                tracing::debug!("ValidateWorkflow: {}", path.display());
            }

            // Theme actions
            ViewAction::SetTheme(variant) => {
                self.cosmic_theme = super::super::cosmic_theme::CosmicTheme::new(variant);
                self.theme = self.cosmic_theme.as_theme();
                self.set_status(&format!("Theme set to: {:?}", variant));
            }
            ViewAction::ToggleTheme => {
                self.toggle_theme();
            }

            // Provider actions - TODO: implement verification
            ViewAction::VerifyProviders => {
                self.set_status("Verifying providers...");
            }
            ViewAction::RefreshVerification => {
                self.set_status("Refreshing verification...");
            }
            ViewAction::ProviderSelectorConfirm { provider_id, model } => {
                self.set_status(&format!("Selected: {} / {}", provider_id, model));
                tracing::debug!("ProviderSelectorConfirm: {} / {}", provider_id, model);
            }

            // Ollama actions - TODO: implement Ollama management
            ViewAction::PullOllamaModel(model) => {
                self.set_status(&format!("Pulling: {}", model));
            }
            ViewAction::DeleteOllamaModel(model) => {
                self.set_status(&format!("Deleting: {}", model));
            }
            ViewAction::RefreshOllamaModels => {
                self.set_status("Refreshing Ollama models...");
            }

            // Chat overlay
            ViewAction::ToggleChatOverlay => {
                // Note: Chat overlay visibility is controlled via input mode, not state toggle
                self.set_status("Chat overlay toggled");
            }

            // Error handling
            ViewAction::Error(msg) => {
                self.set_status(&format!("Error: {}", msg));
            }

            // Navigation - already handled by from_view_action conversion
            ViewAction::None
            | ViewAction::Quit
            | ViewAction::SwitchView(_)
            | ViewAction::OpenSettings => {
                // These are converted to Action variants directly
            }
        }
    }

    /// Switch to a specific view with appropriate mode changes
    ///
    /// v0.21 FIX: Now calls lifecycle hooks (on_leave, on_enter)
    fn switch_to_view(&mut self, view: TuiView) {
        // Skip if already on this view
        if self.current_view == view {
            return;
        }

        // Call on_leave for the current view
        self.call_view_on_leave(self.current_view);

        // Switch view
        let old_view = self.current_view;
        self.current_view = view;

        // Set appropriate input mode
        self.input_mode = match view {
            TuiView::Chat => InputMode::Insert,
            _ => InputMode::Normal,
        };

        // Call on_enter for the new view
        self.call_view_on_enter(view);

        tracing::debug!("Switched view: {:?} -> {:?}", old_view, view);
    }

    /// Call on_enter lifecycle hook for a view
    fn call_view_on_enter(&mut self, view: TuiView) {
        match view {
            TuiView::Studio => self.studio_view.on_enter(&mut self.state),
            TuiView::Runner => self.monitor_view.on_enter(&mut self.state),
            TuiView::Chat => self.chat_view.on_enter(&mut self.state),
            TuiView::Settings => self.settings_view.on_enter(&mut self.state),
        }
    }

    /// Call on_leave lifecycle hook for a view
    fn call_view_on_leave(&mut self, view: TuiView) {
        match view {
            TuiView::Studio => self.studio_view.on_leave(&mut self.state),
            TuiView::Runner => self.monitor_view.on_leave(&mut self.state),
            TuiView::Chat => self.chat_view.on_leave(&mut self.state),
            TuiView::Settings => self.settings_view.on_leave(&mut self.state),
        }
    }

    /// Handle scroll up action for current view
    fn handle_scroll_up(&mut self) {
        // Delegate to current view's scroll handler
        self.state.chat_overlay.scroll_up();
    }

    /// Handle scroll down action for current view
    fn handle_scroll_down(&mut self) {
        // Delegate to current view's scroll handler
        self.state.chat_overlay.scroll_down();
    }

    /// Handle scroll to top action
    #[allow(dead_code)]
    fn handle_scroll_to_top(&mut self) {
        // Scroll to top of current view
    }

    /// Handle scroll to bottom action
    #[allow(dead_code)]
    fn handle_scroll_to_bottom(&mut self) {
        // Scroll to bottom of current view
    }
}
