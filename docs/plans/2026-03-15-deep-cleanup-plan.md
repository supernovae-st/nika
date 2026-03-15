# Nika Deep Cleanup — Architecture Plan

**Date:** 2026-03-15
**Version:** v0.28.0 target
**Philosophy:** v0 — delete everything old as if it never existed
**Scope:** 5 architecture issues, 4 phases, 21 commits

## Problem Statement

The Nika runtime carries legacy code from before the two-phase AST architecture (v0.20).
The analyzer creates clean `AnalyzedWorkflow` with interned `TaskId(u32)`, then `lower()`
immediately **undoes** that work by converting everything back to strings. Two binding systems
coexist. `runner.rs` is 3,548 lines. Dead code paths remain "just in case".

This plan removes all of it.

## Scorecard

| Metric | Before | After |
|--------|--------|-------|
| Lines deleted | — | ~1,800 |
| Lines moved (not new) | — | ~750 |
| New code | — | ~300 |
| **Net** | — | **-1,500 lines** |
| Binding systems | 2 (UseEntry + WithEntry) | 1 (BindingEntry) |
| AST pipeline steps | 4 (Raw → Analyzed → Lower → Runtime) | 3 (Raw → Analyzed → Runtime) |
| Task ID type in Runner | `Arc<str>` | `TaskId(u32)` |
| DAG storage | `HashMap<Arc<str>, Vec<Arc<str>>>` | `Vec<SmallVec<[TaskId; 4]>>` |
| runner.rs lines | 3,548 | ~1,800 |
| Commits | — | 21 atomic commits |

## Architecture Before/After

### BEFORE

```
src/
├── ast/
│   ├── raw/              # Phase 1: YAML → Raw AST
│   ├── analyzed/         # Phase 2: Raw → Analyzed (TaskId)
│   ├── analyzer/         # Validation + transformation
│   ├── lower.rs          # ⚠ Phase 3: Analyzed → Lowered (UNDOES TaskId→String)
│   └── workflow.rs       # ⚠ Lowered Workflow/Task structs (String IDs)
├── binding/
│   ├── entry.rs          # ⚠ BOTH UseEntry (old) + WithEntry (new)
│   ├── resolve.rs        # ⚠ BOTH from_wiring_spec + from_with_spec
│   ├── types.rs          # BindingPath, BindingSource
│   ├── transform.rs      # 27 transforms
│   └── template.rs       # {{use.alias}} substitution
├── dag/
│   ├── flow.rs           # ⚠ HashMap<Arc<str>> adjacency, BOTH from_workflow + from_analyzed
│   └── validate.rs       # ⚠ validate_use_wiring (dead code)
├── runtime/
│   ├── runner.rs         # ⚠ 3,548 lines monolith, uses lowered Workflow
│   └── executor.rs       # Task dispatch
└── store/
    └── datastore.rs      # ⚠ DashMap<String, TaskResult> (default hasher)
```

### AFTER

```
src/
├── ast/
│   ├── raw/              # Phase 1: YAML → Raw AST
│   ├── analyzed/         # Phase 2: Raw → Analyzed (TaskId) ← Runner eats this
│   ├── analyzer/         # Validation + transformation
│   └── workflow.rs       # Supporting types only (McpConfigInline, etc.)
├── binding/
│   ├── entry.rs          # BindingSpec + BindingEntry (single system)
│   ├── resolve.rs        # resolve_bindings() (single path)
│   ├── types.rs          # BindingPath, BindingSource
│   ├── transform.rs      # 27 transforms
│   └── template.rs       # {{use.alias}} substitution
├── dag/
│   ├── mod.rs            # Vec<SmallVec<[TaskId; 4]>> + Kahn's algorithm
│   └── validate.rs       # Cleaned (no validate_use_wiring)
├── runtime/
│   ├── context.rs        # WorkflowMeta (Arc<TaskTable>)
│   ├── runner.rs         # ~1,800 lines (main loop + DAG orchestration)
│   ├── for_each.rs       # for_each + decompose expansion
│   ├── retry.rs          # Structured output retry/repair
│   ├── iteration.rs      # Single task execution unit
│   └── executor.rs       # Task dispatch
└── store/
    └── run_context.rs    # RunContext (DashMap<Arc<str>, TaskResult, FxBuildHasher>)
```

### Pipeline Change

```
BEFORE:
  YAML → RawWorkflow → AnalyzedWorkflow → lower() → Workflow → Runner
                                              ↑
                                         UNDOES TaskId

AFTER:
  YAML → RawWorkflow → AnalyzedWorkflow → Runner
                                            ↑
                                       TaskId flows through
```

---

## Phase 1: Foundation (Additive — Zero Breakage)

All new code, nothing deleted. Old code continues to work. 5 commits.

### Step 1.1: Create WorkflowMeta

**Commit:** `refactor(runtime): add WorkflowMeta struct`
**New file:** `src/runtime/context.rs`

```rust
use std::sync::Arc;
use crate::ast::analyzed::ids::TaskTable;
use crate::ast::analyzed::ids::TaskId;

/// Shared runtime context — read-only after construction.
///
/// Inspired by rustc's Session pattern. Created once from AnalyzedWorkflow,
/// shared via Arc across all runtime components.
pub struct WorkflowMeta {
    /// Bidirectional TaskId ↔ name mapping.
    task_table: TaskTable,

    /// Default provider for the workflow.
    provider: Option<String>,

    /// Default model for the workflow.
    model: Option<String>,
}

impl WorkflowMeta {
    /// Create from an AnalyzedWorkflow.
    pub fn from_workflow(wf: &crate::ast::analyzed::workflow::AnalyzedWorkflow) -> Arc<Self> {
        Arc::new(Self {
            task_table: wf.task_table.clone(),
            provider: wf.provider.clone(),
            model: wf.model.clone(),
        })
    }

    /// Resolve TaskId to human-readable name. Panics if ID is invalid.
    pub fn task_name(&self, id: TaskId) -> &str {
        self.task_table
            .get_name(id)
            .expect("TaskId must be valid — created by analyzer")
    }

    /// Resolve name to TaskId. Returns None for unknown names.
    pub fn task_id(&self, name: &str) -> Option<TaskId> {
        self.task_table.get_id(name)
    }

    /// Default provider.
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// Default model.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Access the task table directly.
    pub fn task_table(&self) -> &TaskTable {
        &self.task_table
    }
}
```

**Tests:** 5-6 unit tests (roundtrip, unknown name, provider access).

---

### Step 1.2: Create Vec-indexed Dag

**Commit:** `refactor(dag): add Vec-indexed Dag with Kahn's algorithm`
**New file:** `src/dag/indexed.rs` (alongside existing `flow.rs`)

```rust
use smallvec::SmallVec;
use crate::ast::analyzed::ids::{TaskId, TaskTable};
use crate::ast::analyzed::workflow::AnalyzedWorkflow;
use crate::error::NikaError;

/// Adjacency list — 4 inline deps covers 95% of tasks.
pub(crate) type DepVec = SmallVec<[TaskId; 4]>;

/// Pre-computed topological ordering with depth info.
pub struct TopoOrder {
    /// Tasks in valid execution order.
    order: Box<[TaskId]>,
    /// Depth of each task (indexed by TaskId.0). Depth 0 = root.
    depths: Box<[u32]>,
}

impl TopoOrder {
    /// Tasks in topological order.
    pub fn order(&self) -> &[TaskId] {
        &self.order
    }

    /// Depth of a task (0 = no dependencies).
    pub fn depth(&self, id: TaskId) -> u32 {
        self.depths[id.0 as usize]
    }
}

/// Vec-indexed DAG — O(1) adjacency lookup by TaskId.
pub struct Dag {
    /// Outgoing edges: successors[task.0] = tasks that depend on task.
    successors: Vec<DepVec>,
    /// Incoming edges: predecessors[task.0] = tasks that task depends on.
    predecessors: Vec<DepVec>,
    /// Pre-computed topological order + depths.
    topo: TopoOrder,
    /// Number of tasks.
    num_tasks: usize,
}

impl Dag {
    /// Build from AnalyzedWorkflow. Performs Kahn's topological sort
    /// with simultaneous cycle detection and depth computation.
    pub fn from_analyzed(wf: &AnalyzedWorkflow) -> Result<Self, NikaError> {
        let n = wf.tasks.len();
        let mut successors: Vec<DepVec> = vec![DepVec::new(); n];
        let mut predecessors: Vec<DepVec> = vec![DepVec::new(); n];
        let mut in_degree: Vec<u32> = vec![0; n];

        // Build adjacency from depends_on + implicit_deps
        for task in &wf.tasks {
            let task_idx = task.id.0 as usize;
            for &dep_id in task.depends_on.iter().chain(task.implicit_deps.iter()) {
                let dep_idx = dep_id.0 as usize;
                successors[dep_idx].push(task.id);
                predecessors[task_idx].push(dep_id);
                in_degree[task_idx] += 1;
            }
        }

        // Kahn's algorithm: BFS from roots, compute topo order + depths
        let topo = kahn_sort(&successors, &mut in_degree, n, &wf.task_table)?;

        Ok(Self { successors, predecessors, topo, num_tasks: n })
    }

    /// Tasks that `id` depends on (predecessors).
    pub fn dependencies(&self, id: TaskId) -> &[TaskId] {
        &self.predecessors[id.0 as usize]
    }

    /// Tasks that depend on `id` (successors).
    pub fn successors(&self, id: TaskId) -> &[TaskId] {
        &self.successors[id.0 as usize]
    }

    /// Pre-computed topological order.
    pub fn topo_order(&self) -> &[TaskId] {
        self.topo.order()
    }

    /// Depth of task in the DAG (0 = root task).
    pub fn depth(&self, id: TaskId) -> u32 {
        self.topo.depth(id)
    }

    /// Tasks with no successors (leaf/final tasks).
    pub fn final_tasks(&self) -> Vec<TaskId> {
        (0..self.num_tasks)
            .filter(|&i| self.successors[i].is_empty())
            .map(|i| TaskId(i as u32))
            .collect()
    }

    /// Number of tasks.
    pub fn len(&self) -> usize {
        self.num_tasks
    }

    /// Check if a task has all dependencies satisfied (for ready-task detection).
    pub fn all_deps_done(&self, id: TaskId, done: &[bool]) -> bool {
        self.predecessors[id.0 as usize]
            .iter()
            .all(|dep| done[dep.0 as usize])
    }
}

/// Kahn's topological sort with cycle detection and depth computation.
fn kahn_sort(
    successors: &[DepVec],
    in_degree: &mut [u32],
    n: usize,
    task_table: &TaskTable,
) -> Result<TopoOrder, NikaError> {
    use std::collections::VecDeque;

    let mut queue = VecDeque::with_capacity(n);
    let mut order = Vec::with_capacity(n);
    let mut depths = vec![0u32; n];

    // Seed with roots (in_degree == 0)
    for i in 0..n {
        if in_degree[i] == 0 {
            queue.push_back(TaskId(i as u32));
        }
    }

    while let Some(id) = queue.pop_front() {
        order.push(id);
        for &succ in &successors[id.0 as usize] {
            let succ_idx = succ.0 as usize;
            in_degree[succ_idx] -= 1;
            // Depth = max(predecessor depths) + 1
            depths[succ_idx] = depths[succ_idx].max(depths[id.0 as usize] + 1);
            if in_degree[succ_idx] == 0 {
                queue.push_back(succ);
            }
        }
    }

    if order.len() != n {
        // Find cycle participants for error message
        let cycle_tasks: Vec<String> = (0..n)
            .filter(|&i| in_degree[i] > 0)
            .filter_map(|i| task_table.get_name(TaskId(i as u32)).map(|s| s.to_string()))
            .collect();
        return Err(NikaError::CyclicDependency {
            tasks: cycle_tasks,
        });
    }

    Ok(TopoOrder {
        order: order.into_boxed_slice(),
        depths: depths.into_boxed_slice(),
    })
}
```

**Tests:** 10-12 unit tests (empty DAG, linear chain, diamond, cycle detection,
depth computation, final_tasks, all_deps_done).

---

### Step 1.3: Rename RunContext → RunContext

**Commit:** `refactor(store): rename RunContext → RunContext`
**File:** `src/store/datastore.rs` → `src/store/run_context.rs`

Mechanical find/replace:
- `RunContext` → `RunContext` (all files)
- `datastore` → `run_ctx` (field names in Runner, tests)
- Update `mod.rs` re-exports

No logic changes. All tests pass with renames.

---

### Step 1.4: Switch RunContext to FxBuildHasher

**Commit:** `refactor(store): switch RunContext to FxBuildHasher`
**File:** `src/store/run_context.rs`

```rust
use dashmap::DashMap;
use rustc_hash::FxBuildHasher;

pub struct RunContext {
    results: Arc<DashMap<Arc<str>, TaskResult, FxBuildHasher>>,
}

impl RunContext {
    pub fn new() -> Self {
        Self {
            results: Arc::new(DashMap::with_hasher(FxBuildHasher)),
        }
    }
}
```

Add `rustc-hash` to `Cargo.toml` if not already present.
All tests pass — DashMap API is hasher-agnostic.

---

### Step 1.5: Rename TaskStatus → TaskOutcome

**Commit:** `refactor(store): rename TaskStatus → TaskOutcome`
**File:** `src/store/run_context.rs` (was datastore.rs)

Mechanical find/replace of the store enum only:
- `store::TaskStatus` → `store::TaskOutcome`
- `TaskStatus::Success` → `TaskOutcome::Success` (in store context)
- TUI `theme::TaskStatus` is **untouched** — different enum, different purpose

---

## Phase 2: Pipeline Surgery (The Big Switch)

Delete `lower()`. Runner consumes `AnalyzedWorkflow` directly. 7 commits.
This is the highest-risk phase — tests that construct lowered types will break.

### Step 2.1: Modify Runner to accept AnalyzedWorkflow

**Commit:** `refactor(runtime): Runner accepts AnalyzedWorkflow`
**File:** `src/runtime/runner.rs`

```rust
// BEFORE
pub struct Runner {
    workflow: Workflow,
    flow_graph: Dag,         // old HashMap Dag
    datastore: RunContext,
    // ...
}

impl Runner {
    pub fn with_event_log(workflow: Workflow, event_log: EventLog) -> Self { ... }
}

// AFTER
pub struct Runner {
    workflow: AnalyzedWorkflow,
    dag: Dag,                // new Vec-indexed Dag
    run_ctx: RunContext,
    rt_ctx: Arc<WorkflowMeta>,
    // ...
}

impl Runner {
    pub fn new(workflow: AnalyzedWorkflow, event_log: EventLog) -> Result<Self, NikaError> {
        let rt_ctx = WorkflowMeta::from_workflow(&workflow);
        let dag = Dag::from_analyzed(&workflow)?;
        let run_ctx = RunContext::new();
        Ok(Self { workflow, dag, run_ctx, rt_ctx, /* ... */ })
    }
}
```

**Impact:** Every call site that creates a Runner needs updating.

---

### Step 2.2: Switch DAG construction

**Commit:** `refactor(dag): wire Dag::from_analyzed() as sole builder`
**Files:** `src/runtime/runner.rs`, `src/dag/mod.rs`

- Runner uses `Dag::from_analyzed()` (new Vec-indexed)
- DAG queries use `TaskId` indexing
- `get_ready_tasks()` uses `dag.all_deps_done(id, &done_flags)`
- `all_done()` checks `done_flags.iter().all(|&d| d)`

---

### Step 2.3: Use TaskId internally in Runner

**Commit:** `refactor(runtime): use TaskId internally in Runner`
**File:** `src/runtime/runner.rs`

Internal task identification switches from `Arc<str>` to `TaskId`:
- Task lookup: `workflow.get_task(id)` → O(1)
- Dependency checks: `TaskId` comparison (no string alloc)
- DAG operations: direct Vec indexing

**Boundary conversion** (String only at edges):
- Event emission: `rt_ctx.task_name(id)` → `Arc<str>`
- Error messages: include task name from WorkflowMeta
- RunContext store keys: still `Arc<str>` (for_each dynamic IDs)

---

### Step 2.4: Update all call sites

**Commit:** `refactor: update call sites for new Runner API`
**Files:** `src/main.rs`, `src/commands/`, `src/tui/`

Every place that currently does:
```rust
let raw = parse(yaml, file_id)?;
let analyzed = analyze(raw)?;
let workflow = lower(analyzed);  // ← DELETE THIS
let runner = Runner::with_event_log(workflow, event_log);
```

Becomes:
```rust
let raw = parse(yaml, file_id)?;
let analyzed = analyze(raw)?;
let runner = Runner::new(analyzed, event_log)?;
```

---

### Step 2.5: Delete lower.rs

**Commit:** `refactor(ast): delete lower.rs`
**File:** `src/ast/lower.rs` — **DELETE ENTIRE FILE** (~788 lines)

Remove from `src/ast/mod.rs` module declarations.
Pure deletion, no replacement code.

---

### Step 2.6: Delete lowered Workflow/Task structs

**Commit:** `refactor(ast): delete lowered Workflow/Task structs`
**File:** `src/ast/workflow.rs`

Delete:
- `Workflow` struct (the lowered one, ~50 lines)
- `Task` struct (the lowered one, ~150 lines)
- `TaskAction` enum (the lowered one)
- `Flow` struct
- All `impl` blocks for these types
- All supporting `Deserialize` impls

**Keep** in workflow.rs (if used by raw parser):
- `McpConfigInline` (if raw parser needs it)
- Any re-exports that raw/ depends on

**Audit:** Before deleting, grep for imports of these types from other modules.
If only lower.rs and runner.rs imported them, safe to delete.

---

### Step 2.7: Delete Dag::from_workflow()

**Commit:** `refactor(dag): delete Dag::from_workflow()`
**File:** `src/dag/flow.rs`

Delete `from_workflow()` method (~90 lines) and its deduplication logic.
Only `Dag::from_analyzed()` (in new `indexed.rs`) remains.

Old `flow.rs` can be deleted if no other code uses the old `Dag` type.
If `StableDag` (for TUI visualization) lives here, keep that part only.

---

## Phase 3: Binding Cleanup (Dead Code Removal)

Delete old binding system. Rename survivor to clean names. 5 commits.

### Step 3.1: Delete UseEntry and WiringSpec

**Commit:** `refactor(binding): delete UseEntry and WiringSpec`
**File:** `src/binding/entry.rs`

Delete (~300 lines):
- `WiringSpec` type alias (line 27)
- `UseEntry` struct + all methods (lines 40-163)
- `parse_use_entry()` function (lines 125-163)
- `UseEntry` Deserialize visitor (lines 196-271)
- All `#[cfg(test)]` tests for UseEntry

---

### Step 3.2: Delete from_wiring_spec and LazyBinding::Pending

**Commit:** `refactor(binding): delete from_wiring_spec() and LazyBinding::Pending`
**File:** `src/binding/resolve.rs`

Delete:
- `LazyBinding::Pending` variant (the old one with raw `String` path)
- `from_wiring_spec()` method
- Old `resolve_entry()` helper
- Any match arms for `LazyBinding::Pending`

**Simplify:** `LazyBinding` enum loses the old variant:
```rust
// BEFORE
pub enum LazyBinding {
    Resolved(Value),
    Pending { path: String, default: Option<Value> },        // ← DELETE
    PendingWithEntry { source: BindingPath, /* ... */ },      // ← KEEP
}

// AFTER
pub enum LazyBinding {
    Resolved(Value),
    Pending { source: BindingPath, transform: Option<TransformExpr>, /* ... */ },
}
```

---

### Step 3.3: Delete validate_use_wiring

**Commit:** `refactor(dag): delete validate_use_wiring()`
**File:** `src/dag/validate.rs`

Delete `validate_use_wiring()` function (~20 lines) and its caller in runner.rs.

---

### Step 3.4: Rename WithSpec → BindingSpec, WithEntry → BindingEntry

**Commit:** `refactor(binding): rename WithSpec → BindingSpec, WithEntry → BindingEntry`
**Files:** All files importing these types

Mechanical find/replace:
- `WithSpec` → `BindingSpec`
- `WithEntry` → `BindingEntry`
- `with_spec` → `bindings` (field names)
- `parse_with_entry()` → `parse_binding_entry()`
- Serde: `#[serde(rename = "with")]` still maps YAML `with:` → Rust field `bindings`

---

### Step 3.5: Rename from_with_spec → resolve_bindings

**Commit:** `refactor(binding): rename from_with_spec() → resolve_bindings()`
**File:** `src/binding/resolve.rs`

- `ResolvedBindings::from_with_spec()` → `ResolvedBindings::resolve()`
- `PendingWithEntry` → `Pending` (only variant left, drop the "With" prefix)

---

## Phase 4: File Splits + Polish (Pure Refactor)

Extract modules from runner.rs. 4 commits.

### Step 4.1: Extract for_each.rs

**Commit:** `refactor(runtime): extract for_each.rs from runner.rs`
**New file:** `src/runtime/for_each.rs`

Move lines ~1008-1330 from runner.rs:
- Decompose modifier expansion via MCP
- for_each binding resolution (`$binding`, `{{use.alias}}`, `$inputs.xxx`)
- Nested path traversal
- JSON string parsing fallbacks

```rust
// Public API
pub(crate) async fn expand_for_each_items(
    task: &AnalyzedTask,
    bindings: &ResolvedBindings,
    run_ctx: &RunContext,
    rt_ctx: &Arc<WorkflowMeta>,
    executor: &TaskExecutor,
    event_log: &EventLog,
) -> Result<Option<Vec<Value>>, NikaError>
```

Move associated tests (~12 test functions).
~320 lines moved.

---

### Step 4.2: Extract retry.rs

**Commit:** `refactor(runtime): extract retry.rs from runner.rs`
**New file:** `src/runtime/retry.rs`

Move lines ~378-605 from runner.rs:
- `get_retry_config()`
- `execute_with_retry()` — main retry loop with LLM repair
- `build_retry_prompt()` — repair prompt generation

```rust
// Public API
pub(crate) async fn execute_with_retry(
    task: &AnalyzedTask,
    initial_result: TaskResult,
    executor: &TaskExecutor,
    rt_ctx: &Arc<WorkflowMeta>,
    event_log: &EventLog,
) -> Result<TaskResult, NikaError>
```

~230 lines moved.

---

### Step 4.3: Extract iteration.rs

**Commit:** `refactor(runtime): extract iteration.rs from runner.rs`
**New file:** `src/runtime/iteration.rs`

Move lines ~605-800 from runner.rs:
- `execute_task_iteration()` — single execution unit
- Context binding setup
- Artifact configuration
- Task dispatch (5 verbs)
- Result collection

```rust
// Public API
pub(crate) async fn execute_task_iteration(
    task: &AnalyzedTask,
    task_id_str: Arc<str>,
    parent_task_id: Arc<str>,
    run_ctx: RunContext,
    executor: TaskExecutor,
    rt_ctx: Arc<WorkflowMeta>,
    event_log: EventLog,
    // ...
) -> IterationResult
```

~200 lines moved.

---

### Step 4.4: Final cleanup

**Commit:** `refactor(dag): consolidate dag module`
**File:** `src/dag/flow.rs` → merge into `src/dag/mod.rs` if old Dag is fully replaced

Remove dead imports, unused `pub(crate)` visibility, orphaned helper functions.
Run `cargo clippy -- -D warnings` and fix any new warnings.

---

## New Type Reference

### WorkflowMeta (`src/runtime/context.rs`)

| Field | Type | Source |
|-------|------|--------|
| `task_table` | `TaskTable` | `AnalyzedWorkflow.task_table` |
| `provider` | `Option<String>` | `AnalyzedWorkflow.provider` |
| `model` | `Option<String>` | `AnalyzedWorkflow.model` |

**Methods:** `from_workflow()`, `task_name()`, `task_id()`, `provider()`, `model()`, `task_table()`
**Sharing:** `Arc<WorkflowMeta>` — created once, shared across runner + spawned tasks.

### Dag (`src/dag/indexed.rs`)

| Field | Type | Purpose |
|-------|------|---------|
| `successors` | `Vec<DepVec>` | Outgoing edges, indexed by `TaskId.0` |
| `predecessors` | `Vec<DepVec>` | Incoming edges, indexed by `TaskId.0` |
| `topo` | `TopoOrder` | Pre-computed order + depths |
| `num_tasks` | `usize` | Task count |

**DepVec:** `SmallVec<[TaskId; 4]>` — 4 inline deps, heap fallback for complex DAGs.
**TopoOrder:** `Box<[TaskId]>` order + `Box<[u32]>` depths — immutable after construction.
**Algorithm:** Kahn's BFS — computes topo order + depths + cycle detection in one pass.
**Ownership:** Owned by Runner (not Arc) — spawned tasks don't access DAG.

### RunContext (`src/store/run_context.rs`)

| Field | Type | Purpose |
|-------|------|---------|
| `results` | `Arc<DashMap<Arc<str>, TaskResult, FxBuildHasher>>` | Concurrent task results |

**Keys:** `Arc<str>` (not `TaskId`) — for_each generates dynamic store IDs like `"task_0"`.
**Hasher:** `FxBuildHasher` — faster than default SipHash for string keys.
**Sharing:** `Clone` via `Arc` — shared across spawned tokio tasks.

**Future (Phase 2 migration):** When Runner extends `TaskTable` with runtime ID allocation
for for_each/decompose tasks, keys can migrate to `TaskId`.

### TaskOutcome (`src/store/run_context.rs`)

```rust
pub enum TaskOutcome {
    Success,
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
}
```

Renamed from `store::TaskStatus`. TUI's `theme::TaskStatus` (7 variants) stays independent.

### BindingSpec / BindingEntry (`src/binding/entry.rs`)

Renamed from `WithSpec` / `WithEntry`. Same fields, same semantics:

| Field | Type | Purpose |
|-------|------|---------|
| `source` | `BindingPath` | Typed path ($step1.data, $context.files, etc.) |
| `binding_type` | `BindingType` | Type constraint (Any, String, Object, Array) |
| `default` | `Option<Value>` | Default after transforms |
| `lazy` | `bool` | Deferred resolution |
| `transform` | `Option<TransformExpr>` | 27 built-in transforms |

YAML keyword remains `with:`. Serde rename handles the mapping.

---

## Commit Sequence (21 commits)

```
Phase 1: Foundation (5 commits)
──────────────────────────────
 1. refactor(runtime): add WorkflowMeta struct
 2. refactor(dag): add Vec-indexed Dag with Kahn's algorithm
 3. refactor(store): rename RunContext → RunContext
 4. refactor(store): switch RunContext to FxBuildHasher
 5. refactor(store): rename TaskStatus → TaskOutcome

Phase 2: Pipeline Surgery (7 commits)
──────────────────────────────────────
 6. refactor(runtime): Runner accepts AnalyzedWorkflow
 7. refactor(dag): wire Dag::from_analyzed() as sole builder
 8. refactor(runtime): use TaskId internally in Runner
 9. refactor: update all call sites for new Runner API
10. refactor(ast): delete lower.rs
11. refactor(ast): delete lowered Workflow/Task structs
12. refactor(dag): delete Dag::from_workflow()

Phase 3: Binding Cleanup (5 commits)
─────────────────────────────────────
13. refactor(binding): delete UseEntry and WiringSpec
14. refactor(binding): delete from_wiring_spec() and LazyBinding::Pending
15. refactor(dag): delete validate_use_wiring()
16. refactor(binding): rename WithSpec → BindingSpec, WithEntry → BindingEntry
17. refactor(binding): rename from_with_spec() → resolve_bindings()

Phase 4: File Splits (4 commits)
────────────────────────────────
18. refactor(runtime): extract for_each.rs from runner.rs
19. refactor(runtime): extract retry.rs from runner.rs
20. refactor(runtime): extract iteration.rs from runner.rs
21. refactor(dag): consolidate dag module
```

---

## Test Strategy

### Phase 1 (additive — zero test breakage)

| Action | Impact |
|--------|--------|
| New WorkflowMeta tests | +5 tests |
| New Dag tests | +12 tests |
| RunContext rename | 0 breakage (find/replace) |
| FxBuildHasher switch | 0 breakage (API-compatible) |
| TaskOutcome rename | 0 breakage (find/replace) |

### Phase 2 (pipeline surgery — tests rewritten)

| Action | Impact |
|--------|--------|
| Runner test fixtures | **Rewrite** — construct AnalyzedTask instead of lowered Task |
| lower.rs tests | **Delete** — lowering no longer exists |
| DAG tests using from_workflow | **Delete** or switch to from_analyzed |
| Integration tests (YAML → Runner) | **Pass** — same pipeline minus lower() |
| TUI tests | **Update** — Runner API change |

**Estimate:** ~50-80 tests rewritten, ~30 tests deleted, ~6,000 tests unchanged.

### Phase 3 (dead code — tests deleted)

| Action | Impact |
|--------|--------|
| UseEntry tests | **Delete** (~20 tests) |
| WiringSpec tests | **Delete** |
| from_wiring_spec tests | **Delete** |
| validate_use_wiring tests | **Delete** |
| Rename-related tests | 0 breakage (find/replace) |

### Phase 4 (file moves — zero test breakage)

Tests move WITH their code. Module paths update in `use` statements.

---

## Risk Assessment

| Phase | Risk | Mitigation |
|-------|------|------------|
| 1 | LOW | Additive only, old code untouched |
| 2 | **HIGH** | Runner contract change, test rewrites. Run `cargo test` after each sub-step |
| 3 | LOW | Removing dead code, old system never runs in production |
| 4 | ZERO | File moves, no logic changes |

**Phase 2 mitigation strategy:**
1. Write new Runner tests FIRST (using AnalyzedWorkflow)
2. Switch Runner implementation
3. Verify new tests pass
4. Delete old tests that reference lowered types
5. Run full `cargo test` — all 6,157 must pass

---

## Constraints

### for_each Dynamic IDs

`for_each` generates dynamic `store_id: Arc<str>` values (e.g., `"task_0"`, `"task_1"`)
that are NOT in the static `TaskTable`. This is why RunContext keeps `Arc<str>` keys
instead of `TaskId`.

**Future migration path:** Extend `TaskTable` with a `runtime_alloc(name: &str) -> TaskId`
method that atomically allocates new IDs for dynamic tasks. Then RunContext can use
`DashMap<TaskId, TaskResult>`. This is out of scope for this cleanup.

### TUI StableDag

The TUI chat view uses `petgraph::StableDag` for visualization. This is separate from
the execution DAG and should NOT be affected by the new Vec-indexed Dag.
Keep petgraph dependency for TUI only.

### SSE MCP Transport

`lower()` currently drops SSE MCP servers. After deletion, Runner should skip SSE servers
during MCP client initialization (same behavior, different location).

---

## Dependencies

```
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4
  │              │
  │              └── Phase 3 needs lowered types gone (use_wiring field)
  │
  └── Phase 2 needs WorkflowMeta + new Dag from Phase 1
```

Phase 3 and Phase 4 are independent of each other and could be parallelized
on separate branches. Phase 2 is the critical path.

---

## Success Criteria

- [ ] `cargo test` passes (6,157+ tests)
- [ ] `cargo clippy -- -D warnings` clean
- [ ] `cargo fmt` clean
- [ ] `lower.rs` does not exist
- [ ] `UseEntry` / `WiringSpec` do not exist
- [ ] Runner struct holds `AnalyzedWorkflow`, not `Workflow`
- [ ] DAG uses `Vec<SmallVec<[TaskId; 4]>>`, not `HashMap<Arc<str>>`
- [ ] `RunContext` renamed to `RunContext`
- [ ] `runner.rs` < 2,000 lines
- [ ] Net line count: -1,500 or more
