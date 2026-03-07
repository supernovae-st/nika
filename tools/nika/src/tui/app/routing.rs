//! App Routing and Action Dispatch
//!
//! Contains the apply_action method for routing user actions to appropriate handlers.

use crate::tui::InputMode;

use super::super::views::TuiView;
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
        }
    }

    /// Switch to a specific view with appropriate mode changes
    fn switch_to_view(&mut self, view: TuiView) {
        self.current_view = view;

        // Chat view uses Insert mode by default
        if view == TuiView::Chat {
            self.input_mode = InputMode::Insert;
        } else {
            self.input_mode = InputMode::Normal;
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
