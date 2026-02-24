# Nika v0.9.x Roadmap — Chat-as-DAG

> **For Claude:** Each version is a standalone release. Use TDD, WIRING checkpoints, and subagent-driven-development.

**Goal:** Unify Chat TUI with Workflow DAG system through granular releases.

---

## Version Overview

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  v0.9.x RELEASE TRAIN                                                         ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  v0.9.0  StableGraph Foundation     │ FlowGraph → StableGraph migration       ║
║  v0.9.1  ChatWorkflow Struct        │ DAG wrapper for chat messages           ║
║  v0.9.2  @mention Binding System    │ Parser + WiringSpec generation          ║
║  v0.9.3  Builtin Tools (6 nika:*)   │ sleep, log, emit, assert, prompt, run   ║
║  v0.9.4  ChatDagPanel Widget        │ TUI DAG visualization                   ║
║  v0.9.5  Polish & Export            │ Animations, persistence, Mermaid/JSON   ║
║                                                                               ║
║  v1.0.0  Chat-as-DAG Complete       │ Full integration, 5-view architecture   ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## UX/UI Components to Preserve

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🎯 KEEP THESE EFFECTS                                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  1. MATRIX RAIN (matrix_rain.rs)                                                │
│     ├── Background animation on active panels                                   │
│     ├── Katakana glyphs (80%) + ASCII (15%) + Nika mascots (5%)                │
│     ├── Configurable density, speed, fade                                       │
│     └── Trigger: Panel becomes active or receives important info                │
│                                                                                 │
│  2. MATRIX DECRYPT (matrix_decrypt.rs)                                          │
│     ├── Text reveal effect for streaming LLM responses                          │
│     ├── Verb-themed emoji chaos:                                                │
│     │   ├── 🏴‍☠️ Pirate theme (fetch:)                                            │
│     │   ├── 🌌 Cosmic theme (infer:)                                             │
│     │   ├── 🦄 Unicorn theme (creative)                                          │
│     │   ├── 🤖 Robot theme (exec:)                                               │
│     │   └── 🔮 Magic theme (agent:)                                              │
│     └── Progressive character reveal with random glyphs                         │
│                                                                                 │
│  3. ONE PANEL AT A TIME                                                         │
│     ├── FocusState manages active panel                                         │
│     ├── Tab/Shift+Tab for panel navigation                                      │
│     ├── 12 PanelIds across 4 views                                              │
│     └── Clear visual indicator of focused panel                                 │
│                                                                                 │
│  4. PROVIDER MODAL (v0.8.8)                                                     │
│     ├── 5 tabs: Cloud, Ollama, Keys, Config, Status                             │
│     ├── Shift+P hotkey to open                                                  │
│     ├── 6 providers: Claude, OpenAI, Mistral, Ollama, Groq, DeepSeek            │
│     └── Real-time status indicators                                             │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4-View Architecture (Current)

| View | Hotkey | Panels | Purpose |
|------|--------|--------|---------|
| **Home** | `h` / `1` | 4 | Browse workflows, recent files |
| **Chat** | `a` / `2` | 3 | Conversational agent + DAG panel (v0.9.4) |
| **Studio** | `s` / `3` | 3 | YAML editor with validation |
| **Monitor** | `m` / `4` | 4 | Trace viewer, event log |

> **Note:** User mentioned "5 views" — the 5th view will be defined in v1.0.0 planning.

---

## Version Details

### v0.9.0 — StableGraph Foundation

**Focus:** Migrate FlowGraph from petgraph::DiGraph to StableGraph
**Tasks:** 6 | **Tests:** 25 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 0.1 | Create `dag/stable.rs` with StableGraph wrapper | 5 |
| 0.2 | Implement `add_node()` with NodeIndex stability | 4 |
| 0.3 | Implement `remove_node()` preserving other indices | 4 |
| 0.4 | Implement `add_edge()` with EdgeIndex | 4 |
| 0.5 | Migrate FlowGraph to use StableGraph | 4 |
| 0.6 | Update all FlowGraph tests | 4 |

**WIRING:** FlowGraph → existing DAG validation (WIRING-0)
**Live Test:** `cargo test dag::` must pass

**Plan File:** [v0.9.0-StableGraph.md](./v0.9.0-StableGraph.md)

---

### v0.9.1 — ChatWorkflow Struct

**Focus:** Create ChatWorkflow as DAG wrapper for chat messages
**Tasks:** 6 | **Tests:** 20 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 1.1 | Create `runtime/chat_workflow.rs` module | 4 |
| 1.2 | Implement `add_message()` → node creation | 4 |
| 1.3 | Implement `get_message_by_index()` | 3 |
| 1.4 | Implement auto-edge creation (sequential) | 4 |
| 1.5 | Add message counter for @N references | 3 |
| 1.6 | Thread-safety with parking_lot::Mutex | 2 |

**WIRING:** ChatWorkflow → FlowGraph internal DAG (WIRING-1)
**Live Test:** `cargo run -- chat`, verify messages create nodes

**Plan File:** [v0.9.1-ChatWorkflow.md](./v0.9.1-ChatWorkflow.md)

---

### v0.9.2 — @mention Binding System

**Focus:** Parse @mentions and convert to WiringSpec bindings
**Tasks:** 10 | **Tests:** 35 | **Effort:** 2 sessions

| Task | Description | Tests |
|------|-------------|-------|
| 2.1 | Create `binding/mention.rs` module | 3 |
| 2.2 | Implement Mention enum (Number, Last, All, Range) | 4 |
| 2.3 | Implement `parse_mentions()` regex parser | 6 |
| 2.4 | Implement `@last` resolution | 3 |
| 2.5 | Implement `@all` resolution | 3 |
| 2.6 | Implement `@N..M` range resolution | 4 |
| 2.7 | Implement `//` parallel marker | 3 |
| 2.8 | Create `mentions_to_wiring()` converter | 4 |
| 2.9 | Integrate with ChatWorkflow | 3 |
| 2.10 | Add edge creation from mentions | 2 |

**WIRING:** MentionParser → ChatWorkflow → WiringSpec (WIRING-2)
**Live Test:** `cargo run -- chat`, test `@1`, `@last`, `//` syntax

**Plan File:** [v0.9.2-MentionBindings.md](./v0.9.2-MentionBindings.md)

---

### v0.9.3 — Builtin Tools (6 nika:*)

**Focus:** Implement 6 builtin tools with router
**Tasks:** 10 | **Tests:** 45 | **Effort:** 2 sessions

| Task | Description | Tests |
|------|-------------|-------|
| 3.1 | Create `runtime/builtin/mod.rs` + trait | 4 |
| 3.2 | Implement BuiltinToolRouter | 6 |
| 3.3 | Implement `nika:sleep` tool | 6 |
| 3.4 | Implement `nika:log` tool | 6 |
| 3.5 | Implement `nika:emit` tool | 6 |
| 3.6 | Implement `nika:assert` tool | 7 |
| 3.7 | Implement `nika:prompt` tool | 5 |
| 3.8 | Implement `nika:run` tool | 5 |
| 3.9 | Integrate router with Executor | 3 |
| 3.10 | Add router to RigAgentLoop | 2 |

**WIRING:** BuiltinToolRouter → Executor dispatch (WIRING-3)
**Live Test:** `cargo run -- run examples/test-builtin-*.nika.yaml`

**Plan File:** [v0.9.3-BuiltinTools.md](./v0.9.3-BuiltinTools.md)

---

### v0.9.4 — ChatDagPanel Widget

**Focus:** TUI visualization of chat DAG with real-time updates
**Tasks:** 8 | **Tests:** 25 | **Effort:** 2 sessions

| Task | Description | Tests |
|------|-------------|-------|
| 4.1 | Create `tui/widgets/node_box.rs` | 5 |
| 4.2 | Create `tui/widgets/edge_line.rs` | 4 |
| 4.3 | Create `tui/widgets/chat_dag_panel.rs` | 5 |
| 4.4 | Implement layout algorithm (vertical) | 4 |
| 4.5 | Add EventLog subscription | 3 |
| 4.6 | Implement node selection + scroll sync | 3 |
| 4.7 | Integrate with ChatView | 2 |
| 4.8 | Add Ctrl+D toggle shortcut | 2 |

**WIRING:** ChatDagPanel → EventLog subscription (WIRING-4)
**Live Test:** Visual verification — DAG updates in real-time

**Plan File:** [v0.9.4-DagPanel.md](./v0.9.4-DagPanel.md)

---

### v0.9.5 — Polish & Export

**Focus:** Animations, persistence, export formats
**Tasks:** 6 | **Tests:** 18 | **Effort:** 1 session

| Task | Description | Tests |
|------|-------------|-------|
| 5.1 | Implement node pulse animation | 4 |
| 5.2 | Implement edge flow animation | 3 |
| 5.3 | Add keyboard shortcuts (Ctrl+E export) | 2 |
| 5.4 | Implement DAG serialization | 3 |
| 5.5 | Implement Mermaid export | 3 |
| 5.6 | Implement session persistence | 3 |

**WIRING:** Session → DAG state restore (WIRING-5)
**Live Test:** Export DAG, exit/restart, verify restored

**Plan File:** [v0.9.5-Polish.md](./v0.9.5-Polish.md)

---

## Skill Mapping by Version

| Version | Primary Skills | Agents | Verification |
|---------|---------------|--------|--------------|
| v0.9.0 | @rust-core, @test-driven-development | rust-pro | `cargo test dag::` |
| v0.9.1 | @rust-core, @test-driven-development | rust-pro | `cargo run -- chat` |
| v0.9.2 | @rust-core, @test-driven-development | rust-pro | Manual @mention test |
| v0.9.3 | @rust-async, @test-driven-development | rust-async-expert | `cargo run -- run` |
| v0.9.4 | @frontend-design, @test-driven-development | feature-dev:code-reviewer | Visual inspection |
| v0.9.5 | @verification-before-completion | nika-deep-verify | Full E2E suite |

---

## Summary Stats

| Version | Tasks | Tests | Sessions | Cumulative Tests |
|---------|-------|-------|----------|------------------|
| v0.9.0 | 6 | 25 | 1 | 25 |
| v0.9.1 | 6 | 20 | 1 | 45 |
| v0.9.2 | 10 | 35 | 2 | 80 |
| v0.9.3 | 10 | 45 | 2 | 125 |
| v0.9.4 | 8 | 25 | 2 | 150 |
| v0.9.5 | 6 | 18 | 1 | 168 |

**Total:** 46 tasks, 168 tests, 9 sessions

---

## Git Workflow

```bash
# Each version gets its own branch
git checkout -b feature/v0.9.0-stablegraph
# ... work ...
git checkout main && git merge feature/v0.9.0-stablegraph
git tag v0.9.0

git checkout -b feature/v0.9.1-chatworkflow
# ... work ...
```

---

## WIRING Checkpoints

Run after EACH version release:

```bash
cargo test wiring_checkpoint_0  # After v0.9.0
cargo test wiring_checkpoint_1  # After v0.9.1
cargo test wiring_checkpoint_2  # After v0.9.2
cargo test wiring_checkpoint_3  # After v0.9.3
cargo test wiring_checkpoint_4  # After v0.9.4
cargo test wiring_checkpoint_5  # After v0.9.5
```

---

## References

- [UX-UI-PRESERVE.md](./UX-UI-PRESERVE.md) — Component preservation guide
- [WIRING-CHECKPOINTS.md](./WIRING-CHECKPOINTS.md) — Integration tests
- [INDEX.md](./INDEX.md) — All plan documents
