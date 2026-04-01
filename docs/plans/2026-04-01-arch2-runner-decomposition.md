# ARCH-2: runner.rs run() Decomposition

> Decompose the 1,868-line `Runner::run()` function into 6 extracted modules.
> Post-launch refactor. Estimated: 5-7 days across 7 phases, 8 commits.

## Current State (audited 2026-04-01)

| Metric | Value |
|--------|-------|
| File | `tools/nika-engine/src/runtime/runner.rs` — 7,961 lines |
| `run()` | Lines 1464-3330 — **1,868 lines** |
| Largest helper | `execute_task_iteration` — 447 lines (already extracted) |
| For_each resolution | 602 lines, 5 format branches, **~75% duplication confirmed** |
| Test coverage | ~120 tests (sync unit + async integration) |
| `.await` points in run() | 7 (context load, skills read, decompose timeout, 2x execute_task_iteration spawn, select!, MCP shutdown) |

## Section Map (precise line ranges)

```
run() — 1,868 lines (1464-3330)
│
├── A: Pre-check + orchestrator emit (1464-1491, 28 lines)
│   Cancel guard, OrchestratorStarted event
│
├── B: Initialization (1492-1702, 211 lines)
│   base_path, lockfile, context/inputs/agents/skills load,
│   WorkflowStarted, DAG layer cache, renderer setup
│
├── C: Loop control (1708-1765, 58 lines)
│   Cancel check, pause/resume select!, renderer take/put
│
├── D: Deadlock/completion (1770-1858, 89 lines)
│   ready.is_empty(): all_done, deadlock, dep-chain failure
│
├── E: Task dispatch (1861-2896, 1,036 lines) ← THE MONSTER
│   ├── Decompose expansion (1890-1982, 93 lines)
│   ├── For_each resolution (1983-2588, 606 lines) ← DUPLICATION
│   │   ├── Format 2 pipe: $x | transform (2023-2254, 232 lines)
│   │   ├── Format 2 plain: $alias.path (2256-2408, 153 lines) ← DUP A
│   │   ├── Format 3: {{inputs.x}} (2410-2461, 52 lines)
│   │   ├── Format 5: {{with.alias.path}} (2462-2576, 115 lines) ← DUP B
│   │   └── Inline array (2580-2588, 9 lines)
│   ├── For_each spawning (2591-2854, 264 lines)
│   └── Regular task spawning (2855-2896, 42 lines)
│
├── F: Result collection (2900-3057, 158 lines)
│   tokio::select! { cancel | timeout | join_next }
│
├── G: For_each aggregation (3059-3146, 88 lines)
│   Sort by index, merge media, emit ForEachCompleted
│
└── H: Completion (3148-3330, 183 lines)
    Media integrity, artifact manifest, final output,
    orchestrator completed, WorkflowCompleted, GC, records,
    trace, renderer summary, MCP shutdown
```

## Duplication Analysis: Format 2 vs Format 5

**Identical operations** (shared by both):
- `try_parse_json_str()` call for JSON auto-parse
- `let mut value_ref: &Value` + traversal_failed init
- Segment traversal loop (index-or-field dispatch, match Some/None)
- `value_to_array()` call + error path

**Differences:**

| Point | Format 2 (`$alias.path`) | Format 5 (`{{with.alias.path}}`) |
|-------|--------------------------|----------------------------------|
| Source lookup | `bindings.get_resolved()` + `datastore.get_output()` fallback | `bindings.get_resolved()` only |
| Traversal failure log | Silent (just emit) | `tracing::warn!` before emit |
| Error message | `"nested path segment '{}' not found"` | `"path traversal failed for '{{with.{}}}'"` |

**Conclusion:** ~75% duplication confirmed. A single `traverse_path()` function eliminates ~260 lines.

## Architecture Decision: `runtime/runner/` Submodule Directory

```
tools/nika-engine/src/runtime/runner/
├── mod.rs              ← Runner struct + builders + run() orchestration (~800 lines)
├── init.rs             ← InitContext + run_init() (~240 lines)
├── for_each_resolve.rs ← resolve_for_each_items() + traverse_path() (~280 lines)
├── task_dispatch.rs    ← dispatch_ready_tasks() (~450 lines)
├── result_collector.rs ← collect_batch_results() + BatchCollectOutcome (~190 lines)
├── aggregator.rs       ← aggregate_for_each_results() (~100 lines)
└── completion.rs       ← run_completion() (~195 lines)
```

**Why directory, not flat helpers?** The file is already 7,961 lines. Keeping everything in one file defeats the purpose. The `runtime/runner/` directory follows Rust idiom for modules that outgrow a single file.

**Zero API change:** `runtime/mod.rs` keeps `mod runner; pub use runner::Runner;` — Rust finds `runner/mod.rs` automatically.

## Extraction Catalog

### 1. `resolve_for_each_items` — **Pure free function** (no &self)

```rust
// for_each_resolve.rs

pub(crate) struct ForEachResolveError {
    pub message: String,
    pub nika_code: &'static str,
}

/// Unified for_each item resolution across all 5 formats.
pub(crate) fn resolve_for_each_items(
    items_str: &str,
    task_name: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<Option<Vec<Value>>, ForEachResolveError>

/// Shared path traversal — eliminates the Format 2/5 duplication.
fn traverse_path<'a>(
    root: &'a Value,
    segments: impl Iterator<Item = &'a str>,
    context: &str,
) -> Result<&'a Value, ForEachResolveError>
```

| Property | Value |
|----------|-------|
| Reads &self | **Nothing** — pure free function |
| Mutates | Nothing |
| Async | No |
| Lines replaced | ~602 in run() |
| Lines created | ~280 |
| Net savings | ~322 lines |

**Internal dispatch:**
```rust
fn resolve_for_each_items(...) -> ... {
    if items_str.starts_with('$') && (items_str.contains('|') || items_str.contains("??")) {
        resolve_pipe_transform(...)     // Format 2 pipe
    } else if let Some(alias) = items_str.strip_prefix('$') {
        resolve_dollar_binding(...)     // Format 2 plain + $inputs
    } else if items_str.contains("{{inputs.") {
        resolve_template_inputs(...)    // Format 3
    } else if items_str.contains("{{with.") {
        resolve_template_with(...)      // Format 5
    } else {
        Ok(None) // unrecognised — caller handles
    }
}
```

### 2. `InitContext` + `run_init` — **Method** (&mut self)

```rust
// init.rs

pub(crate) struct InitContext {
    pub base_path: PathBuf,
    pub cached_depths: Option<IndexMap<String, usize>>,
    pub _lockfile_guard: LockfileGuard,
}

impl Runner {
    async fn run_init(&mut self, workflow_start: Instant) -> Result<InitContext, NikaError>
}
```

| Property | Value |
|----------|-------|
| Reads &self | workflow, event_log, quiet, cli_renderer |
| Mutates &mut self | resolved_assets, executor (3x clone-replace), datastore |
| Async | Yes (context load, skills read) |
| Lines replaced | ~211 |

### 3. `collect_batch_results` — **Method** (&mut self) — Hardest extraction

```rust
// result_collector.rs

pub(crate) enum BatchCollectOutcome {
    Completed { for_each_results: IndexMap<Arc<str>, Vec<(usize, TaskResult)>> },
    Cancelled { phase: &'static str },
    TimedOut { duration_secs: u64, running_tasks: Vec<String> },
    Panicked { reason: String },
}

impl Runner {
    async fn collect_batch_results(
        &mut self,
        join_set: &mut JoinSet<IterationResult>,
        workflow_start: Instant,
        for_each_cancel_tokens: &FxHashMap<Arc<str>, CancellationToken>,
    ) -> BatchCollectOutcome
}
```

**Why it's hard:** The `tokio::select!` polls `self.cancel_token.cancelled()` (borrows &self) while the arm body does `self.datastore.insert()` + `self.cli_renderer` access (borrows &mut self). This works because **select! arms are mutually exclusive** — only one body executes per iteration.

**Pin issue:** `timeout_sleep` is `tokio::pin!`-ned. Must be created inside `collect_batch_results`, not passed in.

| Property | Value |
|----------|-------|
| Reads &self | cancel_token, workflow.max_duration_secs, event_log |
| Mutates &mut self | datastore (insert), cli_renderer (render_events) |
| Async | Yes (select!) |
| Lines replaced | ~158 |

### 4. `aggregate_for_each_results` — **Method** (&mut self)

```rust
// aggregator.rs

impl Runner {
    fn aggregate_for_each_results(
        &mut self,
        results: IndexMap<Arc<str>, Vec<(usize, TaskResult)>>,
        for_each_cancel_tokens: &FxHashMap<Arc<str>, CancellationToken>,
    )
}
```

| Property | Value |
|----------|-------|
| Reads &self | — |
| Mutates &mut self | datastore (insert), event_log (emit) |
| Async | No |
| Lines replaced | ~88 |

### 5. `run_completion` — **Method** (&mut self)

```rust
// completion.rs

impl Runner {
    async fn run_completion(
        &mut self,
        workflow_start: Instant,
        base_path: &Path,
        cached_depths: Option<&IndexMap<String, usize>>,
    ) -> Result<String, NikaError>
}
```

| Property | Value |
|----------|-------|
| Reads &self | workflow, event_log, quiet, datastore |
| Mutates &mut self | cli_renderer (summary), executor (shutdown_mcp) |
| Async | Yes (MCP shutdown) |
| Lines replaced | ~183 |

### 6. `dispatch_ready_tasks` — **Method** (&mut self)

```rust
// task_dispatch.rs

impl Runner {
    async fn dispatch_ready_tasks(
        &mut self,
        ready: Vec<&AnalyzedTask>,
        join_set: &mut JoinSet<IterationResult>,
        workflow_artifacts: Option<ArtifactsConfig>,
        artifact_base_path: PathBuf,
        workflow_base_url: Option<String>,
    ) -> FxHashMap<Arc<str>, CancellationToken>
}
```

| Property | Value |
|----------|-------|
| Reads &self | flow_graph, executor, event_log, cancel_token, global_task_semaphore |
| Mutates &mut self | datastore (on failure), event_log (emit) |
| Async | Yes (decompose expansion timeout) |
| Lines replaced | ~1,036 (after for_each extraction: ~430) |

### 7. `check_loop_termination` — **Method** (&self)

```rust
// stays in mod.rs (small enough)

impl Runner {
    fn check_loop_termination(&self, workflow_start: Instant) -> Result<bool, NikaError>
}
```

| Property | Value |
|----------|-------|
| Lines replaced | ~89 |

## Shared State Map

| Runner field | Used by | Pattern |
|--------------|---------|---------|
| `cancel_token` | run_init, dispatch, collect, check_termination | `.is_cancelled()`, `.cancelled()` |
| `event_log` | ALL extractions | `.emit()` — `Clone` + `Arc<Mutex>` internals |
| `workflow` | ALL extractions (read-only) | Task list, config, goal |
| `datastore` | ALL except for_each_resolve | `.insert()`, `.get()`, `.contains()` |
| `executor` | init (mutate), dispatch (clone for spawn) | Clone-replace in init, clone in dispatch |
| `cli_renderer` | init, collect, completion | `take()/put-back` pattern (preserved) |
| `flow_graph` | dispatch, check_termination | `.get_dependencies()` |
| `paused` / `resume_notify` | run() loop control only | Not extracted |
| `global_task_semaphore` | dispatch | `Arc::clone` for spawned tasks |
| `generation_id` | init, completion | Passed to events/trace |
| `quiet` | init, completion | Bool suppression check |

## Expected End State

| Metric | Before | After |
|--------|--------|-------|
| `run()` lines | 1,868 | **~220** |
| Total `runner/mod.rs` | 7,961 | **~5,500** (mod.rs) + 6 submodules |
| Extracted modules | 0 | 6 files |
| Duplication (Format 2/5) | ~260 lines | **0** |
| Independently testable units | 1 (`run()`) | **7** (run + 6 helpers) |

### run() After Extraction (pseudocode)

```rust
pub async fn run(&mut self) -> Result<String, NikaError> {
    let start = Instant::now();

    // A: Pre-check (~25 lines)
    if self.cancel_token.is_cancelled() { /* emit + trace + return */ }
    if let Some(ref goal) = self.workflow.goal { /* emit OrchestratorStarted */ }

    // B: Init (~5 lines)
    let ctx = self.run_init(start).await?;
    let _guard = ctx._lockfile_guard;

    let mut pending = (0..self.workflow.tasks.len()).collect::<Vec<_>>();

    loop {
        // C: Loop safety (~55 lines — cancel + pause inline)
        // D: Termination (~15 lines)
        let ready = self.get_ready_tasks(&mut pending);
        if ready.is_empty() {
            return self.check_loop_termination(start);
        }

        // E: Dispatch (~15 lines)
        let mut join_set = JoinSet::new();
        let tokens = self.dispatch_ready_tasks(ready, &mut join_set, ...).await;

        // F+G: Collect + aggregate (~15 lines)
        match self.collect_batch_results(&mut join_set, start, &tokens).await {
            Completed { results } => self.aggregate_for_each_results(results, &tokens),
            Cancelled { phase } => { self.write_trace(); return Err(...); }
            TimedOut { .. } => { self.write_trace(); return Err(...); }
            Panicked { reason } => { self.write_trace(); return Err(...); }
        }
    }

    // H: Completion (~5 lines)
    self.run_completion(start, &ctx.base_path, ctx.cached_depths.as_ref()).await
}
```

## Phased Execution Plan

### Phase 0 — Scaffold (30 min, 1 commit)

```
feat(runtime): scaffold runner/ submodule directory
```

- [ ] `git mv runner.rs runner/mod.rs`
- [ ] Create empty `init.rs`, `for_each_resolve.rs`, `task_dispatch.rs`, `result_collector.rs`, `aggregator.rs`, `completion.rs`
- [ ] Add `mod` declarations in `mod.rs`
- [ ] `cargo test --workspace --lib` — must pass (zero behavior change)

### Phase 1 — for_each_resolve (2 days, 1 commit) — Highest value

```
refactor(runtime): extract resolve_for_each_items from run()
```

- [ ] Implement `ForEachResolveError`, `traverse_path`, and 4 resolver functions
- [ ] Write 12+ unit tests covering all formats + error cases
- [ ] Replace 602-line block in `mod.rs` with calls to `resolve_for_each_items`
- [ ] Verify: both `$alias.path.nested` and `{{with.alias.path.nested}}` share `traverse_path`
- [ ] `cargo test --workspace --lib`

### Phase 2 — aggregator (0.5 day, 1 commit) — Safest

```
refactor(runtime): extract aggregate_for_each_results from run()
```

- [ ] Move lines 3059-3146 to `aggregator.rs`
- [ ] Unit tests: all-success, partial (fail_fast=false), all-fail, mixed+skipped
- [ ] `cargo test --workspace --lib`

### Phase 3 — completion (0.5 day, 1 commit)

```
refactor(runtime): extract run_completion from run()
```

- [ ] Move lines 3148-3330 to `completion.rs`
- [ ] No new tests needed — existing integration tests cover all completion paths
- [ ] `cargo test --workspace --lib`

### Phase 4 — result_collector (1.5 days, 1 commit) — Hardest

```
refactor(runtime): extract collect_batch_results with tokio::select!
```

- [ ] Implement `BatchCollectOutcome` enum
- [ ] `tokio::pin!` timeout_sleep INSIDE the method
- [ ] Careful with borrow checker: select! arms mutually exclusive → &mut self OK
- [ ] Tests: timeout path, cancellation path (use mock JoinSet)
- [ ] `cargo test --workspace --lib`

### Phase 5 — init (0.5 day, 1 commit)

```
refactor(runtime): extract run_init with InitContext
```

- [ ] `InitContext._lockfile_guard` must be `pub(super)` — RAII guard
- [ ] Move lines 1492-1702 to `init.rs`
- [ ] Preserve the 3x `self.executor = self.executor.clone().with_*()` pattern verbatim
- [ ] `cargo test --workspace --lib`

### Phase 6 — task_dispatch (1.5 days, 1 commit) — Largest

```
refactor(runtime): extract dispatch_ready_tasks from run()
```

- [ ] This calls `resolve_for_each_items` (Phase 1) and spawns `execute_task_iteration` (stays in mod.rs)
- [ ] Import as `use super::for_each_resolve::resolve_for_each_items`
- [ ] Move lines 1875-2896 to `task_dispatch.rs`
- [ ] `cargo test --workspace --lib`

### Phase 7 — Validation (0.5 day, 1 commit)

```
refactor(runtime): verify run() decomposition complete
```

- [ ] `cargo test --workspace --lib` (full: 9,301+ tests)
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] `wc -l runner/mod.rs` ≤ 5,500 lines
- [ ] `grep -c 'fn run(' runner/mod.rs` confirms run() exists and is ~220 lines
- [ ] No `pub` on extracted functions (all `pub(crate)` or `pub(super)`)
- [ ] `cargo bench` on task_execution shows no regression

## Key Patterns to Preserve

### cli_renderer take/put-back

```rust
let mut renderer = self.cli_renderer.take();
// ... use renderer ...
self.cli_renderer = renderer;
```

This exists because the loop body needs `&mut renderer` while also calling `&mut self` methods. The take trick gives a local owned value. **Do not replace with Mutex** — that would be worse.

### executor clone-replace (in init only)

```rust
self.executor = self.executor.clone().with_resolved_agents(...);
self.executor = self.executor.clone().with_skills(...);
```

These are builder methods that consume `self`. Move verbatim to `run_init`.

### execute_task_iteration static pattern

Already extracted as a static async method with all context passed by value/clone. **All new extractions should follow this precedent** for spawned task work. Non-spawned extractions can be `&self`/`&mut self` methods.

## Test Coverage Assessment

| Section | Covered by |
|---------|-----------|
| Init (B) | Integration tests (every test creates a Runner and calls run()) |
| For_each resolution (E/B2) | 15+ dedicated tests (`for_each_*` async tests) |
| Task dispatch (E) | Integration tests (async, exec+echo commands) |
| Result collection (F) | `test_cancellation_during_execution_aborts_workflow` |
| Aggregation (G) | `test_for_each_collects_all_results`, `_preserves_order` |
| Completion (H) | All integration tests (summary rendering) |
| **Weakest** | Orchestrator path (no test for `workflow.goal.is_some()`) |
| **Weakest** | Record compression path |

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Borrow checker fights during select! extraction | Split select! into minimal poll + post-processing |
| LockfileGuard RAII broken by move | Named binding `let _guard = ctx._lockfile_guard;` in run() scope |
| for_each_resolve introduces regressions | 15+ existing tests + 12 new unit tests |
| Performance regression from function call overhead | All extractions are same-module calls — zero overhead after inlining |
| Test isolation broken by submodule boundaries | `pub(crate)` visibility for all internal types |
