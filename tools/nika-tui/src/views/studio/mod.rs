// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
//! Full text editor integration is planned via `ratatui-textarea` (the ratatui org's
//! hard fork of tui-textarea with ratatui 0.30 support).
//!
//! See: <https://github.com/ratatui/ratatui-textarea> (replaces rhysd/tui-textarea)

mod types;
pub use types::*;

mod buffer;
pub use buffer::TextBuffer;

mod editor;
pub use editor::YamlEditorPanel;

mod lsp;
mod render_browser;
mod render_dag;
mod render_editor;
mod syntax;

use camino::Utf8Path;
use std::path::PathBuf;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_lsp_core::analysis::context::detect_context;
use nika_lsp_core::handler::LspHandler;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Span,
    widgets::{Block, Borders, Widget},
    Frame,
};

use super::view_trait::View;
use super::ViewAction;
use crate::state::TuiState;
use crate::theme::Theme;
use crate::views::TuiView;
use crate::widgets::tree::{
    build_git_status_cache, AnimationTicker, FilterConfig, GitStatusCache, TreeAction, TreeColors,
    TreeFilter, TreeNode, TreeState,
};
use crate::widgets::{
    centered_rect, CommandPalette, CommandPaletteState, StatusMessage, WhichKey, WhichKeyState,
};

// ═══════════════════════════════════════════════════════════════════════════════
// StudioView: Main 3-Panel Struct
// ═══════════════════════════════════════════════════════════════════════════════

/// Studio View - Unified 3-panel layout: Browser | Editor | DAG
///
/// This is the main view for workflow development, combining:
/// - **Browser**: TreeWidget for file navigation (VS Code-like)
/// - **Editor**: YamlEditorPanel for YAML editing with validation
/// - **DAG**: Structure preview showing task dependencies
pub struct StudioView {
    // === Browser Panel (Left) - Real TreeWidget ===
    /// Root directory for file browsing
    pub root_dir: PathBuf,
    /// Tree widget state (selection, expansion)
    pub tree_state: TreeState,
    /// Tree widget colors (from theme)
    pub tree_colors: TreeColors,
    /// Animation ticker for glow effects
    pub animation_ticker: AnimationTicker,
    /// File filter configuration (W/A/E/0 hotkeys)
    pub filter_config: FilterConfig,
    /// Cached git status for files (refreshed periodically)
    git_cache: GitStatusCache,
    /// Last time the git cache was refreshed
    git_cache_time: Instant,
    /// Cached tree root node (avoids rebuilding on every frame)
    cached_tree: Option<TreeNode>,
    /// Quick Access: recently used .nika.yaml files
    quick_access: Vec<PathBuf>,

    // === Editor Panel (Center) ===
    /// YAML editor panel with validation and syntax highlighting
    pub editor: YamlEditorPanel,

    // === Focus & Layout ===
    /// Currently focused panel
    pub focus: StudioFocus,
    /// Current panel ratio
    pub ratio: StudioRatio,

    // === Overlays ===
    /// Command palette state (Ctrl+P)
    pub command_palette: CommandPaletteState,
    /// Which-key popup state (g, z, \[, \], Space prefixes)
    pub which_key: WhichKeyState,
}

impl Default for StudioView {
    fn default() -> Self {
        Self::new()
    }
}

impl StudioView {
    /// Create a new StudioView with current working directory
    pub fn new() -> Self {
        let root_dir = std::env::current_dir().unwrap_or_default();
        Self::with_root(root_dir)
    }

    /// Create a StudioView with a specific root directory
    ///
    /// PERF: Git status and tree are built lazily in on_enter(), not here.
    /// This makes construction instant — the skeleton frame renders first,
    /// then on_enter() populates the data (100-300ms saved from startup).
    pub fn with_root(root_dir: PathBuf) -> Self {
        // PERF: Defer git cache + tree build to on_enter() (non-blocking construction)
        let git_cache = GitStatusCache::default();

        // Scan for .nika.yaml files for Quick Access
        let quick_access = Self::scan_nika_files(&root_dir);

        Self {
            root_dir,
            tree_state: TreeState::default(),
            tree_colors: TreeColors::default(),
            animation_ticker: AnimationTicker::new(),
            filter_config: FilterConfig::default(),
            editor: YamlEditorPanel::new(),
            focus: StudioFocus::default(),
            ratio: StudioRatio::default(),
            git_cache,
            git_cache_time: Instant::now(),
            cached_tree: None, // Built lazily in on_enter()
            quick_access,
            // Overlay states
            command_palette: CommandPaletteState::new(),
            which_key: WhichKeyState::new(),
        }
    }

    /// Scan for .nika.yaml files (max 5 total: root-level first, then workflows/)
    fn scan_nika_files(root: &PathBuf) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // First pass: root level .nika.yaml files
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.filter_map(|e| e.ok()) {
                // Security: skip symlinks (same guard as tree browser)
                if entry.file_type().ok().is_some_and(|ft| ft.is_symlink()) {
                    continue;
                }
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".nika.yaml") {
                        files.push(path);
                    }
                }
            }
        }

        // Second pass: workflows/ directory
        let workflows_dir = root.join("workflows");
        if workflows_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&workflows_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    // Security: skip symlinks
                    if entry.file_type().ok().is_some_and(|ft| ft.is_symlink()) {
                        continue;
                    }
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".nika.yaml") && files.len() < 5 {
                            files.push(path);
                        }
                    }
                }
            }
        }

        // Limit to 5 files
        files.truncate(5);
        files
    }

    /// Load a file into the editor panel (returns Result for error handling)
    pub fn load_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let result = self.editor.load_file(path);
        self.focus = StudioFocus::Editor;
        result
    }

    /// Open a file in the editor panel (ignores errors)
    pub fn open_file(&mut self, path: PathBuf) {
        // Ignore errors for now - user will see empty editor
        let _ = self.load_file(path);
    }

    /// Get border style based on focus state
    fn border_style(&self, panel: StudioFocus, theme: &Theme) -> Style {
        use ratatui::style::Modifier;
        if self.focus == panel {
            Style::default()
                .fg(theme.border_focused)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        }
    }

    /// Refresh the tree cache (called when filesystem changes are expected)
    pub fn refresh_tree(&mut self) {
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));
        self.git_cache = build_git_status_cache(root_path);
        self.git_cache_time = Instant::now();
        self.cached_tree = None;
    }

    /// Get or build the cached tree node (borrows from cache, zero-clone)
    fn ensure_tree(&mut self) {
        if self.cached_tree.is_none() {
            let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));
            let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
            self.cached_tree = Some(tree);
        }
    }

    /// Handle keyboard input for the browser panel
    fn handle_browser_key(&mut self, key: KeyEvent) -> ViewAction {
        // PERF: Ensure tree is built, then borrow from cache (zero-clone)
        self.ensure_tree();

        // Try TreeAction navigation first
        if let Some(action) = TreeAction::from_key_event(key) {
            // Only update visible_nodes for structure-changing actions
            let needs_update = matches!(
                action,
                TreeAction::Toggle | TreeAction::Left | TreeAction::Right
            );
            if let Some(ref root_node) = self.cached_tree {
                self.tree_state.handle_action(action, root_node);
                if needs_update {
                    self.tree_state.update_visible_nodes(root_node);
                }
            }
            return ViewAction::None;
        }

        // Handle other keys
        match key.code {
            KeyCode::Enter => {
                // Open selected file in editor
                if let Some(node_id) = self.tree_state.selected() {
                    let node_path = self
                        .cached_tree
                        .as_ref()
                        .and_then(|root| self.tree_state.find_node(root, node_id))
                        .map(|node| PathBuf::from(node.path.as_str()));
                    if let Some(path) = node_path {
                        if path.is_file()
                            && path.extension().is_some_and(|e| e == "yaml" || e == "yml")
                        {
                            self.open_file(path);
                        }
                    }
                }
                ViewAction::None
            }
            // Refresh tree (R key)
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_tree();
                ViewAction::None
            }
            // Filter hotkeys (W/A/E/0)
            // Invalidate cached_tree so visible_nodes updates correctly
            KeyCode::Char('w') | KeyCode::Char('W') => {
                self.filter_config.set_filter(TreeFilter::WorkflowsOnly);
                self.cached_tree = None; // Force rebuild with new filter
                ViewAction::None
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.filter_config.set_filter(TreeFilter::All);
                self.cached_tree = None; // Force rebuild with new filter
                ViewAction::None
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.filter_config.set_filter(TreeFilter::EcosystemOnly);
                self.cached_tree = None; // Force rebuild with new filter
                ViewAction::None
            }
            KeyCode::Char('0') => {
                self.filter_config.reset();
                self.cached_tree = None; // Force rebuild with new filter
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }
}

impl View for StudioView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Main layout: 3 horizontal panels + status bar at bottom
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
            .split(area);

        // Split main area into 3 horizontal panels
        let panel_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(self.ratio.constraints())
            .split(main_chunks[0]);

        // Render each panel
        self.render_browser(frame, panel_chunks[0], theme);

        // Editor panel with focused border
        // UX: Add focus indicator ● at start, modified indicator ◆ after name
        let editor_style = self.border_style(StudioFocus::Editor, theme);
        let focus_indicator = if self.focus == StudioFocus::Editor {
            "●"
        } else {
            " "
        };
        let modified_indicator = if self.editor.modified { " ◆" } else { "" };
        let editor_block = Block::default()
            .title(Span::styled(
                format!(" {} Editor{} ", focus_indicator, modified_indicator),
                editor_style,
            ))
            .borders(Borders::ALL)
            .border_style(editor_style);

        let editor_inner = editor_block.inner(panel_chunks[1]);
        frame.render_widget(editor_block, panel_chunks[1]);
        self.editor.render_editor(frame, editor_inner, theme);

        // DAG panel
        self.render_dag_panel(frame, panel_chunks[2], theme);

        // Validation bar at bottom
        self.editor.render_validation(frame, main_chunks[1], theme);

        // Render overlays on top
        // Command palette (Ctrl+P)
        if self.command_palette.visible {
            let palette_area = centered_rect(60, 50, area);
            CommandPalette::new(&self.command_palette).render(palette_area, frame.buffer_mut());
        }

        // Which-key popup (vim-style)
        if self.which_key.is_visible() {
            WhichKey::new(&self.which_key).render(area, frame.buffer_mut());
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        // Handle command palette overlay first
        if self.command_palette.visible {
            return self.handle_palette_key(key);
        }

        // Handle which-key popup
        if self.which_key.is_visible() || self.which_key.is_pending() {
            if let KeyCode::Char(c) = key.code {
                if let Some((prefix, key_char)) = self.which_key.on_key(c) {
                    return self.handle_which_key_action(prefix, key_char);
                }
            }
            if key.code == KeyCode::Esc {
                self.which_key.close();
                return ViewAction::None;
            }
        }

        // Detect which-key prefixes in Normal mode
        // Only trigger for g, z, [, ], Space when not in Insert mode
        if self.focus != StudioFocus::Editor || self.editor.mode != EditorMode::Insert {
            if let KeyCode::Char(c) = key.code {
                if key.modifiers == KeyModifiers::NONE && self.which_key.on_prefix(c) {
                    // Prefix detected - wait for popup or next key
                    return ViewAction::None;
                }
            }
        }

        // Global shortcuts first
        match (key.code, key.modifiers) {
            // Ctrl+S: Save file (global — works from any panel and any editor mode)
            (KeyCode::Char('s'), mods) if mods.contains(KeyModifiers::CONTROL) => {
                match self.editor.save_file() {
                    Ok(()) => {
                        let name = self
                            .editor
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
                        state
                            .status_messages
                            .push(StatusMessage::success(format!("Saved: {}", name)));
                    }
                    Err(e) => {
                        state
                            .status_messages
                            .push(StatusMessage::error(format!("Save failed: {}", e)));
                    }
                }
                return ViewAction::None;
            }
            // Ctrl+P: Open command palette
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.command_palette.open();
                return ViewAction::None;
            }
            // Tab: Cycle focus forward (but NOT when editor is in Insert mode)
            // Allow Tab for indentation in Insert mode
            (KeyCode::Tab, KeyModifiers::NONE) => {
                if self.focus == StudioFocus::Editor && self.editor.mode == EditorMode::Insert {
                    // Let editor handle Tab for indentation
                    return self.editor.handle_key(key, state);
                }
                self.focus = self.focus.next();
                return ViewAction::None;
            }
            // Shift+Tab: Cycle focus backward (or dedent in Insert mode)
            (KeyCode::BackTab, _) => {
                if self.focus == StudioFocus::Editor && self.editor.mode == EditorMode::Insert {
                    // Let editor handle Shift+Tab for dedent
                    return self.editor.handle_key(key, state);
                }
                self.focus = self.focus.prev();
                return ViewAction::None;
            }
            // Ctrl+]: Cycle ratio
            (KeyCode::Char(']'), KeyModifiers::CONTROL) => {
                self.ratio = self.ratio.next();
                return ViewAction::None;
            }
            // Ctrl+T or Shift+T: Toggle theme
            (KeyCode::Char('T'), _) | (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                return ViewAction::ToggleTheme;
            }
            // View switching: number keys 2-3
            (KeyCode::Char('2'), _) => return ViewAction::SwitchView(TuiView::Command),
            (KeyCode::Char('3'), _) => return ViewAction::SwitchView(TuiView::Control),
            _ => {}
        }

        // Dispatch to focused panel
        match self.focus {
            StudioFocus::Browser => self.handle_browser_key(key),
            StudioFocus::Editor => self.editor.handle_key(key, state),
            StudioFocus::Dag => {
                // DAG panel is read-only, but support scroll
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.editor.dag_scroll = self.editor.dag_scroll.saturating_sub(1);
                        ViewAction::None
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.editor.dag_scroll = self.editor.dag_scroll.saturating_add(1);
                        ViewAction::None
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        self.editor.toggle_dag_mode();
                        ViewAction::None
                    }
                    _ => ViewAction::None,
                }
            }
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        let mode = match self.editor.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
        };
        // UX: Highlight focus panel with ● prefix
        format!(
            "Studio | ●{} | {} | {} | [Tab] Panel • [Ctrl+]] Ratio",
            self.focus.title(),
            self.ratio.title(),
            mode
        )
    }

    fn tick(&mut self, _state: &mut TuiState) {
        // Tick animation for tree widget glow effects
        self.animation_ticker.tick();
        // Tick editor (matrix rain, etc.)
        self.editor.tick();
        // Run debounced validation
        self.editor.maybe_validate();
        // Tick which-key popup for delay timing
        self.which_key.tick();
    }

    fn on_enter(&mut self, _state: &mut TuiState) {
        // PERF: Reuse git cache + tree from with_root() constructor.
        // Only rebuild if cache is stale (> 5s) or missing.
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));

        // Refresh git cache only if stale
        if self.git_cache.is_empty()
            || self.git_cache_time.elapsed() > std::time::Duration::from_secs(5)
        {
            self.git_cache = build_git_status_cache(root_path);
            self.git_cache_time = Instant::now();
        }

        // Reuse cached tree if available, else build once
        if self.cached_tree.is_none() {
            let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
            self.cached_tree = Some(tree);
        }

        // Init navigation state from cached tree (fast, in-memory only)
        if let Some(ref root_node) = self.cached_tree {
            self.tree_state.update_visible_nodes(root_node);
            if !self.tree_state.is_expanded(root_node.id) {
                self.tree_state.expand(root_node.id);
                self.tree_state.update_visible_nodes(root_node);
            }
            self.tree_state.select_first_if_none();
        }
    }

    fn on_leave(&mut self, _state: &mut TuiState) {
        // Save session state when leaving
        // Could persist open files, cursor positions, etc.
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Command Palette & Which-Key Handling
// ═══════════════════════════════════════════════════════════════════════════════

impl StudioView {
    /// Handle keyboard input when command palette is open
    fn handle_palette_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Esc => {
                self.command_palette.close();
                ViewAction::None
            }
            KeyCode::Enter => {
                // Clone the command ID to avoid borrow conflict
                let cmd_id = self
                    .command_palette
                    .selected_command()
                    .map(|cmd| cmd.id.clone());
                if let Some(id) = cmd_id {
                    let action = self.execute_palette_command(&id);
                    self.command_palette.close();
                    return action;
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
                self.command_palette.query.push(c);
                self.command_palette.update_filter();
                ViewAction::None
            }
            KeyCode::Backspace => {
                self.command_palette.query.pop();
                self.command_palette.update_filter();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    /// Execute a command from the palette
    fn execute_palette_command(&mut self, cmd_id: &str) -> ViewAction {
        match cmd_id {
            // View switching (3-view: Studio, Command, Control)
            "studio" => ViewAction::SwitchView(TuiView::Studio),
            "command" | "chat" | "home" | "monitor" => ViewAction::SwitchView(TuiView::Command),
            "control" => ViewAction::SwitchView(TuiView::Control),
            // Workflow actions
            "run" => {
                // Run the current workflow
                if let Some(path) = &self.editor.path {
                    ViewAction::RunWorkflow(path.clone())
                } else {
                    ViewAction::None
                }
            }
            "validate" => {
                // Trigger validation
                self.editor.validate();
                ViewAction::None
            }
            // Help - switch to Settings view which has help info
            "help" => ViewAction::SwitchView(TuiView::Control),
            _ => ViewAction::None,
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Which-Key Popup Handling
    // ═══════════════════════════════════════════════════════════════════════════

    /// Handle a which-key action (prefix + follow-up key)
    fn handle_which_key_action(&mut self, prefix: char, key: char) -> ViewAction {
        match (prefix, key) {
            // g prefix - Go to commands
            ('g', 'g') => {
                // Go to top of file
                self.editor.buffer.cursor_row = 0;
                self.editor.buffer.cursor_col = 0;
                self.editor.buffer.scroll_offset = 0;
                ViewAction::None
            }
            ('g', 'e') => {
                // Go to end of file
                let last_line = self.editor.buffer.lines.len().saturating_sub(1);
                self.editor.buffer.cursor_row = last_line;
                self.editor.buffer.cursor_col = 0;
                ViewAction::None
            }
            ('g', 'd') => {
                // Go to definition via LSP
                let content = self.editor.buffer.content();
                let offset = self.editor.buffer.cursor_position() as u32;
                let context = detect_context(&content, offset, None);
                if let Some(result) = self
                    .editor
                    .lsp_handler
                    .definition(&content, offset, &context)
                {
                    // DefinitionResult has byte offsets — convert to cursor position
                    self.editor
                        .buffer
                        .set_cursor_position(result.offset as usize);
                }
                ViewAction::None
            }
            // z prefix - View/centering commands
            ('z', 'z') => {
                // Center cursor in viewport
                let visible = self.editor.buffer.visible_height.get();
                let target_scroll = self.editor.buffer.cursor_row.saturating_sub(visible / 2);
                self.editor.buffer.scroll_offset = target_scroll;
                ViewAction::None
            }
            ('z', 't') => {
                // Cursor to top of viewport
                self.editor.buffer.scroll_offset = self.editor.buffer.cursor_row;
                ViewAction::None
            }
            ('z', 'b') => {
                // Cursor to bottom of viewport
                let visible = self.editor.buffer.visible_height.get();
                self.editor.buffer.scroll_offset = self
                    .editor
                    .buffer
                    .cursor_row
                    .saturating_sub(visible.saturating_sub(1));
                ViewAction::None
            }
            // Space (leader) prefix - Common commands
            (' ', 'w') => {
                // Save file
                match self.editor.save_file() {
                    Ok(()) => {
                        let name = self
                            .editor
                            .path
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
                        ViewAction::StatusMessage(StatusMessage::success(format!(
                            "Saved: {}",
                            name
                        )))
                    }
                    Err(e) => {
                        tracing::error!("Failed to save: {}", e);
                        ViewAction::StatusMessage(StatusMessage::error(format!(
                            "Save failed: {}",
                            e
                        )))
                    }
                }
            }
            (' ', 'f') => {
                // Find file (open command palette)
                self.command_palette.open();
                ViewAction::None
            }
            (' ', 'e') => {
                // Toggle browser focus
                self.focus = StudioFocus::Browser;
                ViewAction::None
            }
            (' ', 'q') => ViewAction::Quit,
            _ => ViewAction::None,
        }
    }
}

#[cfg(test)]
mod tests;
