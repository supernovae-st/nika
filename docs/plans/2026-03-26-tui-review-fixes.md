# TUI Review Fixes — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all CRITICAL + HIGH bugs and top performance regressions found in the 8-agent deep review of `nika-tui` (86k LOC).

**Architecture:** Fixes are grouped into batches by risk level: state correctness first (show-stoppers that corrupt workflow data), then input/keyboard correctness (navigation broken), then perf hot-paths (allocs in render loop), then UX polish. Each task is self-contained and testable.

**Tech Stack:** Rust, ratatui 0.28, indicatif, tokio, crossterm, nika-tui (86k LOC)

**Test command:** `cargo test -p nika-tui --lib 2>&1 | tail -20`
**All-workspace check:** `cargo test --workspace --lib 2>&1 | tail -10`

---

## BATCH 1 — State management show-stoppers

### Task 1: Kill Running tasks on WorkflowFailed / WorkflowAborted

**Why:** On workflow failure, any task still in `TaskStatus::Running` stays spinning forever. The UI shows live spinners for dead tasks until TUI restart. Also `current_task` is not cleared on failure, so the "active task" display shows stale output.

**File:** `tools/nika-tui/src/state/event_handler/workflow.rs`

**Step 1: Find the handlers**

Read lines 40–90 of `workflow.rs`. You'll find:
- `on_workflow_failed` — sets `phase = Abort` but never touches tasks
- `on_workflow_aborted` — receives `running_tasks` parameter but ignores it entirely

**Step 2: Fix `on_workflow_failed`**

After setting `self.workflow.phase = MissionPhase::Abort`, add:
```rust
// Kill any still-Running tasks (they will never get a TaskFailed event)
for task in self.tasks.values_mut() {
    if task.status == TaskStatus::Running {
        task.status = TaskStatus::Failed;
        if task.error.is_none() {
            task.error = Some(error.to_string());
        }
    }
}
self.current_task = None;
```

**Step 3: Fix `on_workflow_aborted`**

The `running_tasks: &[Arc<str>]` parameter is currently unused. Use it:
```rust
for task_id in running_tasks {
    if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
        if task.status == TaskStatus::Running {
            task.status = TaskStatus::Skipped;
        }
    }
}
self.current_task = None;
```

**Step 4: Fix `on_workflow_failed` — clear `current_task`**

Already done above. Verify `on_workflow_completed` also clears it (it should already).

**Step 5: Verify existing tests still pass**

```bash
cargo test -p nika-tui --lib -- state 2>&1 | tail -20
```

**Step 6: Commit**

```bash
git add tools/nika-tui/src/state/event_handler/workflow.rs
git commit -m "fix(tui): kill Running tasks on WorkflowFailed/Aborted, clear current_task"
```

---

### Task 2: Fix agent state leak — call `reset()` on agent start, fix retry reset

**Why:** `on_agent_start` manually clears some fields but not `spawned_agents`, so sub-agents from a previous run pollute the next run. `reset_for_retry` skips `Running` and `Skipped` tasks, leaving them in wrong state.

**Files:**
- `tools/nika-tui/src/state/event_handler/agent.rs`
- `tools/nika-tui/src/state/workflow_ops.rs`

**Step 1: Fix `on_agent_start` in `agent.rs`**

Find `on_agent_start` (lines ~10–25). Replace the manual field clearing with `self.agent.reset()`, then set `max_turns` after:
```rust
pub(super) fn on_agent_start(&mut self, max_turns: u32, ...) {
    self.agent.reset();              // clears turns, buffer, spawned_agents, max_turns
    self.agent.max_turns = Some(max_turns);
    self.dirty.reasoning = true;
}
```

**Step 2: Fix `reset_for_retry` in `workflow_ops.rs`**

Find the task reset loop (~line 121). Change condition from `status == Failed` to any non-Success state:
```rust
for (task_id, task) in self.tasks.iter_mut() {
    if !matches!(task.status, TaskStatus::Success) {
        task.status = TaskStatus::Pending;
        task.duration_ms = None;
        task.error = None;
        task.output = None;
        reset_tasks.push(task_id.clone());
    }
}
```

**Step 3: Fix token threshold notifications firing repeatedly (provider.rs)**

File: `tools/nika-tui/src/state/event_handler/provider.rs` lines ~79–122.

The 50%/70%/85%/95% fuel notifications fire on *every* `ProviderResponded` above the threshold. Add flags to prevent re-firing. Find where `self.metrics` is defined and add boolean fields, or use a bitmask:

```rust
// In the notification block, guard each threshold:
let pct = (total_tokens * 100) / CONTEXT_WINDOW;
if pct >= 95 && !self.metrics.notified_95pct {
    self.metrics.notified_95pct = true;
    self.add_notification(Notification::warning("🔴 Context 95% full"));
} else if pct >= 85 && !self.metrics.notified_85pct {
    self.metrics.notified_85pct = true;
    self.add_notification(Notification::warning("🟠 Context 85% full"));
} else if pct >= 70 && !self.metrics.notified_70pct {
    self.metrics.notified_70pct = true;
    self.add_notification(Notification::info("🟡 Context 70% full"));
} else if pct >= 50 && !self.metrics.notified_50pct {
    self.metrics.notified_50pct = true;
    self.add_notification(Notification::info("Heating up... 50% context"));
}
```

Add `notified_50pct/70pct/85pct/95pct: bool` to the `Metrics` struct (wherever it's defined, likely `state/mod.rs` or a metrics module). Reset all flags in `reset_for_retry`.

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- state 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/state/event_handler/agent.rs tools/nika-tui/src/state/workflow_ops.rs tools/nika-tui/src/state/event_handler/provider.rs
git commit -m "fix(tui): agent reset on start, retry resets Running/Skipped, dedupe threshold notifications"
```

---

### Task 3: Fix duplicate task_order entries + on_task_failed clears current_task

**Why:** `on_task_scheduled` appends to `task_order` even on re-schedule, duplicating IDs. `on_task_failed` never clears `current_task`.

**File:** `tools/nika-tui/src/state/event_handler/task.rs`

**Step 1: Fix `on_task_scheduled`**

Find `on_task_scheduled` (~line 16). Guard the `task_order.push`:
```rust
let was_new = self.tasks.insert(task_id.to_string(), task).is_none();
if was_new {
    self.task_order.push(task_id.to_string());
}
```

**Step 2: Fix `on_task_failed` — clear current_task**

Find `on_task_failed` (~line 98). After updating task status, add:
```rust
if self.current_task.as_deref() == Some(task_id) {
    self.current_task = None;
}
```

**Step 3: Fix `on_workflow_aborted` — also clear current_task**

File: `workflow.rs`. Already handled in Task 1. Verify it's there.

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- state 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/state/event_handler/task.rs
git commit -m "fix(tui): prevent duplicate task_order entries, clear current_task on failure"
```

---

## BATCH 2 — Input / keyboard show-stoppers

### Task 4: Fix Escape swallowed before overlays can close

**Why:** The global Escape handler in `events.rs` fires before `dispatch_to_current_view`, so provider modal, command palette, and help overlay can never be dismissed with Escape. The user is stuck.

**File:** `tools/nika-tui/src/app/events.rs` lines ~143–146

**Step 1: Read the file**

Read `events.rs` lines 130–200 to understand the full Escape + dispatch flow.

**Step 2: Add an overlay-check helper**

Find where `dispatch_to_current_view` is called. Before the global Escape grab, check if the current view has a modal open. The simplest approach is to let the view handle Escape first and only run the global handler if the view returned `ViewAction::None`:

```rust
// Dispatch to view FIRST — views handle their own overlays
let view_action = self.dispatch_to_current_view(key_event);

// Only run global Escape logic if view didn't consume it
if view_action == ViewAction::None && code == KeyCode::Esc {
    if self.input_mode != InputMode::Normal {
        self.input_mode = InputMode::Normal;
        return Action::Continue;
    }
}
```

Reorder accordingly. Ensure `dispatch_to_current_view` is called only once (move it before the Escape check, remove the call after).

**Step 3: Verify tests**

```bash
cargo test -p nika-tui --lib -- app 2>&1 | tail -20
```

**Step 4: Commit**

```bash
git add tools/nika-tui/src/app/events.rs
git commit -m "fix(tui): dispatch to view before global Escape grab so overlays can close"
```

---

### Task 5: Fix 'q' quits app instead of exiting Monitor mode

**Why:** In Monitor mode (Command view), `'q'` should return to Chat mode. But the app-level quit check in `handle_unified_key` intercepts `'q'` *before* dispatching to the view.

**File:** `tools/nika-tui/src/app/events.rs` lines ~135–140 + `tools/nika-tui/src/views/command.rs` lines ~393–400

**Step 1: Read both files**

Read the quit check section in `events.rs` and the Monitor-mode `'q'` handler in `command.rs`.

**Step 2: Fix the ordering**

The fix from Task 4 (dispatch to view first) already partially solves this. Ensure the quit check runs AFTER dispatch:

```rust
// events.rs — after view dispatch
if view_action == ViewAction::None {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') if ... => {
            return Action::Quit;
        }
        // ... other global shortcuts
    }
}
```

Verify `CommandView` handles `'q'` in Monitor mode before returning `None`.

**Step 3: Commit**

```bash
git add tools/nika-tui/src/app/events.rs
git commit -m "fix(tui): 'q' exits Monitor mode before reaching app-level quit handler"
```

---

### Task 6: Fix Esc from Conversation panel → Studio (should restore input focus)

**Why:** Pressing Escape while reading chat history (Conversation panel focused) sends the user to the Studio view. It should return focus to the Input panel.

**File:** `tools/nika-tui/src/views/chat/keys.rs` lines ~761–769

**Step 1: Read the Escape handler**

Find the block where `focused_panel != ChatPanel::Input` returns `ViewAction::SwitchView(TuiView::Studio)`.

**Step 2: Change the behavior**

Replace:
```rust
// OLD:
_ => Some(ViewAction::SwitchView(TuiView::Studio)),
```

With:
```rust
// NEW: Esc always returns focus to input — use Ctrl+W or view shortcut to leave
_ => {
    self.focus_panel(ChatPanel::Input);
    Some(ViewAction::None)
}
```

**Step 3: Fix provider modal close — restore focus**

In the `ModalAction::Close` handler (~line 217), after `self.provider_modal.visible = false`, add:
```rust
self.focus_panel(ChatPanel::Input);
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- views 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/views/chat/keys.rs
git commit -m "fix(tui): Esc from Conversation panel restores input focus, not Studio switch"
```

---

## BATCH 3 — Chat agent correctness

### Task 7: Fix chat history corrupted on inference error + errors silently dropped

**Why:** If `infer_stream` fails mid-stream, `streaming_state.finish()` is never called (leaked `is_streaming = true`) and the user message is left dangling in history with no assistant reply. Subsequent calls are corrupted. Errors are also never surfaced to the user.

**File:** `tools/nika-tui/src/chat_agent/inference.rs`

**Step 1: Read the file**

Read `inference.rs` completely to understand the stream + history flow.

**Step 2: Wrap the call in a cleanup guard**

Restructure `infer` to always finish streaming and rollback on error:

```rust
pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
    // Push user message
    self.history.push(ChatMessage::user(prompt));
    self.streaming_state.start();

    // Run inference — capture result without early-returning
    let result = self.run_inference_inner(prompt).await;

    // Always clean up streaming state
    self.streaming_state.finish();

    match result {
        Ok(response) => {
            self.history.push(ChatMessage::assistant(&response));
            Ok(response)
        }
        Err(e) => {
            // Roll back dangling user message
            self.history.pop();
            Err(e)
        }
    }
}
```

Extract the actual inference work into `run_inference_inner` (private method).

**Step 3: Surface errors to caller**

The caller (wherever `chat_agent.infer()` is spawned) must handle `Err`. Find the spawn site and add error handling that calls `overlay_state.finish_streaming()` and pushes an error message:

```rust
match result {
    Ok(text) => { /* push assistant message */ }
    Err(e) => {
        overlay_state.is_streaming = false;
        overlay_state.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::System,
            format!("⚠ Error: {}", e),
        ));
    }
}
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- chat 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/chat_agent/inference.rs
git commit -m "fix(tui): chat history not corrupted on inference error, surface errors to user"
```

---

### Task 8: Fix streaming partial_response shown in PROMPT section

**Why:** `infer.rs` lines 349–354 render `ctx.partial_response` inside the PROMPT section header, not the RESPONSE section. During streaming, the user sees LLM output appear under "PROMPT" — completely wrong.

**File:** `tools/nika-tui/src/widgets/task_box/infer.rs` lines ~349–354

**Step 1: Remove the misplaced streaming block**

Find and delete the block:
```rust
// DELETE THIS:
if ctx.is_streaming && !ctx.partial_response.is_empty() {
    let prompt_line = Line::from(vec![
        Span::styled("│ ", border_style),
        Span::styled(format!("┊ {}", ctx.partial_response), content_style),
    ]);
    items.push(ListItem::new(prompt_line));
}
```

The partial response is already correctly rendered in the RESPONSE section further down.

**Step 2: Verify the RESPONSE section already handles streaming**

Read lines 414–464 to confirm the response section shows `partial_response` during streaming. If it doesn't, add the live streaming display there instead.

**Step 3: Run tests**

```bash
cargo test -p nika-tui --lib -- widgets 2>&1 | tail -20
```

**Step 4: Commit**

```bash
git add tools/nika-tui/src/widgets/task_box/infer.rs
git commit -m "fix(tui): streaming partial_response was shown in PROMPT section, remove misplacement"
```

---

## BATCH 4 — Performance CRITICAL (hot render paths)

### Task 9: Cache stdout/stderr line counts — eliminate O(n) scan per frame

**Why:** `exec.rs` calls `self.stdout.lines().count()` every render frame for every visible ExecBox. For a long-running command with KB of output, this is a full string scan at 30fps. Same for stderr.

**File:** `tools/nika-tui/src/widgets/task_box/exec.rs`

**Step 1: Add cached count fields to ExecBox**

Find the `ExecBox` struct definition. Add:
```rust
stdout_line_count: usize,
stderr_line_count: usize,
```

**Step 2: Increment in append methods**

In `append_stdout(chunk: &str)`:
```rust
self.stdout_line_count += chunk.chars().filter(|&c| c == '\n').count();
// (or use bytecount for speed: chunk.bytes().filter(|&b| b == b'\n').count())
```
Same for `append_stderr`.

**Step 3: Replace the hot-path calls**

Find lines ~206 and ~304 where `.lines().count()` is called and replace with `self.stdout_line_count` / `self.stderr_line_count`.

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- task_box 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/task_box/exec.rs
git commit -m "perf(tui): cache stdout/stderr line counts, eliminate O(n) scan per frame in ExecBox"
```

---

### Task 10: Cache single-line response — eliminate full clone per frame in InferBox

**Why:** `infer.rs` calls `self.response.replace('\n', " ")` on every frame in both the collapsed prompt render (line 374) and collapsed response render (line 457). For a 10KB response this is 10KB allocated and discarded every frame.

**File:** `tools/nika-tui/src/widgets/task_box/infer.rs`

**Step 1: Add a cached first-line field**

In the `InferBox` struct, add:
```rust
response_first_line: String,   // cached: first line of response, for collapsed mode
```

**Step 2: Update it in `append_response`**

In the `append_response` method (or wherever response text is set):
```rust
// Update first-line cache — find first \n or end of string
let end = self.response.find('\n').unwrap_or(self.response.len());
let end = end.min(200); // cap at 200 chars max
self.response_first_line = self.response[..end].to_string();
```

**Step 3: Use it in collapsed render**

Replace lines 374 and 457:
```rust
// OLD: format!("...", self.response.replace('\n', " "))
// NEW:
let preview = Self::truncate(&self.response_first_line, 60);
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- task_box 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/task_box/infer.rs
git commit -m "perf(tui): cache response first-line, eliminate full String clone per frame in InferBox"
```

---

### Task 11: Cache status bar hints — eliminate Vec allocation every frame

**Why:** `status_bar.rs` builds a `Vec<KeyHint>` via `default_hints()` inside `Widget::render` every frame. This allocates 60+ times/second at idle.

**File:** `tools/nika-tui/src/widgets/status_bar.rs`

**Step 1: Add hint cache to App or wherever StatusBar is constructed**

The hints only change when the view or input mode changes. Find where `StatusBar` is created in `render.rs` (likely in the render closure). Add cached hints to `App`:

```rust
// In App struct:
cached_hints: Vec<KeyHint>,
cached_hints_key: (TuiView, Option<InputMode>),
```

**Step 2: Compute hints only on change**

In the pre-render section (before the terminal.draw closure):
```rust
let hints_key = (current_view, input_mode_opt);
if hints_key != self.cached_hints_key {
    self.cached_hints = StatusBar::compute_hints(current_view, input_mode_opt, &self.keybindings);
    self.cached_hints_key = hints_key;
}
```

Extract `default_hints` logic into a static `compute_hints` function that takes `(TuiView, Option<InputMode>)` and returns `Vec<KeyHint>`.

**Step 3: Pass cached hints into StatusBar**

Change `StatusBar::new(...)` to accept `hints: &[KeyHint]` instead of recomputing:
```rust
StatusBar::new(frame, mode, metrics, &self.cached_hints, theme)
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- display 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/status_bar.rs tools/nika-tui/src/app/mod.rs tools/nika-tui/src/app/render.rs
git commit -m "perf(tui): cache status bar hints, eliminate Vec allocation every render frame"
```

---

### Task 12: Cache sparkline data in TokenVelocity

**Why:** `sparkline_chars()` and `samples()` both allocate a new `String`/`Vec<f32>` every frame for every visible InferBox / AgentBox. Called at 30fps × N visible tasks.

**File:** `tools/nika-tui/src/widgets/task_box/token_velocity.rs`

**Step 1: Add cache fields**

In `TokenVelocity`:
```rust
cached_sparkline: String,
sparkline_dirty: bool,
```

**Step 2: Invalidate on `push`**

In `push(&mut self, tokens_per_sec: f32)`:
```rust
self.sparkline_dirty = true;
```

**Step 3: Lazy recompute in `sparkline_chars`**

Change `sparkline_chars` to take `&mut self` and return `&str`:
```rust
pub fn sparkline_chars(&mut self) -> &str {
    if self.sparkline_dirty {
        self.cached_sparkline = self.build_sparkline();
        self.sparkline_dirty = false;
    }
    &self.cached_sparkline
}
```

**Step 4: Fix `min()` returning infinity**

Change the fold identity from `f32::INFINITY` to `0.0`:
```rust
pub fn min(&self) -> f32 {
    self.samples.iter().copied().fold(0.0_f32, f32::min)
}
```

**Step 5: Run tests**

```bash
cargo test -p nika-tui --lib -- task_box 2>&1 | tail -20
```

**Step 6: Commit**

```bash
git add tools/nika-tui/src/widgets/task_box/token_velocity.rs
git commit -m "perf(tui): cache sparkline string in TokenVelocity, fix min() returning infinity"
```

---

## BATCH 5 — Scroll/bounds bugs

### Task 13: Clamp scroll offsets — task_flow, info panel, standalone

**Why:** Multiple panels allow `scroll_offset` to exceed content height, rendering a blank view. `browser_index` and `history_index` are not clamped after list resizes.

**Files:**
- `tools/nika-tui/src/widgets/panels/task_flow.rs`
- `tools/nika-tui/src/widgets/panels/info.rs`
- `tools/nika-tui/src/standalone/state.rs`

**Step 1: Fix `task_flow.rs` — clamp at top of render**

Find the `render` method. At the very top, before any other logic, add:
```rust
self.scroll_offset = self.scroll_offset.min(self.max_scroll());
```

**Step 2: Fix `info.rs` — track content height, clamp scroll_down**

Add field `rendered_line_count: u16` to `InfoPanel`. At end of `render_task_details`, set `self.rendered_line_count = line_count`.

In `handle_key` for Down/PageDown:
```rust
KeyCode::Down => {
    let max = self.rendered_line_count.saturating_sub(1) as u32;
    self.scroll_offset = (self.scroll_offset + 1).min(max);
    true
}
```

**Step 3: Fix `standalone/state.rs` — clamp browser_index after scan**

In `scan_workflows()` after repopulating `browser_entries`:
```rust
let max_idx = self.browser_entries.len().saturating_sub(1);
self.browser_index = self.browser_index.min(max_idx);
```

In `clear_history()`:
```rust
self.history.clear();
self.history_index = 0;
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/panels/task_flow.rs tools/nika-tui/src/widgets/panels/info.rs tools/nika-tui/src/standalone/state.rs
git commit -m "fix(tui): clamp scroll_offset and list indices after content resize/clear"
```

---

### Task 14: Fix broken date arithmetic in history panel

**Why:** `history.rs` assumes 365 days/year and 30 days/month, producing month 13 for days 360–364 and wrong years since 2026 (off by months due to 14 leap years).

**File:** `tools/nika-tui/src/standalone/history.rs` lines ~63–79

**Step 1: Replace the broken arithmetic**

The simplest correct fix without adding a new crate dependency — use `SystemTime` and convert to a readable string via standard unix division:

```rust
fn timestamp_display(&self) -> String {
    use std::time::UNIX_EPOCH;
    let secs = match self.timestamp.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => return "unknown".to_string(),
    };

    // Days since epoch, using correct Gregorian leap-year algorithm
    let mut days = secs / 86400;
    let mut year = 1970u32;
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let month_days: &[u32] = if is_leap_year(year) {
        &[31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        &[31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut month = 1u32;
    for &md in month_days {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    let time_secs = secs % 86400;
    let hour = time_secs / 3600;
    let min = (time_secs % 3600) / 60;

    format!("{:04}-{:02}-{:02} {:02}:{:02}", year, month, day, hour, min)
}

fn is_leap_year(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}
```

**Step 2: Run tests**

```bash
cargo test -p nika-tui --lib -- standalone 2>&1 | tail -20
```

**Step 3: Write a quick smoke test**

In the test module:
```rust
#[test]
fn timestamp_never_produces_month_13() {
    // 2026-01-01 00:00:00 UTC = 1767225600 seconds
    let entry = HistoryEntry { timestamp: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1767225600), .. };
    let s = entry.timestamp_display();
    assert!(s.starts_with("2026-01"), "got: {}", s);
    // Jan 1 2026 — must not be month 13
    assert!(!s.contains("-13-"), "month 13 found: {}", s);
}
```

**Step 4: Commit**

```bash
git add tools/nika-tui/src/standalone/history.rs
git commit -m "fix(tui): correct Gregorian date arithmetic in history panel, prevent month 13"
```

---

### Task 15: Fix duplicate notifications + notification dedup

**Why:** Notifications fire on every `ProviderResponded` once above threshold (already fixed in Task 2). Additionally, rapid identical events stack up the notification list. Add dedup to `push`.

**File:** `tools/nika-tui/src/state/notification_state.rs`

**Step 1: Add dedup in `push`**

At the start of `push`:
```rust
pub fn push(&mut self, notification: Notification) {
    // Deduplicate consecutive identical notifications
    if let Some(last) = self.items.back() {
        if last.level == notification.level && last.message == notification.message {
            return;
        }
    }
    if self.items.len() >= self.max_items {
        self.items.pop_front();
    }
    self.items.push_back(notification);
}
```

**Step 2: Simplify `active_count` and `active`**

Since `compact()` already removes dismissed items, all items in `self.items` are active:
```rust
pub fn active_count(&self) -> usize {
    self.items.len() // compact() guarantees all items are active
}

pub fn active(&self) -> Vec<&Notification> {
    self.items.iter().collect()
}
```

**Step 3: Commit**

```bash
git add tools/nika-tui/src/state/notification_state.rs
git commit -m "fix(tui): deduplicate consecutive identical notifications, simplify active() post-compact"
```

---

## BATCH 6 — Provider modal + highlight correctness

### Task 16: Fix provider modal — key_input_mode leaked on tab switch, xAI missing, OOB index

**Why:** (1) `key_input_mode = true` when user presses a tab-number key while typing API key → Cloud tab unresponsive. (2) `selected_provider()` in handler doesn't know about xAI (index 6). (3) `selected_idx` goes OOB after native model reload.

**Files:**
- `tools/nika-tui/src/widgets/provider_modal/state/navigation.rs`
- `tools/nika-tui/src/widgets/provider_modal/handler.rs`
- `tools/nika-tui/src/widgets/provider_modal/state/providers.rs`

**Step 1: Fix `switch_tab` — zeroize key input mode**

In `navigation.rs`, `switch_tab`:
```rust
pub fn switch_tab(&mut self, tab: ProviderModalTab) {
    if self.key_input_mode {
        self.key_input_mode = false;
        self.key_input_buffer.clear(); // or .zeroize() if zeroize dep available
    }
    self.active_tab = tab;
    self.selected_idx = 0;
    self.item_count = /* same as before */;
}
```

**Step 2: Fix `selected_provider` — use canonical provider list**

In `handler.rs`, replace the hardcoded match table in `selected_provider` / `selected_cloud_provider_by_idx` with a lookup into `llm_provider_ids()`:
```rust
fn selected_provider(state: &ProviderModalState) -> &'static str {
    crate::providers::llm_provider_ids()
        .get(state.nav.selected_idx)
        .copied()
        .unwrap_or("anthropic")
}
```

(Check the exact function name by reading `providers/mod.rs`.)

**Step 3: Fix `set_native_models` — clamp selected_idx**

In `providers.rs`:
```rust
pub fn set_native_models(&mut self, models: Vec<NativeModelInfo>) {
    self.native_models = models;
    if self.nav.active_tab == ProviderModalTab::Native {
        let max = self.native_models.len().saturating_sub(1);
        self.nav.selected_idx = self.nav.selected_idx.min(max);
        self.nav.item_count = self.native_models.len().max(1);
    }
}
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- provider 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/provider_modal/state/navigation.rs tools/nika-tui/src/widgets/provider_modal/handler.rs tools/nika-tui/src/widgets/provider_modal/state/providers.rs
git commit -m "fix(tui): provider modal — clear key_input on tab switch, xAI support, clamp idx on reload"
```

---

### Task 17: Fix tree-sitter incremental parse discarding its own tree

**Why:** `highlight_incremental` in `treesitter.rs` does an incremental parse then immediately calls `self.highlight(source)` which re-parses from scratch, discarding the incremental result. The incremental parse is completely wasted.

**File:** `tools/nika-tui/src/highlight/treesitter.rs` lines ~275–313

**Step 1: Extract rendering into a private helper**

Create:
```rust
fn render_highlights<'a>(&self, tree: &Tree, source: &'a str) -> Vec<Line<'a>> {
    // Move the per-line rendering logic from highlight() here
    // Takes an already-parsed tree — does NOT call parser.parse()
}
```

**Step 2: Update `highlight` to use the helper**

```rust
pub fn highlight<'a>(&self, source: &'a str) -> Vec<Line<'a>> {
    let mut parser = /* get parser */;
    match parser.parse(source, self.tree.borrow().as_ref()) {
        Some(tree) => {
            *self.tree.borrow_mut() = Some(tree.clone());
            self.render_highlights(&tree, source)
        }
        None => source.lines().map(Line::raw).collect(),
    }
}
```

**Step 3: Update `highlight_incremental` to use the helper**

```rust
pub fn highlight_incremental<'a>(&mut self, source: &'a str, edit: InputEdit) -> Vec<Line<'a>> {
    if let Some(tree) = self.tree.borrow_mut().as_mut() {
        tree.edit(&edit);
    }
    let cached = self.tree.borrow();
    let new_tree = self.parser.borrow_mut().parse(source, cached.as_ref());
    drop(cached);
    match new_tree {
        Some(tree) => {
            let result = self.render_highlights(&tree, source);
            *self.tree.borrow_mut() = Some(tree);
            result
        }
        None => self.highlight(source), // fallback to full parse
    }
}
```

**Step 4: Run tests**

```bash
cargo test -p nika-tui --lib -- highlight 2>&1 | tail -20
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/highlight/treesitter.rs
git commit -m "perf(tui): fix highlight_incremental — stop discarding incremental tree, use shared render_highlights helper"
```

---

## BATCH 7 — Final verification

### Task 18: Full workspace test + clippy

**Step 1: Run all tests**

```bash
cargo test --workspace --lib 2>&1 | tail -30
```

Expected: all pass, 0 failures.

**Step 2: Run clippy**

```bash
cargo clippy -p nika-tui -- -D warnings 2>&1 | head -40
```

Fix any new warnings introduced by the above changes.

**Step 3: Check test count hasn't regressed**

```bash
cargo test -p nika-tui --lib 2>&1 | grep "test result"
```

**Step 4: Final commit if any clippy fixes needed**

```bash
git add -p
git commit -m "fix(tui): clippy warnings from review fixes"
```

---

## Summary

| Batch | Tasks | Issues |
|-------|-------|--------|
| 1 | 1–3 | State: Running tasks stuck, agent leak, duplicate IDs |
| 2 | 4–6 | Input: Esc swallowed, 'q' quits wrong, Esc→Studio |
| 3 | 7–8 | Chat: history corruption, wrong section |
| 4 | 9–12 | Perf: O(n) line counts, full clone, hint alloc, sparkline |
| 5 | 13–15 | Scroll OOB, date arithmetic, notification dedup |
| 6 | 16–17 | Provider modal, tree-sitter incremental |
| 7 | 18 | Final verification |

**Total: ~18 commits, all verified with `cargo test --workspace --lib`.**
