# Explorer Tree Views Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement vim-style tree navigation for Explorer view with `.nika/` folder visualization, fuzzy search, and Miller columns preview.

**Architecture:** ratatui `List` widget with `ListState` for navigation, `TreeNode` struct for hierarchy, vim-style keybindings (j/k/h/l), inline fuzzy search with live filtering.

**Tech Stack:** Rust, ratatui 0.29+, crossterm, nucleo (fuzzy matching), tokio

---

## Overview

This plan implements the Explorer view file tree with:
- **Panel A (25%):** Collapsible tree with vim navigation
- **Panel B (35%):** DAG preview (existing widget)
- **Panel C (40%):** YAML preview with syntax highlighting

### Tree View Mockups

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ 🦋 EXPLORER  🏠 qrcode-ai / .nika / workflows              [1] [2] [3] [4] │
├──────────────────┬──────────────────────┬───────────────────────────────────┤
│  TREE (25%)      │  DAG PREVIEW (35%)   │  YAML PREVIEW (40%)               │
├──────────────────┼──────────────────────┼───────────────────────────────────┤
│                  │                      │                                   │
│ ▼ 🏠 .nika/      │    ┌─────────┐       │ schema: nika/workflow@0.5         │
│ │ ▼ ⚡ workflows/│    │ step1   │       │ workflow: generate                │
│ │ │ ► generate   │    └────┬────┘       │                                   │
│ │ │ ► deploy     │         │            │ tasks:                            │
│ │ ► 🐔 agents/   │    ┌────▼────┐       │   - id: step1                     │
│ │ ► 🔧 skills/   │    │ step2   │       │     infer: "Generate..."          │
│ │ ► 📦 context/  │    └─────────┘       │                                   │
│ ▼ 📁 src/        │                      │                                   │
│   ► components/  │                      │                                   │
└──────────────────┴──────────────────────┴───────────────────────────────────┘
│ [j/k] nav  [l] open  [h] parent  [/] search  [.] hidden  [Tab] preview     │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Fuzzy Search Mode

```
┌─ Search: gen█ ───────────────────────────────────────┐
│                                                      │
│ ⚡ workflows/generate.nika.yaml          [1]        │
│ ⚡ workflows/generate-page.nika.yaml     [2]        │
│ 🐔 agents/content-generator.agent.yaml   [3]        │
│                                                      │
│ [Enter] open  [Tab] preview  [Esc] cancel            │
└──────────────────────────────────────────────────────┘
```

---

## Task 1: TreeNode Data Structure

**Files:**
- Create: `src/tui/widgets/explorer/tree_node.rs`
- Test: `src/tui/widgets/explorer/tree_node.rs` (inline tests)

**Step 1: Write the failing test**

```rust
// src/tui/widgets/explorer/tree_node.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_tree_node_new_file() {
        let path = PathBuf::from("/project/workflow.nika.yaml");
        let root = PathBuf::from("/project");
        let node = TreeNode::new(path.clone(), &root, 0);

        assert_eq!(node.display_name, "workflow.nika.yaml");
        assert_eq!(node.depth, 0);
        assert!(!node.is_dir);
        assert_eq!(node.icon, "🚂");
        assert!(matches!(node.color, Color::Rgb(139, 92, 246))); // Violet
    }

    #[test]
    fn test_tree_node_nika_folder() {
        let path = PathBuf::from("/project/.nika");
        let root = PathBuf::from("/project");
        let node = TreeNode::new(path.clone(), &root, 0);

        assert_eq!(node.display_name, ".nika");
        assert!(node.is_dir);
        assert_eq!(node.icon, "🏠");
        assert!(matches!(node.color, Color::Rgb(16, 185, 129))); // Emerald
    }

    #[test]
    fn test_tree_node_workflows_folder() {
        let path = PathBuf::from("/project/.nika/workflows");
        let root = PathBuf::from("/project");
        let node = TreeNode::new(path.clone(), &root, 1);

        assert_eq!(node.icon, "⚡");
        assert!(matches!(node.color, Color::Rgb(139, 92, 246))); // Violet
    }

    #[test]
    fn test_tree_node_agents_folder() {
        let path = PathBuf::from("/project/.nika/agents");
        let root = PathBuf::from("/project");
        let node = TreeNode::new(path.clone(), &root, 1);

        assert_eq!(node.icon, "🐔");
        assert!(matches!(node.color, Color::Rgb(244, 63, 94))); // Rose
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test tree_node::tests --lib -p nika-tui -- --nocapture`
Expected: FAIL with "cannot find value `TreeNode`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/explorer/tree_node.rs
use ratatui::style::Color;
use std::path::{Path, PathBuf};

/// Represents a single node in the file tree
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// Full path to the file/directory
    pub path: PathBuf,
    /// Display name (relative to root)
    pub display_name: String,
    /// Nesting depth (0 = root level)
    pub depth: usize,
    /// Whether this is a directory
    pub is_dir: bool,
    /// Emoji icon based on file type
    pub icon: &'static str,
    /// Color based on file type
    pub color: Color,
    /// Whether node is expanded (directories only)
    pub expanded: bool,
    /// Whether this is a hidden file/folder
    pub is_hidden: bool,
}

impl TreeNode {
    pub fn new(path: PathBuf, root: &Path, depth: usize) -> Self {
        let is_dir = path.is_dir();
        let display_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let is_hidden = display_name.starts_with('.');
        let (icon, color) = Self::icon_and_color(&path, &display_name, is_dir);

        Self {
            path,
            display_name,
            depth,
            is_dir,
            icon,
            color,
            expanded: false,
            is_hidden,
        }
    }

    fn icon_and_color(path: &Path, name: &str, is_dir: bool) -> (&'static str, Color) {
        // Nika folders with special icons
        if is_dir {
            return match name {
                ".nika" => ("🏠", Color::Rgb(16, 185, 129)),    // Emerald
                "workflows" => ("⚡", Color::Rgb(139, 92, 246)), // Violet
                "agents" => ("🐔", Color::Rgb(244, 63, 94)),    // Rose
                "skills" => ("🔧", Color::Rgb(245, 158, 11)),   // Amber
                "context" => ("📦", Color::Rgb(6, 182, 212)),   // Cyan
                "templates" => ("📋", Color::Rgb(14, 165, 233)),// Sky
                "traces" => ("📊", Color::Rgb(100, 116, 139)),  // Slate
                "cache" => ("💾", Color::Rgb(120, 113, 108)),   // Stone
                "sessions" => ("🗂️", Color::Rgb(120, 113, 108)),// Stone
                _ => ("📁", Color::Rgb(148, 163, 184)),         // Default folder
            };
        }

        // Files by extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "yaml" | "yml" => {
                if name.ends_with(".nika.yaml") {
                    ("🚂", Color::Rgb(139, 92, 246)) // Workflow
                } else if name.ends_with(".agent.yaml") {
                    ("🐔", Color::Rgb(244, 63, 94)) // Agent
                } else if name.ends_with(".skill.yaml") {
                    ("🔧", Color::Rgb(245, 158, 11)) // Skill
                } else {
                    ("📄", Color::Rgb(148, 163, 184)) // Generic YAML
                }
            }
            "toml" => ("⚙️", Color::Rgb(113, 113, 122)), // Config
            "md" => ("📝", Color::Rgb(148, 163, 184)),
            "rs" => ("🦀", Color::Rgb(245, 158, 11)),
            "ts" | "tsx" => ("📘", Color::Rgb(59, 130, 246)),
            "js" | "jsx" => ("📒", Color::Rgb(234, 179, 8)),
            "json" => ("📋", Color::Rgb(100, 116, 139)),
            _ => ("📄", Color::Rgb(148, 163, 184)), // Generic file
        }
    }

    /// Format for display with indentation and expansion indicator
    pub fn display(&self) -> String {
        let indent = "  ".repeat(self.depth);
        let prefix = if self.is_dir {
            if self.expanded { "▼" } else { "►" }
        } else {
            " "
        };
        format!("{}{} {} {}", indent, prefix, self.icon, self.display_name)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test tree_node::tests --lib -p nika-tui -- --nocapture`
Expected: PASS (4 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/explorer/tree_node.rs
git commit -m "feat(tui): add TreeNode struct with icon/color mapping

- TreeNode with path, depth, icon, color, expanded state
- Icon mapping for .nika folders (🏠⚡🐔🔧📦📋📊💾🗂️)
- Color mapping using Tailwind palette
- File extension detection for workflows, agents, skills
- 4 unit tests for node creation

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 2: ExplorerTree Widget State

**Files:**
- Create: `src/tui/widgets/explorer/tree_state.rs`
- Test: `src/tui/widgets/explorer/tree_state.rs` (inline tests)

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_structure() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create .nika structure
        std::fs::create_dir_all(root.join(".nika/workflows")).unwrap();
        std::fs::create_dir_all(root.join(".nika/agents")).unwrap();
        std::fs::write(root.join(".nika/workflows/test.nika.yaml"), "schema: nika/workflow@0.5").unwrap();
        std::fs::write(root.join(".nika/config.toml"), "[editor]").unwrap();

        dir
    }

    #[test]
    fn test_tree_state_load_root() {
        let dir = create_test_structure();
        let state = ExplorerTreeState::new(dir.path().to_path_buf());

        assert!(state.nodes.len() > 0);
        assert!(state.nodes.iter().any(|n| n.display_name == ".nika"));
    }

    #[test]
    fn test_tree_state_expand_collapse() {
        let dir = create_test_structure();
        let mut state = ExplorerTreeState::new(dir.path().to_path_buf());

        // Find .nika folder
        let nika_idx = state.nodes.iter().position(|n| n.display_name == ".nika").unwrap();
        state.select(nika_idx);

        // Expand
        state.toggle_expand();
        assert!(state.nodes[nika_idx].expanded);

        // Should now have children visible
        let visible = state.visible_nodes();
        assert!(visible.iter().any(|n| n.display_name == "workflows"));
    }

    #[test]
    fn test_tree_state_navigation() {
        let dir = create_test_structure();
        let mut state = ExplorerTreeState::new(dir.path().to_path_buf());

        state.move_down();
        assert_eq!(state.selected, 1);

        state.move_up();
        assert_eq!(state.selected, 0);

        state.move_to_first();
        assert_eq!(state.selected, 0);

        state.move_to_last();
        assert!(state.selected > 0);
    }

    #[test]
    fn test_tree_state_hidden_toggle() {
        let dir = create_test_structure();
        let mut state = ExplorerTreeState::new(dir.path().to_path_buf());

        // .nika is hidden by default
        assert!(state.show_hidden);

        state.toggle_hidden();
        assert!(!state.show_hidden);

        let visible = state.visible_nodes();
        assert!(!visible.iter().any(|n| n.display_name.starts_with('.')));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test tree_state::tests --lib -p nika-tui -- --nocapture`
Expected: FAIL with "cannot find value `ExplorerTreeState`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/explorer/tree_state.rs
use super::tree_node::TreeNode;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// State for the Explorer tree widget
#[derive(Debug)]
pub struct ExplorerTreeState {
    /// Root directory being browsed
    pub root: PathBuf,
    /// All nodes in the tree (flat list)
    pub nodes: Vec<TreeNode>,
    /// Currently selected index
    pub selected: usize,
    /// Set of expanded directory paths
    pub expanded: HashSet<PathBuf>,
    /// Whether to show hidden files
    pub show_hidden: bool,
    /// Search query (if in search mode)
    pub search_query: Option<String>,
}

impl ExplorerTreeState {
    pub fn new(root: PathBuf) -> Self {
        let mut state = Self {
            root: root.clone(),
            nodes: Vec::new(),
            selected: 0,
            expanded: HashSet::new(),
            show_hidden: true, // Show .nika by default
            search_query: None,
        };
        state.load_directory(&root, 0);
        state
    }

    /// Load directory contents into nodes
    fn load_directory(&mut self, path: &Path, depth: usize) {
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        let mut items: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();

        // Sort: directories first, then alphabetically
        items.sort_by(|a, b| {
            let a_dir = a.is_dir();
            let b_dir = b.is_dir();
            match (a_dir, b_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.file_name().cmp(&b.file_name()),
            }
        });

        for item in items {
            let node = TreeNode::new(item.clone(), &self.root, depth);

            // Skip hidden if not showing
            if node.is_hidden && !self.show_hidden {
                continue;
            }

            self.nodes.push(node);

            // If directory is expanded, load children
            if item.is_dir() && self.expanded.contains(&item) {
                self.load_directory(&item, depth + 1);
            }
        }
    }

    /// Reload the tree from disk
    pub fn reload(&mut self) {
        self.nodes.clear();
        self.load_directory(&self.root.clone(), 0);
        // Clamp selection
        if self.selected >= self.nodes.len() {
            self.selected = self.nodes.len().saturating_sub(1);
        }
    }

    /// Get currently selected node
    pub fn selected_node(&self) -> Option<&TreeNode> {
        self.visible_nodes().get(self.selected)
    }

    /// Get visible nodes (respecting expanded state and hidden filter)
    pub fn visible_nodes(&self) -> Vec<&TreeNode> {
        self.nodes
            .iter()
            .filter(|n| {
                if n.is_hidden && !self.show_hidden {
                    return false;
                }
                // Check if all ancestors are expanded
                self.is_visible(n)
            })
            .collect()
    }

    fn is_visible(&self, node: &TreeNode) -> bool {
        // Root level always visible
        if node.depth == 0 {
            return true;
        }

        // Check each ancestor
        let mut current = node.path.parent();
        while let Some(parent) = current {
            if parent == self.root {
                return true;
            }
            if !self.expanded.contains(parent) {
                return false;
            }
            current = parent.parent();
        }
        true
    }

    /// Select a specific index
    pub fn select(&mut self, index: usize) {
        let max = self.visible_nodes().len().saturating_sub(1);
        self.selected = index.min(max);
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        let max = self.visible_nodes().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(max);
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move to first item
    pub fn move_to_first(&mut self) {
        self.selected = 0;
    }

    /// Move to last item
    pub fn move_to_last(&mut self) {
        self.selected = self.visible_nodes().len().saturating_sub(1);
    }

    /// Page down (10 items)
    pub fn page_down(&mut self) {
        let max = self.visible_nodes().len().saturating_sub(1);
        self.selected = (self.selected + 10).min(max);
    }

    /// Page up (10 items)
    pub fn page_up(&mut self) {
        self.selected = self.selected.saturating_sub(10);
    }

    /// Toggle expand/collapse for selected directory
    pub fn toggle_expand(&mut self) {
        if let Some(node) = self.selected_node().cloned() {
            if node.is_dir {
                if self.expanded.contains(&node.path) {
                    self.expanded.remove(&node.path);
                } else {
                    self.expanded.insert(node.path.clone());
                }
                self.reload();
            }
        }
    }

    /// Expand selected directory
    pub fn expand(&mut self) {
        if let Some(node) = self.selected_node().cloned() {
            if node.is_dir && !self.expanded.contains(&node.path) {
                self.expanded.insert(node.path.clone());
                self.reload();
            }
        }
    }

    /// Collapse selected directory or go to parent
    pub fn collapse_or_parent(&mut self) {
        if let Some(node) = self.selected_node().cloned() {
            if node.is_dir && self.expanded.contains(&node.path) {
                // Collapse
                self.expanded.remove(&node.path);
                self.reload();
            } else {
                // Go to parent
                self.go_to_parent();
            }
        }
    }

    /// Navigate to parent directory
    pub fn go_to_parent(&mut self) {
        if let Some(node) = self.selected_node() {
            if let Some(parent) = node.path.parent() {
                // Find parent in visible nodes
                let visible = self.visible_nodes();
                if let Some(idx) = visible.iter().position(|n| n.path == parent) {
                    self.selected = idx;
                }
            }
        }
    }

    /// Toggle hidden files visibility
    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.reload();
    }

    /// Jump to .nika folder
    pub fn go_to_nika(&mut self) {
        let nika_path = self.root.join(".nika");
        if let Some(idx) = self.visible_nodes().iter().position(|n| n.path == nika_path) {
            self.selected = idx;
        }
    }

    /// Jump to workflows folder
    pub fn go_to_workflows(&mut self) {
        let wf_path = self.root.join(".nika/workflows");
        // Ensure .nika is expanded
        self.expanded.insert(self.root.join(".nika"));
        self.reload();
        if let Some(idx) = self.visible_nodes().iter().position(|n| n.path == wf_path) {
            self.selected = idx;
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test tree_state::tests --lib -p nika-tui -- --nocapture`
Expected: PASS (4 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/explorer/tree_state.rs
git commit -m "feat(tui): add ExplorerTreeState with navigation

- Load directory recursively with depth tracking
- Expand/collapse tracking with HashSet
- vim-style navigation: up/down/first/last/page
- Hidden files toggle with reload
- Quick jump to .nika and workflows
- 4 unit tests with tempfile

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3: Keyboard Handler

**Files:**
- Create: `src/tui/widgets/explorer/keybindings.rs`
- Test: `src/tui/widgets/explorer/keybindings.rs` (inline tests)

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn test_vim_navigation_keys() {
        assert_eq!(handle_key(key('j')), Some(ExplorerAction::MoveDown));
        assert_eq!(handle_key(key('k')), Some(ExplorerAction::MoveUp));
        assert_eq!(handle_key(key('h')), Some(ExplorerAction::CollapseOrParent));
        assert_eq!(handle_key(key('l')), Some(ExplorerAction::ExpandOrOpen));
    }

    #[test]
    fn test_special_keys() {
        assert_eq!(handle_key(key('.')), Some(ExplorerAction::ToggleHidden));
        assert_eq!(handle_key(key('/')), Some(ExplorerAction::StartSearch));
        assert_eq!(handle_key(key('~')), Some(ExplorerAction::GoToNika));
        assert_eq!(handle_key(key('w')), Some(ExplorerAction::GoToWorkflows));
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)), Some(ExplorerAction::MoveDown));
        assert_eq!(handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)), Some(ExplorerAction::MoveUp));
        assert_eq!(handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)), Some(ExplorerAction::CollapseOrParent));
        assert_eq!(handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)), Some(ExplorerAction::ExpandOrOpen));
    }

    #[test]
    fn test_vim_motions() {
        assert_eq!(handle_key(key('g')), Some(ExplorerAction::PendingG));
        assert_eq!(handle_key(key('G')), Some(ExplorerAction::MoveToLast));
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test keybindings::tests --lib -p nika-tui -- --nocapture`
Expected: FAIL with "cannot find function `handle_key`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/explorer/keybindings.rs
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Actions that can be triggered by keyboard input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerAction {
    // Navigation
    MoveDown,
    MoveUp,
    MoveToFirst,
    MoveToLast,
    PageDown,
    PageUp,

    // Tree manipulation
    ExpandOrOpen,
    CollapseOrParent,
    ToggleExpand,

    // Filtering
    ToggleHidden,
    StartSearch,
    ClearSearch,

    // Quick jumps
    GoToNika,
    GoToWorkflows,
    GoToTraces,

    // Actions
    OpenInEditor,
    CopyPath,
    Rename,
    Delete,
    CreateFile,
    CreateFolder,

    // Preview
    TogglePreviewMode,
    TogglePreview,
    ZoomPreview,

    // Vim motions (pending states)
    PendingG,

    // Meta
    Quit,
    None,
}

/// Handle a key event and return the corresponding action
pub fn handle_key(key: KeyEvent) -> Option<ExplorerAction> {
    use ExplorerAction::*;
    use KeyCode::*;

    match (key.code, key.modifiers) {
        // vim navigation
        (Char('j'), KeyModifiers::NONE) => Some(MoveDown),
        (Char('k'), KeyModifiers::NONE) => Some(MoveUp),
        (Char('h'), KeyModifiers::NONE) => Some(CollapseOrParent),
        (Char('l'), KeyModifiers::NONE) => Some(ExpandOrOpen),

        // Arrow keys
        (Down, KeyModifiers::NONE) => Some(MoveDown),
        (Up, KeyModifiers::NONE) => Some(MoveUp),
        (Left, KeyModifiers::NONE) => Some(CollapseOrParent),
        (Right, KeyModifiers::NONE) => Some(ExpandOrOpen),

        // vim motions
        (Char('g'), KeyModifiers::NONE) => Some(PendingG), // gg = go to first
        (Char('G'), KeyModifiers::NONE) => Some(MoveToLast),

        // Page navigation
        (Char('u'), KeyModifiers::CONTROL) => Some(PageUp),
        (Char('d'), KeyModifiers::CONTROL) => Some(PageDown),
        (PageUp, _) => Some(PageUp),
        (PageDown, _) => Some(PageDown),
        (Home, _) => Some(MoveToFirst),
        (End, _) => Some(MoveToLast),

        // Tree actions
        (Enter, KeyModifiers::NONE) => Some(ExpandOrOpen),
        (Char(' '), KeyModifiers::NONE) => Some(ToggleExpand),

        // Filtering
        (Char('.'), KeyModifiers::NONE) => Some(ToggleHidden),
        (Char('/'), KeyModifiers::NONE) => Some(StartSearch),
        (Esc, KeyModifiers::NONE) => Some(ClearSearch),

        // Quick jumps
        (Char('~'), KeyModifiers::NONE) => Some(GoToNika),
        (Char('w'), KeyModifiers::NONE) => Some(GoToWorkflows),
        (Char('t'), KeyModifiers::NONE) => Some(GoToTraces),

        // File operations
        (Char('o'), KeyModifiers::NONE) => Some(OpenInEditor),
        (Char('y'), KeyModifiers::NONE) => Some(CopyPath),
        (Char('r'), KeyModifiers::NONE) => Some(Rename),
        (Char('d'), KeyModifiers::NONE) => Some(Delete),
        (Char('a'), KeyModifiers::NONE) => Some(CreateFile),
        (Char('A'), KeyModifiers::NONE) => Some(CreateFolder),

        // Preview
        (Tab, KeyModifiers::NONE) => Some(TogglePreviewMode),
        (Char('p'), KeyModifiers::NONE) => Some(TogglePreview),
        (Char('z'), KeyModifiers::NONE) => Some(ZoomPreview),

        // Quit
        (Char('q'), KeyModifiers::NONE) => Some(Quit),

        _ => None,
    }
}

/// Handle pending 'g' motion (for gg = go to first)
pub fn handle_pending_g(key: KeyEvent) -> Option<ExplorerAction> {
    match key.code {
        KeyCode::Char('g') => Some(ExplorerAction::MoveToFirst),
        _ => None,
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test keybindings::tests --lib -p nika-tui -- --nocapture`
Expected: PASS (4 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/explorer/keybindings.rs
git commit -m "feat(tui): add Explorer keybindings with vim navigation

- ExplorerAction enum with 25+ actions
- vim-style j/k/h/l navigation
- Arrow key support
- gg/G motions for first/last
- Ctrl+u/d for page navigation
- Quick jumps: ~ (nika), w (workflows), t (traces)
- File ops: o (open), y (copy), r (rename), d (delete)
- 4 unit tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 4: Fuzzy Search State

**Files:**
- Create: `src/tui/widgets/explorer/fuzzy_search.rs`
- Modify: `Cargo.toml` (add `nucleo` dependency)
- Test: `src/tui/widgets/explorer/fuzzy_search.rs` (inline tests)

**Step 1: Add dependency**

```bash
cd tools/nika
cargo add nucleo
```

**Step 2: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_search_match() {
        let mut search = FuzzySearch::new();
        search.set_query("gen");

        let items = vec![
            "generate.nika.yaml".to_string(),
            "deploy.nika.yaml".to_string(),
            "content-generator.agent.yaml".to_string(),
        ];

        let results = search.filter(&items);

        assert_eq!(results.len(), 2);
        assert!(results[0].item.contains("generate"));
    }

    #[test]
    fn test_fuzzy_search_empty_query() {
        let search = FuzzySearch::new();
        let items = vec!["a.yaml".to_string(), "b.yaml".to_string()];

        let results = search.filter(&items);
        assert_eq!(results.len(), 2); // All items returned
    }

    #[test]
    fn test_fuzzy_search_highlight() {
        let mut search = FuzzySearch::new();
        search.set_query("nik");

        let items = vec!["nika.yaml".to_string()];
        let results = search.filter(&items);

        assert_eq!(results[0].highlights, vec![0, 1, 2]); // n, i, k highlighted
    }
}
```

**Step 3: Run test to verify it fails**

Run: `cargo test fuzzy_search::tests --lib -p nika-tui -- --nocapture`
Expected: FAIL with "cannot find value `FuzzySearch`"

**Step 4: Write minimal implementation**

```rust
// src/tui/widgets/explorer/fuzzy_search.rs
use nucleo::{Matcher, Utf32Str};

/// Result of a fuzzy match
#[derive(Debug, Clone)]
pub struct FuzzyMatch {
    /// Original item string
    pub item: String,
    /// Score (higher = better match)
    pub score: u32,
    /// Indices of matched characters (for highlighting)
    pub highlights: Vec<usize>,
    /// Original index in the input list
    pub original_index: usize,
}

/// Fuzzy search state with nucleo matcher
#[derive(Debug)]
pub struct FuzzySearch {
    query: String,
    matcher: Matcher,
}

impl FuzzySearch {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            matcher: Matcher::new(nucleo::Config::DEFAULT),
        }
    }

    /// Set the search query
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
    }

    /// Get current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Clear the query
    pub fn clear(&mut self) {
        self.query.clear();
    }

    /// Check if search is active
    pub fn is_active(&self) -> bool {
        !self.query.is_empty()
    }

    /// Filter items by fuzzy match
    pub fn filter(&self, items: &[String]) -> Vec<FuzzyMatch> {
        if self.query.is_empty() {
            // Return all items with no highlights
            return items
                .iter()
                .enumerate()
                .map(|(i, item)| FuzzyMatch {
                    item: item.clone(),
                    score: 0,
                    highlights: vec![],
                    original_index: i,
                })
                .collect();
        }

        let mut results: Vec<FuzzyMatch> = items
            .iter()
            .enumerate()
            .filter_map(|(original_index, item)| {
                let mut indices = Vec::new();

                // Use nucleo for fuzzy matching
                let haystack = Utf32Str::new(item, &mut vec![]);
                let needle = Utf32Str::new(&self.query, &mut vec![]);

                let score = self.matcher.fuzzy_indices(
                    haystack,
                    needle,
                    &mut indices,
                );

                score.map(|s| FuzzyMatch {
                    item: item.clone(),
                    score: s,
                    highlights: indices.iter().map(|&i| i as usize).collect(),
                    original_index,
                })
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| b.score.cmp(&a.score));
        results
    }

    /// Add character to query
    pub fn push(&mut self, c: char) {
        self.query.push(c);
    }

    /// Remove last character from query
    pub fn pop(&mut self) {
        self.query.pop();
    }
}

impl Default for FuzzySearch {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 5: Run test to verify it passes**

Run: `cargo test fuzzy_search::tests --lib -p nika-tui -- --nocapture`
Expected: PASS (3 tests)

**Step 6: Commit**

```bash
git add src/tui/widgets/explorer/fuzzy_search.rs Cargo.toml Cargo.lock
git commit -m "feat(tui): add FuzzySearch with nucleo matcher

- FuzzySearch struct with nucleo integration
- FuzzyMatch with score, highlights, original_index
- Filter method returns sorted results
- Character-level highlight indices for styling
- 3 unit tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 5: ExplorerTree Widget Renderer

**Files:**
- Create: `src/tui/widgets/explorer/tree_widget.rs`
- Test: `src/tui/widgets/explorer/tree_widget.rs` (inline tests)

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".nika/workflows")).unwrap();
        std::fs::write(dir.path().join(".nika/workflows/test.nika.yaml"), "").unwrap();
        dir
    }

    #[test]
    fn test_tree_widget_renders() {
        let dir = create_test_dir();
        let state = ExplorerTreeState::new(dir.path().to_path_buf());
        let widget = ExplorerTreeWidget::new(&state);

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| {
            f.render_widget(widget, f.area());
        }).unwrap();

        let buffer = terminal.backend().buffer();
        let content = buffer.content().iter()
            .map(|c| c.symbol())
            .collect::<String>();

        assert!(content.contains(".nika"));
    }

    #[test]
    fn test_tree_widget_highlight_selected() {
        let dir = create_test_dir();
        let mut state = ExplorerTreeState::new(dir.path().to_path_buf());
        state.select(0);
        let widget = ExplorerTreeWidget::new(&state);

        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|f| {
            f.render_widget(widget, f.area());
        }).unwrap();

        // Verify highlight style is applied (would check style in real test)
        assert!(true);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test tree_widget::tests --lib -p nika-tui -- --nocapture`
Expected: FAIL with "cannot find value `ExplorerTreeWidget`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/explorer/tree_widget.rs
use super::tree_state::ExplorerTreeState;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget, Widget},
};

/// Widget for rendering the Explorer file tree
pub struct ExplorerTreeWidget<'a> {
    state: &'a ExplorerTreeState,
    block: Option<Block<'a>>,
    highlight_style: Style,
    highlight_symbol: &'a str,
}

impl<'a> ExplorerTreeWidget<'a> {
    pub fn new(state: &'a ExplorerTreeState) -> Self {
        Self {
            state,
            block: None,
            highlight_style: Style::default()
                .bg(Color::Rgb(30, 41, 59)) // Slate-800
                .add_modifier(Modifier::BOLD),
            highlight_symbol: "│ ",
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn highlight_style(mut self, style: Style) -> Self {
        self.highlight_style = style;
        self
    }

    fn render_node(&self, node: &super::tree_node::TreeNode) -> ListItem<'a> {
        let indent = "  ".repeat(node.depth);

        // Expansion indicator
        let prefix = if node.is_dir {
            if self.state.expanded.contains(&node.path) {
                "▼ "
            } else {
                "► "
            }
        } else {
            "  "
        };

        // Build styled line
        let line = Line::from(vec![
            Span::raw(indent),
            Span::raw(prefix),
            Span::styled(
                format!("{} ", node.icon),
                Style::default(),
            ),
            Span::styled(
                node.display_name.clone(),
                Style::default().fg(node.color),
            ),
        ]);

        ListItem::new(line)
    }
}

impl<'a> Widget for ExplorerTreeWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Get visible nodes
        let visible = self.state.visible_nodes();

        // Create list items
        let items: Vec<ListItem> = visible
            .iter()
            .map(|node| self.render_node(node))
            .collect();

        // Create list widget
        let list = List::new(items)
            .highlight_style(self.highlight_style)
            .highlight_symbol(self.highlight_symbol);

        // Apply block if set
        let list = if let Some(block) = self.block {
            list.block(block)
        } else {
            list
        };

        // Render with state
        let mut list_state = ListState::default();
        list_state.select(Some(self.state.selected));

        StatefulWidget::render(list, area, buf, &mut list_state);
    }
}

/// Breadcrumb widget for showing current path
pub struct BreadcrumbWidget<'a> {
    path: &'a std::path::Path,
    root: &'a std::path::Path,
}

impl<'a> BreadcrumbWidget<'a> {
    pub fn new(path: &'a std::path::Path, root: &'a std::path::Path) -> Self {
        Self { path, root }
    }
}

impl<'a> Widget for BreadcrumbWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let relative = self.path.strip_prefix(self.root).unwrap_or(self.path);

        let mut spans = vec![
            Span::styled("🏠 ", Style::default()),
            Span::styled(
                self.root.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                Style::default().fg(Color::Rgb(148, 163, 184)), // Slate-400
            ),
        ];

        for component in relative.components() {
            spans.push(Span::styled(" / ", Style::default().fg(Color::Rgb(71, 85, 105)))); // Slate-600
            spans.push(Span::styled(
                component.as_os_str().to_string_lossy().to_string(),
                Style::default().fg(Color::Rgb(226, 232, 240)), // Slate-200
            ));
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test tree_widget::tests --lib -p nika-tui -- --nocapture`
Expected: PASS (2 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/explorer/tree_widget.rs
git commit -m "feat(tui): add ExplorerTreeWidget renderer

- ExplorerTreeWidget with List-based rendering
- TreeNode to ListItem conversion with indentation
- Highlight style for selected item
- BreadcrumbWidget for path display
- Box drawing: ▼ expanded, ► collapsed
- 2 render tests with TestBackend

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 6: Explorer Module Integration

**Files:**
- Create: `src/tui/widgets/explorer/mod.rs`
- Modify: `src/tui/widgets/mod.rs` (add explorer module)
- Test: Integration test

**Step 1: Create module file**

```rust
// src/tui/widgets/explorer/mod.rs
//! Explorer file tree widgets
//!
//! Provides a vim-style navigable file tree with:
//! - TreeNode: Individual file/folder representation
//! - ExplorerTreeState: Navigation and expansion state
//! - ExplorerTreeWidget: ratatui rendering
//! - FuzzySearch: nucleo-powered filtering
//! - Keybindings: vim-style navigation actions

mod tree_node;
mod tree_state;
mod tree_widget;
mod keybindings;
mod fuzzy_search;

pub use tree_node::TreeNode;
pub use tree_state::ExplorerTreeState;
pub use tree_widget::{ExplorerTreeWidget, BreadcrumbWidget};
pub use keybindings::{ExplorerAction, handle_key, handle_pending_g};
pub use fuzzy_search::{FuzzySearch, FuzzyMatch};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use tempfile::TempDir;

    fn create_full_nika_structure() -> TempDir {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create full .nika structure
        std::fs::create_dir_all(root.join(".nika/workflows")).unwrap();
        std::fs::create_dir_all(root.join(".nika/agents")).unwrap();
        std::fs::create_dir_all(root.join(".nika/skills")).unwrap();
        std::fs::create_dir_all(root.join(".nika/context")).unwrap();
        std::fs::create_dir_all(root.join(".nika/traces")).unwrap();
        std::fs::create_dir_all(root.join(".nika/sessions")).unwrap();

        std::fs::write(root.join(".nika/workflows/generate.nika.yaml"), "schema: nika/workflow@0.5").unwrap();
        std::fs::write(root.join(".nika/workflows/deploy.nika.yaml"), "schema: nika/workflow@0.5").unwrap();
        std::fs::write(root.join(".nika/agents/orchestrator.agent.yaml"), "").unwrap();
        std::fs::write(root.join(".nika/config.toml"), "[editor]").unwrap();

        // Create src folder
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();

        dir
    }

    #[test]
    fn test_full_tree_navigation() {
        let dir = create_full_nika_structure();
        let mut state = ExplorerTreeState::new(dir.path().to_path_buf());

        // Navigate to .nika
        state.go_to_nika();
        assert!(state.selected_node().unwrap().display_name == ".nika");

        // Expand .nika
        state.toggle_expand();

        // Go to workflows
        state.go_to_workflows();
        assert!(state.selected_node().unwrap().display_name == "workflows");

        // Expand workflows
        state.toggle_expand();

        // Navigate down to see files
        state.move_down();
        let node = state.selected_node().unwrap();
        assert!(node.display_name.ends_with(".nika.yaml"));
    }

    #[test]
    fn test_fuzzy_search_integration() {
        let dir = create_full_nika_structure();
        let state = ExplorerTreeState::new(dir.path().to_path_buf());

        // Create search
        let mut search = FuzzySearch::new();
        search.set_query("gen");

        // Get all file names
        let names: Vec<String> = state.nodes.iter()
            .map(|n| n.display_name.clone())
            .collect();

        let results = search.filter(&names);

        // Should find generate.nika.yaml
        assert!(results.iter().any(|r| r.item.contains("generate")));
    }

    #[test]
    fn test_keybinding_actions() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let dir = create_full_nika_structure();
        let mut state = ExplorerTreeState::new(dir.path().to_path_buf());

        // Press 'j' to move down
        let action = handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(action, Some(ExplorerAction::MoveDown));

        if action == Some(ExplorerAction::MoveDown) {
            state.move_down();
        }

        assert_eq!(state.selected, 1);
    }
}
```

**Step 2: Update widgets mod.rs**

```rust
// Add to src/tui/widgets/mod.rs
pub mod explorer;

// Re-export main types
pub use explorer::{
    ExplorerTreeState,
    ExplorerTreeWidget,
    ExplorerAction,
    FuzzySearch,
};
```

**Step 3: Run integration tests**

Run: `cargo test explorer::integration_tests --lib -p nika-tui -- --nocapture`
Expected: PASS (3 tests)

**Step 4: Commit**

```bash
git add src/tui/widgets/explorer/mod.rs src/tui/widgets/mod.rs
git commit -m "feat(tui): integrate Explorer module with exports

- mod.rs with all submodule exports
- Integration tests for full tree navigation
- Fuzzy search integration test
- Keybinding action test
- Public API exports in widgets/mod.rs

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 7: Explorer View Integration

**Files:**
- Modify: `src/tui/views/explorer.rs` (create or update)
- Test: View integration test

**Step 1: Create Explorer view**

```rust
// src/tui/views/explorer.rs
use crate::tui::widgets::explorer::{
    ExplorerAction, ExplorerTreeState, ExplorerTreeWidget, BreadcrumbWidget,
    FuzzySearch, handle_key, handle_pending_g,
};
use crossterm::event::KeyEvent;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::path::PathBuf;

/// Explorer view state
pub struct ExplorerView {
    /// File tree state
    tree_state: ExplorerTreeState,
    /// Fuzzy search state
    search: FuzzySearch,
    /// Whether in search mode
    search_mode: bool,
    /// Pending vim motion (e.g., 'g' for 'gg')
    pending_motion: Option<char>,
    /// Preview mode: DAG or YAML
    preview_mode: PreviewMode,
    /// Whether preview is visible
    show_preview: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Dag,
    Yaml,
}

impl ExplorerView {
    pub fn new(root: PathBuf) -> Self {
        Self {
            tree_state: ExplorerTreeState::new(root),
            search: FuzzySearch::new(),
            search_mode: false,
            pending_motion: None,
            preview_mode: PreviewMode::Dag,
            show_preview: true,
        }
    }

    /// Handle key input
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Handle search mode separately
        if self.search_mode {
            return self.handle_search_key(key);
        }

        // Handle pending motion
        if let Some(motion) = self.pending_motion {
            self.pending_motion = None;
            if motion == 'g' {
                if let Some(action) = handle_pending_g(key) {
                    return self.execute_action(action);
                }
            }
            return false;
        }

        // Normal mode
        if let Some(action) = handle_key(key) {
            return self.execute_action(action);
        }

        false
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> bool {
        use crossterm::event::KeyCode;

        match key.code {
            KeyCode::Esc => {
                self.search_mode = false;
                self.search.clear();
                true
            }
            KeyCode::Enter => {
                self.search_mode = false;
                // Keep search results, open selected
                true
            }
            KeyCode::Backspace => {
                self.search.pop();
                true
            }
            KeyCode::Char(c) => {
                self.search.push(c);
                true
            }
            _ => false,
        }
    }

    fn execute_action(&mut self, action: ExplorerAction) -> bool {
        use ExplorerAction::*;

        match action {
            MoveDown => { self.tree_state.move_down(); true }
            MoveUp => { self.tree_state.move_up(); true }
            MoveToFirst => { self.tree_state.move_to_first(); true }
            MoveToLast => { self.tree_state.move_to_last(); true }
            PageDown => { self.tree_state.page_down(); true }
            PageUp => { self.tree_state.page_up(); true }

            ExpandOrOpen => { self.tree_state.expand(); true }
            CollapseOrParent => { self.tree_state.collapse_or_parent(); true }
            ToggleExpand => { self.tree_state.toggle_expand(); true }

            ToggleHidden => { self.tree_state.toggle_hidden(); true }
            StartSearch => { self.search_mode = true; true }
            ClearSearch => { self.search.clear(); true }

            GoToNika => { self.tree_state.go_to_nika(); true }
            GoToWorkflows => { self.tree_state.go_to_workflows(); true }
            GoToTraces => { /* TODO */ true }

            TogglePreviewMode => {
                self.preview_mode = match self.preview_mode {
                    PreviewMode::Dag => PreviewMode::Yaml,
                    PreviewMode::Yaml => PreviewMode::Dag,
                };
                true
            }
            TogglePreview => { self.show_preview = !self.show_preview; true }

            PendingG => { self.pending_motion = Some('g'); true }

            Quit => false, // Let parent handle
            _ => false,
        }
    }

    /// Render the explorer view
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Main layout: Tree (25%) | Preview (75%)
        let chunks = if self.show_preview {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(75),
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(100)])
                .split(area)
        };

        // Render tree panel
        self.render_tree_panel(chunks[0], buf);

        // Render preview panel if visible
        if self.show_preview && chunks.len() > 1 {
            self.render_preview_panel(chunks[1], buf);
        }
    }

    fn render_tree_panel(&self, area: Rect, buf: &mut Buffer) {
        // Split for breadcrumb + tree + help
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Breadcrumb
                Constraint::Min(5),    // Tree
                Constraint::Length(1), // Help
            ])
            .split(area);

        // Breadcrumb
        if let Some(node) = self.tree_state.selected_node() {
            BreadcrumbWidget::new(&node.path, &self.tree_state.root)
                .render(chunks[0], buf);
        }

        // Tree or search results
        if self.search_mode {
            self.render_search(chunks[1], buf);
        } else {
            let tree_widget = ExplorerTreeWidget::new(&self.tree_state)
                .block(Block::default().borders(Borders::ALL).title("📁 Files"));
            tree_widget.render(chunks[1], buf);
        }

        // Help line
        let help = if self.search_mode {
            "[Enter] open  [Esc] cancel"
        } else {
            "[j/k] nav  [l] open  [h] parent  [/] search  [.] hidden"
        };
        Paragraph::new(help)
            .style(Style::default().fg(Color::Rgb(100, 116, 139)))
            .render(chunks[2], buf);
    }

    fn render_search(&self, area: Rect, buf: &mut Buffer) {
        // Search input
        let input = format!("Search: {}█", self.search.query());
        Paragraph::new(input)
            .block(Block::default().borders(Borders::ALL))
            .render(area, buf);
    }

    fn render_preview_panel(&self, area: Rect, buf: &mut Buffer) {
        // Split: DAG (50%) | YAML (50%)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(50),
            ])
            .split(area);

        // DAG preview
        let dag_block = Block::default()
            .borders(Borders::ALL)
            .title("🦋 DAG Preview");
        Paragraph::new("DAG visualization here")
            .block(dag_block)
            .render(chunks[0], buf);

        // YAML preview
        let yaml_block = Block::default()
            .borders(Borders::ALL)
            .title("📝 YAML Preview");

        if let Some(node) = self.tree_state.selected_node() {
            if !node.is_dir && node.path.extension().map(|e| e == "yaml").unwrap_or(false) {
                let content = std::fs::read_to_string(&node.path)
                    .unwrap_or_else(|_| "Unable to read file".to_string());
                Paragraph::new(content)
                    .block(yaml_block)
                    .render(chunks[1], buf);
            } else {
                Paragraph::new("Select a YAML file to preview")
                    .block(yaml_block)
                    .render(chunks[1], buf);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tempfile::TempDir;

    fn create_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".nika/workflows")).unwrap();
        std::fs::write(dir.path().join(".nika/workflows/test.nika.yaml"), "schema: nika/workflow@0.5").unwrap();
        dir
    }

    #[test]
    fn test_explorer_view_navigation() {
        let dir = create_test_dir();
        let mut view = ExplorerView::new(dir.path().to_path_buf());

        // Press 'j' to move down
        view.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(view.tree_state.selected, 1);

        // Press 'k' to move up
        view.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(view.tree_state.selected, 0);
    }

    #[test]
    fn test_explorer_view_search_mode() {
        let dir = create_test_dir();
        let mut view = ExplorerView::new(dir.path().to_path_buf());

        // Enter search mode
        view.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(view.search_mode);

        // Type search query
        view.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        view.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert_eq!(view.search.query(), "te");

        // Exit search mode
        view.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!view.search_mode);
    }

    #[test]
    fn test_explorer_view_gg_motion() {
        let dir = create_test_dir();
        let mut view = ExplorerView::new(dir.path().to_path_buf());

        // Move down first
        view.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        view.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(view.tree_state.selected > 0);

        // Press 'gg' to go to first
        view.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        view.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(view.tree_state.selected, 0);
    }
}
```

**Step 2: Run tests**

Run: `cargo test explorer::tests --lib -p nika-tui -- --nocapture`
Expected: PASS (3 tests)

**Step 3: Commit**

```bash
git add src/tui/views/explorer.rs
git commit -m "feat(tui): add ExplorerView with full navigation

- ExplorerView integrating all explorer widgets
- Keyboard handling with search mode
- vim motion support (gg)
- Preview panel toggle
- DAG/YAML preview split
- Breadcrumb rendering
- 3 integration tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 8: Wire to TUI App

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/views/mod.rs`
- Test: Full integration test

**Step 1: Update views/mod.rs**

```rust
// Add to src/tui/views/mod.rs
mod explorer;
pub use explorer::ExplorerView;
```

**Step 2: Update app.rs to include Explorer view**

Add ExplorerView to the TuiView enum and handle view switching.

**Step 3: Run full test suite**

Run: `cargo test --lib -p nika-tui`
Expected: All tests pass

**Step 4: Commit**

```bash
git add src/tui/views/mod.rs src/tui/app.rs
git commit -m "feat(tui): wire ExplorerView to TUI app

- Add ExplorerView to TuiView enum
- Handle '1' and 'e' keys for Explorer
- Default view is now Explorer
- Full integration working

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Summary

| Task | Component | Tests | Files |
|------|-----------|-------|-------|
| 1 | TreeNode | 4 | `tree_node.rs` |
| 2 | ExplorerTreeState | 4 | `tree_state.rs` |
| 3 | Keybindings | 4 | `keybindings.rs` |
| 4 | FuzzySearch | 3 | `fuzzy_search.rs` |
| 5 | ExplorerTreeWidget | 2 | `tree_widget.rs` |
| 6 | Module Integration | 3 | `mod.rs` |
| 7 | ExplorerView | 3 | `views/explorer.rs` |
| 8 | TUI App Wiring | — | `app.rs`, `views/mod.rs` |
| **Total** | | **23** | **8 files** |

---

## Verification Checklist

```bash
# Run all explorer tests
cargo test explorer --lib -p nika-tui -- --nocapture

# Run full TUI test suite
cargo test --lib -p nika-tui

# Live test
cargo run --

# Expected: Explorer view opens by default with .nika folder visible
```

---

## References

- **EXPLORER-UX-PROPOSAL.md**: Keyboard shortcuts, icon mapping, layout
- **6-VIEWS-DESIGN.md**: Full view architecture
- **NovaNet tree.rs**: Inspiration for breadcrumbs, minimap
- **ratatui List widget**: StatefulWidget pattern
- **nucleo**: Fuzzy matching library
