//! Browser View
//!
//! Workflow browser for selecting and previewing workflows before execution.
//!
//! # Layout
//!
//! ```text
//! ╭──────────────────────────────────────────────────────────────────────────────────╮
//! │  ⚡ NIKA WORKFLOW STUDIO                                              v0.5.1    │
//! │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
//! ├────────────────────────┬─────────────────────────────────────────────────────────┤
//! │  📁 WORKFLOWS          │  ┌─────────────────── DAG PREVIEW ───────────────────┐  │
//! │  ══════════════════    │  │                                                   │  │
//! │                        │  │            ╭──────────╮                           │  │
//! │  ▾ 📂 examples/        │  │            │  task1   │                           │  │
//! │    ├─ 📄 invoke.nika   │  │            ╰────┬─────╯                           │  │
//! │    ├─ 📄 agent.nika    │  │                 │                                 │  │
//! │    └─ 📄 fetch.nika    │  │                 ▼                                 │  │
//! │  ▸ 📂 workflows/       │  │            ╭──────────╮                           │  │
//! │                        │  │            │  task2   │                           │  │
//! │  ─────────────────     │  │            ╰──────────╯                           │  │
//! │  ► invoke.nika.yaml    │  │                                                   │  │
//! │    4 tasks · 3 flows   │  │   Tasks: 4    Flows: 3    MCP: novanet            │  │
//! │                        │  └───────────────────────────────────────────────────┘  │
//! ├────────────────────────┼─────────────────────────────────────────────────────────┤
//! │  📋 YAML PREVIEW       │  ℹ️  WORKFLOW INFO                                      │
//! │  ══════════════════    │  ══════════════════                                    │
//! │  ┌──────────────────┐  │                                                        │
//! │  │ schema: nika/0.5 │  │  ┌─────────────────────────────────────────────────┐   │
//! │  │ workflow: invoke │  │  │ VERBS        invoke ████████░░ 3                │   │
//! │  │                  │  │  │              infer  ██░░░░░░░░ 1                │   │
//! │  │ tasks:           │  │  └─────────────────────────────────────────────────┘   │
//! │  │   - id: schema   │  │                                                        │
//! │  │     invoke: ...  │  │  ╭─────────────────────────────────────────────────╮   │
//! │  └──────────────────┘  │  │           ▶▶  PRESS ENTER TO RUN  ◀◀            │   │
//! │  ▲▼ scroll             │  ╰─────────────────────────────────────────────────╯   │
//! ╰────────────────────────┴─────────────────────────────────────────────────────────╯
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::ast::{Task, TaskAction, Workflow};
use crate::tui::standalone::StandaloneState;
use crate::tui::theme::Theme;
use crate::tui::watcher::FileEvent;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════════
// VALIDATION STATUS
// ═══════════════════════════════════════════════════════════════════════════════

/// Validation status for a workflow file
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ValidationStatus {
    /// Workflow is valid
    Valid,
    /// Workflow has warnings (non-blocking issues)
    Warning(String),
    /// Workflow has errors (parse failures)
    Error(String),
    /// Not yet validated
    #[default]
    Unknown,
}

impl ValidationStatus {
    /// Get the status icon
    pub fn icon(&self) -> &'static str {
        match self {
            ValidationStatus::Valid => "✓",
            ValidationStatus::Warning(_) => "⚠",
            ValidationStatus::Error(_) => "✗",
            ValidationStatus::Unknown => "?",
        }
    }

    /// Get the status color
    pub fn color(&self) -> Color {
        match self {
            ValidationStatus::Valid => Color::Green,
            ValidationStatus::Warning(_) => Color::Yellow,
            ValidationStatus::Error(_) => Color::Red,
            ValidationStatus::Unknown => Color::DarkGray,
        }
    }

    /// Get the status message (if any)
    pub fn message(&self) -> Option<&str> {
        match self {
            ValidationStatus::Warning(msg) | ValidationStatus::Error(msg) => Some(msg),
            _ => None,
        }
    }

    /// Check if the workflow is runnable (valid or warning)
    pub fn is_runnable(&self) -> bool {
        matches!(self, ValidationStatus::Valid | ValidationStatus::Warning(_))
    }
}

/// Focused panel in the browser view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrowserPanel {
    /// Workflow tree (left top)
    #[default]
    Tree,
    /// DAG preview (right top)
    DagPreview,
    /// YAML preview (left bottom)
    YamlPreview,
    /// Workflow info (right bottom)
    Info,
}

impl BrowserPanel {
    /// Cycle to next panel
    pub fn next(&self) -> Self {
        match self {
            BrowserPanel::Tree => BrowserPanel::DagPreview,
            BrowserPanel::DagPreview => BrowserPanel::YamlPreview,
            BrowserPanel::YamlPreview => BrowserPanel::Info,
            BrowserPanel::Info => BrowserPanel::Tree,
        }
    }

    /// Cycle to previous panel
    pub fn prev(&self) -> Self {
        match self {
            BrowserPanel::Tree => BrowserPanel::Info,
            BrowserPanel::DagPreview => BrowserPanel::Tree,
            BrowserPanel::YamlPreview => BrowserPanel::DagPreview,
            BrowserPanel::Info => BrowserPanel::YamlPreview,
        }
    }

    /// Get panel number (1-4)
    pub fn number(&self) -> u8 {
        match self {
            BrowserPanel::Tree => 1,
            BrowserPanel::DagPreview => 2,
            BrowserPanel::YamlPreview => 3,
            BrowserPanel::Info => 4,
        }
    }

    /// Get panel title
    pub fn title(&self) -> &'static str {
        match self {
            BrowserPanel::Tree => "WORKFLOWS",
            BrowserPanel::DagPreview => "DAG PREVIEW",
            BrowserPanel::YamlPreview => "YAML PREVIEW",
            BrowserPanel::Info => "WORKFLOW INFO",
        }
    }

    /// Get panel icon
    pub fn icon(&self) -> &'static str {
        match self {
            BrowserPanel::Tree => "📁",
            BrowserPanel::DagPreview => "🔷",
            BrowserPanel::YamlPreview => "📋",
            BrowserPanel::Info => "ℹ️",
        }
    }
}

/// Summary of a single task for DAG display
#[derive(Debug, Clone)]
pub struct TaskSummary {
    /// Task ID
    pub id: String,
    /// Verb icon (🧠, ⚡, 🌐, 🔌, 🤖)
    pub icon: &'static str,
    /// Verb name (infer, exec, fetch, invoke, agent)
    pub verb: &'static str,
    /// Estimated duration
    pub estimate: &'static str,
    /// Dependencies (task IDs this task depends on)
    pub depends_on: Vec<String>,
}

/// MCP tool usage for preview
#[derive(Debug, Clone, Default)]
pub struct McpToolUsage {
    /// Tool name (e.g., "novanet_generate")
    pub tool: String,
    /// Number of times this tool is called
    pub count: usize,
}

/// Parsed workflow info for display
#[derive(Debug, Clone, Default)]
pub struct WorkflowInfo {
    /// Workflow name
    pub name: String,
    /// Number of tasks
    pub task_count: usize,
    /// Number of flows
    pub flow_count: usize,
    /// MCP servers used
    pub mcp_servers: Vec<String>,
    /// MCP tools per server (server_name -> [tools])
    pub mcp_tools: HashMap<String, Vec<McpToolUsage>>,
    /// MCP resources accessed (server_name -> [resource_uris])
    pub mcp_resources: HashMap<String, Vec<String>>,
    /// Verb counts (verb name -> count)
    pub verb_counts: HashMap<String, usize>,
    /// Schema version
    pub schema: String,
    /// Parse error if any
    pub error: Option<String>,
    /// Validation status (valid, warning, error)
    pub validation_status: ValidationStatus,
    /// Summary of verbs used (e.g., "invoke,infer,agent")
    pub verb_summary: String,
    /// Task summaries for DAG display
    pub tasks: Vec<TaskSummary>,
}

impl WorkflowInfo {
    /// Parse workflow info from YAML content
    pub fn from_yaml(yaml: &str) -> Self {
        match serde_yaml::from_str::<Workflow>(yaml) {
            Ok(workflow) => {
                let mut verb_counts = HashMap::new();
                let mut verb_set: HashSet<&str> = HashSet::new();

                for task in &workflow.tasks {
                    let verb = Self::get_verb_name(&task.action);
                    *verb_counts.entry(verb.to_string()).or_insert(0) += 1;
                    verb_set.insert(verb);
                }

                let mcp_servers: Vec<String> = workflow
                    .mcp
                    .as_ref()
                    .map(|mcp| mcp.keys().cloned().collect())
                    .unwrap_or_default();

                // Extract MCP tools and resources from invoke/agent tasks
                let (mcp_tools, mcp_resources) = Self::extract_mcp_usage(&workflow.tasks);

                // Extract name from schema (e.g., "nika/workflow@0.5" -> "workflow@0.5")
                let name = workflow
                    .schema
                    .split('/')
                    .next_back()
                    .unwrap_or(&workflow.schema)
                    .to_string();

                // Build verb summary
                let verb_summary = Self::build_verb_summary(&verb_set);

                // Validate workflow and get warnings
                let warnings = Self::check_warnings(&workflow, &mcp_servers);
                let validation_status = if warnings.is_empty() {
                    ValidationStatus::Valid
                } else {
                    ValidationStatus::Warning(warnings.join(", "))
                };

                // Build dependencies map from flows (target -> [sources])
                let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
                for flow in &workflow.flows {
                    let sources = flow.source.as_vec();
                    let targets = flow.target.as_vec();
                    for target in targets {
                        let deps = dependencies.entry(target.to_string()).or_default();
                        for source in &sources {
                            deps.push(source.to_string());
                        }
                    }
                }

                // Build task summaries for DAG display
                let tasks: Vec<TaskSummary> = workflow
                    .tasks
                    .iter()
                    .map(|task| TaskSummary {
                        id: task.id.clone(),
                        icon: Self::get_verb_icon(&task.action),
                        verb: Self::get_verb_name(&task.action),
                        estimate: Self::estimate_duration(&task.action),
                        depends_on: dependencies.get(&task.id).cloned().unwrap_or_default(),
                    })
                    .collect();

                Self {
                    name,
                    task_count: workflow.tasks.len(),
                    flow_count: workflow.flows.len(),
                    mcp_servers,
                    mcp_tools,
                    mcp_resources,
                    verb_counts,
                    schema: workflow.schema.clone(),
                    error: None,
                    validation_status,
                    verb_summary,
                    tasks,
                }
            }
            Err(e) => {
                // Extract a shorter error message
                let error_msg = e.to_string();
                let short_error = error_msg
                    .lines()
                    .next()
                    .unwrap_or(&error_msg)
                    .chars()
                    .take(50)
                    .collect::<String>();

                Self {
                    error: Some(e.to_string()),
                    validation_status: ValidationStatus::Error(short_error),
                    ..Default::default()
                }
            }
        }
    }

    /// Build a comma-separated summary of verbs used
    fn build_verb_summary(verbs: &HashSet<&str>) -> String {
        let mut sorted: Vec<&str> = verbs.iter().copied().collect();
        sorted.sort();
        sorted.join(",")
    }

    /// Check for warnings in the workflow
    fn check_warnings(workflow: &Workflow, mcp_servers: &[String]) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for invoke without MCP config
        let has_invoke = workflow
            .tasks
            .iter()
            .any(|t| matches!(&t.action, TaskAction::Invoke { .. }));
        if has_invoke && mcp_servers.is_empty() {
            warnings.push("invoke without MCP config".to_string());
        }

        // Check for agent without MCP config
        let has_agent = workflow
            .tasks
            .iter()
            .any(|t| matches!(&t.action, TaskAction::Agent { .. }));
        if has_agent && mcp_servers.is_empty() {
            warnings.push("agent without MCP servers".to_string());
        }

        // Check for empty tasks
        if workflow.tasks.is_empty() {
            warnings.push("no tasks defined".to_string());
        }

        warnings
    }

    /// Extract MCP tools and resources from workflow tasks
    ///
    /// Returns (mcp_tools, mcp_resources) where:
    /// - mcp_tools: HashMap of server_name -> Vec<McpToolUsage>
    /// - mcp_resources: HashMap of server_name -> Vec<resource_uri>
    fn extract_mcp_usage(
        tasks: &[Arc<Task>],
    ) -> (
        HashMap<String, Vec<McpToolUsage>>,
        HashMap<String, Vec<String>>,
    ) {
        let mut tools: HashMap<String, HashMap<String, usize>> = HashMap::new();
        let mut resources: HashMap<String, Vec<String>> = HashMap::new();

        for task in tasks {
            match &task.action {
                TaskAction::Invoke { invoke } => {
                    let server = invoke.mcp.clone();

                    // Extract tool name if present
                    if let Some(ref tool_name) = invoke.tool {
                        let server_tools = tools.entry(server.clone()).or_default();
                        *server_tools.entry(tool_name.clone()).or_insert(0) += 1;
                    }

                    // Extract resource URI if present
                    if let Some(ref resource_uri) = invoke.resource {
                        let server_resources = resources.entry(server).or_default();
                        if !server_resources.contains(resource_uri) {
                            server_resources.push(resource_uri.clone());
                        }
                    }
                }
                TaskAction::Agent { agent } => {
                    // Agent tasks list MCP servers they can use
                    // We just note which servers are available (no specific tools)
                    for server in &agent.mcp {
                        tools.entry(server.clone()).or_default();
                    }
                }
                _ => {} // Other verbs don't use MCP
            }
        }

        // Convert tool counts to McpToolUsage structs
        let mcp_tools: HashMap<String, Vec<McpToolUsage>> = tools
            .into_iter()
            .map(|(server, tool_counts)| {
                let mut tool_list: Vec<McpToolUsage> = tool_counts
                    .into_iter()
                    .map(|(tool, count)| McpToolUsage { tool, count })
                    .collect();
                // Sort by count descending, then by name
                tool_list.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tool.cmp(&b.tool)));
                (server, tool_list)
            })
            .collect();

        (mcp_tools, resources)
    }

    /// Get the verb name from a TaskAction
    fn get_verb_name(action: &TaskAction) -> &'static str {
        match action {
            TaskAction::Infer { .. } => "infer",
            TaskAction::Exec { .. } => "exec",
            TaskAction::Fetch { .. } => "fetch",
            TaskAction::Invoke { .. } => "invoke",
            TaskAction::Agent { .. } => "agent",
        }
    }

    /// Get verb icon for display
    pub fn get_verb_icon(action: &TaskAction) -> &'static str {
        match action {
            TaskAction::Infer { .. } => "🧠",
            TaskAction::Exec { .. } => "⚡",
            TaskAction::Fetch { .. } => "🔗",
            TaskAction::Invoke { .. } => "📥",
            TaskAction::Agent { .. } => "🤖",
        }
    }

    /// Estimate duration for a task based on verb type
    pub fn estimate_duration(action: &TaskAction) -> &'static str {
        match action {
            TaskAction::Infer { .. } => "~2-5s",
            TaskAction::Exec { .. } => "~0.1s",
            TaskAction::Fetch { .. } => "~0.5s",
            TaskAction::Invoke { .. } => "~0.5-2s",
            TaskAction::Agent { .. } => "~5-30s",
        }
    }
}

/// Browser view state
#[derive(Debug)]
pub struct BrowserView {
    /// Standalone state (tree, history, etc.)
    pub standalone: StandaloneState,
    /// Currently focused panel
    pub focused_panel: BrowserPanel,
    /// Parsed workflow info for selected file
    pub workflow_info: Option<WorkflowInfo>,
    /// YAML scroll offset
    pub yaml_scroll: u16,
    /// DAG scroll offset
    pub dag_scroll: u16,
    /// Tree scroll offset
    pub tree_scroll: u16,
    /// Info panel scroll offset
    pub info_scroll: u16,
    /// Cached workflow info per file path (for validation display)
    pub workflow_cache: HashMap<PathBuf, WorkflowInfo>,
    /// Run history per workflow (duration in ms, max 10 entries)
    pub run_history: HashMap<PathBuf, Vec<u64>>,
}

impl BrowserView {
    /// Create a new browser view
    pub fn new(root: PathBuf) -> Self {
        let standalone = StandaloneState::new(root);
        let workflow_info = if !standalone.preview_content.is_empty() {
            Some(WorkflowInfo::from_yaml(&standalone.preview_content))
        } else {
            None
        };

        let mut view = Self {
            standalone,
            focused_panel: BrowserPanel::Tree,
            workflow_info,
            yaml_scroll: 0,
            dag_scroll: 0,
            tree_scroll: 0,
            info_scroll: 0,
            workflow_cache: HashMap::new(),
            run_history: HashMap::new(),
        };

        // Scan all workflow files for validation
        view.refresh_workflow_cache();
        view
    }

    /// Refresh the workflow cache by scanning all files
    pub fn refresh_workflow_cache(&mut self) {
        self.workflow_cache.clear();

        for entry in &self.standalone.browser_entries {
            if !entry.is_dir
                && entry
                    .path
                    .extension()
                    .is_some_and(|e| e == "yaml" || e == "yml")
            {
                if let Ok(content) = std::fs::read_to_string(&entry.path) {
                    let info = WorkflowInfo::from_yaml(&content);
                    self.workflow_cache.insert(entry.path.clone(), info);
                }
            }
        }
    }

    /// Get validation stats for the tree summary
    pub fn validation_stats(&self) -> (usize, usize, usize) {
        let mut valid = 0;
        let mut warnings = 0;
        let mut errors = 0;

        for info in self.workflow_cache.values() {
            match &info.validation_status {
                ValidationStatus::Valid => valid += 1,
                ValidationStatus::Warning(_) => warnings += 1,
                ValidationStatus::Error(_) => errors += 1,
                ValidationStatus::Unknown => {}
            }
        }

        (valid, warnings, errors)
    }

    /// Handle a file change event from the watcher
    ///
    /// Returns `true` if the view needs to be refreshed
    pub fn handle_file_event(&mut self, event: FileEvent) -> bool {
        match event {
            FileEvent::Created(path) => {
                // Re-parse the workflow and update cache
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let info = WorkflowInfo::from_yaml(&content);
                    self.workflow_cache.insert(path.clone(), info);
                }

                // Refresh the browser entries to show new files
                self.standalone.refresh_entries();

                true
            }
            FileEvent::Modified(path) => {
                // Re-parse the workflow and update cache
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let info = WorkflowInfo::from_yaml(&content);
                    self.workflow_cache.insert(path.clone(), info);

                    // If this is the currently selected file, update the preview
                    if self.standalone.selected_workflow() == Some(path.as_path()) {
                        self.standalone.update_preview();
                        self.update_workflow_info();
                    }
                }

                true
            }
            FileEvent::Removed(path) => {
                // Remove from cache
                self.workflow_cache.remove(&path);

                // Refresh browser entries
                self.standalone.refresh_entries();

                // If this was the selected file, clear the preview
                if self.standalone.selected_workflow() == Some(path.as_path()) {
                    self.workflow_info = None;
                }

                true
            }
            FileEvent::Renamed(old_path, new_path) => {
                // Update cache with new path
                if let Some(info) = self.workflow_cache.remove(&old_path) {
                    self.workflow_cache.insert(new_path.clone(), info);
                }

                // Refresh browser entries
                self.standalone.refresh_entries();

                true
            }
        }
    }

    /// Get currently selected workflow path
    pub fn selected_workflow(&self) -> Option<&Path> {
        self.standalone.selected_workflow()
    }

    /// Navigate up in the current panel
    pub fn navigate_up(&mut self) {
        match self.focused_panel {
            BrowserPanel::Tree => {
                self.standalone.browser_up();
                self.update_workflow_info();
                self.ensure_tree_selection_visible();
            }
            BrowserPanel::YamlPreview => {
                self.yaml_scroll = self.yaml_scroll.saturating_sub(1);
            }
            BrowserPanel::DagPreview => {
                self.dag_scroll = self.dag_scroll.saturating_sub(1);
            }
            BrowserPanel::Info => {
                self.info_scroll = self.info_scroll.saturating_sub(1);
            }
        }
    }

    /// Navigate down in the current panel
    pub fn navigate_down(&mut self) {
        match self.focused_panel {
            BrowserPanel::Tree => {
                self.standalone.browser_down();
                self.update_workflow_info();
                self.ensure_tree_selection_visible();
            }
            BrowserPanel::YamlPreview => {
                let max_scroll = self
                    .standalone
                    .preview_content
                    .lines()
                    .count()
                    .saturating_sub(10) as u16;
                if self.yaml_scroll < max_scroll {
                    self.yaml_scroll += 1;
                }
            }
            BrowserPanel::DagPreview => {
                self.dag_scroll += 1;
            }
            BrowserPanel::Info => {
                self.info_scroll += 1;
            }
        }
    }

    /// Ensure the selected tree item is visible by adjusting tree_scroll
    fn ensure_tree_selection_visible(&mut self) {
        let selected = self.standalone.browser_index;
        let scroll = self.tree_scroll as usize;
        // Assume approximately 15 visible items (will be refined based on actual area)
        let visible_height = 15;

        // If selected is above visible area, scroll up
        if selected < scroll {
            self.tree_scroll = selected as u16;
        }
        // If selected is below visible area, scroll down
        else if selected >= scroll + visible_height {
            self.tree_scroll = (selected - visible_height + 1) as u16;
        }
    }

    /// Update workflow info when selection changes
    fn update_workflow_info(&mut self) {
        self.workflow_info = if !self.standalone.preview_content.is_empty() {
            Some(WorkflowInfo::from_yaml(&self.standalone.preview_content))
        } else {
            None
        };
        self.yaml_scroll = 0;
        self.dag_scroll = 0;
    }

    /// Cycle to next panel
    pub fn next_panel(&mut self) {
        self.focused_panel = self.focused_panel.next();
    }

    /// Cycle to previous panel
    pub fn prev_panel(&mut self) {
        self.focused_panel = self.focused_panel.prev();
    }

    /// Focus a specific panel by number (1-4)
    pub fn focus_panel(&mut self, number: u8) {
        self.focused_panel = match number {
            1 => BrowserPanel::Tree,
            2 => BrowserPanel::DagPreview,
            3 => BrowserPanel::YamlPreview,
            4 => BrowserPanel::Info,
            _ => self.focused_panel,
        };
    }

    /// Render the browser view
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        // Split into top and bottom halves
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        // Split top into left (tree) and right (DAG preview)
        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(chunks[0]);

        // Split bottom into left (YAML) and right (info)
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        self.render_tree(top[0], buf, theme);
        self.render_dag_preview(top[1], buf, theme);
        self.render_yaml_preview(bottom[0], buf, theme);
        self.render_info(bottom[1], buf, theme);
    }

    /// Render workflow tree panel
    fn render_tree(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        use crate::tui::widgets::ScrollIndicator;

        let is_focused = self.focused_panel == BrowserPanel::Tree;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        // Get validation stats for title
        let (valid, warnings, errors) = self.validation_stats();
        let title = format!(
            " {} {} (✓{} ⚠{} ✗{}) ",
            BrowserPanel::Tree.icon(),
            BrowserPanel::Tree.title(),
            valid,
            warnings,
            errors
        );

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Reserve 1 line for summary bar at bottom, 1 column for scrollbar
        let content_width = inner.width.saturating_sub(1);
        let list_area = Rect {
            x: inner.x,
            y: inner.y,
            width: content_width,
            height: inner.height.saturating_sub(1),
        };
        let scrollbar_area = Rect {
            x: inner.x + content_width,
            y: inner.y,
            width: 1,
            height: inner.height.saturating_sub(1),
        };
        let summary_area = Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };

        let total_items = self.standalone.browser_entries.len();
        let visible_height = list_area.height as usize;
        let scroll_offset = self.tree_scroll as usize;

        // Build list items with validation info (only visible ones)
        let items: Vec<ListItem> = self
            .standalone
            .browser_entries
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|(i, entry)| {
                let indent = "  ".repeat(entry.depth);
                let icon = if entry.is_dir { "📂" } else { "📄" };

                // Get validation info for files
                let (status_icon, status_color, extra_info) = if !entry.is_dir {
                    if let Some(info) = self.workflow_cache.get(&entry.path) {
                        let icon = info.validation_status.icon();
                        let color = info.validation_status.color();
                        let extra = format!(
                            "{}t {}",
                            info.task_count,
                            if info.verb_summary.is_empty() {
                                String::new()
                            } else {
                                info.verb_summary.clone()
                            }
                        );
                        (icon, color, extra)
                    } else {
                        ("?", Color::DarkGray, String::new())
                    }
                } else {
                    ("", Color::Reset, String::new())
                };

                let is_selected = i == self.standalone.browser_index;
                let base_style = if is_selected {
                    Style::default().bg(theme.highlight).fg(theme.text_primary)
                } else {
                    Style::default().fg(theme.text_muted)
                };

                // Build the line with spans for colored status
                let mut spans = vec![
                    Span::styled(format!("{}{} ", indent, icon), base_style),
                    Span::styled(entry.display_name.clone(), base_style),
                ];

                // Add validation status for files
                if !entry.is_dir && !status_icon.is_empty() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        status_icon.to_string(),
                        if is_selected {
                            Style::default().bg(theme.highlight).fg(status_color)
                        } else {
                            Style::default().fg(status_color)
                        },
                    ));
                    if !extra_info.is_empty() {
                        spans.push(Span::styled(
                            format!(" {}", extra_info),
                            if is_selected {
                                Style::default().bg(theme.highlight).fg(Color::DarkGray)
                            } else {
                                Style::default().fg(Color::DarkGray)
                            },
                        ));
                    }
                }

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(items);
        Widget::render(list, list_area, buf);

        // Render scrollbar
        let scroll_indicator = ScrollIndicator::new()
            .position(scroll_offset, total_items, visible_height)
            .track_style(Style::default().fg(theme.border_normal))
            .thumb_style(Style::default().fg(if is_focused {
                theme.border_focused
            } else {
                Color::DarkGray
            }));
        Widget::render(scroll_indicator, scrollbar_area, buf);

        // Render summary bar
        self.render_tree_summary(summary_area, buf, theme);
    }

    /// Render the tree summary bar
    fn render_tree_summary(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let (valid, warnings, errors) = self.validation_stats();
        let total = self.workflow_cache.len();

        let line = Line::from(vec![
            Span::styled("─".repeat(2), Style::default().fg(theme.border_normal)),
            Span::raw(" "),
            Span::styled(
                format!("{} files", total),
                Style::default().fg(Color::White),
            ),
            Span::raw(" "),
            Span::styled(format!("✓{}", valid), Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(format!("⚠{}", warnings), Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(format!("✗{}", errors), Style::default().fg(Color::Red)),
        ]);

        Paragraph::new(line).render(area, buf);
    }

    /// Render DAG preview panel
    fn render_dag_preview(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        use crate::tui::widgets::ScrollIndicator;

        let is_focused = self.focused_panel == BrowserPanel::DagPreview;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::DagPreview.icon(),
                BrowserPanel::DagPreview.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Generate ASCII DAG from workflow info
        let dag_text = if let Some(info) = &self.workflow_info {
            if let Some(error) = &info.error {
                format!("⚠️ Parse error:\n{}", error)
            } else {
                self.generate_dag_ascii(info)
            }
        } else {
            "No workflow selected".to_string()
        };

        let total_lines = dag_text.lines().count();
        let visible_height = inner.height as usize;
        let scroll_offset = self.dag_scroll as usize;

        // Reserve 1 column for scrollbar
        let content_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height,
        };
        let scrollbar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };

        let paragraph = Paragraph::new(dag_text)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false })
            .scroll((self.dag_scroll, 0));

        paragraph.render(content_area, buf);

        // Render scrollbar
        let scroll_indicator = ScrollIndicator::new()
            .position(scroll_offset, total_lines, visible_height)
            .track_style(Style::default().fg(theme.border_normal))
            .thumb_style(Style::default().fg(if is_focused {
                theme.border_focused
            } else {
                Color::DarkGray
            }));
        Widget::render(scroll_indicator, scrollbar_area, buf);
    }

    /// Generate ASCII DAG visualization
    fn generate_dag_ascii(&self, info: &WorkflowInfo) -> String {
        let mut lines = Vec::new();

        lines.push(format!("   Workflow: {}", info.name));
        lines.push(String::new());

        if info.tasks.is_empty() {
            // Fallback for workflows with no parsed tasks
            lines.push("   ╭──────────────────╮".to_string());
            lines.push("   │  (no tasks)      │".to_string());
            lines.push("   ╰──────────────────╯".to_string());
        } else {
            // Render each task with icon, id, verb, and estimate
            for (i, task) in info.tasks.iter().enumerate() {
                // Task box with verb icon and details
                let task_line = format!(
                    "{} {} ({}) {}",
                    task.icon, task.id, task.verb, task.estimate
                );
                let box_width = task_line.chars().count().max(20) + 4;
                let padding = box_width - task_line.chars().count() - 2;

                // Draw dependencies (if any)
                if !task.depends_on.is_empty() {
                    let deps = task.depends_on.join(", ");
                    lines.push(format!("   ┌─ from: {}", deps));
                    lines.push("   │".to_string());
                    lines.push("   ▼".to_string());
                }

                // Top border
                lines.push(format!("   ╭{}╮", "─".repeat(box_width - 2)));
                // Task content
                lines.push(format!("   │ {}{}│", task_line, " ".repeat(padding)));
                // Bottom border
                lines.push(format!("   ╰{}╯", "─".repeat(box_width - 2)));

                // Arrow to next task (if not last)
                if i < info.tasks.len() - 1 {
                    lines.push("        │".to_string());
                    lines.push("        ▼".to_string());
                }
            }
        }

        lines.push(String::new());
        lines.push(format!(
            "   Tasks: {}    Flows: {}",
            info.task_count, info.flow_count
        ));

        if !info.mcp_servers.is_empty() {
            lines.push(format!("   MCP: {}", info.mcp_servers.join(", ")));
        }

        lines.join("\n")
    }

    /// Render YAML preview panel
    fn render_yaml_preview(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        use crate::tui::widgets::ScrollIndicator;

        let is_focused = self.focused_panel == BrowserPanel::YamlPreview;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::YamlPreview.icon(),
                BrowserPanel::YamlPreview.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        let content = if self.standalone.preview_content.is_empty() {
            "No file selected".to_string()
        } else {
            self.standalone.preview_content.clone()
        };

        let total_lines = content.lines().count();
        let visible_height = inner.height as usize;
        let scroll_offset = self.yaml_scroll as usize;

        // Reserve 1 column for scrollbar
        let content_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height,
        };
        let scrollbar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_muted))
            .wrap(Wrap { trim: false })
            .scroll((self.yaml_scroll, 0));

        paragraph.render(content_area, buf);

        // Render scrollbar
        let scroll_indicator = ScrollIndicator::new()
            .position(scroll_offset, total_lines, visible_height)
            .track_style(Style::default().fg(theme.border_normal))
            .thumb_style(Style::default().fg(if is_focused {
                theme.border_focused
            } else {
                Color::DarkGray
            }));
        Widget::render(scroll_indicator, scrollbar_area, buf);
    }

    /// Render workflow info panel
    fn render_info(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let is_focused = self.focused_panel == BrowserPanel::Info;
        let border_style = if is_focused {
            Style::default().fg(theme.border_focused)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                BrowserPanel::Info.icon(),
                BrowserPanel::Info.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        let content = if let Some(info) = &self.workflow_info {
            if let Some(error) = &info.error {
                format!("⚠️ Parse error:\n{}", error)
            } else {
                self.format_workflow_info(info)
            }
        } else {
            "No workflow selected".to_string()
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(Wrap { trim: false });

        paragraph.render(inner, buf);
    }

    /// Format workflow info for display
    fn format_workflow_info(&self, info: &WorkflowInfo) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Schema: {}", info.schema));
        lines.push(format!("Name: {}", info.name));
        lines.push(String::new());

        // Verb distribution with icons (sorted by count descending)
        lines.push("VERBS".to_string());
        lines.push("─────────────────────────".to_string());

        let max_count = info.verb_counts.values().max().copied().unwrap_or(1);

        // Sort verbs by count (descending)
        let mut verb_list: Vec<(&String, &usize)> = info.verb_counts.iter().collect();
        verb_list.sort_by(|a, b| b.1.cmp(a.1)); // descending

        for (verb, count) in verb_list {
            let bar_len = (*count * 16) / max_count.max(1);
            let bar = "█".repeat(bar_len);
            let empty = "░".repeat(16 - bar_len);

            // Get icon for verb
            let icon = Self::verb_icon_for_name(verb);

            lines.push(format!("{} {:8} {}{} {}", icon, verb, bar, empty, count));
        }

        lines.push(String::new());

        // Total estimated duration
        if !info.tasks.is_empty() {
            let (min_total, max_total) = Self::estimate_total_duration(info);
            lines.push(format!("⏱ Est. duration: ~{}-{}s", min_total, max_total));
            lines.push(String::new());
        }

        // MCP servers with tools preview
        if !info.mcp_servers.is_empty() {
            lines.push("MCP SERVERS".to_string());
            lines.push("─────────────────────────".to_string());

            for server in &info.mcp_servers {
                lines.push(format!("  ● {}", server));

                // Show tools used for this server
                if let Some(tools) = info.mcp_tools.get(server) {
                    if !tools.is_empty() {
                        for tool in tools {
                            let count_str = if tool.count > 1 {
                                format!(" ×{}", tool.count)
                            } else {
                                String::new()
                            };
                            lines.push(format!("    🔧 {}{}", tool.tool, count_str));
                        }
                    }
                }

                // Show resources accessed for this server
                if let Some(resources) = info.mcp_resources.get(server) {
                    if !resources.is_empty() {
                        for resource in resources {
                            // Truncate long resource URIs
                            let display = if resource.len() > 30 {
                                format!("{}...", &resource[..27])
                            } else {
                                resource.clone()
                            };
                            lines.push(format!("    📄 {}", display));
                        }
                    }
                }
            }
            lines.push(String::new());
        }

        // Run history
        let history_lines = self.format_run_history_section();
        lines.extend(history_lines);

        // Validation status
        match &info.validation_status {
            ValidationStatus::Valid => {
                lines.push(format!("✓ Valid workflow ({} tasks)", info.task_count));
            }
            ValidationStatus::Warning(msg) => {
                lines.push(format!("⚠ Warning: {}", msg));
            }
            ValidationStatus::Error(msg) => {
                lines.push(format!("✗ Error: {}", msg));
            }
            ValidationStatus::Unknown => {}
        }
        lines.push(String::new());

        // Run prompt
        if info.validation_status.is_runnable() {
            lines.push("╭─────────────────────────────────────────────╮".to_string());
            lines.push("│         ▶▶  PRESS ENTER TO RUN  ◀◀         │".to_string());
            lines.push("╰─────────────────────────────────────────────╯".to_string());
        } else {
            lines.push("╭─────────────────────────────────────────────╮".to_string());
            lines.push("│       ✗ Fix errors before running          │".to_string());
            lines.push("╰─────────────────────────────────────────────╯".to_string());
        }

        lines.join("\n")
    }

    /// Get verb icon from verb name string
    fn verb_icon_for_name(verb: &str) -> &'static str {
        match verb {
            "infer" => "🧠",
            "exec" => "⚡",
            "fetch" => "🔗",
            "invoke" => "📥",
            "agent" => "🤖",
            _ => "  ",
        }
    }

    /// Estimate total duration range from task summaries
    fn estimate_total_duration(info: &WorkflowInfo) -> (u32, u32) {
        let mut min_total: u32 = 0;
        let mut max_total: u32 = 0;

        for task in &info.tasks {
            // Parse estimate like "~2-5s" or "~0.1s"
            let (min, max) = Self::parse_duration_estimate(task.estimate);
            min_total += min;
            max_total += max;
        }

        (min_total, max_total)
    }

    /// Parse duration estimate string like "~2-5s" or "~0.5s"
    fn parse_duration_estimate(estimate: &str) -> (u32, u32) {
        // Remove ~ and s
        let stripped = estimate.trim_start_matches('~').trim_end_matches('s');

        if let Some(dash_pos) = stripped.find('-') {
            // Range like "2-5" or "0.5-2"
            let min_str = &stripped[..dash_pos];
            let max_str = &stripped[dash_pos + 1..];
            let min: f32 = min_str.parse().unwrap_or(0.0);
            let max: f32 = max_str.parse().unwrap_or(min);
            (min.ceil() as u32, max.ceil() as u32)
        } else {
            // Single value like "0.1" or "0.5"
            let val: f32 = stripped.parse().unwrap_or(0.0);
            let val_u32 = val.ceil() as u32;
            (val_u32, val_u32)
        }
    }

    /// Record a run duration for a workflow (in milliseconds)
    pub fn record_run(&mut self, path: PathBuf, duration_ms: u64) {
        let history = self.run_history.entry(path).or_default();
        history.push(duration_ms);
        // Keep only the last 10 runs
        if history.len() > 10 {
            history.remove(0);
        }
    }

    /// Get run history for a workflow path
    pub fn get_run_history(&self, path: &Path) -> Option<&Vec<u64>> {
        self.run_history.get(path)
    }

    /// Format run history as a sparkline string
    /// Uses Unicode block characters: ▁▂▃▄▅▆▇█
    fn format_sparkline(values: &[u64]) -> String {
        if values.is_empty() {
            return String::new();
        }

        let min = *values.iter().min().unwrap_or(&0);
        let max = *values.iter().max().unwrap_or(&1);
        let range = (max - min).max(1);

        let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

        values
            .iter()
            .map(|&v| {
                let normalized = ((v - min) * 7) / range;
                blocks[normalized.min(7) as usize]
            })
            .collect()
    }

    /// Format run history section for the info panel
    fn format_run_history_section(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push("RUN HISTORY".to_string());
        lines.push("─────────────────────────".to_string());

        if let Some(path) = self.standalone.selected_workflow() {
            if let Some(history) = self.get_run_history(path) {
                if !history.is_empty() {
                    // Sparkline visualization
                    let sparkline = Self::format_sparkline(history);
                    lines.push(format!("📊 {}", sparkline));

                    // Stats
                    let last = history.last().unwrap_or(&0);
                    let avg: u64 = history.iter().sum::<u64>() / history.len() as u64;
                    let min = history.iter().min().unwrap_or(&0);
                    let max = history.iter().max().unwrap_or(&0);

                    lines.push(format!(
                        "   Last: {:.1}s  Avg: {:.1}s",
                        *last as f64 / 1000.0,
                        avg as f64 / 1000.0
                    ));
                    lines.push(format!(
                        "   Min: {:.1}s   Max: {:.1}s",
                        *min as f64 / 1000.0,
                        *max as f64 / 1000.0
                    ));
                    lines.push(format!("   Runs: {}", history.len()));
                } else {
                    lines.push("   (no runs yet)".to_string());
                }
            } else {
                lines.push("   (no runs yet)".to_string());
            }
        } else {
            lines.push("   (no workflow selected)".to_string());
        }

        lines.push(String::new());
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_panel_cycle() {
        let panel = BrowserPanel::Tree;
        assert_eq!(panel.next(), BrowserPanel::DagPreview);
        assert_eq!(panel.next().next(), BrowserPanel::YamlPreview);
        assert_eq!(panel.next().next().next(), BrowserPanel::Info);
        assert_eq!(panel.next().next().next().next(), BrowserPanel::Tree);
    }

    #[test]
    fn test_browser_panel_numbers() {
        assert_eq!(BrowserPanel::Tree.number(), 1);
        assert_eq!(BrowserPanel::DagPreview.number(), 2);
        assert_eq!(BrowserPanel::YamlPreview.number(), 3);
        assert_eq!(BrowserPanel::Info.number(), 4);
    }

    #[test]
    fn test_workflow_info_from_valid_yaml() {
        // Note: Workflow struct doesn't have a `workflow:` field
        // The name is extracted from the schema (e.g., "nika/workflow@0.5" -> "workflow@0.5")
        // InvokeParams uses `mcp` field for server name, not `server`
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Generate something"
  - id: task2
    invoke:
      mcp: novanet
      tool: novanet_describe
"#;
        let info = WorkflowInfo::from_yaml(yaml);
        assert!(info.error.is_none(), "Parse error: {:?}", info.error);
        assert_eq!(info.name, "workflow@0.5"); // Extracted from schema
        assert_eq!(info.task_count, 2);
        assert_eq!(info.verb_counts.get("infer"), Some(&1));
        assert_eq!(info.verb_counts.get("invoke"), Some(&1));
    }

    #[test]
    fn test_workflow_info_from_invalid_yaml() {
        let yaml = "not: valid: yaml:";
        let info = WorkflowInfo::from_yaml(yaml);
        assert!(info.error.is_some());
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // VALIDATION STATUS TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_validation_status_icons() {
        assert_eq!(ValidationStatus::Valid.icon(), "✓");
        assert_eq!(ValidationStatus::Warning("test".to_string()).icon(), "⚠");
        assert_eq!(ValidationStatus::Error("test".to_string()).icon(), "✗");
        assert_eq!(ValidationStatus::Unknown.icon(), "?");
    }

    #[test]
    fn test_validation_status_colors() {
        assert_eq!(ValidationStatus::Valid.color(), Color::Green);
        assert_eq!(
            ValidationStatus::Warning("test".to_string()).color(),
            Color::Yellow
        );
        assert_eq!(
            ValidationStatus::Error("test".to_string()).color(),
            Color::Red
        );
        assert_eq!(ValidationStatus::Unknown.color(), Color::DarkGray);
    }

    #[test]
    fn test_validation_status_is_runnable() {
        assert!(ValidationStatus::Valid.is_runnable());
        assert!(ValidationStatus::Warning("warn".to_string()).is_runnable());
        assert!(!ValidationStatus::Error("err".to_string()).is_runnable());
        assert!(!ValidationStatus::Unknown.is_runnable());
    }

    #[test]
    fn test_validation_status_message() {
        assert!(ValidationStatus::Valid.message().is_none());
        assert_eq!(
            ValidationStatus::Warning("warn".to_string()).message(),
            Some("warn")
        );
        assert_eq!(
            ValidationStatus::Error("err".to_string()).message(),
            Some("err")
        );
        assert!(ValidationStatus::Unknown.message().is_none());
    }

    #[test]
    fn test_workflow_info_valid_status() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Generate something"
"#;
        let info = WorkflowInfo::from_yaml(yaml);
        assert_eq!(info.validation_status, ValidationStatus::Valid);
        assert_eq!(info.verb_summary, "infer");
    }

    #[test]
    fn test_workflow_info_warning_invoke_no_mcp() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    invoke:
      mcp: novanet
      tool: test_tool
"#;
        let info = WorkflowInfo::from_yaml(yaml);
        // invoke without MCP config should be a warning
        assert!(matches!(
            info.validation_status,
            ValidationStatus::Warning(_)
        ));
        if let ValidationStatus::Warning(msg) = &info.validation_status {
            assert!(msg.contains("invoke without MCP config"));
        }
    }

    #[test]
    fn test_workflow_info_warning_empty_tasks() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks: []
"#;
        let info = WorkflowInfo::from_yaml(yaml);
        assert!(matches!(
            info.validation_status,
            ValidationStatus::Warning(_)
        ));
        if let ValidationStatus::Warning(msg) = &info.validation_status {
            assert!(msg.contains("no tasks defined"));
        }
    }

    #[test]
    fn test_workflow_info_error_status() {
        let yaml = "invalid: yaml: syntax:";
        let info = WorkflowInfo::from_yaml(yaml);
        assert!(matches!(info.validation_status, ValidationStatus::Error(_)));
    }

    #[test]
    fn test_workflow_info_verb_summary_sorted() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: task1
    infer: "test"
  - id: task2
    exec: "echo"
  - id: task3
    invoke:
      mcp: novanet
      tool: test
"#;
        let info = WorkflowInfo::from_yaml(yaml);
        // Verbs should be sorted alphabetically
        assert_eq!(info.verb_summary, "exec,infer,invoke");
    }

    #[test]
    fn test_verb_icon_mapping() {
        use crate::ast::{ExecParams, InferParams, TaskAction};

        let infer_action = TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
            },
        };
        assert_eq!(WorkflowInfo::get_verb_icon(&infer_action), "🧠");

        let exec_action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo test".to_string(),
            },
        };
        assert_eq!(WorkflowInfo::get_verb_icon(&exec_action), "⚡");
    }

    #[test]
    fn test_duration_estimation() {
        use crate::ast::{ExecParams, InferParams, TaskAction};

        let infer_action = TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
            },
        };
        assert_eq!(WorkflowInfo::estimate_duration(&infer_action), "~2-5s");

        let exec_action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo test".to_string(),
            },
        };
        assert_eq!(WorkflowInfo::estimate_duration(&exec_action), "~0.1s");
    }

    #[test]
    fn test_ensure_tree_selection_visible_scroll_down() {
        use std::path::PathBuf;
        let mut view = BrowserView::new(PathBuf::from("."));

        // Simulate having many entries
        for _ in 0..30 {
            view.standalone
                .browser_entries
                .push(crate::tui::standalone::BrowserEntry {
                    path: PathBuf::from("test.nika.yaml"),
                    display_name: "test.nika.yaml".to_string(),
                    is_dir: false,
                    depth: 0,
                    expanded: false,
                });
        }

        // Set selection to item 20, scroll should adjust
        view.standalone.browser_index = 20;
        view.tree_scroll = 0;
        view.ensure_tree_selection_visible();

        // Scroll should have moved to make item 20 visible
        assert!(
            view.tree_scroll > 0,
            "Scroll should move down when selection is below viewport"
        );
    }

    #[test]
    fn test_ensure_tree_selection_visible_scroll_up() {
        use std::path::PathBuf;
        let mut view = BrowserView::new(PathBuf::from("."));

        // Simulate having many entries
        for _ in 0..30 {
            view.standalone
                .browser_entries
                .push(crate::tui::standalone::BrowserEntry {
                    path: PathBuf::from("test.nika.yaml"),
                    display_name: "test.nika.yaml".to_string(),
                    is_dir: false,
                    depth: 0,
                    expanded: false,
                });
        }

        // Set scroll to middle, selection at top
        view.standalone.browser_index = 2;
        view.tree_scroll = 10;
        view.ensure_tree_selection_visible();

        // Scroll should have moved up to make item 2 visible
        assert!(
            view.tree_scroll <= 2,
            "Scroll should move up when selection is above viewport"
        );
    }

    #[test]
    fn test_ensure_tree_selection_visible_no_scroll_needed() {
        use std::path::PathBuf;
        let mut view = BrowserView::new(PathBuf::from("."));

        // Simulate having a few entries
        for _ in 0..10 {
            view.standalone
                .browser_entries
                .push(crate::tui::standalone::BrowserEntry {
                    path: PathBuf::from("test.nika.yaml"),
                    display_name: "test.nika.yaml".to_string(),
                    is_dir: false,
                    depth: 0,
                    expanded: false,
                });
        }

        // Selection in middle, scroll at 0
        view.standalone.browser_index = 5;
        view.tree_scroll = 0;
        view.ensure_tree_selection_visible();

        // Scroll should not change as item 5 is visible
        assert_eq!(
            view.tree_scroll, 0,
            "Scroll should not change when selection is visible"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // TASK SUMMARY EXTRACTION TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_task_summary_extraction() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: fetch_data
    fetch:
      url: https://example.com
  - id: process
    infer: "Process the data"
  - id: execute
    exec: "echo done"
flows:
  - source: fetch_data
    target: process
  - source: process
    target: execute
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Verify tasks are extracted
        assert_eq!(info.tasks.len(), 3);

        // Verify fetch_data task
        let fetch_task = &info.tasks[0];
        assert_eq!(fetch_task.id, "fetch_data");
        assert_eq!(fetch_task.icon, "🔗");
        assert_eq!(fetch_task.verb, "fetch");
        assert_eq!(fetch_task.estimate, "~0.5s");
        assert!(fetch_task.depends_on.is_empty());

        // Verify process task (depends on fetch_data)
        let process_task = &info.tasks[1];
        assert_eq!(process_task.id, "process");
        assert_eq!(process_task.icon, "🧠");
        assert_eq!(process_task.verb, "infer");
        assert_eq!(process_task.estimate, "~2-5s");
        assert_eq!(process_task.depends_on, vec!["fetch_data"]);

        // Verify execute task (depends on process)
        let execute_task = &info.tasks[2];
        assert_eq!(execute_task.id, "execute");
        assert_eq!(execute_task.icon, "⚡");
        assert_eq!(execute_task.verb, "exec");
        assert_eq!(execute_task.estimate, "~0.1s");
        assert_eq!(execute_task.depends_on, vec!["process"]);
    }

    #[test]
    fn test_task_summary_multiple_dependencies() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: source_a
    exec: "echo a"
  - id: source_b
    exec: "echo b"
  - id: combine
    infer: "Combine A and B"
flows:
  - source: [source_a, source_b]
    target: combine
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Verify combine task has both dependencies
        let combine_task = &info.tasks[2];
        assert_eq!(combine_task.id, "combine");
        assert_eq!(combine_task.depends_on.len(), 2);
        assert!(combine_task.depends_on.contains(&"source_a".to_string()));
        assert!(combine_task.depends_on.contains(&"source_b".to_string()));
    }

    #[test]
    fn test_task_summary_with_invoke_and_agent() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: call_tool
    invoke:
      mcp: novanet
      tool: novanet_describe
  - id: run_agent
    agent:
      prompt: "Do something"
      mcp: [novanet]
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Verify invoke task
        let invoke_task = &info.tasks[0];
        assert_eq!(invoke_task.id, "call_tool");
        assert_eq!(invoke_task.icon, "📥");
        assert_eq!(invoke_task.verb, "invoke");
        assert_eq!(invoke_task.estimate, "~0.5-2s");

        // Verify agent task
        let agent_task = &info.tasks[1];
        assert_eq!(agent_task.id, "run_agent");
        assert_eq!(agent_task.icon, "🤖");
        assert_eq!(agent_task.verb, "agent");
        assert_eq!(agent_task.estimate, "~5-30s");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // DURATION ESTIMATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_duration_estimate_range() {
        // Range like "~2-5s"
        let (min, max) = BrowserView::parse_duration_estimate("~2-5s");
        assert_eq!(min, 2);
        assert_eq!(max, 5);

        // Range with decimals
        let (min, max) = BrowserView::parse_duration_estimate("~0.5-2s");
        assert_eq!(min, 1); // 0.5 ceil = 1
        assert_eq!(max, 2);
    }

    #[test]
    fn test_parse_duration_estimate_single() {
        // Single value
        let (min, max) = BrowserView::parse_duration_estimate("~0.1s");
        assert_eq!(min, 1);
        assert_eq!(max, 1);

        let (min, max) = BrowserView::parse_duration_estimate("~0.5s");
        assert_eq!(min, 1);
        assert_eq!(max, 1);
    }

    #[test]
    fn test_estimate_total_duration() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    exec: "echo"
  - id: task2
    infer: "test"
  - id: task3
    exec: "echo done"
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // exec: ~0.1s (1s ceil), infer: ~2-5s, exec: ~0.1s (1s ceil)
        let (min, max) = BrowserView::estimate_total_duration(&info);
        // min: 1 + 2 + 1 = 4
        // max: 1 + 5 + 1 = 7
        assert_eq!(min, 4);
        assert_eq!(max, 7);
    }

    #[test]
    fn test_verb_icon_for_name() {
        assert_eq!(BrowserView::verb_icon_for_name("infer"), "🧠");
        assert_eq!(BrowserView::verb_icon_for_name("exec"), "⚡");
        assert_eq!(BrowserView::verb_icon_for_name("fetch"), "🔗");
        assert_eq!(BrowserView::verb_icon_for_name("invoke"), "📥");
        assert_eq!(BrowserView::verb_icon_for_name("agent"), "🤖");
        assert_eq!(BrowserView::verb_icon_for_name("unknown"), "  ");
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // RUN HISTORY TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_record_run_history() {
        use std::path::PathBuf;
        let mut view = BrowserView::new(PathBuf::from("."));
        let path = PathBuf::from("test.nika.yaml");

        // Record some runs
        view.record_run(path.clone(), 1500);
        view.record_run(path.clone(), 2000);
        view.record_run(path.clone(), 1800);

        let history = view.get_run_history(&path);
        assert!(history.is_some());
        let history = history.unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0], 1500);
        assert_eq!(history[1], 2000);
        assert_eq!(history[2], 1800);
    }

    #[test]
    fn test_run_history_max_entries() {
        use std::path::PathBuf;
        let mut view = BrowserView::new(PathBuf::from("."));
        let path = PathBuf::from("test.nika.yaml");

        // Record more than 10 runs
        for i in 0..15 {
            view.record_run(path.clone(), (i * 1000) as u64);
        }

        let history = view.get_run_history(&path);
        assert!(history.is_some());
        let history = history.unwrap();
        // Should only keep last 10
        assert_eq!(history.len(), 10);
        // First entry should be 5000 (5th run), not 0 (1st run)
        assert_eq!(history[0], 5000);
        // Last entry should be 14000 (15th run)
        assert_eq!(history[9], 14000);
    }

    #[test]
    fn test_format_sparkline_basic() {
        let values = vec![1000, 2000, 3000, 4000, 5000];
        let sparkline = BrowserView::format_sparkline(&values);

        // Should have 5 characters
        assert_eq!(sparkline.chars().count(), 5);
        // First should be lowest block, last should be highest
        assert!(sparkline.starts_with('▁'));
        assert!(sparkline.ends_with('█'));
    }

    #[test]
    fn test_format_sparkline_uniform() {
        let values = vec![1000, 1000, 1000];
        let sparkline = BrowserView::format_sparkline(&values);

        // All values same, so all should be same block (lowest due to range=0 handling)
        assert_eq!(sparkline.chars().count(), 3);
        // With uniform values, all blocks should be the same
        let chars: Vec<char> = sparkline.chars().collect();
        assert_eq!(chars[0], chars[1]);
        assert_eq!(chars[1], chars[2]);
    }

    #[test]
    fn test_format_sparkline_empty() {
        let values: Vec<u64> = vec![];
        let sparkline = BrowserView::format_sparkline(&values);
        assert!(sparkline.is_empty());
    }

    #[test]
    fn test_format_sparkline_single() {
        let values = vec![5000];
        let sparkline = BrowserView::format_sparkline(&values);
        assert_eq!(sparkline.chars().count(), 1);
    }

    #[test]
    fn test_format_sparkline_varied() {
        // Test with varied values to verify gradient
        let values = vec![1000, 4000, 2000, 8000, 5000, 3000];
        let sparkline = BrowserView::format_sparkline(&values);

        assert_eq!(sparkline.chars().count(), 6);
        // The 4th value (8000) should be the highest block
        let chars: Vec<char> = sparkline.chars().collect();
        assert_eq!(chars[3], '█'); // 8000 is max
                                   // The 1st value (1000) should be the lowest block
        assert_eq!(chars[0], '▁'); // 1000 is min
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // MCP TOOLS PREVIEW TESTS
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mcp_tools_extraction_from_invoke() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: describe
    invoke:
      mcp: novanet
      tool: novanet_describe
  - id: traverse
    invoke:
      mcp: novanet
      tool: novanet_traverse
  - id: describe_again
    invoke:
      mcp: novanet
      tool: novanet_describe
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Should have novanet server with tools
        assert!(info.mcp_tools.contains_key("novanet"));
        let tools = info.mcp_tools.get("novanet").unwrap();

        // Should have 2 unique tools
        assert_eq!(tools.len(), 2);

        // novanet_describe called twice (count=2), should be first due to descending sort
        let describe_tool = tools.iter().find(|t| t.tool == "novanet_describe");
        assert!(describe_tool.is_some());
        assert_eq!(describe_tool.unwrap().count, 2);

        // novanet_traverse called once
        let traverse_tool = tools.iter().find(|t| t.tool == "novanet_traverse");
        assert!(traverse_tool.is_some());
        assert_eq!(traverse_tool.unwrap().count, 1);
    }

    #[test]
    fn test_mcp_resources_extraction_from_invoke() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: read_entity
    invoke:
      mcp: novanet
      resource: neo4j://entity/qr-code
  - id: read_locale
    invoke:
      mcp: novanet
      resource: neo4j://locale/fr-FR
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Should have novanet server with resources
        assert!(info.mcp_resources.contains_key("novanet"));
        let resources = info.mcp_resources.get("novanet").unwrap();

        // Should have 2 resources
        assert_eq!(resources.len(), 2);
        assert!(resources.contains(&"neo4j://entity/qr-code".to_string()));
        assert!(resources.contains(&"neo4j://locale/fr-FR".to_string()));
    }

    #[test]
    fn test_mcp_tools_extraction_from_agent() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
  tools:
    command: other_test
tasks:
  - id: run_agent
    agent:
      prompt: "Do something"
      mcp: [novanet, tools]
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Agent tasks should register their servers (but without specific tools)
        assert!(info.mcp_tools.contains_key("novanet"));
        assert!(info.mcp_tools.contains_key("tools"));

        // No specific tools known for agents (empty vectors)
        assert!(info.mcp_tools.get("novanet").unwrap().is_empty());
        assert!(info.mcp_tools.get("tools").unwrap().is_empty());
    }

    #[test]
    fn test_mcp_tools_multiple_servers() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test1
  filesystem:
    command: test2
tasks:
  - id: describe
    invoke:
      mcp: novanet
      tool: novanet_describe
  - id: read_file
    invoke:
      mcp: filesystem
      tool: read_file
  - id: write_file
    invoke:
      mcp: filesystem
      tool: write_file
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Should have both servers
        assert!(info.mcp_tools.contains_key("novanet"));
        assert!(info.mcp_tools.contains_key("filesystem"));

        // novanet has 1 tool
        let novanet_tools = info.mcp_tools.get("novanet").unwrap();
        assert_eq!(novanet_tools.len(), 1);
        assert_eq!(novanet_tools[0].tool, "novanet_describe");

        // filesystem has 2 tools
        let fs_tools = info.mcp_tools.get("filesystem").unwrap();
        assert_eq!(fs_tools.len(), 2);
    }

    #[test]
    fn test_mcp_resources_deduplication() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: read_entity1
    invoke:
      mcp: novanet
      resource: neo4j://entity/qr-code
  - id: read_entity2
    invoke:
      mcp: novanet
      resource: neo4j://entity/qr-code
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // Same resource accessed twice should only appear once
        let resources = info.mcp_resources.get("novanet").unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0], "neo4j://entity/qr-code");
    }

    #[test]
    fn test_mcp_tools_empty_workflow() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: simple
    exec: "echo hello"
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        // No MCP usage
        assert!(info.mcp_tools.is_empty());
        assert!(info.mcp_resources.is_empty());
    }

    #[test]
    fn test_mcp_tools_sorting_by_count() {
        let yaml = r#"
schema: nika/workflow@0.5
mcp:
  novanet:
    command: test
tasks:
  - id: t1
    invoke:
      mcp: novanet
      tool: tool_a
  - id: t2
    invoke:
      mcp: novanet
      tool: tool_b
  - id: t3
    invoke:
      mcp: novanet
      tool: tool_b
  - id: t4
    invoke:
      mcp: novanet
      tool: tool_b
  - id: t5
    invoke:
      mcp: novanet
      tool: tool_c
  - id: t6
    invoke:
      mcp: novanet
      tool: tool_c
"#;
        let info = WorkflowInfo::from_yaml(yaml);

        let tools = info.mcp_tools.get("novanet").unwrap();

        // Should be sorted by count descending
        // tool_b: 3, tool_c: 2, tool_a: 1
        assert_eq!(tools[0].tool, "tool_b");
        assert_eq!(tools[0].count, 3);
        assert_eq!(tools[1].tool, "tool_c");
        assert_eq!(tools[1].count, 2);
        assert_eq!(tools[2].tool, "tool_a");
        assert_eq!(tools[2].count, 1);
    }
}
