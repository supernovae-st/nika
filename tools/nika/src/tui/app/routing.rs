//! App Routing and Action Dispatch
//!
//! Contains the apply_action method for routing user actions to appropriate handlers.

use std::path::PathBuf;

use crate::ast::{expand_includes, Workflow};
use crate::event::EventLog;
use crate::runtime::Runner;
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
                // v0.22.3: Delegate to scroll up (simple implementation)
                self.handle_scroll_up();
            }
            Action::ScrollToBottom => {
                // v0.22.3: Delegate to scroll down (simple implementation)
                self.handle_scroll_down();
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
                self.run_workflow(path);
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

            // Native model actions - TODO: implement native model management
            ViewAction::PullNativeModel(model) => {
                self.set_status(&format!("Pulling: {}", model));
            }
            ViewAction::DeleteNativeModel(model) => {
                self.set_status(&format!("Deleting: {}", model));
            }
            ViewAction::RefreshNativeModels => {
                self.set_status("Refreshing native models...");
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
    pub(super) fn call_view_on_enter(&mut self, view: TuiView) {
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

    /// Run a workflow from a file path
    ///
    /// v0.21.1: Actually executes the workflow and displays results in Runner view.
    /// This is the glue code that connects:
    /// - Studio view (F5 to run)
    /// - Workflow parsing (ast module)
    /// - Runner execution (runtime module)
    /// - TaskBox widgets (MonitorView)
    fn run_workflow(&mut self, path: PathBuf) {
        tracing::info!("Running workflow: {}", path.display());

        // 1. Read and parse workflow file
        let yaml_content = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                self.set_status(&format!("Failed to read file: {}", e));
                tracing::error!("Failed to read workflow file: {}", e);
                return;
            }
        };

        let workflow: Workflow = match crate::serde_yaml::from_str(&yaml_content) {
            Ok(w) => w,
            Err(e) => {
                let line_info = e
                    .location()
                    .map(|l| format!(" (line {})", l.line()))
                    .unwrap_or_default();
                self.set_status(&format!("Parse error{}: {}", line_info, e));
                tracing::error!("Failed to parse workflow: {}", e);
                return;
            }
        };

        // 2. Expand includes (DAG fusion)
        let base_path = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));

        let workflow = match expand_includes(workflow, base_path) {
            Ok(w) => w,
            Err(e) => {
                self.set_status(&format!("Include error: {}", e));
                tracing::error!("Failed to expand includes: {}", e);
                return;
            }
        };

        // 3. Validate schema
        if let Err(e) = workflow.validate_schema() {
            self.set_status(&format!("Schema error: {}", e));
            tracing::error!("Schema validation failed: {}", e);
            return;
        }

        // 4. Create EventLog with broadcast channel for TUI
        let (event_log, event_rx) = EventLog::new_with_broadcast();

        // 5. Wire broadcast receiver to App (must be before spawn)
        self.broadcast_rx = Some(event_rx);

        // 6. Reset TUI state for new workflow execution
        let workflow_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workflow")
            .to_string();
        self.state.workflow = crate::tui::state::WorkflowState::new(workflow_name.clone());
        self.state.tasks.clear();
        self.workflow_done = false;

        // 7. Create Runner with quiet mode (no console output)
        let mut runner = Runner::with_event_log(workflow, event_log).quiet();

        // 8. Spawn Runner in background task
        self.spawn_tracked(async move {
            tracing::info!("Starting workflow execution: {}", workflow_name);
            match runner.run().await {
                Ok(output) => {
                    tracing::info!(
                        "Workflow '{}' completed: {} bytes output",
                        workflow_name,
                        output.len()
                    );
                }
                Err(e) => {
                    tracing::error!("Workflow '{}' failed: {}", workflow_name, e);
                }
            }
        });

        // 9. Switch to Runner view and update status
        self.switch_to_view(TuiView::Runner);
        self.set_status(&format!("Running: {}", path.display()));
    }
}
