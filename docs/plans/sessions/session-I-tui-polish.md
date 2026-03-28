# Session I: TUI Performance Polish (~3-4h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-27-tui-improvements-v4.md` -- READ IT FIRST.
Dev reference: `tools/nika/CLAUDE.md` for crate layout.

TUI crate: `nika-tui` (86k LOC), event crate: `nika-event` (4k LOC).
Current TUI test count: 2153 tests, 0 clippy warnings.
All 26/26 event handlers are tested (100% coverage post-v3 cleanup).

## Mission: Eliminate per-frame allocations in the TUI rendering pipeline

The v3 cleanup session fixed the critical phase clobbering bug and achieved 100% event handler
coverage. Three performance categories remain: JSON clones in event handlers (per-invocation),
DAG deps cloned per render (per-frame at ~60fps), and ~20 `format!()` calls in render paths.
This session addresses the first two (highest impact) and prepares the ground for the third.

### Methodology
For EVERY change: write benchmark or test showing the allocation -> fix -> measure -> commit.
Profile with `cargo flamegraph` or `DHAT` if needed.
`cargo test -p nika-tui --lib` after every change. 1 fix = 1 commit.

---

## VERIFIED FINDINGS (from v4 plan, line numbers TBD at session start)

### HIGH: JSON clones in event handlers

| # | File | Type Cloned | When |
|---|------|-------------|------|
| 1 | `nika-tui/src/app/events.rs` (provider handler) | `Option<serde_json::Value>` (MCP params) | Per MCP invocation |
| 2 | `nika-tui/src/app/events.rs` (provider handler) | `Option<serde_json::Value>` (MCP response) | Per MCP response |
| 3 | `nika-tui/src/app/events.rs` (task handler) | `serde_json::Value` (task inputs) | Per task start |

### HIGH: DAG deps cloned per render

| # | File | Type Cloned | When |
|---|------|-------------|------|
| 4 | `nika-tui/src/` (DAG render) | `FxHashMap<String, Vec<String>>` | Per frame (~60fps) |
| 5 | `nika-tui/src/` (monitor) | `String` task_id, `Vec<String>` deps | Per frame cache rebuild |

### MEDIUM: format!() in render paths (~20 instances)

Deferred to future session. Document the pattern for later cleanup.

---

## Bug 1: Arc<Value> for JSON event data

### Problem
`EventKind::ProviderCalled`, `ProviderResponded`, and `TaskStarted` carry `serde_json::Value` fields.
The TUI event handlers clone these values to store in the TUI state model. For large MCP responses
(10KB+ JSON), this is a significant allocation per event.

### Fix: Engine-level change
**File**: `tools/nika-event/src/log.rs` (3961 LOC)

Wrap large `serde_json::Value` fields in `Arc<serde_json::Value>`:

```rust
// In EventKind variants that carry JSON:
ProviderCalled {
    task_id: Arc<str>,
    provider: String,
    model: String,
    params: Option<Arc<serde_json::Value>>,  // WAS: Option<serde_json::Value>
}
```

This is a cross-crate change: `nika-event` (definition), `nika-engine` (emission), `nika-tui` (consumption).

**Affected event variants** (verify at session start by grepping for `serde_json::Value` in log.rs):
- `ProviderCalled.params`
- `ProviderResponded.response`
- `TaskStarted.inputs`
- Any other variants carrying `Value`

### TDD
1. Write test: emit event with large JSON, clone the event, assert both point to same allocation
2. Fix: change field types to `Arc<Value>`
3. Update all emission sites in `nika-engine` (grep for the variant name)
4. Update all consumption sites in `nika-tui` (event handlers)
5. Verify: `cargo test --workspace --lib`

**Files to modify**:
- `tools/nika-event/src/log.rs` -- field type changes
- `tools/nika-engine/src/runtime/runner.rs` -- wrap emitted values in Arc
- `tools/nika-engine/src/runtime/executor/*.rs` -- wrap emitted values in Arc
- `tools/nika-tui/src/app/events.rs` -- remove `.clone()` on Value fields (Arc::clone is cheap)

**Estimated LOC**: ~60 (type changes + emission wrapping)
**Commit**: `perf(event): wrap JSON event data in Arc to avoid TUI clones`

---

## Bug 2: DAG layout cache

### Problem
The DAG render path rebuilds the dependency map every frame. At 60fps this means cloning
`FxHashMap<String, Vec<String>>` 60 times per second.

### Fix: Cache DAG layout, invalidate on task status change

**File**: TUI state model (find the struct holding DAG data)

Add a `dag_layout_cache: Option<CachedDagLayout>` field to the TUI state. The cache stores
the pre-computed layout (node positions, edge lists, formatted strings). Invalidate the cache
only when a `TaskStarted`, `TaskCompleted`, or `TaskFailed` event arrives.

```rust
struct CachedDagLayout {
    /// Pre-computed node positions
    nodes: Vec<DagNode>,
    /// Pre-computed edge list
    edges: Vec<(usize, usize)>,
    /// Generation counter for invalidation
    generation: u64,
}
```

### TDD
1. Write test: create TUI state with 5 tasks, render DAG, assert cache populated
2. Write test: send TaskCompleted event, assert cache invalidated
3. Write test: render again without status change, assert cache reused (no allocation)
4. Fix: implement caching
5. Verify: `cargo test -p nika-tui --lib`

**Files to modify**:
- `tools/nika-tui/src/` (DAG render module -- locate at session start)
- TUI state struct -- add cache field

**Estimated LOC**: ~80
**Commit**: `perf(tui): cache DAG layout, invalidate only on task status change`

---

## Bug 3: String interning for task_id across event pipeline

### Problem
~15 instances of `to_string()` in event handlers for task_id, verb, error_message. Each allocates.
The same task_id string appears in 5+ events (Started, Completed, ProviderCalled, etc.).

### Fix: Use Arc<str> consistently

Most event fields already use `Arc<str>` for `task_id`. The TUI handlers may still call
`.to_string()` when storing into the state model. Audit and replace with `Arc::clone()`.

### TDD
1. Grep for `.to_string()` and `.clone()` on string fields in TUI event handlers
2. For each: check if the source is already `Arc<str>` -- if so, use `Arc::clone`
3. For state model fields: change `String` to `Arc<str>` where the field originates from events
4. Verify: `cargo test -p nika-tui --lib`

**Files to modify**:
- `tools/nika-tui/src/app/events.rs` -- replace `.to_string()` with Arc::clone
- TUI state types -- change `String` to `Arc<str>` for event-sourced fields

**Estimated LOC**: ~40
**Commit**: `perf(tui): use Arc<str> for task_id in TUI state, avoid to_string()`

---

## Bug 4: Document format!() pattern for future cleanup

### Problem
~20 instances of `format!()` in render methods. Each allocates per frame.

### Fix (this session): Document, do not fix
Create a `// PERF: format!() per frame -- pre-build in state on value change` comment at each site.
Create a tracking section in the v4 plan. Actual fix deferred -- terminal rendering is already fast.

**Commit**: `docs(tui): annotate format!() per-frame allocations for future optimization`

---

## E2E Verification

No `.nika.yaml` needed -- these are internal performance improvements.

### Verification commands:
```bash
# TUI tests pass
cd tools && cargo test -p nika-tui --lib

# Event crate tests pass
cd tools && cargo test -p nika-event --lib

# Engine tests pass (emission sites changed)
cd tools && cargo test -p nika-engine --lib

# Full workspace
cd tools && cargo test --workspace --lib

# Zero clippy warnings
cd tools && cargo clippy --workspace -- -D warnings
```

### Manual TUI verification:
```bash
# Run a workflow and verify TUI renders correctly
nika run examples/agents-preset.nika.yaml --provider mock
# Or: nika ui (if available)
```

---

## After All Fixes
1. `cargo test --workspace --lib` -- ALL pass, no regressions
2. `cargo clippy --workspace -- -D warnings` -- 0 warnings
3. TUI renders correctly with no visual regressions
4. No new per-frame allocations introduced

---

## Commit Strategy (4 commits)

```
perf(event): wrap JSON event data in Arc to avoid TUI clones
perf(tui): cache DAG layout, invalidate only on task status change
perf(tui): use Arc<str> for task_id in TUI state, avoid to_string()
docs(tui): annotate format!() per-frame allocations for future optimization
```
