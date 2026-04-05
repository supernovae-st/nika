# TUI Architecture Cleanup — Nuclear Refactor

> **Copy-paste into a fresh Claude Code chat. ultrathink, full autonomy.**

---

## Context

Nika TUI is a ratatui terminal app at `/Users/thibaut/dev/supernovae/nika/tools/nika-tui/src/`.
3-view architecture (Studio/Command/Control), ~87K LOC, 2103 tests passing.

The TUI is **functionally correct** (zero crash bugs, all tests green) but has **architectural
debt**: wizard duplication, god-module widgets/mod.rs with 9 `#[allow(dead_code)]` modules,
oversized files, and a DAG subsystem that should be its own module.

This session is a **clean architecture refactor** — no new features, just better Rust.

---

## METRICS (current state)

```
Total: 87,375 LOC across ~130 files
Tests: 2,103 passing (4,207 LOC state/tests.rs + 2,542 LOC chat/tests.rs)
Largest non-test files:
  1,544  new_wizard/mod.rs       (duplicate of wizard/)
  1,501  views/studio/mod.rs     (god module)
  1,400  widgets/matrix_decrypt.rs
  1,151  widgets/task_box/invoke.rs
  1,105  views/wizard.rs         (3rd wizard file!)
  1,076  widgets/dag_node_box.rs
  1,046  widgets/tree/node.rs
  1,040  widgets/task_box/agent.rs
  1,038  state/event_handler.rs  (giant match)

Dead code modules (9 with #[allow(dead_code)]):
  activity_stack, agent_steps, dag_edge, dag_layout, dag_node_box,
  infer_stream_box, matrix_decrypt, provider_selector, verb_input

Wizard triplication: wizard/ (975) + new_wizard/ (1544) + views/wizard.rs (1105) = 3,624 LOC
DAG subsystem: dag_ascii + dag_edge + dag_layout + dag_node_box = 3,532 LOC (no clear API boundary)
```

---

## PHASE 1 — Wizard Consolidation (kill 2 of 3)

### Problem
Three wizard implementations exist:
- `wizard/mod.rs` (975 LOC) — original
- `new_wizard/mod.rs` (1544 LOC) — rewrite, currently used by `nika init`
- `views/wizard.rs` (1105 LOC) — TUI view wrapper

### Task
1. Determine which wizard is actually called (trace from `lib.rs` entry points + CLI)
2. Delete the unused one(s)
3. If `new_wizard` is the active one, rename it to `wizard/` and delete the old
4. Consolidate `views/wizard.rs` into the surviving wizard module or make it a thin wrapper

**Expected: -1,500 to -2,500 LOC**

---

## PHASE 2 — DAG Subsystem Extraction

### Problem
4 tightly coupled files at widget root level form an internal subsystem:
- `dag_ascii.rs` (831 LOC) — main renderer
- `dag_edge.rs` (803 LOC) — edge routing (marked `#[allow(dead_code)]`)
- `dag_layout.rs` (822 LOC) — Sugiyama layout (marked `#[allow(dead_code)]`)
- `dag_node_box.rs` (1076 LOC) — node box widget

### Task
1. Create `widgets/dag/` module directory
2. Move all 4 files into it: `dag/mod.rs`, `dag/edge.rs`, `dag/layout.rs`, `dag/node_box.rs`
3. Define a clean public API in `dag/mod.rs` — only export what's used externally
4. Make `edge.rs` and `layout.rs` `pub(super)` (internal to dag module)
5. Remove `#[allow(dead_code)]` from items that are used within the module
6. Audit truly unused functions in `edge.rs` and `layout.rs` — delete them
7. Update all imports in `views/monitor/render_dag.rs` and `views/chat/`

**Expected: -200 to -500 LOC dead code, better encapsulation**

---

## PHASE 3 — Dead Code Widget Audit

### Problem
`widgets/mod.rs` has 9 modules marked `#[allow(dead_code)]`. Some are legitimate
(data types used elsewhere), others are truly dead.

### Task
For each `#[allow(dead_code)]` module, answer:
1. What types/functions does it export?
2. Which are imported outside the file (grep `use.*module_name::` excluding tests)?
3. Can the `#[allow(dead_code)]` be removed? If items are unused, delete them.

**Modules to audit:**
- `activity_stack` — likely all used (ActivityItem, ActivityTemp)
- `agent_steps` — just trimmed, should be clean now
- `dag_edge` / `dag_layout` / `dag_node_box` — handled in Phase 2
- `infer_stream_box` — data types used by chat/types.rs
- `matrix_decrypt` — 1400 LOC, check what's actually called
- `provider_selector` — just trimmed to VerifyStatus only
- `verb_input` — just trimmed, should be clean

**For `matrix_decrypt.rs` (1400 LOC):** This is the biggest widget. Check if
`StreamingDecrypt`, `MultiLineDecrypt`, `MatrixDecrypt`, `DecryptVerb` are all
used or if some are staged.

**Expected: -500 to -1000 LOC**

---

## PHASE 4 — God Module Splits

### Problem
Several files exceed 1000 LOC with mixed responsibilities:

### 4.1 `views/studio/mod.rs` (1501 LOC)
Split into:
- `studio/mod.rs` — View trait impl + state
- `studio/render.rs` — rendering logic
- `studio/keys.rs` — key handling

### 4.2 `state/event_handler.rs` (1038 LOC)
One giant `match kind { ... }` with 30+ arms. Split by event category:
- `event_handler.rs` — dispatcher (delegates to sub-handlers)
- `event_handler/workflow.rs` — WorkflowStarted, WorkflowFailed, etc.
- `event_handler/task.rs` — TaskStarted, TaskCompleted, etc.
- `event_handler/mcp.rs` — McpInvoke, McpResponse, McpError, etc.
- `event_handler/agent.rs` — AgentTurn, AgentComplete, etc.

### 4.3 `views/chat/mod.rs` (948 LOC)
Already partially split (keys.rs, render.rs, etc.) but still large.
Check if render() and handle_key() can be further extracted.

### 4.4 `views/monitor/mod.rs` (998 LOC)
Split rendering into submodules (render_mission, render_dag, etc. already exist
as separate files — check if mod.rs still has rendering code that should move).

**Expected: Same LOC but better file sizes (target <500 LOC per file)**

---

## PHASE 5 — Rust Idiom Improvements

### 5.1 Remove unnecessary `#[allow(dead_code)]` on View trait
`views/view_trait.rs` lines 53, 64, 77 have `#[allow(dead_code, unused_variables)]`
on default trait method implementations. These are valid Rust patterns — the allows
are unnecessary if the methods are actually overridden by implementors. Check and remove.

### 5.2 Remove `#[allow(dead_code)]` on `Action` enum
`app/types.rs:13` — if all variants are used in the match in routing.rs, the allow
is unnecessary. Verify and remove.

### 5.3 Cache module test-only methods
`state/cache.rs:27,74` — `with_capacity()` and `stats()` have `#[allow(dead_code)]`.
If only used in tests, gate them with `#[cfg(test)]` instead of allowing dead code.

### 5.4 Type-state where possible
Look for `bool` flags that could be type-state patterns (e.g., `loading: bool` →
`AppState<Loading>` / `AppState<Ready>`). Don't over-engineer, but flag opportunities.

---

## Rules

- `cargo check -p nika-tui && cargo clippy -p nika-tui -- -D warnings` after EVERY edit
- `cargo test -p nika-tui --lib` before EVERY commit
- Pre-commit hook uses git stash/pop — stage ALL related files in one `git add`
- Commits: `type(scope): desc` with both co-authors:
  ```
  Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  ```
- 1 logical change = 1 commit (but file moves can batch)
- **NEVER add features** — this is pure refactor
- **NEVER change behavior** — tests must pass unchanged
- If a test breaks, the refactor is wrong, not the test
- Use `spn-rust:rust-architect` agent for architecture decisions
- Read files before modifying
- Push when done

## Commit Plan (estimated)

| # | Message | Phase |
|---|---------|-------|
| 1 | `refactor(tui): delete old wizard/, keep new_wizard as wizard` | 1 |
| 2 | `refactor(tui): consolidate views/wizard.rs into wizard module` | 1 |
| 3 | `refactor(tui): extract dag subsystem into widgets/dag/` | 2 |
| 4 | `refactor(tui): define clean dag public API, delete internal dead code` | 2 |
| 5 | `refactor(tui): audit matrix_decrypt usage, strip unused` | 3 |
| 6 | `refactor(tui): remove remaining unnecessary #[allow(dead_code)]` | 3 |
| 7 | `refactor(tui): split studio/mod.rs into render + keys` | 4 |
| 8 | `refactor(tui): split event_handler into category sub-handlers` | 4 |
| 9 | `refactor(tui): split monitor/mod.rs rendering into submodules` | 4 |
| 10 | `refactor(tui): Rust idiom cleanup — remove unnecessary allows` | 5 |

**Target: 87K → ~80K LOC, zero files > 1000 LOC (non-test), zero unnecessary `#[allow(dead_code)]`**
