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

    #[test]
    fn test_standalone_panel_cycle() {
        let panel = StandalonePanel::Browser;
        assert_eq!(panel.next(), StandalonePanel::History);
        assert_eq!(panel.next().next(), StandalonePanel::Preview);
        assert_eq!(panel.next().next().next(), StandalonePanel::Browser);
    }

    #[test]
    fn test_history_entry_duration_display() {
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
    fn test_history_entry_long_duration() {
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
    fn test_browser_entry_depth() {
        let root = PathBuf::from("/project");
        let entry = BrowserEntry::new(PathBuf::from("/project/examples/test.nika.yaml"), &root)
            .with_depth(1);
        assert_eq!(entry.depth, 1);
        assert_eq!(entry.display_name, "examples/test.nika.yaml");
    }
}
