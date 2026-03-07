//! App Types
//!
//! Contains the Action enum and other shared types for the TUI application.

use super::super::state::{PanelId, TuiMode};
use super::super::views::TuiView;

/// Action resulting from input handling
///
/// Note: Some variants are temporarily unused during module extraction refactoring.
/// They will be used when events.rs and routing.rs are fully implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    MouseClickPanel(PanelId),
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
    // ═══ Chat Overlay Actions ═══
    /// Insert character in chat overlay input
    ChatOverlayInput(char),
    /// Backspace in chat overlay input
    ChatOverlayBackspace,
    /// Delete character in chat overlay input
    ChatOverlayDelete,
    /// Move cursor left in chat overlay
    ChatOverlayCursorLeft,
    /// Move cursor right in chat overlay
    ChatOverlayCursorRight,
    /// Navigate history up in chat overlay
    ChatOverlayHistoryUp,
    /// Navigate history down in chat overlay
    ChatOverlayHistoryDown,
    /// Send message in chat overlay
    ChatOverlaySend,
    /// Clear chat overlay messages
    ChatOverlayClear,
    /// Scroll up in chat overlay
    ChatOverlayScrollUp,
    /// Scroll down in chat overlay
    ChatOverlayScrollDown,
}
