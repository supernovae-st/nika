# Research Report: Rust Architecture Patterns for Nika Deep Cleanup

**Date:** 2026-03-15
**Author:** Claude (research agent)
**Scope:** 5 specific architecture problems in the Nika codebase
**Sources:** rustc internals, rust-analyzer, ripgrep, servo, salsa, cargo, recent Rust ecosystem blog posts

---

## Summary

This report investigates five concrete Rust architecture patterns relevant to Nika's planned deep cleanup. Each section draws from real-world Rust projects (rustc, rust-analyzer, ripgrep, cargo, salsa) and recent community best practices. The goal is to provide actionable patterns, not abstract theory.

---

## 1. Newtype Pattern vs String for IDs Across Module Boundaries

### The Problem in Nika

Nika has a two-phase AST where the Analyzed layer uses `TaskId(u32)` with a `TaskTable` for interning, but the `lower()` function converts everything back to `String` for the runtime `Workflow`/`Task` types. The runtime then does string comparisons and HashMap lookups by name.

### How Major Projects Handle This

**rustc: Interned IDs everywhere, never convert back**

rustc's approach is definitive: once you intern, you stay interned. `DefId`, `HirId`, `NodeId` are all newtypes that flow through every compiler phase. The key insight is that rustc never "un-interns" IDs back to strings except at the very final output boundary (error messages, diagnostics).

```
Source → AST (NodeId) → HIR (HirId) → MIR (DefId) → codegen
         ^-- interned once, never converted back to strings
```

The resolution context (the equivalent of `TaskTable`) travels alongside through a `TyCtxt` context object that is available everywhere.

**rust-analyzer: `la-arena` crate for typed arenas**

rust-analyzer uses the `la-arena` crate (which they authored) for typed index newtypes. Their pattern:

```rust
// la-arena defines:
pub struct Idx<T> { raw: u32, _marker: PhantomData<T> }
pub struct Arena<T> { data: Vec<T> }
```

Every AST node gets an `Idx<T>` that is type-safe (you cannot accidentally use a `FunctionIdx` where an `ExprIdx` is expected). The arena travels through the entire compilation pipeline.

**salsa: Interned values as first-class concept**

The salsa incremental computation framework (used by rust-analyzer) treats interning as a core database operation. Interned values get IDs that are valid for the lifetime of the database session. The key pattern: the interning database is available everywhere, so you never need to convert back.

### Recommended Pattern for Nika

**Keep `TaskId(u32)` all the way to the runtime.** The `lower()` function should not convert back to strings. Instead:

1. **Pass the `TaskTable` alongside the workflow** -- either embed it in the workflow struct or pass it as a separate context parameter to the runtime.

2. **Use a context object pattern** (like rustc's `TyCtxt`):

```rust
/// Runtime execution context -- carries interning tables
pub struct RunCtx {
    pub task_table: TaskTable,
    pub mcp_table: StringTable,
    // ... other shared state
}
```

3. **Only convert to String at the boundary** -- error messages, TUI display, trace output. This is where `Display` impls on the newtype come in.

4. **Consider `la-arena`** if you want type-safe indices with zero boilerplate. It is mature, widely used, and only ~200 lines of code.

### Migration Strategy

Phase 1: Make `Runner` accept `TaskTable` alongside `Workflow`.
Phase 2: Change `runner.rs` internal lookups from `HashMap<String, _>` to `HashMap<TaskId, _>`.
Phase 3: Gradually push `TaskId` deeper into executor, binding resolution.
Phase 4: Remove string-based fields from the runtime `Task` struct (keep `TaskId` + table reference).

---

## 2. Splitting Large Files in Rust

### The Problem in Nika

Several files exceed comfortable review size:

| File | Lines | Content |
|------|-------|---------|
| `runtime/runner.rs` | 3,548 | Workflow orchestration |
| `tui/views/studio.rs` | 3,444 | Studio TUI view |
| `binding/resolve.rs` | 2,434 | Binding resolution |
| `ast/raw/parser.rs` | 1,909 | YAML parser |

### Patterns from Large Rust Projects

**Pattern A: Vertical splitting by responsibility (rustc, cargo)**

rustc splits large structs by having the struct definition in one file and `impl` blocks in separate files within the same module:

```
runtime/
  runner/
    mod.rs         -- struct Runner + core public API
    orchestrate.rs -- impl Runner { fn orchestrate() ... }
    cancel.rs      -- impl Runner { fn cancel_tasks() ... }
    metrics.rs     -- impl Runner { fn collect_metrics() ... }
```

This works because Rust allows `impl` blocks for a type in any file within the same crate. The key rule: **the struct definition and its fields stay in `mod.rs`**, method implementations are split by concern.

**Pattern B: Helper structs extraction (ripgrep)**

ripgrep's `grep-searcher` crate demonstrates extracting internal state machines into separate types:

```rust
// Instead of one massive Searcher with 40 methods:
pub struct Searcher { config: Config, state: SearchState }

// SearchState is its own type in its own file:
struct SearchState { /* ... */ }
impl SearchState { /* all state-manipulation methods */ }
```

The outer type becomes a thin orchestrator that delegates to specialized inner types.

**Pattern C: Private submodules with re-export (rust-analyzer)**

rust-analyzer's `hir-def` crate uses private submodules that are re-exported from `mod.rs`:

```rust
// mod.rs
mod body;      // private
mod lower;     // private
mod resolver;  // private

pub use body::Body;
pub use lower::lower;
```

Users of the module see a flat API, but the implementation is split across files.

### Recommended Approach for Nika

For `runner.rs` (3,548 lines), the vertical split is most natural:

```
runtime/
  runner/
    mod.rs              -- struct Runner, new(), run() entry point
    dag_walker.rs       -- DAG traversal and task scheduling
    task_dispatch.rs    -- Individual task execution dispatch
    cancellation.rs     -- fail_fast, abort, DependencyFailed logic
    for_each.rs         -- for_each parallel expansion
```

For `studio.rs` (3,444 lines), extract widget rendering:

```
tui/views/studio/
    mod.rs              -- StudioView struct, handle_event()
    browser_panel.rs    -- File browser rendering
    editor_panel.rs     -- YAML editor rendering
    dag_panel.rs        -- DAG preview rendering
    keybindings.rs      -- Keyboard shortcut handling
```

### Rules for Splitting

1. **Never split a struct definition across files.** Fields and `new()` stay together.
2. **Split `impl` blocks by concern.** Group related methods.
3. **Use `pub(super)` for internal APIs** that sibling files need but external callers should not see.
4. **Re-export the main type from `mod.rs`** so external imports don't change.
5. **Aim for 500-800 lines per file** as a soft target.

---

## 3. Eliminating Legacy Bridge/Adapter Layers

### The Problem in Nika

Nika has a three-layer type system:

```
RawWorkflow  -->  AnalyzedWorkflow  -->  lower()  -->  Workflow (legacy)
   (Phase 1)        (Phase 2)           (bridge)       (used by runtime)
```

The `lower()` function in `src/ast/lower.rs` converts `AnalyzedWorkflow` back into the legacy `Workflow` type, which is essentially the old serde-deserialized type from before the two-phase AST was introduced. This means:
- `TaskId(u32)` is converted back to `String`
- `IndexMap` is converted to `FxHashMap`
- Typed enums like `HttpMethod` are converted to strings
- Information is lost (spans, descriptions, structured enums)

### How Compilers Handle This

**rustc: Explicit lowering passes with distinct types**

rustc has: `AST -> HIR -> THIR -> MIR -> codegen IR`. Each lowering is a distinct pass. The key principle: **each IR is designed for its consumer, not its producer.** HIR is designed for type checking, MIR for borrow checking and optimization. They never share types.

But crucially, **rustc does not have a "legacy" layer.** When they introduced THIR (Typed HIR) between HIR and MIR, they designed THIR from scratch for its purpose and then migrated MIR construction to consume THIR instead of HIR. The old code path was removed.

**rust-analyzer: `hir-def` -> `hir-ty` with clear ownership**

rust-analyzer has a clean IR pipeline where each layer owns its types. When they refactored from the old `ra_hir` monolith, they did it in phases:
1. Introduce the new type alongside the old one
2. Add conversion (similar to Nika's `lower()`)
3. Gradually migrate consumers to use the new type directly
4. Remove the conversion and the old type

**The Strangler Fig Pattern (general software architecture)**

This is the canonical approach: wrap the old system with a new interface, gradually move consumers to the new interface, then remove the old one. In Rust terms:

```
Phase 1: Runtime uses Workflow (legacy)
Phase 2: Runtime uses Workflow, but Workflow is now just a projection of AnalyzedWorkflow
Phase 3: Runtime uses AnalyzedWorkflow directly, lower() removed
Phase 4: Legacy Workflow type deleted
```

### Recommended Migration for Nika

The `lower()` function is doing destructive work -- it is throwing away information (spans, typed enums, descriptions) and converting back to stringly-typed representations. The runtime should consume `AnalyzedWorkflow` directly.

**Step-by-step elimination:**

1. **Audit runtime imports.** Currently the runtime imports from `crate::ast::{Workflow, Task, TaskAction, ...}`. Map every usage.

2. **Create type aliases as a bridge:**
```rust
// In runtime/mod.rs, temporarily:
pub type RuntimeWorkflow = crate::ast::analyzed::AnalyzedWorkflow;
pub type RuntimeTask = crate::ast::analyzed::AnalyzedTask;
```

3. **Migrate runner.rs first.** It is the top-level consumer. Change `Runner::new()` to accept `AnalyzedWorkflow`. Internally, it can still call helper functions that convert lazily.

4. **Migrate executor.rs next.** Task dispatch reads action type (Infer/Exec/Fetch/Invoke/Agent). The `AnalyzedTaskAction` enum is actually better for this because it is typed, not stringly.

5. **Keep `lower()` as an escape hatch** during migration: any code that still needs the old type can call it locally, but the entry point accepts the new type.

6. **Delete the legacy types** once all consumers are migrated.

### What to Keep from `lower()`

Some transformations in `lower()` are genuinely useful:
- Default provider to "claude" when None
- Converting `for_each` items string to `serde_json::Value`
- Building the flows list from task dependencies

These should become methods on `AnalyzedWorkflow` or helper functions in the runtime, not a separate lowering pass.

---

## 4. Unifying Duplicate Enums

### The Problem in Nika

Two `TaskStatus` enums exist:

**`store::TaskStatus`** (runtime/data):
```rust
pub enum TaskStatus {
    Success,
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
}
```

**`tui::theme::TaskStatus`** (display):
```rust
pub enum TaskStatus {
    Queued, Pending, Running,
    Success, Failed, Paused, Skipped,
}
```

These represent different things: the store version is a terminal execution result (what happened), while the TUI version is a visual state (what to display). But they share the name and overlap semantically.

### Patterns from the Ecosystem

**Pattern A: Single canonical enum + conversion (cargo)**

Cargo has a single `PackageStatus` enum and conversion methods for different display contexts. The principle: **one source of truth, multiple views.**

```rust
// One canonical enum in a shared location
pub enum TaskStatus {
    Queued,
    Pending,
    Running,
    Success,
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
    Paused,
}

// TUI gets styling via a trait or method
impl TaskStatus {
    pub fn display_category(&self) -> StatusCategory {
        match self {
            Self::Queued => StatusCategory::Inactive,
            Self::Pending | Self::Running => StatusCategory::Active,
            Self::Success => StatusCategory::Complete,
            Self::Failed(_) | Self::DependencyFailed { .. } => StatusCategory::Error,
            Self::Skipped { .. } => StatusCategory::Inactive,
            Self::Paused => StatusCategory::Suspended,
        }
    }
}
```

**Pattern B: Domain-specific enums with From impls (rust-analyzer)**

rust-analyzer often has domain-specific types that convert between each other:

```rust
// In the runtime/store module:
pub enum ExecutionResult { Success, Failed(String), ... }

// In the TUI module:
pub enum DisplayStatus { Queued, Pending, Running, ... }

impl From<&ExecutionResult> for DisplayStatus {
    fn from(result: &ExecutionResult) -> Self {
        match result {
            ExecutionResult::Success => DisplayStatus::Success,
            ExecutionResult::Failed(_) => DisplayStatus::Failed,
            // ...
        }
    }
}
```

**Pattern C: Trait-based abstraction (servo)**

Define behavior through traits rather than shared types:

```rust
trait Styleable {
    fn status_color(&self, theme: &Theme) -> Color;
    fn status_icon(&self) -> &'static str;
    fn status_label(&self) -> &str;
}
```

### Recommended Approach for Nika

These are genuinely different domains. The store tracks **execution results** (terminal states). The TUI tracks **visual states** (includes non-terminal states like Queued, Running, Paused).

**Use Pattern B with clear naming:**

1. Keep `store::TaskStatus` but rename to `store::TaskResult` or `store::ExecutionOutcome` (it only represents terminal states).

2. Keep `tui::theme::TaskStatus` but rename to `tui::DisplayStatus` or `tui::TaskPhase` (it represents the full lifecycle).

3. Add `From` conversion:
```rust
impl From<&store::ExecutionOutcome> for tui::TaskPhase {
    fn from(outcome: &store::ExecutionOutcome) -> Self { ... }
}
```

4. The TUI state already has its own `TaskPhase` that includes Queued/Running/Paused -- these are states the store never sees because they are transient.

**Do NOT merge them into one enum.** They serve different purposes and forcing them together would mean the store has to handle visual-only states or the TUI has to handle error strings it does not need.

---

## 5. Binding/Wiring Systems in DAG Executors

### The Problem in Nika

Nika's binding system lets tasks reference outputs from other tasks via `use:` blocks and `{{use.alias}}` templates. The `binding/resolve.rs` file is 2,434 lines handling this complexity.

### How Rust Workflow Engines Handle Data Flow

**Pattern A: Typed channels (tokio-based executors)**

Several Rust DAG executors use typed channels for task-to-task data flow:

```rust
struct TaskNode {
    inputs: Vec<Receiver<Value>>,
    outputs: Vec<Sender<Value>>,
    execute: Box<dyn Fn(Vec<Value>) -> Value>,
}
```

The DAG executor wires channels during setup, then tasks consume inputs and produce outputs through the channel system. This is clean but requires knowing the topology at setup time.

**Pattern B: Shared store with publish/subscribe (petgraph-based)**

A common pattern in Rust workflow engines built on petgraph:

```rust
struct RunContext {
    results: DashMap<TaskId, Arc<Value>>,
    waiters: DashMap<TaskId, Vec<oneshot::Sender<()>>>,
}

impl RunContext {
    async fn wait_for(&self, task_id: TaskId) -> Arc<Value> {
        if let Some(val) = self.results.get(&task_id) {
            return val.clone();
        }
        let (tx, rx) = oneshot::channel();
        self.waiters.entry(task_id).or_default().push(tx);
        rx.await.unwrap();
        self.results.get(&task_id).unwrap().clone()
    }

    fn publish(&self, task_id: TaskId, value: Value) {
        self.results.insert(task_id, Arc::new(value));
        if let Some((_, waiters)) = self.waiters.remove(&task_id) {
            for tx in waiters { let _ = tx.send(()); }
        }
    }
}
```

**Pattern C: Binding resolution as a separate compile phase (dbt, Airflow ports)**

Some workflow engines resolve bindings during a "compilation" phase before execution:

```rust
struct ResolvedBinding {
    source_task: TaskId,
    source_path: JsonPath,    // e.g., ".output.title"
    target_task: TaskId,
    target_slot: String,      // e.g., "use.title"
}

fn resolve_bindings(workflow: &Workflow) -> Vec<ResolvedBinding> {
    // Static analysis: resolve all {{use.X}} references
    // before any task executes
}
```

This approach separates the concern of "what connects to what" from "how do I get the value at runtime."

### Recommended Approach for Nika

Nika already has a good RunContext pattern. The 2,434-line `resolve.rs` is large because it handles multiple concerns:

1. **Template parsing** (`{{use.alias}}` extraction)
2. **Path resolution** (jsonpath navigation through nested Values)
3. **Lazy binding** (deferred resolution)
4. **Context references** (`{{context.files.X}}`)
5. **Input references** (`{{inputs.X}}`)

**Split by concern:**

```
binding/
  mod.rs          -- re-exports, BindingSpec type
  template.rs     -- {{use.X}} parsing and interpolation (stateless)
  resolver.rs     -- runtime resolution against RunContext (stateful)
  lazy.rs         -- LazyBinding enum and deferred resolution
  jsonpath.rs     -- already exists, keep as-is
```

**Use the compile-phase pattern for static validation:**

During the analyzer phase (Phase 2), resolve all `use:` references to `TaskId` and validate that referenced tasks exist. This catches errors early without waiting for runtime.

```rust
struct AnalyzedBinding {
    source: TaskId,           // Already interned
    path: Option<JsonPath>,   // Pre-parsed path
    alias: String,            // Local name in the use: block
    lazy: bool,
}
```

At runtime, the resolver only needs to:
1. Look up the TaskId in the RunContext (O(1) with DashMap)
2. Navigate the path if present
3. Return the value

This moves complexity from runtime to analysis time, where errors are easier to report with source locations.

---

## Cross-Cutting Recommendation: The "Context Object" Pattern

Several of the above patterns converge on a common solution: **a shared context object that carries interning tables, configuration, and shared state through the execution pipeline.**

rustc has `TyCtxt`, rust-analyzer has `db: &dyn DefDatabase`, salsa has `&dyn salsa::Database`. Nika could benefit from a similar pattern:

```rust
/// Shared execution context, created once per workflow run.
///
/// Carries interning tables and shared configuration.
/// Passed by reference to all runtime functions.
pub struct NikaCtx {
    /// Task name interning table
    pub tasks: TaskTable,
    /// MCP server name table
    pub mcp_servers: StringTable,
    /// Workflow-level configuration
    pub config: WorkflowConfig,
    /// RunContext for task results
    pub store: RunContext,
}
```

This eliminates the need for `lower()` entirely -- the runtime functions receive `&NikaCtx` alongside `AnalyzedWorkflow` and can resolve any `TaskId` to its name on demand.

---

## Sources and References

1. **rustc Developer Guide** -- [Compiler Architecture](https://rustc-dev-guide.rust-lang.org/overview.html) -- IR layering, interning, query system
2. **rust-analyzer Architecture** -- [Architecture.md](https://github.com/rust-lang/rust-analyzer/blob/master/docs/dev/architecture.md) -- la-arena, salsa integration, module splitting
3. **la-arena crate** -- [docs.rs/la-arena](https://docs.rs/la-arena) -- typed index arenas, used by rust-analyzer
4. **salsa** -- [salsa-rs/salsa](https://github.com/salsa-rs/salsa) -- incremental computation with interned values
5. **ripgrep architecture** -- [ARCHITECTURE.md](https://github.com/BurntSushi/ripgrep/blob/master/ARCHITECTURE.md) -- crate splitting, helper struct extraction
6. **cargo source** -- [github.com/rust-lang/cargo](https://github.com/rust-lang/cargo) -- single canonical enums, config passing
7. **Strangler Fig Pattern** -- Martin Fowler's pattern for incremental migration of legacy systems
8. **"Splitting large Rust modules"** -- Rust community consensus on `impl` blocks across files
9. **petgraph** -- [docs.rs/petgraph](https://docs.rs/petgraph) -- graph-based DAG patterns in Rust
10. **dashmap** -- [docs.rs/dashmap](https://docs.rs/dashmap) -- concurrent map patterns for shared state

## Confidence Level

**High** -- The patterns described are well-established in production Rust codebases. The recommendations are directly applicable to Nika's current architecture based on the analysis of `lower.rs`, `ids.rs`, `runner.rs`, `resolve.rs`, and the dual `TaskStatus` enums.

## Applicability to Nika

| Topic | Effort | Impact | Priority |
|-------|--------|--------|----------|
| 1. TaskId through runtime | Medium | High (eliminates string comparisons) | P1 |
| 2. Split large files | Low | Medium (readability, PR reviews) | P2 |
| 3. Eliminate lower() | High | High (removes 800 lines, preserves info) | P1 |
| 4. Unify TaskStatus | Low | Low (rename + From impl) | P3 |
| 5. Split binding/resolve | Medium | Medium (maintainability) | P2 |

Topics 1 and 3 are tightly coupled -- eliminating `lower()` naturally requires flowing `TaskId` into the runtime. They should be tackled together.
