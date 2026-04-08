// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Standalone TUI state: browser, history, preview, and validation.

use ignore::WalkBuilder;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use nika_engine::ast::parse_workflow;
use nika_engine::util::{atomic_write, check_preview_size, format_size};

use super::browser::BrowserEntry;
use super::history::HistoryEntry;
use super::panel::StandalonePanel;

/// State for standalone TUI mode
#[derive(Debug)]
pub struct StandaloneState {
    /// Project root directory
    pub root: PathBuf,
    /// Current focused panel
    pub focused_panel: StandalonePanel,
    /// File browser entries
    pub browser_entries: Vec<BrowserEntry>,
    /// Selected index in browser
    pub browser_index: usize,
    /// Execution history
    pub history: VecDeque<HistoryEntry>,
    /// Selected index in history
    pub history_index: usize,
    /// Preview content (YAML of selected file)
    pub preview_content: String,
    /// Scroll offset in preview
    pub preview_scroll: usize,
    /// Search query for filtering
    pub search_query: String,
    /// Is search active?
    pub search_active: bool,
}

impl StandaloneState {
    /// Create new standalone state from project root
    pub fn new(root: PathBuf) -> Self {
        let mut state = Self {
            root: root.clone(),
            focused_panel: StandalonePanel::Browser,
            browser_entries: Vec::new(),
            browser_index: 0,
            history: VecDeque::new(),
            history_index: 0,
            preview_content: String::new(),
            preview_scroll: 0,
            search_query: String::new(),
            search_active: false,
        };
        state.scan_workflows();
        state.load_history();
        state.update_preview();
        state
    }

    /// Scan for .nika.yaml files in the project
    /// Refresh the browser entries by rescanning directories
    ///
    /// Alias for `scan_workflows()` - used by the file watcher
    pub fn refresh_entries(&mut self) {
        self.scan_workflows();
    }

    /// Scan for .nika.yaml workflow files using the `ignore` crate.
    ///
    /// Uses WalkBuilder for:
    /// - .gitignore support (automatically skips ignored files)
    /// - .ignore file support
    /// - Efficient traversal (from ripgrep author)
    /// - Automatic hidden file/dir skipping
    pub fn scan_workflows(&mut self) {
        self.browser_entries.clear();

        // Common locations to scan
        let scan_dirs = ["examples", "workflows", ".", "tests"];

        for dir in scan_dirs {
            let dir_path = self.root.join(dir);
            if dir_path.exists() && dir_path.is_dir() {
                self.scan_directory_with_ignore(&dir_path);
            }
        }

        // Sort by path
        self.browser_entries
            .sort_by(|a, b| a.display_name.cmp(&b.display_name));

        // Clamp browser_index after rescan (entries may have shrunk)
        if self.browser_entries.is_empty() {
            self.browser_index = 0;
        } else {
            self.browser_index = self.browser_index.min(self.browser_entries.len() - 1);
        }
    }

    /// Maximum browser entries to prevent OOM on large directories
    const MAX_BROWSER_ENTRIES: usize = 500;

    /// Scan directory using `ignore` crate for .gitignore-aware traversal.
    ///
    /// Uses WalkBuilder which provides:
    /// - .gitignore support
    /// - .ignore file support
    /// - Automatic hidden file filtering
    /// - Efficient parallel traversal capability
    fn scan_directory_with_ignore(&mut self, dir: &Path) {
        // WalkBuilder respects .gitignore, .ignore, and hidden files
        let walker = WalkBuilder::new(dir)
            .git_ignore(true) // Respect .gitignore
            .git_exclude(true) // Respect .git/info/exclude
            .ignore(true) // Respect .ignore files
            .hidden(true) // Skip hidden files/dirs
            .parents(true) // Check parent directories for ignore files
            .max_depth(Some(4)) // Limit depth to 4 levels
            .follow_links(false) // Don't follow symlinks (security)
            .build();

        for entry in walker.flatten() {
            // Safety: stop scanning if we've hit the limit
            if self.browser_entries.len() >= Self::MAX_BROWSER_ENTRIES {
                tracing::warn!(
                    "Browser entry limit reached ({}), stopping scan",
                    Self::MAX_BROWSER_ENTRIES
                );
                break;
            }
            let path = entry.path();

            // Only include .nika.yaml files
            if path.is_file() {
                if let Some(name) = path.file_name() {
                    let name_str = name.to_string_lossy();
                    if name_str.ends_with(".nika.yaml") {
                        // Calculate depth from root
                        let depth = path
                            .strip_prefix(&self.root)
                            .map(|p| p.components().count().saturating_sub(1))
                            .unwrap_or(0);

                        self.browser_entries.push(
                            BrowserEntry::new(path.to_path_buf(), &self.root).with_depth(depth),
                        );
                    }
                }
            }
        }
    }

    /// Load history from ~/.nika/history.json
    pub fn load_history(&mut self) {
        let history_path = dirs::home_dir()
            .map(|h| h.join(".nika").join("history.json"))
            .unwrap_or_else(|| PathBuf::from(".nika/history.json"));

        if let Ok(content) = std::fs::read_to_string(&history_path) {
            if let Ok(history) = serde_json::from_str::<VecDeque<HistoryEntry>>(&content) {
                self.history = history;
            }
        }
    }

    /// Save history to ~/.nika/history.json
    /// Uses atomic write (temp+rename) for data integrity.
    pub fn save_history(&self) {
        let history_path = dirs::home_dir()
            .map(|h| h.join(".nika").join("history.json"))
            .unwrap_or_else(|| PathBuf::from(".nika/history.json"));

        if let Ok(content) = serde_json::to_string_pretty(&self.history) {
            if let Err(e) = atomic_write(&history_path, content.as_bytes()) {
                tracing::warn!(path = %history_path.display(), "Failed to save history: {}", e);
            }
        }
    }

    /// Add entry to history
    pub fn add_history(&mut self, entry: HistoryEntry) {
        // Keep last 50 entries
        if self.history.len() >= 50 {
            self.history.pop_front();
        }
        self.history.push_back(entry);
        self.save_history();
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.save_history();
    }

    /// Update preview content based on selected file
    /// Checks file size before reading to prevent memory issues.
    pub fn update_preview(&mut self) {
        if let Some(entry) = self.browser_entries.get(self.browser_index) {
            if !entry.is_dir {
                // Check file size before reading
                if let Err(size) = check_preview_size(&entry.path) {
                    self.preview_content = format!(
                        "File too large for preview ({})\n\nPress Enter to open in editor",
                        format_size(size)
                    );
                    self.preview_scroll = 0;
                    return;
                }

                match std::fs::read_to_string(&entry.path) {
                    Ok(content) => {
                        self.preview_content = content;
                        self.preview_scroll = 0;
                    }
                    Err(e) => {
                        self.preview_content = format!("Error reading file: {}", e);
                    }
                }
            }
        } else {
            self.preview_content = "No workflow selected".to_string();
        }
    }

    /// Get currently selected workflow path
    pub fn selected_workflow(&self) -> Option<&Path> {
        self.browser_entries
            .get(self.browser_index)
            .filter(|e| !e.is_dir)
            .map(|e| e.path.as_path())
    }

    /// Navigate up in browser
    pub fn browser_up(&mut self) {
        if self.browser_index > 0 {
            self.browser_index -= 1;
            self.update_preview();
        }
    }

    /// Navigate down in browser
    pub fn browser_down(&mut self) {
        if self.browser_index < self.browser_entries.len().saturating_sub(1) {
            self.browser_index += 1;
            self.update_preview();
        }
    }

    /// Navigate up in history
    pub fn history_up(&mut self) {
        if self.history_index > 0 {
            self.history_index -= 1;
        }
    }

    /// Navigate down in history
    pub fn history_down(&mut self) {
        if self.history_index < self.history.len().saturating_sub(1) {
            self.history_index += 1;
        }
    }

    /// Scroll preview up
    pub fn preview_up(&mut self) {
        if self.preview_scroll > 0 {
            self.preview_scroll -= 1;
        }
    }

    /// Scroll preview down
    pub fn preview_down(&mut self) {
        let lines = self.preview_content.lines().count();
        if self.preview_scroll < lines.saturating_sub(10) {
            self.preview_scroll += 1;
        }
    }

    /// Validate selected workflow and show result in preview
    ///
    /// Performs full validation: schema, parsing, DAG, and bindings.
    pub fn validate_selected(&mut self) {
        use nika_engine::ast::schema_validator::WorkflowSchemaValidator;
        use nika_engine::dag::{validate_bindings, Dag};

        let Some(entry) = self.browser_entries.get(self.browser_index) else {
            self.preview_content = "No workflow selected".to_string();
            return;
        };

        if entry.is_dir {
            self.preview_content = "Cannot validate directory".to_string();
            return;
        }

        let mut result = String::new();
        result.push_str(&format!("╭─ Validating: {}\n", entry.display_name));
        result.push_str("│\n");

        // Step 1: Read file
        let yaml = match std::fs::read_to_string(&entry.path) {
            Ok(content) => content,
            Err(e) => {
                result.push_str(&format!("│ ✗ Failed to read file: {}\n", e));
                result.push_str("╰─\n");
                self.preview_content = result;
                return;
            }
        };
        result.push_str("│ ✓ File read successfully\n");

        // Step 2: JSON Schema validation
        match WorkflowSchemaValidator::new() {
            Ok(validator) => match validator.validate_yaml(&yaml) {
                Ok(()) => result.push_str("│ ✓ JSON Schema validation passed\n"),
                Err(e) => {
                    result.push_str(&format!("│ ✗ Schema validation failed: {}\n", e));
                    result.push_str("╰─\n");
                    self.preview_content = result;
                    return;
                }
            },
            Err(_) => {
                result.push_str("│ ⚠ Schema validator unavailable\n");
            }
        }

        // Step 3: Parse and validate workflow (two-phase IR pipeline)
        let workflow = match parse_workflow(&yaml) {
            Ok(w) => {
                result.push_str("│ ✓ YAML parsing + validation passed\n");
                w
            }
            Err(e) => {
                result.push_str(&format!("│ ✗ Workflow parsing failed: {}\n", e));
                result.push_str("╰─\n");
                self.preview_content = result;
                return;
            }
        };

        // Step 4: Build and validate DAG
        let flow_graph = match Dag::from_workflow(&workflow) {
            Ok(dag) => dag,
            Err(e) => {
                result.push_str(&format!("│ ✗ DAG construction failed: {}\n", e));
                result.push_str("╰─\n");
                self.preview_content = result;
                return;
            }
        };
        if let Err(e) = validate_bindings(&workflow, &flow_graph) {
            result.push_str(&format!("│ ✗ Binding validation failed: {}\n", e));
            result.push_str("╰─\n");
            self.preview_content = result;
            return;
        }
        result.push_str("│ ✓ DAG and binding validation passed\n");

        // Summary
        result.push_str("│\n");
        result.push_str("├─ Summary ─────────────────────\n");
        let provider_display = workflow.provider.as_str();
        result.push_str(&format!("│ • Provider: {}\n", provider_display));
        result.push_str(&format!(
            "│ • Model: {}\n",
            workflow.model.as_deref().unwrap_or("(default)")
        ));
        result.push_str(&format!("│ • Tasks: {}\n", workflow.tasks.len()));
        result.push_str(&format!("│ • Edges: {}\n", workflow.flow_count()));
        result.push_str("│\n");
        result.push_str("╰─ ✓ Workflow is valid\n");

        self.preview_content = result;
        self.preview_scroll = 0;
    }

    /// Filter browser entries by search query
    pub fn filtered_entries(&self) -> Vec<&BrowserEntry> {
        if self.search_query.is_empty() {
            self.browser_entries.iter().collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.browser_entries
                .iter()
                .filter(|e| e.display_name.to_lowercase().contains(&query))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn test_standalone_state_new() {
        let root = PathBuf::from("/tmp/nika_test");
        let state = StandaloneState::new(root.clone());

        assert_eq!(state.root, root);
        assert_eq!(state.focused_panel, StandalonePanel::Browser);
        assert_eq!(state.browser_index, 0);
        assert_eq!(state.history_index, 0);
        assert!(!state.search_active);
        assert_eq!(state.search_query, "");
        assert_eq!(state.preview_scroll, 0);
    }

    #[test]
    fn test_standalone_state_browser_up_at_beginning() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.browser_index = 0;

        state.browser_up();

        // Should remain at 0 (can't go negative)
        assert_eq!(state.browser_index, 0);
    }

    #[test]
    fn test_standalone_state_browser_up_decrements() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.browser_index = 5;

        state.browser_up();

        assert_eq!(state.browser_index, 4);
    }

    #[test]
    fn test_standalone_state_browser_down_at_end() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries = vec![
            BrowserEntry::new(PathBuf::from("/tmp/test1.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test2.nika.yaml"), &root_clone),
        ];
        state.browser_index = 1; // At end

        state.browser_down();

        // Should remain at 1 (can't exceed bounds)
        assert_eq!(state.browser_index, 1);
    }

    #[test]
    fn test_standalone_state_browser_down_increments() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries = vec![
            BrowserEntry::new(PathBuf::from("/tmp/test1.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test2.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test3.nika.yaml"), &root_clone),
        ];
        state.browser_index = 0;

        state.browser_down();

        assert_eq!(state.browser_index, 1);
    }

    #[test]
    fn test_standalone_state_history_up_at_beginning() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.history_index = 0;

        state.history_up();

        assert_eq!(state.history_index, 0);
    }

    #[test]
    fn test_standalone_state_history_up_decrements() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.history = VecDeque::from(vec![
            HistoryEntry::new(
                PathBuf::from("test1.nika.yaml"),
                Duration::from_secs(1),
                1,
                true,
                "summary1".to_string(),
            ),
            HistoryEntry::new(
                PathBuf::from("test2.nika.yaml"),
                Duration::from_secs(2),
                2,
                true,
                "summary2".to_string(),
            ),
        ]);
        state.history_index = 1;

        state.history_up();

        assert_eq!(state.history_index, 0);
    }

    #[test]
    fn test_standalone_state_history_down_at_end() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.history = VecDeque::from(vec![HistoryEntry::new(
            PathBuf::from("test1.nika.yaml"),
            Duration::from_secs(1),
            1,
            true,
            "summary1".to_string(),
        )]);
        state.history_index = 0;

        state.history_down();

        // Should remain at 0 (only one entry)
        assert_eq!(state.history_index, 0);
    }

    #[test]
    fn test_standalone_state_history_down_increments() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.history = VecDeque::from(vec![
            HistoryEntry::new(
                PathBuf::from("test1.nika.yaml"),
                Duration::from_secs(1),
                1,
                true,
                "summary1".to_string(),
            ),
            HistoryEntry::new(
                PathBuf::from("test2.nika.yaml"),
                Duration::from_secs(2),
                2,
                true,
                "summary2".to_string(),
            ),
        ]);
        state.history_index = 0;

        state.history_down();

        assert_eq!(state.history_index, 1);
    }

    #[test]
    fn test_standalone_state_preview_up_at_beginning() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.preview_scroll = 0;

        state.preview_up();

        assert_eq!(state.preview_scroll, 0);
    }

    #[test]
    fn test_standalone_state_preview_up_decrements() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.preview_scroll = 5;

        state.preview_up();

        assert_eq!(state.preview_scroll, 4);
    }

    #[test]
    fn test_standalone_state_preview_down_limited_by_lines() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.preview_content = "line1\nline2\nline3".to_string();
        state.preview_scroll = 0;

        // Preview shows 10 lines, so max scroll is (3 - 10) = 0
        for _ in 0..10 {
            state.preview_down();
        }

        assert_eq!(state.preview_scroll, 0);
    }

    #[test]
    fn test_standalone_state_preview_down_increments() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        // Create content with many lines
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("line {}\n", i));
        }
        state.preview_content = content;
        state.preview_scroll = 0;

        state.preview_down();

        assert_eq!(state.preview_scroll, 1);
    }

    #[test]
    fn test_standalone_state_selected_workflow_when_empty() {
        // Use unique temp directory to avoid picking up leftover files
        let temp_dir = tempdir().unwrap();
        let mut state = StandaloneState::new(temp_dir.path().to_path_buf());
        // Clear any entries that might have been found
        state.browser_entries.clear();

        assert_eq!(state.selected_workflow(), None);
    }

    #[test]
    fn test_standalone_state_selected_workflow_on_directory() {
        // Use unique temp directory to avoid picking up leftover files
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        // Clear entries and add only our test directory entry
        state.browser_entries.clear();
        let mut dir_entry = BrowserEntry::new(PathBuf::from("/tmp/examples"), &root_clone);
        dir_entry.is_dir = true;
        state.browser_entries.push(dir_entry);
        state.browser_index = 0;

        assert_eq!(state.selected_workflow(), None);
    }

    #[test]
    fn test_standalone_state_selected_workflow_on_file() {
        // Use unique temp directory to avoid picking up leftover files
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        // Clear entries and add only our test file entry
        state.browser_entries.clear();
        let path = PathBuf::from("/tmp/test.nika.yaml");
        state
            .browser_entries
            .push(BrowserEntry::new(path.clone(), &root_clone));
        state.browser_index = 0;

        assert_eq!(state.selected_workflow(), Some(path.as_path()));
    }

    #[test]
    fn test_standalone_state_add_history_below_limit() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        // Clear initial history from load_history()
        state.history.clear();

        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(1),
            1,
            true,
            "summary".to_string(),
        );
        state.add_history(entry);

        assert_eq!(state.history.len(), 1);
    }

    #[test]
    fn test_standalone_state_add_history_respects_limit() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        // Clear initial history from load_history()
        state.history.clear();

        // Add 51 entries (limit is 50)
        for i in 0..51 {
            let entry = HistoryEntry::new(
                PathBuf::from(format!("test{}.nika.yaml", i)),
                Duration::from_secs(1),
                1,
                true,
                format!("summary{}", i),
            );
            state.add_history(entry);
        }

        assert_eq!(state.history.len(), 50);
        // First entry should have been removed
        assert_eq!(state.history[0].summary, "summary1");
    }

    #[test]
    fn test_standalone_state_clear_history() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        // Clear initial history from load_history()
        state.history.clear();

        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(1),
            1,
            true,
            "summary".to_string(),
        );
        state.add_history(entry);
        assert_eq!(state.history.len(), 1);

        state.clear_history();

        assert_eq!(state.history.len(), 0);
    }

    #[test]
    fn test_standalone_state_refresh_entries() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries.push(BrowserEntry::new(
            PathBuf::from("/tmp/test.nika.yaml"),
            &root_clone,
        ));

        state.refresh_entries();

        // After refresh, entries might be different depending on filesystem
        // Test passes if refresh_entries() doesn't panic
    }

    #[test]
    fn test_standalone_state_update_preview_no_entries() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.browser_entries.clear();

        state.update_preview();

        assert_eq!(state.preview_content, "No workflow selected");
    }

    #[test]
    fn test_standalone_state_update_preview_directory() {
        // Use unique temp directory to avoid picking up leftover files
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        // Clear entries and add only our test directory entry
        state.browser_entries.clear();
        let mut entry = BrowserEntry::new(PathBuf::from("/tmp/examples"), &root_clone);
        entry.is_dir = true;
        state.browser_entries.push(entry);
        state.browser_index = 0;

        // Set preview to something before calling update_preview
        state.preview_content = "existing content".to_string();

        state.update_preview();

        // For directories, preview should not be updated (stays as is)
        assert_eq!(state.preview_content, "existing content");
    }

    #[test]
    fn test_standalone_state_filtered_entries_empty_query() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries = vec![
            BrowserEntry::new(PathBuf::from("/tmp/test1.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test2.nika.yaml"), &root_clone),
        ];
        state.search_query = String::new();

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_standalone_state_filtered_entries_with_query() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries = vec![
            BrowserEntry::new(PathBuf::from("/tmp/generate.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test.nika.yaml"), &root_clone),
            BrowserEntry::new(
                PathBuf::from("/tmp/generate_content.nika.yaml"),
                &root_clone,
            ),
        ];
        state.search_query = "generate".to_string();

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 2);
        assert!(filtered[0].display_name.contains("generate"));
        assert!(filtered[1].display_name.contains("generate"));
    }

    #[test]
    fn test_standalone_state_filtered_entries_case_insensitive() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries = vec![
            BrowserEntry::new(PathBuf::from("/tmp/GENERATE.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test.nika.yaml"), &root_clone),
        ];
        state.search_query = "generate".to_string();

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_standalone_state_filtered_entries_no_match() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        state.browser_entries = vec![
            BrowserEntry::new(PathBuf::from("/tmp/test1.nika.yaml"), &root_clone),
            BrowserEntry::new(PathBuf::from("/tmp/test2.nika.yaml"), &root_clone),
        ];
        state.search_query = "nonexistent".to_string();

        let filtered = state.filtered_entries();

        assert_eq!(filtered.len(), 0);
    }

    #[test]
    fn test_standalone_state_focused_panel_navigation() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        assert_eq!(state.focused_panel, StandalonePanel::Browser);

        state.focused_panel = state.focused_panel.next();
        assert_eq!(state.focused_panel, StandalonePanel::History);

        state.focused_panel = state.focused_panel.next();
        assert_eq!(state.focused_panel, StandalonePanel::Preview);

        state.focused_panel = state.focused_panel.next();
        assert_eq!(state.focused_panel, StandalonePanel::Browser);
    }
}
