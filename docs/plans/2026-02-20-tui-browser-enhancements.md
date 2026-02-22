# Nika TUI Browser View Enhancements Plan

**Date:** 2026-02-20
**Status:** In Progress
**Target:** View 1 (Browser) visual and functional improvements

---

## Overview

Enhance the Nika TUI Browser View with 7 features across 3 priority tiers, transforming it from a basic file browser into a rich workflow preview and management interface.

```
╭──────────────────────────────────────────────────────────────────────────────────╮
│  EXECUTION ORDER                                                                 │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  TIER 1: HIGH PRIORITY (Visual Feedback)                           ▶ CURRENT    │
│  ├── 1.1 Tree validation inline (✓ valid, ⚠ warn, ✗ error)                      │
│  └── 1.2 Scroll indicators (scrollbar, position, percentage)                    │
│                                                                                  │
│  TIER 2: MEDIUM PRIORITY (Comprehension)                                         │
│  ├── 2.1 DAG icons + estimation (~Xs per task)                                  │
│  ├── 2.2 Verb bar charts (visual distribution)                                  │
│  └── 2.3 Run history sparkline (▁▂▃▄▅▆▇█)                                       │
│                                                                                  │
│  TIER 3: LOW PRIORITY (Live Features)                                            │
│  ├── 3.1 File watcher (auto-refresh on change)                                  │
│  └── 3.2 MCP tools preview (list available tools)                               │
│                                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────╯
```

---

## TIER 1: High Priority Features

### 1.1 Tree Validation Inline

**Goal:** Show validation status next to each workflow file in the tree.

**Current State:**
```
├─ 📄 invoke.nika.yaml
├─ 📄 agent.nika.yaml
```

**Target State:**
```
├─ 📄 invoke.nika.yaml      ✓ valid   4 tasks · invoke,infer
├─ 📄 agent.nika.yaml       ⚠ warn    2 tasks · missing desc
├─ 📄 broken.nika.yaml      ✗ error   Parse error line 12
```

**Implementation:**

#### Step 1.1.1: Add ValidationStatus to WorkflowInfo

```rust
// src/tui/views/browser.rs

#[derive(Debug, Clone)]
pub enum ValidationStatus {
    Valid,
    Warning(String),  // Warning message
    Error(String),    // Error message
    Unknown,          // Not yet validated
}

// Add to WorkflowInfo
pub struct WorkflowInfo {
    // ... existing fields ...
    pub validation_status: ValidationStatus,
    pub verb_summary: String,  // "invoke,infer,agent"
}
```

#### Step 1.1.2: Implement validate_workflow() function

```rust
// src/tui/views/browser.rs

impl WorkflowInfo {
    pub fn validate(&mut self) {
        // Try to parse the workflow
        match crate::ast::Workflow::from_yaml(&self.yaml_content) {
            Ok(workflow) => {
                // Check for warnings
                let warnings = self.check_warnings(&workflow);
                if warnings.is_empty() {
                    self.validation_status = ValidationStatus::Valid;
                } else {
                    self.validation_status = ValidationStatus::Warning(warnings.join(", "));
                }

                // Build verb summary
                self.verb_summary = self.extract_verb_summary(&workflow);
            }
            Err(e) => {
                self.validation_status = ValidationStatus::Error(e.to_string());
            }
        }
    }

    fn check_warnings(&self, workflow: &Workflow) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check for missing descriptions
        if workflow.tasks.iter().any(|t| t.description.is_none()) {
            warnings.push("missing task descriptions".to_string());
        }

        // Check for missing MCP config when invoke is used
        let has_invoke = workflow.tasks.iter().any(|t| matches!(&t.action, TaskAction::Invoke(_)));
        if has_invoke && workflow.mcp.is_none() {
            warnings.push("invoke without MCP config".to_string());
        }

        warnings
    }

    fn extract_verb_summary(&self, workflow: &Workflow) -> String {
        let mut verbs: HashSet<&str> = HashSet::new();
        for task in &workflow.tasks {
            match &task.action {
                TaskAction::Infer(_) => { verbs.insert("infer"); }
                TaskAction::Exec(_) => { verbs.insert("exec"); }
                TaskAction::Fetch(_) => { verbs.insert("fetch"); }
                TaskAction::Invoke(_) => { verbs.insert("invoke"); }
                TaskAction::Agent(_) => { verbs.insert("agent"); }
            }
        }
        verbs.into_iter().collect::<Vec<_>>().join(",")
    }
}
```

#### Step 1.1.3: Update render_tree() to show validation

```rust
// In render_tree_panel()

fn format_tree_item(&self, item: &TreeItem, width: usize) -> Line<'static> {
    let icon = if item.is_dir {
        if self.tree.expanded.contains(&item.path) { "▾ 📂" } else { "▸ 📂" }
    } else {
        "  📄"
    };

    let indent = "  ".repeat(item.depth);
    let name = &item.name;

    // Get validation info if it's a file
    let (status_icon, status_style, extra_info) = if !item.is_dir {
        if let Some(info) = self.workflow_infos.get(&item.path) {
            match &info.validation_status {
                ValidationStatus::Valid => (
                    "✓",
                    Style::default().fg(Color::Green),
                    format!("{} tasks · {}", info.task_count, info.verb_summary)
                ),
                ValidationStatus::Warning(msg) => (
                    "⚠",
                    Style::default().fg(Color::Yellow),
                    msg.clone()
                ),
                ValidationStatus::Error(msg) => (
                    "✗",
                    Style::default().fg(Color::Red),
                    msg.chars().take(30).collect::<String>()
                ),
                ValidationStatus::Unknown => (
                    "?",
                    Style::default().fg(Color::Gray),
                    String::new()
                ),
            }
        } else {
            ("?", Style::default().fg(Color::Gray), String::new())
        }
    } else {
        ("", Style::default(), format!("{} files", item.child_count))
    };

    // Build the line with proper spacing
    Line::from(vec![
        Span::raw(format!("{}{} {}", indent, icon, name)),
        Span::raw("  "),
        Span::styled(status_icon, status_style),
        Span::raw("  "),
        Span::styled(extra_info, Style::default().fg(Color::DarkGray)),
    ])
}
```

#### Step 1.1.4: Add summary bar at bottom of tree

```rust
fn render_tree_summary(&self, area: Rect, buf: &mut Buffer) {
    let (valid, warnings, errors) = self.count_validation_stats();

    let summary = Line::from(vec![
        Span::styled(format!("✓ {}", valid), Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled(format!("⚠ {}", warnings), Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled(format!("✗ {}", errors), Style::default().fg(Color::Red)),
    ]);

    Paragraph::new(summary)
        .alignment(Alignment::Center)
        .render(area, buf);
}
```

**Files to modify:**
- `src/tui/views/browser.rs` - Add ValidationStatus, update WorkflowInfo, update render

**Tests:**
- `test_validation_status_valid`
- `test_validation_status_warning`
- `test_validation_status_error`
- `test_verb_summary_extraction`
- `test_tree_summary_counts`

---

### 1.2 Scroll Indicators

**Goal:** Add visual scrollbar and position indicators to all scrollable panels.

**Current State:** No scroll indicators, users don't know their position

**Target State:**
```
╭─── 📋 YAML PREVIEW ───────────────────────────────────────────────────╮
│▲│ schema: nika/workflow@0.2                                           │
│║│ workflow: invoke-novanet                                            │
│█│   servers:                                                          │
│█│     novanet:                                                        │
│║│       command: cargo run --manifest-path ...                        │
│▼│                                                                     │
├─┴─────────────────────────────────────────────────────────────────────┤
│ Line 5-12 of 47  ██████░░░░░░░░░░░░░░ 25%                             │
╰───────────────────────────────────────────────────────────────────────╯
```

**Implementation:**

#### Step 1.2.1: Create ScrollIndicator widget

```rust
// src/tui/widgets/scroll_indicator.rs

pub struct ScrollIndicator {
    pub total_lines: usize,
    pub visible_lines: usize,
    pub scroll_offset: usize,
}

impl ScrollIndicator {
    pub fn new(total: usize, visible: usize, offset: usize) -> Self {
        Self {
            total_lines: total,
            visible_lines: visible,
            scroll_offset: offset,
        }
    }

    /// Returns the scrollbar characters for a vertical bar
    /// Height is the available height for the scrollbar
    pub fn render_vertical(&self, height: usize) -> Vec<char> {
        if self.total_lines <= self.visible_lines {
            // No scrolling needed
            return vec!['│'; height];
        }

        let scroll_ratio = self.scroll_offset as f64 / (self.total_lines - self.visible_lines) as f64;
        let thumb_size = ((self.visible_lines as f64 / self.total_lines as f64) * height as f64)
            .max(1.0) as usize;
        let thumb_pos = ((height - thumb_size) as f64 * scroll_ratio) as usize;

        let mut chars = Vec::with_capacity(height);
        for i in 0..height {
            if i == 0 && self.scroll_offset > 0 {
                chars.push('▲');
            } else if i == height - 1 && self.scroll_offset + self.visible_lines < self.total_lines {
                chars.push('▼');
            } else if i >= thumb_pos && i < thumb_pos + thumb_size {
                chars.push('█');
            } else {
                chars.push('║');
            }
        }
        chars
    }

    /// Returns a status line like "Line 5-12 of 47  ██░░░░ 25%"
    pub fn render_status(&self, width: usize) -> Line<'static> {
        let start_line = self.scroll_offset + 1;
        let end_line = (self.scroll_offset + self.visible_lines).min(self.total_lines);

        let percentage = if self.total_lines > 0 {
            ((self.scroll_offset + self.visible_lines) as f64 / self.total_lines as f64 * 100.0) as usize
        } else {
            100
        };

        // Progress bar
        let bar_width = 20;
        let filled = (bar_width * percentage / 100).min(bar_width);
        let empty = bar_width - filled;
        let progress_bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        Line::from(vec![
            Span::styled(
                format!("Line {}-{} of {}", start_line, end_line, self.total_lines),
                Style::default().fg(Color::DarkGray)
            ),
            Span::raw("  "),
            Span::styled(progress_bar, Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::styled(format!("{}%", percentage), Style::default().fg(Color::DarkGray)),
        ])
    }
}
```

#### Step 1.2.2: Add scroll state to BrowserView

```rust
// src/tui/views/browser.rs

pub struct BrowserView {
    // ... existing fields ...

    // Scroll states for each panel
    pub tree_scroll: usize,
    pub yaml_scroll: usize,
    pub dag_scroll: usize,
    pub info_scroll: usize,
}

impl BrowserView {
    pub fn scroll_up(&mut self) {
        match self.focused_panel {
            BrowserPanel::Tree => self.tree_scroll = self.tree_scroll.saturating_sub(1),
            BrowserPanel::YamlPreview => self.yaml_scroll = self.yaml_scroll.saturating_sub(1),
            BrowserPanel::DagPreview => self.dag_scroll = self.dag_scroll.saturating_sub(1),
            BrowserPanel::Info => self.info_scroll = self.info_scroll.saturating_sub(1),
        }
    }

    pub fn scroll_down(&mut self) {
        match self.focused_panel {
            BrowserPanel::Tree => self.tree_scroll += 1,
            BrowserPanel::YamlPreview => self.yaml_scroll += 1,
            BrowserPanel::DagPreview => self.dag_scroll += 1,
            BrowserPanel::Info => self.info_scroll += 1,
        }
    }

    pub fn page_up(&mut self, page_size: usize) {
        match self.focused_panel {
            BrowserPanel::Tree => self.tree_scroll = self.tree_scroll.saturating_sub(page_size),
            BrowserPanel::YamlPreview => self.yaml_scroll = self.yaml_scroll.saturating_sub(page_size),
            BrowserPanel::DagPreview => self.dag_scroll = self.dag_scroll.saturating_sub(page_size),
            BrowserPanel::Info => self.info_scroll = self.info_scroll.saturating_sub(page_size),
        }
    }

    pub fn page_down(&mut self, page_size: usize) {
        match self.focused_panel {
            BrowserPanel::Tree => self.tree_scroll += page_size,
            BrowserPanel::YamlPreview => self.yaml_scroll += page_size,
            BrowserPanel::DagPreview => self.dag_scroll += page_size,
            BrowserPanel::Info => self.info_scroll += page_size,
        }
    }
}
```

#### Step 1.2.3: Update render methods to include scrollbar

```rust
fn render_yaml_panel(&self, area: Rect, buf: &mut Buffer) {
    // Reserve 1 column for scrollbar, 1 row for status
    let content_area = Rect {
        x: area.x + 1,  // Leave space for scrollbar
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(1),  // Leave space for status
    };

    let scrollbar_area = Rect {
        x: area.x,
        y: area.y,
        width: 1,
        height: area.height.saturating_sub(1),
    };

    let status_area = Rect {
        x: area.x,
        y: area.y + area.height - 1,
        width: area.width,
        height: 1,
    };

    // Render YAML content with scroll
    let lines: Vec<&str> = self.yaml_content.lines().collect();
    let total_lines = lines.len();
    let visible_lines = content_area.height as usize;

    // Clamp scroll offset
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll_offset = self.yaml_scroll.min(max_scroll);

    // Render visible lines
    for (i, line) in lines.iter().skip(scroll_offset).take(visible_lines).enumerate() {
        let y = content_area.y + i as u16;
        buf.set_string(content_area.x, y, *line, Style::default());
    }

    // Render scrollbar
    let indicator = ScrollIndicator::new(total_lines, visible_lines, scroll_offset);
    let scrollbar_chars = indicator.render_vertical(scrollbar_area.height as usize);
    for (i, ch) in scrollbar_chars.iter().enumerate() {
        buf.set_string(
            scrollbar_area.x,
            scrollbar_area.y + i as u16,
            ch.to_string(),
            Style::default().fg(Color::DarkGray)
        );
    }

    // Render status line
    let status = indicator.render_status(status_area.width as usize);
    Paragraph::new(status).render(status_area, buf);
}
```

**Files to modify:**
- `src/tui/widgets/scroll_indicator.rs` (NEW)
- `src/tui/widgets/mod.rs` - Export ScrollIndicator
- `src/tui/views/browser.rs` - Add scroll state, update render methods

**Tests:**
- `test_scroll_indicator_no_scroll_needed`
- `test_scroll_indicator_at_top`
- `test_scroll_indicator_at_middle`
- `test_scroll_indicator_at_bottom`
- `test_scroll_status_line_format`

---

## TIER 2: Medium Priority Features

### 2.1 DAG Icons + Estimation

**Goal:** Add verb-specific icons and duration estimates to DAG nodes.

**Current State:**
```
       ╭────────╮
       │ schema │
       ╰────────╯
```

**Target State:**
```
       ╭─────────────────╮
       │    📥 schema    │
       │  invoke:novanet │
       │     ~0.5s       │
       ╰─────────────────╯
```

**Implementation:**

#### Step 2.1.1: Add verb icons mapping

```rust
// src/tui/views/browser.rs

pub fn verb_icon(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer(_) => "🧠",
        TaskAction::Exec(_) => "⚡",
        TaskAction::Fetch(_) => "🔗",
        TaskAction::Invoke(_) => "📥",
        TaskAction::Agent(_) => "🤖",
    }
}

pub fn verb_name(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer(_) => "infer",
        TaskAction::Exec(_) => "exec",
        TaskAction::Fetch(_) => "fetch",
        TaskAction::Invoke(p) => &format!("invoke:{}", p.server),
        TaskAction::Agent(_) => "agent",
    }
}
```

#### Step 2.1.2: Add duration estimation

```rust
// src/tui/views/browser.rs

pub fn estimate_duration(action: &TaskAction) -> &'static str {
    match action {
        TaskAction::Infer(_) => "~2-5s",
        TaskAction::Exec(_) => "~0.1s",
        TaskAction::Fetch(_) => "~0.5s",
        TaskAction::Invoke(_) => "~0.5-2s",
        TaskAction::Agent(_) => "~5-30s",
    }
}
```

#### Step 2.1.3: Update generate_dag_ascii()

```rust
fn generate_dag_ascii(&self, workflow: &Workflow) -> String {
    let mut lines = Vec::new();

    for task in &workflow.tasks {
        let icon = verb_icon(&task.action);
        let verb = verb_name(&task.action);
        let estimate = estimate_duration(&task.action);

        // Multi-line node box
        lines.push(format!("       ╭─────────────────╮"));
        lines.push(format!("       │    {} {}    │", icon, task.id));
        lines.push(format!("       │  {:^13}  │", verb));
        lines.push(format!("       │     {:^9}     │", estimate));
        lines.push(format!("       ╰────────┬────────╯"));
        lines.push(format!("                │"));
        lines.push(format!("                ▼"));
    }

    lines.join("\n")
}
```

**Files to modify:**
- `src/tui/views/browser.rs` - Add icon/estimate functions, update DAG render

**Tests:**
- `test_verb_icons`
- `test_duration_estimates`
- `test_dag_ascii_with_icons`

---

### 2.2 Verb Bar Charts

**Goal:** Show visual distribution of verbs used in the workflow.

**Target State:**
```
VERBS   invoke ████████████░░░░░░░░ 3
        infer  ██████░░░░░░░░░░░░░░ 2
        agent  ██░░░░░░░░░░░░░░░░░░ 1
```

**Implementation:**

#### Step 2.2.1: Add verb counting

```rust
// src/tui/views/browser.rs

pub fn count_verbs(workflow: &Workflow) -> HashMap<&'static str, usize> {
    let mut counts: HashMap<&'static str, usize> = HashMap::new();

    for task in &workflow.tasks {
        let verb = match &task.action {
            TaskAction::Infer(_) => "infer",
            TaskAction::Exec(_) => "exec",
            TaskAction::Fetch(_) => "fetch",
            TaskAction::Invoke(_) => "invoke",
            TaskAction::Agent(_) => "agent",
        };
        *counts.entry(verb).or_insert(0) += 1;
    }

    counts
}
```

#### Step 2.2.2: Render bar chart

```rust
fn render_verb_chart(&self, area: Rect, buf: &mut Buffer, counts: &HashMap<&str, usize>) {
    let max_count = counts.values().max().copied().unwrap_or(1);
    let bar_width = 20;

    let mut y = area.y;
    for (verb, count) in counts.iter() {
        let filled = (bar_width * count / max_count).min(bar_width);
        let empty = bar_width - filled;

        let line = Line::from(vec![
            Span::styled(format!("{:>8}", verb), Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::styled("█".repeat(filled), Style::default().fg(Color::Green)),
            Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(count.to_string(), Style::default().fg(Color::White)),
        ]);

        buf.set_line(area.x, y, &line, area.width);
        y += 1;
    }
}
```

**Files to modify:**
- `src/tui/views/browser.rs` - Add verb counting, add bar chart render to info panel

**Tests:**
- `test_count_verbs`
- `test_bar_chart_rendering`

---

### 2.3 Run History Sparkline

**Goal:** Show a mini sparkline of recent run durations with pass/fail indicators.

**Target State:**
```
HISTORY ▁▂▃▄▅▆▇█▇▆ avg 4.5s
        ✓ ✓ ✗ ✓ ✓  last 5
```

**Implementation:**

#### Step 2.3.1: Add RunHistory struct

```rust
// src/tui/views/browser.rs

#[derive(Debug, Clone, Default)]
pub struct RunHistory {
    pub runs: Vec<RunRecord>,
}

#[derive(Debug, Clone)]
pub struct RunRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration_secs: f64,
    pub success: bool,
    pub tasks_completed: usize,
    pub tasks_total: usize,
}

impl RunHistory {
    pub fn load_for_workflow(workflow_path: &Path) -> Self {
        // Load from .nika/traces/ directory
        let traces_dir = workflow_path.parent()
            .map(|p| p.join(".nika/traces"))
            .unwrap_or_default();

        if !traces_dir.exists() {
            return Self::default();
        }

        // Find trace files matching this workflow
        let workflow_name = workflow_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let mut runs = Vec::new();
        // Parse NDJSON trace files...
        // (Implementation reads trace files and extracts run stats)

        Self { runs }
    }

    pub fn sparkline(&self, width: usize) -> String {
        const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

        if self.runs.is_empty() {
            return "No history".to_string();
        }

        let durations: Vec<f64> = self.runs.iter()
            .take(width)
            .map(|r| r.duration_secs)
            .collect();

        let max = durations.iter().cloned().fold(0.0, f64::max);
        let min = durations.iter().cloned().fold(f64::MAX, f64::min);
        let range = (max - min).max(0.001);

        durations.iter()
            .map(|d| {
                let normalized = ((d - min) / range * 7.0) as usize;
                BARS[normalized.min(7)]
            })
            .collect()
    }

    pub fn average_duration(&self) -> f64 {
        if self.runs.is_empty() {
            return 0.0;
        }
        self.runs.iter().map(|r| r.duration_secs).sum::<f64>() / self.runs.len() as f64
    }

    pub fn success_rate(&self) -> f64 {
        if self.runs.is_empty() {
            return 0.0;
        }
        let successes = self.runs.iter().filter(|r| r.success).count();
        successes as f64 / self.runs.len() as f64 * 100.0
    }
}
```

#### Step 2.3.2: Render history in info panel

```rust
fn render_run_history(&self, area: Rect, buf: &mut Buffer, history: &RunHistory) {
    let sparkline = history.sparkline(10);
    let avg = history.average_duration();

    // Line 1: Sparkline + average
    let line1 = Line::from(vec![
        Span::styled("HISTORY ", Style::default().fg(Color::Cyan)),
        Span::styled(sparkline, Style::default().fg(Color::Green)),
        Span::styled(format!(" avg {:.1}s", avg), Style::default().fg(Color::DarkGray)),
    ]);
    buf.set_line(area.x, area.y, &line1, area.width);

    // Line 2: Success/fail indicators
    let status_icons: String = history.runs.iter()
        .take(5)
        .map(|r| if r.success { "✓ " } else { "✗ " })
        .collect();

    let line2 = Line::from(vec![
        Span::raw("        "),
        Span::styled(status_icons, Style::default().fg(Color::Green)),
        Span::styled(" last 5", Style::default().fg(Color::DarkGray)),
    ]);
    buf.set_line(area.x, area.y + 1, &line2, area.width);
}
```

**Files to modify:**
- `src/tui/views/browser.rs` - Add RunHistory, sparkline rendering

**Tests:**
- `test_run_history_sparkline`
- `test_run_history_average`
- `test_run_history_success_rate`

---

## TIER 3: Low Priority Features

### 3.1 File Watcher

**Goal:** Auto-refresh the browser when workflow files change.

**Target State:**
```
⏱️ LIVE FILE WATCH
┌────────────────────────────────────────────────────────────────┐
│  🟢 Watching: invoke.nika.yaml                                 │
│     Modified: 2s ago   Auto-refresh: ON                        │
└────────────────────────────────────────────────────────────────┘
```

**Implementation:**

#### Step 3.1.1: Add notify dependency

```toml
# Cargo.toml
[dependencies]
notify = { version = "6.1", optional = true }

[features]
tui = ["ratatui", "crossterm", "notify"]
```

#### Step 3.1.2: Create FileWatcher struct

```rust
// src/tui/file_watcher.rs

use notify::{Watcher, RecursiveMode, Result as NotifyResult};
use std::sync::mpsc;

pub struct FileWatcher {
    watcher: notify::RecommendedWatcher,
    rx: mpsc::Receiver<notify::Event>,
    watching: HashSet<PathBuf>,
    last_modified: HashMap<PathBuf, std::time::Instant>,
}

impl FileWatcher {
    pub fn new() -> NotifyResult<Self> {
        let (tx, rx) = mpsc::channel();

        let watcher = notify::recommended_watcher(move |res: NotifyResult<notify::Event>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;

        Ok(Self {
            watcher,
            rx,
            watching: HashSet::new(),
            last_modified: HashMap::new(),
        })
    }

    pub fn watch(&mut self, path: &Path) -> NotifyResult<()> {
        if !self.watching.contains(path) {
            self.watcher.watch(path, RecursiveMode::NonRecursive)?;
            self.watching.insert(path.to_path_buf());
        }
        Ok(())
    }

    pub fn check_events(&mut self) -> Vec<PathBuf> {
        let mut changed = Vec::new();

        while let Ok(event) = self.rx.try_recv() {
            if event.kind.is_modify() {
                for path in event.paths {
                    if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                        self.last_modified.insert(path.clone(), std::time::Instant::now());
                        changed.push(path);
                    }
                }
            }
        }

        changed
    }

    pub fn time_since_modified(&self, path: &Path) -> Option<std::time::Duration> {
        self.last_modified.get(path).map(|t| t.elapsed())
    }
}
```

#### Step 3.1.3: Integrate with BrowserView

```rust
// src/tui/views/browser.rs

impl BrowserView {
    pub fn handle_file_changes(&mut self, changed: Vec<PathBuf>) {
        for path in changed {
            // Re-parse the workflow
            if let Some(info) = self.workflow_infos.get_mut(&path) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    info.yaml_content = content;
                    info.validate();
                }
            }

            // If this is the selected workflow, update preview
            if self.selected_path.as_ref() == Some(&path) {
                self.update_preview();
            }
        }
    }
}
```

**Files to modify:**
- `Cargo.toml` - Add notify dependency
- `src/tui/file_watcher.rs` (NEW)
- `src/tui/mod.rs` - Export file_watcher
- `src/tui/views/browser.rs` - Integrate file watcher
- `src/tui/app.rs` - Poll file watcher in event loop

**Tests:**
- `test_file_watcher_detects_changes`
- `test_file_watcher_updates_validation`

---

### 3.2 MCP Tools Preview

**Goal:** Show available MCP tools when a server is configured.

**Target State:**
```
🔌 MCP SERVERS
┌────────────────────────────────────────────────────────────────┐
│  ● novanet    cargo run --manifest-path ../novanet-mcp/...     │
│               Tools: novanet_generate, novanet_describe (+5)   │
└────────────────────────────────────────────────────────────────┘
```

**Implementation:**

#### Step 3.2.1: Parse MCP config from workflow

```rust
// src/tui/views/browser.rs

#[derive(Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub command: String,
    pub tools: Vec<String>,
    pub is_connected: bool,
}

impl WorkflowInfo {
    pub fn extract_mcp_servers(&self, workflow: &Workflow) -> Vec<McpServerInfo> {
        let Some(mcp_config) = &workflow.mcp else {
            return Vec::new();
        };

        mcp_config.servers.iter()
            .map(|(name, config)| {
                McpServerInfo {
                    name: name.clone(),
                    command: config.command.clone(),
                    tools: Vec::new(),  // Populated on connect
                    is_connected: false,
                }
            })
            .collect()
    }
}
```

#### Step 3.2.2: Render MCP servers panel

```rust
fn render_mcp_servers(&self, area: Rect, buf: &mut Buffer, servers: &[McpServerInfo]) {
    let block = Block::default()
        .title(" 🔌 MCP SERVERS ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    block.render(area, buf);

    if servers.is_empty() {
        let text = Paragraph::new("No MCP servers configured")
            .style(Style::default().fg(Color::DarkGray));
        text.render(inner, buf);
        return;
    }

    let mut y = inner.y;
    for server in servers {
        // Line 1: Server name + command
        let status = if server.is_connected { "●" } else { "○" };
        let status_color = if server.is_connected { Color::Green } else { Color::DarkGray };

        let line1 = Line::from(vec![
            Span::styled(format!("  {} ", status), Style::default().fg(status_color)),
            Span::styled(&server.name, Style::default().fg(Color::Cyan).bold()),
            Span::raw("    "),
            Span::styled(
                server.command.chars().take(40).collect::<String>(),
                Style::default().fg(Color::DarkGray)
            ),
        ]);
        buf.set_line(inner.x, y, &line1, inner.width);
        y += 1;

        // Line 2: Tools list
        if !server.tools.is_empty() {
            let tools_str = if server.tools.len() <= 3 {
                server.tools.join(", ")
            } else {
                format!("{} (+{})",
                    server.tools[..2].join(", "),
                    server.tools.len() - 2
                )
            };

            let line2 = Line::from(vec![
                Span::raw("               Tools: "),
                Span::styled(tools_str, Style::default().fg(Color::White)),
            ]);
            buf.set_line(inner.x, y, &line2, inner.width);
            y += 1;
        }

        y += 1;  // Spacing between servers
    }
}
```

**Files to modify:**
- `src/tui/views/browser.rs` - Add McpServerInfo, MCP panel rendering

**Tests:**
- `test_mcp_server_extraction`
- `test_mcp_panel_rendering`

---

## Execution Order Summary

```
╭──────────────────────────────────────────────────────────────────────────────────╮
│  EXECUTION CHECKLIST                                                             │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  TIER 1: HIGH PRIORITY                                                           │
│  ☐ 1.1.1 Add ValidationStatus enum                                              │
│  ☐ 1.1.2 Implement validate_workflow()                                          │
│  ☐ 1.1.3 Update render_tree() with validation icons                             │
│  ☐ 1.1.4 Add tree summary bar                                                   │
│  ☐ 1.1.5 Write tests for validation                                             │
│  ☐ 1.2.1 Create ScrollIndicator widget                                          │
│  ☐ 1.2.2 Add scroll state to BrowserView                                        │
│  ☐ 1.2.3 Update render methods with scrollbars                                  │
│  ☐ 1.2.4 Write tests for scroll                                                 │
│                                                                                  │
│  TIER 2: MEDIUM PRIORITY                                                         │
│  ☐ 2.1.1 Add verb icons mapping                                                 │
│  ☐ 2.1.2 Add duration estimation                                                │
│  ☐ 2.1.3 Update generate_dag_ascii()                                            │
│  ☐ 2.2.1 Add verb counting                                                      │
│  ☐ 2.2.2 Render bar chart                                                       │
│  ☐ 2.3.1 Add RunHistory struct                                                  │
│  ☐ 2.3.2 Render history sparkline                                               │
│  ☐ 2.3.3 Write tests for TIER 2                                                 │
│                                                                                  │
│  TIER 3: LOW PRIORITY                                                            │
│  ☐ 3.1.1 Add notify dependency                                                  │
│  ☐ 3.1.2 Create FileWatcher struct                                              │
│  ☐ 3.1.3 Integrate with BrowserView                                             │
│  ☐ 3.2.1 Parse MCP config                                                       │
│  ☐ 3.2.2 Render MCP servers panel                                               │
│  ☐ 3.2.3 Write tests for TIER 3                                                 │
│                                                                                  │
╰──────────────────────────────────────────────────────────────────────────────────╯
```

---

## Success Criteria

| Feature | Metric |
|---------|--------|
| Tree validation | All workflows show ✓/⚠/✗ status |
| Scroll indicators | Scrollbar visible, position accurate |
| DAG icons | Icons match verb type |
| Verb bar charts | Proportional bars render correctly |
| Run history | Sparkline from trace files |
| File watcher | <100ms refresh on file change |
| MCP tools | Tools list populated on focus |

---

## Files Summary

| File | Changes |
|------|---------|
| `src/tui/views/browser.rs` | All features integrated |
| `src/tui/widgets/scroll_indicator.rs` | NEW - Scroll widget |
| `src/tui/widgets/mod.rs` | Export new widgets |
| `src/tui/file_watcher.rs` | NEW - File watching |
| `src/tui/mod.rs` | Export file_watcher |
| `src/tui/app.rs` | Integrate file watcher |
| `Cargo.toml` | Add notify dependency |

---

## Estimated Lines of Code

| TIER | New Lines | Modified Lines |
|------|-----------|----------------|
| 1 | ~300 | ~100 |
| 2 | ~200 | ~50 |
| 3 | ~250 | ~50 |
| **Total** | **~750** | **~200** |
