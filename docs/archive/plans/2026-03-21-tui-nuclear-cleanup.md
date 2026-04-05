# TUI Nuclear Cleanup — Full Wiring + Dead Code Purge

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire all disconnected TUI data, purge 11 dead widgets, add catppuccin crate + tachyonfx effects, fix 5 critical UX bugs.

**Architecture:** The Nika TUI uses a 3-view architecture (Studio/Command/Control) with a token-based design system. Data flows from CLI runtime → EventLog → TuiState → Views → Widgets. This plan fixes broken connections in that pipeline and removes dead weight.

**Tech Stack:** Rust, ratatui 0.30, crossterm, catppuccin 2.5, tachyonfx 0.25

---

## Phase 1: Wire Missing Data (5 UX bugs)

### Task 1.1: StatusBar animation frame

**Files:**
- Modify: `tools/nika/src/tui/app/render.rs:157-161`

**Step 1: Add `.frame()` to StatusBar construction**

In `render.rs`, find the StatusBar construction (around line 157):
```rust
let status_bar = StatusBar::new(current_view, theme)
    .mode(input_mode)
    .metrics(metrics)
    .custom_text(status_text);
```

Change to:
```rust
let status_bar = StatusBar::new(current_view, theme)
    .mode(input_mode)
    .metrics(metrics)
    .custom_text(status_text)
    .frame(state.frame as u8);
```

`state.frame` is a `u32` (defined at `state/mod.rs:128`), StatusBar expects `u8`. The cast wraps naturally which is fine for animation cycling.

**Step 2: Build and verify**

Run: `cd tools/nika && cargo build --features tui 2>&1 | tail -3`
Expected: `Finished`

**Step 3: Commit**

```bash
git add tools/nika/src/tui/app/render.rs
git commit -m "fix(tui): wire animation frame to StatusBar

- StatusBar.frame was never set, causing frozen spinners
- Now receives state.frame for connecting/phase animations

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 1.2: MCP connection status — use real data

**Files:**
- Modify: `tools/nika/src/tui/app/render.rs:152-156`

**Step 1: Fix hardcoded connection status**

Find the hardcoded logic (around line 152):
```rust
.connection(if mcp_total > 0 {
    ConnectionStatus::Connected
} else {
    ConnectionStatus::Disconnected
});
```

Replace with real connection-aware logic:
```rust
.connection(if mcp_connected > 0 {
    ConnectionStatus::Connected
} else if mcp_total > 0 {
    ConnectionStatus::Connecting
} else {
    ConnectionStatus::Disconnected
});
```

`mcp_connected` comes from `self.mcp_pool.connected_count()` (real active connections). `mcp_total` comes from `self.mcp_pool.config_count()` (configured servers). When servers are configured but none connected → Connecting state.

**Step 2: Build and verify**

Run: `cargo build --features tui 2>&1 | tail -3`

**Step 3: Commit**

```bash
git add tools/nika/src/tui/app/render.rs
git commit -m "fix(tui): use real MCP connection state in StatusBar

- Was hardcoded Connected/Disconnected based on config count
- Now uses connected_count() for real connection state
- Shows Connecting when servers configured but not yet connected

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 1.3: Header context — make view-aware

**Files:**
- Modify: `tools/nika/src/tui/app/render.rs:104-110`

**Step 1: Make header context view-specific**

Find the static context (around line 104):
```rust
let workflow_name = std::path::Path::new(workflow_path)
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("No workflow");
let header = Header::new(current_view, theme)
    .context(workflow_name)
```

Replace with view-aware context:
```rust
let context_text = match current_view {
    TuiView::Studio => {
        let name = std::path::Path::new(workflow_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Standalone Mode");
        name
    }
    TuiView::Command => {
        let model = provider.unwrap_or("No provider");
        model
    }
    TuiView::Control => "Settings",
};
let header = Header::new(current_view, theme)
    .context(context_text)
```

Note: `provider` is already extracted at render.rs line 40 as `self.command_view.chat.provider()` which returns `Option<&str>`.

**Step 2: Build and verify**

Run: `cargo build --features tui 2>&1 | tail -3`

**Step 3: Commit**

```bash
git add tools/nika/src/tui/app/render.rs
git commit -m "fix(tui): show view-specific context in Header

- Studio: workflow filename or 'Standalone Mode'
- Command: current provider/model name
- Control: 'Settings'

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 1.4: MissionControl context_items — bridge from state

**Files:**
- Modify: `tools/nika/src/tui/views/chat/mod.rs:764-765`
- Modify: `tools/nika/src/tui/state/mod.rs` (ContextAssembled handler, around line 554)

**Step 1: Add context_items to TuiState**

In `state/mod.rs`, add a `context_items` field to `TuiState`:
```rust
pub context_items: Vec<(String, String)>, // (name, status)
```

Initialize it empty in `TuiState::new()`.

**Step 2: Populate in ContextAssembled handler**

In `state/mod.rs`, find the ContextAssembled handler (around line 554). After existing logic, add:
```rust
self.context_items.clear();
for source in &sources {
    self.context_items.push((source.clone(), "loaded".to_string()));
}
for excluded in &excluded {
    self.context_items.push((excluded.clone(), "excluded".to_string()));
}
```

**Step 3: Pass to ChatView render**

In `views/chat/mod.rs`, where MissionControlPanel is constructed (line 764), update:
```rust
// Before: .context(&self.context_items)
// After: pass from state
.context_from_state(&state.context_items)
```

Or simpler: populate `self.context_items` from state in the ChatView tick/render.

**Step 4: Build, test, commit**

Run: `cargo build --features tui && cargo test --lib -- tui 2>&1 | grep "test result"`

```bash
git commit -m "feat(tui): wire ContextAssembled data to MissionControl

- context_items now populated from ContextAssembled events
- MissionControl shows loaded/excluded context sources

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 1.5: ProStatusBar MCP sync

**Files:**
- Modify: `tools/nika/src/tui/views/chat/mod.rs` (where session_metrics is updated)

**Step 1: Sync MCP servers from session_context to session_metrics**

Find where `session_metrics` is used in `views/chat/mod.rs`. Add a sync call before ProStatusBar rendering:
```rust
// Sync MCP server info from session_context → session_metrics
self.session_metrics.mcp_servers = self.session_context.mcp_servers
    .iter()
    .map(|s| McpServerInfo {
        name: s.name.clone(),
        tool_count: s.tool_count,
        latency_ms: s.latency_ms,
    })
    .collect();
```

This bridges the data gap between the two structs.

**Step 2: Build, test, commit**

---

## CHECKPOINT 1: Verify all 5 UX bugs are fixed

Run: `cargo build --features tui && cargo test --lib -- tui 2>&1 | grep "test result"`

Verify:
- [ ] StatusBar spinners animate (frame is non-zero)
- [ ] MCP shows Connecting when servers configured but not connected
- [ ] Header shows view-specific context
- [ ] MissionControl shows context sources after ContextAssembled
- [ ] ProStatusBar shows MCP servers

---

## Phase 2: Nuclear Widget Cleanup (11 dead widgets)

### Task 2.1: Remove dead widget files

**Files to DELETE:**
- `tools/nika/src/tui/widgets/session_context.rs` — never rendered
- `tools/nika/src/tui/widgets/gauge.rs` — only test usage
- `tools/nika/src/tui/widgets/provider_selector.rs` — superseded by ProviderModal
- `tools/nika/src/tui/widgets/verb_input.rs` — VerbIndicator never rendered
- `tools/nika/src/tui/widgets/panels/info.rs` — never rendered
- `tools/nika/src/tui/widgets/panels/task_flow.rs` — never rendered
- `tools/nika/src/tui/widgets/panels/task_list.rs` — never rendered

**Step 1: Remove files and update mod.rs exports**

Delete each file, then update `widgets/mod.rs` to remove the corresponding `mod` declarations and `pub use` re-exports.

Update `widgets/panels/mod.rs` to remove dead panel modules.

**Step 2: Fix compilation errors**

Any remaining references to deleted types will cause compile errors. Fix each one:
- Remove `use` statements referencing deleted types
- Remove any test code that uses deleted widgets

**Step 3: Build and verify**

Run: `cargo build --features tui 2>&1 | tail -5`
Expected: Clean build with no errors

**Step 4: Run full test suite**

Run: `cargo test --lib -- tui 2>&1 | grep "test result"`
Expected: All tests pass (some test count reduction from removed widget tests)

**Step 5: Commit**

```bash
git commit -m "refactor(tui): remove 7 dead widget files

- SessionContextBar: never rendered (200+ lines)
- NikaGauge: only test usage
- ProviderSelector: superseded by ProviderModal
- VerbIndicator: never rendered
- InfoPanel, TaskFlowPanel, TaskListPanel: never rendered

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.2: Assess and decide on remaining dead widgets

These widgets have partial usage or future potential. For each, decide: DELETE or KEEP with `#[allow(dead_code)]` + justification comment.

| Widget | File | Recommendation |
|--------|------|---------------|
| AgentStepsWidget | `agent_steps.rs` | KEEP — future Monitor view integration |
| ActivityStack | `activity_stack.rs` | KEEP data types, mark widget as `#[cfg(test)]` |
| BrowserPanel | `panels/browser.rs` | KEEP — actively used in Studio file browser |
| MatrixDecrypt | `matrix_decrypt.rs` | KEEP — used in ChatView streaming |
| InferStreamBox | N/A | Check if this exists as separate file or is part of task_box |

**Step 1: Add justification comments to kept dead widgets**

For each kept widget, add a comment at the top:
```rust
// NOTE: Widget not currently rendered in any view.
// Kept for planned Monitor view Phase 5 integration.
// See: docs/plans/2026-03-21-tui-nuclear-cleanup.md Task 2.2
```

**Step 2: Commit**

---

## CHECKPOINT 2: Dead code purge complete

Run: `cargo build --features tui && cargo test --lib -- tui`

Verify:
- [ ] Build passes with zero errors
- [ ] All remaining TUI tests pass
- [ ] No new warnings from removed code
- [ ] `grep -r "session_context\.rs\|gauge\.rs\|provider_selector\|verb_input" tools/nika/src/tui/` returns only comments

---

## Phase 3: Catppuccin Crate + tachyonfx

### Task 3.1: Add catppuccin crate dependency

**Files:**
- Modify: `tools/nika/Cargo.toml`

**Step 1: Add dependency**

In `Cargo.toml`, add to `[dependencies]`:
```toml
catppuccin = { version = "2.5", features = ["ratatui"], optional = true }
```

Add to the `tui` feature list:
```toml
tui = ["dep:ratatui", ..., "dep:catppuccin"]
```

**Step 2: Replace hardcoded RGB with catppuccin constants**

In `tools/nika/src/tui/tokens/semantic.rs`, update `cosmic_dark()`:
```rust
pub fn cosmic_dark(_palette: &ColorPalette) -> Self {
    use catppuccin::PALETTE;
    let mocha = &PALETTE.mocha.colors;

    Self {
        bg_primary: mocha.base.into(),
        bg_secondary: mocha.mantle.into(),
        bg_tertiary: mocha.surface0.into(),
        bg_hover: mocha.surface1.into(),
        bg_active: mocha.surface2.into(),
        text_primary: mocha.text.into(),
        text_secondary: mocha.subtext1.into(),
        text_muted: mocha.subtext0.into(),
        text_disabled: mocha.overlay0.into(),
        text_inverse: mocha.crust.into(),
        border_default: mocha.surface0.into(),
        border_focused: mocha.lavender.into(),
        border_subtle: mocha.mantle.into(),
        accent_primary: mocha.lavender.into(),
        accent_secondary: mocha.blue.into(),
        accent_tertiary: mocha.sapphire.into(),
        status_success: mocha.green.into(),
        status_warning: mocha.yellow.into(),
        status_error: mocha.red.into(),
        status_info: mocha.blue.into(),
        verb_infer: mocha.mauve.into(),
        verb_exec: mocha.peach.into(),
        verb_fetch: mocha.sky.into(),
        verb_invoke: mocha.teal.into(),
        verb_agent: mocha.pink.into(),
        scrollbar_thumb: mocha.surface1.into(),
        scrollbar_track: mocha.mantle.into(),
        scrollbar_arrows: mocha.subtext0.into(),
    }
}
```

**Step 3: Update intro colors similarly**

In `widgets/nika_intro.rs`, update EXPLOSION_COLORS to use catppuccin:
```rust
use catppuccin::PALETTE;
lazy_static! or const block using PALETTE.mocha.colors
```

Note: catppuccin Color has `.into()` for ratatui Color via the `ratatui` feature flag.

**Step 4: Build, update tests, commit**

Run: `cargo build --features tui && cargo test --lib -- tui`
Fix any test assertions that check exact RGB values.

```bash
git commit -m "feat(tui): use catppuccin crate for Mocha palette

- Replace 26 hardcoded Color::Rgb() with catppuccin::PALETTE
- Ensures exact color match with official Catppuccin spec
- Intro particles use catppuccin accents

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 3.2: Add tachyonfx for view transitions

**Files:**
- Modify: `tools/nika/Cargo.toml`
- Modify: `tools/nika/src/tui/app/mod.rs`

**Step 1: Add dependency**

```toml
tachyonfx = { version = "0.25", optional = true }
```

Add to `tui` feature list.

**Step 2: Add basic fade effect on view switch**

In `app/routing.rs`, where `switch_to_view()` handles view changes, add a fade effect:
```rust
use tachyonfx::{fx, Effect, EffectManager};
```

This is a larger integration. Start with a simple dissolve on view switch:
- Store an `EffectManager` in App
- On view switch, trigger `fx::dissolve(300)` (300ms)
- In render, apply effects after main render via `effect_manager.process(elapsed, buf)`

**Step 3: Build, test, commit**

---

### Task 3.3: Add Catppuccin Light variant

**Files:**
- Modify: `tools/nika/src/tui/tokens/semantic.rs`

**Step 1: Update cosmic_light() to use Catppuccin Latte**

```rust
pub fn cosmic_light(_palette: &ColorPalette) -> Self {
    use catppuccin::PALETTE;
    let latte = &PALETTE.latte.colors;
    // Same structure as mocha but with latte colors
}
```

**Step 2: Build, test, commit**

---

## CHECKPOINT 3: Design system complete

Run: `cargo build --features tui && cargo test --lib -- tui`

Verify:
- [ ] catppuccin crate used for all theme colors
- [ ] `grep -r "Color::Rgb" tools/nika/src/tui/tokens/semantic.rs` returns zero hardcoded colors
- [ ] Theme cycling (Dark/Light) works with Catppuccin Mocha/Latte
- [ ] tachyonfx compiles (integration can be minimal for now)

---

## Phase 4: Event Handling Hardening

Based on the deep exploration, **all 41 EventKind variants already have match arms** in `state/mod.rs`. The earlier audit was incorrect — the code was updated. However, some handlers are minimal or drop data.

### Task 4.1: Capture dropped ProviderResponded fields

**Files:**
- Modify: `tools/nika/src/tui/state/mod.rs` (ProviderResponded handler, ~line 681)
- Modify: `tools/nika/src/tui/state/types.rs` (TaskState struct)

**Step 1: Add finish_reason to TaskState**

In `state/types.rs`, add to `TaskState`:
```rust
pub finish_reason: Option<String>,
```

**Step 2: Capture in handler**

In `state/mod.rs`, update the ProviderResponded handler to store `finish_reason`:
```rust
if let Some(task) = self.tasks.get_mut(&task_id) {
    task.finish_reason = Some(finish_reason.clone());
}
```

**Step 3: Build, test, commit**

---

### Task 4.2: Display cache_read_tokens in Metrics

**Files:**
- Modify: `tools/nika/src/tui/widgets/status_bar.rs` or ProStatusBar

**Step 1: Add cache token display**

When `metrics.cache_read_tokens > 0`, show it in the status bar:
```
Tokens: 1.2k (cache: 800)
```

**Step 2: Build, test, commit**

---

### Task 4.3: Wire McpResponse.cached for hit/miss tracking

**Files:**
- Modify: `tools/nika/src/tui/state/mod.rs` (McpResponse handler)
- Modify: `tools/nika/src/tui/state/types.rs`

**Step 1: Add cache tracking to Metrics**

```rust
pub mcp_cache_hits: u32,
pub mcp_cache_misses: u32,
```

**Step 2: Update McpResponse handler**

Instead of `cached: _`, capture and track:
```rust
if cached { self.metrics.mcp_cache_hits += 1; }
else { self.metrics.mcp_cache_misses += 1; }
```

**Step 3: Build, test, commit**

---

## CHECKPOINT 4: Full verification

Run complete test suite:
```bash
cd tools/nika
cargo build --features tui
cargo test --lib -- tui
cargo clippy --features tui -- -W clippy::all
```

Verify:
- [ ] Zero compile errors
- [ ] All TUI tests pass
- [ ] No new clippy warnings from changes
- [ ] StatusBar animations work (frame wired)
- [ ] MCP status reflects real connection state
- [ ] Header shows view-specific context
- [ ] Dead widgets removed (7 files deleted)
- [ ] catppuccin crate provides all theme colors
- [ ] ProviderResponded.finish_reason captured
- [ ] MCP cache hit/miss tracked

---

## Summary

| Phase | Tasks | Impact |
|-------|-------|--------|
| **1. Wire Data** | 5 tasks | Fix 5 critical UX bugs |
| **2. Dead Widgets** | 2 tasks | Remove ~1500 lines dead code |
| **3. Catppuccin + FX** | 3 tasks | Professional design system |
| **4. Events** | 3 tasks | Complete telemetry pipeline |
| **Total** | 13 tasks | ~25 commits |

Estimated time: 2-3 hours of focused implementation.
