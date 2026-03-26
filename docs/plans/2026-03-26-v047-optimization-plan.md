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

### A1: Provider command behind TUI feature gate

`nika provider` disappears without TUI feature. Move to nika-cli. 5-min fix.

### A2: Layer 0 tool injection for from_example

`policy.schema` is None for from_example tasks → Layer 0 DynamicSubmitTool
injection skipped. Need to resolve schema from example before tool injection.

### A3: Model/Models — confirmed by-design (no action needed)

### A4: VerbRunner — already resolved (verbs split into per-verb modules)

---

## Architecture (from rust-architect, 317k LOC audit)

### ARCH-1: Extract nika-init crate (HIGH — saves ~13% compile)

`init/` module is 20,872 LOC of pure code generation (courses, showcases,
scaffolds). Zero runtime coupling. Moving to `nika-init` crate eliminates
recompilation of engine when init code changes.

### ARCH-2: Move builtin media tools to nika-media (HIGH — saves ~7% compile)

24 media tools (11k LOC prod) live in `nika-engine/src/runtime/builtin/media/`.
They already depend on `nika-media` for CAS. Moving implementations there
eliminates duplicate deps (blake3, infer, imagesize, thumbhash).

### ARCH-3: Split NikaError (91 variants → per-crate errors)

Current: monolithic 91-variant enum with NIKA-160 collision. Manual 60-line
`From<McpError>` conversion. 125-line `code()` match.

Target: `EngineError` (~30 variants), `RuntimeError`, `InitError`, keep
existing `CoreError` (3), `McpError`, `EventError`.

### ARCH-4: Seal nika-engine public API

21 `pub mod` declarations leak internals. Change `io`, `util`, `new`, `secrets`
to `pub(crate)`. Replace wildcard re-exports with explicit type lists.

### ARCH-5: Extract ContentBlock to nika-core

Breaks `nika-media → nika-mcp` dependency (imported for 1 enum).

---

## Execution Order

| Phase | Tasks | Estimated LOC | Impact |
|-------|-------|--------------|--------|
| 1 | H1 + H2 (TUI frame perf) | ~80 | 10x fewer cell writes/frame |
| 2 | H3 + H4 (schema caching) | ~60 | 10-50ms saved per retry |
| 3 | M1-M5 (runtime clones) | ~120 | Less GC pressure |
| 4 | M6-M9 (TUI memory + render) | ~100 | Lower memory, fewer idle renders |
| 5 | A1-A2 (quick arch fixes) | ~50 | Provider always available |
| 6 | ARCH-1 (extract nika-init) | ~200 | 13% faster incremental compile |
| 7 | ARCH-2 (media tools → nika-media) | ~300 | 7% faster, no dup deps |
| 8 | ARCH-3 (split NikaError) | ~400 | Clean error boundaries |
| **Total** | | **~1,310** | |
| **Total** | | **~660** |
