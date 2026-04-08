// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tree state management for selection and expansion.
//!
//! Tracks which nodes are selected, expanded, and provides keyboard navigation.

use super::{NodeId, TreeNode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rustc_hash::FxHashSet;

/// Actions that can be performed on the tree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeAction {
    /// Move selection up
    Up,
    /// Move selection down
    Down,
    /// Expand current node (or move into if already expanded)
    Right,
    /// Collapse current node (or move to parent)
    Left,
    /// Toggle expansion state
    Toggle,
    /// Select/open the current node
    Select,
    /// Jump to first visible node
    First,
    /// Jump to last visible node
    Last,
    /// Page up
    PageUp,
    /// Page down
    PageDown,
}

impl TreeAction {
    /// Convert a key event to a tree action (Vim + Arrow key navigation)
    ///
    /// Key mappings:
    /// - `j` / `Down` → Down
    /// - `k` / `Up` → Up
    /// - `h` / `Left` → Left (collapse or parent)
    /// - `l` / `Right` → Right (expand or into)
    /// - `Enter` / `Space` → Toggle
    /// - `o` → Select/Open
    /// - `g` → First
    /// - `G` (Shift+g) → Last
    /// - `Ctrl+d` / `PageDown` → PageDown
    /// - `Ctrl+u` / `PageUp` → PageUp
    pub fn from_key_event(key: KeyEvent) -> Option<Self> {
        match key.code {
            // Vim navigation (hjkl)
            KeyCode::Char('j') => Some(TreeAction::Down),
            KeyCode::Char('k') => Some(TreeAction::Up),
            KeyCode::Char('h') => Some(TreeAction::Left),
            KeyCode::Char('l') => Some(TreeAction::Right),

            // Arrow keys
            KeyCode::Down => Some(TreeAction::Down),
            KeyCode::Up => Some(TreeAction::Up),
            KeyCode::Left => Some(TreeAction::Left),
            KeyCode::Right => Some(TreeAction::Right),

            // Selection/Toggle
            KeyCode::Enter | KeyCode::Char(' ') => Some(TreeAction::Toggle),
            KeyCode::Char('o') => Some(TreeAction::Select),

            // Jump to first/last
            KeyCode::Char('g') if key.modifiers.is_empty() => Some(TreeAction::First),
            KeyCode::Char('G') => Some(TreeAction::Last),
            KeyCode::Home => Some(TreeAction::First),
            KeyCode::End => Some(TreeAction::Last),

            // Page up/down
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TreeAction::PageDown)
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                Some(TreeAction::PageUp)
            }
            KeyCode::PageDown => Some(TreeAction::PageDown),
            KeyCode::PageUp => Some(TreeAction::PageUp),

            _ => None,
        }
    }
}

/// Tree state tracking selection and expansion
#[derive(Debug, Clone)]
pub struct TreeState {
    /// Currently selected node ID
    selected: Option<NodeId>,
    /// Set of expanded node IDs
    expanded: FxHashSet<NodeId>,
    /// Scroll offset for viewport
    scroll_offset: usize,
    /// Number of visible rows (viewport height)
    viewport_height: usize,
    /// Flat list of visible node IDs (cached for navigation)
    visible_nodes: Vec<NodeId>,
    /// Current selection index in flat list (for indexed navigation)
    selection_index: Option<usize>,
}

impl Default for TreeState {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeState {
    /// Create a new tree state
    pub fn new() -> Self {
        Self {
            selected: None,
            expanded: FxHashSet::default(),
            scroll_offset: 0,
            viewport_height: 20,
            visible_nodes: Vec::new(),
            selection_index: None,
        }
    }

    /// Create a new tree state with a pre-allocated visible nodes capacity
    ///
    /// Also selects the first node (index 0) if count > 0.
    pub fn new_with_count(count: usize) -> Self {
        let mut state = Self {
            selected: None,
            expanded: FxHashSet::default(),
            scroll_offset: 0,
            viewport_height: 20,
            visible_nodes: Vec::with_capacity(count),
            selection_index: None,
        };
        // Select first node by default
        if count > 0 {
            state.selection_index = Some(0);
        }
        state
    }

    // === Index-based navigation for flat list views ===

    /// Get the current selection index
    pub fn selection_index(&self) -> Option<usize> {
        self.selection_index
    }

    /// Set the selection index
    pub fn set_selection_index(&mut self, index: Option<usize>) {
        self.selection_index = index;
    }

    /// Move selection to next index (with bounds checking)
    pub fn select_next_index(&mut self, max: usize) {
        match self.selection_index {
            Some(idx) if idx < max.saturating_sub(1) => {
                self.selection_index = Some(idx + 1);
            }
            None if max > 0 => {
                self.selection_index = Some(0);
            }
            _ => {}
        }
    }

    /// Move selection to previous index (with bounds checking)
    pub fn select_prev_index(&mut self) {
        if let Some(idx) = self.selection_index {
            if idx > 0 {
                self.selection_index = Some(idx - 1);
            }
        }
    }

    /// Get the currently selected node ID
    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    /// Set the selected node ID
    pub fn set_selected(&mut self, id: Option<NodeId>) {
        self.selected = id;
        self.ensure_visible();
    }

    /// Check if a node is selected
    pub fn is_selected(&self, id: NodeId) -> bool {
        self.selected == Some(id)
    }

    /// Check if a node is expanded
    pub fn is_expanded(&self, id: NodeId) -> bool {
        self.expanded.contains(&id)
    }

    /// Expand a node
    pub fn expand(&mut self, id: NodeId) {
        self.expanded.insert(id);
    }

    /// Collapse a node
    pub fn collapse(&mut self, id: NodeId) {
        self.expanded.remove(&id);
    }

    /// Toggle expansion state of a node
    pub fn toggle(&mut self, id: NodeId) {
        if self.is_expanded(id) {
            self.collapse(id);
        } else {
            self.expand(id);
        }
    }

    /// Expand all nodes
    pub fn expand_all(&mut self, root: &TreeNode) {
        self.expand_recursive(root);
    }

    fn expand_recursive(&mut self, node: &TreeNode) {
        if node.is_directory() {
            self.expand(node.id);
            for child in &node.children {
                self.expand_recursive(child);
            }
        }
    }

    /// Collapse all nodes
    pub fn collapse_all(&mut self) {
        self.expanded.clear();
    }

    /// Set viewport height
    pub fn set_viewport_height(&mut self, height: usize) {
        self.viewport_height = height.max(1);
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Update the cached visible nodes list
    pub fn update_visible_nodes(&mut self, root: &TreeNode) {
        self.visible_nodes.clear();
        self.collect_visible_nodes(root);
    }

    fn collect_visible_nodes(&mut self, node: &TreeNode) {
        self.visible_nodes.push(node.id);
        if self.is_expanded(node.id) {
            for child in &node.children {
                self.collect_visible_nodes(child);
            }
        }
    }

    /// Get the visible nodes slice
    pub fn visible_nodes(&self) -> &[NodeId] {
        &self.visible_nodes
    }

    /// Handle a tree action
    pub fn handle_action(&mut self, action: TreeAction, root: &TreeNode) -> Option<NodeId> {
        match action {
            TreeAction::Up => self.move_up(),
            TreeAction::Down => self.move_down(),
            TreeAction::Left => self.move_left(root),
            TreeAction::Right => self.move_right(root),
            TreeAction::Toggle => self.toggle_selected(),
            TreeAction::Select => self.selected,
            TreeAction::First => self.move_first(),
            TreeAction::Last => self.move_last(),
            TreeAction::PageUp => self.page_up(),
            TreeAction::PageDown => self.page_down(),
        }
    }

    fn move_up(&mut self) -> Option<NodeId> {
        if self.visible_nodes.is_empty() {
            return None;
        }

        let current_idx = self.current_index();
        if current_idx > 0 {
            let new_idx = current_idx - 1;
            self.selected = Some(self.visible_nodes[new_idx]);
            self.ensure_visible();
        }
        self.selected
    }

    fn move_down(&mut self) -> Option<NodeId> {
        if self.visible_nodes.is_empty() {
            return None;
        }

        let current_idx = self.current_index();
        if current_idx < self.visible_nodes.len().saturating_sub(1) {
            let new_idx = current_idx + 1;
            self.selected = Some(self.visible_nodes[new_idx]);
            self.ensure_visible();
        }
        self.selected
    }

    fn move_left(&mut self, root: &TreeNode) -> Option<NodeId> {
        let selected_id = self.selected?;

        // If expanded, collapse
        if self.is_expanded(selected_id) {
            self.collapse(selected_id);
            return self.selected;
        }

        // Otherwise, move to parent
        if let Some(parent_id) = self.find_parent(root, selected_id) {
            self.selected = Some(parent_id);
            self.ensure_visible();
        }
        self.selected
    }

    fn move_right(&mut self, root: &TreeNode) -> Option<NodeId> {
        let selected_id = self.selected?;

        // If collapsed directory, expand
        if !self.is_expanded(selected_id) {
            if let Some(node) = self.find_node(root, selected_id) {
                if node.is_directory() && !node.children.is_empty() {
                    self.expand(selected_id);
                    self.update_visible_nodes(root);
                    return self.selected;
                }
            }
        }

        // If expanded, move to first child
        if self.is_expanded(selected_id) {
            if let Some(node) = self.find_node(root, selected_id) {
                if let Some(first_child) = node.children.first() {
                    self.selected = Some(first_child.id);
                    self.ensure_visible();
                }
            }
        }
        self.selected
    }

    fn toggle_selected(&mut self) -> Option<NodeId> {
        if let Some(selected_id) = self.selected {
            self.toggle(selected_id);
        }
        self.selected
    }

    fn move_first(&mut self) -> Option<NodeId> {
        if !self.visible_nodes.is_empty() {
            self.selected = Some(self.visible_nodes[0]);
            self.scroll_offset = 0;
        }
        self.selected
    }

    fn move_last(&mut self) -> Option<NodeId> {
        if let Some(last) = self.visible_nodes.last() {
            self.selected = Some(*last);
            self.ensure_visible();
        }
        self.selected
    }

    fn page_up(&mut self) -> Option<NodeId> {
        if self.visible_nodes.is_empty() {
            return None;
        }

        let current_idx = self.current_index();
        let new_idx = current_idx.saturating_sub(self.viewport_height);
        self.selected = Some(self.visible_nodes[new_idx]);
        self.ensure_visible();
        self.selected
    }

    fn page_down(&mut self) -> Option<NodeId> {
        if self.visible_nodes.is_empty() {
            return None;
        }

        let current_idx = self.current_index();
        let max_idx = self.visible_nodes.len().saturating_sub(1);
        let new_idx = (current_idx + self.viewport_height).min(max_idx);
        self.selected = Some(self.visible_nodes[new_idx]);
        self.ensure_visible();
        self.selected
    }

    fn current_index(&self) -> usize {
        self.selected
            .and_then(|id| self.visible_nodes.iter().position(|&n| n == id))
            .unwrap_or(0)
    }

    fn ensure_visible(&mut self) {
        let current_idx = self.current_index();

        // Scroll up if needed
        if current_idx < self.scroll_offset {
            self.scroll_offset = current_idx;
        }

        // Scroll down if needed
        if current_idx >= self.scroll_offset + self.viewport_height {
            self.scroll_offset = current_idx - self.viewport_height + 1;
        }
    }

    /// Find a node by ID in the tree (recursive search)
    ///
    /// `self` is the method receiver for API ergonomics; the search is purely
    /// tree-structural and does not read any `TreeState` fields.
    #[allow(clippy::only_used_in_recursion)]
    pub fn find_node<'a>(&self, root: &'a TreeNode, target: NodeId) -> Option<&'a TreeNode> {
        if root.id == target {
            return Some(root);
        }
        for child in &root.children {
            if let Some(found) = self.find_node(child, target) {
                return Some(found);
            }
        }
        None
    }

    fn find_parent(&self, root: &TreeNode, target: NodeId) -> Option<NodeId> {
        self.find_parent_recursive(root, target, None)
    }

    /// `self` is the method receiver for consistency with `find_node`; the search
    /// is purely tree-structural and does not read any `TreeState` fields.
    #[allow(clippy::only_used_in_recursion)]
    fn find_parent_recursive(
        &self,
        node: &TreeNode,
        target: NodeId,
        parent: Option<NodeId>,
    ) -> Option<NodeId> {
        if node.id == target {
            return parent;
        }
        for child in &node.children {
            if let Some(found) = self.find_parent_recursive(child, target, Some(node.id)) {
                return Some(found);
            }
        }
        None
    }

    /// Select the first node if nothing is selected
    pub fn select_first_if_none(&mut self) {
        if self.selected.is_none() && !self.visible_nodes.is_empty() {
            self.selected = Some(self.visible_nodes[0]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    fn create_test_tree() -> TreeNode {
        TreeNode {
            id: 1,
            name: "root".to_string(),
            path: Utf8PathBuf::from("."),
            kind: super::super::NodeKind::Directory,
            git_status: None,
            children: vec![
                TreeNode {
                    id: 2,
                    name: "src".to_string(),
                    path: Utf8PathBuf::from("./src"),
                    kind: super::super::NodeKind::SrcFolder,
                    git_status: None,
                    children: vec![
                        TreeNode {
                            id: 3,
                            name: "main.rs".to_string(),
                            path: Utf8PathBuf::from("./src/main.rs"),
                            kind: super::super::NodeKind::Rust,
                            git_status: None,
                            children: vec![],
                            expanded: false,
                            depth: 2,
                        },
                        TreeNode {
                            id: 4,
                            name: "lib.rs".to_string(),
                            path: Utf8PathBuf::from("./src/lib.rs"),
                            kind: super::super::NodeKind::Rust,
                            git_status: None,
                            children: vec![],
                            expanded: false,
                            depth: 2,
                        },
                    ],
                    expanded: false,
                    depth: 1,
                },
                TreeNode {
                    id: 5,
                    name: "Cargo.toml".to_string(),
                    path: Utf8PathBuf::from("./Cargo.toml"),
                    kind: super::super::NodeKind::CargoToml,
                    git_status: None,
                    children: vec![],
                    expanded: false,
                    depth: 1,
                },
            ],
            expanded: false,
            depth: 0,
        }
    }

    #[test]
    fn test_tree_state_new() {
        let state = TreeState::new();
        assert!(state.selected().is_none());
        assert_eq!(state.scroll_offset(), 0);
    }

    #[test]
    fn test_expand_collapse() {
        let mut state = TreeState::new();
        let id = 1;

        assert!(!state.is_expanded(id));
        state.expand(id);
        assert!(state.is_expanded(id));
        state.collapse(id);
        assert!(!state.is_expanded(id));
    }

    #[test]
    fn test_toggle() {
        let mut state = TreeState::new();
        let id = 1;

        assert!(!state.is_expanded(id));
        state.toggle(id);
        assert!(state.is_expanded(id));
        state.toggle(id);
        assert!(!state.is_expanded(id));
    }

    #[test]
    fn test_selection() {
        let mut state = TreeState::new();

        assert!(state.selected().is_none());
        state.set_selected(Some(42));
        assert_eq!(state.selected(), Some(42));
        assert!(state.is_selected(42));
        assert!(!state.is_selected(1));
    }

    #[test]
    fn test_visible_nodes() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        // Initially only root is visible
        state.update_visible_nodes(&root);
        assert_eq!(state.visible_nodes().len(), 1);
        assert_eq!(state.visible_nodes()[0], 1);

        // Expand root
        state.expand(1);
        state.update_visible_nodes(&root);
        assert_eq!(state.visible_nodes().len(), 3); // root, src, Cargo.toml

        // Expand src
        state.expand(2);
        state.update_visible_nodes(&root);
        assert_eq!(state.visible_nodes().len(), 5); // root, src, main.rs, lib.rs, Cargo.toml
    }

    #[test]
    fn test_move_down_up() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand(1);
        state.expand(2);
        state.update_visible_nodes(&root);
        state.set_selected(Some(1)); // root

        // Move down
        state.handle_action(TreeAction::Down, &root);
        assert_eq!(state.selected(), Some(2)); // src

        state.handle_action(TreeAction::Down, &root);
        assert_eq!(state.selected(), Some(3)); // main.rs

        // Move up
        state.handle_action(TreeAction::Up, &root);
        assert_eq!(state.selected(), Some(2)); // src
    }

    #[test]
    fn test_move_left_collapse() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand(1);
        state.expand(2);
        state.update_visible_nodes(&root);
        state.set_selected(Some(2)); // src (expanded)

        // Left should collapse
        state.handle_action(TreeAction::Left, &root);
        assert!(!state.is_expanded(2));
    }

    #[test]
    fn test_move_left_to_parent() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand(1);
        state.expand(2);
        state.update_visible_nodes(&root);
        state.set_selected(Some(3)); // main.rs (file, no children)

        // Left should move to parent
        state.handle_action(TreeAction::Left, &root);
        assert_eq!(state.selected(), Some(2)); // src
    }

    #[test]
    fn test_move_right_expand() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand(1);
        state.update_visible_nodes(&root);
        state.set_selected(Some(2)); // src (collapsed)

        assert!(!state.is_expanded(2));

        // Right should expand
        state.handle_action(TreeAction::Right, &root);
        assert!(state.is_expanded(2));
    }

    #[test]
    fn test_move_first_last() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand(1);
        state.expand(2);
        state.update_visible_nodes(&root);
        state.set_selected(Some(3)); // middle

        state.handle_action(TreeAction::First, &root);
        assert_eq!(state.selected(), Some(1)); // root

        state.handle_action(TreeAction::Last, &root);
        assert_eq!(state.selected(), Some(5)); // Cargo.toml
    }

    #[test]
    fn test_expand_all() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand_all(&root);
        assert!(state.is_expanded(1));
        assert!(state.is_expanded(2));
        // Files are not expanded (no children)
    }

    #[test]
    fn test_collapse_all() {
        let mut state = TreeState::new();
        let root = create_test_tree();

        state.expand_all(&root);
        state.collapse_all();
        assert!(!state.is_expanded(1));
        assert!(!state.is_expanded(2));
    }

    // === Key Event Tests ===

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn make_key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_from_key_event_vim_navigation() {
        // hjkl vim keys
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('j'))),
            Some(TreeAction::Down)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('k'))),
            Some(TreeAction::Up)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('h'))),
            Some(TreeAction::Left)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('l'))),
            Some(TreeAction::Right)
        );
    }

    #[test]
    fn test_from_key_event_arrow_keys() {
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Down)),
            Some(TreeAction::Down)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Up)),
            Some(TreeAction::Up)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Left)),
            Some(TreeAction::Left)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Right)),
            Some(TreeAction::Right)
        );
    }

    #[test]
    fn test_from_key_event_toggle_select() {
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Enter)),
            Some(TreeAction::Toggle)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char(' '))),
            Some(TreeAction::Toggle)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('o'))),
            Some(TreeAction::Select)
        );
    }

    #[test]
    fn test_from_key_event_first_last() {
        // g for first (no modifier)
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('g'))),
            Some(TreeAction::First)
        );
        // G (uppercase) for last
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('G'))),
            Some(TreeAction::Last)
        );
        // Home/End keys
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Home)),
            Some(TreeAction::First)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::End)),
            Some(TreeAction::Last)
        );
    }

    #[test]
    fn test_from_key_event_page_up_down() {
        // Ctrl+d / Ctrl+u
        assert_eq!(
            TreeAction::from_key_event(make_key_with_mod(
                KeyCode::Char('d'),
                KeyModifiers::CONTROL
            )),
            Some(TreeAction::PageDown)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key_with_mod(
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            )),
            Some(TreeAction::PageUp)
        );
        // PageDown/PageUp keys
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::PageDown)),
            Some(TreeAction::PageDown)
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::PageUp)),
            Some(TreeAction::PageUp)
        );
    }

    #[test]
    fn test_from_key_event_unknown_keys() {
        // Unknown keys return None
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('x'))),
            None
        );
        assert_eq!(
            TreeAction::from_key_event(make_key(KeyCode::Char('q'))),
            None
        );
        assert_eq!(TreeAction::from_key_event(make_key(KeyCode::Tab)), None);
        assert_eq!(TreeAction::from_key_event(make_key(KeyCode::Esc)), None);
    }
}
