# Phase 4: Command View Fusion — Detailed Implementation Plan

> **Branch:** `feat/tui-phase4-command-fusion`
> **Created:** 2026-03-21
> **Baseline:** 2105 TUI tests pass, 0 warnings
> **Depends on:** Phase 1 (cleanup) ✅, Phase 2.1 (state decomp) ✅, Phase 3 (3-view) ✅

---

## Architecture Target

```
CURRENT (Ctrl+M toggles between 2 separate views):
┌──────────────────────────────────────────┐
│ CommandView                              │
│  ├─ mode: Chat → renders full ChatView   │
│  └─ mode: Monitor → renders full Monitor │
└──────────────────────────────────────────┘

TARGET (unified 2-column layout):
┌──────────────────────────────┬───────────────────┐
│ CONVERSATION TIMELINE  (65%) │ INSTRUMENTS (35%) │
│ messages + inline TaskBoxes  │ DAG + Metrics     │
│ + execution events           │ + MCP status      │
│ + input bar                  │ (collapsible [)   │
├──────────────────────────────┤                   │
│ > input...                   │                   │
└──────────────────────────────┴───────────────────┘
  Ctrl+M: toggle to full Monitor 4-panel view
```

---

## Phase 4A — 2-Column Layout in CommandView

### Task 4A.1: Add instruments_visible flag + layout logic

**File:** `views/command.rs`

```rust
pub struct CommandView {
    pub mode: CommandMode,
    pub chat: ChatView,
    pub monitor: MonitorView,
    pub instruments_visible: bool,  // NEW — toggle with [
}
```

In `render()`, when `mode == Chat`:
```rust
if self.instruments_visible {
    let [left, right] = Layout::horizontal([
        Constraint::Percentage(65),
        Constraint::Percentage(35),
    ]).areas(area);
    self.chat.render(frame, left, state, theme);
    self.render_instruments(frame, right, state, theme);
} else {
    self.chat.render(frame, area, state, theme);
}
```

Add `[` key handler to toggle `instruments_visible`.

**Commit:** `feat(tui): add instruments panel toggle to CommandView`

### Task 4A.2: Create render_instruments() method

**File:** `views/command.rs`

```rust
fn render_instruments(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
    // Stack instruments vertically
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(compat::SLATE_700));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split into sections based on what's active
    let mut sections = Vec::new();
    if state.workflow.task_count > 0 {
        sections.push(("DAG", 40));
        sections.push(("Metrics", 30));
    }
    if !state.mcp.calls.is_empty() {
        sections.push(("MCP", 30));
    }
    // Render each section...
}
```

Uses existing `DagAscii` for DAG, `TuiState` metrics for Metrics, `McpState` for MCP.

**Commit:** `feat(tui): implement instruments panel with DAG + Metrics + MCP`

### Task 4A.3: Render execution events inline in chat

**File:** `views/command.rs` + `views/chat/task_boxes.rs`

When a workflow starts (`RunWorkflow` action), the CommandView should:
1. Add a "workflow started" message to the chat timeline
2. As TaskStarted/Completed events arrive, create inline TaskBoxes in the chat
3. The instruments panel shows the DAG + metrics updating in real-time

This requires connecting `TuiState` events to `ChatView.inline_content`. The existing `poll_runtime_events()` in `app/events.rs` updates `TuiState` — we need to ALSO update the ChatView's inline content.

**File:** `app/events.rs` — In `poll_runtime_events()`:
```rust
// After updating TuiState, also create inline TaskBoxes in CommandView
match &event.kind {
    EventKind::TaskStarted { task_id, verb, .. } => {
        // Create TaskBox and add to command_view.chat.inline_content
        let task_box = match verb.as_ref() {
            "infer" => TaskBox::Infer(InferBox::new().with_model("...")),
            "exec" => TaskBox::Exec(ExecBox::new("...")),
            // ...
        };
        self.command_view.chat.add_inline_task(task_id, task_box);
    }
    EventKind::TaskCompleted { task_id, output, duration_ms, .. } => {
        self.command_view.chat.complete_inline_task(task_id, output, *duration_ms);
    }
    // ...
}
```

**Commit:** `feat(tui): render execution events as inline TaskBoxes in Command chat`

---

## Phase 4B — TaskBox v2 Upgrades

### Task 4B.1: Add BuiltinHint enum for nika:* tool specialization

**File:** `widgets/task_box/invoke.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinHint {
    Generic,
    FileRead,      // nika:read, nika:glob, nika:grep
    FileWrite,     // nika:write, nika:edit
    MediaThumbnail,// nika:thumbnail, nika:convert, nika:strip
    MediaPipeline, // nika:pipeline
    Import,        // nika:import
    Assert,        // nika:assert
    Complete,      // nika:complete
    Sleep,         // nika:sleep
}

impl BuiltinHint {
    pub fn from_tool_name(name: &str) -> Self { ... }
}
```

Add `builtin_hint: BuiltinHint` field to `InvokeBox`. In compact render, use specialized display:
- `nika:read` → `📖 /src/main.rs │ 247 lines │ 8.2 KB`
- `nika:pipeline` → `⊡ [thumbnail → strip → optimize → convert:webp] │ 1.2MB → 32KB`
- Generic → current JSON display

**Commit:** `feat(tui-widgets): add BuiltinHint for specialized nika:* tool display`

### Task 4B.2: Add subsystem badges to TaskBox rendering

**File:** `widgets/task_box/{infer,agent}.rs`

After the main content, render subsystem badges using `micro::guardrail_badges()`, `micro::structured_output_layers()`, `micro::vision_indicator()`:

```rust
// In InferBox to_list_items(), after response section:
if has_structured_output {
    items.push(ListItem::new(micro::structured_output_layers(&layers)));
}
if has_guardrails {
    items.push(ListItem::new(micro::guardrail_badges(&guards)));
}
if has_vision {
    items.push(ListItem::new(micro::vision_indicator(img_count, bytes, ms)));
}
```

**Commit:** `feat(tui-widgets): add structured/guardrail/vision badges to TaskBoxes`

### Task 4B.3: Wire throughput + timing micro-widgets

**File:** `widgets/task_box/{infer,fetch,invoke}.rs`

- InferBox: Add `micro::throughput_meter()` in metrics footer (replace raw tok/s display)
- FetchBox: Add `micro::timing_waterfall()` after HTTP response (dns/connect/tls/ttfb/transfer)
- InvokeBox: Add `micro::latency_sparkline()` showing last 10 call latencies
- AgentBox: Add `micro::agent_turn_progress()` in header

**Commit:** `feat(tui-widgets): wire throughput, waterfall, latency micro-widgets into TaskBoxes`

### Task 4B.4: Connect pulse_intensity animation

**File:** `views/chat/mod.rs` (tick method) + `widgets/task_box/mod.rs`

Currently `TaskBox::pulse_intensity(frame)` computes a sine wave but the value is never written back to the box. Fix:

```rust
// In ChatView::tick()
for content in &mut self.inline_content {
    if let InlineContent::Task(task_box) = content {
        if task_box.state().is_running() {
            let intensity = TaskBox::pulse_intensity(self.frame as u64);
            task_box.set_pulse_intensity(intensity);
        }
        task_box.tick();
    }
}
```

**Commit:** `fix(tui-widgets): connect pulse_intensity to running TaskBox borders`

### Task 4B.5: Add compact mode for InferBox + ExecBox

**File:** `widgets/task_box/{infer,exec}.rs`

Currently InferBox and ExecBox have no compact single-line mode (FetchBox and InvokeBox do). Add:

```rust
// InferBox compact:
// ⚡ INFER: summarize  ✅ 2.3s │ 1.2k→847 tok │ $0.018

// ExecBox compact:
// 📟 EXEC: npm run build  ✅ exit 0 │ 12.4s │ 247 stdout
```

**Commit:** `feat(tui-widgets): add compact render mode for InferBox and ExecBox`

---

## Phase 4C — Remaining Cleanup

### Task 4C.1: Remove ChatOverlayState

**Files:** `state/chat_overlay.rs`, `state/mod.rs`, `state/ui.rs`, `session.rs`

ChatOverlayState is the OLD chat system (before ChatView). ChatView has its own message system. Remove:
1. Delete `state/chat_overlay.rs`
2. Remove `ChatOverlayState` from `UiState` struct in `state/ui.rs`
3. Remove re-exports from `state/mod.rs`
4. Update `session.rs` to work with ChatView's session system
5. Update `app/routing.rs` scroll handlers that reference `chat_overlay`
6. Fix all compilation errors

**Commit:** `refactor(tui): remove ChatOverlayState (replaced by ChatView session system)`

### Task 4C.2: Remove stale ViewAction variants + type aliases

**File:** `views/mod.rs`

Remove unused ViewAction variants (identified in E2E audit):
- `SendChatMessage` (logs only, no inference)
- `ToggleChatOverlay` (status message only)
- `ValidateWorkflow` (logs only)
- `ChatMcp` (status message only)

Remove stale type aliases:
- `BrowseView` (alias for HomeView)
- `RunnerView` (alias for MonitorView)
- `EditorView` (alias for YamlEditorPanel)

**Commit:** `chore(tui): remove dead ViewAction variants and stale type aliases`

---

## Phase 4D — Instruments Panel

### Task 4D.1: Create InstrumentPanel trait

**File:** `views/command/instruments.rs` (new file)

```rust
pub trait InstrumentPanel {
    fn title(&self) -> &str;
    fn is_visible(&self, state: &TuiState) -> bool;
    fn min_height(&self) -> u16;
    fn render(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme);
}
```

**Commit:** `feat(tui): create InstrumentPanel trait for modular instruments`

### Task 4D.2: Implement DagInstrument

**File:** `views/command/instruments.rs`

Wraps existing `DagAscii` widget. Visible when `state.workflow.task_count > 0`.

```rust
pub struct DagInstrument;

impl InstrumentPanel for DagInstrument {
    fn title(&self) -> &str { "DAG" }
    fn is_visible(&self, state: &TuiState) -> bool {
        state.workflow.task_count > 0
    }
    fn min_height(&self) -> u16 { 6 }
    fn render(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Build NodeBoxData from state.tasks, render DagAscii
    }
}
```

**Commit:** `feat(tui): implement DagInstrument wrapping DagAscii`

### Task 4D.3: Implement MetricsInstrument

**File:** `views/command/instruments.rs`

Shows elapsed time (big), task progress bar, token throughput sparkline, cost accumulator, provider info. Uses micro-widgets.

**Commit:** `feat(tui): implement MetricsInstrument with F1 telemetry`

### Task 4D.4: Implement McpInstrument

**File:** `views/command/instruments.rs`

Shows connected MCP servers with status lights, call counts, latency sparklines. Uses `micro::mcp_server_status()`.

**Commit:** `feat(tui): implement McpInstrument with server status + latency`

### Task 4D.5: Instrument stack renderer

**File:** `views/command.rs`

Update `render_instruments()` to use the trait-based system:

```rust
fn render_instruments(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
    let instruments: Vec<Box<dyn InstrumentPanel>> = vec![
        Box::new(DagInstrument),
        Box::new(MetricsInstrument),
        Box::new(McpInstrument),
    ];

    let visible: Vec<_> = instruments.iter()
        .filter(|i| i.is_visible(state))
        .collect();

    // Calculate layout constraints
    let constraints: Vec<_> = visible.iter()
        .map(|i| Constraint::Min(i.min_height()))
        .collect();

    let areas = Layout::vertical(constraints).split(area);

    for (i, instrument) in visible.iter().enumerate() {
        let block = Block::default()
            .title(instrument.title())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);
        let inner = block.inner(areas[i]);
        frame.render_widget(block, areas[i]);
        instrument.render(frame, inner, state, theme);
    }
}
```

**Commit:** `feat(tui): implement instruments stack renderer with auto-layout`

---

## Execution Order

```
4A.1 instruments_visible flag
  ↓
4A.2 render_instruments() method
  ↓
4D.1 InstrumentPanel trait
  ↓
4D.2-4 DagInstrument + MetricsInstrument + McpInstrument
  ↓
4D.5 Stack renderer
  ↓  ← CHECKPOINT: instruments panel working
4B.1 BuiltinHint enum
  ↓
4B.2 Subsystem badges
  ↓
4B.3 Micro-widgets wiring
  ↓
4B.4 Pulse animation fix
  ↓
4B.5 Compact mode InferBox/ExecBox
  ↓  ← CHECKPOINT: TaskBox v2 complete
4A.3 Inline execution events
  ↓  ← CHECKPOINT: full fusion working
4C.1 Remove ChatOverlayState
  ↓
4C.2 Remove dead ViewActions + aliases
  ↓  ← CHECKPOINT: cleanup done
FINAL: Push, code review, merge
```

---

## Verification Protocol

After EACH task:
```
cargo check --features tui       → 0 warnings
cargo test --lib --features tui  → all pass
cargo clippy --features tui      → clean
git commit (conventional)
```

After EACH checkpoint:
```
Launch code-reviewer agent
Fix findings
git push
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| ChatOverlay removal breaks session persistence | Test session save/load before and after |
| Instruments panel causes layout issues on small terminals | Add MIN_WIDTH guard + responsive fallback |
| Inline execution events conflict with manual chat | Buffer events separately, merge on render |
| Pulse animation causes high CPU in idle | Only pulse when is_running(), skip when idle |
| BuiltinHint dispatch overhead | Computed once in constructor, not per-frame |
