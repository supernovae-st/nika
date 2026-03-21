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

use crate::ast::parse_workflow;
use camino::Utf8Path;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

use super::view_trait::View;
use super::ViewAction;
use crate::ast::schema_validator::WorkflowSchemaValidator;
use crate::ast::Workflow;
use crate::error::NikaError;
use crate::tui::diagnostics::DiagnosticsEngine;
use crate::tui::edit_history::EditHistory;
use crate::tui::git::{GitStatus, LineChange};
use crate::tui::selection::{Position, Selection, SelectionSet};
use crate::tui::state::TuiState;
use crate::tui::theme::{TaskStatus, Theme, VerbColor};
use crate::tui::views::TuiView;
use crate::tui::widgets::tree::{
    build_git_status_cache, AnimationTicker, FilterConfig, GitStatusCache, TreeAction, TreeColors,
    TreeFilter, TreeNode, TreeState, TreeWidget,
};
use crate::tui::widgets::{
    centered_rect, CommandPalette, CommandPaletteState, DagAscii, MatrixRain, NodeBoxData,
    NodeBoxMode, ScrollIndicator, WhichKey, WhichKeyState,
};
use crate::util::atomic_write;

/// Editor mode (vim-like)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    #[default]
    Normal,
    Insert,
}

// ═══════════════════════════════════════════════════════════════════════════════
// StudioView: 3-Panel Layout
// Browser (20%) | Editor (50%) | DAG Structure (30%)
// ═══════════════════════════════════════════════════════════════════════════════

/// Panel focus in 3-panel Studio layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StudioFocus {
    /// Left panel: File browser (TreeWidget)
    #[default]
    Browser,
    /// Center panel: YAML editor
    Editor,
    /// Right panel: DAG structure (read-only)
    Dag,
}

impl StudioFocus {
    /// Cycle to next panel (Browser -> Editor -> Dag -> Browser)
    pub fn next(&self) -> Self {
        match self {
            StudioFocus::Browser => StudioFocus::Editor,
            StudioFocus::Editor => StudioFocus::Dag,
            StudioFocus::Dag => StudioFocus::Browser,
        }
    }

    /// Cycle to previous panel (Browser <- Editor <- Dag <- Browser)
    pub fn prev(&self) -> Self {
        match self {
            StudioFocus::Browser => StudioFocus::Dag,
            StudioFocus::Editor => StudioFocus::Browser,
            StudioFocus::Dag => StudioFocus::Editor,
        }
    }

    /// Display name for status bar
    pub fn title(&self) -> &'static str {
        match self {
            StudioFocus::Browser => "Browser",
            StudioFocus::Editor => "Editor",
            StudioFocus::Dag => "DAG",
        }
    }
}

/// Layout ratios for 3-panel Studio
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StudioRatio {
    /// Balanced: 20% / 50% / 30%
    #[default]
    Balanced,
    /// Editor focus: 15% / 65% / 20%
    EditorFocus,
    /// Browser focus: 35% / 45% / 20%
    BrowserFocus,
    /// DAG focus: 15% / 35% / 50%
    DagFocus,
}

impl StudioRatio {
    /// Get constraints for Layout::split()
    pub fn constraints(&self) -> [Constraint; 3] {
        match self {
            StudioRatio::Balanced => [
                Constraint::Percentage(20),
                Constraint::Percentage(50),
                Constraint::Percentage(30),
            ],
            StudioRatio::EditorFocus => [
                Constraint::Percentage(15),
                Constraint::Percentage(65),
                Constraint::Percentage(20),
            ],
            StudioRatio::BrowserFocus => [
                Constraint::Percentage(35),
                Constraint::Percentage(45),
                Constraint::Percentage(20),
            ],
            StudioRatio::DagFocus => [
                Constraint::Percentage(15),
                Constraint::Percentage(35),
                Constraint::Percentage(50),
            ],
        }
    }

    /// Cycle to next ratio (Balanced -> EditorFocus -> BrowserFocus -> DagFocus -> Balanced)
    pub fn next(&self) -> Self {
        match self {
            StudioRatio::Balanced => StudioRatio::EditorFocus,
            StudioRatio::EditorFocus => StudioRatio::BrowserFocus,
            StudioRatio::BrowserFocus => StudioRatio::DagFocus,
            StudioRatio::DagFocus => StudioRatio::Balanced,
        }
    }

    /// Display name for status bar
    pub fn title(&self) -> &'static str {
        match self {
            StudioRatio::Balanced => "Balanced",
            StudioRatio::EditorFocus => "Editor+",
            StudioRatio::BrowserFocus => "Browser+",
            StudioRatio::DagFocus => "DAG+",
        }
    }
}

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
    pub fn with_root(root_dir: PathBuf) -> Self {
        // Build git status cache from root directory
        let root_path = Utf8Path::from_path(&root_dir).unwrap_or(Utf8Path::new("."));
        let git_cache = build_git_status_cache(root_path);

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
            cached_tree: None,
            quick_access,
            // Overlay states
            command_palette: CommandPaletteState::new(),
            which_key: WhichKeyState::new(),
        }
    }

    /// Scan for .nika.yaml files in the root directory (max 5, top-level first)
    fn scan_nika_files(root: &PathBuf) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // First pass: root level .nika.yaml files
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.filter_map(|e| e.ok()) {
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

    /// Render the browser panel with TreeWidget
    fn render_browser(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let style = self.border_style(StudioFocus::Browser, theme);

        // UX: Add focus indicator ● to show which panel is active
        let focus_indicator = if self.focus == StudioFocus::Browser {
            "●"
        } else {
            " "
        };

        // Show filter badge in title so users know which filter is active
        let title = if self.filter_config.filter.badge().is_empty() {
            format!(" {} Browser ", focus_indicator)
        } else {
            format!(
                " {} Browser [{}] ",
                focus_indicator,
                self.filter_config.filter.badge()
            )
        };

        let block = Block::default()
            .title(Span::styled(title, style))
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split into Quick Access (top) + Tree (bottom)
        let quick_access_height = if self.quick_access.is_empty() {
            0
        } else {
            // Header (1) + files (n) + separator (1)
            (self.quick_access.len() + 2) as u16
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(quick_access_height), Constraint::Min(3)])
            .split(inner);

        // Render Quick Access section
        if !self.quick_access.is_empty() {
            self.render_quick_access(frame, chunks[0], theme);
        }

        let tree_area = chunks[1];

        // Refresh git cache periodically (every 5 seconds)
        const GIT_CACHE_TTL: Duration = Duration::from_secs(5);
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));

        if self.git_cache_time.elapsed() > GIT_CACHE_TTL {
            self.git_cache = build_git_status_cache(root_path);
            self.git_cache_time = Instant::now();
            // Invalidate tree cache when git status changes
            self.cached_tree = None;
        }

        // Build tree with caching (rebuild only when cache is empty)
        // Track if tree was just rebuilt to avoid per-frame overhead
        let tree_rebuilt = self.cached_tree.is_none();
        let root_node = if let Some(ref cached) = self.cached_tree {
            cached.clone()
        } else {
            let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
            self.cached_tree = Some(tree.clone());
            tree
        };

        // Only update visible_nodes when tree structure changes
        // This was being called EVERY frame causing severe performance issues
        if tree_rebuilt {
            self.tree_state.update_visible_nodes(&root_node);
            self.tree_state.select_first_if_none();
            // Ensure root is expanded on rebuild
            if !self.tree_state.is_expanded(root_node.id) {
                self.tree_state.expand(root_node.id);
                self.tree_state.update_visible_nodes(&root_node);
            }
        }

        // Use theme-aware colors so tree background matches the app background
        self.tree_colors = TreeColors::from_theme(theme);

        // Render TreeWidget with state
        let tree_widget = TreeWidget::new(&root_node)
            .colors(self.tree_colors.clone())
            .ticker(&self.animation_ticker)
            .filter(self.filter_config.clone());

        frame.render_stateful_widget(tree_widget, tree_area, &mut self.tree_state);
    }

    /// Render Quick Access section
    fn render_quick_access(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = Vec::new();

        // Header with emoji
        lines.push(Line::from(vec![
            Span::styled("⚡ ", Style::default().fg(theme.status_running)),
            Span::styled(
                "QUICK ACCESS",
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        // Quick access files
        for path in &self.quick_access {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("🦋 ", Style::default()),
                Span::styled(name, Style::default().fg(theme.border_focused)),
            ]));
        }

        // Separator
        let sep_width = area.width.saturating_sub(2) as usize;
        lines.push(Line::from(Span::styled(
            "─".repeat(sep_width),
            Style::default().fg(theme.border_normal),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    /// Render the DAG panel (delegates to editor's DAG rendering)
    fn render_dag_panel(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let style = self.border_style(StudioFocus::Dag, theme);

        // UX: Add focus indicator ● to show which panel is active
        let focus_indicator = if self.focus == StudioFocus::Dag {
            "●"
        } else {
            " "
        };

        let block = Block::default()
            .title(Span::styled(
                format!(" {} DAG Preview ", focus_indicator),
                style,
            ))
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Render DAG structure from editor's cached workflow
        let yaml = self.editor.buffer.content();
        self.editor.render_dag_structure(frame, inner, &yaml, theme);
    }

    /// Refresh the tree cache (called when filesystem changes are expected)
    pub fn refresh_tree(&mut self) {
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));
        self.git_cache = build_git_status_cache(root_path);
        self.git_cache_time = Instant::now();
        self.cached_tree = None;
    }

    /// Get or build the cached tree node
    fn get_or_build_tree(&mut self) -> TreeNode {
        if let Some(ref cached) = self.cached_tree {
            return cached.clone();
        }
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));
        let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
        self.cached_tree = Some(tree.clone());
        tree
    }

    /// Handle keyboard input for the browser panel
    fn handle_browser_key(&mut self, key: KeyEvent) -> ViewAction {
        // Get cached tree for tree operations
        let root_node = self.get_or_build_tree();

        // Try TreeAction navigation first
        if let Some(action) = TreeAction::from_key_event(key) {
            // Only update visible_nodes for structure-changing actions
            let needs_update = matches!(
                action,
                TreeAction::Toggle | TreeAction::Left | TreeAction::Right
            );
            self.tree_state.handle_action(action, &root_node);
            if needs_update {
                self.tree_state.update_visible_nodes(&root_node);
            }
            return ViewAction::None;
        }

        // Handle other keys
        match key.code {
            KeyCode::Enter => {
                // Open selected file in editor
                if let Some(node_id) = self.tree_state.selected() {
                    if let Some(node) = self.tree_state.find_node(&root_node, node_id) {
                        let path = PathBuf::from(node.path.as_str());
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
        // Root cause: visible_nodes was never populated at startup,
        // causing keyboard navigation to fail.

        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));

        // 1. Refresh git cache
        self.git_cache = build_git_status_cache(root_path);
        self.git_cache_time = Instant::now();

        // 2. Build tree and cache it
        let root_node = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
        self.cached_tree = Some(root_node.clone());

        // 3. Update visible_nodes for navigation
        self.tree_state.update_visible_nodes(&root_node);

        // 4. Expand root directory so children are visible
        self.tree_state.expand(root_node.id);

        // 5. Re-update visible_nodes after expansion (to include children)
        self.tree_state.update_visible_nodes(&root_node);

        // 6. Select first item for immediate navigation
        self.tree_state.select_first_if_none();
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
                if let Err(e) = self.editor.save_file() {
                    tracing::error!("Failed to save: {}", e);
                }
                ViewAction::None
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

// ═══════════════════════════════════════════════════════════════════════════════
// YamlEditorPanel and TextBuffer
// ═══════════════════════════════════════════════════════════════════════════════

/// Validation result
#[derive(Debug, Clone)]
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
#[derive(Debug)]
pub struct TextBuffer {
    /// Lines of text
    lines: Vec<String>,
    /// Cursor row (0-indexed)
    cursor_row: usize,
    /// Cursor column (0-indexed)
    cursor_col: usize,
    /// Scroll offset (first visible line)
    scroll_offset: usize,
    /// Last known viewport height — Cell allows update via &self from render path
    visible_height: Cell<usize>,
    /// Track if buffer has unsaved changes
    modified: bool,
    /// Selection state (anchor/head model)
    /// Upgraded to SelectionSet for multi-cursor support
    selections: SelectionSet,
}

impl Clone for TextBuffer {
    fn clone(&self) -> Self {
        Self {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            scroll_offset: self.scroll_offset,
            visible_height: Cell::new(self.visible_height.get()),
            modified: self.modified,
            selections: self.selections.clone(),
        }
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            visible_height: Cell::new(20),
            modified: false,
            selections: SelectionSet::origin(),
        }
    }
}

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
            visible_height: Cell::new(20),
            modified: false, // Fresh content is not modified
            selections: SelectionSet::origin(),
        }
    }

    /// Check if buffer has unsaved changes
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Mark buffer as modified (called after any edit)
    pub fn mark_modified(&mut self) {
        self.modified = true;
    }

    /// Clear modified flag (called after save)
    pub fn clear_modified(&mut self) {
        self.modified = false;
    }

    /// Update the known viewport height (called by renderer each frame via &self)
    pub fn set_visible_height(&self, height: usize) {
        if height > 0 {
            self.visible_height.set(height);
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
            self.modified = true; // track modification
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
            self.modified = true; // track modification
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
                    self.modified = true; // track modification
                }
            }
        } else if self.cursor_row > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&current_line);
            self.modified = true; // track modification
            self.adjust_scroll();
        }
    }

    /// Delete character at cursor
    pub fn delete(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let col = self.cursor_col.min(line.len());
            if col < line.len() {
                line.remove(col);
                self.modified = true; // track modification
            } else if self.cursor_row < self.lines.len() - 1 {
                // Merge with next line
                let next_line = self.lines.remove(self.cursor_row + 1);
                self.lines[self.cursor_row].push_str(&next_line);
                self.modified = true; // track modification
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

    /// Adjust scroll to keep cursor visible (both up and down)
    fn adjust_scroll(&mut self) {
        let height = self.visible_height.get();
        // Scroll up: cursor above viewport
        if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        }
        // Scroll down: cursor below viewport
        if height > 0 && self.cursor_row >= self.scroll_offset + height {
            self.scroll_offset = self.cursor_row - height + 1;
        }
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get current selection (primary selection from SelectionSet)
    pub fn selection(&self) -> &Selection {
        self.selections.primary()
    }

    /// Get mutable selection (primary selection from SelectionSet)
    pub fn selection_mut(&mut self) -> &mut Selection {
        self.selections.primary_mut()
    }

    /// Get all selections (for multi-cursor support)
    pub fn selections(&self) -> &SelectionSet {
        &self.selections
    }

    /// Get mutable selections (for multi-cursor support)
    pub fn selections_mut(&mut self) -> &mut SelectionSet {
        &mut self.selections
    }

    /// Check if there is an active selection (any selection is active)
    pub fn has_selection(&self) -> bool {
        self.selections.has_active_selection()
    }

    /// Check if multi-cursor mode is active
    pub fn is_multi_cursor(&self) -> bool {
        self.selections.is_multi_cursor()
    }

    /// Get number of cursors
    pub fn cursor_count(&self) -> usize {
        self.selections.cursor_count()
    }

    /// Clear all selections (collapse to primary cursor position)
    pub fn clear_selection(&mut self) {
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections.move_all_to(pos);
    }

    /// Clear additional cursors, keep primary
    pub fn clear_additional_cursors(&mut self) {
        self.selections.clear_additional();
    }

    /// Start a new selection at current cursor position
    pub fn start_selection(&mut self) {
        // Reset to single cursor mode with fresh selection
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections = SelectionSet::new(pos);
    }

    /// Extend selection to current cursor position
    pub fn extend_selection(&mut self) {
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections.primary_mut().extend_to(pos);
    }

    /// Sync selection head with cursor (call after cursor movement when extending)
    pub fn sync_selection_to_cursor(&mut self) {
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections.primary_mut().extend_to(pos);
    }

    /// Add a cursor at position (multi-cursor support)
    pub fn add_cursor(&mut self, line: usize, col: usize) {
        let pos = Position::new(line, col);
        self.selections.add_selection(pos);
    }

    /// Get the selected text as a string (primary selection)
    pub fn get_selected_text(&self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }

        let selection = self.selections.primary();
        let start = selection.start();
        let end = selection.end();

        if start.line == end.line {
            // Single line selection
            let line = self.lines.get(start.line)?;
            let start_col = start.col.min(line.len());
            let end_col = end.col.min(line.len());
            Some(line[start_col..end_col].to_string())
        } else {
            // Multi-line selection
            let mut result = String::new();

            // First line (from start_col to end)
            if let Some(first_line) = self.lines.get(start.line) {
                let start_col = start.col.min(first_line.len());
                result.push_str(&first_line[start_col..]);
            }

            // Middle lines (full lines)
            for line_idx in (start.line + 1)..end.line {
                result.push('\n');
                if let Some(line) = self.lines.get(line_idx) {
                    result.push_str(line);
                }
            }

            // Last line (from start to end_col)
            if let Some(last_line) = self.lines.get(end.line) {
                result.push('\n');
                let end_col = end.col.min(last_line.len());
                result.push_str(&last_line[..end_col]);
            }

            Some(result)
        }
    }

    /// Delete selected text and collapse selection (primary selection)
    pub fn delete_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }

        let selection = self.selections.primary();
        let start = selection.start();
        let end = selection.end();

        if start.line == end.line {
            // Single line deletion
            if let Some(line) = self.lines.get_mut(start.line) {
                let start_col = start.col.min(line.len());
                let end_col = end.col.min(line.len());
                line.replace_range(start_col..end_col, "");
            }
        } else {
            // Multi-line deletion
            // Keep start of first line + end of last line
            let first_part = self
                .lines
                .get(start.line)
                .map(|l| l[..start.col.min(l.len())].to_string())
                .unwrap_or_default();

            let last_part = self
                .lines
                .get(end.line)
                .map(|l| l[end.col.min(l.len())..].to_string())
                .unwrap_or_default();

            // Remove lines from start.line to end.line (inclusive)
            self.lines.drain(start.line..=end.line);

            // Insert merged line
            let merged = format!("{}{}", first_part, last_part);
            self.lines.insert(start.line, merged);
        }

        // Move cursor to selection start
        self.cursor_row = start.line;
        self.cursor_col = start.col;
        self.clear_selection();
        self.modified = true;

        true
    }

    /// Select all text
    pub fn select_all(&mut self) {
        let start = Position::new(0, 0);
        let last_line = self.lines.len().saturating_sub(1);
        let last_col = self.lines.get(last_line).map(|l| l.len()).unwrap_or(0);
        let end = Position::new(last_line, last_col);
        self.selections.primary_mut().select_range(start, end);
        self.cursor_row = last_line;
        self.cursor_col = last_col;
    }

    /// Select next occurrence of current selection (Ctrl+D - VS Code style)
    /// Returns true if a new occurrence was found and selected
    pub fn select_next_occurrence(&mut self) -> bool {
        // Get the current selected text
        let selected = match self.get_selected_text() {
            Some(text) if !text.is_empty() => text,
            _ => {
                // No selection - select word under cursor
                return self.select_word_under_cursor();
            }
        };

        // Get current primary selection end position
        let primary = self.selections.primary();
        let search_start_line = primary.end().line;
        let search_start_col = primary.end().col;

        // Search for next occurrence after the current selection
        for line_idx in search_start_line..self.lines.len() {
            if let Some(line) = self.lines.get(line_idx) {
                let start_col = if line_idx == search_start_line {
                    search_start_col
                } else {
                    0
                };

                // Search in this line from start_col
                if start_col < line.len() {
                    if let Some(found_col) = line[start_col..].find(&selected) {
                        let abs_col = start_col + found_col;
                        // Add new cursor at this occurrence
                        let new_anchor = Position::new(line_idx, abs_col);
                        let new_head = Position::new(line_idx, abs_col + selected.len());

                        // Add a new selection spanning the occurrence
                        let mut new_selection = Selection::new(new_anchor);
                        new_selection.extend_to(new_head);
                        self.selections.add_selection_spanning(new_anchor, new_head);

                        // Move cursor to the new selection
                        self.cursor_row = line_idx;
                        self.cursor_col = abs_col + selected.len();

                        return true;
                    }
                }
            }
        }

        // Wrap around and search from beginning
        for line_idx in 0..=search_start_line {
            if let Some(line) = self.lines.get(line_idx) {
                let end_col = if line_idx == search_start_line {
                    search_start_col.saturating_sub(selected.len())
                } else {
                    line.len()
                };

                if let Some(found_col) = line[..end_col].find(&selected) {
                    let new_anchor = Position::new(line_idx, found_col);
                    let new_head = Position::new(line_idx, found_col + selected.len());
                    self.selections.add_selection_spanning(new_anchor, new_head);
                    self.cursor_row = line_idx;
                    self.cursor_col = found_col + selected.len();
                    return true;
                }
            }
        }

        false
    }

    /// Select the word under cursor (helper for select_next_occurrence)
    fn select_word_under_cursor(&mut self) -> bool {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if line.is_empty() || self.cursor_col >= line.len() {
                return false;
            }

            let chars: Vec<char> = line.chars().collect();
            let col = self.cursor_col.min(chars.len().saturating_sub(1));

            // Check if current char is a word char
            if !chars
                .get(col)
                .map(|c| c.is_alphanumeric() || *c == '_')
                .unwrap_or(false)
            {
                return false;
            }

            // Find word start
            let mut word_start = col;
            while word_start > 0 {
                if !chars
                    .get(word_start - 1)
                    .map(|c| c.is_alphanumeric() || *c == '_')
                    .unwrap_or(false)
                {
                    break;
                }
                word_start -= 1;
            }

            // Find word end
            let mut word_end = col;
            while word_end < chars.len() {
                if !chars
                    .get(word_end)
                    .map(|c| c.is_alphanumeric() || *c == '_')
                    .unwrap_or(false)
                {
                    break;
                }
                word_end += 1;
            }

            // Select the word
            let start = Position::new(self.cursor_row, word_start);
            let end = Position::new(self.cursor_row, word_end);
            self.selections.primary_mut().select_range(start, end);
            self.cursor_col = word_end;

            true
        } else {
            false
        }
    }

    /// Get cursor position as linear index (for EditHistory)
    pub fn cursor_position(&self) -> usize {
        let mut pos = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if i == self.cursor_row {
                return pos + self.cursor_col.min(line.len());
            }
            pos += line.len() + 1; // +1 for newline
        }
        pos
    }

    /// Set content from string (clears buffer and reloads)
    pub fn set_content(&mut self, content: &str) {
        *self = Self::from_content(content);
    }

    /// Set cursor to linear position
    pub fn set_cursor_position(&mut self, pos: usize) {
        let mut remaining = pos;
        for (row, line) in self.lines.iter().enumerate() {
            let line_len = line.len() + 1; // +1 for newline
            if remaining < line_len || row == self.lines.len() - 1 {
                self.cursor_row = row;
                self.cursor_col = remaining.min(line.len());
                self.adjust_scroll();
                return;
            }
            remaining -= line_len;
        }
        // Fallback: end of document
        self.cursor_row = self.lines.len().saturating_sub(1);
        self.cursor_col = self.lines.last().map(|l| l.len()).unwrap_or(0);
        self.adjust_scroll();
    }
}

/// Studio view state
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
    validation_pending: bool,
    /// Time of last edit (for debouncing)
    last_edit_time: Option<Instant>,
    /// Cached parsed workflow (RefCell for interior mutability in render)
    cached_workflow: RefCell<Option<crate::ast::Workflow>>,
    /// Content hash when workflow was cached (invalidation)
    cached_content_hash: Cell<u64>,
    /// Animation frame counter (0-255, wraps)
    pub frame: u8,
    /// Matrix rain background opacity (0.0 = invisible, 1.0 = full)
    pub rain_opacity: f32,
    /// Whether rain effect is actively fading out
    pub rain_fading: bool,
    /// Whether matrix effect is enabled
    pub matrix_effect_enabled: bool,
    /// Edit history for undo/redo support (Ctrl+Z/Ctrl+Y)
    edit_history: EditHistory,
    /// Diagnostics engine for real-time error display (gutter + underline)
    diagnostics: DiagnosticsEngine,
    /// System clipboard for copy/paste
    clipboard: Option<arboard::Clipboard>,
    /// Git status for line-level change indicators
    git_status: Option<GitStatus>,
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
            // Matrix Rain starts visible and fades
            frame: 0,
            rain_opacity: 1.0,
            rain_fading: true,
            matrix_effect_enabled: true,
            // Edit History (Undo/Redo)
            edit_history: EditHistory::new(100),
            // Real-time Diagnostics
            diagnostics: DiagnosticsEngine::new(),
            // Clipboard (graceful fallback if unavailable)
            clipboard: arboard::Clipboard::new().ok(),
            // Git gutter (initialized on file load)
            git_status: None,
        }
    }

    /// Tick animation frame (called from main loop)
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // Tick matrix rain fade effect
        if self.rain_fading && self.rain_opacity > 0.0 {
            self.rain_opacity = (self.rain_opacity - 0.04).max(0.0); // Smooth fade ~2s
        }
    }

    /// Mark content as edited - validation will run after debounce delay
    fn mark_edited(&mut self) {
        self.modified = true;
        self.validation_pending = true;
        self.last_edit_time = Some(Instant::now());
        // Push state to edit history for undo/redo
        let content = self.buffer.content();
        let cursor = self.buffer.cursor_position();
        self.edit_history.push(&content, cursor);
    }

    /// Simple hash for cache invalidation
    fn content_hash(content: &str) -> u64 {
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
        match crate::serde_yaml::from_str::<serde_json::Value>(&content) {
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
    #[allow(dead_code)] // Will be used for status line display
    pub fn current_line(&self) -> usize {
        self.buffer.cursor().0 + 1
    }

    /// Get current column (1-indexed)
    #[allow(dead_code)] // Will be used for status line display
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
        // Matrix Rain background effect (full screen, renders FIRST)
        if self.matrix_effect_enabled && self.rain_opacity > 0.05 {
            let rain_area = Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(2),
            };
            MatrixRain::new()
                .frame(self.frame)
                .density(0.08) // Very subtle, smooth
                .opacity(self.rain_opacity)
                .with_mascots(true)
                .render(rain_area, frame.buffer_mut());
        }

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
        match key.code {
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

    /// Render just the editor content (for use in StudioView 3-panel layout)
    pub(crate) fn render_editor(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mode_indicator = match self.mode {
            EditorMode::Normal => "",
            EditorMode::Insert => " [INSERT]",
        };

        // Breadcrumb with file path (VS Code-like)
        let title = if let Some(path) = &self.path {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let icon = if filename.ends_with(".nika.yaml") {
                "🦋"
            } else {
                "📄"
            };
            let modified = if self.buffer.is_modified() {
                " •"
            } else {
                ""
            };
            format!(" {} {}{}{} ", icon, filename, modified, mode_indicator)
        } else {
            format!(" EDITOR{} ", mode_indicator)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Split inner into content + status bar
        let content_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: inner.height.saturating_sub(1), // Leave 1 row for status bar
        };
        let status_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };

        // Show placeholder when no file is loaded
        if self.path.is_none() {
            let placeholder = vec![
                Line::from(""),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  No file selected",
                    Style::default().fg(theme.text_muted),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "  Press [Tab] to focus Browser",
                    Style::default().fg(theme.text_muted),
                )]),
                Line::from(vec![Span::styled(
                    "  Select a .nika.yaml file with [Enter]",
                    Style::default().fg(theme.text_muted),
                )]),
            ];
            let paragraph = Paragraph::new(placeholder);
            frame.render_widget(paragraph, content_area);
            return;
        }

        // Calculate visible height (excluding borders and status bar) and sync to buffer
        let visible_height = content_area.height as usize;
        self.buffer.set_visible_height(visible_height);

        // Build lines with line numbers, syntax highlighting, and selection
        let selection = self.buffer.selection();

        // Get git line changes for gutter display
        // We clone the changes to avoid borrow issues in the closure
        let git_line_changes: Option<HashMap<usize, LineChange>> =
            self.path.as_ref().and_then(|p| {
                self.git_status
                    .as_mut()
                    .map(|gs| gs.line_changes(p).changes_map())
            });

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

                let base_style = if is_cursor_line {
                    Style::default().bg(theme.highlight)
                } else {
                    Style::default()
                };

                // Git gutter indicator
                let git_gutter = git_line_changes
                    .as_ref()
                    .and_then(|changes| changes.get(&i))
                    .map(|change| {
                        let (symbol, color) = match change {
                            LineChange::Added => ("+", theme.git_added),
                            LineChange::Modified => ("~", theme.git_modified),
                            LineChange::Deleted => ("-", theme.git_deleted),
                        };
                        Span::styled(symbol, Style::default().fg(color))
                    })
                    .unwrap_or_else(|| Span::raw(" "));

                // Line number with git gutter prefix
                let mut spans = vec![
                    git_gutter,
                    Span::styled(
                        format!("{:4} ", line_num),
                        Style::default().fg(theme.text_muted),
                    ),
                ];

                // Check for selection on this line
                let selection_range = selection.line_range(i);

                if let Some((sel_start, sel_end)) = selection_range {
                    // Line has selection - split into before/selected/after
                    let sel_end_col = sel_end.unwrap_or(line.len());
                    let sel_start = sel_start.min(line.len());
                    let sel_end_col = sel_end_col.min(line.len());

                    // Before selection
                    if sel_start > 0 {
                        let before = &line[..sel_start];
                        spans.extend(YamlHighlight::highlight_line(before, base_style));
                    }

                    // Selected portion
                    if sel_start < sel_end_col {
                        let selected = &line[sel_start..sel_end_col];
                        let selection_style = Style::default().bg(theme.selection).fg(theme.text);
                        spans.push(Span::styled(selected.to_string(), selection_style));
                    }

                    // After selection (only if selection ends on this line)
                    if sel_end.is_some() && sel_end_col < line.len() {
                        let after = &line[sel_end_col..];
                        spans.extend(YamlHighlight::highlight_line(after, base_style));
                    } else if sel_end.is_none() {
                        // Selection continues to next line - no after portion
                    }
                } else {
                    // No selection on this line - normal highlighting
                    spans.extend(YamlHighlight::highlight_line(line, base_style));
                }

                Line::from(spans)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, content_area);

        // Render ScrollIndicator with dynamic arrows
        let total_lines = self.buffer.lines().len();
        if total_lines > visible_height {
            let scrollbar_area = Rect {
                x: content_area.x + content_area.width.saturating_sub(1),
                y: content_area.y,
                width: 1,
                height: content_area.height,
            };

            let scroll_indicator = ScrollIndicator::new()
                .position(self.buffer.scroll_offset(), total_lines, visible_height)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);

            frame.render_widget(scroll_indicator, scrollbar_area);
        }

        // Render status bar (Ln:Col | Mode | Schema)
        // Show cursor count when multi-cursor active
        let (cursor_row, cursor_col) = self.buffer.cursor();
        let mode_str = match self.mode {
            EditorMode::Normal => "NORMAL",
            EditorMode::Insert => "INSERT",
        };
        let cursor_count = self.buffer.cursor_count();
        let status_left = if cursor_count > 1 {
            format!(
                " Ln {}, Col {} ({} cursors) ",
                cursor_row + 1,
                cursor_col + 1,
                cursor_count
            )
        } else {
            format!(" Ln {}, Col {} ", cursor_row + 1, cursor_col + 1)
        };
        let status_right = format!(" {} | nika/workflow ", mode_str);

        // Calculate padding for right-alignment
        let padding = status_area.width as usize - status_left.len() - status_right.len();
        let padding_str = " ".repeat(padding.max(0));

        let status_line = Line::from(vec![
            Span::styled(status_left, Style::default().fg(theme.text_muted)),
            Span::raw(padding_str),
            Span::styled(status_right, Style::default().fg(theme.text_muted)),
        ]);

        let status_paragraph = Paragraph::new(status_line)
            .style(Style::default().bg(theme.highlight).fg(theme.text_primary));
        frame.render_widget(status_paragraph, status_area);
    }

    /// Render just the DAG structure (for use in StudioView 3-panel layout)
    pub(crate) fn render_structure(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Title shows mode state
        let title = if self.dag_expanded {
            " STRUCTURE [E]xpanded "
        } else {
            " STRUCTURE [E]->expand "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Parse YAML and render with DagAscii
        let content = self.buffer.content();
        self.render_dag_structure(frame, inner, &content, theme);
    }

    /// Render DAG structure using DagAscii widget (for use in StudioView 3-panel layout)
    ///
    /// PERF: Uses cached_workflow to avoid re-parsing YAML every frame (60 FPS).
    /// Only parses when content hash changes.
    pub(crate) fn render_dag_structure(
        &self,
        frame: &mut Frame,
        area: Rect,
        yaml: &str,
        theme: &Theme,
    ) {
        // PERF: Compute content hash to check if we need to re-parse
        let current_hash = Self::content_hash(yaml);
        let cached_hash = self.cached_content_hash.get();

        // Update cache if content changed
        if current_hash != cached_hash {
            *self.cached_workflow.borrow_mut() = parse_workflow(yaml).ok();
            self.cached_content_hash.set(current_hash);
        }

        // Use cached workflow (avoids parsing every frame)
        let cached = self.cached_workflow.borrow();
        match cached.as_ref() {
            Some(wf) => {
                if wf.tasks.is_empty() {
                    let paragraph =
                        Paragraph::new("(no tasks)").style(Style::default().fg(theme.text_muted));
                    frame.render_widget(paragraph, area);
                    return;
                }

                // Build dependency map from Flow objects
                let deps = self.extract_flow_dependencies(wf);

                // Convert tasks to NodeBoxData
                let nodes: Vec<NodeBoxData> = wf
                    .tasks
                    .iter()
                    .map(|task| {
                        let verb = self.task_verb_color(task.as_ref());
                        NodeBoxData::new(&task.id, verb).with_status(TaskStatus::Pending)
                    })
                    .collect();

                let mode = if self.dag_expanded {
                    NodeBoxMode::Expanded
                } else {
                    NodeBoxMode::Minimal
                };

                // Create and render DagAscii widget
                let widget = DagAscii::new(&nodes)
                    .with_dependencies(deps)
                    .mode(mode)
                    .scroll(0, self.dag_scroll);

                // Render to buffer (DagAscii implements Widget)
                let buf = frame.buffer_mut();
                widget.render(area, buf);
            }
            None => {
                // YAML doesn't parse as workflow - show parse error state
                let paragraph =
                    Paragraph::new("⚠ Invalid workflow\n\nFix YAML errors to\nsee task structure")
                        .style(Style::default().fg(theme.status_failed));
                frame.render_widget(paragraph, area);
            }
        }
    }

    /// Extract dependencies from depends_on fields
    ///
    /// Returns a map: target_task_id -> [source_task_ids]
    fn extract_flow_dependencies(&self, wf: &Workflow) -> HashMap<String, Vec<String>> {
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();

        for task in &wf.tasks {
            if let Some(ref task_deps) = task.depends_on {
                let entry = deps.entry(task.id.clone()).or_default();
                for source in task_deps {
                    if !entry.contains(source) {
                        entry.push(source.clone());
                    }
                }
            }
        }

        deps
    }

    /// Get VerbColor from task action
    fn task_verb_color(&self, task: &crate::ast::Task) -> VerbColor {
        use crate::ast::TaskAction;
        match &task.action {
            TaskAction::Infer { .. } => VerbColor::Infer,
            TaskAction::Exec { .. } => VerbColor::Exec,
            TaskAction::Fetch { .. } => VerbColor::Fetch,
            TaskAction::Invoke { .. } => VerbColor::Invoke,
            TaskAction::Agent { .. } => VerbColor::Agent,
        }
    }

    /// Render the validation status bar
    pub(crate) fn render_validation(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
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

        // DAG Complexity Meter
        let complexity_meter = self.render_complexity_meter(theme);

        let line = Line::from(vec![
            Span::raw(" "),
            yaml_status,
            Span::raw("  |  "),
            schema_status,
            Span::raw("  |  "),
            warning_status,
            Span::raw("  |  "),
            complexity_meter,
        ]);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render DAG complexity meter
    /// Shows visual indicator of workflow complexity (tasks × flows)
    fn render_complexity_meter(&self, theme: &Theme) -> Span<'static> {
        let cached = self.cached_workflow.borrow();
        match cached.as_ref() {
            Some(wf) => {
                let task_count = wf.tasks.len();
                let flow_count = wf.flow_count();
                // Complexity score: tasks + edges (simple heuristic)
                let score = task_count + flow_count;

                // Visual meter: ▁▂▃▄▅▆▇█ (8 levels)
                let (meter, color) = match score {
                    0 => ("▁", theme.text_muted),
                    1..=2 => ("▂", theme.status_success),
                    3..=5 => ("▃", theme.status_success),
                    6..=8 => ("▄", theme.highlight),
                    9..=12 => ("▅", theme.highlight),
                    13..=16 => ("▆", theme.status_running),
                    17..=20 => ("▇", theme.status_running),
                    _ => ("█", theme.status_failed),
                };

                Span::styled(
                    format!("DAG {} {}t {}e", meter, task_count, flow_count),
                    Style::default().fg(color),
                )
            }
            None => Span::styled("DAG ▁ --", Style::default().fg(theme.text_muted)),
        }
    }
}

/// YAML syntax highlighting colors
struct YamlHighlight;

impl YamlHighlight {
    /// Key color (Solarized blue)
    const KEY: Color = Color::Rgb(38, 139, 210);
    /// String value color (Solarized green)
    const STRING: Color = Color::Rgb(133, 153, 0);
    /// Number value color (Solarized orange)
    const NUMBER: Color = Color::Rgb(203, 75, 22);
    /// Boolean value color (Solarized violet)
    const BOOL: Color = Color::Rgb(108, 113, 196);
    /// Comment color (Solarized base01 - muted)
    const COMMENT: Color = Color::Rgb(88, 110, 117);
    /// Default text color (reserved for future use)
    #[allow(dead_code)]
    const DEFAULT: Color = Color::Reset;
    /// Nika verbs (Solarized MAGENTA) - infer, exec, fetch, invoke, agent 🦋
    const VERB: Color = Color::Rgb(211, 54, 130);
    /// Nika structure keywords (Solarized yellow)
    const NIKA_KEYWORD: Color = Color::Rgb(181, 137, 0);

    /// Highlight a single YAML line into styled spans
    fn highlight_line(line: &str, base_style: Style) -> Vec<Span<'static>> {
        let line_owned = line.to_string();
        let trimmed = line_owned.trim();

        // Empty line
        if trimmed.is_empty() {
            return vec![Span::styled(line_owned, base_style)];
        }

        // Full-line comment
        if trimmed.starts_with('#') {
            return vec![Span::styled(line_owned, base_style.fg(Self::COMMENT))];
        }

        // Try to parse as key: value
        if let Some(colon_pos) = line.find(':') {
            let (key_part, rest) = line.split_at(colon_pos);
            let key_trimmed = key_part.trim();

            // Check if it's a Nika verb (magenta) or Nika keyword (yellow)
            let key_color = if matches!(
                key_trimmed,
                "infer" | "exec" | "fetch" | "invoke" | "agent" | "decompose"
            ) {
                Self::VERB // 🦋 Nika verbs in magenta
            } else if matches!(
                key_trimmed,
                "schema"
                    | "workflow"
                    | "tasks"
                    | "flows"
                    | "mcp"
                    | "servers"
                    | "with"
                    | "params"
                    | "context"
                    | "include"
                    | "skills"
                    | "for_each"
                    | "id"
                    | "model"
                    | "provider"
            ) {
                Self::NIKA_KEYWORD // Structure keywords in yellow
            } else {
                Self::KEY
            };

            let mut spans = vec![Span::styled(
                format!("{}:", key_part),
                base_style.fg(key_color),
            )];

            // Handle the value part (skip the colon we already included)
            let value = &rest[1..]; // Skip ':'
            if !value.is_empty() {
                spans.push(Self::highlight_value(value, base_style));
            }

            return spans;
        }

        // List item (- value)
        if trimmed.starts_with('-') {
            // SAFETY: starts_with('-') guarantees find('-') will succeed
            if let Some(dash_pos) = line.find('-') {
                let before_dash = &line[..dash_pos];
                let after_dash = &line[dash_pos + 1..];

                let mut spans = vec![
                    Span::styled(before_dash.to_string(), base_style),
                    Span::styled("-".to_string(), base_style.fg(Self::KEY)),
                ];

                if !after_dash.is_empty() {
                    spans.push(Self::highlight_value(after_dash, base_style));
                }

                return spans;
            }
        }

        // Default: return as-is
        vec![Span::styled(line_owned, base_style)]
    }

    /// Highlight a YAML value
    fn highlight_value(value: &str, base_style: Style) -> Span<'static> {
        let trimmed = value.trim();

        // String in quotes
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return Span::styled(value.to_string(), base_style.fg(Self::STRING));
        }

        // Boolean
        if matches!(trimmed, "true" | "false" | "yes" | "no" | "True" | "False") {
            return Span::styled(value.to_string(), base_style.fg(Self::BOOL));
        }

        // Number (integer or float)
        if trimmed.parse::<f64>().is_ok() || trimmed.parse::<i64>().is_ok() {
            return Span::styled(value.to_string(), base_style.fg(Self::NUMBER));
        }

        // Inline comment - for now, return as-is (would need multiple spans)
        if value.contains(" #") {
            return Span::styled(value.to_string(), base_style);
        }

        // Default
        Span::styled(value.to_string(), base_style)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // Schema validation tests (TDD)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_yaml_editor_panel_schema_validation_valid_workflow() {
        let mut view = YamlEditorPanel::new();

        // Valid Nika workflow YAML
        let valid_yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Hello world""#;

        view.buffer = TextBuffer::from_content(valid_yaml);
        view.validate();

        assert!(view.validation.yaml_valid, "YAML should be valid");
        assert!(view.validation.schema_valid, "Schema should be valid");
        assert!(view.validation.errors.is_empty(), "No errors expected");
    }

    #[test]
    fn test_yaml_editor_panel_schema_validation_invalid_schema() {
        let mut view = YamlEditorPanel::new();

        // Invalid Nika workflow - missing required 'tasks' field
        let invalid_yaml = r#"schema: "nika/workflow@0.12"
unknown_field: "should fail""#;

        view.buffer = TextBuffer::from_content(invalid_yaml);
        view.validate();

        assert!(view.validation.yaml_valid, "YAML syntax is valid");
        assert!(!view.validation.schema_valid, "Schema should be invalid");
        assert!(!view.validation.errors.is_empty(), "Should have errors");
    }

    #[test]
    fn test_yaml_editor_panel_schema_validation_missing_schema_field() {
        let mut view = YamlEditorPanel::new();

        // Missing required 'schema' field
        let yaml = r#"tasks:
  - id: step1
    infer: "Hello""#;

        view.buffer = TextBuffer::from_content(yaml);
        view.validate();

        assert!(view.validation.yaml_valid, "YAML syntax is valid");
        assert!(!view.validation.schema_valid, "Schema should be invalid");
        assert!(
            view.validation.errors.iter().any(|e| e.contains("schema")),
            "Error should mention 'schema'"
        );
    }

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
    // YamlEditorPanel tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_yaml_editor_panel_new() {
        let view = YamlEditorPanel::new();
        assert_eq!(view.mode, EditorMode::Normal);
        assert!(!view.modified);
        assert!(view.path.is_none());
    }

    #[test]
    fn test_yaml_editor_panel_mode_switch() {
        let mut view = YamlEditorPanel::new();
        assert_eq!(view.mode, EditorMode::Normal);

        view.mode = EditorMode::Insert;
        assert_eq!(view.mode, EditorMode::Insert);
    }

    #[test]
    fn test_yaml_editor_panel_validation_valid_yaml_syntax() {
        let mut view = YamlEditorPanel::new();

        // Valid YAML syntax (but not a valid Nika workflow schema)
        view.buffer = TextBuffer::from_content("key: value");
        view.validate();
        assert!(view.validation.yaml_valid, "YAML syntax should be valid");
        // Note: schema validation will fail because this isn't a valid workflow
        // but yaml_valid should be true because the syntax is correct
        assert!(
            !view.validation.schema_valid,
            "Schema should be invalid for non-workflow YAML"
        );
    }

    #[test]
    fn test_yaml_editor_panel_validation_invalid_yaml() {
        let mut view = YamlEditorPanel::new();

        // Invalid YAML
        view.buffer = TextBuffer::from_content("key: [unclosed");
        view.validate();
        assert!(!view.validation.yaml_valid);
        assert!(!view.validation.errors.is_empty());
    }

    #[test]
    fn test_yaml_editor_panel_cursor_position() {
        let view = YamlEditorPanel::new();
        assert_eq!(view.current_line(), 1);
        assert_eq!(view.current_col(), 1);
    }

    #[test]
    fn test_yaml_editor_panel_default_validation_result() {
        let result = ValidationResult::default();
        assert!(result.yaml_valid);
        assert!(result.schema_valid);
        assert!(result.warnings.is_empty());
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_yaml_editor_panel_status_line_normal_mode() {
        let view = YamlEditorPanel::new();
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("NORMAL"));
        assert!(status.contains("Ln 1"));
        assert!(status.contains("Col 1"));
    }

    #[test]
    fn test_yaml_editor_panel_status_line_insert_mode() {
        let mut view = YamlEditorPanel::new();
        view.mode = EditorMode::Insert;
        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("INSERT"));
    }

    #[test]
    fn test_yaml_editor_panel_status_line_modified() {
        let mut view = YamlEditorPanel::new();
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
    fn test_yaml_editor_panel_handle_normal_mode_quit() {
        let mut view = YamlEditorPanel::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(KeyCode::Char('q'));
        let action = view.handle_key(key, &mut state);
        match action {
            ViewAction::SwitchView(TuiView::Studio) => {}
            _ => panic!("Expected SwitchView(Studio)"),
        }
    }

    #[test]
    fn test_yaml_editor_panel_handle_normal_mode_insert() {
        let mut view = YamlEditorPanel::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(KeyCode::Char('i'));
        let _ = view.handle_key(key, &mut state);
        assert_eq!(view.mode, EditorMode::Insert);
    }

    #[test]
    fn test_yaml_editor_panel_handle_insert_mode_escape() {
        let mut view = YamlEditorPanel::new();
        view.mode = EditorMode::Insert;
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::from(KeyCode::Esc);
        let _ = view.handle_key(key, &mut state);
        assert_eq!(view.mode, EditorMode::Normal);
    }

    #[test]
    fn test_yaml_editor_panel_handle_insert_mode_typing() {
        let mut view = YamlEditorPanel::new();
        view.mode = EditorMode::Insert;
        let mut state = TuiState::new("test.nika.yaml");

        // Type some characters
        view.handle_key(KeyEvent::from(KeyCode::Char('a')), &mut state);
        view.handle_key(KeyEvent::from(KeyCode::Char('b')), &mut state);
        view.handle_key(KeyEvent::from(KeyCode::Char('c')), &mut state);

        assert_eq!(view.buffer.lines()[0], "abc");
        assert!(view.modified);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // YAML syntax highlighting tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_yaml_highlight_comment() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("# This is a comment", base);
        assert_eq!(spans.len(), 1);
        // Should have comment color (gray)
        assert_eq!(spans[0].style.fg, Some(YamlHighlight::COMMENT));
    }

    #[test]
    fn test_yaml_highlight_key_value() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("name: my-workflow", base);
        assert!(spans.len() >= 2, "Should have key and value spans");
        // First span should be the key with colon
        assert!(spans[0].content.contains("name:"));
    }

    #[test]
    fn test_yaml_highlight_verb() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("    infer: \"prompt\"", base);
        // Should highlight 'infer' as a Nika verb (cyan)
        assert!(spans[0].content.contains("infer:"));
        assert_eq!(spans[0].style.fg, Some(YamlHighlight::VERB));
    }

    #[test]
    fn test_yaml_highlight_boolean() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("enabled: true", base);
        assert!(spans.len() >= 2);
        // Value should have boolean color (purple)
        let value_span = &spans[1];
        assert_eq!(value_span.style.fg, Some(YamlHighlight::BOOL));
    }

    #[test]
    fn test_yaml_highlight_number() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("port: 8080", base);
        assert!(spans.len() >= 2);
        // Value should have number color (orange)
        let value_span = &spans[1];
        assert_eq!(value_span.style.fg, Some(YamlHighlight::NUMBER));
    }

    #[test]
    fn test_yaml_highlight_string() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("prompt: \"Hello world\"", base);
        assert!(spans.len() >= 2);
        // Value should have string color (green)
        let value_span = &spans[1];
        assert_eq!(value_span.style.fg, Some(YamlHighlight::STRING));
    }

    #[test]
    fn test_yaml_highlight_list_item() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("  - item", base);
        assert!(
            spans.len() >= 2,
            "Should have indent, dash, and value spans"
        );
        // Should contain dash
        assert!(spans.iter().any(|s| s.content.contains("-")));
    }

    #[test]
    fn test_yaml_highlight_empty_line() {
        let base = Style::default();
        let spans = YamlHighlight::highlight_line("", base);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].content.is_empty());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // StudioView lifecycle tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_studio_view_on_enter_initializes_tree_state() {
        use crate::tui::state::TuiState;
        use crate::tui::views::View;

        // Create a StudioView with the current directory (which should have files)
        let mut studio = StudioView::new();
        let mut state = TuiState::new("test.yaml");

        // Before on_enter: tree_state should be empty
        assert!(
            studio.tree_state.visible_nodes().is_empty(),
            "visible_nodes should be empty before on_enter"
        );
        assert!(
            studio.tree_state.selected().is_none(),
            "selection should be None before on_enter"
        );

        // Call on_enter (the lifecycle hook)
        studio.on_enter(&mut state);

        // After on_enter: tree_state should be initialized
        assert!(
            !studio.tree_state.visible_nodes().is_empty(),
            "visible_nodes should NOT be empty after on_enter"
        );
        assert!(
            studio.tree_state.selected().is_some(),
            "selection should be Some after on_enter"
        );
        assert!(
            studio.cached_tree.is_some(),
            "cached_tree should be Some after on_enter"
        );
    }

    #[test]
    fn test_studio_view_on_enter_expands_root() {
        use crate::tui::state::TuiState;
        use crate::tui::views::View;

        let mut studio = StudioView::new();
        let mut state = TuiState::new("test.yaml");

        studio.on_enter(&mut state);

        // Root directory should be expanded
        if let Some(ref tree) = studio.cached_tree {
            assert!(
                studio.tree_state.is_expanded(tree.id),
                "Root directory should be expanded after on_enter"
            );
        } else {
            panic!("cached_tree should be Some after on_enter");
        }
    }

    #[test]
    fn test_studio_view_tab_cycles_panels_in_normal_mode() {
        use crate::tui::state::TuiState;
        use crate::tui::views::View;

        let mut studio = StudioView::new();
        let mut state = TuiState::new("test.yaml");

        // Start in Browser focus, editor in Normal mode
        studio.focus = StudioFocus::Browser;
        studio.editor.mode = EditorMode::Normal;

        // Press Tab - should cycle to Editor
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        studio.handle_key(key, &mut state);

        assert_eq!(
            studio.focus,
            StudioFocus::Editor,
            "Tab should cycle from Browser to Editor"
        );

        // Press Tab again - should cycle to Dag (not insert indent)
        studio.handle_key(key, &mut state);
        assert_eq!(
            studio.focus,
            StudioFocus::Dag,
            "Tab should cycle from Editor to Dag when in Normal mode"
        );
    }

    #[test]
    fn test_studio_view_tab_indents_in_insert_mode() {
        use crate::tui::state::TuiState;
        use crate::tui::views::View;

        let mut studio = StudioView::new();
        let mut state = TuiState::new("test.yaml");

        // Focus editor in Insert mode
        studio.focus = StudioFocus::Editor;
        studio.editor.mode = EditorMode::Insert;

        // Press Tab - should NOT cycle panels (handled by editor)
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        studio.handle_key(key, &mut state);

        // Focus should still be Editor (Tab was sent to editor, not used for cycling)
        assert_eq!(
            studio.focus,
            StudioFocus::Editor,
            "Tab should NOT cycle panels when editor is in Insert mode"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Selection tests
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_text_buffer_selection_single_line() {
        let mut buffer = TextBuffer::from_content("hello world");

        // Select "world"
        buffer.start_selection();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.cursor_right(); // Now at position 6
        buffer.sync_selection_to_cursor();

        assert!(buffer.has_selection());
        assert_eq!(buffer.get_selected_text(), Some("hello ".to_string()));
    }

    #[test]
    fn test_text_buffer_selection_clear() {
        let mut buffer = TextBuffer::from_content("hello");
        buffer.start_selection();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.sync_selection_to_cursor();

        assert!(buffer.has_selection());
        buffer.clear_selection();
        assert!(!buffer.has_selection());
    }

    #[test]
    fn test_text_buffer_delete_selection() {
        let mut buffer = TextBuffer::from_content("hello world");

        // Select "hello "
        buffer.start_selection();
        for _ in 0..6 {
            buffer.cursor_right();
        }
        buffer.sync_selection_to_cursor();

        assert!(buffer.delete_selection());
        assert_eq!(buffer.content(), "world");
        assert!(!buffer.has_selection());
    }

    #[test]
    fn test_text_buffer_select_all() {
        let mut buffer = TextBuffer::from_content("line1\nline2\nline3");
        buffer.select_all();

        assert!(buffer.has_selection());
        assert_eq!(
            buffer.get_selected_text(),
            Some("line1\nline2\nline3".to_string())
        );
    }

    #[test]
    fn test_text_buffer_delete_multiline_selection() {
        let mut buffer = TextBuffer::from_content("aaa\nbbb\nccc");

        // Move to line 0, col 1 and start selection
        buffer.cursor_right();
        buffer.start_selection();

        // Move to line 2, col 1
        buffer.cursor_down();
        buffer.cursor_down();
        buffer.sync_selection_to_cursor();

        assert!(buffer.has_selection());
        buffer.delete_selection();

        // Should have "a" + "cc" = "acc"
        assert_eq!(buffer.content(), "acc");
    }

    #[test]
    fn test_multi_cursor_add_cursor() {
        let mut buffer = TextBuffer::from_content("line1\nline2\nline3");

        assert_eq!(buffer.cursor_count(), 1);
        assert!(!buffer.is_multi_cursor());

        buffer.add_cursor(1, 0);
        assert_eq!(buffer.cursor_count(), 2);
        assert!(buffer.is_multi_cursor());

        buffer.add_cursor(2, 0);
        assert_eq!(buffer.cursor_count(), 3);
    }

    #[test]
    fn test_multi_cursor_clear_additional() {
        let mut buffer = TextBuffer::from_content("hello\nworld");

        buffer.add_cursor(1, 0);
        buffer.add_cursor(1, 2);
        assert_eq!(buffer.cursor_count(), 3);

        buffer.clear_additional_cursors();
        assert_eq!(buffer.cursor_count(), 1);
        assert!(!buffer.is_multi_cursor());
    }

    #[test]
    fn test_select_word_under_cursor() {
        let mut buffer = TextBuffer::from_content("hello world test");

        // Position cursor in "world"
        for _ in 0..7 {
            buffer.cursor_right();
        }

        // First Ctrl+D should select "world"
        let selected = buffer.select_next_occurrence();
        assert!(selected);
        assert!(buffer.has_selection());
        assert_eq!(buffer.get_selected_text(), Some("world".to_string()));
    }

    #[test]
    fn test_select_next_occurrence() {
        let mut buffer = TextBuffer::from_content("foo bar foo baz foo");

        // Select first "foo" manually
        buffer.start_selection();
        for _ in 0..3 {
            buffer.cursor_right();
        }
        buffer.sync_selection_to_cursor();

        assert_eq!(buffer.get_selected_text(), Some("foo".to_string()));
        assert_eq!(buffer.cursor_count(), 1);

        // Ctrl+D should find next "foo" and add cursor
        let found = buffer.select_next_occurrence();
        assert!(found);
        assert_eq!(buffer.cursor_count(), 2);
    }

    #[test]
    fn test_select_next_occurrence_wraps() {
        let mut buffer = TextBuffer::from_content("ab cd ab");

        // Select last "ab" (position 6-8)
        for _ in 0..6 {
            buffer.cursor_right();
        }
        buffer.start_selection();
        buffer.cursor_right();
        buffer.cursor_right();
        buffer.sync_selection_to_cursor();

        assert_eq!(buffer.get_selected_text(), Some("ab".to_string()));

        // Should wrap around and find "ab" at position 0
        let found = buffer.select_next_occurrence();
        assert!(found);
        assert_eq!(buffer.cursor_count(), 2);
    }

    #[test]
    fn test_select_word_boundary() {
        let mut buffer = TextBuffer::from_content("hello_world test_case");

        // cursor at 'h'
        let selected = buffer.select_next_occurrence();
        assert!(selected);
        // Should select "hello_world" (underscore is part of word)
        assert_eq!(buffer.get_selected_text(), Some("hello_world".to_string()));
    }
}
