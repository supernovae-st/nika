// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! YAML Editor Panel — the center panel of the Studio view.
//!
//! Contains the `YamlEditorPanel` struct (text buffer, validation, LSP,
//! undo/redo, clipboard, git gutter) and its `View` impl for rendering
//! and key handling.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_lsp_core::handler::DefaultHandler;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::diagnostics::DiagnosticsEngine;
use crate::edit_history::EditHistory;
use crate::git::GitStatus;
use crate::state::TuiState;
use crate::theme::Theme;
use crate::views::view_trait::View;
use crate::views::{TuiView, ViewAction};
use nika_engine::ast::schema_validator::WorkflowSchemaValidator;
use nika_engine::error::NikaError;
use nika_engine::util::atomic_write;

use super::buffer::TextBuffer;
use super::types::*;

// ═══════════════════════════════════════════════════════════════════════════════
// YamlEditorPanel
// ═══════════════════════════════════════════════════════════════════════════════

/// Debounce delay for validation (ms)
const VALIDATION_DEBOUNCE_MS: u64 = 300;

pub struct YamlEditorPanel {
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
    /// DAG expanded mode (toggle with 'E')
    pub dag_expanded: bool,
    /// DAG scroll offset for vertical scrolling
    pub dag_scroll: u16,
    /// Whether validation is pending (debounced)
    pub(crate) validation_pending: bool,
    /// Time of last edit (for debouncing)
    pub(crate) last_edit_time: Option<Instant>,
    /// Cached parsed workflow (RefCell for interior mutability in render)
    pub(crate) cached_workflow: RefCell<Option<nika_engine::ast::Workflow>>,
    /// Content hash when workflow was cached (invalidation)
    pub(crate) cached_content_hash: Cell<u64>,
    /// Animation frame counter (0-255, wraps)
    pub frame: u8,
    /// Edit history for undo/redo support (Ctrl+Z/Ctrl+Y)
    pub(crate) edit_history: EditHistory,
    /// Diagnostics engine for real-time error display (gutter + underline)
    pub(crate) diagnostics: DiagnosticsEngine,
    /// System clipboard for copy/paste
    pub(crate) clipboard: Option<arboard::Clipboard>,
    /// Git status for line-level change indicators
    pub(crate) git_status: Option<GitStatus>,
    /// In-process LSP handler for completion + hover (Phase 4)
    pub(crate) lsp_handler: DefaultHandler,
    /// Inline completion popup state
    pub completion: CompletionState,
    /// Hover tooltip state
    pub hover: HoverState,
    /// Code action popup state (Ctrl+.)
    pub code_actions: CodeActionState,
}

impl YamlEditorPanel {
    pub fn new() -> Self {
        Self {
            path: None,
            buffer: TextBuffer::default(),
            mode: EditorMode::Normal,
            validation: ValidationResult::default(),
            modified: false,
            dag_expanded: false,
            dag_scroll: 0,
            validation_pending: false,
            last_edit_time: None,
            cached_workflow: RefCell::new(None),
            cached_content_hash: Cell::new(0),
            frame: 0,
            // Edit History (Undo/Redo)
            edit_history: EditHistory::new(100),
            // Real-time Diagnostics
            diagnostics: DiagnosticsEngine::new(),
            // Clipboard (graceful fallback if unavailable)
            clipboard: arboard::Clipboard::new().ok(),
            // Git gutter (initialized on file load)
            git_status: None,
            // In-process LSP (Phase 4)
            lsp_handler: DefaultHandler::new(),
            completion: CompletionState::default(),
            hover: HoverState::default(),
            // Code actions (Phase 5)
            code_actions: CodeActionState::default(),
        }
    }

    /// Tick animation frame (called from main loop)
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Mark content as edited - validation will run after debounce delay
    pub(super) fn mark_edited(&mut self) {
        self.modified = true;
        self.validation_pending = true;
        self.last_edit_time = Some(Instant::now());
        // Push state to edit history for undo/redo
        let content = self.buffer.content();
        let cursor = self.buffer.cursor_position();
        self.edit_history.push(&content, cursor);
    }

    /// Simple hash for cache invalidation
    pub(super) fn content_hash(content: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Check if debounced validation should run (call in render/tick)
    pub fn maybe_validate(&mut self) {
        if !self.validation_pending {
            return;
        }
        if let Some(last_edit) = self.last_edit_time {
            if last_edit.elapsed() >= Duration::from_millis(VALIDATION_DEBOUNCE_MS) {
                self.validate();
                self.validation_pending = false;
            }
        }
    }

    /// Toggle DAG expanded/minimal mode
    pub fn toggle_dag_mode(&mut self) {
        self.dag_expanded = !self.dag_expanded;
    }

    /// Load a file into the editor
    pub fn load_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        self.buffer = TextBuffer::from_content(&content);
        self.path = Some(path.clone());
        self.modified = false;
        self.dag_scroll = 0;
        self.validate();
        // Initialize edit history with loaded content
        self.edit_history.init(&content, 0);
        // Initialize git status for gutter display
        self.git_status = GitStatus::open(&path);
        Ok(())
    }

    /// Save the file using atomic write (temp+rename) for data integrity
    pub fn save_file(&mut self) -> Result<(), std::io::Error> {
        if let Some(path) = &self.path {
            let content = self.buffer.content();
            atomic_write(path, content.as_bytes())?;
            self.modified = false;
            self.buffer.clear_modified();
        }
        Ok(())
    }

    /// Undo the last edit (Ctrl+Z)
    fn undo(&mut self) {
        if let Some((text, cursor)) = self.edit_history.undo() {
            self.buffer.set_content(&text);
            self.buffer.set_cursor_position(cursor);
            self.modified = true;
            self.validation_pending = true;
            self.last_edit_time = Some(Instant::now());
        }
    }

    /// Redo the last undone edit (Ctrl+Y or Ctrl+Shift+Z)
    fn redo(&mut self) {
        if let Some((text, cursor)) = self.edit_history.redo() {
            self.buffer.set_content(&text);
            self.buffer.set_cursor_position(cursor);
            self.modified = true;
            self.validation_pending = true;
            self.last_edit_time = Some(Instant::now());
        }
    }

    /// Validate the YAML content
    pub fn validate(&mut self) {
        let content = self.buffer.content();
        self.validation.errors.clear();
        self.validation.warnings.clear();

        // Phase 1: Check YAML syntax validity
        // Note: Using serde_json::Value as target since serde-saphyr doesn't export Value type
        match nika_engine::serde_yaml::from_str::<serde_json::Value>(&content) {
            Ok(_) => {
                self.validation.yaml_valid = true;
            }
            Err(e) => {
                self.validation.yaml_valid = false;
                self.validation.schema_valid = false;
                self.validation.errors = vec![format!("YAML syntax error: {}", e)];
                return; // Can't validate schema if YAML is invalid
            }
        }

        // Phase 2: Validate against Nika workflow schema
        match WorkflowSchemaValidator::new() {
            Ok(validator) => match validator.validate_yaml(&content) {
                Ok(()) => {
                    self.validation.schema_valid = true;
                }
                Err(NikaError::SchemaValidationFailed { errors }) => {
                    self.validation.schema_valid = false;
                    self.validation.errors = errors
                        .into_iter()
                        .map(|e| format!("{}: {}", e.path, e.message))
                        .collect();
                }
                Err(e) => {
                    // Other validation error (e.g., YAML parse error from validator)
                    self.validation.schema_valid = false;
                    self.validation.errors = vec![e.to_string()];
                }
            },
            Err(e) => {
                // Schema validator creation failed
                self.validation.schema_valid = false;
                self.validation.warnings = vec![format!("Schema validator unavailable: {}", e)];
            }
        }

        // Phase 3: Two-Phase IR diagnostics
        // Provides precise span locations for gutter icons and underlines
        self.diagnostics.analyze(&content);
    }

    /// Get current line number (1-indexed)
    pub fn current_line(&self) -> usize {
        self.buffer.cursor().0 + 1
    }

    /// Get current column (1-indexed)
    pub fn current_col(&self) -> usize {
        self.buffer.cursor().1 + 1
    }

    /// Get diagnostics engine reference (for gutter and underline rendering)
    pub fn diagnostics(&self) -> &DiagnosticsEngine {
        &self.diagnostics
    }

    /// Check if line has any diagnostics (for gutter icon)
    pub fn line_has_diagnostic(&self, line: usize) -> bool {
        self.diagnostics.has_diagnostics_on_line(line)
    }

    /// Get error count from diagnostics
    pub fn diagnostic_error_count(&self) -> usize {
        self.diagnostics.error_count()
    }
}

impl Default for YamlEditorPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl View for YamlEditorPanel {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
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

        // Undo/Redo indicator (⤺N ⤻M format)
        let undo_depth = self.edit_history.undo_depth();
        let redo_depth = self.edit_history.redo_depth();
        let undo_redo = if undo_depth > 0 || redo_depth > 0 {
            format!(" | ⤺{} ⤻{}", undo_depth, redo_depth)
        } else {
            String::new()
        };

        format!(
            "{} | Ln {}, Col {}{}{}",
            mode,
            self.current_line(),
            self.current_col(),
            modified,
            undo_redo
        )
    }
}

impl YamlEditorPanel {
    fn handle_normal_mode(&mut self, key: KeyEvent) -> ViewAction {
        // Dismiss hover on any key press in normal mode
        self.hover.visible = false;

        // ── Code action popup key interception (Normal mode) ────────────
        if self.code_actions.visible {
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    let len = self.code_actions.actions.len();
                    self.code_actions.selected =
                        (self.code_actions.selected + 1).min(len.saturating_sub(1));
                    return ViewAction::None;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.code_actions.selected = self.code_actions.selected.saturating_sub(1);
                    return ViewAction::None;
                }
                KeyCode::Enter => {
                    self.accept_code_action();
                    return ViewAction::None;
                }
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.code_actions.visible = false;
                    return ViewAction::None;
                }
                _ => {
                    self.code_actions.visible = false;
                }
            }
        }

        // ── Ctrl+.: trigger code actions ────────────────────────────────
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('.') {
            self.trigger_code_actions();
            return ViewAction::None;
        }

        match key.code {
            // K: Hover documentation (vim-style)
            KeyCode::Char('K') => {
                self.trigger_hover();
                ViewAction::None
            }
            KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Studio),
            // Ctrl+S to save file (must be before plain 's')
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(e) = self.save_file() {
                    ViewAction::Error(format!("Save failed: {}", e))
                } else {
                    ViewAction::None
                }
            }
            // Ctrl+Z for undo
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.undo();
                ViewAction::None
            }
            // Ctrl+Y or Ctrl+Shift+Z for redo
            KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.redo();
                ViewAction::None
            }
            KeyCode::Char('Z')
                if key
                    .modifiers
                    .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT) =>
            {
                self.redo();
                ViewAction::None
            }
            // 's' opens Settings view
            KeyCode::Char('s') => ViewAction::OpenControl,
            // Shift+T or Ctrl+t toggles theme
            KeyCode::Char('T') => ViewAction::ToggleTheme,
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ViewAction::ToggleTheme
            }
            KeyCode::Char('i') => {
                self.mode = EditorMode::Insert;
                ViewAction::None
            }
            KeyCode::Char('c') => ViewAction::SwitchView(TuiView::Command),
            // Toggle DAG expanded/minimal mode
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.toggle_dag_mode();
                ViewAction::None
            }
            KeyCode::F(5) => {
                if let Some(path) = &self.path {
                    ViewAction::RunWorkflow(path.clone())
                } else {
                    ViewAction::Error("No file loaded".to_string())
                }
            }
            // View switching: number keys
            // 1=Studio (current), 2=Command, 3=Control
            KeyCode::Char('2') => ViewAction::SwitchView(TuiView::Command),
            KeyCode::Char('3') => ViewAction::SwitchView(TuiView::Control),
            KeyCode::Up | KeyCode::Char('k') => {
                self.buffer.cursor_up();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.buffer.cursor_down();
                ViewAction::None
            }
            KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.cursor_word_left();
                ViewAction::None
            }
            KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.cursor_word_right();
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
            KeyCode::Home | KeyCode::Char('0') => {
                self.buffer.cursor_home();
                ViewAction::None
            }
            KeyCode::End | KeyCode::Char('$') => {
                self.buffer.cursor_end();
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.buffer.page_up();
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.buffer.page_down();
                ViewAction::None
            }
            // Vim: 'w' for word-forward, 'b' for word-backward
            KeyCode::Char('w') => {
                self.buffer.cursor_word_right();
                ViewAction::None
            }
            KeyCode::Char('b') => {
                self.buffer.cursor_word_left();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn handle_insert_mode(&mut self, key: KeyEvent) -> ViewAction {
        // Dismiss hover on any key press in insert mode
        self.hover.visible = false;

        // ── Completion popup key interception ──────────────────────────
        // When the popup is visible, intercept navigation + accept keys.
        if self.completion.visible {
            match key.code {
                KeyCode::Down => {
                    let len = self.completion.items.len();
                    self.completion.selected =
                        (self.completion.selected + 1).min(len.saturating_sub(1));
                    return ViewAction::None;
                }
                KeyCode::Up => {
                    self.completion.selected = self.completion.selected.saturating_sub(1);
                    return ViewAction::None;
                }
                KeyCode::Enter | KeyCode::Tab => {
                    self.accept_completion();
                    return ViewAction::None;
                }
                KeyCode::Esc => {
                    self.completion.visible = false;
                    return ViewAction::None;
                }
                _ => {
                    // Dismiss popup, fall through to normal handling
                    self.completion.visible = false;
                }
            }
        }

        // ── Ctrl+Space: explicit completion trigger ───────────────────
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char(' ') {
            self.trigger_completion();
            return ViewAction::None;
        }

        // ── Code action popup key interception ──────────────────────────
        if self.code_actions.visible {
            match key.code {
                KeyCode::Down => {
                    let len = self.code_actions.actions.len();
                    self.code_actions.selected =
                        (self.code_actions.selected + 1).min(len.saturating_sub(1));
                    return ViewAction::None;
                }
                KeyCode::Up => {
                    self.code_actions.selected = self.code_actions.selected.saturating_sub(1);
                    return ViewAction::None;
                }
                KeyCode::Enter => {
                    self.accept_code_action();
                    return ViewAction::None;
                }
                KeyCode::Esc => {
                    self.code_actions.visible = false;
                    return ViewAction::None;
                }
                _ => {
                    self.code_actions.visible = false;
                }
            }
        }

        // ── Ctrl+.: trigger code actions ────────────────────────────────
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('.') {
            self.trigger_code_actions();
            return ViewAction::None;
        }

        // Handle Ctrl+Z and Ctrl+Y before other key handlers
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('z') => {
                    self.undo();
                    return ViewAction::None;
                }
                KeyCode::Char('y') => {
                    self.redo();
                    return ViewAction::None;
                }
                // Ctrl+A for select all
                KeyCode::Char('a') => {
                    self.buffer.select_all();
                    return ViewAction::None;
                }
                // Ctrl+C for copy
                KeyCode::Char('c') => {
                    if let Some(text) = self.buffer.get_selected_text() {
                        if let Some(ref mut clipboard) = self.clipboard {
                            let _ = clipboard.set_text(text);
                        }
                    }
                    return ViewAction::None;
                }
                // Ctrl+X for cut
                KeyCode::Char('x') => {
                    if let Some(text) = self.buffer.get_selected_text() {
                        if let Some(ref mut clipboard) = self.clipboard {
                            let _ = clipboard.set_text(text);
                        }
                        self.buffer.delete_selection();
                        self.mark_edited();
                    }
                    return ViewAction::None;
                }
                // Ctrl+V for paste
                KeyCode::Char('v') => {
                    if let Some(ref mut clipboard) = self.clipboard {
                        if let Ok(text) = clipboard.get_text() {
                            // Delete selection first if active
                            if self.buffer.has_selection() {
                                self.buffer.delete_selection();
                            }
                            // Insert pasted text character by character
                            for c in text.chars() {
                                if c == '\n' {
                                    self.buffer.insert_newline();
                                } else if c != '\r' {
                                    self.buffer.insert_char(c);
                                }
                            }
                            self.mark_edited();
                        }
                    }
                    return ViewAction::None;
                }
                // Ctrl+D for select next occurrence (multi-cursor)
                KeyCode::Char('d') => {
                    self.buffer.select_next_occurrence();
                    return ViewAction::None;
                }
                // Escape clears additional cursors first
                KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Ctrl+G: Clear additional cursors (back to single cursor mode)
                    if self.buffer.is_multi_cursor() {
                        self.buffer.clear_additional_cursors();
                        return ViewAction::None;
                    }
                }
                _ => {}
            }
            // Ctrl+Arrow word movement (checked after Ctrl+char shortcuts)
            match key.code {
                KeyCode::Left => {
                    self.buffer.cursor_word_left();
                    return ViewAction::None;
                }
                KeyCode::Right => {
                    self.buffer.cursor_word_right();
                    return ViewAction::None;
                }
                _ => {}
            }
        }
        // Ctrl+Shift+Z for redo
        if key
            .modifiers
            .contains(KeyModifiers::CONTROL | KeyModifiers::SHIFT)
            && matches!(key.code, KeyCode::Char('Z'))
        {
            self.redo();
            return ViewAction::None;
        }

        // Shift+Arrow for selection extension
        let extending = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            KeyCode::Esc => {
                // Clear selection first, then exit insert mode
                if self.buffer.has_selection() {
                    self.buffer.clear_selection();
                } else {
                    self.mode = EditorMode::Normal;
                }
                ViewAction::None
            }
            KeyCode::Up => {
                if extending && !self.buffer.has_selection() {
                    self.buffer.start_selection();
                }
                self.buffer.cursor_up();
                if extending {
                    self.buffer.sync_selection_to_cursor();
                } else {
                    self.buffer.clear_selection();
                }
                ViewAction::None
            }
            KeyCode::Down => {
                if extending && !self.buffer.has_selection() {
                    self.buffer.start_selection();
                }
                self.buffer.cursor_down();
                if extending {
                    self.buffer.sync_selection_to_cursor();
                } else {
                    self.buffer.clear_selection();
                }
                ViewAction::None
            }
            KeyCode::Left => {
                if extending && !self.buffer.has_selection() {
                    self.buffer.start_selection();
                }
                self.buffer.cursor_left();
                if extending {
                    self.buffer.sync_selection_to_cursor();
                } else {
                    self.buffer.clear_selection();
                }
                ViewAction::None
            }
            KeyCode::Right => {
                if extending && !self.buffer.has_selection() {
                    self.buffer.start_selection();
                }
                self.buffer.cursor_right();
                if extending {
                    self.buffer.sync_selection_to_cursor();
                } else {
                    self.buffer.clear_selection();
                }
                ViewAction::None
            }
            KeyCode::Home => {
                if extending && !self.buffer.has_selection() {
                    self.buffer.start_selection();
                }
                self.buffer.cursor_home();
                if extending {
                    self.buffer.sync_selection_to_cursor();
                } else {
                    self.buffer.clear_selection();
                }
                ViewAction::None
            }
            KeyCode::End => {
                if extending && !self.buffer.has_selection() {
                    self.buffer.start_selection();
                }
                self.buffer.cursor_end();
                if extending {
                    self.buffer.sync_selection_to_cursor();
                } else {
                    self.buffer.clear_selection();
                }
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.buffer.page_up();
                self.buffer.clear_selection();
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.buffer.page_down();
                self.buffer.clear_selection();
                ViewAction::None
            }
            KeyCode::Enter => {
                // Delete selection before inserting newline
                if self.buffer.has_selection() {
                    self.buffer.delete_selection();
                }
                self.buffer.insert_newline();
                self.mark_edited();
                ViewAction::None
            }
            KeyCode::Backspace => {
                // Delete selection if active
                if self.buffer.has_selection() {
                    self.buffer.delete_selection();
                    self.mark_edited();
                } else {
                    self.buffer.backspace();
                    self.mark_edited();
                }
                ViewAction::None
            }
            KeyCode::Delete => {
                // Delete selection if active
                if self.buffer.has_selection() {
                    self.buffer.delete_selection();
                    self.mark_edited();
                } else {
                    self.buffer.delete();
                    self.mark_edited();
                }
                ViewAction::None
            }
            KeyCode::Char(c) => {
                // Replace selection with typed character
                if self.buffer.has_selection() {
                    self.buffer.delete_selection();
                }
                self.buffer.insert_char(c);
                self.mark_edited();
                // Auto-trigger completion on trigger characters
                if matches!(c, ':' | '.' | '$' | '{' | ' ') {
                    self.trigger_completion();
                }
                ViewAction::None
            }
            KeyCode::Tab => {
                // Replace selection with tab spaces
                if self.buffer.has_selection() {
                    self.buffer.delete_selection();
                }
                self.buffer.insert_char(' ');
                self.buffer.insert_char(' ');
                self.mark_edited();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }
}
