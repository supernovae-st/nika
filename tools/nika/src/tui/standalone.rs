//! TUI Standalone Mode
//!
//! File browser, history, and workflow preview for standalone TUI operation.
//!
//! # Layout
//!
//! ```text
//! ┌─────────────────────────────┬─────────────────────────────────────────┐
//! │ [1] WORKFLOW BROWSER        │ [2] HISTORY                             │
//! │ ─────────────────────────   │ ─────────────────────                   │
//! │ examples/                   │ 2026-02-20 01:02:03                     │
//! │ ├── workflow1.nika.yaml     │ ├── workflow.nika.yaml ✓                │
//! │ ├── workflow2.nika.yaml     │ └── 2.7s | 3 tasks                      │
//! │ └── ...                     │                                         │
//! ├─────────────────────────────┴─────────────────────────────────────────┤
//! │ [3] PREVIEW                                                           │
//! │ ─────────────────────────────────────────────────────────             │
//! │ schema: nika/workflow@0.5                                             │
//! │ tasks: ...                                                            │
//! └───────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Crates Used
//!
//! - `ignore`: .gitignore-aware directory traversal (from ripgrep author)
//! - `camino`: UTF-8 safe paths

use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// Panel in standalone mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StandalonePanel {
    #[default]
    Browser,
    History,
    Preview,
}

impl StandalonePanel {
    pub fn next(&self) -> Self {
        match self {
            StandalonePanel::Browser => StandalonePanel::History,
            StandalonePanel::History => StandalonePanel::Preview,
            StandalonePanel::Preview => StandalonePanel::Browser,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            StandalonePanel::Browser => StandalonePanel::Preview,
            StandalonePanel::History => StandalonePanel::Browser,
            StandalonePanel::Preview => StandalonePanel::History,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            StandalonePanel::Browser => "WORKFLOW BROWSER",
            StandalonePanel::History => "HISTORY",
            StandalonePanel::Preview => "PREVIEW",
        }
    }

    pub fn number(&self) -> u8 {
        match self {
            StandalonePanel::Browser => 1,
            StandalonePanel::History => 2,
            StandalonePanel::Preview => 3,
        }
    }
}

/// Entry in the file browser
#[derive(Debug, Clone)]
pub struct BrowserEntry {
    /// Full path to the file
    pub path: PathBuf,
    /// Display name (relative to project root)
    pub display_name: String,
    /// Is this a directory?
    pub is_dir: bool,
    /// Is this expanded? (for directories)
    pub expanded: bool,
    /// Indentation level
    pub depth: usize,
}

impl BrowserEntry {
    pub fn new(path: PathBuf, root: &Path) -> Self {
        let display_name = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        let is_dir = path.is_dir();
        Self {
            path,
            display_name,
            is_dir,
            expanded: false,
            depth: 0,
        }
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }
}

/// Execution history entry (persisted to ~/.nika/history.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Path to the workflow file
    pub workflow_path: PathBuf,
    /// Timestamp of execution
    pub timestamp: SystemTime,
    /// Duration of execution
    pub duration_ms: u64,
    /// Number of tasks
    pub task_count: usize,
    /// Success or failure
    pub success: bool,
    /// Brief summary (first task output or error)
    pub summary: String,
}

impl HistoryEntry {
    pub fn new(
        workflow_path: PathBuf,
        duration: Duration,
        task_count: usize,
        success: bool,
        summary: String,
    ) -> Self {
        Self {
            workflow_path,
            timestamp: SystemTime::now(),
            duration_ms: duration.as_millis() as u64,
            task_count,
            success,
            summary,
        }
    }

    /// Format duration for display
    pub fn duration_display(&self) -> String {
        let secs = self.duration_ms as f64 / 1000.0;
        if secs < 60.0 {
            format!("{:.1}s", secs)
        } else {
            let mins = secs / 60.0;
            format!("{:.1}m", mins)
        }
    }

    /// Format timestamp for display
    pub fn timestamp_display(&self) -> String {
        use std::time::UNIX_EPOCH;
        let duration = self
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs();

        // Simple formatting: YYYY-MM-DD HH:MM:SS
        let days_since_epoch = secs / 86400;
        let secs_today = secs % 86400;
        let hours = secs_today / 3600;
        let mins = (secs_today % 3600) / 60;
        let secs = secs_today % 60;

        // Approximate date calculation (not accounting for leap years precisely)
        let year = 1970 + (days_since_epoch / 365);
        let day_of_year = days_since_epoch % 365;
        let month = (day_of_year / 30) + 1;
        let day = (day_of_year % 30) + 1;

        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            year, month, day, hours, mins, secs
        )
    }
}

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
    pub history: Vec<HistoryEntry>,
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
            history: Vec::new(),
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
    }

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
            if let Ok(history) = serde_json::from_str::<Vec<HistoryEntry>>(&content) {
                self.history = history;
            }
        }
    }

    /// Save history to ~/.nika/history.json
    pub fn save_history(&self) {
        let history_dir = dirs::home_dir()
            .map(|h| h.join(".nika"))
            .unwrap_or_else(|| PathBuf::from(".nika"));

        if let Err(e) = std::fs::create_dir_all(&history_dir) {
            tracing::warn!("Failed to create history directory: {}", e);
            return;
        }

        let history_path = history_dir.join("history.json");
        if let Ok(content) = serde_json::to_string_pretty(&self.history) {
            if let Err(e) = std::fs::write(&history_path, content) {
                tracing::warn!("Failed to save history: {}", e);
            }
        }
    }

    /// Add entry to history
    pub fn add_history(&mut self, entry: HistoryEntry) {
        // Keep last 50 entries
        if self.history.len() >= 50 {
            self.history.remove(0);
        }
        self.history.push(entry);
        self.save_history();
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.history.clear();
        self.save_history();
    }

    /// Update preview content based on selected file
    pub fn update_preview(&mut self) {
        if let Some(entry) = self.browser_entries.get(self.browser_index) {
            if !entry.is_dir {
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
        use crate::ast::schema_validator::WorkflowSchemaValidator;
        use crate::ast::Workflow;
        use crate::dag::{validate_use_wiring, FlowGraph};

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

        // Step 3: Parse workflow
        let workflow: Workflow = match serde_yaml::from_str(&yaml) {
            Ok(w) => {
                result.push_str("│ ✓ YAML parsing passed\n");
                w
            }
            Err(e) => {
                result.push_str(&format!("│ ✗ YAML parsing failed: {}\n", e));
                result.push_str("╰─\n");
                self.preview_content = result;
                return;
            }
        };

        // Step 4: Validate schema version
        if let Err(e) = workflow.validate_schema() {
            result.push_str(&format!("│ ✗ Schema version invalid: {}\n", e));
            result.push_str("╰─\n");
            self.preview_content = result;
            return;
        }
        result.push_str("│ ✓ Schema version valid\n");

        // Step 5: Build and validate DAG
        let flow_graph = FlowGraph::from_workflow(&workflow);
        if let Err(e) = validate_use_wiring(&workflow, &flow_graph) {
            result.push_str(&format!("│ ✗ Binding validation failed: {}\n", e));
            result.push_str("╰─\n");
            self.preview_content = result;
            return;
        }
        result.push_str("│ ✓ DAG and binding validation passed\n");

        // Summary
        result.push_str("│\n");
        result.push_str("├─ Summary ─────────────────────\n");
        let provider_display = if workflow.provider.is_empty() {
            "(default)"
        } else {
            &workflow.provider
        };
        result.push_str(&format!("│ • Provider: {}\n", provider_display));
        result.push_str(&format!(
            "│ • Model: {}\n",
            workflow.model.as_deref().unwrap_or("(default)")
        ));
        result.push_str(&format!("│ • Tasks: {}\n", workflow.tasks.len()));
        result.push_str(&format!("│ • Flows: {}\n", workflow.flows.len()));
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

    // ───────────────────────────────────────────────────────────────────────────
    // StandalonePanel Tests
    // ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_standalone_panel_cycle() {
        let panel = StandalonePanel::Browser;
        assert_eq!(panel.next(), StandalonePanel::History);
        assert_eq!(panel.next().next(), StandalonePanel::Preview);
        assert_eq!(panel.next().next().next(), StandalonePanel::Browser);
    }

    #[test]
    fn test_standalone_panel_prev() {
        let panel = StandalonePanel::Browser;
        assert_eq!(panel.prev(), StandalonePanel::Preview);
        assert_eq!(panel.prev().prev(), StandalonePanel::History);
        assert_eq!(panel.prev().prev().prev(), StandalonePanel::Browser);
    }

    #[test]
    fn test_standalone_panel_title() {
        assert_eq!(StandalonePanel::Browser.title(), "WORKFLOW BROWSER");
        assert_eq!(StandalonePanel::History.title(), "HISTORY");
        assert_eq!(StandalonePanel::Preview.title(), "PREVIEW");
    }

    #[test]
    fn test_standalone_panel_number() {
        assert_eq!(StandalonePanel::Browser.number(), 1);
        assert_eq!(StandalonePanel::History.number(), 2);
        assert_eq!(StandalonePanel::Preview.number(), 3);
    }

    #[test]
    fn test_standalone_panel_default() {
        let panel = StandalonePanel::default();
        assert_eq!(panel, StandalonePanel::Browser);
    }

    #[test]
    fn test_standalone_panel_equality() {
        let browser1 = StandalonePanel::Browser;
        let browser2 = StandalonePanel::Browser;
        let history = StandalonePanel::History;
        assert_eq!(browser1, browser2);
        assert_ne!(browser1, history);
    }

    // ───────────────────────────────────────────────────────────────────────────
    // BrowserEntry Tests
    // ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_browser_entry_new_file() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/examples/test.nika.yaml");
        let entry = BrowserEntry::new(path.clone(), &root);

        assert_eq!(entry.path, path);
        assert_eq!(entry.display_name, "examples/test.nika.yaml");
        assert!(!entry.is_dir);
        assert!(!entry.expanded);
        assert_eq!(entry.depth, 0);
    }

    #[test]
    fn test_browser_entry_with_depth() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/examples/test.nika.yaml");
        let entry = BrowserEntry::new(path, &root).with_depth(2);

        assert_eq!(entry.depth, 2);
    }

    #[test]
    fn test_browser_entry_depth_chain() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/examples/test.nika.yaml");
        let entry = BrowserEntry::new(path, &root).with_depth(1).with_depth(3);

        assert_eq!(entry.depth, 3);
    }

    #[test]
    fn test_browser_entry_strip_prefix() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/deep/nested/workflow.nika.yaml");
        let entry = BrowserEntry::new(path, &root);

        assert_eq!(entry.display_name, "deep/nested/workflow.nika.yaml");
    }

    #[test]
    fn test_browser_entry_fallback_display_name() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/other/path/test.nika.yaml");
        let entry = BrowserEntry::new(path.clone(), &root);

        // When path is not under root, display_name should be the full path
        assert_eq!(entry.display_name, path.display().to_string());
    }

    #[test]
    fn test_browser_entry_clone() {
        let root = PathBuf::from("/project");
        let path = PathBuf::from("/project/test.nika.yaml");
        let entry1 = BrowserEntry::new(path, &root).with_depth(1);
        let entry2 = entry1.clone();

        assert_eq!(entry1.path, entry2.path);
        assert_eq!(entry1.display_name, entry2.display_name);
        assert_eq!(entry1.is_dir, entry2.is_dir);
        assert_eq!(entry1.depth, entry2.depth);
    }

    // ───────────────────────────────────────────────────────────────────────────
    // HistoryEntry Tests
    // ───────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_history_entry_new() {
        let path = PathBuf::from("test.nika.yaml");
        let duration = Duration::from_millis(2700);
        let entry = HistoryEntry::new(path.clone(), duration, 3, true, "test summary".to_string());

        assert_eq!(entry.workflow_path, path);
        assert_eq!(entry.duration_ms, 2700);
        assert_eq!(entry.task_count, 3);
        assert!(entry.success);
        assert_eq!(entry.summary, "test summary");
    }

    #[test]
    fn test_history_entry_duration_display_seconds() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_millis(2700),
            3,
            true,
            "test".to_string(),
        );
        assert_eq!(entry.duration_display(), "2.7s");
    }

    #[test]
    fn test_history_entry_duration_display_minutes() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(90),
            5,
            true,
            "test".to_string(),
        );
        assert_eq!(entry.duration_display(), "1.5m");
    }

    #[test]
    fn test_history_entry_duration_display_zero() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_millis(0),
            1,
            true,
            "instant".to_string(),
        );
        assert_eq!(entry.duration_display(), "0.0s");
    }

    #[test]
    fn test_history_entry_duration_display_sub_second() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_millis(150),
            1,
            true,
            "fast".to_string(),
        );
        assert_eq!(entry.duration_display(), "0.1s");
    }

    #[test]
    fn test_history_entry_duration_display_exactly_60_seconds() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(60),
            1,
            true,
            "minute".to_string(),
        );
        assert_eq!(entry.duration_display(), "1.0m");
    }

    #[test]
    fn test_history_entry_timestamp_display() {
        let entry = HistoryEntry::new(
            PathBuf::from("test.nika.yaml"),
            Duration::from_secs(1),
            1,
            true,
            "test".to_string(),
        );

        let timestamp_str = entry.timestamp_display();
        // Format: YYYY-MM-DD HH:MM:SS
        assert_eq!(timestamp_str.len(), 19); // "2026-02-20 15:30:45"
        assert_eq!(timestamp_str.chars().nth(4), Some('-'));
        assert_eq!(timestamp_str.chars().nth(7), Some('-'));
        assert_eq!(timestamp_str.chars().nth(10), Some(' '));
        assert_eq!(timestamp_str.chars().nth(13), Some(':'));
        assert_eq!(timestamp_str.chars().nth(16), Some(':'));
    }

    #[test]
    fn test_history_entry_serialization() {
        let path = PathBuf::from("test.nika.yaml");
        let entry = HistoryEntry::new(path, Duration::from_secs(5), 2, true, "summary".to_string());

        let json = serde_json::to_string(&entry).expect("serialization should succeed");
        let deserialized: HistoryEntry =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(deserialized.workflow_path, entry.workflow_path);
        assert_eq!(deserialized.duration_ms, entry.duration_ms);
        assert_eq!(deserialized.task_count, entry.task_count);
        assert_eq!(deserialized.success, entry.success);
        assert_eq!(deserialized.summary, entry.summary);
    }

    #[test]
    fn test_history_entry_clone() {
        let path = PathBuf::from("test.nika.yaml");
        let entry1 =
            HistoryEntry::new(path, Duration::from_secs(5), 2, true, "summary".to_string());
        let entry2 = entry1.clone();

        assert_eq!(entry1.workflow_path, entry2.workflow_path);
        assert_eq!(entry1.duration_ms, entry2.duration_ms);
        assert_eq!(entry1.task_count, entry2.task_count);
        assert_eq!(entry1.success, entry2.success);
        assert_eq!(entry1.summary, entry2.summary);
    }

    // ───────────────────────────────────────────────────────────────────────────
    // StandaloneState Tests
    // ───────────────────────────────────────────────────────────────────────────

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
        state.history = vec![
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
        ];
        state.history_index = 1;

        state.history_up();

        assert_eq!(state.history_index, 0);
    }

    #[test]
    fn test_standalone_state_history_down_at_end() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.history = vec![HistoryEntry::new(
            PathBuf::from("test1.nika.yaml"),
            Duration::from_secs(1),
            1,
            true,
            "summary1".to_string(),
        )];
        state.history_index = 0;

        state.history_down();

        // Should remain at 0 (only one entry)
        assert_eq!(state.history_index, 0);
    }

    #[test]
    fn test_standalone_state_history_down_increments() {
        let root = PathBuf::from("/tmp/nika_test");
        let mut state = StandaloneState::new(root);
        state.history = vec![
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
        ];
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
        let root = PathBuf::from("/tmp/nika_test");
        let state = StandaloneState::new(root);

        assert_eq!(state.selected_workflow(), None);
    }

    #[test]
    fn test_standalone_state_selected_workflow_on_directory() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
        let mut dir_entry = BrowserEntry::new(PathBuf::from("/tmp/examples"), &root_clone);
        dir_entry.is_dir = true;
        state.browser_entries.push(dir_entry);
        state.browser_index = 0;

        assert_eq!(state.selected_workflow(), None);
    }

    #[test]
    fn test_standalone_state_selected_workflow_on_file() {
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
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
        let root = PathBuf::from("/tmp/nika_test");
        let root_clone = root.clone();
        let mut state = StandaloneState::new(root);
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

    #[test]
    fn test_browser_entry_is_dir_false() {
        let root = PathBuf::from("/tmp");
        let path = PathBuf::from("/tmp/test.nika.yaml");
        let entry = BrowserEntry::new(path, &root);

        assert!(!entry.is_dir);
    }
}
