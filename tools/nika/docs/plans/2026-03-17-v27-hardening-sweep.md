# v0.27.0 Hardening Sweep — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 4 real bugs/gaps found during the deep audit of Nika v0.27.0, completing the safety hardening initiated by the for_each+TaskTable plan.

**Architecture:** All changes are defense-in-depth hardening. Two fix broken tests from recent nuclear cleanup, one hardens a reverse-lowering path (`unlower`), and one adds a missing analyzer warning. The structured output Layer 3/4 is confirmed fully wired — no fix needed, just documentation.

**Tech Stack:** Rust, TDD (red-green-refactor), `cargo test`, `cargo clippy`

---

## Summary of Findings

| # | Area | Severity | Fix |
|---|------|----------|-----|
| 1 | `chat_ux_test.rs` stale `ActivityStack` import | **BROKEN** — E2E tests fail to compile | Remove dead import + dead tests |
| 2 | `unlower()` filter_map silent skip | **MEDIUM** — invariant violation hidden | Replace with `.ok_or_else` (same pattern as `lower()`) |
| 3 | `retry:` on non-fetch verb silently ignored | **LOW** — user confusion | Add analyzer warning |
| 4 | Structured output Layer 3/4 callback | **NONE** — already wired | Verify test + close audit item |

---

### Task 1: Fix chat_ux_test.rs — Remove Dead ActivityStack Widget

The `ActivityStack` widget struct was deleted in commit `8158df05` (nuclear delete dead widgets). But `tests/chat_ux_test.rs` still imports and uses it. The module `activity_stack.rs` still exists but only exports `ActivityItem` and `ActivityTemp` — NOT `ActivityStack`.

**Files:**
- Modify: `tests/chat_ux_test.rs:13` (remove `ActivityStack` from import)
- Modify: `tests/chat_ux_test.rs:177-205` (delete `test_activity_stack_rendering` + `test_activity_stack_empty`)
- Modify: `tests/chat_ux_test.rs:493` (delete `ActivityStack::new(&activities).render(...)` usage in `test_chat_ux_full_layout`)

**Step 1: Remove `ActivityStack` from the import line**

Line 13 currently:
```rust
use nika::tui::widgets::{
    default_commands, ActivityItem, ActivityStack, ActivityTemp, CommandPaletteState, InferStatus,
    InferStreamBox, InferStreamData, McpCallBox, McpCallData, McpCallStatus, McpServerInfo,
    McpStatus, PaletteCommand, SessionContext, SessionContextBar,
};
```

Remove `ActivityStack, ` from the import.

**Step 2: Delete `test_activity_stack_rendering` test (lines 176-192)**

Delete:
```rust
#[test]
fn test_activity_stack_rendering() {
    // ... uses ActivityStack::new(&items).frame(0)
}
```

**Step 3: Delete `test_activity_stack_empty` test (lines 194-205)**

Delete:
```rust
#[test]
fn test_activity_stack_empty() {
    // ... uses ActivityStack::new(&items)
}
```

**Step 4: Remove `ActivityStack` usage from `test_chat_ux_full_layout` (line 493)**

In `test_chat_ux_full_layout`, remove lines that use `ActivityStack::new(&activities)`:
```rust
    // Activity stack in sidebar
    let stack_area = Rect::new(0, 3, 40, 15);
    ActivityStack::new(&activities).render(stack_area, &mut buffer);
```

Also remove the `activities` variable if no longer used.

**Step 5: Run tests to verify compilation**

```bash
CARGO_TARGET_DIR=target-main cargo test --test chat_ux_test 2>&1
```

Expected: All remaining tests PASS. No compilation error.

**Step 6: Run full E2E test suite**

```bash
CARGO_TARGET_DIR=target-main cargo test --tests 2>&1 | tail -5
```

Expected: `test result: ok.`

**Step 7: Commit**

```bash
git add tests/chat_ux_test.rs
git commit -m "fix(test): remove dead ActivityStack import from chat_ux_test

ActivityStack widget was deleted in 8158df05 (nuclear cleanup) but
the test file still imported and used it, breaking E2E compilation.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2: Harden unlower() — Reject Dangling Dependency Names

`unlower()` at `src/ast/lower.rs:438-444` uses `filter_map` to silently skip dependency names that don't resolve to TaskIds. This is the same pattern we already fixed in `task_dep_names()` (commit `f6329bd5`). Apply the same hardening.

**Files:**
- Modify: `src/ast/lower.rs:438-446` (replace `filter_map` with explicit loop + error)
- Modify: `src/ast/lower.rs` (change `unlower()` return type to `Result<AnalyzedWorkflow, NikaError>`)
- Modify: all callers of `unlower()` (propagate `Result`)
- Test: `src/ast/lower.rs` (add new unit tests in existing test module)

**Step 1: Find all callers of `unlower()`**

```bash
grep -rn "unlower(" src/ --include="*.rs" | grep -v "fn unlower\|unlower_"
```

**Step 2: Write the failing test**

Add to the test module in `lower.rs`:
```rust
#[test]
fn unlower_rejects_dangling_dep_name() {
    let mut wf = dummy_workflow();
    let id = wf.task_table.insert("producer");
    let mut task = dummy_task(id, "producer");
    task.action = AnalyzedTaskAction::Exec(AnalyzedExecAction {
        command: "echo test".into(),
        ..Default::default()
    });
    wf.tasks.push(task);

    // Lower to Workflow, then tamper with flow to create dangling ref
    let mut lowered = lower(wf).unwrap();
    let task = Arc::make_mut(&mut lowered.tasks[0]);
    task.flow = Some(vec!["nonexistent_task".to_string()]);

    // unlower should reject the dangling name
    let result = unlower(lowered);
    assert!(result.is_err(), "unlower should reject dangling dep name");
}
```

**Step 3: Run test — verify RED**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib lower::tests::unlower_rejects_dangling_dep_name 2>&1
```

Expected: FAIL (currently `unlower()` returns `AnalyzedWorkflow`, not `Result`)

**Step 4: Change `unlower()` signature**

Change from:
```rust
pub fn unlower(workflow: Workflow) -> AnalyzedWorkflow {
```
To:
```rust
pub fn unlower(workflow: Workflow) -> Result<AnalyzedWorkflow, NikaError> {
```

Replace the `filter_map` block (lines 438-446):
```rust
let depends_on: Vec<TaskId> = task
    .flow
    .as_ref()
    .map(|deps| {
        deps.iter()
            .filter_map(|name| task_table.get_id(name))
            .collect()
    })
    .unwrap_or_default();
```

With:
```rust
let depends_on: Vec<TaskId> = match task.flow.as_ref() {
    Some(deps) => {
        let mut ids = Vec::with_capacity(deps.len());
        for name in deps {
            let id = task_table.get_id(name).ok_or_else(|| NikaError::ValidationError {
                reason: format!(
                    "Unlowering: dependency '{}' not found in TaskTable (invariant violation)",
                    name
                ),
            })?;
            ids.push(id);
        }
        ids
    }
    None => vec![],
};
```

Wrap the return value in `Ok(...)`.

**Step 5: Update callers**

Update the re-export in `src/ast/mod.rs`:
```rust
pub use lower::{lower, unlower};
```
No change needed — callers must handle `Result`.

Find and update all call sites (likely `include_loader.rs` and tests).

**Step 6: Run test — verify GREEN**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib lower::tests::unlower_rejects_dangling_dep_name 2>&1
```

Expected: PASS

**Step 7: Run full suite**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib 2>&1 | tail -5
```

Expected: All pass.

**Step 8: Clippy**

```bash
CARGO_TARGET_DIR=target-main cargo clippy -- -D warnings 2>&1
```

Expected: Clean.

**Step 9: Commit**

```bash
git add src/ast/lower.rs src/ast/mod.rs <any other changed files>
git commit -m "fix(ast): harden unlower() to reject dangling dependency names

Same pattern as task_dep_names() fix (f6329bd5): replace filter_map
with explicit loop + ok_or_else to surface invariant violations
instead of silently dropping unknown dependencies.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 3: Add Analyzer Warning — retry on Non-Fetch Verb

The analyzer (Phase 2) checks `version.supports_retry()` but never checks if the verb is `fetch`. A `retry:` on `infer:`, `exec:`, `invoke:`, or `agent:` is silently dropped during lowering (only `lower_fetch()` receives retry config). Users get no feedback.

**Files:**
- Modify: `src/ast/analyzer/analyze.rs:360-370` (add verb-type check after version check)
- Test: `src/ast/analyzer/analyze.rs` or `tests/` (new test)

**Step 1: Write the failing test**

Add a test that parses YAML with `retry:` on an `infer:` task and expects a warning:
```rust
#[test]
fn test_retry_on_infer_emits_warning() {
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: my_task
    infer: "Generate something"
    retry:
      max_attempts: 3
      delay_ms: 1000
"#;
    let raw = raw::parse(yaml, FileId(0)).unwrap();
    let result = analyzer::analyze(raw);
    // Should succeed (retry on infer is not an error)
    assert!(result.is_ok(), "retry on infer should not be an error");
    // But should emit a warning
    assert!(
        !result.warnings.is_empty(),
        "retry on non-fetch verb should emit a warning"
    );
    assert!(
        result.warnings[0].message.contains("retry"),
        "Warning should mention retry"
    );
}
```

**Step 2: Run test — verify RED**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib <test_path> 2>&1
```

Expected: FAIL (warnings is empty — no warning emitted yet)

**Step 3: Implement the warning**

In `analyze.rs`, after the existing retry version check (line 370), add:

```rust
// Check retry verb compatibility
if let Some(ref retry) = task.retry {
    // retry: is only effective on fetch: tasks (lowering drops it for other verbs)
    if let Some(ref action) = task.action {
        let is_fetch = matches!(action, RawTaskAction::Fetch(_));
        if !is_fetch {
            let verb_name = match action {
                RawTaskAction::Infer(_) => "infer",
                RawTaskAction::Exec(_) => "exec",
                RawTaskAction::Invoke(_) => "invoke",
                RawTaskAction::Agent(_) => "agent",
                RawTaskAction::Fetch(_) => unreachable!(),
            };
            ctx.add_warning(
                AnalyzeError::new(
                    AnalyzeErrorKind::InvalidValue,
                    retry.span,
                    format!(
                        "'retry' has no effect on '{}' tasks (only 'fetch' supports retry)",
                        verb_name
                    ),
                )
                .with_suggestion("move retry to a fetch task, or remove it".to_string()),
            );
        }
    }
}
```

**Step 4: Run test — verify GREEN**

Expected: PASS

**Step 5: Run full suite**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib 2>&1 | tail -5
```

Expected: All pass (existing tests should not break since this is a WARNING, not error).

**Step 6: Clippy**

```bash
CARGO_TARGET_DIR=target-main cargo clippy -- -D warnings 2>&1
```

**Step 7: Commit**

```bash
git add src/ast/analyzer/analyze.rs <test file>
git commit -m "feat(ast): warn when retry: is used on non-fetch verb

retry: config is only wired to fetch: tasks during lowering.
Using it on infer:/exec:/invoke:/agent: is silently dropped.
Now emits analyzer warning NIKA-144 to inform the user.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 4: Verify Structured Output Layer 3/4 Wiring (Close Audit)

**Finding:** The `InferCallback` IS correctly wired in `executor/verbs.rs:411-430`. Layer 3 (retry with feedback) and Layer 4 (LLM repair) are fully implemented and connected. The `with_infer_callback()` call is present for the `infer:` verb. For the `agent:` verb, Layer 0 (DynamicSubmitTool injection) handles structured output instead.

**Action:** Verify via existing tests. No code change needed.

**Step 1: Run structured output tests**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib structured_output 2>&1
```

Expected: All pass.

**Step 2: Run runner structured tests**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib runner::tests 2>&1 | grep -i structured
```

Expected: All structured-related tests pass.

**Step 3: Close audit item — no fix needed**

Add a comment to the existing test that documents the wiring:
```rust
// AUDIT(2026-03-17): Verified InferCallback is wired in verbs.rs:411-430
// Layers 3 & 4 are fully functional for infer: verb.
// Agent verb uses Layer 0 (DynamicSubmitTool) instead.
```

---

## Checkpoint Schedule

| After Task | Verification |
|-----------|-------------|
| Task 1 | `cargo test --tests` compiles + all pass |
| Task 2 | `cargo test --lib` all pass + clippy clean |
| Task 3 | `cargo test --lib` all pass + clippy clean |
| Task 4 | `cargo test --lib structured_output` all pass |
| Final | `cargo test` full suite + `cargo clippy -- -D warnings` + push |

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| `unlower()` signature change breaks callers | grep all callers first, update atomically |
| Warning on retry breaks existing workflows | WARNING not ERROR — analysis succeeds, no behavior change |
| ActivityStack removal affects other tests | Only `chat_ux_test.rs` uses it (confirmed via grep) |
