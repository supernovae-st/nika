# Nika TUI — Deep Polish, UX Coherence & Connected Features

> **Copy-paste into a fresh Claude Code chat. ultrathink, full autonomy.**

---

## Context

Nika TUI: ratatui terminal app at `/Users/thibaut/dev/supernovae/nika/tools/nika-tui/src/`.
3-view architecture (Studio/Command/Control), ~86K LOC, 2091 tests, 145 files.

Two architecture cleanup sessions completed — the codebase is **structurally clean**:
no file >1050 LOC non-test, zero unnecessary `#[allow(dead_code)]` on modules, DAG/wizard/
event_handler/studio all properly split into focused submodules.

This session focuses on **real issues found by deep audit**: silent failures, disconnected
features, UX coherence gaps, design inconsistencies, config fields that don't work, input
edge cases, memory bounds, testing gaps, and wiring stubs into working features.

**Skills to use**: `rust-core`, `rust-async`, `test-driven-development`, `systematic-debugging`,
`defense-in-depth`, `verification-before-completion`, `root-cause-tracing`

---

## AUDIT FINDINGS (verified, all with file:line)

```
CRITICAL   5 issues (silent failures, input safety, signal handler)
HIGH       8 issues (stubs, theme, config, testing)
MEDIUM    12 issues (UX, organization, memory, polish)
Total     25 actionable items across 14 phases
```

---

## PHASE 1 — Fix Critical Silent Failures
**Skill**: `defense-in-depth` | **Risk**: LOW | **Time**: 30min

### 1.1 ChatAgent init error handling
`app/mod.rs:180,246` + `app/routing.rs:303` — `ChatAgent::new().ok()` converts errors
to `None`. User types `/infer` → nothing happens. No error, no feedback.

```rust
// Replace in all 3 locations:
let chat_agent = ChatAgent::new().ok();
// With:
let chat_agent = match ChatAgent::new() {
    Ok(agent) => Some(agent),
    Err(e) => {
        tracing::warn!("Chat agent unavailable: {e}");
        // set_status() if available at this point
        None
    }
};
```

### 1.2 ViewAction::ChatMcp is a stub
`app/routing.rs:308-310` — Handler receives `ChatMcp(_mcp_action)` but only logs
`"MCP action received"`. No routing to MCP tool execution. Wire actual logic.

### 1.3 MCP pool init failure silent
`app/commands.rs:14` — `init_mcp_clients()` errors `.ok()`'d. Show notification on failure.

---

## PHASE 2 — Input Safety & Resilience
**Skill**: `defense-in-depth`, `rust-core` | **Risk**: MEDIUM | **Time**: 45min

### 2.1 Bracketed paste mode
`views/chat/input.rs` uses `tui-input` which processes paste character-by-character.
Pasting 1000+ chars triggers per-char processing. Detect CSI 200h/201h and buffer.

### 2.2 Max input length
`views/chat/input.rs:101-119` — No max input length enforced. User can paste 1MB.
Add `const MAX_INPUT_LEN: usize = 10_240;` and clamp in input handler.

### 2.3 Signal handler fix
`lib.rs:218-230` — Uses `std::thread::spawn()` with `loop { sleep(100ms) }` poll.
Wastes CPU. Replace with `signal_hook::iterator::Signals` (uses epoll/kqueue).

```rust
// Replace polling loop with:
use signal_hook::iterator::Signals;
let mut signals = Signals::new(&[SIGTERM, SIGHUP]).ok();
std::thread::spawn(move || {
    if let Some(ref mut sigs) = signals {
        for _ in sigs.forever() {
            let _ = crossterm::terminal::disable_raw_mode();
            let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
            std::process::exit(128 + 15);
        }
    }
});
```

---

## PHASE 3 — Wire Up Quick Win Actions
**Skill**: `rust-core` | **Risk**: LOW | **Time**: 1h

20 `Action` variants in `app/types.rs` are matched in `routing.rs` with empty bodies.
Wire the 6 most useful:

| Action | routing.rs line | Implementation |
|--------|----------------|----------------|
| CopyToClipboard | 115 | `arboard::Clipboard::new()?.set_text(state.workflow.final_output)` |
| DismissError | 148 | `state.workflow.error_message = None; state.dirty.status = true;` |
| ScrollToTop | 82 | Delegate to active view's scroll: `view.scroll_to_top()` |
| ScrollToBottom | 85 | Delegate: `view.scroll_to_bottom()` |
| TogglePause | 90 | `state.workflow.paused = !paused; add_notification(...)` |
| DismissNotification | 142 | `state.dismiss_latest()` |

---

## PHASE 4 — Config Fields: Apply or Remove
**Skill**: `rust-core`, `verification-before-completion` | **Risk**: LOW | **Time**: 1h

`config.rs` defines 15+ settings that are **parsed but never applied**:

| Config field | Line | Status |
|-------------|------|--------|
| `tui.mouse` | 69 | UNUSED — mouse is always on |
| `tui.animations` | 71 | UNUSED — animations always on |
| `tui.show_timestamps` | 73 | UNUSED — timestamps never shown |
| `studio.auto_save` | 123 | UNUSED — no auto-save logic |
| `studio.tab_width` | 127 | UNUSED — hard-coded 2 spaces |
| `studio.highlight_line` | 131 | UNUSED — always highlights |
| `chat.default_provider` | 95 | UNUSED — doesn't set initial provider |
| `chat.default_model` | 97 | UNUSED — doesn't set initial model |
| `chat.max_history` | 75 | UNUSED — no truncation enforced |

**Task**: For each, either wire it into the code or delete it from the config struct.
Priority: `chat.default_provider` + `chat.default_model` (most user-facing),
`studio.tab_width` (easy), `chat.max_history` (safety).

---

## PHASE 5 — Memory Bounds
**Skill**: `defense-in-depth` | **Risk**: LOW | **Time**: 30min

### 5.1 Activity stack unbounded
`views/chat/mod.rs:170` — `activity_items: Vec<ActivityItem>` grows per agent turn.
1000 agent turns = unbounded growth. Add `const MAX_ACTIVITY_ITEMS: usize = 100;`

### 5.2 Browser entries unbounded
`standalone/state.rs:74-131` — File scan has `max_depth: 4` but no file count limit.
Directory with 10,000 `.nika.yaml` files = OOM. Add `limit: 500` to WalkBuilder.

### 5.3 Chat history max_history not enforced
`config.rs:75` defines `max_history: 100` but `ChatView.messages` is never truncated.
Add truncation in `push_message()`.

---

## PHASE 6 — Theme Coherence
**Skill**: `rust-core` | **Risk**: MEDIUM | **Time**: 1h30

### 6.1 Kill hardcoded colors
~20 instances of `const Color = Rgb(...)` bypass theme system:
- `pro_status_bar.rs`: 8 `COLOR_*` constants → replace with `theme.status_*`
- `header.rs`: `TAB_ACTIVE_BG` hardcoded → replace with `theme.accent`
- `dag/node_data.rs`: 10 `DEFAULT_*_COLOR` constants → replace with theme lookups
- `dag/edge.rs`: 5 `DEFAULT_*_COLOR` constants → replace with theme lookups

### 6.2 Status bar consolidation
`status_bar.rs` (955 LOC) and `pro_status_bar.rs` both render status with different
depth. Both pull from SessionContext independently → desync risk.
Consolidate into single rendering that reads from one source.

---

## PHASE 7 — Provider State Consolidation
**Skill**: `rust-core` | **Risk**: MEDIUM | **Time**: 45min

ChatView has 4 redundant provider fields updated together in `routing.rs:287-300`:
```rust
pub current_model: String,          // "claude-sonnet-4"
pub cached_provider: Provider,      // enum
pub provider_name: String,          // "Claude"
pub current_provider_id: String,    // "claude"
```

Replace with:
```rust
pub struct ActiveProvider {
    pub id: String,
    pub name: String,
    pub model: String,
    pub kind: Provider,
}
```

Update all 4 construction sites + all read sites (~30 references).

---

## PHASE 8 — Scroll State Cleanup + Metrics Unification
**Skill**: `root-cause-tracing` | **Risk**: MEDIUM | **Time**: 45min

### 8.1 ChatView scroll states
3 overlapping scroll fields — clarify ownership:
- Remove `scroll: usize` if `conversation_scroll` covers it
- Derive `user_at_bottom` from scroll state, not separate bool
- Document which scroll controls which panel

### 8.2 Metrics unification
`SessionContext`, `SessionMetrics`, `TurnMetrics` are 3 independent structs.
Both status bars read independently → desync risk. Unify into single
`SessionState` that both read from.

---

## PHASE 9 — Testing Gaps
**Skill**: `test-driven-development` | **Risk**: LOW | **Time**: 2h

### Critical modules with ZERO tests:
- `app/routing.rs` — view switching, action dispatch (CRITICAL)
- `app/render.rs` — frame rendering
- `views/monitor/mod.rs` (998 LOC)
- `views/control/mod.rs`
- `standalone/mod.rs`

### 9.1 Routing tests
Test that each `Action::*` variant does what it claims. Test view switching lifecycle
(`on_leave` → `on_enter`). Test that `ViewAction` → `Action` conversion is complete.

### 9.2 Integration tests
Test the full cycle: key press → ViewAction → routing → state change → render.
Use `TestBackend` from ratatui for screenshot testing.

### 9.3 Edge case tests
- Terminal resize mid-render
- Clipboard unavailable (headless)
- Empty workflow (no tasks)
- Very long task IDs / output strings

---

## PHASE 10 — Command System Polish
**Skill**: `rust-core` | **Risk**: LOW | **Time**: 1h

### 10.1 Tab completion
`command/mod.rs` — No tab completion exists. Add:
```rust
impl Command {
    pub fn completions(prefix: &str) -> Vec<&'static str> {
        ["/infer", "/exec", "/fetch", "/invoke", "/agent",
         "/help", "/model", "/mcp", "/clear", "/export", "/run"]
        .iter()
        .filter(|cmd| cmd.starts_with(prefix))
        .copied()
        .collect()
    }
}
```
Wire into chat input handler on Tab key.

### 10.2 Argument validation
`/agent --max-turns NAN` is not validated. Parse and validate numeric args.
`/fetch` without URL shows good error (FetchError), but `/invoke` without tool doesn't.

### 10.3 Keybinding documentation
Add missing bindings to `keybindings.rs:keybindings_for_context()`:
- Ctrl+Z/Y → "Undo/Redo" (Studio insert mode)
- Alt+Left/Right → "Word jump"
Remove Ctrl+L reference if unimplemented.

---

## PHASE 11 — Widget Organization
**Skill**: `rust-architect` agent | **Risk**: LOW | **Time**: 30min

Move chat-only widgets to `views/chat/widgets/`:
- `widgets/chat_dag_panel.rs` → `views/chat/widgets/dag_panel.rs`
- `widgets/chat_node_box.rs` → `views/chat/widgets/node_box.rs`
- `widgets/chat_edge_line.rs` → `views/chat/widgets/edge_line.rs`
- `widgets/chat_task_queue.rs` → `views/chat/widgets/task_queue.rs`

Update imports. Remove from `widgets/mod.rs` re-exports.

---

## PHASE 12 — Standalone Mode Polish
**Skill**: `rust-core`, `rust-async` | **Risk**: MEDIUM | **Time**: 1h

### 12.1 Search filter
`standalone/state.rs:32-35` has `search_query`, `search_active` fields but filtering
logic is not implemented. Wire filter into `browser_entries` rendering.

### 12.2 Recent files shortcut
No "open recent" shortcut. Add Ctrl+R → show last 10 opened workflows.

### 12.3 File watcher (stretch)
No auto-refresh when workflows added/modified on disk. Consider `notify` crate
with debounced events for tree refresh.

---

## PHASE 13 — Async Resilience
**Skill**: `rust-async` | **Risk**: MEDIUM | **Time**: 30min

### 13.1 Unbounded provider verification
`app/lifecycle.rs:200+` spawns 7+ verification tasks without concurrency limit.
Repeated calls = unbounded task spawning. Use `tokio::task::JoinSet` with max=4.

### 13.2 Background task cleanup
`background_handles` in `app/mod.rs:126` — verify handles are cleaned up on view
switch and app exit. Check for leaked tasks.

---

## PHASE 14 — Performance Polish
**Skill**: `rust-perf` agent | **Risk**: LOW | **Time**: 30min

### 14.1 Tree widget cloning
`views/studio/mod.rs:223,227` — `TreeNode .clone()` every frame. Use `Rc<TreeNode>`
or cache the reference.

### 14.2 Syntax highlighting cache
Studio editor re-highlights on every frame if `validation_pending`. Cache highlighted
lines and only re-highlight changed lines.

### 14.3 DAG layout idle skip
Verify DAG layout computation is skipped when workflow is idle (no active tasks).
The content hash cache in `render_dag.rs:119-128` should handle this.

---

## Rules

- `cargo check -p nika-tui && cargo clippy -p nika-tui -- -D warnings` after EVERY edit
- `cargo test -p nika-tui --lib` before EVERY commit (must stay at 2091+, ideally grow)
- Commits: `type(scope): desc` with both co-authors:
  ```
  Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  ```
- 1 logical change = 1 commit
- Start with Phase 1 (critical fixes), then Phase 2 (safety), then pick by ROI
- Use skills: `defense-in-depth` for validation, `test-driven-development` for Phase 9,
  `systematic-debugging` for tracing issues, `rust-async` for Phase 13
- Use agents: `rust-perf` for Phase 14, `rust-architect` for Phase 11,
  `rust-security` for Phase 2, `rust-async-expert` for Phase 13
- Ask before changing public API (widget re-exports, struct fields)
- Push when done

## Priority Order
```
Phase  1  (critical silent failures)     → 30min  → MUST DO
Phase  2  (input safety + signal fix)    → 45min  → MUST DO
Phase  3  (wire action stubs)            → 1h     → HIGH VALUE
Phase  4  (config apply or remove)       → 1h     → HIGH VALUE
Phase  5  (memory bounds)               → 30min  → SAFETY
Phase  9  (testing gaps)                → 2h     → QUALITY
Phase  6  (theme coherence)             → 1h30   → DESIGN
Phase  7  (provider consolidation)      → 45min  → COHERENCE
Phase  8  (scroll + metrics cleanup)    → 45min  → COHERENCE
Phase 10  (command polish)              → 1h     → UX
Phase 11  (widget organization)         → 30min  → CLEAN
Phase 12  (standalone polish)           → 1h     → FEATURE
Phase 13  (async resilience)            → 30min  → SAFETY
Phase 14  (performance)                 → 30min  → PERF
```

**Estimated: 14 phases, ~12h total, 30-40 commits**
