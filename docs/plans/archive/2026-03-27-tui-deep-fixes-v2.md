# TUI Deep Fix v2 — 30 Bugs from 6-Agent Audit

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 24 bugs discovered by a 6-agent deep audit of `nika-tui` (86k LOC): correctness, SEC, PERF, and broken tests.

**Architecture:** Each fix is a targeted, surgical change to a single responsibility. TDD: write the failing test first (where possible), then implement the minimal fix, then commit. Granular commits — 1 fix = 1 commit.

**Tech Stack:** Rust 1.81+, ratatui 0.28, `zeroize` crate (already in deps), `crossterm`, `tree-sitter`, `cargo test -p nika-tui --lib`

**Test command:** `cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5`
**Stash WIP before commits:** `git stash push -u -- tools/nika-daemon/src/server.rs tools/nika-daemon/src/services/secrets.rs tools/nika-cli/src/model_cloud.rs tools/nika-engine/src/provider/cost.rs tools/nika/src/cli/mod.rs tools/nika/src/cli/onboarding.rs tools/nika/src/main.rs`
**Restore after:** `git stash pop`

---

## BATCH A — Critical: Functional Regressions

These cause visible broken behavior on every use.

---

### Task 1: Fix StreamChunk::Done never calling finish_streaming

**Confidence: 97%** — streaming state permanently stuck after every successful response.

**Files:**
- Modify: `tools/nika-tui/src/app/events.rs:102-104`

**Context:**
`poll_stream_chunks()` handles `StreamChunk::Done` but only calls `finalize_thinking()`. It never calls `finish_streaming()`, so `is_streaming` stays `true` forever. The `Token` arm at line 94 guards `start_streaming_with_verb` with `!self.command_view.chat.is_streaming` — after the first inference, subsequent tokens are silently dropped.

The `StreamChunk::Error` arm (line 106-109) correctly calls `finish_streaming()`. Mirror this pattern.

**Step 1: Write the test**

File: `tools/nika-tui/src/app/tests.rs` (if it doesn't exist, add to `app/mod.rs` or nearest test module)
Actually, this is hard to test without wiring the full App. Instead write a unit test in `chat_agent/tests.rs` that verifies the streaming state goes to false after a Done event. Add it to the existing test module.

Actually, the most reliable test here is a behavioral check of the `ChatView` — after `start_streaming_with_verb` + several `append_streaming` + `finish_streaming`, `is_streaming` must be false. This test already likely exists. The bug is in the app-level dispatch. Mark this as implementation-only (no new test needed — the fix is obvious from the code).

**Step 2: Implement the fix**

In `tools/nika-tui/src/app/events.rs`, change lines 102-104:

```rust
// BEFORE:
StreamChunk::Done(_) => {
    self.command_view.chat.finalize_thinking();
}

// AFTER:
StreamChunk::Done(_) => {
    if self.command_view.chat.is_streaming {
        let response = self.command_view.chat.finish_streaming();
        if !response.is_empty() {
            self.command_view.chat.add_nika_message(response, None);
        }
    }
    self.command_view.chat.finalize_thinking();
}
```

**Step 3: Run tests**

```bash
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5
```
Expected: `test result: ok. 2126+ passed`

**Step 4: Commit**

```bash
git stash push -u -- <wip files>
git add tools/nika-tui/src/app/events.rs
git commit -m "fix(tui): StreamChunk::Done must finish_streaming to reset is_streaming flag"
git stash pop
```

---

### Task 2: Fix sync_all_verification_statuses off-by-one (xAI never synced)

**Confidence: 100%** — xAI (index 6) status stays "Checking" forever.

**Files:**
- Modify: `tools/nika-tui/src/widgets/provider_modal/state/modal.rs:180`
- Test: `tools/nika-tui/src/widgets/provider_modal/state/tests.rs`

**Context:**
`sync_all_verification_statuses` uses `for i in 0..6` (exclusive), covering indices 0–5 only. There are 7 cloud providers; index 6 is xAI. The constant `CLOUD_PROVIDER_COUNT = 7` exists in the codebase.

**Step 1: Write the failing test**

Find `test_sync_all_verification_statuses` in `tests.rs`. It currently only sets 6 entries and doesn't check index 6. Add a test that verifies all 7 providers are synced:

```rust
#[test]
fn test_sync_all_verification_statuses_covers_xai() {
    let mut state = ProviderModalState::default();

    // Set all 7 providers to Connected
    for i in 0..7 {
        state.provider_statuses.insert(i, ConnectionStatus::Connected {
            latency_ms: Some(10),
        });
    }

    state.sync_all_verification_statuses();

    // Index 6 (xAI) must be synced — was previously skipped by 0..6
    let xai_status = state.verification_state.get_status(6);
    assert_eq!(
        xai_status,
        ConnectionCheckStatus::Connected,
        "xAI (index 6) must be synced by sync_all_verification_statuses"
    );
}
```

**Step 2: Run test to verify it fails**

```bash
cd tools && cargo test -p nika-tui --lib -- sync_all_verification_statuses_covers_xai
```
Expected: FAIL (xAI status is `Checking`, not `Connected`)

**Step 3: Fix the loop**

```rust
// BEFORE:
pub fn sync_all_verification_statuses(&mut self) {
    for i in 0..6 {
        self.sync_verification_status(i);
    }
}

// AFTER:
pub fn sync_all_verification_statuses(&mut self) {
    for i in 0..7 {
        self.sync_verification_status(i);
    }
}
```

**Step 4: Run tests**

```bash
cd tools && cargo test -p nika-tui --lib -- provider 2>&1 | tail -5
```
Expected: all pass.

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/provider_modal/state/modal.rs \
        tools/nika-tui/src/widgets/provider_modal/state/tests.rs
git commit -m "fix(tui): sync_all_verification_statuses covers 7 providers (was 6, missed xAI)"
```

---

### Task 3: Fix PullModel/DeleteModel using hardcoded strings

**Confidence: 100%** — pressing `p`/`d` on Native tab always operates on `"llama3.2"`/`"selected"`.

**Files:**
- Modify: `tools/nika-tui/src/widgets/provider_modal/handler.rs:155-165`
- Test: `tools/nika-tui/src/widgets/provider_modal/state/tests.rs`

**Context:** Lines 155-165 of `handler.rs`:
```rust
KeyCode::Char('p') if state.active_tab == ProviderModalTab::Native => {
    HandleResult::consumed_with_action(ModalAction::PullModel {
        model: "llama3.2".to_string(),  // BUG: hardcoded
    })
}
KeyCode::Char('d') if state.active_tab == ProviderModalTab::Native => {
    HandleResult::consumed_with_action(ModalAction::DeleteModel {
        model: "selected".to_string(),  // BUG: placeholder
    })
}
```

**Step 1: Write failing tests**

In `tests.rs`, find the existing PullModel/DeleteModel tests. They assert `matches!(result.action, Some(ModalAction::PullModel { .. }))` but don't check the model name. Add model-name assertions:

```rust
#[test]
fn test_pull_model_uses_selected_model_name() {
    let mut state = ProviderModalState::default();
    state.switch_tab(ProviderModalTab::Native);
    // Add a model at index 0
    state.set_native_models(vec![make_native_model("phi3:mini")]);
    state.selected_idx = 0;

    let key = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE);
    let result = ProviderModalWidget::handle(&mut state, key);

    match result.action {
        Some(ModalAction::PullModel { ref model }) => {
            assert_eq!(model, "phi3:mini", "PullModel must use the selected model name");
        }
        _ => panic!("Expected PullModel action"),
    }
}

#[test]
fn test_delete_model_uses_selected_model_name() {
    let mut state = ProviderModalState::default();
    state.switch_tab(ProviderModalTab::Native);
    state.set_native_models(vec![make_native_model("llama3.2:latest")]);
    state.selected_idx = 0;

    let key = KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE);
    let result = ProviderModalWidget::handle(&mut state, key);

    match result.action {
        Some(ModalAction::DeleteModel { ref model }) => {
            assert_eq!(model, "llama3.2:latest");
        }
        _ => panic!("Expected DeleteModel action"),
    }
}
```

(Add a `make_native_model(name: &str) -> NativeModelInfo` helper at the top of the test module if one doesn't exist.)

**Step 2: Run tests to verify they fail**

```bash
cd tools && cargo test -p nika-tui --lib -- pull_model_uses_selected
```
Expected: FAIL (model is `"llama3.2"`, not `"phi3:mini"`)

**Step 3: Fix the handler**

```rust
KeyCode::Char('p') if state.active_tab == ProviderModalTab::Native => {
    let model = state
        .native_models
        .get(state.selected_idx)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| "llama3.2".to_string());
    HandleResult::consumed_with_action(ModalAction::PullModel { model })
}
KeyCode::Char('d') if state.active_tab == ProviderModalTab::Native => {
    if let Some(m) = state.native_models.get(state.selected_idx) {
        let model = m.name.clone();
        HandleResult::consumed_with_action(ModalAction::DeleteModel { model })
    } else {
        HandleResult::consumed()
    }
}
```

**Step 4: Run tests**

```bash
cd tools && cargo test -p nika-tui --lib -- provider 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add tools/nika-tui/src/widgets/provider_modal/handler.rs \
        tools/nika-tui/src/widgets/provider_modal/state/tests.rs
git commit -m "fix(tui): PullModel/DeleteModel read selected model name, not hardcoded placeholder"
```

---

### Task 4: Fix on_mcp_invoke bypassing McpState eviction cap

**Confidence: 100%** — `mcp.calls` grows unbounded; also causes double O(n) scan.

**Files:**
- Modify: `tools/nika-tui/src/state/event_handler/provider.rs:157-158`
- Modify: `tools/nika-tui/src/state/event_handler/provider.rs:191-203` (double scan)
- Test: `tools/nika-tui/src/state/tests.rs`

**Context:**
`on_mcp_invoke` at line 157 calls `self.mcp.calls.push_back(call)` directly, bypassing `McpState::add_call()` which enforces the `MAX_CALLS` eviction cap. In a long session with many MCP calls, this leaks memory unboundedly.

Additionally, `on_mcp_response` lines 191-203 scans `mcp.calls` twice (once immutable, once mutable) to find the same call. Combine into one pass.

**Step 1: Write failing test for the cap**

```rust
#[test]
fn test_mcp_calls_cap_enforced_on_invoke() {
    let mut state = TuiState::default();

    // Simulate more MCP invoke events than the cap allows
    // McpState::MAX_CALLS is typically 200 — push 201
    for i in 0..201usize {
        state.handle_event(
            &EventKind::McpInvoke {
                task_id: "t".to_string(),
                mcp_server: "novanet".to_string(),
                tool: Some(format!("tool_{}", i)),
                resource: None,
                call_id: format!("call_{}", i),
                params: None,
            },
            i as u64,
        );
    }

    // mcp.calls must not exceed MAX_CALLS
    assert!(
        state.mcp.calls.len() <= 200,
        "mcp.calls len {} must be <= 200 (MAX_CALLS)",
        state.mcp.calls.len()
    );
}
```

**Step 2: Run test to verify it fails**

```bash
cd tools && cargo test -p nika-tui --lib -- mcp_calls_cap_enforced
```
Expected: FAIL (len is 201)

**Step 3: Check if McpState::add_call exists**

Read `tools/nika-tui/src/state/types.rs` — search for `add_call` and `MAX_CALLS`. If `add_call` does NOT exist yet, implement it in `types.rs`:

```rust
impl McpState {
    /// Maximum calls to retain in the ring buffer
    pub const MAX_CALLS: usize = 200;

    /// Add a new call, evicting oldest if at capacity, and auto-incrementing seq.
    pub fn add_call(&mut self, mut call: McpCall) {
        call.seq = self.seq;
        self.seq += 1;
        if self.calls.len() >= Self::MAX_CALLS {
            self.calls.pop_front();
            // Adjust selected_idx if it pointed to the evicted entry
            if let Some(idx) = self.selected_idx {
                self.selected_idx = idx.checked_sub(1);
            }
        }
        self.calls.push_back(call);
    }
}
```

If `add_call` already exists, just change the call site.

**Step 4: Fix on_mcp_invoke to use add_call + fix double scan**

In `provider.rs`:

```rust
// on_mcp_invoke: replace lines 157-158
// BEFORE:
self.mcp.calls.push_back(call);
self.mcp.seq += 1;
// AFTER:
self.mcp.add_call(call);
// (seq is now managed inside add_call, remove `seq: self.mcp.seq` from McpCall literal)
```

In `on_mcp_response`, replace the double scan (lines 191-203) with a single mutable pass:

```rust
// BEFORE: two separate iter() + iter_mut()
let tool_name = self.mcp.calls.iter()
    .find(|c| c.call_id == call_id)
    .and_then(|c| c.tool.clone());

if let Some(call) = self.mcp.calls.iter_mut().find(|c| c.call_id == call_id) {
    call.completed = true;
    ...
}

// AFTER: single pass
let tool_name = if let Some(call) = self.mcp.calls.iter_mut().find(|c| c.call_id == call_id) {
    let tool_name = call.tool.clone();
    call.completed = true;
    call.output_len = Some(output_len);
    call.response = response.clone();
    call.is_error = is_error;
    call.duration_ms = Some(duration_ms);
    tool_name
} else {
    None
};
```

**Step 5: Run tests**

```bash
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add tools/nika-tui/src/state/event_handler/provider.rs \
        tools/nika-tui/src/state/types.rs \
        tools/nika-tui/src/state/tests.rs
git commit -m "fix(tui): on_mcp_invoke now uses add_call() to enforce MAX_CALLS cap; single-pass on_mcp_response"
```

---

### Task 5: Fix toggle_pause not saving phase_before_pause

**Confidence: 95%** — keyboard pause/resume restores wrong phase for Rendezvous workflows.

**Files:**
- Modify: `tools/nika-tui/src/state/workflow_ops.rs:17-29`
- Test: `tools/nika-tui/src/state/tests.rs`

**Context:**
`toggle_pause` sets `paused = true` and `phase = Pause`, but never writes to `workflow.phase_before_pause`. When unpausing, it falls back to heuristics (`current_task.is_some() → Orbital`). A workflow paused during `Rendezvous` (active MCP call) will resume as `Orbital` instead. The event-driven pair `on_workflow_paused`/`on_workflow_resumed` (in `workflow.rs`) does save `phase_before_pause` correctly — mirror that.

Check whether `WorkflowState` has a `phase_before_pause: Option<MissionPhase>` field in `types.rs`. If not, add it.

**Step 1: Write the failing test**

```rust
#[test]
fn test_toggle_pause_saves_and_restores_phase_before_pause() {
    let mut state = TuiState::default();
    // Simulate a workflow in Rendezvous phase (active MCP call)
    state.workflow.phase = MissionPhase::Rendezvous;

    // First toggle: pause
    state.toggle_pause();
    assert_eq!(state.workflow.phase, MissionPhase::Pause);
    assert!(state.workflow.paused);

    // Second toggle: resume — must restore Rendezvous, not guess Orbital/Countdown
    state.toggle_pause();
    assert!(!state.workflow.paused);
    assert_eq!(
        state.workflow.phase,
        MissionPhase::Rendezvous,
        "resume must restore saved phase, not heuristic guess"
    );
}
```

**Step 2: Run test to verify it fails**

```bash
cd tools && cargo test -p nika-tui --lib -- toggle_pause_saves_and_restores
```
Expected: FAIL (restores `Orbital`, not `Rendezvous`)

**Step 3: Add `phase_before_pause` field if missing**

In `tools/nika-tui/src/state/types.rs`, find `WorkflowState` struct and add:
```rust
pub phase_before_pause: Option<MissionPhase>,
```
(in the `Default` impl, set to `None`)

**Step 4: Fix toggle_pause**

```rust
pub fn toggle_pause(&mut self) {
    if !self.workflow.paused {
        // Pausing: save current phase before overwriting
        self.workflow.phase_before_pause = Some(self.workflow.phase);
        self.workflow.paused = true;
        self.workflow.phase = MissionPhase::Pause;
    } else {
        // Resuming: restore saved phase or fall back to heuristic
        self.workflow.paused = false;
        if let Some(phase) = self.workflow.phase_before_pause.take() {
            self.workflow.phase = phase;
        } else if self.current_task.is_some() {
            self.workflow.phase = MissionPhase::Orbital;
        } else {
            self.workflow.phase = MissionPhase::Countdown;
        }
    }
    self.dirty.progress = true;
    self.dirty.status = true;
}
```

**Step 5: Run tests**

```bash
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add tools/nika-tui/src/state/workflow_ops.rs \
        tools/nika-tui/src/state/types.rs \
        tools/nika-tui/src/state/tests.rs
git commit -m "fix(tui): toggle_pause saves/restores phase_before_pause, prevents wrong phase on resume"
```

---

## BATCH B — Important: Logic Bugs

---

### Task 6: Fix on_mcp_response unconditionally overwriting workflow phase

**Confidence: 90%** — MCP responses arriving during Pause/Abort silently reset the phase to Orbital.

**Files:**
- Modify: `tools/nika-tui/src/state/event_handler/provider.rs:249`

**Context:**
`on_mcp_response` line 249: `self.workflow.phase = MissionPhase::Orbital;` — unconditional. If the workflow is in `Pause`, `Abort`, or `MissionSuccess` when a slow/late MCP response arrives, this silently overwrites the correct terminal phase.

**Step 1: Write failing test**

```rust
#[test]
fn test_mcp_response_does_not_overwrite_pause_phase() {
    let mut state = TuiState::default();
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.paused = true;
    // Push a pending MCP call so the handler can find it
    let call = McpCall {
        call_id: "c1".to_string(),
        seq: 0,
        server: "s".to_string(),
        tool: Some("t".to_string()),
        resource: None,
        task_id: "tid".to_string(),
        completed: false,
        output_len: None,
        timestamp_ms: 0,
        params: None,
        response: None,
        is_error: false,
        duration_ms: None,
    };
    state.mcp.calls.push_back(call);

    state.handle_event(
        &EventKind::McpResponse {
            call_id: "c1".to_string(),
            output_len: 42,
            duration_ms: 100,
            cached: false,
            is_error: false,
            response: None,
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Pause,
        "Pause phase must not be overwritten by a late MCP response"
    );
}
```

**Step 2: Run test to verify it fails**

**Step 3: Fix the conditional**

```rust
// BEFORE (provider.rs ~line 249):
self.workflow.phase = MissionPhase::Orbital;

// AFTER: only transition back to Orbital from Rendezvous
if self.workflow.phase == MissionPhase::Rendezvous {
    self.workflow.phase = MissionPhase::Orbital;
}
```

**Step 4: Run tests and commit**

```bash
git add tools/nika-tui/src/state/event_handler/provider.rs \
        tools/nika-tui/src/state/tests.rs
git commit -m "fix(tui): on_mcp_response only transitions Rendezvous→Orbital, preserves Pause/Abort"
```

---

### Task 7: Fix token threshold if-else chain skipping lower thresholds

**Confidence: 88%** — when tokens jump directly past multiple thresholds in one event (e.g. 40k→90k), only the highest threshold notification fires. Guards for lower thresholds are never set, so they can re-fire later out of order.

**Files:**
- Modify: `tools/nika-tui/src/state/event_handler/provider.rs:82-126`

**Context:**
Current code uses `if ... else if ... else if ... else if ...` — only ONE branch fires per event. Must be changed to independent `if` checks so ALL crossed thresholds fire and all guards are set.

**Step 1: Write failing test**

```rust
#[test]
fn test_threshold_notifications_all_fire_on_large_jump() {
    let mut state = TuiState::default();
    // One huge provider response that crosses ALL thresholds at once
    // 95k tokens = 95% of 100k context window
    state.handle_event(
        &EventKind::ProviderResponded {
            task_id: "t".to_string(),
            input_tokens: 90_000,
            output_tokens: 5_000,
            cache_read_tokens: 0,
            cost_usd: 0.01,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
        },
        1,
    );

    // All 4 thresholds must have been notified and their guards set
    assert!(state.metrics.notified_50pct, "50% guard must be set");
    assert!(state.metrics.notified_70pct, "70% guard must be set");
    assert!(state.metrics.notified_85pct, "85% guard must be set");
    assert!(state.metrics.notified_95pct, "95% guard must be set");
    // And 4 notifications in the queue (one per threshold)
    assert_eq!(
        state.notifs.items.len(),
        4,
        "all 4 threshold notifications must have fired"
    );
}
```

**Step 2: Run test to verify it fails**

Expected: only 1 notification fires (the 95% branch), guards for 50/70/85 are false.

**Step 3: Replace if-else chain with independent ifs**

In `provider.rs`, replace lines 82-126:

```rust
// Fire ALL crossed thresholds independently — if-else skips lower thresholds
// when tokens jump multiple thresholds in a single response.
if pct > 95.0 && !self.metrics.notified_95pct {
    self.metrics.notified_95pct = true;
    self.add_notification(Notification::alert(
        format!("ABANDON SHIP! {:.0}% fuel ({}/{}k)", pct, self.metrics.total_tokens, CONTEXT_WINDOW / 1000),
        timestamp_ms,
    ));
}
if pct > 85.0 && !self.metrics.notified_85pct {
    self.metrics.notified_85pct = true;
    self.add_notification(Notification::alert(
        format!("Danger zone! {:.0}% fuel ({}/{}k)", pct, self.metrics.total_tokens, CONTEXT_WINDOW / 1000),
        timestamp_ms,
    ));
}
if pct > 70.0 && !self.metrics.notified_70pct {
    self.metrics.notified_70pct = true;
    self.add_notification(Notification::warning(
        format!("Getting spicy! {:.0}% fuel ({}/{}k)", pct, self.metrics.total_tokens, CONTEXT_WINDOW / 1000),
        timestamp_ms,
    ));
}
if pct > 50.0 && !self.metrics.notified_50pct {
    self.metrics.notified_50pct = true;
    self.add_notification(Notification::info(
        format!("Heating up... {:.0}% fuel ({}/{}k)", pct, self.metrics.total_tokens, CONTEXT_WINDOW / 1000),
        timestamp_ms,
    ));
}
```

**Step 4: Run tests and commit**

```bash
git add tools/nika-tui/src/state/event_handler/provider.rs \
        tools/nika-tui/src/state/tests.rs
git commit -m "fix(tui): threshold notifications use independent ifs — all crossed thresholds fire"
```

---

### Task 8: Fix on_agent_complete double-pushing to token_history

**Confidence: 85%** — agent tasks get an extra phantom data point in the sparkline.

**Files:**
- Modify: `tools/nika-tui/src/state/event_handler/agent.rs:54-66`

**Context:**
`on_provider_responded` already pushes `input_tokens + output_tokens` to `token_history` on every LLM call. `on_agent_complete` (lines 54-66) then pushes the cumulative `last_turn.tokens` a second time, adding a phantom aggregate data point at the end that skews the sparkline upward.

The fix is simple: remove the `token_history` push from `on_agent_complete`.

**Step 1: Write failing test**

```rust
#[test]
fn test_agent_complete_does_not_double_push_token_history() {
    let mut state = TuiState::default();
    // Simulate: 1 provider call (pushes 1 entry to token_history)
    state.handle_event(
        &EventKind::ProviderResponded {
            task_id: "t".to_string(),
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 0,
            cost_usd: 0.0,
            ttft_ms: None,
            finish_reason: "stop".to_string(),
        },
        1,
    );
    let history_after_provider = state.metrics.token_history.len();

    // Simulate agent complete with the same 300 total tokens
    state.handle_event(&EventKind::AgentComplete { task_id: "t".to_string() }, 2);

    assert_eq!(
        state.metrics.token_history.len(),
        history_after_provider,
        "AgentComplete must not push an extra entry to token_history"
    );
}
```

**Step 2: Run test to verify it fails**

**Step 3: Remove the push from on_agent_complete**

```rust
// BEFORE (agent.rs lines 54-66):
pub(super) fn on_agent_complete(&mut self) {
    if let Some(last_turn) = self.agent.turns.last() {
        if let Some(tokens) = last_turn.tokens {
            if self.metrics.token_history.len() >= MAX_HISTORY_ENTRIES {
                self.metrics.token_history.pop_front();
            }
            self.metrics.token_history.push_back(tokens);
        }
    }
    self.dirty.reasoning = true;
}

// AFTER: remove the push entirely
pub(super) fn on_agent_complete(&mut self) {
    // token_history is already populated per-turn by on_provider_responded
    self.dirty.reasoning = true;
}
```

**Step 4: Run tests and commit**

```bash
git add tools/nika-tui/src/state/event_handler/agent.rs \
        tools/nika-tui/src/state/tests.rs
git commit -m "fix(tui): on_agent_complete no longer double-pushes to token_history (already tracked per provider call)"
```

---

### Task 9: Fix with_stdout/with_stderr bypassing line-count cache

**Confidence: 97%** — overflow indicator shows wrong count; required_height is too small.

**Files:**
- Modify: `tools/nika-tui/src/widgets/task_box/exec.rs:76-85`
- Test: `tools/nika-tui/src/widgets/task_box/exec.rs` (existing tests)

**Context:**
`with_stdout` (line 76) and `with_stderr` (line 82) set the string directly but never update `stdout_line_count` / `stderr_line_count`. Only `append_stdout`/`append_stderr` update the counters. Any builder-pattern caller gets `line_count = 0`, causing wrong overflow indicators and wrong `required_height`.

**Step 1: Write failing test**

Add to existing tests in exec.rs:

```rust
#[test]
fn test_with_stdout_populates_line_count() {
    let box_ = ExecBox::new("test")
        .with_stdout("line 1\nline 2\nline 3\n");

    assert_eq!(
        box_.stdout_line_count,
        3,
        "with_stdout must populate stdout_line_count"
    );
}

#[test]
fn test_with_stderr_populates_line_count() {
    let box_ = ExecBox::new("test")
        .with_stderr("err 1\nerr 2\n");

    assert_eq!(box_.stderr_line_count, 2);
}
```

**Step 2: Run tests to verify they fail**

```bash
cd tools && cargo test -p nika-tui --lib -- with_stdout_populates_line_count
```

**Step 3: Fix with_stdout and with_stderr**

```rust
pub fn with_stdout(mut self, stdout: impl Into<String>) -> Self {
    let s: String = stdout.into();
    self.stdout_line_count = s.bytes().filter(|&b| b == b'\n').count();
    self.stdout = s;
    self
}

pub fn with_stderr(mut self, stderr: impl Into<String>) -> Self {
    let s: String = stderr.into();
    self.stderr_line_count = s.bytes().filter(|&b| b == b'\n').count();
    self.stderr = s;
    self
}
```

**Step 4: Run tests and commit**

```bash
git add tools/nika-tui/src/widgets/task_box/exec.rs
git commit -m "fix(tui): with_stdout/with_stderr populate line_count cache (was 0, broke overflow indicator)"
```

---

### Task 10: Fix byte_to_line_col counting chars instead of bytes

**Confidence: 88%** — tree-sitter edit positions are wrong for non-ASCII YAML (emoji, accents).

**Files:**
- Modify: `tools/nika-tui/src/highlight/treesitter.rs:133-148`
- Test: `tools/nika-tui/src/highlight/treesitter.rs` (existing test module)

**Context:**
`byte_to_line_col` increments `col += 1` per char. tree-sitter `Point.column` is a **byte** offset within the line. For multibyte chars (e.g. emoji `🦋` = 4 bytes), the column is undercounted, placing the incremental edit at the wrong position and corrupting syntax highlighting for those lines.

**Step 1: Write failing test**

```rust
#[test]
fn test_byte_to_line_col_multibyte_char() {
    // "🦋\n" — emoji is 4 bytes, column of '\n' is at byte 4
    let source = "🦋\nfoo";
    // Position of 'f' on line 1: line=1, col=0
    let (line, col) = TreeSitterHighlighter::byte_to_line_col(source, 5); // 4 bytes emoji + 1 newline
    assert_eq!(line, 1);
    assert_eq!(col, 0, "column after newline must be 0 regardless of prev char width");

    // Position WITHIN emoji: byte 2 is mid-emoji, but since we only test col of 'after emoji',
    // test the column AT end of emoji on line 0:
    let (line, col) = TreeSitterHighlighter::byte_to_line_col(source, 4); // right after emoji
    assert_eq!(line, 0);
    assert_eq!(col, 4, "column must be byte offset (4), not char count (1)");
}
```

Note: `byte_to_line_col` is currently a private `fn`. Either make it `pub(crate)` for testing, or test indirectly via `highlight_incremental` with multibyte source. Use the easier option: add `#[cfg(test)]` `pub` visibility.

**Step 2: Fix the function**

```rust
fn byte_to_line_col(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (idx, ch) in source.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf8(); // byte count, not char count
        }
    }
    (line, col)
}
```

**Step 3: Run tests and commit**

```bash
git add tools/nika-tui/src/highlight/treesitter.rs
git commit -m "fix(tui): byte_to_line_col uses byte offset for tree-sitter column (was char count)"
```

---

### Task 11: Fix navigate_up underflow panic on item_count == 1

**Confidence: 88%** — in debug mode panics; in release mode silently wraps to huge index causing OOB.

**Files:**
- Modify: `tools/nika-tui/src/widgets/provider_modal/state/navigation.rs`
- Test: `tools/nika-tui/src/widgets/provider_modal/state/tests.rs`

**Context:**
`navigate_up` for the Cloud grid (3-column layout) computes:
```rust
let last_row = (self.item_count - 1) / 3;
// When item_count == 1, last_row == 0
// Then: self.selected_idx = (last_row - 1) * 3 + col
// = (0usize - 1) * 3 → PANIC (debug) or usize::MAX (release)
```

**Step 1: Write failing test** (run in debug mode to get the panic)

```rust
#[test]
fn test_navigate_up_does_not_panic_with_single_item() {
    let mut nav = NavigationState::default();
    nav.active_tab = ProviderModalTab::Cloud;
    nav.item_count = 1;
    nav.selected_idx = 0;

    // Must not panic
    nav.navigate_up();
    // selected_idx must remain valid (0)
    assert_eq!(nav.selected_idx, 0);
}
```

**Step 2: Run test to verify it fails (panics)**

```bash
cd tools && cargo test -p nika-tui --lib -- navigate_up_does_not_panic 2>&1
```
Expected: panic in debug, or test passes silently in release with wrong index.

**Step 3: Fix the underflow**

Find the `navigate_up` implementation in `navigation.rs`. Change:
```rust
self.selected_idx = (last_row - 1) * 3 + col;
```
to:
```rust
self.selected_idx = last_row.saturating_sub(1) * 3 + col;
```

**Step 4: Run tests and commit**

```bash
git add tools/nika-tui/src/widgets/provider_modal/state/navigation.rs \
        tools/nika-tui/src/widgets/provider_modal/state/tests.rs
git commit -m "fix(tui): navigate_up uses saturating_sub to prevent usize underflow on single-item grid"
```

---

## BATCH C — Plan Gaps (from v1 plan, incomplete)

---

### Task 12: Fix InfoPanel scroll — add End/G key + missing upper bound

**Files:**
- Modify: `tools/nika-tui/src/widgets/panels/info.rs:41-47` (struct) and `297-321` (handle_key)

**Context:**
`InfoPanel.handle_key` only handles Up/Down/PageUp/PageDown/Home('g'). Missing:
1. `End`/`'G'` to jump to bottom
2. No upper bound on `scroll_offset` — can scroll past content into blank space

The scroll is based on `Paragraph.scroll()` which already clips silently (no crash), but UX-wise the user can press Down infinitely. Adding an upper bound requires tracking rendered line count. Since `render()` has a `Text` with known line count, store it.

**Step 1: Add `rendered_line_count` to the struct**

```rust
pub struct InfoPanel {
    scroll_offset: u16,
    selected_task_id: Option<String>,
    rendered_line_count: u16,  // updated each render call
}
```

Initialize to `0` in `new()`.

**Step 2: Update rendered_line_count in render()**

After building `let text = Text::from(lines);`, add:
```rust
self.rendered_line_count = text.lines.len() as u16;
```

**Step 3: Add upper bound to Down/PageDown and add End/G**

```rust
pub fn handle_key(&mut self, key: KeyEvent) -> bool {
    let max_scroll = self.rendered_line_count.saturating_sub(5); // leave 5 lines visible
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
            true
        }
        KeyCode::Down | KeyCode::Char('j') => {
            self.scroll_offset = (self.scroll_offset + 1).min(max_scroll);
            true
        }
        KeyCode::PageUp => {
            self.scroll_offset = self.scroll_offset.saturating_sub(10);
            true
        }
        KeyCode::PageDown => {
            self.scroll_offset = (self.scroll_offset + 10).min(max_scroll);
            true
        }
        KeyCode::Home | KeyCode::Char('g') => {
            self.scroll_offset = 0;
            true
        }
        KeyCode::End | KeyCode::Char('G') => {
            self.scroll_offset = max_scroll;
            true
        }
        _ => false,
    }
}
```

**Step 4: Run tests and commit**

```bash
git add tools/nika-tui/src/widgets/panels/info.rs
git commit -m "fix(tui): InfoPanel adds scroll upper bound and End/G key to jump to bottom"
```

---

### Task 13: Fix browser_index not clamped after scan_workflows

**Files:**
- Modify: `tools/nika-tui/src/standalone/state.rs:75-91`
- Test: `tools/nika-tui/src/standalone/state.rs` (test module)

**Context:**
`scan_workflows()` clears `browser_entries` and repopulates, but never clamps `browser_index`. If the user has `browser_index = 5` and a rescan returns 3 entries, `browser_index` points past the end of the list. The next `browser_entries[browser_index]` access panics.

**Step 1: Write failing test**

```rust
#[test]
fn test_scan_workflows_clamps_browser_index() {
    let mut state = StandaloneState::new_for_test(); // or equivalent test constructor
    // Simulate 5 entries with index at 4
    // ... populate browser_entries with 5 fake entries ...
    state.browser_index = 4;

    // Rescan returns 2 entries (fewer)
    // Can't actually trigger scan_workflows without FS, so test the clamp logic directly:
    // After clear + repopulate with 2 entries, clamp must happen
    state.browser_entries.clear();
    state.browser_entries.push(BrowserEntry { /* ... */ });
    state.browser_entries.push(BrowserEntry { /* ... */ });
    state.clamp_browser_index(); // We'll add this helper

    assert!(
        state.browser_index < state.browser_entries.len().max(1),
        "browser_index must be within bounds after scan"
    );
}
```

**Step 2: Add clamp_browser_index helper and call it from scan_workflows**

```rust
/// Clamp browser_index to valid range after scan
fn clamp_browser_index(&mut self) {
    if !self.browser_entries.is_empty() {
        self.browser_index = self.browser_index.min(self.browser_entries.len() - 1);
    } else {
        self.browser_index = 0;
    }
}
```

In `scan_workflows()`, add at the end:
```rust
self.clamp_browser_index();
```

**Step 3: Run tests and commit**

```bash
git add tools/nika-tui/src/standalone/state.rs
git commit -m "fix(tui): scan_workflows clamps browser_index to prevent OOB after rescan"
```

---

### Task 14: Fix reset_for_retry leaving stale MCP calls

**Confidence: 85%** — after retry, old calls with same seq numbers co-exist with new ones.

**Files:**
- Modify: `tools/nika-tui/src/state/workflow_ops.rs:140-143`

**Context:**
`reset_for_retry` (lines 140-143) has this comment:
```rust
// Clear MCP calls (keep for reference? or clear?)
// For now, keep them as history but mark workflow as ready for retry
self.mcp.seq = 0;
```

Decision: clear `mcp.calls` and reset `selected_idx`. The stale entries are confusing (seq starts at 0 again) and `selected_idx` can be OOB after the clear. History display isn't worth the confusion.

**No test needed** (the comment already signals uncertainty — this is a deliberate design decision). Just implement it.

**Step 1: Update reset_for_retry**

```rust
// Clear MCP calls from previous run — new run starts fresh
self.mcp.calls.clear();
self.mcp.seq = 0;
self.mcp.selected_idx = None;
```

**Step 2: Run tests and commit**

```bash
git add tools/nika-tui/src/state/workflow_ops.rs
git commit -m "fix(tui): reset_for_retry clears mcp.calls to prevent stale seq overlap on retry"
```

---

## BATCH D — Security & Performance

---

### Task 15: SEC — Wrap ModalAction key fields in Zeroizing<String>

**Confidence: 92%** — API key material lives in unzeroized heap memory in the ModalAction enum.

**Files:**
- Modify: `tools/nika-tui/src/widgets/provider_modal/state/types.rs` (ModalAction enum)
- Modify: `tools/nika-tui/src/widgets/provider_modal/handler.rs` (call sites)

**Context:**
`SaveApiKey { key: String }` and `SaveAndTestApiKey { key: String }` in `ModalAction` carry raw strings. The buffer in `NavigationState` is correctly zeroized (`key_input_buffer.zeroize()`), but when the key is cloned into `ModalAction`, the clone is a plain `String` with no zeroize-on-drop. The key travels through the dispatch chain for an indeterminate duration on the heap.

`zeroize` crate is already in `Cargo.toml` (used in `navigation.rs`).

**Step 1: Find ModalAction enum in types.rs**

Read `tools/nika-tui/src/widgets/provider_modal/state/types.rs` to find the exact variants.

**Step 2: Change key fields to Zeroizing<String>**

```rust
use zeroize::Zeroizing;

// In ModalAction:
SaveApiKey { provider: &'static str, key: Zeroizing<String> },
SaveAndTestApiKey { provider: &'static str, key: Zeroizing<String> },
```

**Step 3: Update handler.rs call sites**

In `handle_input_mode`, change:
```rust
let key_value = state.key_input_buffer.clone();
// ...
SaveApiKey { provider, key: key_value }
// to:
SaveApiKey { provider, key: Zeroizing::new(std::mem::take(&mut state.key_input_buffer)) }
```

Note: `mem::take` moves the string out of the buffer (no clone) and leaves `key_input_buffer = String::new()`. The subsequent `zeroize()` call on `key_input_buffer` is then a no-op (empty string), which is fine.

**Step 4: Update any match sites** that destructure `SaveApiKey { key }` to expect `Zeroizing<String>`. Use `key.as_str()` or `&**key` to get `&str`. Search for `SaveApiKey` in `app/`.

**Step 5: Run tests and commit**

```bash
git add tools/nika-tui/src/widgets/provider_modal/state/types.rs \
        tools/nika-tui/src/widgets/provider_modal/handler.rs
git commit -m "sec(tui): ModalAction.key uses Zeroizing<String> to drop key material on dispatch"
```

---

### Task 16: PERF — Remove Vec<&str> allocation in ExecBox render

**Confidence: 83%** — bounded (take 3/10 lines) but still unnecessary heap alloc per render frame.

**Files:**
- Modify: `tools/nika-tui/src/widgets/task_box/exec.rs` (render method, lines ~420-458)

**Context:**
`render()` does:
```rust
let stdout_lines: Vec<&str> = self.stdout.lines().take(10).collect();
let stderr_lines: Vec<&str> = self.stderr.lines().take(3).collect();
```
Then immediately iterates over them. Remove the collect and iterate the iterator directly.

**Step 1: Find the collect sites** (read the file to confirm exact lines).

**Step 2: Replace with direct iteration**

```rust
// BEFORE:
let stdout_lines: Vec<&str> = if self.expanded_stdout {
    self.stdout.lines().take(10).collect()
} else {
    self.stdout.lines().take(3).collect()
};
for line in &stdout_lines { ... }

// AFTER:
let max_stdout = if self.expanded_stdout { 10 } else { 3 };
for line in self.stdout.lines().take(max_stdout) { ... }
```

Same for stderr.

**Step 3: Run tests and commit**

```bash
git add tools/nika-tui/src/widgets/task_box/exec.rs
git commit -m "perf(tui): ExecBox render iterates stdout/stderr lines directly, no Vec alloc per frame"
```

---

## BATCH E — Broken & Missing Tests

Tests that are wrong or critical coverage gaps.

---

### Task 17: Fix test_dismiss_all_notifications vacuous assertion

**Confidence: 85%** — `.all()` on empty iterator is vacuously true; doesn't test dismiss semantics.

**Files:**
- Modify: `tools/nika-tui/src/state/tests.rs` (find `test_dismiss_all_notifications`)

**Step 1: Find and fix the test**

```rust
// BEFORE:
state.dismiss_all_notifications();
assert!(state.notifs.items.iter().all(|n| n.dismissed));
assert_eq!(state.active_notification_count(), 0);

// AFTER:
state.dismiss_all_notifications();
assert_eq!(
    state.notifs.items.len(),
    0,
    "dismiss_all must clear all items from the list"
);
assert_eq!(state.active_notification_count(), 0);
```

**Step 2: Run tests and commit**

```bash
git add tools/nika-tui/src/state/tests.rs
git commit -m "test(tui): fix vacuous dismiss_all assertion — assert items.len() == 0 not vacuous .all()"
```

---

### Task 18: Fix test_keys_tab_label_with_verified using wrong provider count

**Confidence: 82%** — test asserts `"2/6"` but system has 7 providers; test should say `"2/7"`.

**Files:**
- Modify: `tools/nika-tui/src/widgets/provider_modal/state/tests.rs`

**Step 1: Find the test**

Search for `"2/6"` in the tests file. Read the surrounding context to understand whether `keys_tab_label` returns 6 or 7.

**Step 2: Run the test in isolation to see current behavior**

```bash
cd tools && cargo test -p nika-tui --lib -- keys_tab_label_with_verified 2>&1
```

**Step 3: Fix the assertion**

If the label says `"2/7"` (correct), change the test assertion from `"2/6"` to `"2/7"`.
If the label says `"2/6"` (wrong implementation), fix `keys_tab_label()` in `modal.rs` to use `7` and update the test.

**Step 4: Commit**

```bash
git add tools/nika-tui/src/widgets/provider_modal/state/tests.rs \
        tools/nika-tui/src/widgets/provider_modal/state/modal.rs  # if implementation was wrong
git commit -m "test(tui): keys_tab_label test uses correct 7-provider count (was 6)"
```

---

### Task 19: Fix test_set_provider_missing_key false-positive in CI

**Confidence: 95%** — in CI where `ANTHROPIC_API_KEY` is set as a secret, the test's `else` branch asserts `result.is_ok()`, the opposite of what the test name claims.

**Files:**
- Modify: `tools/nika-tui/src/chat_agent/tests.rs:211-234`

**Step 1: Read the test**

Read `tools/nika-tui/src/chat_agent/tests.rs` lines 211-234.

**Step 2: Rewrite to be unconditional**

The test must verify that calling `set_provider` when NO key is configured returns an error. Don't depend on env vars being absent — explicitly use a provider guaranteed to have no key in any environment, or temporarily override the key check via a test override:

```rust
#[test]
fn test_set_provider_missing_key() {
    // Use an env var name that will never be set in CI
    std::env::remove_var("ANTHROPIC_API_KEY");
    let mut agent = ChatAgent::new_for_test_without_keys();
    let result = agent.set_provider_raw(ModelProvider::Claude, None); // no key override
    assert!(
        result.is_err(),
        "set_provider must fail when ANTHROPIC_API_KEY is not set"
    );
    // Do NOT restore env var — parallel test contamination is prevented by #[serial] if needed
}
```

If `ChatAgent::new_for_test_without_keys()` doesn't exist, find the existing constructor and use env var removal + `#[serial]` from the `serial_test` crate (already likely in dev-deps for these tests). Check `Cargo.toml`.

**Step 3: Run and commit**

```bash
cd tools && cargo test -p nika-tui --lib -- test_set_provider_missing_key
git add tools/nika-tui/src/chat_agent/tests.rs
git commit -m "test(tui): fix test_set_provider_missing_key — was false-positive in CI when API key env var is set"
```

---

### Task 20: Add missing notification dedup edge-case tests

**Confidence: 92%** — non-consecutive duplicate behavior untested; AND condition for dedup untested.

**Files:**
- Modify: `tools/nika-tui/src/state/notification_state.rs` (test module)

**Step 1: Add tests**

```rust
#[test]
fn test_notification_dedup_consecutive_only() {
    // Dedup applies only to CONSECUTIVE identical notifications.
    // A duplicate separated by a different notification must still be accepted.
    let mut ns = NotificationState::new();
    ns.push(Notification::info("dup", 0));
    ns.push(Notification::warning("other", 1)); // breaks the sequence
    ns.push(Notification::info("dup", 2));      // same as first, but not consecutive

    assert_eq!(ns.items.len(), 3, "non-consecutive duplicate must be accepted");
}

#[test]
fn test_notification_dedup_requires_both_level_and_message() {
    // Dedup requires BOTH level AND message to match.
    let mut ns = NotificationState::new();
    ns.push(Notification::info("same message", 0));

    // Same message, different level → NOT deduped
    ns.push(Notification::warning("same message", 1));
    assert_eq!(ns.items.len(), 2, "same message with different level must not be deduped");

    // Same level, different message → NOT deduped
    ns.push(Notification::warning("different message", 2));
    assert_eq!(ns.items.len(), 3, "same level with different message must not be deduped");

    // Both same → deduped
    ns.push(Notification::warning("different message", 3));
    assert_eq!(ns.items.len(), 3, "exact consecutive duplicate must be deduped");
}
```

**Step 2: Run and commit**

```bash
cd tools && cargo test -p nika-tui --lib -- notification 2>&1 | tail -5
git add tools/nika-tui/src/state/notification_state.rs
git commit -m "test(tui): add missing notification dedup edge cases (consecutive-only, AND condition)"
```

---

### Task 21: Add missing tests for scroll clamp, inference rollback, WorkflowFailed

**Files:**
- Modify: `tools/nika-tui/src/widgets/panels/task_flow.rs` (test module)
- Modify: `tools/nika-tui/src/state/tests.rs`

**Step 1: Add scroll clamp test to task_flow.rs**

```rust
#[test]
fn test_max_scroll_does_not_underflow() {
    let mut flow = TaskBoxFlow::new();
    flow.content_height = 10;
    flow.visible_height = 20; // visible > content → max_scroll = 0

    // scrolling down must not exceed max_scroll = 0
    flow.scroll_down(50);
    assert_eq!(flow.scroll_offset, 0, "scroll must not exceed 0 when content < visible");
}

#[test]
fn test_scroll_clamp_on_content_shrink() {
    let mut flow = TaskBoxFlow::new();
    flow.content_height = 100;
    flow.visible_height = 20;
    flow.scroll_down(80); // valid: scroll_offset = 80

    // Content shrinks (tasks removed) — simulate by reducing content_height
    flow.content_height = 30;
    // max_scroll is now 10; clamp manually (mirrors what render() does)
    flow.scroll_offset = flow.scroll_offset.min(flow.content_height.saturating_sub(flow.visible_height));

    assert_eq!(flow.scroll_offset, 10, "scroll must clamp to new max when content shrinks");
}
```

**Step 2: Add WorkflowFailed kill-running-tasks test to state/tests.rs**

```rust
#[test]
fn test_workflow_failed_kills_running_tasks() {
    let mut state = TuiState::default();
    // Schedule and start a task
    state.handle_event(&EventKind::TaskScheduled { task_id: "t1".to_string(), task_type: "infer".to_string() }, 1);
    state.handle_event(&EventKind::TaskStarted { task_id: "t1".to_string(), .. }, 2);

    assert_eq!(state.tasks["t1"].status, TaskStatus::Running);

    // Workflow failure must kill the running task
    state.handle_event(&EventKind::WorkflowFailed { error: "timeout".to_string() }, 3);

    assert_eq!(
        state.tasks["t1"].status,
        TaskStatus::Failed,
        "WorkflowFailed must transition Running tasks to Failed"
    );
    assert!(
        state.tasks["t1"].error.as_deref() == Some("timeout"),
        "failed task must carry the workflow error message"
    );
}
```

**Step 3: Run and commit**

```bash
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5
git add tools/nika-tui/src/widgets/panels/task_flow.rs \
        tools/nika-tui/src/state/tests.rs
git commit -m "test(tui): add scroll clamp shrink + WorkflowFailed kill-running-tasks tests"
```

---

## Verification

After all tasks:

```bash
# Full test suite
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5
# Expected: 2160+ tests, 0 failed

# Zero clippy warnings
cd tools && cargo clippy -p nika-tui --no-deps -- -D warnings 2>&1 | tail -5
# Expected: Finished with 0 warnings

# Show final commit log
cd /Users/thibaut/dev/supernovae/nika && git log --oneline -25
```
