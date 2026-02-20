//! Studio View - YAML editor with validation and task DAG
//!
//! Layout:
//! ```text
//! ┌─────────────────────────────────────────────────────┬───────────────────────┐
//! │ EDITOR                                              │ STRUCTURE             │
//! │ YAML with line numbers and syntax highlighting      │ Task DAG mini-view    │
//! ├─────────────────────────────────────────────────────┴───────────────────────┤
//! │ Valid YAML │ Schema OK │ 1 warning                                          │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Note: This implementation uses a simple line-based editor (`TextBuffer`).
//! Full tui-textarea integration is planned once ratatui 0.30 compatibility
//! is available (tui-textarea 0.7 requires ratatui 0.29).
//!
//! TODO: Track https://github.com/rhysd/tui-textarea/issues for ratatui 0.30 support

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Editor mode (vim-like)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
pub enum EditorMode {
    #[default]
    Normal,
    Insert,
}

/// Validation result
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
pub struct ValidationResult {
    pub yaml_valid: bool,
    pub schema_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self {
            yaml_valid: true,
            schema_valid: true,
            warnings: vec![],
            errors: vec![],
        }
    }
}

/// Simple line-based text buffer for the editor
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
pub struct TextBuffer {
    /// Lines of text
    lines: Vec<String>,
    /// Cursor row (0-indexed)
    cursor_row: usize,
    /// Cursor column (0-indexed)
    cursor_col: usize,
    /// Scroll offset (first visible line)
    scroll_offset: usize,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
        }
    }
}

#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
impl TextBuffer {
    /// Create a new text buffer from content
    pub fn from_content(content: &str) -> Self {
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
        }
    }

    /// Get all lines as a joined string
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// Get lines slice
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Get cursor position (row, col) - 0-indexed
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    /// Move cursor up
    pub fn cursor_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor_col();
            self.adjust_scroll();
        }
    }

    /// Move cursor down
    pub fn cursor_down(&mut self) {
        if self.cursor_row < self.lines.len().saturating_sub(1) {
            self.cursor_row += 1;
            self.clamp_cursor_col();
            self.adjust_scroll();
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
            self.adjust_scroll();
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row < self.lines.len().saturating_sub(1) {
            self.cursor_row += 1;
            self.cursor_col = 0;
            self.adjust_scroll();
        }
    }

    /// Insert a character at cursor
    pub fn insert_char(&mut self, c: char) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let col = self.cursor_col.min(line.len());
            line.insert(col, c);
            self.cursor_col = col + 1;
        }
    }

    /// Insert a newline at cursor
    pub fn insert_newline(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let col = self.cursor_col.min(line.len());
            let rest = line[col..].to_string();
            line.truncate(col);
            self.lines.insert(self.cursor_row + 1, rest);
            self.cursor_row += 1;
            self.cursor_col = 0;
            self.adjust_scroll();
        }
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            if let Some(line) = self.lines.get_mut(self.cursor_row) {
                let col = self.cursor_col.min(line.len());
                if col > 0 {
                    line.remove(col - 1);
                    self.cursor_col = col - 1;
                }
            }
        } else if self.cursor_row > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&current_line);
            self.adjust_scroll();
        }
    }

    /// Delete character at cursor
    pub fn delete(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let col = self.cursor_col.min(line.len());
            if col < line.len() {
                line.remove(col);
            } else if self.cursor_row < self.lines.len() - 1 {
                // Merge with next line
                let next_line = self.lines.remove(self.cursor_row + 1);
                self.lines[self.cursor_row].push_str(&next_line);
            }
        }
    }

    /// Get current line length
    fn current_line_len(&self) -> usize {
        self.lines
            .get(self.cursor_row)
            .map(|l| l.len())
            .unwrap_or(0)
    }

    /// Clamp cursor column to line length
    fn clamp_cursor_col(&mut self) {
        let line_len = self.current_line_len();
        self.cursor_col = self.cursor_col.min(line_len);
    }

    /// Adjust scroll to keep cursor visible
    fn adjust_scroll(&mut self) {
        // Keep 2 lines of context if possible
        if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        }
    }

    /// Adjust scroll for viewport height
    pub fn adjust_scroll_for_height(&mut self, height: usize) {
        let visible_end = self.scroll_offset + height.saturating_sub(1);
        if self.cursor_row >= visible_end {
            self.scroll_offset = self.cursor_row.saturating_sub(height.saturating_sub(2));
        } else if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        }
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }
}

/// Studio view state
#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
pub struct StudioView {
    /// File path being edited
    pub path: Option<PathBuf>,
    /// Text buffer
    pub buffer: TextBuffer,
    /// Editor mode
    pub mode: EditorMode,
    /// Validation result
    pub validation: ValidationResult,
    /// Whether file has unsaved changes
    pub modified: bool,
}

#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
impl StudioView {
    pub fn new() -> Self {
        Self {
            path: None,
            buffer: TextBuffer::default(),
            mode: EditorMode::Normal,
            validation: ValidationResult::default(),
            modified: false,
        }
    }

    /// Load a file into the editor
    pub fn load_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        self.buffer = TextBuffer::from_content(&content);
        self.path = Some(path);
        self.modified = false;
        self.validate();
        Ok(())
    }

    /// Save the file
    pub fn save_file(&mut self) -> Result<(), std::io::Error> {
        if let Some(path) = &self.path {
            let content = self.buffer.content();
            std::fs::write(path, content)?;
            self.modified = false;
        }
        Ok(())
    }

    /// Validate the YAML content
    pub fn validate(&mut self) {
        let content = self.buffer.content();

        // Check YAML validity
        match serde_yaml::from_str::<serde_yaml::Value>(&content) {
            Ok(_) => {
                self.validation.yaml_valid = true;
                self.validation.errors.clear();
            }
            Err(e) => {
                self.validation.yaml_valid = false;
                self.validation.errors = vec![e.to_string()];
            }
        }

        // TODO: Schema validation with jsonschema crate
        self.validation.schema_valid = self.validation.yaml_valid;
    }

    /// Get current line number (1-indexed)
    pub fn current_line(&self) -> usize {
        self.buffer.cursor().0 + 1
    }

    /// Get current column (1-indexed)
    pub fn current_col(&self) -> usize {
        self.buffer.cursor().1 + 1
    }
}

impl Default for StudioView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for StudioView {
    fn render(&self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Layout: Editor (70%) | Structure (30%) above, Validation bar below
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(chunks[0]);

        // Editor panel
        self.render_editor(frame, main_chunks[0], theme);

        // Structure panel
        self.render_structure(frame, main_chunks[1], theme);

        // Validation bar
        self.render_validation(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match self.mode {
            EditorMode::Normal => self.handle_normal_mode(key),
            EditorMode::Insert => self.handle_insert_mode(key),
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        let mode = match self.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
        };
        let modified = if self.modified { " ●" } else { "" };
        format!(
            "{} | Ln {}, Col {}{}",
            mode,
            self.current_line(),
            self.current_col(),
            modified
        )
    }
}

#[allow(dead_code)] // Will be used in Task 5.1 (App integration)
impl StudioView {
    fn handle_normal_mode(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Home),
            KeyCode::Char('i') => {
                self.mode = EditorMode::Insert;
                ViewAction::None
            }
            KeyCode::Char('c') => ViewAction::ToggleChatOverlay,
            KeyCode::F(5) => {
                if let Some(path) = &self.path {
                    ViewAction::RunWorkflow(path.clone())
                } else {
                    ViewAction::Error("No file loaded".to_string())
                }
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(e) = self.save_file() {
                    ViewAction::Error(format!("Save failed: {}", e))
                } else {
                    ViewAction::None
                }
            }
            KeyCode::Char('1') | KeyCode::Char('a') => ViewAction::SwitchView(TuiView::Chat),
            KeyCode::Char('2') | KeyCode::Char('h') => ViewAction::SwitchView(TuiView::Home),
            KeyCode::Char('4') | KeyCode::Char('m') => ViewAction::SwitchView(TuiView::Monitor),
            KeyCode::Up | KeyCode::Char('k') => {
                self.buffer.cursor_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.buffer.cursor_down();
                ViewAction::None
            }
            KeyCode::Left => {
                self.buffer.cursor_left();
                ViewAction::None
            }
            KeyCode::Right => {
                self.buffer.cursor_right();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn handle_insert_mode(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                self.mode = EditorMode::Normal;
                ViewAction::None
            }
            KeyCode::Up => {
                self.buffer.cursor_up();
                ViewAction::None
            }
            KeyCode::Down => {
                self.buffer.cursor_down();
                ViewAction::None
            }
            KeyCode::Left => {
                self.buffer.cursor_left();
                ViewAction::None
            }
            KeyCode::Right => {
                self.buffer.cursor_right();
                ViewAction::None
            }
            KeyCode::Enter => {
                self.buffer.insert_newline();
                self.modified = true;
                self.validate();
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.buffer.backspace();
                self.modified = true;
                self.validate();
                ViewAction::None
            }
            KeyCode::Delete => {
                self.buffer.delete();
                self.modified = true;
                self.validate();
                ViewAction::None
            }
            KeyCode::Char(c) => {
                self.buffer.insert_char(c);
                self.modified = true;
                self.validate();
                ViewAction::None
            }
            KeyCode::Tab => {
                // Insert 2 spaces for tab
                self.buffer.insert_char(' ');
                self.buffer.insert_char(' ');
                self.modified = true;
                self.validate();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render_editor(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mode_indicator = match self.mode {
            EditorMode::Normal => "",
            EditorMode::Insert => " [INSERT]",
        };
        let title = format!(" EDITOR{} ", mode_indicator);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Calculate visible height (excluding borders)
        let visible_height = inner.height as usize;

        // Build lines with line numbers
        let lines: Vec<Line> = self
            .buffer
            .lines()
            .iter()
            .enumerate()
            .skip(self.buffer.scroll_offset())
            .take(visible_height)
            .map(|(i, line)| {
                let line_num = i + 1;
                let is_cursor_line = i == self.buffer.cursor().0;

                let line_style = if is_cursor_line {
                    Style::default().bg(theme.highlight)
                } else {
                    Style::default()
                };

                Line::from(vec![
                    Span::styled(
                        format!("{:4} ", line_num),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::styled(line.as_str(), line_style),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);

        // Render scrollbar if content exceeds viewport
        if self.buffer.lines().len() > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state = ScrollbarState::new(self.buffer.lines().len())
                .position(self.buffer.scroll_offset());
            frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
        }
    }

    fn render_structure(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // TODO: Parse YAML and show task DAG
        let content = "Task structure\n(coming soon)";

        let paragraph = Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" STRUCTURE ")
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    fn render_validation(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let yaml_status = if self.validation.yaml_valid {
            Span::styled("Valid YAML", Style::default().fg(theme.status_success))
        } else {
            Span::styled("Invalid YAML", Style::default().fg(theme.status_failed))
        };

        let schema_status = if self.validation.schema_valid {
            Span::styled("Schema OK", Style::default().fg(theme.status_success))
        } else {
            Span::styled("Schema Error", Style::default().fg(theme.status_failed))
        };

        let warning_count = self.validation.warnings.len();
        let warning_status = if warning_count > 0 {
            Span::styled(
                format!("{} warning(s)", warning_count),
                Style::default().fg(theme.status_running), // Amber for warnings
            )
        } else {
            Span::styled("No warnings", Style::default().fg(theme.status_success))
        };

        let line = Line::from(vec![
            Span::raw(" "),
            yaml_status,
            Span::raw("  |  "),
            schema_status,
            Span::raw("  |  "),
            warning_status,
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

    // ═══════════════════════════════════════════════════════════════════════════
    // TextBuffer tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_text_buffer_default() {
        let buffer = TextBuffer::default();
        assert_eq!(buffer.lines().len(), 1);
        assert_eq!(buffer.lines()[0], "");
        assert_eq!(buffer.cursor(), (0, 0));
    }

    #[test]
    fn test_text_buffer_from_content() {
        let buffer = TextBuffer::from_content("line1\nline2\nline3");
        assert_eq!(buffer.lines().len(), 3);
        assert_eq!(buffer.lines()[0], "line1");
        assert_eq!(buffer.lines()[1], "line2");
        assert_eq!(buffer.lines()[2], "line3");
    }

    #[test]
    fn test_text_buffer_from_empty_content() {
        let buffer = TextBuffer::from_content("");
        assert_eq!(buffer.lines().len(), 1);
        assert_eq!(buffer.lines()[0], "");
    }

    #[test]
    fn test_text_buffer_content() {
        let buffer = TextBuffer::from_content("a\nb\nc");
        assert_eq!(buffer.content(), "a\nb\nc");
    }

    #[test]
    fn test_text_buffer_cursor_movement() {
        let mut buffer = TextBuffer::from_content("abc\ndef\nghi");

        // Move down
        buffer.cursor_down();
        assert_eq!(buffer.cursor(), (1, 0));

        // Move right
        buffer.cursor_right();
        buffer.cursor_right();
        assert_eq!(buffer.cursor(), (1, 2));

        // Move up (cursor col should clamp)
        buffer.cursor_up();
        assert_eq!(buffer.cursor(), (0, 2));

        // Move left
        buffer.cursor_left();
        assert_eq!(buffer.cursor(), (0, 1));
    }

    #[test]
    fn test_text_buffer_cursor_boundary() {
        let mut buffer = TextBuffer::from_content("ab\ncd");

        // Can't go up from first line
        buffer.cursor_up();
        assert_eq!(buffer.cursor(), (0, 0));

        // Go to last line
        buffer.cursor_down();
        buffer.cursor_down(); // Should stay at last line
        assert_eq!(buffer.cursor(), (1, 0));
    }

    #[test]
    fn test_text_buffer_insert_char() {
        let mut buffer = TextBuffer::default();
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.insert_char('c');
        assert_eq!(buffer.lines()[0], "abc");
        assert_eq!(buffer.cursor(), (0, 3));
    }

    #[test]
    fn test_text_buffer_insert_newline() {
        let mut buffer = TextBuffer::from_content("abc");
        buffer.cursor_right();
        buffer.cursor_right(); // cursor at position 2
        buffer.insert_newline();
        assert_eq!(buffer.lines().len(), 2);
        assert_eq!(buffer.lines()[0], "ab");
        assert_eq!(buffer.lines()[1], "c");
        assert_eq!(buffer.cursor(), (1, 0));
    }

    #[test]
    fn test_text_buffer_backspace() {
        let mut buffer = TextBuffer::from_content("abc");
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.backspace();
        assert_eq!(buffer.lines()[0], "ab");
        assert_eq!(buffer.cursor(), (0, 2));
    }

    #[test]
    fn test_text_buffer_backspace_merge_lines() {
        let mut buffer = TextBuffer::from_content("ab\ncd");
        buffer.cursor_down();
        buffer.backspace(); // Should merge lines
        assert_eq!(buffer.lines().len(), 1);
        assert_eq!(buffer.lines()[0], "abcd");
        assert_eq!(buffer.cursor(), (0, 2));
    }

    #[test]
    fn test_text_buffer_delete() {
        let mut buffer = TextBuffer::from_content("abc");
        buffer.cursor_right();
        buffer.delete();
        assert_eq!(buffer.lines()[0], "ac");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // StudioView tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_studio_view_new() {
        let view = StudioView::new();
        assert_eq!(view.mode, EditorMode::Normal);
        assert!(!view.modified);
        assert!(view.path.is_none());
    }

    #[test]
    fn test_studio_view_mode_switch() {
        let mut view = StudioView::new();
        assert_eq!(view.mode, EditorMode::Normal);

        view.mode = EditorMode::Insert;
        assert_eq!(view.mode, EditorMode::Insert);
    }

    #[test]
    fn test_studio_view_validation_valid_yaml() {
        let mut view = StudioView::new();

        // Valid YAML
        view.buffer = TextBuffer::from_content("key: value");
        view.validate();
        assert!(view.validation.yaml_valid);
        assert!(view.validation.errors.is_empty());
    }

    #[test]
    fn test_studio_view_validation_invalid_yaml() {
        let mut view = StudioView::new();

        // Invalid YAML
        view.buffer = TextBuffer::from_content("key: [unclosed");
        view.validate();
        assert!(!view.validation.yaml_valid);
        assert!(!view.validation.errors.is_empty());
    }

    #[test]
    fn test_studio_view_cursor_position() {
        let view = StudioView::new();
        assert_eq!(view.current_line(), 1);
        assert_eq!(view.current_col(), 1);
    }

    #[test]
    fn test_studio_view_default_validation_result() {
        let result = ValidationResult::default();
        assert!(result.yaml_valid);
        assert!(result.schema_valid);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_studio_view_status_line_normal_mode() {
        let view = StudioView::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("NORMAL"));
        assert!(status.contains("Ln 1"));
        assert!(status.contains("Col 1"));
    }

    #[test]
    fn test_studio_view_status_line_insert_mode() {
        let mut view = StudioView::new();
        view.mode = EditorMode::Insert;
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("INSERT"));
    }

    #[test]
    fn test_studio_view_status_line_modified() {
        let mut view = StudioView::new();
        view.modified = true;
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("●"));
    }

    #[test]
    fn test_editor_mode_default() {
        let mode = EditorMode::default();
        assert_eq!(mode, EditorMode::Normal);
    }

    #[test]
    fn test_studio_view_handle_normal_mode_quit() {
        let mut view = StudioView::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(KeyCode::Char('q'));
        let action = view.handle_key(key, &mut state);
        match action {
            ViewAction::SwitchView(TuiView::Home) => {}
            _ => panic!("Expected SwitchView(Home)"),
        }
    }

    #[test]
    fn test_studio_view_handle_normal_mode_insert() {
        let mut view = StudioView::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(KeyCode::Char('i'));
        let _ = view.handle_key(key, &mut state);
        assert_eq!(view.mode, EditorMode::Insert);
    }

    #[test]
    fn test_studio_view_handle_insert_mode_escape() {
        let mut view = StudioView::new();
        view.mode = EditorMode::Insert;
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(KeyCode::Esc);
        let _ = view.handle_key(key, &mut state);
        assert_eq!(view.mode, EditorMode::Normal);
    }

    #[test]
    fn test_studio_view_handle_insert_mode_typing() {
        let mut view = StudioView::new();
        view.mode = EditorMode::Insert;
        let mut state = TuiState::new("test.nika.yaml");

        // Type some characters
        view.handle_key(KeyEvent::from(KeyCode::Char('a')), &mut state);
        view.handle_key(KeyEvent::from(KeyCode::Char('b')), &mut state);
        view.handle_key(KeyEvent::from(KeyCode::Char('c')), &mut state);

        assert_eq!(view.buffer.lines()[0], "abc");
        assert!(view.modified);
    }
}
