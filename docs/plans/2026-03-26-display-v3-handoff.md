# Display & Telemetry v3 — Handoff Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete the display system evolution: Renderer trait, telemetry emission for 5 dead events, summary testability, LiveRenderer event tests, and CLI UX polish (animations, colors, interactivity).

**Architecture:** Four phases. Phase 1 extracts a `Renderer` trait enabling extensibility. Phase 2 wires the 5 remaining dead EventKind emissions. Phase 3 refactors summary.rs for testability and adds LiveRenderer event tests. Phase 4 polishes CLI UX with better animations and visuals.

**Tech Stack:** Rust, indicatif, colored, unicode-width, nika-event, nika-engine

---

## What Was Done (Previous Session — DO NOT REDO)

### Display Refactor v1+v2 (complete)
- **format_event.rs**: 44 shared formatters (CliRenderer + LiveRenderer both call them)
- **RunStats::apply_event()**: DRY stat accumulation (both renderers use it)
- **12 swallowed events**: All now rendered (FetchRetry, Boot, Binding, Decompose, etc.)
- **214 display tests** (~95% formatter coverage)
- **Perf fixes**: SeqCst→Relaxed, output preview avoid-clone, cached terminal width
- **Correctness**: stripped_len unicode-width, TTFT div-by-zero, hash truncation safety

### Current Architecture
```
display/
├── format_event.rs  — 44 pub fn fmt_*() → String (shared formatting)
├── renderer.rs      — CliRenderer (append-only, 916 LOC)
│   └── RunStats     — stats struct + apply_event() method
├── live.rs          — LiveRenderer (indicatif, 1097 LOC)
├── run_renderer.rs  — RunRenderer enum dispatch (Live|Classic)
├── summary.rs       — print_run_summary() + helpers (664 LOC)
├── colors.rs        — stripped_len, sparkline, budget_bar
├── icons.rs         — cosmic icon palette
├── spinner.rs       — indicatif spinner constants
├── detail.rs        — DetailLevel (Max/Default/Min/Json)
├── header.rs        — workflow header box
├── check.rs         — pre-flight validation display
├── dag.rs           — compact DAG flow
├── dag_render.rs    — rich DAG box visualization
├── mod.rs           — module registration + re-exports
└── tests.rs         — 214 tests
```

---

## Phase 1: Renderer Trait Extraction (3 tasks)

### Task 1: Define the Renderer trait + implement for both renderers

**Files:**
- Modify: `nika-engine/src/display/renderer.rs` (trait definition + impl for CliRenderer)
- Modify: `nika-engine/src/display/live.rs` (impl for LiveRenderer)
- Modify: `nika-engine/src/display/mod.rs` (re-export Renderer)

**Trait signature (from architect research):**

```rust
pub trait Renderer {
    /// Set task-to-DAG-layer mapping. Default: no-op.
    fn set_task_layers(&mut self, _layers: HashMap<Arc<str>, usize>) {}
    /// Initialize per-task display state. Default: no-op.
    fn init_tasks(&mut self, _task_ids: &[String], _task_deps: &HashMap<String, Vec<String>>) {}
    /// ID of the last rendered event.
    fn last_rendered_id(&self) -> Option<u64>;
    /// Render a single EventKind (synthesizes temporary Event).
    fn render_kind(&mut self, kind: &EventKind);
    /// Render all events newer than last_rendered_id.
    fn render_new_events(&mut self, events: &[Event]);
    /// Render full summary footer.
    fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>);
    /// Render compact one-line summary.
    fn render_quiet_summary(&mut self, total_duration_ms: u64);
    /// Access accumulated stats.
    fn stats(&self) -> &RunStats;
}
```

Key decisions:
- `set_task_layers` + `init_tasks` have default no-ops (CliRenderer inherits)
- All `render_*` methods take `&mut self` (LiveRenderer needs mutability for finalize_bars)
- `stats()` accessor instead of trait field

### Task 2: Replace RunRenderer enum with `Box<dyn Renderer>`

**Files:**
- Modify: `nika-engine/src/display/run_renderer.rs` (enum → 3 factory functions)
- Modify: `nika-engine/src/runtime/runner.rs` (Option<RunRenderer> → Option<Box<dyn Renderer>>)
- Modify: `nika-engine/src/display/mod.rs` (re-exports)

Replace 157-line enum dispatch with ~40 lines:
```rust
pub fn auto_renderer(detail: DetailLevel) -> Box<dyn Renderer> { ... }
pub fn classic_renderer(detail: DetailLevel) -> Box<dyn Renderer> { ... }
pub fn live_renderer(detail: DetailLevel) -> Box<dyn Renderer> { ... }
```

### Task 3: Add TestRenderer for mocking + runner integration tests

**Files:**
- Modify: `nika-engine/src/display/renderer.rs` (add #[cfg(test)] TestRenderer)
- Modify: `nika-engine/src/display/tests.rs` (TestRenderer tests)

---

## Phase 2: Wire 5 Dead Event Emissions (3 tasks)

### Task 4: Thread EventLog through binding/resolve.rs

**Files:**
- Modify: `nika-engine/src/binding/resolve.rs`
- Modify: `nika-engine/src/runtime/runner.rs` (pass EventLog to resolve)

Thread `event_log: &EventLog` and `task_id: Arc<str>` through:
- `ResolvedBindings::from_with_spec()`
- `resolve_entry()`
- `resolve_binding_path()`

### Task 5: Emit 3 Binding events + NativeModelLoaded

**Files:**
- Modify: `nika-engine/src/binding/resolve.rs` (emit BindingDefaultApplied, BindingTransformApplied, BindingEnvResolved)
- Modify: `nika-engine/src/provider/native.rs` (emit NativeModelLoaded)

### Task 6: Add MediaCleanup GC stub

**Files:**
- Modify: `nika-engine/src/media/cas.rs` (add cleanup method)
- Emit MediaCleanup event after GC pass

---

## Phase 3: Testability + LiveRenderer Tests (4 tasks)

### Task 7: Refactor summary.rs — extract format_* functions (4 incremental commits)

**Files:**
- Modify: `nika-engine/src/display/summary.rs`
- Modify: `nika-engine/src/display/mod.rs` (re-exports)

**Commit order (each independently compilable):**
1. `format_done_summary()` + thin `print_done_summary()` wrapper (~50 LOC)
2. `format_doctor_header()` + `format_doctor_summary()` + wrappers
3. `format_run_quiet_summary()` + wrapper
4. `format_run_summary(stats, detail, duration, trace, term_width)` + 6 section extractors

Key: `term_width: u16` becomes an explicit parameter on `format_run_summary`. The `print_` wrapper queries terminal_size. Tests pass `80`.

Also extract `nika_dir_exists: bool` parameter on `format_doctor_summary` (eliminates filesystem coupling).

### Task 8: Add 20 summary tests

**Files:**
- Modify: `nika-engine/src/display/tests.rs`

**Test cases (from summary research):**
- Full box: all_passed, with_failures, json_returns_empty, min_delegates_to_quiet, default_detail, zero_tokens, zero_cost, width_clamping
- Sections: tokens_with_cache, tokens_no_cache, cost_per_task, performance_ttft, infrastructure_all, timeline_gantt
- Quiet: passed, failed_icon
- Done: basic, with_trace
- Doctor: header_box, all_3_scenarios

Use `make_stats()` helper function with realistic RunStats data.
Consider `insta` snapshots for full box visual regression (already in deps).

### Task 9: Add LiveRenderer event rendering tests

**Files:**
- Modify: `nika-engine/src/display/live.rs` (inline tests module)

Use `LiveRenderer::hidden()` to render event sequences:
- TaskStarted → ProviderResponded → TaskCompleted (verify stats)
- TaskFailed (verify root_failure)
- Detail level filtering (Min suppresses sub-events)

### Task 10: Add format_output_preview tests

**Files:**
- Modify: `nika-engine/src/display/tests.rs`

Test: empty output, JSON, Markdown, plain text, very long input, Unicode.

---

## Phase 4: CLI UX Polish (5 tasks)

### Task 11: Add real-time cost counter to overall progress bar

Live-updating `$0.0042` on the overall bar as ProviderResponded events arrive.

### Task 12: Add elapsed time tick on running task bars

Show `+2.3s` that updates every 500ms, not just on events.

### Task 13: Improve error presentation

Add `suggested_fix()` method to NikaError. Render fix suggestions in output.

### Task 14: Add completion animation

Brief green flash / checkmark animation when task completes.

### Task 15: Improve daemon status display

`nika daemon status` with colored health indicators, uptime, connection count.

---

## Socratic Questions for UX Improvement

1. **When a task hangs for 30s, what does the user see?** Just a spinning indicator. Should we show a "slow" warning with elapsed time?
2. **When binding falls back to a default, is the user aware?** Currently invisible. Should we show a yellow warning?
3. **When fetch retries 3 times, does the user understand why?** FetchRetry is now visible, but do we show the backoff strategy?
4. **Can the user cancel a single task without aborting the workflow?** No — should we add Ctrl+C task cancellation in interactive mode?
5. **After a workflow fails, does the user know which task to fix?** The summary shows root_failure, but do we link to the error doc?
6. **Is the output preview useful?** It shows 2 lines — is that enough? Should we expand on demand?
7. **Does the cost display help the user optimize?** Per-task cost is shown, but do we show cost-per-token or compare to alternatives?

---
