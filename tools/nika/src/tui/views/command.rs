//! Command View — Fusion of Chat + Runner
//!
//! The Command view merges the Chat agent and Runner execution monitoring
//! into a single unified view with two internal modes:
//!
//! - **Chat mode** (default): Conversational AI interface with inline TaskBoxes
//! - **Monitor mode**: Real-time workflow execution dashboard (4-panel layout)
//!
//! Users switch modes with Ctrl+M. Workflows auto-switch to Monitor mode.
//! Both views stay alive — chat history persists while monitoring.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

use super::chat::ChatView;
use super::monitor::MonitorView;
use super::{View, ViewAction};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Internal mode within CommandView
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandMode {
    /// Chat agent conversation (default)
    #[default]
    Chat,
    /// Workflow execution monitoring (auto-switched when workflow starts)
    Monitor,
}

/// Command View — Chat agent + workflow execution monitoring
pub struct CommandView {
    /// Current internal mode
    pub mode: CommandMode,
    /// Chat view (always alive — preserves conversation)
    pub chat: ChatView,
    /// Monitor view (always alive — preserves execution state)
    pub monitor: MonitorView,
}

impl CommandView {
    pub fn new() -> Self {
        Self {
            mode: CommandMode::Chat,
            chat: ChatView::new(),
            monitor: MonitorView::new(),
        }
    }

    /// Switch to Monitor mode (called when workflow execution starts)
    pub fn switch_to_monitor(&mut self) {
        self.mode = CommandMode::Monitor;
    }

    /// Switch to Chat mode
    #[allow(dead_code)]
    pub fn switch_to_chat(&mut self) {
        self.mode = CommandMode::Chat;
    }

    /// Check if currently in chat mode
    #[allow(dead_code)]
    pub fn is_chat(&self) -> bool {
        self.mode == CommandMode::Chat
    }
}

impl View for CommandView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        match self.mode {
            CommandMode::Chat => self.chat.render(frame, area, state, theme),
            CommandMode::Monitor => self.monitor.render(frame, area, state, theme),
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        // Ctrl+M: Toggle between Chat and Monitor modes
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('m') {
            self.mode = match self.mode {
                CommandMode::Chat => CommandMode::Monitor,
                CommandMode::Monitor => CommandMode::Chat,
            };
            return ViewAction::None;
        }

        match self.mode {
            CommandMode::Chat => self.chat.handle_key(key, state),
            CommandMode::Monitor => self.monitor.handle_key(key, state),
        }
    }

    fn status_line(&self, state: &TuiState) -> String {
        match self.mode {
            CommandMode::Chat => self.chat.status_line(state),
            CommandMode::Monitor => self.monitor.status_line(state),
        }
    }

    fn on_enter(&mut self, state: &mut TuiState) {
        match self.mode {
            CommandMode::Chat => self.chat.on_enter(state),
            CommandMode::Monitor => self.monitor.on_enter(state),
        }
    }

    fn on_leave(&mut self, state: &mut TuiState) {
        match self.mode {
            CommandMode::Chat => self.chat.on_leave(state),
            CommandMode::Monitor => self.monitor.on_leave(state),
        }
    }

    fn tick(&mut self, state: &mut TuiState) {
        // Always tick both — monitor tracks execution in background while
        // chat is visible, and chat may be streaming while monitor is visible
        self.chat.tick();
        self.monitor.tick(state);
    }
}
