//! Provider modal navigation logic
//!
//! Grid and list navigation, tab switching, model expand/collapse.

use super::types::ProviderModalTab;
use super::ProviderModalState;

impl ProviderModalState {
    /// Switch to a different tab
    pub fn switch_tab(&mut self, tab: ProviderModalTab) {
        self.active_tab = tab;
        self.selected_idx = 0; // Reset selection on tab change
                               // Update item_count based on tab
        self.item_count = match tab {
            ProviderModalTab::Cloud => 7, // 7 cloud providers
            ProviderModalTab::Native => self.native_models.len().max(1), // Dynamic
            ProviderModalTab::Keys => 7,  // 7 API key entries (cloud providers)
            ProviderModalTab::Config => 6, // 6 config entries (matches ConfigTab::new())
        };
    }

    /// Navigate up in current list/grid (wraps around)
    /// For Cloud tab (2x3 grid, 6 items): moves up one row (idx - 3), wraps to last row
    /// For other tabs: moves up one item (idx - 1), wraps to last
    pub fn navigate_up(&mut self) {
        if self.item_count == 0 {
            return;
        }
        if self.active_tab == ProviderModalTab::Cloud {
            // 3-column grid (6 items): move up one row, wrap to last row
            if self.selected_idx >= 3 {
                self.selected_idx -= 3;
            } else {
                // Wrap to last row with this column (2x3 grid: idx 0-2 wraps to 3-5)
                let col = self.selected_idx;
                let last_row_idx = col + 3;
                self.selected_idx = last_row_idx.min(self.item_count - 1);
            }
        } else {
            // List navigation: wrap to last
            if self.selected_idx == 0 {
                self.selected_idx = self.item_count - 1;
            } else {
                self.selected_idx -= 1;
            }
        }
    }

    /// Navigate down in current list/grid (wraps around)
    /// For Cloud tab (2x3 grid, 6 items): moves down one row (idx + 3), wraps to top
    /// For other tabs: moves down one item (idx + 1), wraps to first
    pub fn navigate_down(&mut self) {
        if self.item_count == 0 {
            return;
        }
        if self.active_tab == ProviderModalTab::Cloud {
            // 3-column grid (6 items): move down one row, wrap to top
            if self.selected_idx + 3 < self.item_count {
                self.selected_idx += 3;
            } else {
                // Wrap to top row, same column
                let col = self.selected_idx % 3;
                self.selected_idx = col;
            }
        } else {
            // List navigation: wrap to first
            if self.selected_idx >= self.item_count - 1 {
                self.selected_idx = 0;
            } else {
                self.selected_idx += 1;
            }
        }
    }

    /// Navigate left in grid (Cloud tab only, wraps within row)
    pub fn navigate_left(&mut self) {
        if self.active_tab == ProviderModalTab::Cloud {
            if self.selected_idx % 3 != 0 {
                self.selected_idx -= 1;
            } else {
                // Wrap to end of row
                let row_end = (self.selected_idx + 2).min(self.item_count - 1);
                self.selected_idx = row_end;
            }
        }
    }

    /// Navigate right in grid (Cloud tab only, wraps within row)
    pub fn navigate_right(&mut self) {
        if self.active_tab == ProviderModalTab::Cloud && self.item_count > 0 {
            if self.selected_idx % 3 != 2 && self.selected_idx < self.item_count - 1 {
                self.selected_idx += 1;
            } else {
                // Wrap to start of row
                let row_start = (self.selected_idx / 3) * 3;
                self.selected_idx = row_start;
            }
        }
    }

    /// Go to next tab
    pub fn next_tab(&mut self) {
        self.switch_tab(self.active_tab.next());
    }

    /// Go to previous tab
    pub fn prev_tab(&mut self) {
        self.switch_tab(self.active_tab.prev());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Model Expand/Collapse Methods
    // ═══════════════════════════════════════════════════════════════════════════

    /// Check if a provider is currently expanded
    pub fn is_provider_expanded(&self) -> bool {
        self.expanded_provider_idx.is_some()
    }

    /// Check if a specific provider is expanded
    pub fn is_provider_idx_expanded(&self, idx: usize) -> bool {
        self.expanded_provider_idx == Some(idx)
    }

    /// Expand the currently selected provider (show model list)
    pub fn expand_selected_provider(&mut self) {
        if self.active_tab == ProviderModalTab::Cloud {
            self.expanded_provider_idx = Some(self.selected_idx);
            self.model_selection_idx = 0;
        }
    }

    /// Collapse the expanded provider
    pub fn collapse_provider(&mut self) {
        self.expanded_provider_idx = None;
        self.model_selection_idx = 0;
    }

    /// Toggle expand/collapse for selected provider
    pub fn toggle_provider_expand(&mut self) {
        if self.is_provider_idx_expanded(self.selected_idx) {
            self.collapse_provider();
        } else {
            self.expand_selected_provider();
        }
    }

    /// Navigate up in model list (when expanded)
    pub fn model_navigate_up(&mut self, model_count: usize) {
        if self.expanded_provider_idx.is_some() && model_count > 0 {
            if self.model_selection_idx == 0 {
                self.model_selection_idx = model_count - 1;
            } else {
                self.model_selection_idx -= 1;
            }
        }
    }

    /// Navigate down in model list (when expanded)
    pub fn model_navigate_down(&mut self, model_count: usize) {
        if self.expanded_provider_idx.is_some() && model_count > 0 {
            if self.model_selection_idx >= model_count - 1 {
                self.model_selection_idx = 0;
            } else {
                self.model_selection_idx += 1;
            }
        }
    }
}
