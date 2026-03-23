# Research Report: Dynamic Task Expansion in DAG/Workflow Engines

**Date:** 2026-03-16
**Context:** Nika v0.27.0 `for_each` / `decompose` feature design
**Scope:** Best practices for expanding one task definition into N instances at runtime

---

## Summary

Dynamic task expansion (fan-out) is a solved problem across major workflow engines, with a
clear consensus: **validate the template shape at definition time, expand instances at runtime,
and aggregate results for downstream consumers**. The key design tension is between static
DAG integrity (all nodes known upfront) and runtime dynamism (N determined by data). Every
major engine resolves this with a two-phase approach and lazy reference proxies.

---

## 1. How Major Workflow Engines Handle Dynamic Expansion

### 1.1 Apache Airflow -- Dynamic Task Mapping

**Mechanism:** `task.expand(arg=upstream_output)` or `task.expand(arg=[1,2,3])`

**Two-phase validation:**
- **Parse time (definition):** DAG structure validated. Mapped tasks exist as single template
  nodes. Dependencies validated against template task IDs. XComArg acts as lazy placeholder
  -- no resolution of actual values.
- **Runtime (expansion):** Scheduler determines `mapped_length` from upstream XCom value
  length (e.g., `len([1, 2, 3]) = 3`). Creates TaskInstances: `task_id__0`, `task_id__1`,
  `task_id__2`. Capped by `max_map_length` (default 1024).

**Downstream dependency resolution:**
- A regular task depending on a mapped task via `>>` receives **all mapped outputs as an
  aggregate list** via XComArg lazy resolution.
- The downstream does NOT need to know the map cardinality at parse time.
- Trigger rules (ALL_SUCCESS, ALL_DONE) apply per-instance independently.

**Key insight:** The template task ID serves as a **virtual node** in the DAG. At parse time,
it is a single node with edges. At runtime, it expands but the downstream reference resolves
to the aggregate.

### 1.2 Prefect -- `map()`

**Mechanism:** `task.map(iterable)` at flow runtime.

- Definition-time: Flow topology and task signatures validated. `map()` treated as partial
  expansion operator without known input length.
- Runtime: Mapped task receives iterable, creates parallel task runs. Downstream dependencies
  preserved through **result proxies** (lazy evaluation).
- Aggregation: Downstream gets a list of futures/results.

### 1.3 Dagster -- DynamicOut

**Mechanism:** Ops produce `DynamicOutput` instances with runtime-determined count.

- Definition-time: Op signatures and graph structure validated. `DynamicOut()` placeholder
  in op definition.
- Runtime: Op emits N `DynamicOutput` instances. `.map()` fans out to downstream ops.
  `.collect()` gathers results as synchronization barrier.
- Type safety: Typed expectations on fan-out ensure downstream receives correct shape.

### 1.4 Temporal

**Mechanism:** No static DAG. Event-sourced workflow execution.

- Workflow code validated for determinism at registration.
- Dynamic fan-out via activity loops or child workflows.
- Referential integrity via workflow history replay and promises.
- Not directly comparable (imperative, not declarative DAG).

### 1.5 Argo Workflows

**Mechanism:** `withItems` / `withParam` in DAG task specs.

```yaml
dag:
  tasks:
    - name: mapper
      withItems: ["a", "b", "c"]
      template: process-item
    - name: aggregator
      depends: mapper  # Waits for ALL mapper instances
      template: collect-results
```

- Expanded tasks get UUID-suffixed IDs: `mapper-abc123`, `mapper-def456`.
- Downstream `depends: "mapper"` resolves to the **group** (all instances must complete).
- Two-phase: Template shape validated at submission; instances created at execution.

### 1.6 Nextflow

**Mechanism:** Channel-based reactive dataflow.

- Fan-out: `channel.fromList(items)` emits parallel items.
- Fan-in: `.collect()` gathers all emissions into a single batch, blocking until complete.
- No static DAG -- reactive pipeline with backpressure.

---

## 2. Consensus Patterns

### 2.1 The Universal Two-Phase Validation Pattern

```
Phase 1: Definition Time (static)          Phase 2: Runtime (dynamic)
+----------------------------------+       +----------------------------------+
| - Validate template task shape   |       | - Determine N from data          |
| - Check deps reference valid IDs |  -->  | - Create N instances             |
| - Verify no cycles in template   |       | - Re-validate expanded DAG       |
| - Type-check binding signatures  |       | - Execute with concurrency limit |
+----------------------------------+       +----------------------------------+
```

Every engine follows this. Definition time catches structural errors (missing deps, cycles,
type mismatches). Runtime handles cardinality and data-dependent validation.

### 2.2 The Virtual Node Pattern

All engines treat a for_each/mapped task as a **single virtual node** in the static DAG:

```
Static DAG:     fetch_data --> [process_items] --> aggregate
                               (virtual node)

Runtime DAG:    fetch_data --> process_items[0] --> aggregate
                           --> process_items[1] -->
                           --> process_items[2] -->
```

The virtual node:
- Has a single task ID for dependency references
- Participates in cycle detection as one node
- Expands transparently at runtime
- Downstream tasks reference the virtual node, not instances

### 2.3 Aggregation Semantics

**Consensus: downstream gets an array of all results by default.**

| Engine   | Downstream receives           | Override available?              |
|----------|-------------------------------|----------------------------------|
| Airflow  | XComArg (lazy list)           | Yes, can index individual maps   |
| Dagster  | `.collect()` list             | Yes, `.map()` for 1:1 fan-out   |
| Argo     | All results via `outputs`     | No, always waits for all         |
| Nextflow | `.collect()` channel batch    | Yes, `.first()`, `.take(N)`     |

### 2.4 Task ID Namespacing for Expanded Instances

| Engine   | Convention                    | Example                          |
|----------|-------------------------------|----------------------------------|
| Airflow  | `task_id__<map_index>`        | `process_items__0`               |
| Argo     | `task_id-<uuid>`              | `mapper-abc123`                  |
| Dagster  | Scoped sub-graph names        | `process_items.0`                |
| Nextflow | Channel emission index        | Implicit, no named IDs           |

---

## 3. Rust-Specific Patterns

### 3.1 petgraph / daggy for Dynamic Node Insertion

**petgraph `StableGraph`** supports O(1) node/edge insertion:
- `add_node(weight)` -- returns `NodeIndex`, O(1)
- `add_edge(a, b, weight)` -- returns `EdgeIndex`, O(1)
- `StableGraph` preserves indices on removal (unlike `Graph`)

**daggy** wraps petgraph with DAG enforcement:
- `add_edge(a, b, weight)` returns `Err(WouldCycle<E>)` if cycle detected
- `add_parent` / `add_child` for atomic node+edge insertion
- Cycle check via `is_cyclic_directed` on each insertion

**Nika's current approach** (Vec-indexed `IndexedDag` with Kahn's algorithm) is already
well-suited. For dynamic expansion, two strategies:

#### Strategy A: Rebuild on Expansion (Simpler, Nika-friendly)

```
1. Static DAG validated at analysis time (Phase 2)
2. At runtime, when for_each task is reached:
   a. Expand items
   b. Create N synthetic task entries
   c. Do NOT modify the DAG -- handle expansion in the runner
   d. Store results indexed (task_id[0], task_id[1], ...)
   e. Aggregate into task_id result for downstream consumption
```

This is what Nika currently does and matches the Airflow/Argo pattern.

#### Strategy B: Incremental DAG Mutation (Complex, not recommended)

Using Pearce-Kelly algorithm for incremental topological sort:
- O(|K|) per edge insertion where K is the affected region
- Practical O(m) total for sparse graphs
- Requires mutable DAG + synchronization in concurrent execution

**Recommendation: Strategy A.** The DAG should remain immutable after construction
(matching Nika's architectural decision #2). Expansion is handled by the runner
as a loop within a single DAG node's execution.

### 3.2 Type-Safe Template vs Concrete Task Distinction

#### Approach 1: Enum Variant (Pragmatic, fits Nika)

```rust
enum TaskKind {
    /// Regular task -- executes once
    Concrete(TaskAction),
    /// Template task -- expands to N instances at runtime
    Template {
        action: TaskAction,
        for_each: ForEachSpec,
    },
}

// At runtime, template produces:
struct ExpandedInstance {
    parent_id: TaskId,
    index: usize,
    item: Value,
    action: TaskAction,  // cloned from template
}
```

**Pros:** Zero overhead, matches Nika's existing `AnalyzedTask.for_each: Option<ForEach>`.
**Cons:** Not compile-time enforced; relies on runtime checks.

#### Approach 2: Type State Pattern (Maximum safety)

```rust
struct Task<S: TaskState> {
    id: TaskId,
    action: TaskAction,
    _state: PhantomData<S>,
}

trait TaskState {}
struct Template;
struct Expanded;
impl TaskState for Template {}
impl TaskState for Expanded {}

impl Task<Template> {
    fn expand(self, items: Vec<Value>) -> Vec<Task<Expanded>> { ... }
}

impl Task<Expanded> {
    fn execute(&self, executor: &TaskExecutor) -> TaskResult { ... }
}
// Compile error: Task<Template> has no execute() method
```

**Pros:** Compile-time guarantee that templates cannot be executed directly.
**Cons:** Requires two different task types flowing through the system; adds complexity
to the DAG which stores homogeneous nodes. Difficult to retrofit.

#### Approach 3: Sealed Trait with Marker (Middle ground)

```rust
mod sealed {
    pub trait Executable {}
}

struct ConcreteTask { ... }
struct TemplateTask { ... }

impl sealed::Executable for ConcreteTask {}
// TemplateTask intentionally does NOT implement Executable

fn execute(task: &impl sealed::Executable) { ... }
```

**Recommendation for Nika:** Approach 1 (enum variant). Nika already uses
`for_each: Option<AnalyzedForEach>` which is effectively the same pattern. The
type state approach (Approach 2) would require significant refactoring for marginal
benefit since the expansion happens in a single controlled location (the runner).

### 3.3 Compile-Time vs Runtime DAG Validation

| Check                          | When           | Why                                    |
|--------------------------------|----------------|----------------------------------------|
| Task ID uniqueness             | Analysis (P2)  | Static property of YAML                |
| Cycle detection                | Analysis (P2)  | Template DAG shape is fixed            |
| `with:` binding references     | Analysis (P2)  | Source task must exist and be upstream  |
| `depends_on` target exists     | Analysis (P2)  | Static reference check                 |
| `for_each` items is valid expr | Analysis (P2)  | Syntax check, not value check          |
| for_each item count > 0        | Runtime        | Depends on upstream data               |
| Expanded instance bindings     | Runtime        | Each instance gets its own `{{with.item}}`  |
| Concurrency limit enforcement  | Runtime        | Semaphore at execution time            |
| fail_fast propagation          | Runtime        | Depends on instance execution results  |

---

## 4. Avoiding False Positives in Dependency Checking

### Problem

When task B has `for_each` and task C `depends_on: [B]`, the validator must not reject
this because B "doesn't exist yet" (it will expand at runtime).

### Solution (What Nika Should Do)

1. **Treat the template task ID as canonical.** `B` exists in the DAG as a single node.
   The fact that it expands to `B[0]`, `B[1]`, `B[2]` at runtime is invisible to the
   static validator.

2. **Validate `with:` bindings against the template ID.** If C has `with: data: B`,
   the validator checks that `B` exists and is upstream -- which it is.

3. **At runtime, the aggregate result of B is stored under the template ID.** When C
   resolves `{{with.data}}`, it gets the aggregated array `[B[0].result, B[1].result, ...]`.

4. **Never expose instance IDs to downstream tasks.** The instance IDs (`B[0]`, `B[1]`)
   are internal bookkeeping. External references always use the template ID.

This is exactly what Nika already does with `IterationResult.for_each_info` and the
aggregation in the runner. The current approach is correct.

### Edge Case: Downstream for_each Depending on Upstream for_each

```yaml
tasks:
  - id: fetch_urls
    for_each: ["https://a.com", "https://b.com"]
    fetch: { url: "{{with.item}}" }

  - id: process_results
    with:
      pages: fetch_urls
    for_each: "{{with.pages}}"  # Fan-out on aggregated results
    infer: "Summarize: {{with.item}}"
    depends_on: [fetch_urls]
```

This creates a chain: `fetch_urls` expands to N, aggregates to array, then
`process_results` expands to N again on the aggregated array. Both DAG nodes
exist as single virtual nodes in the static DAG. The expansion is purely runtime.

---

## 5. Practical Recommendations for Nika

### 5.1 Current State Assessment

Nika's current implementation is **architecturally sound** and aligns with industry patterns:

- `IndexedDag` is immutable after construction (matches Strategy A).
- `for_each` expansion happens in the runner, not in the DAG.
- Aggregation stores results under the parent task ID.
- `IterationResult.for_each_info` tracks `(parent_id, index)` for ordered aggregation.
- `decompose` adds MCP-driven dynamic expansion (semantic strategy).

### 5.2 Suggested Improvements

#### 5.2.1 Explicit Aggregation Policy

Currently, results are always collected into an array. Consider making this configurable:

```yaml
tasks:
  - id: process
    for_each: [1, 2, 3]
    infer: "..."
    aggregate: array     # default: collect all results into array
    # aggregate: first   # take first successful result
    # aggregate: last    # take last result
    # aggregate: concat  # concatenate string results
```

#### 5.2.2 for_each Cardinality Validation

Add a static check in the analyzer (Phase 2) when for_each items are a literal array:

```rust
// In analyzer Phase 2:
if let Some(for_each) = &task.for_each {
    if for_each.is_array() {
        let items = for_each.parse_items()?;
        if items.is_empty() {
            warn!("for_each with empty literal array on task '{}'", task.name);
        }
        if items.len() > MAX_FOR_EACH_ITEMS {
            return Err(NikaError::ForEachTooManyItems { ... });
        }
    }
    // Dynamic items (bindings) validated at runtime only
}
```

#### 5.2.3 Instance ID Namespacing

Formalize the instance ID convention for tracing and debugging:

```
Convention: {task_id}[{index}]
Examples:   process[0], process[1], process[2]
Storage:    {task_id}  (aggregate result under template ID)
Trace:      task_id=process, for_each_index=0 (in NDJSON events)
```

This matches Airflow's `task_id__N` and Dagster's `task_id.N` conventions.

#### 5.2.4 Concurrency Guard Type

```rust
/// Concurrency-limited for_each execution
struct ForEachGuard {
    semaphore: Arc<Semaphore>,
    fail_fast: Arc<AtomicBool>,
}

impl ForEachGuard {
    fn new(concurrency: Option<u32>, fail_fast: bool) -> Self {
        let permits = concurrency.unwrap_or(u32::MAX);
        Self {
            semaphore: Arc::new(Semaphore::new(permits as usize)),
            fail_fast: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn acquire(&self) -> Result<SemaphorePermit, NikaError> {
        if self.fail_fast.load(Ordering::Relaxed) {
            return Err(NikaError::ForEachFailFast);
        }
        self.semaphore.acquire().await.map_err(|_| NikaError::ForEachCancelled)
    }
}
```

---

## 6. Anti-Patterns to Avoid

1. **Mutating the DAG at runtime.** Never insert new nodes into `IndexedDag` during
   execution. The DAG is a compile-time artifact; expansion is a runtime concern.

2. **Exposing instance IDs to downstream with: bindings.** Downstream tasks should
   reference the template task ID and receive the aggregate. If a downstream needs
   a specific instance, use JSONPath on the aggregate.

3. **Unbounded expansion.** Always enforce `max_items` or `max_map_length` to prevent
   OOM from accidentally iterating over a huge dataset.

4. **Validating expanded instances against the static DAG.** The static DAG only knows
   about template nodes. Instance-level validation (if needed) happens in the runner.

5. **Blocking the executor on expansion.** Expansion (especially `decompose` with MCP
   calls) should be async and not block the DAG scheduler. Nika already handles this
   correctly with `async expand_decompose`.

---

## Sources

1. Apache Airflow Dynamic Task Mapping documentation
2. Prefect 2.x `map()` API and result proxies
3. Dagster `DynamicOutput` and `.map()/.collect()` patterns
4. Argo Workflows `withItems`/`withParam` DAG templates
5. Temporal SDK activity patterns
6. Nextflow channel operators and `.collect()`
7. petgraph `StableGraph` API (docs.rs/petgraph)
8. daggy crate -- DAG enforcement over petgraph (docs.rs/daggy)
9. Pearce-Kelly algorithm for online topological ordering (2007)
10. Marchetti-Spaccamela incremental topological sort (2008, ACM)
11. GitHub Actions matrix strategy documentation

## Methodology

- Tools: Perplexity AI (sonar-pro) for web research, source code analysis of Nika codebase
- Pages analyzed: 15+ primary sources across workflow engine docs
- Nika files examined: `dag/indexed.rs`, `dag/validate.rs`, `runtime/runner.rs`,
  `runtime/executor/decompose.rs`, `ast/analyzed/task.rs`, `ast/lower.rs`

## Confidence Level

**High** -- The patterns described are well-established across 5+ production workflow
engines with consistent design decisions. The recommendations for Nika are directly
informed by analyzing the current codebase and confirming it already follows best practices.

## Further Research Suggestions

- Investigate **partial DAG resumption** -- when a for_each task partially fails, how to
  re-run only failed instances without re-expanding the entire for_each.
- Research **streaming for_each** -- processing items as they arrive rather than waiting
  for the full list (relevant for `decompose: nested` BFS traversal).
- Explore **for_each + structured output** interaction -- should each instance validate
  against the schema independently, or should the aggregate be validated?
- Consider **cross-task for_each zipping** -- when two upstream for_each tasks produce
  arrays of equal length, can a downstream iterate over paired elements?
