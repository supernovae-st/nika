# Nika v0.9-v0.12 Roadmap — Redesigned

> **For Claude:** Each version is a standalone release. Use TDD, WIRING checkpoints, and subagent-driven-development.

**Goal:** Balanced release train with functional names, parallel execution where possible.

**Date:** 2026-02-25
**Author:** Brainstorming session (Claude + Thibaut)

---

## Version Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0.9 → v0.12 RELEASE TRAIN (REDESIGNED)                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ━━━ v0.9 "Chat-as-DAG" ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  Core DAG infrastructure for chat messages                                    ║
║                                                                               ║
║  v0.9.0  🏗️  StableGraph Foundation  │ FlowGraph → StableGraph migration      ║
║  v0.9.1  💬 ChatWorkflow Struct      │ DAG wrapper for chat messages          ║
║  v0.9.2  🔗 @mention Bindings        │ Parser + WiringSpec generation         ║
║  v0.9.3  🛠️  Builtin Tools (6 nika:*) │ sleep, log, emit, assert, prompt, run  ║
║                                                                               ║
║  ━━━ v0.10 "TaskBox" ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  TUI widgets + DAG visualization                                              ║
║                                                                               ║
║  v0.10.0 📦 NodeBox Widget           │ Task node visualization                 ║
║  v0.10.1 ➡️  EdgeLine Widget          │ Dependency arrows + flow                ║
║  v0.10.2 📋 TaskQueue Widget         │ Pending/running/completed queue         ║
║  v0.10.3 📊 ChatDagPanel             │ Integrated DAG sidebar                  ║
║  v0.10.4 ✨ Animation Polish         │ Pulses, flow effects, Ctrl+D toggle     ║
║                                                                               ║
║  ━━━ v0.11 "Six Views" ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  6-view TUI architecture                                                      ║
║                                                                               ║
║  v0.11.0 📁 Explorer View            │ Refactor Home → Explorer               ║
║  v0.11.1 ✏️  Editor View              │ Refactor Studio → Editor               ║
║  v0.11.2 🚀 Runner View              │ Refactor Monitor → Runner              ║
║  v0.11.3 ⏰ Scheduler View (NEW)     │ Workflow scheduling + cron             ║
║  v0.11.4 ⚙️  Settings View (NEW)      │ Embed Provider Modal components        ║
║  v0.11.5 🧭 Navigation Update        │ 6 views, hotkeys 1-6                   ║
║                                                                               ║
║  ━━━ v0.12 "Providers" (PARALLEL) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ ║
║  Provider ecosystem completion (can run parallel to v0.10)                    ║
║                                                                               ║
║  v0.12.0 🔐 Keyring Wiring           │ Wire NikaKeyring::set() to handlers    ║
║  v0.12.1 📦 Env Migration            │ .env → keyring migration utility       ║
║  v0.12.2 ✅ Provider Auto-Select     │ Enter key selects provider             ║
║  v0.12.3 🦙 Ollama Enhancement       │ Model suggestions + input prompt       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Dependency Graph

```
     v0.8.9 (done)
        │
        ▼
     v0.9 "Chat-as-DAG" ✅ COMPLETE (2026-02-25)
        │
        ├──────────────────────────────┐
        ▼                              ▼
     v0.10 "TaskBox"            v0.12 "Providers"
        │                         (PARALLEL)
        │                              │
        ▼                              │
     v0.11 "Six Views" ◄───────────────┘
        │
        ▼
     v0.13+ (Future)
```

### Dependency Table

| Version | Depends On | Blocking Feature | Can Parallelize? |
|---------|------------|------------------|------------------|
| v0.9 | v0.8.9 | None - builds on stable | ❌ Sequential |
| v0.10 | v0.9 | StableGraph for DAG widgets | ❌ After v0.9 |
| v0.11 | v0.10, v0.12 (soft) | TaskBox for Runner, Settings embeds Provider Modal | ❌ After v0.10+v0.12 |
| v0.12 | v0.9 | None - Provider Modal is independent | ✅ Parallel with v0.10 |

**Key Insight:** v0.12 "Providers" can start immediately after v0.9, running in parallel with v0.10 "TaskBox". This saves ~3 days.

---

## v0.9 "Chat-as-DAG" — Core DAG Infrastructure ✅ COMPLETE

**Focus:** DAG infrastructure for chat messages
**Total:** 32 tasks, 131 tests planned → **251 tests actual** (+120 bonus)
**Status:** ✅ RELEASED 2026-02-25

### v0.9.0 — StableGraph Foundation ✅ DONE

**Tasks:** 6 | **Tests:** 25 planned → **17 actual** | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 0.1 | Create `dag/stable.rs` with StableGraph wrapper | 5 |
| 0.2 | Implement `add_node()` with NodeIndex stability | 4 |
| 0.3 | Implement `remove_node()` preserving other indices | 4 |
| 0.4 | Implement `add_edge()` with EdgeIndex | 4 |
| 0.5 | Migrate FlowGraph to use StableGraph | 4 |
| 0.6 | Update all FlowGraph tests | 4 |

**WIRING:** `FlowGraph` → existing DAG validation
**Live Test:** `cargo test dag::` must pass

---

### v0.9.1 — ChatWorkflow Struct ✅ DONE

**Tasks:** 6 | **Tests:** 21 planned → **45 actual** | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 1.1 | Create `runtime/chat_workflow.rs` module | 4 |
| 1.2 | Implement `add_message()` → node creation | 4 |
| 1.3 | Implement `get_message_by_index()` | 3 |
| 1.4 | Implement auto-edge creation (sequential) | 4 |
| 1.5 | Add message counter for @N references | 3 |
| 1.6 | Thread-safety with `parking_lot::Mutex` | 3 |

**WIRING:** `ChatWorkflow` → `FlowGraph` internal DAG
**Live Test:** `cargo run -- chat`, verify messages create nodes

---

### v0.9.2 — @mention Binding System ✅ DONE

**Tasks:** 10 | **Tests:** 40 planned → **58 actual** | **Effort:** 2 sessions

| Task | Description | Tests |
|------|-------------|-------|
| 2.1 | Create `binding/mention.rs` module | 3 |
| 2.2 | Implement `Mention` enum (Number, Last, All, Range) | 4 |
| 2.3 | Implement `parse_mentions()` regex parser | 6 |
| 2.4 | Implement `@last` resolution | 3 |
| 2.5 | Implement `@all` resolution | 3 |
| 2.6 | Implement `@N..M` range resolution | 4 |
| 2.7 | Implement `//` parallel marker | 3 |
| 2.8 | Create `mentions_to_wiring()` converter | 4 |
| 2.9 | Integrate with ChatWorkflow | 3 |
| 2.10 | Add edge creation from mentions | 7 |

**WIRING:** `MentionParser` → `ChatWorkflow` → `WiringSpec`
**Live Test:** `cargo run -- chat`, test `@1`, `@last`, `//` syntax

---

### v0.9.3 — Builtin Tools (6 nika:*) ✅ DONE

**Tasks:** 10 | **Tests:** 45 planned → **96 actual** | **Effort:** 2 sessions

| Task | Description | Tests |
|------|-------------|-------|
| 3.1 | Create `runtime/builtin/mod.rs` + trait | 4 |
| 3.2 | Implement `BuiltinToolRouter` | 6 |
| 3.3 | Implement `nika:sleep` tool | 6 |
| 3.4 | Implement `nika:log` tool | 6 |
| 3.5 | Implement `nika:emit` tool | 6 |
| 3.6 | Implement `nika:assert` tool | 7 |
| 3.7 | Implement `nika:prompt` tool | 5 |
| 3.8 | Implement `nika:run` tool | 5 |
| 3.9 | Integrate router with Executor | 3 |
| 3.10 | Add router to RigAgentLoop | 2 |

**WIRING:** `BuiltinToolRouter` → Executor dispatch
**Live Test:** `cargo run -- run examples/test-builtin-*.nika.yaml`

---

## v0.10 "TaskBox" — TUI Widgets + DAG Panel

**Focus:** TUI visualization components
**Total:** 20 tasks, 75 tests, 4 sessions (~2 days)

### v0.10.0 — NodeBox Widget

**Tasks:** 5 | **Tests:** 15 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 0.1 | Create `tui/widgets/node_box.rs` | 3 |
| 0.2 | Implement task status rendering (pending/running/completed/failed) | 4 |
| 0.3 | Implement verb icon display (⚡📟🛰️🔌🐔) | 3 |
| 0.4 | Implement selection highlighting | 3 |
| 0.5 | Add animation frame support (spinner) | 2 |

**WIRING:** `NodeBox` → `TaskStatus` enum

---

### v0.10.1 — EdgeLine Widget

**Tasks:** 4 | **Tests:** 12 | **Effort:** 0.5 session

| Task | Description | Tests |
|------|-------------|-------|
| 1.1 | Create `tui/widgets/edge_line.rs` | 3 |
| 1.2 | Implement vertical connector rendering | 3 |
| 1.3 | Implement horizontal connector (for parallel) | 3 |
| 1.4 | Add data flow animation (optional) | 3 |

**WIRING:** `EdgeLine` → DAG edges

---

### v0.10.2 — TaskQueue Widget

**Tasks:** 4 | **Tests:** 12 | **Effort:** 0.5 session

| Task | Description | Tests |
|------|-------------|-------|
| 2.1 | Create `tui/widgets/task_queue.rs` | 3 |
| 2.2 | Implement pending/running/completed sections | 4 |
| 2.3 | Implement scroll support | 3 |
| 2.4 | Add real-time updates from EventLog | 2 |

**WIRING:** `TaskQueue` → `EventLog` subscription

---

### v0.10.3 — ChatDagPanel Integration

**Tasks:** 5 | **Tests:** 20 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 3.1 | Create `tui/widgets/chat_dag_panel.rs` | 4 |
| 3.2 | Implement layout algorithm (vertical DAG) | 5 |
| 3.3 | Integrate NodeBox + EdgeLine | 4 |
| 3.4 | Add EventLog subscription for real-time updates | 4 |
| 3.5 | Implement node selection + scroll sync with chat | 3 |

**WIRING:** `ChatDagPanel` → `ChatView` sidebar

---

### v0.10.4 — Animation Polish

**Tasks:** 4 | **Tests:** 16 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 4.1 | Implement node pulse animation (on completion) | 4 |
| 4.2 | Implement edge flow animation (data transfer) | 4 |
| 4.3 | Add Ctrl+D toggle shortcut for DAG panel | 4 |
| 4.4 | Add DAG state to session persistence | 4 |

**WIRING:** Session → DAG state restore

---

## v0.11 "Six Views" — TUI Architecture Upgrade

**Focus:** 6-view TUI architecture
**Total:** 30 tasks, 90 tests, 5 sessions (~2.5 days)

### v0.11.0 — Explorer View (refactor Home)

**Tasks:** 5 | **Tests:** 15 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 0.1 | Rename `home.rs` → `explorer.rs` | 2 |
| 0.2 | Update `TuiView::Home` → `TuiView::Explorer` | 3 |
| 0.3 | Add file tree navigation (vim-style) | 4 |
| 0.4 | Add workflow preview panel | 3 |
| 0.5 | Update hotkey `h` → `e` / `1` | 3 |

---

### v0.11.1 — Editor View (refactor Studio)

**Tasks:** 5 | **Tests:** 15 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 1.1 | Rename `studio.rs` → `editor.rs` | 2 |
| 1.2 | Update `TuiView::Studio` → `TuiView::Editor` | 3 |
| 1.3 | Add split-pane support (left: tree, right: editor) | 4 |
| 1.4 | Add multi-tab file editing | 3 |
| 1.5 | Update hotkey `s` → `d` / `3` | 3 |

---

### v0.11.2 — Runner View (refactor Monitor)

**Tasks:** 5 | **Tests:** 15 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 2.1 | Create `runner.rs` (new, not refactor) | 3 |
| 2.2 | Integrate TaskBox widgets from v0.10 | 4 |
| 2.3 | Add real-time execution panel | 3 |
| 2.4 | Add trace viewer integration | 3 |
| 2.5 | Update hotkey `m` → `r` / `4` | 2 |

---

### v0.11.3 — Scheduler View (NEW)

**Tasks:** 6 | **Tests:** 18 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 3.1 | Create `scheduler.rs` | 3 |
| 3.2 | Implement workflow list with schedule status | 4 |
| 3.3 | Add cron expression editor | 4 |
| 3.4 | Implement schedule enable/disable toggle | 3 |
| 3.5 | Add next-run-time display | 2 |
| 3.6 | Wire hotkey `5` / `s` | 2 |

---

### v0.11.4 — Settings View (NEW)

**Tasks:** 6 | **Tests:** 18 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 4.1 | Create `settings.rs` with 6-section layout | 3 |
| 4.2 | Implement `SettingsPanel` trait | 3 |
| 4.3 | Create `AppearancePanel` (theme, font) | 3 |
| 4.4 | Create `ProvidersPanel` (embed Provider Modal components) | 4 |
| 4.5 | Create `EditorPanel` + `SessionsPanel` + `AdvancedPanel` | 3 |
| 4.6 | Wire hotkey `6` / `,` | 2 |

**Note:** Uses components extracted from Provider Modal v2 (reusability audit confirmed 74% embeddable)

---

### v0.11.5 — Navigation Update

**Tasks:** 3 | **Tests:** 9 | **Effort:** 0.5 session

| Task | Description | Tests |
|------|-------------|-------|
| 5.1 | Update `TuiView` enum to 6 variants | 3 |
| 5.2 | Update Tab/Shift+Tab cycling for 6 views | 3 |
| 5.3 | Update header bar with 6 view tabs | 3 |

**New Navigation:**
```
┌─────────────────────────────────────────────────────────────────────────┐
│  [1/e] Explorer  │  [2/c] Chat  │  [3/d] Editor  │  [4/r] Runner      │
│  [5/s] Scheduler │  [6/,] Settings                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## v0.12 "Providers" — Provider Ecosystem Completion

**Focus:** Complete Provider Modal v2 + wire missing features
**Total:** 15 tasks, 45 tests, 3 sessions (~1.5 days)
**Note:** Can run PARALLEL with v0.10 after v0.9 completes

### Agent Findings Summary

From 5 parallel agents (2026-02-25):

| Component | Status | Missing |
|-----------|--------|---------|
| Provider Modal v2 | 95% complete | 5% UI wiring |
| Keyring | Fully implemented | Save handler not wired |
| Ollama Client | 580 lines, 42 tests | Pull/delete handlers not wired |
| Settings View Design | Architecture ready | 22h effort, 15 files |

### v0.12.0 — Keyring Wiring

**Tasks:** 4 | **Tests:** 12 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 0.1 | Wire `ModalAction::SaveAndTestApiKey` to `NikaKeyring::set()` | 3 |
| 0.2 | Wire `ModalAction::TestApiKey` to async verification | 3 |
| 0.3 | Add save confirmation UI feedback | 3 |
| 0.4 | Update `detect_state()` to check keyring before env vars | 3 |

**Files:**
- Modify: `src/tui/widgets/provider_modal/handler.rs`
- Modify: `src/tui/widgets/provider_modal/tabs/keys.rs`

---

### v0.12.1 — Env Migration Utility

**Tasks:** 4 | **Tests:** 12 | **Effort:** 0.5 session

| Task | Description | Tests |
|------|-------------|-------|
| 1.1 | Create `nika init --migrate-keys` command | 3 |
| 1.2 | Implement `.env` file parser | 3 |
| 1.3 | Migrate found keys to keyring | 3 |
| 1.4 | Add confirmation + dry-run mode | 3 |

**Files:**
- Create: `src/cli/migrate.rs`
- Modify: `src/cli/mod.rs`

---

### v0.12.2 — Provider Auto-Select

**Tasks:** 3 | **Tests:** 9 | **Effort:** 0.5 session

| Task | Description | Tests |
|------|-------------|-------|
| 2.1 | Wire Enter key in CloudTab to select provider | 3 |
| 2.2 | Emit `ViewAction::ProviderSelectorConfirm` | 3 |
| 2.3 | Update app.rs to handle provider switch | 3 |

**Files:**
- Modify: `src/tui/widgets/provider_modal/tabs/cloud.rs`
- Modify: `src/tui/app.rs`

---

### v0.12.3 — Ollama Enhancement

**Tasks:** 4 | **Tests:** 12 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 3.1 | Add `LoaderCommand::PullModel(String)` variant | 3 |
| 3.2 | Wire `[p]` key to model name input prompt | 3 |
| 3.3 | Wire `[d]` key to delete confirmation | 3 |
| 3.4 | Add curated model suggestions (llama3.2, mistral, phi3, etc.) | 3 |

**Files:**
- Modify: `src/tui/widgets/provider_modal/loader.rs`
- Modify: `src/tui/widgets/provider_modal/handler.rs`
- Modify: `src/tui/widgets/provider_modal/tabs/ollama.rs`

---

## Master Statistics

| Version | Name | Tasks | Tests | Sessions | Cumulative Tests |
|---------|------|-------|-------|----------|------------------|
| v0.9 | Chat-as-DAG | 32 | 131 | 6 | 131 |
| v0.10 | TaskBox | 20 | 75 | 4 | 206 |
| v0.11 | Six Views | 30 | 90 | 5 | 296 |
| v0.12 | Providers | 15 | 45 | 3 | 341 |
| **TOTAL** | | **97** | **341** | **18** | **+341 tests** |

**Effort:** 18 sessions × ~2 hours = ~36 hours (~9 working days)

---

## Execution Schedule

### Phase 1 (Week 1): v0.9 "Chat-as-DAG"

```
Day 1-2: v0.9.0 StableGraph + v0.9.1 ChatWorkflow (2 sessions)
Day 3:   v0.9.2 @mention Bindings (2 sessions)
Day 4:   v0.9.3 Builtin Tools (2 sessions)

Checkpoint: cargo test wiring_chat_dag
```

### Phase 2 (Week 2): v0.10 + v0.12 (PARALLEL)

```
TRACK A (v0.10 TaskBox):
  Day 5: v0.10.0 NodeBox + v0.10.1 EdgeLine (1.5 sessions)
  Day 6: v0.10.2 TaskQueue + v0.10.3 ChatDagPanel (1.5 sessions)
  Day 7: v0.10.4 Animation Polish (1 session)

TRACK B (v0.12 Providers):
  Day 5: v0.12.0 Keyring Wiring (1 session)
  Day 6: v0.12.1 Env Migration + v0.12.2 Auto-Select (1 session)
  Day 7: v0.12.3 Ollama Enhancement (1 session)

Checkpoint: cargo test wiring_taskbox && cargo test wiring_providers
```

### Phase 3 (Week 3): v0.11 "Six Views"

```
Day 8:  v0.11.0 Explorer + v0.11.1 Editor (2 sessions)
Day 9:  v0.11.2 Runner + v0.11.3 Scheduler (2 sessions)
Day 10: v0.11.4 Settings + v0.11.5 Navigation (1 session)

Checkpoint: cargo test wiring_six_views
```

---

## WIRING Checkpoints

Run after each version release:

```bash
# After v0.9
cargo test wiring_chat_dag -- --nocapture
# Verifies: StableGraph + ChatWorkflow + MentionParser + BuiltinTools

# After v0.10
cargo test wiring_taskbox -- --nocapture
# Verifies: NodeBox + EdgeLine + TaskQueue + ChatDagPanel

# After v0.11
cargo test wiring_six_views -- --nocapture
# Verifies: 6 views + navigation + Settings integration

# After v0.12
cargo test wiring_providers -- --nocapture
# Verifies: Keyring save + migration + auto-select + Ollama handlers
```

---

## Skill Mapping by Version

| Version | Primary Skills | Agents | Verification |
|---------|---------------|--------|--------------|
| v0.9 | @rust-core, @test-driven-development | rust-pro | `cargo test dag::` |
| v0.10 | @frontend-design, @test-driven-development | feature-dev | Visual inspection |
| v0.11 | @frontend-design, @rust-core | feature-dev | TUI navigation test |
| v0.12 | @rust-core, @verification-before-completion | rust-pro | Keyring integration |

---

## Changes from Previous Roadmap

### What Changed

1. **Split v0.9.x into 2 major versions:**
   - v0.9 "Chat-as-DAG" (core DAG)
   - v0.10 "TaskBox" (TUI widgets)

2. **Moved Provider Modal work to v0.12:**
   - Was v0.11-v0.12 scattered
   - Now consolidated in v0.12 "Providers"

3. **Added functional codenames:**
   - v0.9 "Chat-as-DAG"
   - v0.10 "TaskBox"
   - v0.11 "Six Views"
   - v0.12 "Providers"

4. **Enabled parallel execution:**
   - v0.12 can run alongside v0.10
   - Saves ~3 days

### Comparison

| Old | New | Change |
|-----|-----|--------|
| v0.9.0-v0.9.5 (6 versions) | v0.9.0-v0.9.3 (4 versions) | Reduced scope |
| v0.10.0-v0.10.5 (6 versions) | v0.10.0-v0.10.4 (5 versions) | TUI widgets only |
| v0.11.0-v0.11.5 (6 versions) | v0.11.0-v0.11.5 (6 versions) | Same (6 views) |
| Scattered provider work | v0.12.0-v0.12.3 (4 versions) | Consolidated |

---

## References

- [Original Roadmap](./ROADMAP-v09x.md) — v0.9.x single-version plan
- [6-Views Design](../v0.10+/2026-02-24-v010-v012-6-views-design.md) — Complete 6-view specification
- [Provider Modal v2 Plan](../v0.10+/2026-02-24-provider-modal-v2.md) — Implementation ready (95% complete)
- [UX-UI-PRESERVE.md](./UX-UI-PRESERVE.md) — Component preservation guide
- [WIRING-CHECKPOINTS.md](./WIRING-CHECKPOINTS.md) — Integration tests

---

## Agent Exploration Summary (2026-02-25)

5 parallel agents explored the codebase:

| Agent | Finding |
|-------|---------|
| Provider Modal | 95% complete, 270+ tests, production-ready |
| Keyring | Fully implemented (220 lines), save handler NOT wired |
| Ollama Client | Native HTTP (580 lines), 42 tests, handlers NOT wired |
| Settings View | Architecture designed, 22h effort, 15 files |
| Reusability | 74% of modal components directly embeddable |

**Key Insight:** v0.12 "Providers" is mostly about **wiring existing code**, not writing new features.
