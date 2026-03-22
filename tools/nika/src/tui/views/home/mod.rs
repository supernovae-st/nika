//! Home View - Workflow browser with file tree and DAG preview
//!
//! Layout:
//! ```text
//! +-----------------------------------+---------------------------------------------+
//! | SEARCH: [fuzzy search bar]        | (when active)                               |
//! +-----------------------------------+---------------------------------------------+
//! | FILES (40%)                       | DAG PREVIEW (60%)                           |
//! | Tree view of .nika.yaml files     | Visual task dependency graph                |
//! +-----------------------------------+---------------------------------------------+
//! | HISTORY: recent workflow runs (toggleable with [h])                             |
//! +---------------------------------------------------------------------------------+
//! ```

mod keys;
mod navigation;
mod preview;
mod render;

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use nucleo::{Config, Matcher, Utf32Str};
use ratatui::widgets::ListState;

use crate::ast::Workflow;
use crate::tui::standalone::{BrowserEntry, StandaloneState};
use crate::tui::widgets::tree::TreeState;

/// Preview mode for DAG preview panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMode {
    /// DAG visualization (default)
    #[default]
    Dag,
    /// Raw YAML text with verb-colored syntax highlighting
    Yaml,
}

/// Home view state
pub struct HomeView {
    /// File browser state (reuses StandaloneState from standalone.rs)
    pub standalone: StandaloneState,
    /// List state for file selection (ratatui ListState)
    pub list_state: ListState,
    /// Whether history bar is expanded
    pub history_expanded: bool,
    /// Fuzzy search query
    pub search_query: String,
    /// Whether search mode is active
    pub search_active: bool,
    /// Filtered indices from fuzzy search
    pub(crate) filtered_indices: Vec<usize>,
    /// Fuzzy matcher instance
    matcher: Matcher,
    /// Cached: whether .nika directory exists (avoid syscall per frame)
    pub(crate) has_nika_dir: bool,
    /// Cached parsed workflow (PERF: avoid re-parsing YAML every frame)
    pub(crate) cached_workflow: RefCell<Option<Workflow>>,
    /// Content hash for cache invalidation
    pub(crate) cached_content_hash: Cell<u64>,
    /// DAG expanded mode toggle
    pub dag_expanded: bool,
    /// Preview mode: DAG visualization or verb-colored YAML
    pub preview_mode: PreviewMode,
    /// Animation frame counter (0-255, wraps)
    pub frame: u8,
    /// Tree state for selection and expansion
    pub tree_state: TreeState,
    /// Matrix rain background opacity (0.0 = invisible, 1.0 = full)
    pub rain_opacity: f32,
    /// Whether rain effect is actively fading out
    pub rain_fading: bool,
    /// Whether matrix effect is enabled
    pub matrix_effect_enabled: bool,
}

impl HomeView {
    /// Create a new HomeView for the given root directory
    pub fn new(root: PathBuf) -> Self {
        // Cache filesystem check ONCE at creation time (not per frame!)
        let has_nika_dir = root.join(".nika").exists();
        let standalone = StandaloneState::new(root);
        let mut list_state = ListState::default();
        let entry_count = standalone.browser_entries.len();
        if !standalone.browser_entries.is_empty() {
            list_state.select(Some(0));
        }
        // Initialize tree state with visible nodes count
        let tree_state = TreeState::new_with_count(entry_count);

        Self {
            standalone,
            list_state,
            history_expanded: false,
            search_query: String::new(),
            search_active: false,
            filtered_indices: (0..entry_count).collect(),
            matcher: Matcher::new(Config::DEFAULT),
            has_nika_dir,
            cached_workflow: RefCell::new(None),
            cached_content_hash: Cell::new(0),
            dag_expanded: false,
            preview_mode: PreviewMode::Dag,
            frame: 0,
            tree_state,
            // Matrix Rain starts visible and fades
            rain_opacity: 1.0,
            rain_fading: true,
            matrix_effect_enabled: true,
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

    /// Update filtered indices based on search query
    pub(crate) fn update_filter(&mut self) {
        if self.search_query.is_empty() {
            // No filter - show all entries
            self.filtered_indices = (0..self.standalone.browser_entries.len()).collect();
        } else {
            // Fuzzy match on file names
            let mut scored: Vec<(usize, u16)> = Vec::new();
            let mut query_buf = Vec::new();
            let query = Utf32Str::new(&self.search_query, &mut query_buf);

            for (i, entry) in self.standalone.browser_entries.iter().enumerate() {
                let name = entry
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let mut haystack_buf = Vec::new();
                let haystack = Utf32Str::new(&name, &mut haystack_buf);

                if let Some(score) = self.matcher.fuzzy_match(haystack, query) {
                    scored.push((i, score));
                }
            }

            // Sort by score (highest first)
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered_indices = scored.into_iter().map(|(i, _)| i).collect();
        }

        // Reset selection
        if !self.filtered_indices.is_empty() {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    /// Get filtered entries for display
    pub(crate) fn filtered_entries(&self) -> Vec<&BrowserEntry> {
        self.filtered_indices
            .iter()
            .filter_map(|&i| self.standalone.browser_entries.get(i))
            .collect()
    }

    /// Get currently selected entry (respects filter)
    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.tree_state.selection_index().and_then(|i| {
            self.filtered_indices
                .get(i)
                .and_then(|&idx| self.standalone.browser_entries.get(idx))
        })
    }
}
