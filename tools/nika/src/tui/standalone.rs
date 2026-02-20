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
    pub fn scan_workflows(&mut self) {
        self.browser_entries.clear();

        // Common locations to scan
        let scan_dirs = ["examples", "workflows", ".", "tests"];

        for dir in scan_dirs {
            let dir_path = self.root.join(dir);
            if dir_path.exists() && dir_path.is_dir() {
                self.scan_directory(&dir_path, 0);
            }
        }

        // Sort by path
        self.browser_entries
            .sort_by(|a, b| a.display_name.cmp(&b.display_name));
    }

    fn scan_directory(&mut self, dir: &Path, depth: usize) {
        if depth > 3 {
            return; // Limit recursion
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut files: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            files.sort_by_key(|e| e.path());

            for entry in files {
                let path = entry.path();

                if path.is_file() {
                    // Only include .nika.yaml files
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        if name_str.ends_with(".nika.yaml") {
                            self.browser_entries
                                .push(BrowserEntry::new(path, &self.root).with_depth(depth));
                        }
                    }
                } else if path.is_dir() {
                    // Recurse into subdirectories
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    // Skip hidden dirs and common non-workflow dirs
                    if !name.starts_with('.')
                        && name != "target"
                        && name != "node_modules"
                        && name != ".git"
                    {
                        self.scan_directory(&path, depth + 1);
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
