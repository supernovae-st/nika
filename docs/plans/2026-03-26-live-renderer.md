# Live Renderer — `nika run` Display Overhaul

**Date**: 2026-03-26
**Status**: Implementation Plan
**Scope**: `nika-engine/src/display/` + `nika-engine/Cargo.toml` + `tools/nika/src/main.rs`

## Problem

`nika run` currently uses **append-only `println!()`** for all output. Events cascade down
the terminal as they happen — task scheduled, task started, provider called, provider responded,
task completed — producing a wall of text that's hard to follow during execution.

**Current pain points:**
- No in-place updates: once a task line is printed, it never changes
- No spinner animation for running tasks — just a static `●`
- No overall progress bar — you can't tell "3 of 6 tasks done"
- No live elapsed timer — timestamps are frozen at print time
- Event details (provider, MCP, media) interleave with task status, breaking visual flow
- Long-running LLM calls show nothing while waiting (dead screen for seconds)

## Solution

Replace the append-only `CliRenderer` with a **`LiveRenderer`** that uses `indicatif::MultiProgress`
to maintain a **fixed status area** at the bottom of the terminal while **scrolling event details above**.

### Design Principles

1. **Fixed bottom, scrolling top** — Task status bars stay pinned; event logs scroll above
2. **Animated spinners** — Running tasks pulse with braille dots (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏)
3. **Live counters** — Elapsed time, token count, cost update in real-time
4. **Graceful degradation** — Non-TTY (CI, pipes) falls back to existing `CliRenderer`
5. **Reuse existing** — Keep `icons.rs`, `colors.rs`, `RunStats`, summary box — they're excellent
6. **One new dependency** — `indicatif` only (already used by nextest, turborepo, 3M+ downloads/month)

## Architecture

### Display Layout

```
╭────────────────────────────────────────────────────────╮
│                                                        │   ← Header (printed once,
│  N I K A                                     v0.47.0   │      scrolls away)
│  seo-pipeline · ⋈ anthropic/sonnet · 6 tasks          │
│                                                        │
╰────────────────────────────────────────────────────────╯

  DAG: fetch → [summarize, translate] → review → publish  ← Static DAG (scrolls away)

      │ ⋈ anthropic/sonnet · prompt: 4240 chars            ┐
      │ ← in: 1.2k out: 342 cache: — · ttft: 245ms        │ Scrolling event log
      │ ⊚ → output/summary.md · 2.1 KB · markdown          │ (emitted via multi.println)
      │ ⊞ novanet → search_entities call:1 ← 4.2 KB        ┘

─────────────────────────────────────────────────────────── ← Visual separator

  ⠹ ✧ fetch_data       running  +2.3s  in:1.2k out:—      ┐
  ⠹ ⊛ call_mcp         running  +1.1s                      │
  ✓ ☄ load_context      0.4s    in:280 out:0               │ Fixed status area
  ○   translate          pending  deps: fetch_data          │ (indicatif bars,
  ○   review             pending  deps: summarize           │  update in-place)
  ○   publish            pending  deps: review              │
                                                            │
  ━━━━━━━━━╸─────────────────── 2/6  +3.1s  $0.004        ┘ Overall progress bar
```

### Component Hierarchy

```
LiveRenderer
├── MultiProgress (indicatif)            — manages all bars, handles terminal sync
│   ├── separator_bar: ProgressBar       — thin dimmed line "───────────"
│   ├── task_bars: IndexMap<String, TaskBar>  — one per task, in DAG order
│   │   ├── TaskBar { bar, verb, status, start_time, tokens_in, tokens_out }
│   │   └── ...
│   └── overall_bar: ProgressBar         — bottom progress bar with elapsed + cost
├── stats: RunStats                      — reused from CliRenderer
├── detail: DetailLevel                  — controls sub-event visibility
├── start: Instant                       — workflow start time
└── term_width: u16                      — for layout calculations
```

### Event → Action Mapping

| EventKind | LiveRenderer Action |
|-----------|-------------------|
| `WorkflowStarted` | Print header via `multi.println()`, create all task bars as pending |
| `TaskScheduled` | Update bar message: `"○   {task_id}  pending  deps: {deps}"` |
| `TaskStarted` | Enable spinner, set message: `"⠋ {verb_icon} {task_id}  running"` |
| `ProviderCalled` | `multi.println()` provider detail line (if detail allows) |
| `ProviderResponded` | `multi.println()` response line + update task bar with token counts |
| `ContextAssembled` | `multi.println()` context line (if detail allows) |
| `McpInvoke/Response` | `multi.println()` MCP detail lines |
| `TaskCompleted` | Finish bar: `"✓ {verb_icon} {task_id}  {duration}  in:{tok} out:{tok}"` |
| `TaskFailed` | Finish bar: `"✗ {verb_icon} {task_id}  {duration}  {error}"` |
| `TaskSkipped` | Finish bar: `"⊘   {task_id}  skipped — {reason}"` |
| `ArtifactWritten` | `multi.println()` artifact line |
| `MediaStored` | `multi.println()` media line |
| `GuardrailPassed/Failed` | `multi.println()` guardrail line |
| `AgentTurn` | Update task bar message with turn count |
| `ForEachCompleted` | `multi.println()` aggregation summary |
| `WorkflowCompleted` | Finish overall bar, clear multi, print summary box |
| `WorkflowFailed` | Finish overall bar red, clear multi, print failure summary |

### TTY Detection & Renderer Selection

```rust
// In runner.rs or main.rs
pub enum RunRenderer {
    Live(LiveRenderer),
    Classic(CliRenderer),
}

impl RunRenderer {
    pub fn auto(detail: DetailLevel) -> Self {
        let is_tty = std::io::stderr().is_terminal();
        if is_tty && !detail.is_json() && detail != DetailLevel::Min {
            Self::Live(LiveRenderer::new(detail))
        } else {
            Self::Classic(CliRenderer::new(detail))
        }
    }
}
```

Users can force classic mode with `--no-live` flag or `NIKA_NO_LIVE=1` env var.

### Spinner Design

```rust
// Cosmic spinner — matches Nika's stellar aesthetic
pub const COSMIC_SPINNER: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
pub const COSMIC_DONE: &str = "✓";
pub const COSMIC_FAIL: &str = "✗";
pub const COSMIC_SKIP: &str = "⊘";
pub const COSMIC_PENDING: &str = "○";

// Tick interval: 80ms (standard, matches most CLI tools)
pub const SPINNER_TICK_MS: u64 = 80;
```

### Progress Bar Templates (indicatif syntax)

```rust
// Overall progress bar
const OVERALL_TEMPLATE: &str =
    "  {bar:30.cyan/dim} {pos}/{len}  {elapsed_precise}  {msg}";

// Task bar — running state
const TASK_RUNNING_TEMPLATE: &str =
    "  {spinner:.cyan} {prefix} {wide_msg}";

// Task bar — pending state (no spinner)
const TASK_PENDING_TEMPLATE: &str =
    "  {prefix} {wide_msg}";

// Progress chars for overall bar
const PROGRESS_CHARS: &str = "━╸─";
```

## New Files

### 1. `display/live.rs` — LiveRenderer (~400 lines)

```rust
pub struct LiveRenderer { ... }

impl LiveRenderer {
    pub fn new(detail: DetailLevel) -> Self;
    pub fn init_tasks(&mut self, task_ids: &[String], task_deps: &HashMap<String, Vec<String>>);
    pub fn render(&mut self, event: &Event);
    pub fn render_new_events(&mut self, events: &[Event]);
    pub fn last_rendered_id(&self) -> Option<u64>;
    pub fn render_summary(&self, total_duration_ms: u64, trace_path: Option<&str>);
    pub fn render_quiet_summary(&self, total_duration_ms: u64);

    // Internal
    fn log(&self, line: &str);          // multi.println() wrapper
    fn update_task(&mut self, id: &str, status: TaskStatus);
    fn update_overall(&mut self);
    fn format_task_line(...) -> String;  // Build colored task status line
}
```

### 2. `display/spinner.rs` — Spinner constants (~60 lines)

Constants for spinner character sets, tick intervals, and progress bar templates.

### 3. `display/run_renderer.rs` — Enum dispatch (~80 lines)

```rust
pub enum RunRenderer {
    Live(LiveRenderer),
    Classic(CliRenderer),
}

impl RunRenderer {
    pub fn auto(detail: DetailLevel) -> Self;
    pub fn render_new_events(&mut self, events: &[Event]);
    pub fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>);
    pub fn last_rendered_id(&self) -> Option<u64>;
    pub fn set_task_layers(&mut self, layers: HashMap<Arc<str>, usize>);
    pub fn init_tasks(&mut self, ...);
}
```

## Modified Files

| File | Change |
|------|--------|
| `display/mod.rs` | Add `pub mod live; pub mod spinner; pub mod run_renderer;` + re-exports |
| `runtime/runner.rs` | Replace `cli_renderer: Option<CliRenderer>` with `renderer: Option<RunRenderer>` |
| `tools/nika/src/main.rs` | Use `RunRenderer::auto()`, add `--no-live` flag |
| `tools/Cargo.toml` | Add `indicatif = "0.18"` to workspace deps |
| `tools/nika-engine/Cargo.toml` | Add `indicatif = { workspace = true }` |
| `display/detail.rs` | No change needed — existing levels work as-is |

## Dependency Impact

```toml
# Only ONE new crate:
indicatif = "0.18"

# Transitive deps (all lightweight):
# - console (already used by cliclack in workspace)
# - number_prefix
# - portable-atomic
# - unicode-width (already in workspace)
```

**Binary size impact**: ~50-80 KB — negligible for a CLI tool.

## Implementation Tasks

### Phase 1: Foundation (3 tasks)
1. Add `indicatif` dependency to workspace + nika-engine
2. Create `display/spinner.rs` with constants
3. Create `display/live.rs` with `LiveRenderer` struct + `new()` + `init_tasks()`

### Phase 2: Event Rendering (4 tasks)
4. Implement `render()` — task lifecycle events (Scheduled/Started/Completed/Failed/Skipped)
5. Implement `render()` — sub-events via `multi.println()` (Provider, MCP, Context, etc.)
6. Implement `render()` — agent events (AgentStart/Turn/Complete/Spawned)
7. Implement overall progress bar updates + live cost/token counters

### Phase 3: Integration (3 tasks)
8. Create `display/run_renderer.rs` enum dispatch
9. Wire `RunRenderer` into `runner.rs` (replace `cli_renderer`)
10. Wire into `main.rs` — TTY detection, `--no-live` flag, env var fallback

### Phase 4: Summary & Polish (3 tasks)
11. Implement `render_summary()` — reuse existing summary box format
12. Handle edge cases: single-task workflow, for_each, agent spawning, resize
13. Add tests with `ProgressDrawTarget::hidden()` + update existing display tests

### Phase 5: Wow Effects (2 tasks)
14. Add live elapsed timer on task bars (updates each spinner tick)
15. Add live streaming token preview on running task bars

## Test Strategy

- **Unit**: `LiveRenderer` with `ProgressDrawTarget::hidden()` — no terminal needed
- **Existing**: All 1560+ lines of `CliRenderer` tests continue to pass (classic mode)
- **Integration**: `cargo test -p nika-engine --lib` must stay green (8100+ tests)
- **Manual**: `nika run` on a real workflow to validate visual output

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| Flickering on fast task completion | Set `MultiProgress` draw rate to 15fps max |
| indicatif conflicts with existing stdout writes | Use `multi.println()` for ALL output during live mode |
| Non-TTY breakage (CI, pipes) | Auto-detect + explicit `--no-live` flag |
| Tests that capture stdout | `ProgressDrawTarget::hidden()` for test mode |
| Terminal resize mid-render | indicatif handles this automatically |
| Very large DAGs (50+ tasks) | Cap visible task bars at 20, show "+N more" |

## Visual Comparison

### Before (current CliRenderer)
```
+0.1s  ○   fetch_data        scheduled deps: —
+0.1s  ●  ✧ fetch_data        running
                                                    ← dead screen for 2s while LLM thinks
+2.3s     │ ⋈ anthropic/sonnet · prompt: 4240 chars
+2.3s     │ ⋈ ← in: 1.2k out: 342 cache: —
+2.3s  ✓  ✧ fetch_data        +2.2s
+2.3s  ○   summarize          scheduled deps: fetch_data
+2.3s  ●  ✧ summarize          running
                                                    ← more dead screen
+4.1s     │ ⋈ anthropic/sonnet · prompt: 2100 chars
+4.1s  ✓  ✧ summarize          +1.8s
```

### After (LiveRenderer)
```
      │ ⋈ anthropic/sonnet · prompt: 4240 chars
      │ ← in: 1.2k out: 342 · ttft: 245ms · $0.003
      │ ⊚ → output/summary.md · 2.1 KB

  ✓ ✧ fetch_data       0.4s    in:280 out:142
  ⠹ ✧ summarize        running +1.1s  in:2.1k out:—    ← spinner animates!
  ⠹ ☄ translate        running +0.8s                    ← parallel tasks visible!
  ○   review            pending  deps: summarize, translate
  ○   publish           pending  deps: review

  ━━━━━━━━━━━━╸──────────────── 2/6  +2.3s  $0.003    ← live progress bar
```

## Decision Record

**Why indicatif over alternatives?**
- **vs superconsole** (Meta/Buck2): Heavier, component framework we don't need, less community
- **vs ratatui inline**: Requires raw mode, overkill for progress display, heavier integration
- **vs custom ANSI**: Reinventing the wheel — indicatif solves this exactly
- **vs prodash** (gitoxide): Tree hierarchy is nice but API is more complex than needed

**Why keep two renderers?**
- CI/CD pipelines need clean, parseable output (existing `CliRenderer`)
- JSON mode (`--detail json`) needs raw NDJSON (existing `CliRenderer`)
- Piped output (`nika run | jq`) must not contain ANSI cursor codes
- Different users prefer different styles — choice is good
