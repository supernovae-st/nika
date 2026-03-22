//! Navigation logic for HomeView
//!
//! Handles selection movement (prev/next), folder toggling, and tree action dispatch.

use super::HomeView;
use crate::tui::views::ViewAction;
use crate::tui::widgets::tree::TreeAction;

impl HomeView {
    /// Move selection up
    pub fn select_prev(&mut self) {
        if let Some(selected) = self.tree_state.selection_index() {
            if selected > 0 {
                self.tree_state.select_prev_index();
                // Update preview based on filtered index
                if let Some(&idx) = self.filtered_indices.get(selected - 1) {
                    self.standalone.browser_index = idx;
                    self.standalone.update_preview();
                }
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if let Some(selected) = self.tree_state.selection_index() {
            let max = self.filtered_indices.len();
            if selected < max.saturating_sub(1) {
                self.tree_state.select_next_index(max);
                // Update preview based on filtered index
                if let Some(&idx) = self.filtered_indices.get(selected + 1) {
                    self.standalone.browser_index = idx;
                    self.standalone.update_preview();
                }
            }
        }
    }

    /// Toggle folder open/closed (for directory entries)
    ///
    /// Uses filtered_indices to map list selection to actual entry index.
    pub fn toggle_folder(&mut self) {
        if let Some(selected) = self.tree_state.selection_index() {
            // Map list selection index to actual browser_entries index via filtered_indices
            if let Some(&idx) = self.filtered_indices.get(selected) {
                if let Some(entry) = self.standalone.browser_entries.get_mut(idx) {
                    if entry.is_dir {
                        entry.expanded = !entry.expanded;
                    }
                }
            }
        }
    }

    /// Handle tree navigation action
    ///
    /// Maps TreeAction variants to HomeView operations.
    /// Returns Some(ViewAction) if action was handled, None otherwise.
    pub(crate) fn handle_tree_action(&mut self, action: TreeAction) -> Option<ViewAction> {
        let max = self.filtered_indices.len();

        match action {
            TreeAction::Up => {
                self.select_prev();
                Some(ViewAction::None)
            }
            TreeAction::Down => {
                self.select_next();
                Some(ViewAction::None)
            }
            TreeAction::Left => {
                // Collapse folder if expanded, otherwise move to parent
                if let Some(entry) = self.selected_entry() {
                    if entry.is_dir && entry.expanded {
                        self.toggle_folder();
                    } else if entry.depth > 0 {
                        // Move to parent: find previous entry with depth - 1
                        let target_depth = entry.depth - 1;
                        if let Some(selected) = self.tree_state.selection_index() {
                            for i in (0..selected).rev() {
                                if let Some(&idx) = self.filtered_indices.get(i) {
                                    if let Some(e) = self.standalone.browser_entries.get(idx) {
                                        if e.depth == target_depth {
                                            self.tree_state.set_selection_index(Some(i));
                                            self.standalone.browser_index = idx;
                                            self.standalone.update_preview();
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Some(ViewAction::None)
            }
            TreeAction::Right => {
                // Expand folder if collapsed, otherwise move into first child
                if let Some(entry) = self.selected_entry() {
                    if entry.is_dir && !entry.expanded {
                        self.toggle_folder();
                    } else if entry.is_dir {
                        // Move into first child: next entry with depth + 1
                        let target_depth = entry.depth + 1;
                        if let Some(selected) = self.tree_state.selection_index() {
                            let next = selected + 1;
                            if let Some(&idx) = self.filtered_indices.get(next) {
                                if let Some(e) = self.standalone.browser_entries.get(idx) {
                                    if e.depth == target_depth {
                                        self.tree_state.set_selection_index(Some(next));
                                        self.standalone.browser_index = idx;
                                        self.standalone.update_preview();
                                    }
                                }
                            }
                        }
                    }
                }
                Some(ViewAction::None)
            }
            TreeAction::Toggle => {
                if let Some(entry) = self.selected_entry() {
                    if entry.is_dir {
                        self.toggle_folder();
                        Some(ViewAction::None)
                    } else {
                        Some(ViewAction::RunWorkflow(entry.path.clone()))
                    }
                } else {
                    Some(ViewAction::None)
                }
            }
            TreeAction::Select => {
                // 'o' to open in Studio
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return Some(ViewAction::OpenInStudio(entry.path.clone()));
                    }
                }
                Some(ViewAction::None)
            }
            TreeAction::First => {
                // Jump to first entry
                if max > 0 {
                    self.tree_state.set_selection_index(Some(0));
                    if let Some(&idx) = self.filtered_indices.first() {
                        self.standalone.browser_index = idx;
                        self.standalone.update_preview();
                    }
                }
                Some(ViewAction::None)
            }
            TreeAction::Last => {
                // Jump to last entry
                if max > 0 {
                    self.tree_state.set_selection_index(Some(max - 1));
                    if let Some(&idx) = self.filtered_indices.last() {
                        self.standalone.browser_index = idx;
                        self.standalone.update_preview();
                    }
                }
                Some(ViewAction::None)
            }
            TreeAction::PageUp => {
                // Move up 10 items
                if let Some(selected) = self.tree_state.selection_index() {
                    let new_idx = selected.saturating_sub(10);
                    self.tree_state.set_selection_index(Some(new_idx));
                    if let Some(&idx) = self.filtered_indices.get(new_idx) {
                        self.standalone.browser_index = idx;
                        self.standalone.update_preview();
                    }
                }
                Some(ViewAction::None)
            }
            TreeAction::PageDown => {
                // Move down 10 items
                if let Some(selected) = self.tree_state.selection_index() {
                    let new_idx = (selected + 10).min(max.saturating_sub(1));
                    self.tree_state.set_selection_index(Some(new_idx));
                    if let Some(&idx) = self.filtered_indices.get(new_idx) {
                        self.standalone.browser_index = idx;
                        self.standalone.update_preview();
                    }
                }
                Some(ViewAction::None)
            }
        }
    }
}
