// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Browser Panel - Shared file browser for Studio and Runner views
//!
//! ## Features
//!
//! - **Breadcrumb**: Shows current path in title (e.g., `📁 ~/dev/nika/wf..`)
//! - **Ctrl+H Toggle**: Filter between All files and WorkflowsOnly (.nika.yaml)
//! - **Quick Access**: Shows recent .nika.yaml files at top
//! - **Git Integration**: Shows git status icons for files
//!
//! ## Usage
//!
//! ```rust,ignore
//! let mut browser = BrowserPanel::new(root_dir);
//!
//! // In render loop
//! browser.render(frame, area, theme, is_focused);
//!
//! // In key handler
//! match browser.handle_key(key) {
//!     BrowserAction::SelectFile(path) => { /* handle selection */ }
//!     BrowserAction::None => {}
//! }
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use camino::Utf8Path;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::theme::Theme;
use crate::widgets::tree::{
    build_git_status_cache, AnimationTicker, FilterConfig, GitStatusCache, TreeAction, TreeColors,
    TreeFilter, TreeNode, TreeState, TreeWidget,
};

/// Action returned from BrowserPanel key handling
#[derive(Debug, Clone)]
pub enum BrowserAction {
    /// No action
    None,
    /// File selected (user pressed Enter on a file)
    SelectFile(PathBuf),
    /// Request to refresh the tree
    Refresh,
}

/// Browser Panel - Reusable file browser component
///
/// Extracted from StudioView for sharing between Studio and Runner views.
/// Each view provides its own `on_select` behavior.
pub struct BrowserPanel {
    // === Core State ===
    /// Root directory being browsed
    pub root_dir: PathBuf,
    /// Tree widget state (selection, expansion, visible nodes)
    pub tree_state: TreeState,
    /// Tree colors (solarized theme)
    pub tree_colors: TreeColors,
    /// Animation ticker for glow effects
    pub animation_ticker: AnimationTicker,
    /// Filter configuration (All vs WorkflowsOnly)
    pub filter_config: FilterConfig,

    // === Caching ===
    /// Git status cache for file icons
    git_cache: GitStatusCache,
    /// Last git cache refresh time
    git_cache_time: Instant,
    /// Cached tree node (invalidated on filter change)
    cached_tree: Option<TreeNode>,

    // === Quick Access ===
    /// Recently accessed .nika.yaml files
    pub quick_access: Vec<PathBuf>,
}

impl BrowserPanel {
    /// Create a new BrowserPanel rooted at the given directory
    pub fn new(root_dir: PathBuf) -> Self {
        let quick_access = Self::discover_quick_access(&root_dir);
        let root_path_owned = root_dir.clone();
        let root_path = Utf8Path::from_path(&root_path_owned).unwrap_or(Utf8Path::new("."));
        let git_cache = build_git_status_cache(root_path);

        Self {
            root_dir,
            tree_state: TreeState::new(),
            tree_colors: TreeColors::solarized_dark(),
            animation_ticker: AnimationTicker::default(),
            filter_config: FilterConfig::default(),
            git_cache,
            git_cache_time: Instant::now(),
            cached_tree: None,
            quick_access,
        }
    }

    /// Discover quick access files (.nika.yaml in root and workflows/)
    fn discover_quick_access(root: &PathBuf) -> Vec<PathBuf> {
        let mut files = Vec::new();

        // First pass: root directory
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.filter_map(|e| e.ok()) {
                // Security: skip symlinks
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

        files.truncate(5);
        files
    }

    /// Refresh the tree cache
    pub fn refresh_tree(&mut self) {
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));
        self.git_cache = build_git_status_cache(root_path);
        self.git_cache_time = Instant::now();
        self.cached_tree = None;
    }

    /// Ensure tree is built and cached (zero-clone)
    fn ensure_tree(&mut self) {
        if self.cached_tree.is_none() {
            let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));
            let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
            self.cached_tree = Some(tree);
        }
    }

    /// Initialize tree state (call on view enter)
    pub fn on_enter(&mut self) {
        let root_path = Utf8Path::from_path(&self.root_dir).unwrap_or(Utf8Path::new("."));

        // 1. Refresh git cache
        self.git_cache = build_git_status_cache(root_path);
        self.git_cache_time = Instant::now();

        // 2. Build tree and cache it
        let root_node = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
        self.cached_tree = Some(root_node.clone());

        // 3. Update visible_nodes for navigation
        self.tree_state.update_visible_nodes(&root_node);

        // 4. Expand root directory
        self.tree_state.expand(root_node.id);

        // 5. Re-update visible_nodes after expansion
        self.tree_state.update_visible_nodes(&root_node);

        // 6. Select first item
        self.tree_state.select_first_if_none();
    }

    /// Tick animations
    pub fn tick(&mut self) {
        self.animation_ticker.tick();
    }

    /// Generate breadcrumb from current path
    ///
    /// Returns a shortened path like `~/dev/nika/wf..` with max 20 chars
    fn breadcrumb(&self) -> String {
        let path_str = self.root_dir.to_string_lossy();

        // Replace home directory with ~
        let home = dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_default();
        let display = if path_str.starts_with(&home) {
            // UTF-8 safe: strip_prefix handles char boundaries correctly
            path_str
                .strip_prefix(&home)
                .map(|rest| format!("~{}", rest))
                .unwrap_or_else(|| path_str.to_string())
        } else {
            path_str.to_string()
        };

        // Truncate if too long (UTF-8 safe: use chars())
        let char_count = display.chars().count();
        if char_count > 20 {
            let start: String = display.chars().take(8).collect();
            let end: String = display.chars().skip(char_count - 8).collect();
            format!("{}..{}", start, end)
        } else {
            display
        }
    }

    /// Render the browser panel
    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme, is_focused: bool) {
        let style = if is_focused {
            Style::default()
                .fg(theme.border_focused)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        };

        // Focus indicator and breadcrumb in title
        let focus_indicator = if is_focused { "●" } else { " " };
        let breadcrumb = self.breadcrumb();

        // Show filter badge when filtering is active
        let filter_badge = match self.filter_config.filter {
            TreeFilter::WorkflowsOnly => " [W]",
            TreeFilter::All => "",
            _ => self.filter_config.filter.badge(),
        };

        let title = format!(" {} 📁 {}{}", focus_indicator, breadcrumb, filter_badge);

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
            self.cached_tree = None;
        }

        // Build tree with caching
        let tree_rebuilt = self.cached_tree.is_none();
        let root_node = if let Some(ref cached) = self.cached_tree {
            cached.clone()
        } else {
            let tree = TreeNode::build_tree(root_path, Some(&self.git_cache), None);
            self.cached_tree = Some(tree.clone());
            tree
        };

        // Only update visible_nodes when tree structure changes
        if tree_rebuilt {
            self.tree_state.update_visible_nodes(&root_node);
            self.tree_state.select_first_if_none();
            if !self.tree_state.is_expanded(root_node.id) {
                self.tree_state.expand(root_node.id);
                self.tree_state.update_visible_nodes(&root_node);
            }
        }

        // Only rebuild tree colors when theme actually changes (avoids 16 Color copies per frame)
        if self.tree_colors.bg != theme.background {
            self.tree_colors = TreeColors::from_theme(theme);
        }

        // Render TreeWidget
        // FilterConfig is Copy — no heap allocation on clone
        let tree_widget = TreeWidget::new(&root_node)
            .colors(self.tree_colors.clone())
            .ticker(&self.animation_ticker)
            .filter(self.filter_config);

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

    /// Handle keyboard input
    ///
    /// Returns a BrowserAction indicating what happened.
    pub fn handle_key(&mut self, key: KeyEvent) -> BrowserAction {
        // PERF: Ensure tree exists, then borrow (zero-clone)
        self.ensure_tree();

        // Ctrl+H: Toggle filter between All and WorkflowsOnly
        if key.code == KeyCode::Char('h') && key.modifiers == KeyModifiers::CONTROL {
            let new_filter = match self.filter_config.filter {
                TreeFilter::WorkflowsOnly => TreeFilter::All,
                _ => TreeFilter::WorkflowsOnly,
            };
            self.filter_config.set_filter(new_filter);
            self.cached_tree = None; // Force rebuild
            return BrowserAction::Refresh;
        }

        // Try TreeAction navigation first
        if let Some(action) = TreeAction::from_key_event(key) {
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
            return BrowserAction::None;
        }

        // Handle other keys
        match key.code {
            KeyCode::Enter => {
                // Return selected file
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
                            return BrowserAction::SelectFile(path);
                        }
                    }
                }
                BrowserAction::None
            }
            // Refresh tree (R key)
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.refresh_tree();
                BrowserAction::Refresh
            }
            // Filter hotkeys (W/A/E/0)
            KeyCode::Char('w') | KeyCode::Char('W') => {
                self.filter_config.set_filter(TreeFilter::WorkflowsOnly);
                self.cached_tree = None;
                BrowserAction::Refresh
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.filter_config.set_filter(TreeFilter::All);
                self.cached_tree = None;
                BrowserAction::Refresh
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.filter_config.set_filter(TreeFilter::EcosystemOnly);
                self.cached_tree = None;
                BrowserAction::Refresh
            }
            KeyCode::Char('0') => {
                self.filter_config.reset();
                self.cached_tree = None;
                BrowserAction::Refresh
            }
            _ => BrowserAction::None,
        }
    }

    /// Get currently selected file path (if any)
    pub fn selected_file(&mut self) -> Option<PathBuf> {
        self.ensure_tree();
        if let Some(node_id) = self.tree_state.selected() {
            return self
                .cached_tree
                .as_ref()
                .and_then(|root| self.tree_state.find_node(root, node_id))
                .map(|node| PathBuf::from(node.path.as_str()))
                .filter(|path| path.is_file());
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_browser_panel_new() {
        let dir = env::current_dir().unwrap();
        let panel = BrowserPanel::new(dir.clone());
        assert_eq!(panel.root_dir, dir);
    }

    #[test]
    fn test_breadcrumb_generation() {
        let panel = BrowserPanel::new(PathBuf::from("/tmp/test"));
        let bc = panel.breadcrumb();
        assert!(!bc.is_empty());
    }

    #[test]
    fn test_filter_toggle() {
        let mut panel = BrowserPanel::new(PathBuf::from("/tmp"));

        // Initial state is All
        assert!(matches!(panel.filter_config.filter, TreeFilter::All));

        // Simulate Ctrl+H
        let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::CONTROL);
        let action = panel.handle_key(key);

        assert!(matches!(action, BrowserAction::Refresh));
        assert!(matches!(
            panel.filter_config.filter,
            TreeFilter::WorkflowsOnly
        ));

        // Toggle back
        let action = panel.handle_key(key);
        assert!(matches!(action, BrowserAction::Refresh));
        assert!(matches!(panel.filter_config.filter, TreeFilter::All));
    }
}
