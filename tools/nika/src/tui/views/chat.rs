//! Chat View - AI Agent conversation interface
//!
//! Layout:
//! ```text
//! +-----------------------------------------------------+-----------------------+
//! | Conversation history                                | SESSION               |
//! | - User messages                                     | Actions & context     |
//! | - Nika responses with inline results                |                       |
//! +-----------------------------------------------------+-----------------------+
//! | > Input field                                                               |
//! +-----------------------------------------------------------------------------+
//! ```

// Allow dead code - ChatView will be integrated in Task 5.1 (App integration)
#![allow(dead_code)]

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Message role in conversation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Nika,
    System,
}

/// A chat message
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: Instant,
    /// Optional inline execution result
    pub execution: Option<ExecutionResult>,
}

/// Inline execution result in chat
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub workflow_name: String,
    pub status: ExecutionStatus,
    pub tasks_completed: usize,
    pub tasks_total: usize,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Completed,
    Failed,
}

/// Session info sidebar
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub workflow_count: usize,
    pub last_run: Option<String>,
    pub recent_actions: Vec<String>,
    pub current_context: Option<String>,
}

/// Chat view state
pub struct ChatView {
    /// Conversation history
    pub messages: Vec<ChatMessage>,
    /// Current input buffer
    pub input: String,
    /// Input cursor position
    pub cursor: usize,
    /// Scroll offset in message list
    pub scroll: usize,
    /// Session info
    pub session: SessionInfo,
    /// Command history (for up/down navigation)
    pub history: Vec<String>,
    /// History navigation index
    pub history_index: Option<usize>,
}

impl ChatView {
    pub fn new() -> Self {
        Self {
            messages: vec![ChatMessage {
                role: MessageRole::System,
                content: "Welcome to Nika Agent. How can I help you?".to_string(),
                timestamp: Instant::now(),
                execution: None,
            }],
            input: String::new(),
            cursor: 0,
            scroll: 0,
            session: SessionInfo::default(),
            history: vec![],
            history_index: None,
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.clone(),
            timestamp: Instant::now(),
            execution: None,
        });
        self.history.push(content);
        self.history_index = None;
    }

    /// Add a Nika response
    pub fn add_nika_message(&mut self, content: String, execution: Option<ExecutionResult>) {
        self.messages.push(ChatMessage {
            role: MessageRole::Nika,
            content,
            timestamp: Instant::now(),
            execution,
        });
    }

    /// Submit current input
    pub fn submit(&mut self) -> Option<String> {
        if self.input.trim().is_empty() {
            return None;
        }
        let message = self.input.clone();
        self.add_user_message(message.clone());
        self.input.clear();
        self.cursor = 0;
        Some(message)
    }

    /// Navigate history up
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
            }
            Some(i) if i > 0 => {
                self.history_index = Some(i - 1);
            }
            _ => {}
        }
        if let Some(i) = self.history_index {
            self.input = self.history[i].clone();
            self.cursor = self.input.len();
        }
    }

    /// Navigate history down
    pub fn history_down(&mut self) {
        match self.history_index {
            Some(i) if i < self.history.len() - 1 => {
                self.history_index = Some(i + 1);
                self.input = self.history[i + 1].clone();
                self.cursor = self.input.len();
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    /// Insert character at cursor
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }
}

impl Default for ChatView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for ChatView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout: Messages (75%) | Session (25%) above, Input below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
            .split(chunks[0]);

        // Messages panel
        self.render_messages(frame, main_chunks[0], theme);

        // Session panel
        self.render_session(frame, main_chunks[1], theme);

        // Input panel
        self.render_input(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            KeyCode::Char('q') if self.input.is_empty() => ViewAction::Quit,
            KeyCode::Enter => {
                if let Some(message) = self.submit() {
                    ViewAction::SendChatMessage(message)
                } else {
                    ViewAction::None
                }
            }
            KeyCode::Up => {
                self.history_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.history_down();
                ViewAction::None
            }
            KeyCode::Left => {
                self.cursor_left();
                ViewAction::None
            }
            KeyCode::Right => {
                self.cursor_right();
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.backspace();
                ViewAction::None
            }
            KeyCode::Char(c) => {
                self.insert_char(c);
                ViewAction::None
            }
            KeyCode::Tab => ViewAction::SwitchView(TuiView::Home),
            KeyCode::Esc => ViewAction::SwitchView(TuiView::Home),
            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "{} messages | {} in history",
            self.messages.len(),
            self.history.len()
        )
    }
}

impl ChatView {
    fn render_messages(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let items: Vec<ListItem> = self
            .messages
            .iter()
            .flat_map(|msg| {
                let (prefix, style) = match msg.role {
                    MessageRole::User => ("You", Style::default().fg(theme.highlight)),
                    MessageRole::Nika => ("Nika", Style::default().fg(theme.status_success)),
                    MessageRole::System => ("System", Style::default().fg(theme.text_muted)),
                };

                let mut lines = vec![ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("-- {} ", prefix),
                        style.add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("-".repeat(20)),
                ]))];

                // Wrap message content
                for line in msg.content.lines() {
                    lines.push(ListItem::new(format!("  {}", line)));
                }

                // Add execution result if present
                if let Some(exec) = &msg.execution {
                    let status_icon = match exec.status {
                        ExecutionStatus::Running => ">",
                        ExecutionStatus::Completed => "+",
                        ExecutionStatus::Failed => "x",
                    };
                    lines.push(ListItem::new(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!(
                                "|-- {} {} ({}/{}) ",
                                status_icon,
                                exec.workflow_name,
                                exec.tasks_completed,
                                exec.tasks_total
                            ),
                            Style::default().fg(theme.text_secondary),
                        ),
                    ])));
                }

                lines.push(ListItem::new("")); // spacing
                lines
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" CONVERSATION ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(list, area);
    }

    fn render_session(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("Workflows: ", Style::default().fg(theme.text_muted)),
                Span::raw(self.session.workflow_count.to_string()),
            ]),
            Line::from(""),
            Line::styled("--- Actions ---", Style::default().fg(theme.text_muted)),
        ];

        for action in &self.session.recent_actions {
            lines.push(Line::from(format!("+ {}", action)));
        }

        let paragraph = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" SESSION ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_input(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Show input with cursor
        let before_cursor = &self.input[..self.cursor];
        let cursor_char = self.input.chars().nth(self.cursor).unwrap_or(' ');
        let after_cursor = if self.cursor < self.input.len() {
            &self.input[self.cursor + 1..]
        } else {
            ""
        };

        let line = Line::from(vec![
            Span::raw(" > "),
            Span::raw(before_cursor),
            Span::styled(
                cursor_char.to_string(),
                Style::default().bg(theme.highlight).fg(Color::Black),
            ),
            Span::raw(after_cursor),
        ]);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_view_new() {
        let view = ChatView::new();
        assert_eq!(view.messages.len(), 1); // Welcome message
        assert!(view.input.is_empty());
    }

    #[test]
    fn test_chat_view_submit() {
        let mut view = ChatView::new();
        view.input = "Hello Nika".to_string();
        view.cursor = view.input.len();

        let result = view.submit();
        assert_eq!(result, Some("Hello Nika".to_string()));
        assert!(view.input.is_empty());
        assert_eq!(view.messages.len(), 2); // Welcome + user message
    }

    #[test]
    fn test_chat_view_submit_empty() {
        let mut view = ChatView::new();
        view.input = "   ".to_string();

        let result = view.submit();
        assert_eq!(result, None);
    }

    #[test]
    fn test_chat_view_history_navigation() {
        let mut view = ChatView::new();
        view.add_user_message("First".to_string());
        view.add_user_message("Second".to_string());

        view.history_up();
        assert_eq!(view.input, "Second");

        view.history_up();
        assert_eq!(view.input, "First");

        view.history_down();
        assert_eq!(view.input, "Second");
    }

    #[test]
    fn test_chat_view_history_down_clears_input() {
        let mut view = ChatView::new();
        view.add_user_message("Test".to_string());

        view.history_up();
        assert_eq!(view.input, "Test");

        view.history_down();
        assert!(view.input.is_empty());
    }

    #[test]
    fn test_chat_view_cursor() {
        let mut view = ChatView::new();
        view.insert_char('H');
        view.insert_char('i');
        assert_eq!(view.input, "Hi");
        assert_eq!(view.cursor, 2);

        view.cursor_left();
        assert_eq!(view.cursor, 1);

        view.insert_char('e');
        assert_eq!(view.input, "Hei");

        view.backspace();
        assert_eq!(view.input, "Hi");
    }

    #[test]
    fn test_chat_view_cursor_right() {
        let mut view = ChatView::new();
        view.input = "Hello".to_string();
        view.cursor = 0;

        view.cursor_right();
        assert_eq!(view.cursor, 1);

        view.cursor_right();
        view.cursor_right();
        view.cursor_right();
        view.cursor_right();
        assert_eq!(view.cursor, 5);

        // Should not go past the end
        view.cursor_right();
        assert_eq!(view.cursor, 5);
    }

    #[test]
    fn test_chat_view_backspace_at_start() {
        let mut view = ChatView::new();
        view.input = "Hi".to_string();
        view.cursor = 0;

        view.backspace();
        assert_eq!(view.input, "Hi");
        assert_eq!(view.cursor, 0);
    }

    #[test]
    fn test_chat_view_add_nika_message() {
        let mut view = ChatView::new();
        view.add_nika_message("Hello!".to_string(), None);

        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[1].role, MessageRole::Nika);
        assert_eq!(view.messages[1].content, "Hello!");
    }

    #[test]
    fn test_chat_view_add_nika_message_with_execution() {
        let mut view = ChatView::new();
        let exec = ExecutionResult {
            workflow_name: "test.nika.yaml".to_string(),
            status: ExecutionStatus::Completed,
            tasks_completed: 3,
            tasks_total: 3,
            output: Some("Done".to_string()),
        };
        view.add_nika_message("Workflow completed".to_string(), Some(exec));

        assert_eq!(view.messages.len(), 2);
        assert!(view.messages[1].execution.is_some());
        let exec = view.messages[1].execution.as_ref().unwrap();
        assert_eq!(exec.status, ExecutionStatus::Completed);
    }

    #[test]
    fn test_message_role_equality() {
        assert_eq!(MessageRole::User, MessageRole::User);
        assert_ne!(MessageRole::User, MessageRole::Nika);
        assert_ne!(MessageRole::Nika, MessageRole::System);
    }

    #[test]
    fn test_execution_status_equality() {
        assert_eq!(ExecutionStatus::Running, ExecutionStatus::Running);
        assert_ne!(ExecutionStatus::Running, ExecutionStatus::Completed);
        assert_ne!(ExecutionStatus::Completed, ExecutionStatus::Failed);
    }

    #[test]
    fn test_session_info_default() {
        let session = SessionInfo::default();
        assert_eq!(session.workflow_count, 0);
        assert!(session.last_run.is_none());
        assert!(session.recent_actions.is_empty());
        assert!(session.current_context.is_none());
    }

    #[test]
    fn test_chat_view_status_line() {
        let view = ChatView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("1 messages"));
        assert!(status.contains("0 in history"));
    }

    #[test]
    fn test_chat_view_default() {
        let view = ChatView::default();
        assert_eq!(view.messages.len(), 1);
        assert!(view.input.is_empty());
    }
}
