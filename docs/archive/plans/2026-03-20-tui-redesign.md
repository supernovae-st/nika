# Nika TUI Redesign — Implementation Plan

> **Branch:** `feat/tui-redesign`
> **Created:** 2026-03-20
> **Status:** PLANNING
> **Scope:** 88,716 LOC across 164 files → 3-view architecture with nuclear cleanup

---

## Architecture Overview

### Current (4 views)
```
[1/s] Studio  [2/r] Runner  [3/c] Chat  [4/,] Settings
```

### Target (3 views)
```
[1/s] Studio  [2/c] Command  [3/x] Control
```

| View | Role | Composition |
|------|------|-------------|
| **Studio** | Build workflows | Files \| Editor \| DAG Preview (unchanged) |
| **Command** | Run + Chat + Monitor (FUSION) | Conversation/Execution Log \| Instruments Panel |
| **Control** | Infrastructure config | Providers \| MCP Servers \| Preferences (+ future: Model Slots, Packages, Memory) |

### Command View — Two Modes

| Mode | Trigger | Input bar? | Content |
|------|---------|------------|---------|
| **Execution** | `nika run workflow.yaml` | No | TaskBoxes streaming, event log |
| **Chat** | `nika ui` → tab 2 | Yes | Conversation + inline TaskBoxes |

### Instruments Panel (right side, collapsible with `[`)

| Panel | When visible | Status |
|-------|-------------|--------|
| DAG | Workflow running | **EXISTS** |
| Metrics | Any execution | **EXISTS** |
| MCP | MCP servers connected | **EXISTS** |
| Model Slots | *When implemented* | **FUTURE** |
| Records | *When implemented* | **FUTURE** |
| Satellites | *When implemented* | **FUTURE** |

---

## Phase 1 — Nuclear Cleanup

> **Goal:** Delete dead code, fix bugs, remove duplication. No behavior changes.
> **Estimated:** ~4,500 LOC deleted, ~200 LOC added (shared utilities)

### Batch 1.1 — Remove dead_code blankets & dead widgets

| # | Task | File(s) | LOC impact |
|---|------|---------|------------|
| 1.1.1 | Remove `#![allow(dead_code)]` from `widgets/mod.rs` | `widgets/mod.rs:15` | 0 (reveals warnings) |
| 1.1.2 | Remove `#![allow(dead_code)]` from `views/chat/mod.rs` | `views/chat/mod.rs:21` | 0 (reveals warnings) |
| 1.1.3 | Delete `dag.rs` widget (dead, DagAscii replaces it) | `widgets/dag.rs` | -987 |
| 1.1.4 | Delete `HomeView` (redundant with Studio browser) | `views/home.rs` | -1,301 |
| 1.1.5 | Delete `ProviderSelector` widget (replaced by ProviderModal) | `widgets/provider_selector.rs` | -935 |
| 1.1.6 | Delete `ActivityStack` widget body (keep data types) | `widgets/activity_stack.rs` | ~-150 |
| 1.1.7 | Delete `DownloadProgress` widget (never rendered) | `widgets/progress/download_progress.rs` | ~-500 |
| 1.1.8 | Delete `TaskProgress` widget (never rendered) | `widgets/progress/task_progress.rs` | ~-560 |
| 1.1.9 | Delete `NikaIntro` widget (keep `NikaIntroState` only) | `widgets/nika_intro.rs` | ~-350 |
| 1.1.10 | Remove unused `solarized` module constants | `theme.rs:54-80` | ~-20 |

**Verification:** `cargo check --features tui 2>&1 | grep -c warning` before and after. Fix all new warnings.

### Batch 1.2 — Deduplicate

| # | Task | Files | LOC impact |
|---|------|-------|------------|
| 1.2.1 | Extract `centered_rect()` into `widgets/utils.rs`, delete 3 copies | `views/chat/mod.rs:962`, `views/chat/mode_config.rs:116`, `views/studio.rs:2710` | +20, -60 |
| 1.2.2 | Unify 3 message role enums → 1 `MessageRole` | `chat_agent.rs:116`, `views/chat/types.rs:20`, `state/chat_overlay.rs:14` | +1 canonical, -2 duplicates |
| 1.2.3 | Unify 2 `ChatSession` structs → 1 in `views/chat/types.rs` | `views/chat/types.rs:271`, `session.rs:37` | +0, -~100 |
| 1.2.4 | Delete `ChatOverlayState` (replaced by ChatView) | `state/chat_overlay.rs` | -329 |
| 1.2.5 | Delete old `session.rs` overlay persistence (ChatView has its own) | `session.rs` | -458 |
| 1.2.6 | Merge `DagEdge` + `ChatEdgeLine` → shared edge renderer | `widgets/dag_edge.rs`, `widgets/chat_edge_line.rs` | ~-250 |

**Verification:** `cargo test --features tui` must pass. No behavior changes.

### Batch 1.3 — Bug fixes

| # | Task | File | Issue |
|---|------|------|-------|
| 1.3.1 | Fix `PanelId` collision | `state/types.rs` vs `focus.rs` | Two conflicting enums |
| 1.3.2 | Fix `ScrollToTop`/`ScrollToBottom` | `app/routing.rs` | Maps to same action as ScrollUp/Down |
| 1.3.3 | Fix `ChatModelSwitch` | `app/routing.rs` | Only shows status message, doesn't switch |
| 1.3.4 | Delete ~20 empty `Action::*` match arms | `app/routing.rs` | `// TODO` stubs with no implementation |
| 1.3.5 | Remove `HomeView` from App struct | `app/mod.rs`, `app/lifecycle.rs` | `Option<HomeView>` field, conditional ticking |

**Verification:** `cargo clippy --features tui` clean. Manual test: scroll, model switch, keyboard navigation.

### Batch 1.4 — Remove blanket allows, fix remaining warnings

| # | Task | Description |
|---|------|-------------|
| 1.4.1 | Audit all `#[allow(dead_code)]` annotations | Replace blanket allows with targeted per-item `#[allow]` or delete truly dead code |
| 1.4.2 | Fix `unused` warnings revealed by 1.1.1 and 1.1.2 | Delete or `#[allow]` with justification comment |
| 1.4.3 | Remove `focus_state` dead field from `App` | `app/mod.rs:103` — never read |
| 1.4.4 | Remove `llm_response_tx` dead field from `App` | `app/mod.rs:119-120` — sender held but never used |
| 1.4.5 | Remove `config` dead field from `App` | `app/mod.rs:141-142` — loaded but never read |

**Verification:** `cargo check --features tui` with zero warnings. `cargo test --features tui` passes.

---

## Phase 2 — State Architecture Refactor

> **Goal:** Decompose God Objects. No behavior changes.
> **Estimated:** ~0 LOC change (restructure, not rewrite)

### Batch 2.1 — TuiState decomposition

| # | Task | File | Details |
|---|------|------|---------|
| 2.1.1 | Extract `handle_workflow_event()` from `handle_event()` | `state/mod.rs` | WorkflowStarted/Completed/Failed/Aborted/Paused/Resumed |
| 2.1.2 | Extract `handle_task_event()` | `state/mod.rs` | TaskScheduled/Started/Completed/Failed |
| 2.1.3 | Extract `handle_mcp_event()` | `state/mod.rs` | McpInvoke/Response/Connected/Error/Retry |
| 2.1.4 | Extract `handle_agent_event()` | `state/mod.rs` | AgentStart/Turn/Complete/Spawned |
| 2.1.5 | Extract `handle_provider_event()` | `state/mod.rs` | ProviderCalled/Responded |
| 2.1.6 | Extract `handle_media_event()` | `state/mod.rs` | All Media* + Vision* events |
| 2.1.7 | Extract `handle_telemetry_event()` | `state/mod.rs` | Log/Custom/Artifact/Http/StructuredOutput/Guardrail |

**Target:** `handle_event()` becomes a ~50-line match that delegates to 7 handler methods.

### Batch 2.2 — ChatView decomposition

| # | Task | New struct | Fields moved |
|---|------|-----------|-------------|
| 2.2.1 | Extract `ChatScrollState` | `views/chat/scroll_state.rs` | scroll, scroll_velocity, scroll_accumulator, scroll_animating, user_at_bottom, conversation_scroll, activity_scroll |
| 2.2.2 | Extract `ChatAnimState` | `views/chat/anim_state.rs` | streaming_decrypt, matrix_effect_enabled, rain_opacity, rain_fading, intro_state, explosion_frame, nika_pattern_visible |
| 2.2.3 | Extract `ChatSearchState` | `views/chat/search_state.rs` | search_mode, search_query, search_results, search_current |
| 2.2.4 | Extract `ChatDagState` | `views/chat/dag_state.rs` | show_dag_panel, dag_nodes, dag_edges, task_queue, dag_selected |
| 2.2.5 | Extract `ChatSelectionState` | `views/chat/selection_state.rs` | text_selection, is_selecting, line_positions |
| 2.2.6 | Extract `ChatThinkingState` | `views/chat/thinking_state.rs` | thinking_collapsed, thinking_expanded_default, message_id_counter |
| 2.2.7 | Extract `ChatProviderState` | `views/chat/provider_state.rs` | current_model, cached_provider, provider_name, current_provider_id |

**Target:** ChatView goes from 76 flat pub fields → 7 sub-structs + ~20 remaining core fields.

### Batch 2.3 — File splits

| # | Task | Source | Target files |
|---|------|--------|-------------|
| 2.3.1 | Split `studio.rs` (3,433 LOC) | `views/studio.rs` | `views/studio/mod.rs`, `views/studio/text_buffer.rs`, `views/studio/syntax.rs`, `views/studio/keys.rs`, `views/studio/render.rs` |
| 2.3.2 | Split `theme.rs` (1,817 LOC) | `theme.rs` | `theme/mod.rs`, `theme/palette.rs`, `theme/verb_color.rs`, `theme/mission_phase.rs` |
| 2.3.3 | Move tab enums to state | `views/mod.rs` → `state/types.rs` | `DagTab`, `MissionTab`, `NovanetTab`, `ReasoningTab` |

### Batch 2.4 — Module grouping

| # | Task | Files moved |
|---|------|-------------|
| 2.4.1 | Create `tui/editor/` module | `edit_history.rs`, `selection.rs`, `diagnostics.rs` → `editor/` |
| 2.4.2 | Create `tui/interaction/` module | `focus.rs`, `mode.rs`, `keybindings.rs` → `interaction/` |
| 2.4.3 | Move `chat_agent.rs` under `views/chat/` | `chat_agent.rs` → `views/chat/agent.rs` |

**Verification:** `cargo test --features tui` passes. `cargo clippy` clean. No behavior changes.

---

## Phase 3 — View Architecture (3 Views)

> **Goal:** Implement 3-view navigation. Studio unchanged, Command = stub, Control = stub.
> **Estimated:** ~500 LOC new, ~300 LOC modified

### Batch 3.1 — View enum and navigation

| # | Task | File | Details |
|---|------|------|---------|
| 3.1.1 | Change `TuiView` enum | `views/mod.rs` | `Studio`, `Command`, `Control` (remove `Runner`, `Chat`, `Settings`) |
| 3.1.2 | Update navigation keys | `app/events.rs` | `1/s` → Studio, `2/c` → Command, `3/x` → Control |
| 3.1.3 | Update header rendering | `widgets/header.rs` | 3 tabs instead of 4 |
| 3.1.4 | Update status bar | `widgets/status_bar.rs` | 3-view shortcuts |

### Batch 3.2 — CommandView scaffold

| # | Task | File | Details |
|---|------|------|---------|
| 3.2.1 | Create `views/command/mod.rs` | New | `CommandView` struct, `View` trait impl |
| 3.2.2 | Create `views/command/render.rs` | New | 2-column layout: conversation (65%) + instruments (35%) |
| 3.2.3 | Create `views/command/keys.rs` | New | Key handling (delegate to chat + instruments) |
| 3.2.4 | Create `views/command/instruments.rs` | New | Instruments panel (DAG + Metrics + MCP panels) |
| 3.2.5 | Create `views/command/mode.rs` | New | `CommandMode::Execution` vs `CommandMode::Chat` |

### Batch 3.3 — ControlView scaffold

| # | Task | File | Details |
|---|------|------|---------|
| 3.3.1 | Create `views/control/mod.rs` | New | `ControlView` struct, `View` trait impl |
| 3.3.2 | Create `views/control/render.rs` | New | Vertical sections: Providers, MCP, Preferences |
| 3.3.3 | Create `views/control/keys.rs` | New | Section navigation (j/k), theme switching |

### Batch 3.4 — Wire views into App

| # | Task | File | Details |
|---|------|------|---------|
| 3.4.1 | Replace `chat_view` + `monitor_view` with `command_view` in `App` | `app/mod.rs` | Single `CommandView` field |
| 3.4.2 | Replace `settings_view` with `control_view` in `App` | `app/mod.rs` | Single `ControlView` field |
| 3.4.3 | Update `render_unified_frame()` | `app/render.rs` | Match on 3 views |
| 3.4.4 | Update `handle_unified_key()` routing | `app/events.rs` | Delegate to 3 views |
| 3.4.5 | Update CLI entry points | `tui/mod.rs` | `run_tui_chat()` → opens Command, `--view runner` → opens Command |

**Verification:** TUI launches with 3 tabs. Studio works. Command shows placeholder. Control shows placeholder.

---

## Phase 4 — Command View Fusion

> **Goal:** Merge Chat + Runner functionality into Command view.
> **Estimated:** ~2,000 LOC new, ~1,000 LOC moved from ChatView + MonitorView

### Batch 4.1 — Conversation timeline (left panel)

| # | Task | Details |
|---|------|---------|
| 4.1.1 | Move ChatView conversation rendering into `command/conversation.rs` | Messages, input bar, inline TaskBoxes |
| 4.1.2 | Add execution event rendering | Runtime events (TaskStarted/Completed/Failed) appear as timeline entries |
| 4.1.3 | Add workflow header entry | `▶ workflow-name ── N tasks ── timestamp` when workflow starts |
| 4.1.4 | Integrate streaming display | StreamChunk → matrix decrypt → inline in conversation |
| 4.1.5 | Wire all 5 /verb commands | `/infer`, `/exec`, `/fetch`, `/invoke`, `/agent` → ChatAgent (already working) |
| 4.1.6 | Wire `/run workflow.yaml` command | Parse path, start runner, display inline |

### Batch 4.2 — Instruments panel (right panel)

| # | Task | Details |
|---|------|---------|
| 4.2.1 | Create `InstrumentPanel` trait | `fn render()`, `fn is_visible()`, `fn priority()` |
| 4.2.2 | Create `DagInstrument` | Wraps existing DagAscii, updates from runtime events |
| 4.2.3 | Create `MetricsInstrument` | Tokens, cost, time, task progress bar |
| 4.2.4 | Create `McpInstrument` | Connected MCP servers, latency sparklines |
| 4.2.5 | Implement instruments stack renderer | Auto-layout visible instruments vertically |
| 4.2.6 | Implement `[` key to toggle instruments panel | Full-width conversation when collapsed |

### Batch 4.3 — Mode switching

| # | Task | Details |
|---|------|---------|
| 4.3.1 | Execution mode (no input bar) | When `nika run` or `/run`, hide input bar, show execution log |
| 4.3.2 | Chat mode (with input bar) | Default when entering Command via `nika ui` |
| 4.3.3 | Transition: execution completes → show input bar | After WorkflowCompleted event, enable chat mode |

### Batch 4.4 — Event routing

| # | Task | Details |
|---|------|---------|
| 4.4.1 | Route broadcast events to Command view | `poll_runtime_events()` updates both TuiState AND Command instruments |
| 4.4.2 | Route stream chunks to Command view | `poll_stream_chunks()` feeds conversation timeline |
| 4.4.3 | Handle concurrent chat + execution | User can chat while workflow runs (non-blocking) |

**Verification:** `nika run workflow.yaml` shows execution in Command view. `nika ui` shows chat. `/run` from chat starts inline execution. All 5 /verbs work. Instruments panel shows DAG + metrics.

---

## Phase 5 — Visual Redesign

> **Goal:** New color palette, Evangelion-inspired chrome, consistent styling.
> **Estimated:** ~800 LOC modified in theme/tokens

### Batch 5.1 — New Cosmic Blue-Violet-Orange palette

| # | Task | Details |
|---|------|---------|
| 5.1.1 | Replace base colors in `tokens/colors.rs` | Base: `#0C0E1A`, Surface: `#141829`, Elevated: `#1E2340` |
| 5.1.2 | Update semantic colors in `tokens/semantic.rs` | Primary: blue-500 `#3B82F6`, Secondary: violet-500 `#8B5CF6`, Tertiary: orange-500 `#F59E0B` |
| 5.1.3 | Update verb colors | Infer=Violet, Exec=Orange, Fetch=Cyan, Invoke=Emerald, Agent=Rose |
| 5.1.4 | Retire old `Theme` struct | Replace all consumers with `TokenResolver`, delete `theme.rs` old struct, `cosmic_theme.rs` adapter, `tokens/compat.rs` |

### Batch 5.2 — Evangelion-inspired chrome

| # | Task | Details |
|---|------|---------|
| 5.2.1 | Update header bar | Technical font style, blue-violet gradient, Nika butterfly icon |
| 5.2.2 | Update status bar | Metrics density, orange highlights for active states |
| 5.2.3 | Update border styles | Focused = blue glow, unfocused = subtle slate, running = orange pulse |
| 5.2.4 | Update TaskBox borders | Verb-colored borders with status-dependent intensity |

### Batch 5.3 — Theme variants

| # | Task | Details |
|---|------|---------|
| 5.3.1 | Cosmic Dark (default) | Deep blue-black base, high contrast |
| 5.3.2 | Cosmic Violet | Violet-950 base, full brand accent |
| 5.3.3 | Cosmic Light | Light base for bright environments |

**Verification:** All 3 themes render correctly. Colors match spec. No readability issues.

---

## Phase 6 — Telemetry Completeness

> **Goal:** Handle all 41 EventKind variants in the TUI. Display all telemetry data.
> **Estimated:** ~400 LOC new

### Batch 6.1 — Handle ignored events

| # | EventKind | Action |
|---|-----------|--------|
| 6.1.1 | `StructuredOutputAttempt` | Track in TuiState, show layer attempts in task detail |
| 6.1.2 | `StructuredOutputSuccess` | Show validation success in task detail |
| 6.1.3 | `GuardrailPassed` | Track in security audit trail |
| 6.1.4 | `GuardrailFailed` | Show inline warning in conversation |
| 6.1.5 | `GuardrailEscalation` | Show critical alert notification |

### Batch 6.2 — Improve partial events

| # | EventKind | Current | Target |
|---|-----------|---------|--------|
| 6.2.1 | `ContextAssembled` | State stored, not rendered | Show in task detail (sources, budget %) |
| 6.2.2 | `TemplateResolved` | State stored, not rendered | Show in execution log |
| 6.2.3 | `MediaProcessed` | Ignored | Track in metrics (media count) |
| 6.2.4 | `MediaStored` | Ignored | Track in metrics (CAS dedup ratio) |
| 6.2.5 | `HttpRequest/Response` | Notification only | Aggregate in metrics (latency, status codes) |

**Verification:** Run workflow with structured output + guardrails + media. All events visible in Command view.

---

## File Impact Summary

### Files to DELETE (Phase 1)

| File | LOC | Reason |
|------|-----|--------|
| `widgets/dag.rs` | 987 | Dead code (DagAscii replaces) |
| `views/home.rs` | 1,301 | Redundant with Studio browser |
| `widgets/provider_selector.rs` | 935 | Replaced by ProviderModal |
| `widgets/progress/download_progress.rs` | ~500 | Never rendered |
| `widgets/progress/task_progress.rs` | ~560 | Never rendered |
| `state/chat_overlay.rs` | 329 | Replaced by ChatView |
| `session.rs` (top-level) | 458 | Replaced by `views/chat/session.rs` |
| **Total deleted** | **~5,070** | |

### Files to CREATE (Phase 3-4)

| File | Purpose |
|------|---------|
| `views/command/mod.rs` | CommandView struct + View trait |
| `views/command/render.rs` | 2-column layout rendering |
| `views/command/keys.rs` | Key handling |
| `views/command/instruments.rs` | Instruments panel (DAG + Metrics + MCP) |
| `views/command/conversation.rs` | Conversation timeline rendering |
| `views/command/mode.rs` | Execution vs Chat mode |
| `views/control/mod.rs` | ControlView struct + View trait |
| `views/control/render.rs` | Sections rendering |
| `views/control/keys.rs` | Key handling |
| `widgets/utils.rs` | Shared `centered_rect()` |

### Files to SPLIT (Phase 2)

| Source | Target |
|--------|--------|
| `views/studio.rs` (3,433) | `views/studio/{mod,text_buffer,syntax,keys,render}.rs` |
| `theme.rs` (1,817) | `theme/{mod,palette,verb_color,mission_phase}.rs` |
| `state/mod.rs` (1,801) | Extract 7 handler methods (stay in same file, just decompose) |

### Files to MOVE (Phase 2)

| Source | Destination |
|--------|-------------|
| `chat_agent.rs` | `views/chat/agent.rs` |
| `edit_history.rs` | `editor/edit_history.rs` |
| `selection.rs` | `editor/selection.rs` |
| `diagnostics.rs` | `editor/diagnostics.rs` |
| `focus.rs` | `interaction/focus.rs` |
| `mode.rs` | `interaction/mode.rs` |
| `keybindings.rs` | `interaction/keybindings.rs` |

---

## Dependency Order

```
Phase 1 (Cleanup)
    ↓ no behavior changes, just deletion
Phase 2 (State Refactor)
    ↓ no behavior changes, just restructure
Phase 3 (3 Views)
    ↓ navigation works, Command/Control are stubs
Phase 4 (Command Fusion)
    ↓ Command view fully functional
Phase 5 (Visual Redesign)
    ↓ new palette applied
Phase 6 (Telemetry)
    ↓ all events displayed
```

Each phase is independently shippable. Phases 1-2 are pure refactoring (no behavior changes). Phase 3 is the architectural switch. Phase 4 is the main feature work. Phases 5-6 are polish.

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Breaking existing tests | Run `cargo test --features tui` after every batch |
| Missing feature during merge | Feature matrix tracks every working feature |
| Performance regression | Benchmark render time before/after Phase 4 |
| Theme readability | Manual review all 3 variants on light + dark terminals |
| Dead code identification false positive | Only delete after confirming no imports via `cargo check` |

---

## Success Criteria

- [ ] `cargo check --features tui` — zero warnings
- [ ] `cargo test --features tui` — all tests pass
- [ ] `cargo clippy --features tui` — clean
- [ ] 3 views navigate correctly (1/s, 2/c, 3/x)
- [ ] `nika run workflow.yaml` shows execution in Command view
- [ ] `nika ui` opens Studio, tab 2 opens Command with chat
- [ ] `/infer`, `/exec`, `/fetch`, `/invoke`, `/agent` all work in Command
- [ ] `/run workflow.yaml` from chat shows inline execution
- [ ] Instruments panel shows DAG + Metrics + MCP
- [ ] `[` toggles instruments panel
- [ ] All 3 theme variants render correctly
- [ ] ~5,000 LOC net reduction from cleanup
