# TUI Cleanup v3 — Post-Audit Polish & Test Infrastructure

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the 7 residual issues from the v2 audit session + build test infrastructure + eliminate magic numbers + clean dead code. All surgical, 1 fix = 1 commit.

**Prerequisite:** v2 plan fully completed (21/21 tasks, 2145 tests pass). Read `docs/plans/2026-03-27-tui-deep-fixes-v2.md` for context.

**Test command:** `cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5`
**Clippy:** `cd tools && cargo clippy -p nika-tui --no-deps -- -D warnings 2>&1 | tail -3`

---

## BATCH A — CRITICAL BUG (1 task)

### Task 1: Fix on_mcp_invoke unconditionally clobbering phase

**Confidence: 100%** — Symmetric to Task 6 from v2 (on_mcp_response). If workflow is Paused and a late MCP invoke event arrives, phase is overwritten to Rendezvous.

**Files:**
- Modify: `tools/nika-tui/src/state/event_handler/provider.rs:164`
- Test: `tools/nika-tui/src/state/tests.rs`

**Context:**
Line 164: `self.workflow.phase = MissionPhase::Rendezvous;` — unconditional. Must guard against Pause/Abort/MissionSuccess, exactly like the on_mcp_response fix.

**Step 1: Write failing test**

```rust
#[test]
fn test_mcp_invoke_does_not_overwrite_pause_phase() {
    let mut state = TuiState::new("test.nika.yaml");
    state.workflow.phase = MissionPhase::Pause;
    state.workflow.paused = true;

    state.handle_event(
        &EventKind::McpInvoke {
            task_id: "tid".into(),
            mcp_server: "novanet".to_string(),
            tool: Some("tool".to_string()),
            resource: None,
            call_id: "c1".to_string(),
            params: None,
        },
        1,
    );

    assert_eq!(
        state.workflow.phase,
        MissionPhase::Pause,
        "Pause phase must not be overwritten by MCP invoke"
    );
}
```

**Step 2: Run test — expect FAIL**

**Step 3: Fix**

```rust
// BEFORE (provider.rs:164):
self.workflow.phase = MissionPhase::Rendezvous;

// AFTER: guard against terminal/pause phases
if matches!(
    self.workflow.phase,
    MissionPhase::Countdown | MissionPhase::Launch | MissionPhase::Orbital
) {
    self.workflow.phase = MissionPhase::Rendezvous;
}
```

**Step 4: Run tests, commit**

```
fix(tui): on_mcp_invoke guards phase transition — preserves Pause/Abort
```

---

## BATCH B — MAGIC NUMBERS (3 tasks)

### Task 2: Replace hardcoded 7 with CLOUD_PROVIDER_COUNT

**Files:**
- Modify: `tools/nika-tui/src/widgets/provider_modal/state/modal.rs` (lines 180, 247)

**Context:**
`CLOUD_PROVIDER_COUNT` is already defined in `providers.rs:14` but not used in `modal.rs`. Two sites:
- Line 180: `for i in 0..7` → `for i in 0..CLOUD_PROVIDER_COUNT`
- Line 247: `.max(7)` → `.max(CLOUD_PROVIDER_COUNT)`

**Step 1: Add import** at top of `modal.rs`:
```rust
use super::providers::CLOUD_PROVIDER_COUNT;
```

Note: `CLOUD_PROVIDER_COUNT` is `const` not `pub const`. May need to make it `pub(super) const` or `pub(crate) const`.

**Step 2: Replace both sites, run tests, commit**

```
refactor(tui): use CLOUD_PROVIDER_COUNT constant instead of magic 7
```

### Task 3: Extract CONTEXT_WINDOW to shared constant

**Files:**
- Modify: `tools/nika-tui/src/state/event_handler/provider.rs:79`
- Add to: `tools/nika-tui/src/state/event_handler/mod.rs` or `types.rs`

**Context:**
`const CONTEXT_WINDOW: u64 = 100_000;` is defined locally in `on_provider_responded`. Move to module-level constant so it's visible and reusable.

```rust
// In event_handler/mod.rs or types.rs:
/// Default context window size for token threshold notifications
pub(crate) const CONTEXT_WINDOW_TOKENS: u64 = 100_000;
```

**Commit:** `refactor(tui): extract CONTEXT_WINDOW_TOKENS to module-level constant`

### Task 4: Extract notification max_items constant

**Files:**
- Modify: `tools/nika-tui/src/state/notification_state.rs:26`

**Context:**
`max_items: 10` hardcoded in Default impl. Extract:

```rust
impl NotificationState {
    /// Maximum notifications to retain
    const DEFAULT_MAX_ITEMS: usize = 10;
}

impl Default for NotificationState {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            max_items: Self::DEFAULT_MAX_ITEMS,
        }
    }
}
```

**Commit:** `refactor(tui): extract NotificationState::DEFAULT_MAX_ITEMS constant`

---

## BATCH C — TEST INFRASTRUCTURE (2 tasks)

### Task 5: Create test_helpers.rs module

**Files:**
- Create: `tools/nika-tui/src/test_helpers.rs`
- Modify: `tools/nika-tui/src/lib.rs` (add `#[cfg(test)] mod test_helpers;`)

**Context:**
Across 6 test files, there are:
- 134x `TuiState::new("test...")`
- 89x `Arc::from("...")` for task IDs
- 20x `EventKind::TaskScheduled { ... }` boilerplate
- 21x `EventKind::TaskStarted { ... }` boilerplate

Create a minimal test_helpers module with:

```rust
//! Shared test utilities for nika-tui
//!
//! Reduces boilerplate across 6 test files (2145+ tests).

use std::sync::Arc;

use nika_engine::event::EventKind;

/// Standard test state constructor
pub fn test_state() -> crate::state::TuiState {
    crate::state::TuiState::new("test.nika.yaml")
}

/// Arc<str> shorthand for task IDs
pub fn tid(id: &str) -> Arc<str> {
    Arc::from(id)
}

/// Build TaskScheduled event
pub fn task_scheduled(id: &str) -> EventKind {
    EventKind::TaskScheduled {
        task_id: tid(id),
        dependencies: vec![],
    }
}

/// Build TaskStarted event
pub fn task_started(id: &str, verb: &str) -> EventKind {
    EventKind::TaskStarted {
        task_id: tid(id),
        verb: verb.into(),
        inputs: serde_json::json!({}),
    }
}

/// Build WorkflowStarted event
pub fn workflow_started(task_count: usize) -> EventKind {
    EventKind::WorkflowStarted {
        task_count,
        generation_id: "gen-test".to_string(),
        workflow_hash: "hash-test".to_string(),
        nika_version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Build McpInvoke event
pub fn mcp_invoke(task_id: &str, call_id: &str, tool: &str) -> EventKind {
    EventKind::McpInvoke {
        task_id: tid(task_id),
        call_id: call_id.to_string(),
        mcp_server: "test-server".to_string(),
        tool: Some(tool.to_string()),
        resource: None,
        params: None,
    }
}

/// Build ProviderResponded event
pub fn provider_responded(task_id: &str, input: u64, output: u64) -> EventKind {
    EventKind::ProviderResponded {
        task_id: tid(task_id),
        request_id: None,
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: 0,
        cost_usd: 0.0,
        ttft_ms: None,
        finish_reason: "stop".to_string(),
    }
}
```

**Do NOT refactor existing tests yet** — just create the module so future tests can use it. Refactoring 4500 lines of tests is a separate task.

**Commit:** `test(tui): add test_helpers.rs — shared fixtures for TuiState, EventKind builders`

### Task 6: Add temp_env to dev-dependencies

**Files:**
- Modify: `tools/nika-tui/Cargo.toml` (dev-dependencies)
- Modify: `tools/nika-tui/src/chat_agent/tests.rs` (test_set_provider_missing_key)

**Context:**
`serial_test` + manual `env::remove_var` / `env::set_var` is fragile. `temp_env` crate provides scoped env var overrides that auto-restore on drop.

```toml
[dev-dependencies]
temp-env = "0.3"
```

```rust
#[test]
#[serial]
fn test_set_provider_missing_key() {
    temp_env::with_var_unset("ANTHROPIC_API_KEY", || {
        std::env::set_var("OPENAI_API_KEY", "test-key-for-unit-test");
        let mut agent = ChatAgent::new().expect("Should create agent");
        let result = agent.set_provider(ModelProvider::Claude);
        assert!(result.is_err(), "must fail without ANTHROPIC_API_KEY");
    });
}
```

**Commit:** `test(tui): use temp_env for safe env var scoping in chat_agent tests`

---

## BATCH D — DEAD CODE CLEANUP (2 tasks)

### Task 7: Remove EdgeStyle::Smooth dead variant

**Files:**
- Modify: `tools/nika-tui/src/widgets/dag/edge.rs:75`

**Context:**
`EdgeStyle::Smooth` variant is `#[allow(dead_code)]` and marked "reserved for future use". Zero users = zero compat. Delete it.

**Commit:** `refactor(tui): remove dead EdgeStyle::Smooth variant`

### Task 8: Remove overly broad #[allow(dead_code)] on chat widget modules

**Files:**
- Modify: `tools/nika-tui/src/views/chat/widgets/mod.rs:7-13`

**Context:**
4 modules (`dag_panel`, `edge_line`, `node_box`, `task_queue`) have `#[allow(dead_code)]` at module level. The modules ARE used. Remove the suppression; if individual methods are unused, suppress at method level or delete them.

**Step 1:** Remove `#[allow(dead_code)]` from the 4 module declarations
**Step 2:** Compile. If specific methods are unused, either delete them or add `#[allow(dead_code)]` on the specific method with a comment.
**Step 3:** Run tests, commit.

```
refactor(tui): narrow dead_code suppression on chat widget modules
```

---

## BATCH E — GIT HYGIENE (1 task, manual)

### Task 9: Document hook interference for future sessions

**No code change.** Save to memory:

The pre-commit hooks auto-committed unrelated changes during the v2 session:
- `38badc8ba` bundled daemon cron scheduler
- `1ce62841d` bundled cost.rs reformatting with Zeroizing fix

**Mitigation for future bulk-fix sessions:**
1. Use `git worktree` (isolated copy) or a dedicated branch
2. Disable auto-commit hooks temporarily: `git commit --no-verify` (if user approves)
3. Stash ONLY with `git stash push -p` (partial) to avoid conflicts on pop

---

## Verification

After all tasks:

```bash
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5
# Expected: 2148+ tests, 0 failed

cd tools && cargo clippy -p nika-tui --no-deps -- -D warnings 2>&1 | tail -3
# Expected: 0 warnings

cd /Users/thibaut/dev/supernovae/nika && git log --oneline -12
```
