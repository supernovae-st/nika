# BUG: nika:dag_info task_count incorrect for for_each workflows

**Severity**: MEDIUM  
**Component**: `nika-engine/src/runtime/builtin/introspect_dag.rs`  
**Found by**: Code review (spn-powers:code-reviewer) — 2026-04-02  
**Status**: Confirmed, not yet fixed

---

## Summary

`nika:dag_info` reports incorrect `task_count` and `pending` values when the
workflow contains `for_each` tasks. The count reflects YAML task definitions,
not runtime iterations.

## Reproduction

```yaml
schema: "nika/workflow@0.12"
workflow: dag-info-for-each-bug
provider: mock

tasks:
  - id: generate_items
    exec:
      command: 'echo ''["a","b","c","d","e","f","g","h","i","j"]'''
      shell: true

  - id: process
    depends_on: [generate_items]
    with:
      items: $generate_items
    for_each:
      items: "{{with.items}}"
      as: item
      concurrency: 2
    infer: "Process {{with.item}}"

  - id: check_dag
    depends_on: [generate_items]
    invoke:
      tool: "nika:dag_info"
```

### Expected (mid-execution)

```json
{
  "task_count": 12,
  "completed": 6,
  "failed": 0,
  "pending": 6
}
```

(12 = 1 generate_items + 10 for_each iterations + 1 check_dag)

### Actual

```json
{
  "task_count": 3,
  "completed": 5,
  "failed": 0,
  "pending": 0
}
```

`task_count=3` (YAML definitions), `completed=5` (5 iterations done),
`pending=0` (saturating_sub: 3 - 5 = 0).

## Root Cause

`DagInfoTool` uses `WorkflowStarted.task_count` which equals
`self.workflow.tasks.len()` — the number of static YAML task definitions.
For `for_each`, 1 YAML definition spawns N runtime iterations. The tool
doesn't account for this expansion.

### Code path

1. `Runner::run()` emits `WorkflowStarted { task_count: workflow.tasks.len() }`
   - File: `runner.rs` ~line 1590
   - `tasks.len()` = number of YAML `- id:` blocks

2. `DagInfoTool::call()` reads `task_count` from this event
   - File: `introspect_dag.rs` line 80-81
   - Uses `total_task_count = Some(*task_count)` → 3 (YAML blocks)

3. For each iteration, the runner emits per-iteration events:
   - `ForEachItemStarted { task_id: "process", index: 0, total: 10 }`
   - `ForEachItemCompleted { task_id: "process", index: 0, duration_ms: ... }`
   - These share the SAME `task_id` ("process") but different `index`

4. `completed.insert(task_id.to_string())` deduplicates by task_id
   - All 10 iterations write to the same key "process"
   - `completed.len()` = 1 (not 10)

Wait — actually this means the current code also **undercounts** completed.
The `TaskCompleted` event for for_each uses the parent task_id. So
`completed.len()` stays at 1 regardless of how many iterations finish.

The real issue is dual:
- `task_count` is too low (YAML defs, not runtime tasks)
- `completed` count doesn't reflect for_each iteration progress

## Available Events (already emitted, unused by dag_info)

The event system already emits granular for_each events. These are the
building blocks for a correct fix:

```rust
// Batch-level (emitted once per for_each task)
ForEachStarted {
    task_id: Arc<str>,       // "process"
    item_count: usize,       // 10
    concurrency: usize,      // 2
    fail_fast: bool,
}
ForEachCompleted {
    task_id: Arc<str>,       // "process"
    total: u32,              // 10
    succeeded: u32,          // 8
    failed: u32,             // 2
    skipped: u32,            // 0
    duration_ms: u64,
}

// Item-level (emitted per iteration)
ForEachItemStarted {
    task_id: Arc<str>,       // "process"
    index: usize,            // 0..9
    total: usize,            // 10
}
ForEachItemCompleted {
    task_id: Arc<str>,
    index: usize,
    duration_ms: u64,
}
ForEachItemFailed {
    task_id: Arc<str>,
    index: usize,
    error: String,
}
```

## Proposed Fix

### Option A — Use ForEachStarted to adjust task_count (minimal)

In `DagInfoTool::call()`, also track `ForEachStarted` events. For each
for_each task, add `item_count - 1` to the total (the -1 because the
parent task is already counted in `WorkflowStarted.task_count`).

Track item-level events for accurate completed/failed counts.

```rust
let mut for_each_expansion: usize = 0;
let mut fe_items_completed: usize = 0;
let mut fe_items_failed: usize = 0;

// In the event loop:
EventKind::ForEachStarted { item_count, .. } => {
    for_each_expansion += item_count.saturating_sub(1);
}
EventKind::ForEachItemCompleted { .. } => {
    fe_items_completed += 1;
}
EventKind::ForEachItemFailed { .. } => {
    fe_items_failed += 1;
}

// Adjusted totals:
let task_count = total_task_count.unwrap_or(observed_tasks.len())
    + for_each_expansion;
let completed_count = completed.len() + fe_items_completed;
let failed_count = failed.len() + fe_items_failed;
let pending = task_count.saturating_sub(completed_count + failed_count);
```

**Pros**: Minimal change, uses existing events, no new plumbing.  
**Cons**: Double-counts if both `TaskCompleted` and `ForEachItemCompleted`
fire for the same task (need to check if `TaskCompleted` fires per-item
or only for the parent).

### Option B — Add runtime_task_count to DagInfoTool (cleaner)

Pass the actual runtime task count (including expansions) to the tool at
construction time or via a shared atomic counter.

```rust
pub struct DagInfoTool {
    event_log: EventLog,
    runtime_task_count: Arc<AtomicUsize>,  // updated by runner
}
```

The runner increments `runtime_task_count` when spawning for_each iterations.

**Pros**: Accurate, no event parsing.  
**Cons**: Needs plumbing through `wire_introspection_tools()` and runner state.

### Recommendation

**Option A** — it's self-contained in `introspect_dag.rs`, uses events that
already exist, and doesn't require threading new state through the runner.

## Files to Change

| File | Change |
|------|--------|
| `nika-engine/src/runtime/builtin/introspect_dag.rs` | Add ForEachStarted/Item event handling |
| `nika-engine/src/runtime/builtin/introspect_dag.rs` | Update DagInfoResponse with for_each breakdown |
| `nika-engine/src/runtime/builtin/introspect_dag.rs` | Add test with ForEachStarted events |

## Verification

```yaml
# Test workflow — run and check dag_info output via trace
schema: "nika/workflow@0.12"
provider: mock

tasks:
  - id: items
    exec: 'echo ''["a","b","c"]'''
    shell: true

  - id: process
    depends_on: [items]
    for_each: $items
    as: item
    concurrency: 1
    infer: "{{with.item}}"

  - id: dag_check
    depends_on: [items]
    invoke: { tool: "nika:dag_info" }
```

Expected `dag_check` output:
```json
{
  "task_count": 5,
  "completed": 1,
  "failed": 0,
  "pending": 4
}
```

(5 = items + 3 for_each iterations + dag_check)

## Context

This bug was found during code review of commit `01cd53fd0` which fixed the
original dag_info issue (only counting event-observed tasks). The fix
correctly added `WorkflowStarted.task_count` as the source of truth for
linear workflows, but doesn't account for for_each dynamic expansion.

The `decompose` verb (which also spawns sub-tasks) may have the same issue
but is less common.
