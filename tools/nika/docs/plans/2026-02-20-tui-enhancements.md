# TUI Enhancements Plan

**Date:** 2026-02-20
**Status:** In Progress
**Scope:** 20 improvements across 4 tiers

---

## Overview

This plan enhances the Nika TUI with functional improvements, UX features, advanced capabilities, and code quality refactoring.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ENHANCEMENT ROADMAP                                                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  TIER 1: Quick Wins (~30 min)                                                   │
│  ├── 1.1 Implement [c] copy to clipboard                                        │
│  ├── 1.2 Implement [r] retry workflow                                           │
│  ├── 1.3 MCP call navigation with ↑↓ in Full JSON view                         │
│  ├── 1.4 Sparkline widget for MCP latency                                       │
│  └── 1.5 Workflow filtering with / in Browser                                   │
│                                                                                 │
│  TIER 2: UX Enhancements (~45 min)                                              │
│  ├── 2.1 Global search with Ctrl+F                                              │
│  ├── 2.2 Execution history panel                                                │
│  ├── 2.3 Visual breakpoints on tasks                                            │
│  ├── 2.4 Theme toggle (dark/light)                                              │
│  └── 2.5 Export trace with [e] key                                              │
│                                                                                 │
│  TIER 3: Advanced Features (~60 min)                                            │
│  ├── 3.1 Mouse support for panel focus                                          │
│  ├── 3.2 Bindings diff view between tasks                                       │
│  ├── 3.3 Horizontal timeline widget                                             │
│  ├── 3.4 System notifications on completion                                     │
│  └── 3.5 Trace replay mode                                                      │
│                                                                                 │
│  TIER 4: Performance & Code Quality (~45 min)                                   │
│  ├── 4.1 Lazy rendering for long content                                        │
│  ├── 4.2 Extract render methods to widgets                                      │
│  ├── 4.3 Formalize state machine transitions                                    │
│  ├── 4.4 Memoize expensive JSON formatting                                      │
│  └── 4.5 Add snapshot tests for panels                                          │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## TIER 1: Quick Wins

### 1.1 Copy to Clipboard `[c]`

**Goal:** Copy final output JSON to system clipboard.

**Implementation:**
```rust
// In app.rs - handle KeyCode::Char('c') in Monitor view
KeyCode::Char('c') => {
    if let Some(output) = &state.workflow.final_output {
        let json = serde_json::to_string_pretty(output.as_ref())?;
        copy_to_clipboard(&json)?;
        state.show_toast("Copied to clipboard!", ToastLevel::Success);
    }
    AppAction::None
}
```

**Dependencies:** `arboard` crate for cross-platform clipboard.

**Files:**
- `Cargo.toml` - add `arboard = "3.4"`
- `src/tui/app.rs` - add copy handler
- `src/tui/state.rs` - add toast notification system

---

### 1.2 Retry Workflow `[r]`

**Goal:** Re-run failed workflow from Monitor view.

**Implementation:**
```rust
// In app.rs
KeyCode::Char('r') => {
    if state.workflow.phase == MissionPhase::Abort {
        // Reset state and re-trigger execution
        AppAction::RetryWorkflow
    } else {
        AppAction::None
    }
}
```

**Files:**
- `src/tui/app.rs` - add retry action
- `src/tui/state.rs` - add reset_for_retry() method

---

### 1.3 MCP Navigation `↑↓`

**Goal:** Navigate between MCP calls in Full JSON view.

**Implementation:**
```rust
// In state.rs
pub fn select_prev_mcp(&mut self) {
    if let Some(idx) = self.selected_mcp_idx {
        if idx > 0 {
            self.selected_mcp_idx = Some(idx - 1);
        }
    } else if !self.mcp_calls.is_empty() {
        self.selected_mcp_idx = Some(self.mcp_calls.len() - 1);
    }
}

pub fn select_next_mcp(&mut self) {
    if let Some(idx) = self.selected_mcp_idx {
        if idx < self.mcp_calls.len() - 1 {
            self.selected_mcp_idx = Some(idx + 1);
        }
    } else if !self.mcp_calls.is_empty() {
        self.selected_mcp_idx = Some(0);
    }
}
```

**Files:**
- `src/tui/state.rs` - navigation methods
- `src/tui/app.rs` - key handlers when NovaNet panel focused

---

### 1.4 Sparkline Widget

**Goal:** Show MCP response time distribution as mini-chart.

**Implementation:**
```rust
// In widgets/sparkline.rs
pub struct Sparkline<'a> {
    data: &'a [u64],
    max: Option<u64>,
    style: Style,
}

impl Widget for Sparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let max = self.max.unwrap_or(*self.data.iter().max().unwrap_or(&1));

        for (i, &value) in self.data.iter().enumerate() {
            if i >= area.width as usize { break; }
            let bar_idx = ((value * 7) / max).min(7) as usize;
            buf.set_string(
                area.x + i as u16,
                area.y,
                BARS[bar_idx].to_string(),
                self.style,
            );
        }
    }
}
```

**Files:**
- `src/tui/widgets/sparkline.rs` - new widget
- `src/tui/widgets/mod.rs` - export
- `src/tui/views/monitor.rs` - use in MCP panel

---

### 1.5 Workflow Filtering `/`

**Goal:** Filter workflow list in Browser view.

**Implementation:**
```rust
// In browser.rs state
pub struct BrowserState {
    pub filter: String,
    pub filter_active: bool,
    // ...
}

// Filter logic
pub fn filtered_entries(&self) -> impl Iterator<Item = &BrowserEntry> {
    self.entries.iter().filter(|e| {
        if self.filter.is_empty() {
            true
        } else {
            e.name.to_lowercase().contains(&self.filter.to_lowercase())
        }
    })
}
```

**Files:**
- `src/tui/standalone.rs` - filter state
- `src/tui/views/browser.rs` - filter input rendering
- `src/tui/app.rs` - `/` key to activate filter mode

---

## TIER 2: UX Enhancements

### 2.1 Global Search `Ctrl+F`

**Goal:** Search across all panels (tasks, MCP calls, output).

**Implementation:**
- Add `SearchOverlay` component
- Track search state: query, results, current match
- Highlight matches in all panels
- `n`/`N` for next/prev match

**Files:**
- `src/tui/overlays/search.rs` - new overlay
- `src/tui/state.rs` - search state
- `src/tui/app.rs` - Ctrl+F handler

---

### 2.2 Execution History

**Goal:** Show recent workflow executions in Browser.

**Implementation:**
- Read from `.nika/traces/` directory
- Parse NDJSON headers for workflow name, status, duration
- Display as list with status icons

**Files:**
- `src/tui/standalone.rs` - history loading
- `src/tui/views/browser.rs` - history panel

---

### 2.3 Visual Breakpoints

**Goal:** Mark tasks to pause before execution.

**Implementation:**
```rust
// In state.rs
pub breakpoints: HashSet<String>,  // Task IDs

// In runner integration
if state.breakpoints.contains(&task_id) {
    state.paused = true;
    // Wait for user to press Space to continue
}
```

**Files:**
- `src/tui/state.rs` - breakpoint tracking
- `src/tui/views/monitor.rs` - breakpoint indicator (🔴)
- `src/tui/app.rs` - `b` to toggle breakpoint

---

### 2.4 Theme Toggle

**Goal:** Switch between dark and light themes.

**Implementation:**
```rust
// In theme.rs
pub enum ThemeMode {
    Dark,
    Light,
}

impl Theme {
    pub fn toggle(&mut self) {
        self.mode = match self.mode {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.apply_mode();
    }
}
```

**Files:**
- `src/tui/theme.rs` - dual theme support
- `src/tui/app.rs` - `t` key handler

---

### 2.5 Export Trace `[e]`

**Goal:** Save current execution trace to file.

**Implementation:**
```rust
KeyCode::Char('e') => {
    let path = format!(".nika/traces/export_{}.ndjson", timestamp());
    state.event_log.export_to_file(&path)?;
    state.show_toast(&format!("Exported to {}", path), ToastLevel::Success);
    AppAction::None
}
```

**Files:**
- `src/tui/app.rs` - export handler
- `src/event/trace.rs` - export method

---

## TIER 3: Advanced Features

### 3.1 Mouse Support

**Goal:** Click to focus panels, select tasks/MCP calls.

**Implementation:**
```rust
// In app.rs event loop
Event::Mouse(MouseEvent { kind, column, row, .. }) => {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => {
            // Determine which panel was clicked
            let panel = self.panel_at(column, row);
            self.state.focused_panel = panel;
        }
        _ => {}
    }
}
```

**Dependencies:** Already have `crossterm` with mouse feature.

**Files:**
- `src/tui/app.rs` - mouse event handling
- `src/tui/views/monitor.rs` - hit testing

---

### 3.2 Bindings Diff

**Goal:** Show what changed in context between tasks.

**Implementation:**
- Track `DataStore` snapshots per task
- Compute JSON diff between consecutive tasks
- Display additions (green), removals (red), changes (yellow)

**Files:**
- `src/tui/state.rs` - context snapshots
- `src/tui/widgets/diff.rs` - diff widget
- `src/tui/views/monitor.rs` - TaskIO tab enhancement

---

### 3.3 Timeline Widget

**Goal:** Horizontal Gantt-style execution timeline.

**Implementation:**
```rust
// In widgets/timeline.rs
pub struct Timeline<'a> {
    tasks: &'a [TaskTiming],
    current_time: Duration,
}

// Renders:
// 0s        1s        2s        3s
// │─────────│─────────│─────────│
// ████████████░░░░░░░░░░░░░░░░░░░ task1 (1.2s)
//             ████████████░░░░░░░ task2 (0.8s)
```

**Files:**
- `src/tui/widgets/timeline.rs` - new widget
- `src/tui/state.rs` - TaskTiming struct
- `src/tui/views/monitor.rs` - add to Progress panel

---

### 3.4 System Notifications

**Goal:** Desktop notification when workflow completes.

**Implementation:**
```rust
// On workflow completion
#[cfg(feature = "notifications")]
{
    notify_rust::Notification::new()
        .summary("Nika Workflow Complete")
        .body(&format!("{} finished in {}", workflow_name, duration))
        .icon("nika")
        .show()?;
}
```

**Dependencies:** `notify-rust` crate (optional feature).

**Files:**
- `Cargo.toml` - add optional dependency
- `src/tui/app.rs` - notification trigger

---

### 3.5 Trace Replay

**Goal:** Step through recorded execution.

**Implementation:**
- Load NDJSON trace file
- Parse events into timeline
- Playback controls: play/pause, step, speed

**Files:**
- `src/tui/replay.rs` - replay engine
- `src/tui/app.rs` - replay mode
- CLI: `nika tui --replay trace.ndjson`

---

## TIER 4: Performance & Code Quality

### 4.1 Lazy Rendering

**Goal:** Only render visible lines for large content.

**Implementation:**
```rust
// In render methods
let visible_start = scroll_offset;
let visible_end = scroll_offset + area.height as usize;

for (i, line) in lines.iter().enumerate() {
    if i < visible_start || i >= visible_end {
        continue;
    }
    // Render line at position i - visible_start
}
```

**Files:**
- `src/tui/views/monitor.rs` - all render_* methods
- `src/tui/views/browser.rs` - YAML preview

---

### 4.2 Widget Extraction

**Goal:** Extract inline rendering to reusable widgets.

**Current:** ~300 lines of inline rendering in monitor.rs render_* methods.

**Target:**
```rust
// Before (inline)
fn render_mcp_json(&self, area: Rect, buf: &mut Buffer, ...) {
    // 150 lines of rendering code
}

// After (widget)
let widget = McpJsonView::new(&state.mcp_calls, selected_idx);
widget.render(area, buf);
```

**New widgets:**
- `McpJsonView` - Full MCP request/response display
- `TaskIOView` - Task input/output display
- `DagGraphView` - DAG with animation
- `OutputView` - Final output display

**Files:**
- `src/tui/widgets/mcp_json.rs`
- `src/tui/widgets/task_io.rs`
- `src/tui/widgets/output_view.rs`
- `src/tui/widgets/dag_graph.rs`

---

### 4.3 State Machine Formalization

**Goal:** Explicit state transitions with validation.

**Implementation:**
```rust
pub enum AppState {
    Browser(BrowserState),
    Monitor(MonitorState),
    Search(SearchState),
    Replay(ReplayState),
}

impl AppState {
    pub fn can_transition_to(&self, target: &AppState) -> bool {
        match (self, target) {
            (AppState::Browser(_), AppState::Monitor(_)) => true,
            (AppState::Monitor(_), AppState::Browser(_)) => true,
            // ...
        }
    }

    pub fn transition(self, action: Action) -> Result<AppState, InvalidTransition> {
        // ...
    }
}
```

**Files:**
- `src/tui/state_machine.rs` - new module
- `src/tui/app.rs` - use state machine

---

### 4.4 Memoization

**Goal:** Cache expensive computations.

**Implementation:**
```rust
// In state.rs
pub struct RenderCache {
    json_formatted: HashMap<String, String>,
    dag_layout: Option<DagLayout>,
    last_update: Instant,
}

impl RenderCache {
    pub fn get_or_format_json(&mut self, key: &str, value: &Value) -> &str {
        self.json_formatted.entry(key.to_string()).or_insert_with(|| {
            serde_json::to_string_pretty(value).unwrap_or_default()
        })
    }

    pub fn invalidate(&mut self) {
        self.json_formatted.clear();
        self.dag_layout = None;
    }
}
```

**Files:**
- `src/tui/cache.rs` - render cache
- `src/tui/state.rs` - integrate cache
- `src/tui/views/monitor.rs` - use cache

---

### 4.5 Snapshot Tests

**Goal:** Visual regression tests for panels.

**Implementation:**
```rust
#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    #[test]
    fn test_monitor_progress_panel() {
        let state = TuiState::mock_with_tasks(3);
        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));

        MonitorView::default().render_progress(
            Rect::new(0, 0, 40, 12),
            &mut buf,
            &Theme::default(),
            &state,
        );

        assert_snapshot!(buffer_to_string(&buf));
    }
}
```

**Files:**
- `src/tui/views/monitor.rs` - tests module
- `src/tui/views/browser.rs` - tests module
- `tests/snapshots/*.snap` - snapshot files

---

## Execution Order

```
Phase 1: TIER 1 Quick Wins
├── 1.1 Clipboard copy
├── 1.2 Retry workflow
├── 1.3 MCP navigation
├── 1.4 Sparkline widget
└── 1.5 Workflow filtering

Phase 2: TIER 4 Code Quality (enables TIER 2/3)
├── 4.2 Widget extraction (reduces complexity)
├── 4.4 Memoization (improves perf)
├── 4.1 Lazy rendering
├── 4.3 State machine
└── 4.5 Snapshot tests

Phase 3: TIER 2 UX Enhancements
├── 2.4 Theme toggle
├── 2.5 Export trace
├── 2.3 Breakpoints
├── 2.2 History
└── 2.1 Global search

Phase 4: TIER 3 Advanced Features
├── 3.1 Mouse support
├── 3.3 Timeline widget
├── 3.4 Notifications
├── 3.2 Bindings diff
└── 3.5 Replay mode
```

---

## Dependencies to Add

```toml
# Cargo.toml
[dependencies]
arboard = "3.4"  # Clipboard

[dependencies.notify-rust]
version = "4.10"
optional = true

[features]
default = ["tui"]
tui = ["ratatui", "crossterm"]
notifications = ["notify-rust"]
```

---

## Success Criteria

- [ ] All 20 enhancements implemented
- [ ] 100% test coverage on new widgets
- [ ] Snapshot tests for all panels
- [ ] No performance regression (60fps target)
- [ ] Documentation updated
