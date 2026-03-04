# Unified Workspace View Implementation Plan

**Version:** v0.21.0
**Date:** 2026-03-04
**Status:** Implementation Plan
**Author:** Claude + Thibaut

---

## Executive Summary

Merge HomeView (Browser) and StudioView (Editor) into a single **WorkspaceView** that provides a VS Code-like development experience for Nika workflows.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  UNIFIED WORKSPACE VIEW (v0.21.0)                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌─────────────────┬────────────────────────────────┬──────────────────────┐   │
│  │  FILE TREE      │  YAML EDITOR                   │  DAG PREVIEW         │   │
│  │  (20%)          │  (50%)                         │  (30%)               │   │
│  │                 │                                │                      │   │
│  │  🦋 project/    │  1 │ schema: nika/workflow@0.9 │  ┌─────────┐        │   │
│  │  ├─ 📂 .nika/   │  2 │ workflow: deploy          │  │ infer   │        │   │
│  │  ├─ 📂 workflows│  3 │                           │  └────┬────┘        │   │
│  │  │  ├─ ✨ a.yaml│  4 │ tasks:                    │       │             │   │
│  │  │  └─ ✨ b.yaml│  5 │   - id: step1             │  ┌────▼────┐        │   │
│  │  └─ 📄 README   │  6 │     infer: "Generate"     │  │ exec    │        │   │
│  │                 │  7 │   - id: step2             │  └─────────┘        │   │
│  │                 │  8 │     exec: "npm build"     │                      │   │
│  │  [/] Search     │                                │  [E] Expand          │   │
│  └─────────────────┴────────────────────────────────┴──────────────────────┘   │
│  ┌─────────────────────────────────────────────────────────────────────────┐   │
│  │ ✓ Valid YAML │ ✓ Schema OK │ Ln 5, Col 12 │ INSERT │ deploy.nika.yaml   │   │
│  └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Phase A: Complete Tree Widget (Phases 1-5)

### A.1 Audit Current Implementation ✅

| Module | Lines | Status | Notes |
|--------|-------|--------|-------|
| `node.rs` | 602 | ✅ Complete | TreeNode, 36+ NodeKind variants |
| `state.rs` | 763 | ✅ Complete | TreeState, TreeAction, dual navigation |
| `colors.rs` | 314 | ✅ Complete | Solarized palette, node_color() |

### A.2 Phase 3: Animation System

**File:** `src/tui/widgets/tree/animation.rs` (NEW)

```rust
//! Glow animation system for ecosystem files
//!
//! Provides smooth pulse effects for .nika.yaml, .son, and other
//! SuperNovae ecosystem files.

use std::time::{Duration, Instant};

/// Animation state for a single node
#[derive(Debug, Clone)]
pub struct GlowAnimation {
    /// Start time of animation
    start: Instant,
    /// Animation duration (default: 500ms for pulse)
    duration: Duration,
    /// Animation type
    kind: AnimationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationKind {
    /// Gentle sine wave pulse (hover state)
    Pulse,
    /// Expand/collapse transition
    Expand,
    /// Selection sweep effect
    SelectionSweep,
}

impl GlowAnimation {
    /// Create a new pulse animation
    pub fn pulse() -> Self {
        Self {
            start: Instant::now(),
            duration: Duration::from_millis(500),
            kind: AnimationKind::Pulse,
        }
    }

    /// Get current intensity (0.0 to 1.0)
    pub fn intensity(&self) -> f32 {
        let elapsed = self.start.elapsed().as_secs_f32();
        let cycle = (elapsed / self.duration.as_secs_f32()) % 1.0;

        match self.kind {
            AnimationKind::Pulse => {
                // Sine wave: 0.5 + 0.5 * sin(2π * cycle)
                0.5 + 0.5 * (std::f32::consts::TAU * cycle).sin()
            }
            AnimationKind::Expand => {
                // Ease-out cubic
                1.0 - (1.0 - cycle).powi(3)
            }
            AnimationKind::SelectionSweep => {
                // Linear
                cycle
            }
        }
    }
}

/// Animation ticker (60fps)
pub struct AnimationTicker {
    frame: u8,
    last_tick: Instant,
}

impl AnimationTicker {
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
        }
    }

    /// Tick and return frame number (wraps at 255)
    pub fn tick(&mut self) -> u8 {
        self.frame = self.frame.wrapping_add(1);
        self.last_tick = Instant::now();
        self.frame
    }

    /// Get interpolated glow factor for ecosystem files
    pub fn glow_factor(&self) -> f32 {
        let cycle = (self.frame as f32 / 60.0) % 1.0;
        0.7 + 0.3 * (std::f32::consts::TAU * cycle).sin()
    }
}
```

### A.3 Phase 4: Tree Renderer

**File:** `src/tui/widgets/tree/render.rs` (NEW)

```rust
//! Tree rendering with icons, colors, and animations

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};

use super::{TreeNode, TreeState, TreeColors, NodeKind};
use super::animation::AnimationTicker;

/// Tree widget renderer
pub struct TreeWidget<'a> {
    /// Root node to render
    root: &'a TreeNode,
    /// Current state (selection, expansion)
    state: &'a TreeState,
    /// Color palette
    colors: TreeColors,
    /// Animation ticker
    ticker: &'a AnimationTicker,
    /// Block wrapper
    block: Option<Block<'a>>,
    /// Show hidden files
    show_hidden: bool,
}

impl<'a> TreeWidget<'a> {
    pub fn new(root: &'a TreeNode, state: &'a TreeState, ticker: &'a AnimationTicker) -> Self {
        Self {
            root,
            state,
            colors: TreeColors::solarized_dark(),
            ticker,
            block: None,
            show_hidden: false,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn show_hidden(mut self, show: bool) -> Self {
        self.show_hidden = show;
        self
    }

    /// Render a single node line
    fn render_node(&self, node: &TreeNode, is_selected: bool) -> Line<'static> {
        let indent = "  ".repeat(node.depth);
        let icon = node.kind.icon();
        let name = node.name.clone();

        // Get base color from node kind
        let base_color = self.colors.node_color(&node.kind);

        // Apply glow for ecosystem files
        let style = if node.kind.is_ecosystem() && is_selected {
            let glow = self.ticker.glow_factor();
            Style::default()
                .fg(base_color)
                .add_modifier(Modifier::BOLD)
                // Brightness modulated by glow
        } else if is_selected {
            Style::default()
                .fg(base_color)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(base_color)
        };

        // Tree connector
        let connector = if node.is_directory() {
            if node.expanded { "▾ " } else { "▸ " }
        } else {
            "  "
        };

        Line::from(vec![
            Span::raw(indent),
            Span::styled(connector, Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} ", icon), style),
            Span::styled(name, style),
        ])
    }
}

impl Widget for TreeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render block if present
        let inner = if let Some(block) = self.block {
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        // Flatten visible nodes
        let visible_nodes = self.root.flatten_visible(self.show_hidden);

        // Render each visible node
        for (i, node) in visible_nodes.iter().enumerate().take(inner.height as usize) {
            let is_selected = self.state.is_selected(node.id);
            let line = self.render_node(node, is_selected);

            let y = inner.y + i as u16;
            if y < inner.y + inner.height {
                buf.set_line(inner.x, y, &line, inner.width);
            }
        }
    }
}
```

### A.4 Phase 5: Filtering System

**File:** `src/tui/widgets/tree/filter.rs` (NEW)

```rust
//! Tree filtering for hidden files, workflows, agents

use super::NodeKind;

/// Filter mode for tree display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TreeFilter {
    /// Show all files (except hidden unless toggled)
    #[default]
    All,
    /// Show only .nika.yaml workflows
    WorkflowsOnly,
    /// Show only .son agents
    AgentsOnly,
    /// Show only .skill.md files
    SkillsOnly,
}

impl TreeFilter {
    /// Check if a node kind passes this filter
    pub fn matches(&self, kind: &NodeKind) -> bool {
        match self {
            TreeFilter::All => true,
            TreeFilter::WorkflowsOnly => matches!(kind,
                NodeKind::NikaWorkflow | NodeKind::WorkflowsFolder | NodeKind::Directory
            ),
            TreeFilter::AgentsOnly => matches!(kind,
                NodeKind::SonAgent | NodeKind::AgentsFolder | NodeKind::Directory
            ),
            TreeFilter::SkillsOnly => matches!(kind,
                NodeKind::SkillFile | NodeKind::SkillsFolder | NodeKind::Directory
            ),
        }
    }

    /// Cycle to next filter mode
    pub fn next(&self) -> Self {
        match self {
            TreeFilter::All => TreeFilter::WorkflowsOnly,
            TreeFilter::WorkflowsOnly => TreeFilter::AgentsOnly,
            TreeFilter::AgentsOnly => TreeFilter::SkillsOnly,
            TreeFilter::SkillsOnly => TreeFilter::All,
        }
    }

    /// Display name for status line
    pub fn display_name(&self) -> &'static str {
        match self {
            TreeFilter::All => "All",
            TreeFilter::WorkflowsOnly => "Workflows",
            TreeFilter::AgentsOnly => "Agents",
            TreeFilter::SkillsOnly => "Skills",
        }
    }
}

/// Keyboard shortcuts for filters
pub fn filter_from_key(key: char) -> Option<TreeFilter> {
    match key {
        'W' => Some(TreeFilter::WorkflowsOnly),
        'A' => Some(TreeFilter::AgentsOnly),
        'S' => Some(TreeFilter::SkillsOnly),
        _ => None,
    }
}
```

---

## Phase B: Unified Workspace View

### B.1 New WorkspaceView Structure

**File:** `src/tui/views/workspace.rs` (NEW - ~1500 lines)

```rust
//! WorkspaceView - Unified Browser + Editor + DAG Preview
//!
//! Layout:
//! ```text
//! ┌─────────────────┬────────────────────────────┬──────────────────┐
//! │  FILE TREE      │  YAML EDITOR               │  DAG PREVIEW     │
//! │  (20%)          │  (50%)                     │  (30%)           │
//! │                 │                            │                  │
//! │  TreeWidget     │  TextBuffer + Highlighting │  DagAscii        │
//! │  + Search       │  + Line numbers            │  + Live sync     │
//! │  + Filtering    │  + Cursor                  │                  │
//! │                 │                            │                  │
//! └─────────────────┴────────────────────────────┴──────────────────┘
//! ┌────────────────────────────────────────────────────────────────┐
//! │ ✓ YAML │ ✓ Schema │ Ln 5, Col 12 │ INSERT │ workflow.nika.yaml │
//! └────────────────────────────────────────────────────────────────┘
//! ```

use std::path::PathBuf;
use std::cell::{Cell, RefCell};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::tui::theme::Theme;
use crate::tui::state::TuiState;
use crate::tui::widgets::tree::{
    TreeNode, TreeState, TreeColors, TreeAction,
    animation::AnimationTicker,
    filter::TreeFilter,
    render::TreeWidget,
};
use crate::tui::edit_history::EditHistory;
use crate::tui::standalone::StandaloneState;

/// Focus panel in workspace
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceFocus {
    #[default]
    Tree,
    Editor,
    DagPreview,
}

/// Workspace view combining tree, editor, and DAG
pub struct WorkspaceView {
    // === File Tree (Left Panel) ===
    /// File browser state
    pub browser: StandaloneState,
    /// Tree widget state (selection, expansion)
    pub tree_state: TreeState,
    /// Tree root node (lazily built from browser)
    tree_root: Option<TreeNode>,
    /// Animation ticker for glow effects
    ticker: AnimationTicker,
    /// Current filter mode
    filter: TreeFilter,
    /// Show hidden files toggle
    show_hidden: bool,
    /// Search query (fuzzy)
    search_query: String,
    /// Search mode active
    search_active: bool,

    // === YAML Editor (Center Panel) ===
    /// Text buffer with cursor
    pub buffer: TextBuffer,
    /// Editor mode (Normal/Insert)
    pub mode: EditorMode,
    /// Edit history (undo/redo)
    edit_history: EditHistory,
    /// Current file path
    pub current_file: Option<PathBuf>,
    /// Modified flag
    pub modified: bool,

    // === DAG Preview (Right Panel) ===
    /// Cached parsed workflow
    cached_workflow: RefCell<Option<Workflow>>,
    /// Content hash for cache invalidation
    cached_hash: Cell<u64>,
    /// DAG expanded mode
    dag_expanded: bool,
    /// DAG scroll offset
    dag_scroll: u16,

    // === Validation ===
    pub validation: ValidationResult,
    validation_pending: bool,

    // === Layout ===
    /// Current focus panel
    pub focus: WorkspaceFocus,
    /// Panel widths (percentages)
    tree_width: u16,
    editor_width: u16,
    dag_width: u16,

    // === Animation ===
    pub frame: u8,
}

impl WorkspaceView {
    pub fn new(root: PathBuf) -> Self {
        let browser = StandaloneState::new(root);
        let tree_state = TreeState::new_with_count(browser.browser_entries.len());

        Self {
            browser,
            tree_state,
            tree_root: None,
            ticker: AnimationTicker::new(),
            filter: TreeFilter::All,
            show_hidden: false,
            search_query: String::new(),
            search_active: false,

            buffer: TextBuffer::default(),
            mode: EditorMode::Normal,
            edit_history: EditHistory::new(100),
            current_file: None,
            modified: false,

            cached_workflow: RefCell::new(None),
            cached_hash: Cell::new(0),
            dag_expanded: false,
            dag_scroll: 0,

            validation: ValidationResult::default(),
            validation_pending: false,

            focus: WorkspaceFocus::Tree,
            tree_width: 20,
            editor_width: 50,
            dag_width: 30,

            frame: 0,
        }
    }

    /// Tick animations
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        self.ticker.tick();
    }

    /// Open file in editor
    pub fn open_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        self.buffer = TextBuffer::from_content(&content);
        self.current_file = Some(path);
        self.modified = false;
        self.edit_history.init(&content, 0);
        self.validate();
        self.update_dag_cache();
        Ok(())
    }

    /// Update DAG cache from current buffer
    fn update_dag_cache(&self) {
        let content = self.buffer.content();
        let hash = Self::content_hash(&content);

        if hash != self.cached_hash.get() {
            let workflow: Result<Workflow, _> = serde_yaml::from_str(&content);
            *self.cached_workflow.borrow_mut() = workflow.ok();
            self.cached_hash.set(hash);
        }
    }

    // ... render methods for each panel ...
}
```

### B.2 Panel Rendering

```rust
impl WorkspaceView {
    /// Render the workspace layout
    fn render_layout(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Vertical split: main content + status bar
        let [main_area, status_area] = Layout::vertical([
            Constraint::Min(10),
            Constraint::Length(1),
        ]).areas(area);

        // Horizontal split: tree | editor | dag
        let [tree_area, editor_area, dag_area] = Layout::horizontal([
            Constraint::Percentage(self.tree_width),
            Constraint::Percentage(self.editor_width),
            Constraint::Percentage(self.dag_width),
        ]).areas(main_area);

        // Render each panel
        self.render_tree(frame, tree_area, theme);
        self.render_editor(frame, editor_area, theme);
        self.render_dag(frame, dag_area, theme);
        self.render_status(frame, status_area, theme);
    }

    /// Render file tree panel
    fn render_tree(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Build tree widget from browser entries
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.tree_title())
            .border_style(self.panel_border_style(WorkspaceFocus::Tree, theme));

        // Use our custom TreeWidget
        if let Some(ref root) = self.tree_root {
            let widget = TreeWidget::new(root, &self.tree_state, &self.ticker)
                .block(block)
                .show_hidden(self.show_hidden);
            frame.render_widget(widget, area);
        } else {
            // Fallback: render block only
            frame.render_widget(block, area);
        }
    }

    /// Render YAML editor panel with syntax highlighting
    fn render_editor(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.editor_title())
            .border_style(self.panel_border_style(WorkspaceFocus::Editor, theme));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Render lines with verb-colored syntax highlighting
        self.render_yaml_lines(frame, inner, theme);

        // Render cursor if in editor focus
        if self.focus == WorkspaceFocus::Editor {
            self.render_cursor(frame, inner);
        }
    }

    /// Render DAG preview panel (live sync with YAML)
    fn render_dag(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" DAG ")
            .border_style(self.panel_border_style(WorkspaceFocus::DagPreview, theme));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Render DagAscii from cached workflow
        if let Some(ref wf) = *self.cached_workflow.borrow() {
            let nodes = self.build_dag_nodes(wf);
            let deps = Self::extract_flow_dependencies(wf);
            let mode = if self.dag_expanded {
                NodeBoxMode::Expanded
            } else {
                NodeBoxMode::Minimal
            };

            let dag = DagAscii::new(&nodes)
                .with_dependencies(deps)
                .mode(mode)
                .scroll(0, self.dag_scroll);

            dag.render(inner, frame.buffer_mut());
        }
    }
}
```

### B.3 Keyboard Handling with Focus

```rust
impl View for WorkspaceView {
    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        // Global shortcuts (work regardless of focus)
        match key.code {
            // Tab cycles focus: Tree → Editor → DAG → Tree
            KeyCode::Tab => {
                self.focus = match self.focus {
                    WorkspaceFocus::Tree => WorkspaceFocus::Editor,
                    WorkspaceFocus::Editor => WorkspaceFocus::DagPreview,
                    WorkspaceFocus::DagPreview => WorkspaceFocus::Tree,
                };
                return ViewAction::None;
            }
            // Ctrl+S saves file
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.save_file().ok();
                return ViewAction::None;
            }
            // Ctrl+P opens fuzzy finder
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_active = true;
                return ViewAction::None;
            }
            _ => {}
        }

        // Focus-specific handling
        match self.focus {
            WorkspaceFocus::Tree => self.handle_tree_key(key),
            WorkspaceFocus::Editor => self.handle_editor_key(key),
            WorkspaceFocus::DagPreview => self.handle_dag_key(key),
        }
    }

    fn handle_tree_key(&mut self, key: KeyEvent) -> ViewAction {
        // Search mode
        if self.search_active {
            return self.handle_search_key(key);
        }

        // Filter shortcuts
        match key.code {
            KeyCode::Char('H') => {
                self.show_hidden = !self.show_hidden;
                return ViewAction::None;
            }
            KeyCode::Char('W') => {
                self.filter = TreeFilter::WorkflowsOnly;
                return ViewAction::None;
            }
            KeyCode::Char('A') => {
                self.filter = TreeFilter::AgentsOnly;
                return ViewAction::None;
            }
            KeyCode::Char('/') => {
                self.search_active = true;
                return ViewAction::None;
            }
            _ => {}
        }

        // TreeAction navigation
        if let Some(action) = TreeAction::from_key_event(key) {
            match action {
                TreeAction::Select | TreeAction::Toggle => {
                    // Open selected file in editor
                    if let Some(entry) = self.selected_entry() {
                        if !entry.is_dir {
                            self.open_file(entry.path.clone()).ok();
                            self.focus = WorkspaceFocus::Editor;
                        } else {
                            self.toggle_folder();
                        }
                    }
                }
                _ => {
                    self.handle_tree_action(action);
                }
            }
        }

        ViewAction::None
    }
}
```

---

## Phase C: LSP Integration (Future)

### C.1 LSP Client Setup

The Nika LSP server is in development. Integration points:

```rust
// src/tui/lsp/client.rs

use tower_lsp::Client;

pub struct NikaLspClient {
    client: Option<Client>,
    diagnostics: Vec<Diagnostic>,
    completions: Vec<CompletionItem>,
}

impl NikaLspClient {
    /// Connect to nika-lsp server
    pub async fn connect() -> Result<Self, LspError> {
        // Spawn nika-lsp process
        // Connect via stdio
    }

    /// Request completions at cursor position
    pub async fn complete(&self, pos: Position) -> Vec<CompletionItem> {
        // textDocument/completion
    }

    /// Get diagnostics for current file
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
```

### C.2 Completion Popup Widget

```rust
// src/tui/widgets/completion_popup.rs

pub struct CompletionPopup {
    items: Vec<CompletionItem>,
    selected: usize,
    visible: bool,
}

impl Widget for CompletionPopup {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Render floating completion list near cursor
    }
}
```

---

## Implementation Sequence

### Day 1: Phase A (Tree Widget Completion)

1. **A.2**: Create `animation.rs` (glow system)
2. **A.3**: Create `render.rs` (TreeWidget)
3. **A.4**: Create `filter.rs` (H/W/A keys)
4. Update `mod.rs` exports
5. Run tests, commit

### Day 2: Phase B.1-B.2 (WorkspaceView Foundation)

1. Create `workspace.rs` struct
2. Implement layout rendering
3. Migrate tree rendering from HomeView
4. Migrate editor from StudioView
5. Add focus management

### Day 3: Phase B.3 (Integration)

1. Wire keyboard handling
2. Sync file selection → editor → DAG
3. Add status bar with validation
4. Test full workflow
5. Code review + commit

### Day 4: Polish + LSP Prep

1. Performance optimization
2. Edge cases (empty dirs, large files)
3. LSP client skeleton
4. Documentation update

---

## Files to Create/Modify

### New Files

| File | Lines | Purpose |
|------|-------|---------|
| `src/tui/widgets/tree/animation.rs` | ~100 | Glow animation system |
| `src/tui/widgets/tree/render.rs` | ~200 | TreeWidget renderer |
| `src/tui/widgets/tree/filter.rs` | ~80 | Filtering system |
| `src/tui/views/workspace.rs` | ~1500 | Unified view |
| `src/tui/lsp/client.rs` | ~200 | LSP client (future) |
| `src/tui/lsp/mod.rs` | ~20 | LSP module |

### Modified Files

| File | Changes |
|------|---------|
| `src/tui/widgets/tree/mod.rs` | Add new module exports |
| `src/tui/views/mod.rs` | Add WorkspaceView |
| `src/tui/app.rs` | Wire WorkspaceView as default |
| `src/tui/state.rs` | Add workspace state |

---

## Success Criteria

- [ ] Tree widget renders with icons and colors
- [ ] Glow animation visible on ecosystem files
- [ ] Filter modes work (H/W/A keys)
- [ ] WorkspaceView renders 3-panel layout
- [ ] Focus cycles with Tab
- [ ] File selection opens in editor
- [ ] DAG updates live as YAML changes
- [ ] Validation status in footer
- [ ] All tests pass
- [ ] No clippy warnings

---

## References

- Design doc: `docs/plans/2026-03-04-enhanced-tree-view-design.md`
- ratatui Layout: `/ratatui/ratatui` Context7 docs
- Current HomeView: `src/tui/views/home.rs`
- Current StudioView: `src/tui/views/studio.rs`
- Tree widget: `src/tui/widgets/tree/`
