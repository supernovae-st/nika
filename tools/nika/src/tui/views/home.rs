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

use crate::serde_yaml;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nucleo::{Config, Matcher, Utf32Str};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Widget},
    Frame,
};

use super::trait_view::View;
use super::ViewAction;
use crate::ast::{TaskAction, Workflow};

/// Preview mode for DAG preview panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMode {
    /// DAG visualization (default)
    #[default]
    Dag,
    /// Raw YAML text with verb-colored syntax highlighting
    Yaml,
}
use crate::tui::standalone::{BrowserEntry, StandaloneState};
use crate::tui::state::TuiState;
use crate::tui::theme::{TaskStatus, Theme, VerbColor};
use crate::tui::views::TuiView;
use crate::tui::widgets::{
    tree::{TreeAction, TreeState},
    AnimatedLatencySparkline, DagAscii, MatrixRain, NodeBoxData, NodeBoxMode, SparklineAnimation,
    Timeline, TimelineEntry,
};

/// Home view state
pub struct HomeView {
    /// File browser state (reuses StandaloneState from standalone.rs)
    pub standalone: StandaloneState,
    /// List state for file selection (ratatui ListState)
    pub list_state: ListState,
    /// Whether history bar is expanded
    pub history_expanded: bool,
    /// Fuzzy search query (v0.6.1)
    pub search_query: String,
    /// Whether search mode is active
    pub search_active: bool,
    /// Filtered indices from fuzzy search
    filtered_indices: Vec<usize>,
    /// Fuzzy matcher instance
    matcher: Matcher,
    /// Cached: whether .nika directory exists (avoid syscall per frame)
    has_nika_dir: bool,
    /// Cached parsed workflow (PERF: avoid re-parsing YAML every frame)
    cached_workflow: RefCell<Option<Workflow>>,
    /// Content hash for cache invalidation
    cached_content_hash: Cell<u64>,
    /// DAG expanded mode toggle
    pub dag_expanded: bool,
    /// Preview mode: DAG visualization or verb-colored YAML
    pub preview_mode: PreviewMode,
    /// Animation frame counter (0-255, wraps)
    pub frame: u8,
    // === v0.20: Tree Widget ===
    /// Tree state for selection and expansion
    pub tree_state: TreeState,
    // === v0.9.1: Matrix Rain Effect ===
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
        // v0.20: Initialize tree state with visible nodes count
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
            // v0.9.1: Matrix Rain starts visible and fades
            rain_opacity: 1.0,
            rain_fading: true,
            matrix_effect_enabled: true,
        }
    }

    /// Tick animation frame (called from main loop)
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        // v0.9.1: Tick matrix rain fade effect
        if self.rain_fading && self.rain_opacity > 0.0 {
            self.rain_opacity = (self.rain_opacity - 0.04).max(0.0); // Smooth fade ~2s
        }
    }

    /// Trigger matrix rain effect with fade-out
    #[allow(dead_code)]
    pub fn trigger_rain_effect(&mut self) {
        self.rain_opacity = 0.6; // Start subtle
        self.rain_fading = true;
    }

    /// Update filtered indices based on search query
    fn update_filter(&mut self) {
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
    fn filtered_entries(&self) -> Vec<&BrowserEntry> {
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
    /// BUG FIX: Use filtered_indices to map list selection to actual entry index
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

    /// Handle tree navigation action (v0.20 tree widget)
    ///
    /// Maps TreeAction variants to HomeView operations.
    /// Returns Some(ViewAction) if action was handled, None otherwise.
    fn handle_tree_action(&mut self, action: TreeAction) -> Option<ViewAction> {
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
                    }
                    // TODO: move to parent when implemented
                }
                Some(ViewAction::None)
            }
            TreeAction::Right => {
                // Expand folder if collapsed, otherwise move into
                if let Some(entry) = self.selected_entry() {
                    if entry.is_dir && !entry.expanded {
                        self.toggle_folder();
                    }
                    // TODO: move into children when implemented
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

    /// Render the files panel (left 40%)
    fn render_files(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Split area for search bar if active
        let (search_area, list_area) = if self.search_active {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(5)])
                .split(area);
            (Some(chunks[0]), chunks[1])
        } else {
            (None, area)
        };

        // Render search bar if active
        if let Some(search_area) = search_area {
            let search_text = format!("🔍 {}", self.search_query);
            let search_bar = Paragraph::new(search_text)
                .style(Style::default().fg(theme.text_primary))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" SEARCH (Esc to cancel) ")
                        .border_style(Style::default().fg(theme.highlight)),
                );
            frame.render_widget(search_bar, search_area);
        }

        // Use filtered entries with v0.12 icons (📁 folder, 📄 file)
        let items: Vec<ListItem> = self
            .filtered_entries()
            .iter()
            .map(|entry| {
                let icon = if entry.is_dir {
                    if entry.expanded {
                        "📂 " // Open folder
                    } else {
                        "📁 " // Closed folder
                    }
                } else {
                    "📄 " // Workflow file
                };
                let indent = "  ".repeat(entry.depth);
                let name = entry
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| entry.display_name.clone());
                ListItem::new(format!("{}{}{}", indent, icon, name))
            })
            .collect();

        // Build title with project root and .nika indicator
        // NOTE: has_nika_dir is cached at HomeView creation to avoid syscall per frame
        let project_name = self
            .standalone
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "~".to_string());
        let nika_marker = if self.has_nika_dir { " ◆" } else { "" };

        let title = if self.search_active && !self.search_query.is_empty() {
            format!(
                " 📁 {}{} ({} matches) ",
                project_name,
                nika_marker,
                self.filtered_indices.len()
            )
        } else {
            format!(" 📁 {}{} ", project_name, nika_marker)
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(theme.border_normal)),
            )
            .highlight_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, list_area, &mut self.list_state.clone());
    }

    // =========================================================================
    // DAG PREVIEW HELPERS (v0.7.2+)
    // =========================================================================

    /// Compute a simple hash of content for cache invalidation
    fn content_hash(content: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Extract dependencies from Flow objects
    ///
    /// Returns a map: target_task_id -> [source_task_ids]
    fn extract_flow_dependencies(wf: &Workflow) -> HashMap<String, Vec<String>> {
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();

        for flow in &wf.flows {
            let sources = flow.source.as_vec();
            let targets = flow.target.as_vec();

            // Each target depends on all sources
            for target in &targets {
                let entry = deps.entry(target.to_string()).or_default();
                for source in &sources {
                    if !entry.contains(&source.to_string()) {
                        entry.push(source.to_string());
                    }
                }
            }
        }

        deps
    }

    /// Get VerbColor from task action
    fn task_verb_color(task: &crate::ast::Task) -> VerbColor {
        match &task.action {
            TaskAction::Infer { .. } => VerbColor::Infer,
            TaskAction::Exec { .. } => VerbColor::Exec,
            TaskAction::Fetch { .. } => VerbColor::Fetch,
            TaskAction::Invoke { .. } => VerbColor::Invoke,
            TaskAction::Agent { .. } => VerbColor::Agent,
        }
    }

    /// Render the preview panel (right 60%) with DAG visualization or YAML
    ///
    /// WOW UX: Enhanced with styled mode badge and keyboard hints footer
    fn render_preview(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // WOW UX: Build styled title with mode badge
        let title: Line = match self.preview_mode {
            PreviewMode::Dag => {
                let expand_hint = if self.dag_expanded { "━" } else { "+" };
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        " DAG ",
                        Style::default()
                            .fg(Color::White)
                            .bg(Color::Indexed(99)) // Violet background
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("[E]{}", expand_hint),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::raw(" "),
                    Span::styled("[D]→YAML", Style::default().fg(theme.text_muted)),
                    Span::raw(" "),
                ])
            }
            PreviewMode::Yaml => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    " YAML ",
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Indexed(36)) // Cyan background
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" verb-colored "),
                Span::styled("[D]→DAG", Style::default().fg(theme.text_muted)),
                Span::raw(" "),
            ]),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Check if we have a valid file selected
        let content: &str = if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                // WOW UX: Enhanced directory selected hint with icon
                let hint_lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "📁",
                        Style::default().add_modifier(Modifier::DIM),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Select a .nika.yaml file",
                        Style::default().fg(theme.text_muted),
                    )),
                    Line::from(Span::styled(
                        "to preview task graph",
                        Style::default().fg(theme.text_muted),
                    )),
                ];
                let paragraph =
                    Paragraph::new(hint_lines).alignment(ratatui::layout::Alignment::Center);
                frame.render_widget(paragraph, inner);
                return;
            }
            &self.standalone.preview_content
        } else {
            // WOW UX: Enhanced no file selected state
            let hint_lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "📄",
                    Style::default().add_modifier(Modifier::DIM),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "No file selected",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Use ↑↓ or j/k to navigate",
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::DIM),
                )),
            ];
            let paragraph =
                Paragraph::new(hint_lines).alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, inner);
            return;
        };

        // YAML mode: render verb-colored text directly (no DAG parsing needed)
        if self.preview_mode == PreviewMode::Yaml {
            Self::render_yaml_preview(frame, inner, content, theme);
            return;
        }

        // PERF: Compute content hash to check if we need to re-parse
        let current_hash = Self::content_hash(content);
        let cached_hash = self.cached_content_hash.get();

        // Update cache if content changed
        if current_hash != cached_hash {
            let workflow: Result<Workflow, _> = serde_yaml::from_str(content);
            *self.cached_workflow.borrow_mut() = workflow.ok();
            self.cached_content_hash.set(current_hash);
        }

        // Use cached workflow (avoids parsing every frame)
        let cached = self.cached_workflow.borrow();
        match cached.as_ref() {
            Some(wf) => {
                if wf.tasks.is_empty() {
                    let paragraph = Paragraph::new("(no tasks defined)")
                        .style(Style::default().fg(theme.text_muted));
                    frame.render_widget(paragraph, inner);
                    return;
                }

                // Build dependency map from Flow objects
                let deps = Self::extract_flow_dependencies(wf);

                // Convert tasks to NodeBoxData
                let nodes: Vec<NodeBoxData> = wf
                    .tasks
                    .iter()
                    .map(|task| {
                        let verb = Self::task_verb_color(task.as_ref());
                        let mut node =
                            NodeBoxData::new(&task.id, verb).with_status(TaskStatus::Pending);

                        // Add prompt preview if available (for infer/agent)
                        if let Some(prompt) = Self::extract_prompt_preview(task.as_ref()) {
                            node = node.with_prompt_preview(prompt);
                        }

                        // Add for_each info if present
                        if let Some(for_each) = &task.for_each {
                            if let Some(arr) = for_each.as_array() {
                                node = node.with_for_each_count(arr.len());
                            }
                        }

                        node
                    })
                    .collect();

                let mode = if self.dag_expanded {
                    NodeBoxMode::Expanded
                } else {
                    NodeBoxMode::Minimal
                };

                // Create and render DagAscii widget
                let widget = DagAscii::new(&nodes)
                    .with_dependencies(deps)
                    .mode(mode)
                    .scroll(0, 0);

                // Render to buffer (DagAscii implements Widget)
                let buf = frame.buffer_mut();
                widget.render(inner, buf);
            }
            None => {
                // YAML doesn't parse as workflow - show text fallback with line numbers
                let lines: Vec<Line> = content
                    .lines()
                    .take(inner.height as usize) // Limit to visible area
                    .enumerate()
                    .map(|(i, line)| {
                        Line::from(vec![
                            Span::styled(
                                format!("{:4} │ ", i + 1),
                                Style::default().fg(theme.text_muted),
                            ),
                            Span::raw(line),
                        ])
                    })
                    .collect();

                let paragraph = Paragraph::new(lines);
                frame.render_widget(paragraph, inner);
            }
        }
    }

    /// Get VerbColor for YAML line highlighting
    /// Returns the VerbColor if the line starts a verb block (infer:, exec:, etc.)
    fn get_line_verb_color(line: &str) -> Option<VerbColor> {
        let trimmed = line.trim();

        // Check for verb declarations (exact match with VerbColor)
        if trimmed.starts_with("infer:") {
            Some(VerbColor::Infer) // ⚡ Infer - violet
        } else if trimmed.starts_with("exec:") {
            Some(VerbColor::Exec) // 📟 Exec - amber
        } else if trimmed.starts_with("fetch:") {
            Some(VerbColor::Fetch) // 🛰️ Fetch - cyan
        } else if trimmed.starts_with("invoke:") {
            Some(VerbColor::Invoke) // 🔌 Invoke - emerald
        } else if trimmed.starts_with("agent:") {
            Some(VerbColor::Agent) // 🐔 Agent - rose
        } else {
            None
        }
    }

    /// Render YAML content with verb-colored syntax highlighting
    ///
    /// Each verb (infer, exec, fetch, invoke, agent) gets its canonical VerbColor.
    /// Lines within a verb block inherit the color until a new top-level key appears.
    fn render_yaml_preview(frame: &mut Frame, area: Rect, content: &str, theme: &Theme) {
        // Track current verb context for indented lines
        let mut current_verb: Option<VerbColor> = None;

        let lines: Vec<Line> = content
            .lines()
            .take(area.height as usize) // Limit to visible area
            .enumerate()
            .map(|(i, line)| {
                // Check if line starts a new verb block
                if let Some(verb) = Self::get_line_verb_color(line) {
                    current_verb = Some(verb);
                } else if !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty() {
                    // Non-indented, non-empty line resets verb context
                    current_verb = None;
                }

                // Build styled line with verb coloring
                // Use Cow to avoid allocations for static strings (performance)
                let (prefix_icon, line_style): (Cow<'static, str>, Style) = match current_verb {
                    Some(verb) => {
                        // Verb icon as prefix for verb lines only
                        let icon: Cow<'static, str> = if Self::get_line_verb_color(line).is_some() {
                            Cow::Owned(format!("{} ", verb.icon()))
                        } else {
                            Cow::Borrowed("  ") // Static str, no allocation
                        };
                        (icon, Style::default().fg(verb.rgb()))
                    }
                    None => (
                        Cow::Borrowed("  "), // Static str, no allocation
                        Style::default().fg(theme.text_secondary),
                    ),
                };

                Line::from(vec![
                    Span::styled(
                        format!("{:4} │", i + 1),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::styled(prefix_icon, line_style),
                    Span::styled(line.to_string(), line_style),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    /// Extract prompt preview from task for DAG display
    fn extract_prompt_preview(task: &crate::ast::Task) -> Option<String> {
        match &task.action {
            TaskAction::Infer { infer } => {
                // Truncate to ~50 chars for preview
                let prompt = &infer.prompt;
                let preview: String = prompt.chars().take(50).collect();
                if prompt.len() > 50 {
                    Some(format!("{}...", preview))
                } else {
                    Some(preview)
                }
            }
            TaskAction::Agent { agent } => {
                let prompt = &agent.prompt;
                let preview: String = prompt.chars().take(50).collect();
                if prompt.len() > 50 {
                    Some(format!("{}...", preview))
                } else {
                    Some(preview)
                }
            }
            TaskAction::Exec { exec } => {
                let command = &exec.command;
                let preview: String = command.chars().take(40).collect();
                if command.len() > 40 {
                    Some(format!("$ {}...", preview))
                } else {
                    Some(format!("$ {}", command))
                }
            }
            TaskAction::Fetch { fetch } => Some(fetch.url.clone()),
            TaskAction::Invoke { invoke } => {
                invoke.tool.clone().or_else(|| invoke.resource.clone())
            }
        }
    }

    /// Render the history bar (bottom, toggleable)
    /// v0.8.2: Uses Timeline widget for visual history display
    fn render_history(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let toggle_hint = if self.history_expanded { "^" } else { "v" };
        let title = format!(" HISTORY [h] {} ", toggle_hint);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.standalone.history.is_empty() {
            let empty_msg =
                Paragraph::new(" No history yet ").style(Style::default().fg(theme.text_muted));
            frame.render_widget(empty_msg, inner);
            return;
        }

        // Convert HistoryEntry to TimelineEntry for visual display
        let max_items = if self.history_expanded { 10 } else { 5 };
        let entries: Vec<TimelineEntry> = self
            .standalone
            .history
            .iter()
            .rev() // Most recent first
            .take(max_items)
            .enumerate()
            .map(|(i, h)| {
                let name = h
                    .workflow_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let status = if h.success {
                    TaskStatus::Success
                } else {
                    TaskStatus::Failed
                };

                let mut entry = TimelineEntry::new(name, status).with_duration(h.duration_ms);
                if i == 0 {
                    entry = entry.current(); // Mark most recent
                }
                entry
            })
            .collect();

        // Collect latency data for sparkline (v0.12: AnimatedLatencySparkline)
        let latency_data: Vec<u64> = self
            .standalone
            .history
            .iter()
            .rev()
            .take(max_items)
            .map(|h| h.duration_ms)
            .collect();

        // Calculate total elapsed time from all runs
        let total_ms: u64 = latency_data.iter().sum();

        // v0.12: Split inner area - Timeline (70%) | Sparkline (30%)
        let history_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
            .split(inner);

        // Left: Timeline
        let timeline = Timeline::new(&entries)
            .elapsed(total_ms)
            .with_theme(theme)
            .with_frame(self.frame);
        frame.render_widget(timeline, history_chunks[0]);

        // Right: Latency sparkline (v0.12)
        if !latency_data.is_empty() {
            let sparkline = AnimatedLatencySparkline::new(&latency_data)
                .frame(self.frame)
                .animation(SparklineAnimation::Pulse)
                .warn_threshold(1000) // 1s warning
                .error_threshold(5000); // 5s error
            frame.render_widget(sparkline, history_chunks[1]);
        }
    }
}

impl View for HomeView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // v0.9.1: Matrix Rain background effect (full screen, renders FIRST)
        if self.matrix_effect_enabled && self.rain_opacity > 0.05 {
            let rain_area = Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(2),
            };
            MatrixRain::new()
                .frame(self.frame)
                .density(0.08) // Very subtle, smooth
                .opacity(self.rain_opacity)
                .with_mascots(true)
                .render(rain_area, frame.buffer_mut());
        }

        // Layout: Files (40%) | Preview (60%) above, History bar below
        let history_height = if self.history_expanded { 6 } else { 3 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(history_height)])
            .split(area);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);

        // Files panel (left 40%)
        self.render_files(frame, main_chunks[0], theme);

        // Preview panel (right 60%)
        self.render_preview(frame, main_chunks[1], theme);

        // History bar (bottom)
        self.render_history(frame, chunks[1], theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        // Handle search mode input
        if self.search_active {
            match key.code {
                // Cancel search
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.update_filter();
                    return ViewAction::None;
                }
                // Confirm selection
                KeyCode::Enter => {
                    self.search_active = false;
                    if let Some(entry) = self.selected_entry() {
                        if entry.is_dir {
                            self.toggle_folder();
                            return ViewAction::None;
                        } else {
                            return ViewAction::RunWorkflow(entry.path.clone());
                        }
                    }
                    return ViewAction::None;
                }
                // Navigation in search mode
                KeyCode::Up => {
                    self.select_prev();
                    return ViewAction::None;
                }
                KeyCode::Down => {
                    self.select_next();
                    return ViewAction::None;
                }
                // Backspace to delete
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.update_filter();
                    return ViewAction::None;
                }
                // Type character
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.update_filter();
                    return ViewAction::None;
                }
                _ => return ViewAction::None,
            }
        }

        // Normal mode key handling
        //
        // v0.20: Priority order:
        // 1. Reserved keys (s, T, /, e, E, d, h, c, number keys)
        // 2. TreeAction navigation (j/k, Up/Down, Left/Right, Enter, g/G, etc.)
        // 3. Fallback to ViewAction::None

        // 1. Reserved keys - check these first before TreeAction
        match key.code {
            // 's' opens Settings view
            KeyCode::Char('s') => return ViewAction::OpenSettings,

            // Shift+T or Ctrl+t toggles theme
            KeyCode::Char('T') => return ViewAction::ToggleTheme,
            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return ViewAction::ToggleTheme;
            }

            // Start search with / or Ctrl+P
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
                return ViewAction::None;
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_active = true;
                self.search_query.clear();
                return ViewAction::None;
            }

            // Edit: open in Studio (e key)
            KeyCode::Char('e') if !key.modifiers.contains(KeyModifiers::SHIFT) => {
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return ViewAction::OpenInStudio(entry.path.clone());
                    }
                }
                return ViewAction::None;
            }

            // Toggle DAG expand mode (Shift+E)
            KeyCode::Char('E') => {
                self.dag_expanded = !self.dag_expanded;
                return ViewAction::None;
            }

            // Toggle preview mode: DAG ↔ YAML (D key, not Ctrl+D)
            KeyCode::Char('d') | KeyCode::Char('D')
                if !key.modifiers.contains(KeyModifiers::SHIFT)
                    && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.preview_mode = match self.preview_mode {
                    PreviewMode::Dag => PreviewMode::Yaml,
                    PreviewMode::Yaml => PreviewMode::Dag,
                };
                return ViewAction::None;
            }

            // 'h' toggles history (reserved, not TreeAction::Left)
            KeyCode::Char('h') => {
                self.history_expanded = !self.history_expanded;
                return ViewAction::None;
            }

            // Chat overlay toggle
            KeyCode::Char('c') => return ViewAction::ToggleChatOverlay,

            // View switching: number keys (4-view architecture v0.22)
            // 1=Studio, 2=Runner, 3=Chat, 4=Settings
            KeyCode::Char('1') => return ViewAction::SwitchView(TuiView::Studio),
            KeyCode::Char('2') => return ViewAction::SwitchView(TuiView::Runner),
            KeyCode::Char('3') => return ViewAction::SwitchView(TuiView::Chat),
            KeyCode::Char('4') => return ViewAction::SwitchView(TuiView::Settings),

            // v0.11.0: Validate selected workflow
            KeyCode::Char('v') => {
                if let Some(entry) = self.selected_entry() {
                    if !entry.is_dir {
                        return ViewAction::ValidateWorkflow(entry.path.clone());
                    }
                }
                return ViewAction::None;
            }

            // Tab: handled at app level for view cycling
            KeyCode::Tab => return ViewAction::None,

            _ => {} // Fall through to TreeAction handling
        }

        // 2. TreeAction navigation (v0.20)
        // Convert key to TreeAction and delegate to handle_tree_action
        if let Some(action) = TreeAction::from_key_event(key) {
            if let Some(view_action) = self.handle_tree_action(action) {
                return view_action;
            }
        }

        // 3. Fallback
        ViewAction::None
    }

    fn status_line(&self, _state: &TuiState) -> String {
        let workflow_count = self
            .standalone
            .browser_entries
            .iter()
            .filter(|e| !e.is_dir)
            .count();
        let history_count = self.standalone.history.len();
        format!(
            "{} workflows | {} in history",
            workflow_count, history_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_home_view_new_creates_valid_state() {
        let view = HomeView::new(PathBuf::from("."));
        assert!(!view.history_expanded);
        // TreeState selection index may or may not be set depending on entries
        assert!(
            view.tree_state.selection_index().is_none()
                || view.tree_state.selection_index().is_some()
        );
    }

    #[test]
    fn test_home_view_select_navigation() {
        let mut view = HomeView::new(PathBuf::from("."));

        // Add some mock entries for testing
        view.standalone.browser_entries.clear();
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test1.nika.yaml"),
            &PathBuf::from("."),
        ));
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test2.nika.yaml"),
            &PathBuf::from("."),
        ));
        // Set up filtered indices to match entries
        view.filtered_indices = vec![0, 1];
        view.tree_state.set_selection_index(Some(0));

        // Navigate down
        view.select_next();
        assert_eq!(view.tree_state.selection_index(), Some(1));

        // Navigate up
        view.select_prev();
        assert_eq!(view.tree_state.selection_index(), Some(0));

        // Navigate up at top (should stay at 0)
        view.select_prev();
        assert_eq!(view.tree_state.selection_index(), Some(0));
    }

    #[test]
    fn test_home_view_history_toggle() {
        let mut view = HomeView::new(PathBuf::from("."));
        assert!(!view.history_expanded);

        view.history_expanded = true;
        assert!(view.history_expanded);

        view.history_expanded = false;
        assert!(!view.history_expanded);
    }

    #[test]
    fn test_preview_mode_toggle() {
        let mut view = HomeView::new(PathBuf::from("."));

        // Default is DAG mode
        assert_eq!(view.preview_mode, PreviewMode::Dag);

        // Toggle to YAML
        view.preview_mode = PreviewMode::Yaml;
        assert_eq!(view.preview_mode, PreviewMode::Yaml);

        // Toggle back to DAG
        view.preview_mode = PreviewMode::Dag;
        assert_eq!(view.preview_mode, PreviewMode::Dag);
    }

    #[test]
    fn test_preview_mode_key_handler() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut view = HomeView::new(PathBuf::from("."));
        let mut state = TuiState::new("test.nika.yaml");

        // Initial state: DAG mode
        assert_eq!(view.preview_mode, PreviewMode::Dag);

        // Press 'D' to toggle to YAML
        let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.preview_mode, PreviewMode::Yaml);

        // Press 'D' again to toggle back to DAG
        view.handle_key(key, &mut state);
        assert_eq!(view.preview_mode, PreviewMode::Dag);
    }

    #[test]
    fn test_get_line_verb_color() {
        // Test verb detection
        assert_eq!(
            HomeView::get_line_verb_color("infer: prompt"),
            Some(VerbColor::Infer)
        );
        assert_eq!(
            HomeView::get_line_verb_color("exec: command"),
            Some(VerbColor::Exec)
        );
        assert_eq!(
            HomeView::get_line_verb_color("fetch: url"),
            Some(VerbColor::Fetch)
        );
        assert_eq!(
            HomeView::get_line_verb_color("invoke: tool"),
            Some(VerbColor::Invoke)
        );
        assert_eq!(
            HomeView::get_line_verb_color("agent: loop"),
            Some(VerbColor::Agent)
        );

        // Test non-verb lines return None
        assert_eq!(HomeView::get_line_verb_color("tasks:"), None);
        assert_eq!(HomeView::get_line_verb_color("  - id: step1"), None);
        assert_eq!(HomeView::get_line_verb_color(""), None);

        // Test indented verb lines (still detect verb)
        assert_eq!(
            HomeView::get_line_verb_color("    infer: nested"),
            Some(VerbColor::Infer)
        );
    }

    #[test]
    fn test_home_view_selected_entry_with_empty_list() {
        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.list_state.select(None);

        assert!(view.selected_entry().is_none());
    }

    #[test]
    fn test_home_view_status_line() {
        let mut view = HomeView::new(PathBuf::from("."));
        view.standalone.browser_entries.clear();
        view.standalone.browser_entries.push(BrowserEntry::new(
            PathBuf::from("test.nika.yaml"),
            &PathBuf::from("."),
        ));
        view.standalone.history.clear();

        let state = TuiState::new("test.nika.yaml");
        let status = view.status_line(&state);
        assert!(status.contains("1 workflows"));
        assert!(status.contains("0 in history"));
    }

    // === File Browser Tests ===

    #[test]
    fn test_empty_directory_has_no_entries() {
        // Use a non-existent directory so StandaloneState starts empty
        let view = HomeView::new(PathBuf::from("/nonexistent/path/that/has/no/nika/files"));
        assert!(
            view.standalone.browser_entries.is_empty(),
            "Browser should be empty for non-existent path"
        );
    }

    #[test]
    fn test_home_view_renders_file_browser() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut view = HomeView::new(PathBuf::from("."));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = TuiState::new("test.nika.yaml");
        let theme = Theme::novanet();

        terminal
            .draw(|frame| {
                view.render(frame, frame.area(), &state, &theme);
            })
            .unwrap();

        // Just verify it renders without panic
        let buffer = terminal.backend().buffer();
        let _content: String = buffer.content.iter().map(|c| c.symbol()).collect();
    }
}
