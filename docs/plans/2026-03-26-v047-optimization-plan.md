# Plan: v0.47.0 — Performance + Architecture Optimization

> Results from 18 review agents across 2 swarms.
> 4 HIGH, 10 MEDIUM, 8 LOW findings. Estimated: ~600 LOC changes.

## HIGH Impact — Fix First

### H1: TUI chat message items cloned every frame

**File:** `nika-tui/src/views/chat/messages/mod.rs:102`

The entire `Vec<ListItem<'static>>` is deep-cloned every frame even when the
cache is valid. `ListItem` contains `Line` → `Vec<Span>` → `String`. For 50+
messages, this is hundreds of heap allocations per frame.

**Fix:** Truncate-and-extend pattern instead of clone. Cache phase 1 items,
truncate to cached length on each frame, then append phases 2-4 in-place.

### H2: TUI Clear+set_style overwrites all buffer cells

**File:** `nika-tui/src/app/render.rs:88-91`

Every frame writes to every cell twice (Clear + set_style), defeating ratatui's
diff-based rendering. On 200x50 terminal = 10,000 cells flushed per frame.

**Fix:** Remove Clear + set_style. Let ratatui's double-buffer diffing handle
unchanged cells. Set background only on empty cells.

### H3: jsonschema recompiled every retry iteration

**File:** `nika-engine/src/runtime/runner.rs:701`

Inside the retry loop, `jsonschema::validator_for(schema)` recompiles the
validator on every iteration. Schema doesn't change between retries.

**Fix:** Move compilation before the loop. Save 10-50ms per retry.

### H4: Validator compiled per-validation call, not cached

**File:** `nika-engine/src/runtime/output.rs:191,217,237`

`validate_schema()` and `validate_inline_schema()` compile the validator fresh
every call. The same schema may be validated against multiple times in the
retry cycle.

**Fix:** Cache compiled validators alongside JSON values. Use
`DashMap<u64, Arc<Validator>>` keyed by schema hash.

---

## MEDIUM Impact — Next Batch

### M1: for_each task.clone() per iteration

**File:** `runner.rs:1904`

Full `AnalyzedTask` cloned for every for_each item. Use `Arc<AnalyzedTask>`.

### M2: compute_layers() called twice for same DAG

**File:** `runner.rs:1220,2308`

Cache the result after first call, reuse for summary.

### M3: event.clone() on every emit

**File:** `nika-event/src/log.rs:970`

Clone only when broadcast channel exists. Move event into events vec otherwise.

### M4: lower_action() clones owned values

**File:** `runner.rs:884-888`

Accept references instead of owned values to eliminate 4 clones per task.

### M5: Template value.clone() with no transforms

**File:** `template.rs:397,421`

Pass `&value` to `value_to_display()` instead of cloning.

### M6: TUI unbounded MCP call history

**File:** `state/event_handler/provider.rs:153`

Cap at 100 entries with FIFO eviction. Use VecDeque.

### M7: TUI star animation forces idle renders

**File:** `app/mod.rs:487`

Gate `frame % 6 == 0` on recent interaction (5-second decay).

### M8: TUI DAG layout recomputed every frame

**File:** `widgets/dag/ascii.rs`

Cache `DagLayout` result, recompute only on version change.

### M9: TUI DAG nodes/edges cloned every frame

**File:** `views/chat/dag_panel.rs:35-39`

Accept references instead of owned clones.

### M10: resolve_alias_path clones final Value

**File:** `template.rs:323`

Return reference or Cow instead of owned clone.

---

## LOW Impact — Defer

- DAG: O(V*E) → O(V+E) Kahn's algorithm
- Cost: std HashMap → FxHashMap for pricing tables
- Cost: ProviderKind::parse to_lowercase → eq_ignore_ascii_case
- Template: TransformExpr re-parsed per match
- TUI: Vec::remove(0) → VecDeque for history
- Event: Vec not pre-allocated
- Runner: format! inside for_each loop
- Runner: dag_edges Vec not pre-allocated

---

## Architecture Issues (from rust-architect)

### A1: Unify Model / Models commands

Currently `nika model` (gated) and `nika models` (always) are separate.
Should be one command with feature-gated subcommands.

### A2: VerbRunner abstraction

4 handle_* functions share ~60% boilerplate. Extract into VerbRunner struct.

### A3: Provider command behind TUI feature gate

`nika provider` disappears without TUI feature. Should always be available.

### A4: Layer 0 tool injection for from_example

`policy.schema` is None for from_example tasks → Layer 0 DynamicSubmitTool
injection skipped. Need to resolve schema before tool injection.

---

## Execution Order

| Phase | Tasks | Estimated LOC |
|-------|-------|--------------|
| 1 | H1 + H2 (TUI frame perf) | ~80 |
| 2 | H3 + H4 (schema caching) | ~60 |
| 3 | M1-M5 (runtime clones) | ~120 |
| 4 | M6-M9 (TUI memory + render) | ~100 |
| 5 | A1-A4 (architecture) | ~200 |
| 6 | Low priority items | ~100 |
| **Total** | | **~660** |
