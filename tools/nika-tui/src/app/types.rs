// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! App Types
//!
//! Contains the Action enum and other shared types for the TUI application.

use super::super::state::{MonitorPanel, TuiMode};
use super::super::views::{TuiView, ViewAction};

/// Action resulting from input handling
///
/// Variants are matched in routing.rs but some are not yet
/// connected to input handling (staged for keybinding system).
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Action {
    /// Continue normal operation
    Continue,
    /// Quit the application
    Quit,
    /// Toggle pause
    TogglePause,
    /// Step one event (when paused)
    Step,
    /// Focus next panel
    FocusNext,
    /// Focus previous panel
    FocusPrev,
    /// Focus specific panel
    FocusPanel(u8),
    /// Cycle tabs in focused panel
    CycleTab,
    /// Toggle mode
    SetMode(TuiMode),
    /// Scroll up in focused panel
    ScrollUp,
    /// Scroll down in focused panel
    ScrollDown,
    /// Scroll to top of focused panel [g]
    ScrollToTop,
    /// Scroll to bottom of focused panel [G]
    ScrollToBottom,
    // ═══ Quick Actions (TIER 1) ═══
    /// Copy current panel content to clipboard [c]
    CopyToClipboard,
    /// Retry failed workflow [r]
    RetryWorkflow,
    /// Export trace to file [e]
    ExportTrace,
    // ═══ Breakpoint Actions (TIER 2.3) ═══
    /// Toggle breakpoint on current task [b]
    ToggleBreakpoint,
    // ═══ Theme Actions (TIER 2.4) ═══
    /// Toggle theme dark/light [t]
    ToggleTheme,
    // ═══ Mouse Actions (TIER 3.1) ═══
    /// Click on a panel to focus it
    MouseClickPanel(MonitorPanel),
    /// Scroll up
    MouseScrollUp,
    /// Scroll down
    MouseScrollDown,
    // ═══ Notification Actions (TIER 3.4) ═══
    /// Dismiss the most recent notification [n]
    DismissNotification,
    /// Dismiss all notifications [N]
    DismissAllNotifications,
    /// Dismiss error message [E] (P3 fix: error dismissal shortcut)
    DismissError,
    // ═══ Filter/Search Actions (TIER 1.5) ═══
    /// Enter search/filter mode
    EnterFilter,
    /// Exit search/filter mode
    ExitFilter,
    /// Insert character in filter query
    FilterInput(char),
    /// Backspace in filter query
    FilterBackspace,
    /// Delete character in filter query
    FilterDelete,
    /// Move filter cursor left
    FilterCursorLeft,
    /// Move filter cursor right
    FilterCursorRight,
    /// Clear filter query
    FilterClear,
    // ═══ Settings Overlay Actions ═══
    /// Focus next settings field
    SettingsNextField,
    /// Focus previous settings field
    SettingsPrevField,
    /// Toggle edit mode for current field
    SettingsToggleEdit,
    /// Insert character in edit buffer
    SettingsInput(char),
    /// Backspace in edit buffer
    SettingsBackspace,
    /// Delete character in edit buffer
    SettingsDelete,
    /// Cancel editing (restore original)
    SettingsCancelEdit,
    /// Save settings to config file
    SettingsSave,
    /// Move cursor left in edit mode
    SettingsCursorLeft,
    /// Move cursor right in edit mode
    SettingsCursorRight,
    // ═══ View Navigation Actions ═══
    /// Switch to a specific view (number keys 1/2/3/4)
    SwitchView(TuiView),
    // ═══ View-Specific Actions ═══
    /// Action delegated from a view's handle_key() that needs App-level handling
    ViewSpecific(ViewAction),
}

impl Action {
    /// Convert ViewAction to Action for unified event handling
    ///
    /// This bridges the view-level event system with the app-level action system,
    /// enabling view handle_key() methods to trigger app-wide state changes.
    pub fn from_view_action(view_action: ViewAction) -> Self {
        match view_action {
            // Direct mappings to Action variants
            ViewAction::None => Action::Continue,
            ViewAction::Quit => Action::Quit,
            ViewAction::SwitchView(view) => Action::SwitchView(view),
            ViewAction::ToggleTheme => Action::ToggleTheme,
            ViewAction::OpenControl => Action::SwitchView(TuiView::Control),

            // Chat/Editor/Agent actions need App-level orchestration
            ViewAction::ChatInfer(_)
            | ViewAction::ChatExec(_)
            | ViewAction::ChatFetch(_, _)
            | ViewAction::ChatInvoke(_, _, _)
            | ViewAction::ChatAgent(_, _, _, _)
            | ViewAction::ChatClear
            | ViewAction::ChatModelSwitch(_)
            | ViewAction::ChatMcp(_)
            | ViewAction::RunWorkflow(_)
            | ViewAction::OpenInStudio(_)
            | ViewAction::ValidateWorkflow(_)
            | ViewAction::SendChatMessage(_)
            | ViewAction::SetTheme(_)
            | ViewAction::VerifyProviders
            | ViewAction::RefreshVerification
            | ViewAction::ProviderSelectorConfirm { .. }
            | ViewAction::PullNativeModel(_)
            | ViewAction::DeleteNativeModel(_)
            | ViewAction::RefreshNativeModels
            | ViewAction::LaunchWizard
            | ViewAction::Error(_)
            | ViewAction::StatusMessage(_) => Action::ViewSpecific(view_action),
            // Note: No catch-all - we explicitly handle all variants
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_equality() {
        assert_eq!(Action::Quit, Action::Quit);
        assert_eq!(Action::Continue, Action::Continue);
        assert_ne!(Action::Quit, Action::Continue);
    }

    #[test]
    fn test_action_clone() {
        let action = Action::FilterInput('a');
        let cloned = action.clone();
        assert_eq!(action, cloned);
    }

    #[test]
    fn test_from_view_action_none() {
        let result = Action::from_view_action(ViewAction::None);
        assert_eq!(result, Action::Continue);
    }

    #[test]
    fn test_from_view_action_quit() {
        let result = Action::from_view_action(ViewAction::Quit);
        assert_eq!(result, Action::Quit);
    }

    #[test]
    fn test_from_view_action_toggle_theme() {
        let result = Action::from_view_action(ViewAction::ToggleTheme);
        assert_eq!(result, Action::ToggleTheme);
    }

    #[test]
    fn test_from_view_action_open_settings() {
        let result = Action::from_view_action(ViewAction::OpenControl);
        assert_eq!(result, Action::SwitchView(TuiView::Control));
    }

    #[test]
    fn test_from_view_action_switch_view() {
        let result = Action::from_view_action(ViewAction::SwitchView(TuiView::Command));
        assert_eq!(result, Action::SwitchView(TuiView::Command));
    }

    #[test]
    fn test_from_view_action_chat_infer() {
        let prompt = "test prompt".to_string();
        let result = Action::from_view_action(ViewAction::ChatInfer(prompt.clone()));
        match result {
            Action::ViewSpecific(ViewAction::ChatInfer(p)) => assert_eq!(p, prompt),
            _ => panic!("Expected ViewSpecific(ChatInfer)"),
        }
    }

    #[test]
    fn test_from_view_action_launch_wizard() {
        let result = Action::from_view_action(ViewAction::LaunchWizard);
        match result {
            Action::ViewSpecific(ViewAction::LaunchWizard) => {}
            _ => panic!("Expected ViewSpecific(LaunchWizard)"),
        }
    }

    #[test]
    fn test_action_debug_format() {
        let action = Action::Quit;
        let debug_str = format!("{:?}", action);
        assert!(debug_str.contains("Quit"));
    }

    #[test]
    fn test_filter_actions() {
        // Test that FilterInput captures characters correctly
        let action = Action::FilterInput('x');
        match action {
            Action::FilterInput(c) => assert_eq!(c, 'x'),
            _ => panic!("Expected FilterInput"),
        }
    }

    #[test]
    fn test_focus_panel_action() {
        let action = Action::FocusPanel(2);
        match action {
            Action::FocusPanel(n) => assert_eq!(n, 2),
            _ => panic!("Expected FocusPanel"),
        }
    }

    #[test]
    fn test_mouse_click_panel() {
        let action = Action::MouseClickPanel(MonitorPanel::Dag);
        match action {
            Action::MouseClickPanel(panel) => assert_eq!(panel, MonitorPanel::Dag),
            _ => panic!("Expected MouseClickPanel"),
        }
    }
}
