# TUI UX Session — Fix BROKEN + CONFUSING + key MISSING features

> **Copy-paste into a fresh Claude Code chat. ultrathink, full autonomy.**

---

## Context

Nika TUI is a 90K LOC ratatui terminal app at `/Users/thibaut/dev/supernovae/nika/tools/`.
3-view architecture (Studio/Command/Control), 7 cloud providers, 60 FPS adaptive.

The TUI is **technically solid** (zero crash bugs after 9 audit waves) but has **UX gaps**
that make it confusing for new users. This session fixes the 4 BROKEN items, the 3
CONFUSING items, and adds 2 critical MISSING features.

---

## PHASE 1 — BROKEN (must fix)

### 1.1 Startup loading indicator
**Problem**: 1-2s blank screen while `verify_startup()` + `on_enter()` run.
**File**: `nika-tui/src/app/mod.rs` (run_unified loop), `nika-tui/src/lib.rs`
**Fix**: Render a skeleton frame with "Loading..." spinner BEFORE any blocking I/O.
The app already does `on_enter()` after first frame — verify this actually works.
If the first frame is empty, add a centered "◐ Starting Nika..." text.

### 1.2 API key missing → visible feedback
**Problem**: If no API key is set, Chat mode silently does nothing. No error, no modal.
**File**: `nika-tui/src/views/chat/mod.rs`, `nika-tui/src/chat_agent/mod.rs`
**Fix**: When ChatAgent fails to initialize due to missing API key, show a
status bar message: "No API key configured — press Ctrl+P to set up a provider"
in amber/yellow. Check `ChatView::new()` — if provider detection returns "none",
set a `no_provider_warning: bool` flag and render it prominently.

### 1.3 Error messages visible in status bar
**Problem**: Errors blend into metrics text, users miss them.
**File**: `nika-tui/src/widgets/status_bar.rs`
**Fix**: When `status_message` is set and contains "error" or starts with "NIKA-",
render it in RED with bold, taking priority over metrics text (push metrics right).
Add a `status_severity: Option<StatusSeverity>` field (Info/Warning/Error).

### 1.4 MCP connection error feedback
**Problem**: MCP failures are logged but not shown in TUI.
**File**: `nika-tui/src/state/event_handler.rs`
**Fix**: When `EventKind::McpResponse { is_error: true, .. }` arrives, set
`status_message` with the error. When MCP reconnect fails, show warning in
status bar: "MCP: novanet disconnected — retrying..."

---

## PHASE 2 — CONFUSING (should fix)

### 2.1 Ctrl+P dual behavior
**Problem**: Ctrl+P = provider modal in Command view, fuzzy search in Studio view.
**Fix**: Keep Ctrl+P as provider modal EVERYWHERE. Use Ctrl+F for fuzzy file search
in Studio (matching IDE convention). Update keybindings.rs and help text.
OR: Just document the dual behavior clearly in the help overlay.

### 2.2 Help overlay on first launch
**Problem**: Users don't know to press `?` for help.
**File**: `nika-tui/src/views/chat/mod.rs` (or app/mod.rs)
**Fix**: On first launch (check `~/.nika/tui_first_run` marker), show a brief
welcome bar at top: "Press ? for help, Ctrl+P for providers, 1/2/3 to switch views"
that auto-dismisses after 10 seconds or on first keypress. Write the marker file
after first display so it only shows once.

### 2.3 Status bar priority ordering
**Problem**: 6 hints crammed together, important ones get hidden on narrow terminals.
**File**: `nika-tui/src/widgets/status_bar.rs`
**Fix**: Prioritize hints by importance:
1. Error/warning message (always visible, red/amber)
2. Mode indicator (Insert/Normal)
3. Provider + model
4. Tokens + cost
5. MCP status
6. Keybinding hints (only if space allows)
Use progressive disclosure: on narrow terminals, drop low-priority items.

---

## PHASE 3 — MISSING (critical features)

### 3.1 Search in output (Ctrl+F)
**Problem**: No way to search chat history or monitor output.
**File**: `nika-tui/src/views/chat/mod.rs` (or new search overlay)
**Fix**: Add a search mode:
- Ctrl+F opens search bar at bottom of conversation panel
- Type query → highlight matches in yellow
- Enter/n = next match, N = previous
- Esc = close search
- Keep it simple: substring search, case-insensitive
This needs a new `SearchState { query: String, matches: Vec<(usize, usize)>, current: usize }`
and a render overlay that highlights matches in the conversation buffer.

### 3.2 Copy panel output
**Problem**: Can't copy task output from monitor panels.
**File**: `nika-tui/src/views/monitor/` panels
**Fix**: Add `y` (yank) keybinding in Monitor view:
- When a task is selected in Mission Control panel, `y` copies its output to clipboard
- Show brief "Copied!" toast in status bar (auto-dismiss after 2s)
- Use `arboard` crate (already in deps) for clipboard access

---

## PHASE 4 — Polish

### 4.1 Welcome message in empty chat
**Problem**: Empty chat shows nothing — user doesn't know what to do.
**File**: `nika-tui/src/views/chat/render.rs`
**Fix**: When chat history is empty, render centered welcome:
```
🦋 Nika Chat

Type a message or use a slash command:
  /infer "prompt"     — LLM generation
  /exec "command"     — Shell execution
  /fetch url          — HTTP request
  /invoke tool params — MCP tool call
  /agent "goal"       — Multi-turn agent

Press i to start typing, ? for help
```

### 4.2 Provider status in welcome
When no provider is configured, add to the welcome:
```
⚠ No API key detected — press Ctrl+P to configure a provider
```

---

## Rules

- `cargo check --workspace && cargo clippy --workspace -- -D warnings` after EVERY edit
- `cargo test --workspace --lib` before EVERY commit
- Pre-commit hook uses git stash/pop — stage ALL related files in one `git add`
- Commits: `type(scope): desc` with both co-authors
- 1 FIX = 1 COMMIT
- Use `spn-rust:rust` skill before writing Rust code
- Read files before editing
- Test edge cases: empty state, narrow terminal, no API keys

## Commit Plan (8 commits)

| # | Message | Files |
|---|---------|-------|
| 1 | `fix(tui): add startup loading indicator` | app/mod.rs |
| 2 | `fix(tui): show API key missing warning in chat view` | views/chat/mod.rs |
| 3 | `fix(tui): prioritize errors in status bar with severity colors` | widgets/status_bar.rs |
| 4 | `fix(tui): show MCP errors in status bar` | state/event_handler.rs |
| 5 | `fix(tui): add first-launch help hint with auto-dismiss` | app/mod.rs or views/ |
| 6 | `fix(tui): reorder status bar hints by priority` | widgets/status_bar.rs |
| 7 | `feat(tui): add Ctrl+F search in chat conversation` | views/chat/ (new search module) |
| 8 | `feat(tui): add yank (y) to copy task output in monitor` | views/monitor/ |

Bonus if time:
| 9 | `feat(tui): welcome message in empty chat` | views/chat/render.rs |
| 10 | `feat(tui): provider status in welcome` | views/chat/render.rs |
