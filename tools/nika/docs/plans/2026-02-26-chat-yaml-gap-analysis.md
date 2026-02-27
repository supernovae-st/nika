# Chat → YAML Workflow Gap Analysis

**Date:** 2026-02-26 (Updated)
**Author:** Claude Opus 4.5
**Target:** v0.13.0
**Status:** ANALYSIS CORRECTED

---

## Executive Summary

**CORRECTION:** L'infrastructure runtime est IMPLÉMENTÉE. Le gap est uniquement le **câblage TUI**.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  RUNTIME INFRASTRUCTURE — IMPLÉMENTÉ ✅                                       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ChatWorkflow (src/runtime/chat_workflow.rs) — 1014 lignes, 40+ tests         ║
║    ├── StableDag<ChatMessage> wrapper                                         ║
║    ├── add_message() avec auto-edges séquentiels                              ║
║    ├── add_message_parallel() pour // prefix                                  ║
║    ├── add_edges_from_mentions() pour @N références                           ║
║    └── get_dependencies(), get_dependents()                                   ║
║                                                                               ║
║  Mention System (src/binding/mention.rs) — @N, @last, @all, @N..M             ║
║    ├── parse_mentions() regex parser                                          ║
║    ├── resolve_mention() → ResolvedMention                                    ║
║    ├── text_to_wiring() → WiringSpec                                          ║
║    └── has_parallel_marker(), strip_parallel_marker()                         ║
║                                                                               ║
║  nika:run builtin (src/runtime/builtin/run.rs)                                ║
║    └── Execute nested workflow via Runner::new(workflow).run()                ║
║                                                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║  TUI WIRING — MANQUANT ❌                                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ChatView (src/tui/views/chat.rs) n'utilise PAS ChatWorkflow                  ║
║    • grep "ChatWorkflow" dans src/tui/ = 0 résultats                          ║
║    • ChatView utilise toujours ChatAgent → RigProvider directement            ║
║                                                                               ║
║  /export yaml — conversion ChatWorkflow → YAML non implémentée                ║
║                                                                               ║
║  ChatDagPanel — sync depuis messages, pas depuis ChatWorkflow.dag             ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Impact:** Le runtime est prêt. Il faut câbler ChatView à ChatWorkflow.

---

## Findings

### 1. Chat Execution Path (ChatAgent)

**Location:** `src/tui/chat_agent.rs:199`

```rust
pub struct ChatAgent {
    provider: RigProvider,           // Direct LLM access
    history: Vec<ChatMessage>,       // Local history
    streaming_tx: Option<mpsc::Sender<String>>,
}

impl ChatAgent {
    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError>
    pub async fn exec_command(&self, command: &str) -> Result<String, NikaError>
    pub async fn fetch(&self, url: &str, method: &str) -> Result<String, NikaError>
}
```

**Observations:**
- Utilise `RigProvider` directement, pas `Executor`
- Pas d'émission d'events EventLog
- Pas de création de Task/Workflow
- Pas de DAG de dépendances

### 2. YAML Workflow Execution Path (Runner)

**Location:** `src/runtime/runner.rs:324`

```rust
impl Runner {
    pub async fn run(&self) -> Result<String, NikaError> {
        // 1. Parse workflow YAML
        // 2. Build DAG from tasks/flows
        // 3. Execute via Executor
        // 4. Emit events to EventLog
    }
}
```

**Observations:**
- Parse YAML → AST (Workflow, Task)
- Construit un DAG de dépendances
- Exécute via `Executor`
- Émet des events via `EventLog`

### 3. Chat → YAML Export Status

| Feature | Status | Evidence |
|---------|--------|----------|
| `/export` command | Exists | `src/tui/command.rs:77-78` |
| Export format | JSON only | `Export { path: Option<String> }` |
| YAML workflow output | MISSING | No `to_yaml`, `to_workflow` |
| `/yaml` toggle | Exists | Shows messages as YAML **view** (not export) |

**Code evidence:**
```rust
// src/tui/views/chat.rs:412-413
// === v0.7.3 YAML View Toggle ===
/// Show messages as YAML tasks instead of chat bubbles
```

Le `/yaml` toggle affiche les messages **en format YAML** mais ne les **exporte pas** comme workflow.

### 4. DAG Panel Wiring

| Component | Status | Location |
|-----------|--------|----------|
| ChatDagPanel widget | Implemented | `src/tui/widgets/chat_dag_panel.rs` |
| Node/Edge data structures | Implemented | 30+ tests |
| sync_dag_from_messages() | Implemented | `views/chat.rs:4711` |
| Real-time YAML building | MISSING | No workflow construction |

---

## Implementation Matrix: Plans v0.9 → v0.12

| Plan | Feature | Designed | Implemented | Location |
|------|---------|----------|-------------|----------|
| v0.9.1 | ChatWorkflow | Yes | **✅ YES** | `src/runtime/chat_workflow.rs` |
| v0.9.2 | Mention System | Yes | **✅ YES** | `src/binding/mention.rs` |
| v0.9.3 | nika:run builtin | Yes | **✅ YES** | `src/runtime/builtin/run.rs` |
| v0.9 | CommandParser | Yes | Yes | `src/tui/command.rs` |
| v0.9 | FileResolver | Yes | Yes | `src/tui/file_resolver.rs` |
| v0.9 | ChatAgent | Yes | Yes | `src/tui/chat_agent.rs` |
| v0.9 | 5 verb commands | Yes | Yes | Chat commands |
| v0.10 | ChatDagPanel | Yes | Yes | `src/tui/widgets/chat_dag_panel.rs` |
| v0.10 | ChatNodeBox | Yes | Yes | `src/tui/widgets/chat_node_box.rs` |
| v0.10 | ChatEdgeLine | Yes | Yes | `src/tui/widgets/chat_edge_line.rs` |
| v0.10 | Wiring checkpoint tests | Yes | **✅ YES** | `tests/wiring_checkpoint_*.rs` |
| **v0.13** | **ChatView ↔ ChatWorkflow wire** | NOW | **❌ TODO** | **This is the gap** |
| **v0.13** | **/export yaml** | NOW | **❌ TODO** | Needs ChatWorkflow.to_yaml() |
| **v0.13** | **ChatDagPanel ↔ ChatWorkflow.dag** | NOW | **❌ TODO** | Sync from ChatWorkflow |

---

## Architecture Gap

### Current Architecture (Disconnected)

```
  ┌─────────────────────┐          ┌─────────────────────┐
  │     CHAT VIEW       │    ≠     │    YAML WORKFLOW    │
  ├─────────────────────┤          ├─────────────────────┤
  │ ChatAgent           │          │ Runner              │
  │ ├── infer()         │          │ ├── run()           │
  │ ├── exec_command()  │          │ └── Executor        │
  │ └── fetch()         │          │     ├── infer       │
  │                     │          │     ├── exec        │
  │ NO EventLog         │          │     ├── fetch       │
  │ NO Workflow AST     │          │     ├── invoke      │
  │ NO DAG deps         │          │     └── agent       │
  └─────────────────────┘          └─────────────────────┘
           │                                │
           ▼                                ▼
      Results only                   EventLog + Traces
```

### Desired Architecture (Unified)

```
  ┌─────────────────────┐          ┌─────────────────────┐
  │     CHAT VIEW       │          │    YAML WORKFLOW    │
  │ /infer, /exec, etc  │          │ workflow.nika.yaml  │
  └──────────┬──────────┘          └──────────┬──────────┘
             │                                │
             ▼                                ▼
  ┌────────────────────────────────────────────────────────┐
  │              UNIFIED EXECUTION ENGINE                   │
  │  ┌──────────────────────────────────────────────────┐  │
  │  │  WorkflowBuilder (NEW)                           │  │
  │  │  - Constructs Workflow AST from commands         │  │
  │  │  - Tracks dependencies (@N references)           │  │
  │  │  - Exports to YAML                               │  │
  │  └──────────────────────────────────────────────────┘  │
  │                         │                               │
  │  ┌──────────────────────────────────────────────────┐  │
  │  │  Executor (existing)                             │  │
  │  │  - Same engine for Chat and Workflow             │  │
  │  │  - Emits events to EventLog                      │  │
  │  └──────────────────────────────────────────────────┘  │
  └────────────────────────────────────────────────────────┘
                         │
                         ▼
            EventLog + Traces + YAML Export
```

---

## Solution: Wire ChatWorkflow to ChatView

**ChatWorkflow already exists!** Il faut juste le câbler à ChatView.

### ChatWorkflow API (EXISTANT)

```rust
// src/runtime/chat_workflow.rs — ALREADY IMPLEMENTED

pub struct ChatWorkflow {
    pub dag: StableDag<ChatMessage>,
    message_counter: u32,
    id_to_index: HashMap<String, NodeIndex>,
}

impl ChatWorkflow {
    pub fn add_message(&mut self, content: &str, role: Role) -> NodeIndex;
    pub fn add_message_parallel(&mut self, content: &str, role: Role) -> NodeIndex;
    pub fn add_message_with_mentions(&mut self, content: &str, role: Role) -> Result<NodeIndex>;
    pub fn get_dependencies(&self, idx: NodeIndex) -> Vec<NodeIndex>;
    pub fn get_dependents(&self, idx: NodeIndex) -> Vec<NodeIndex>;
    // ... 40+ tested methods
}
```

### Missing: to_yaml() Export

```rust
// NEW METHOD NEEDED in chat_workflow.rs
impl ChatWorkflow {
    /// Export chat conversation to Nika workflow YAML
    pub fn to_yaml(&self) -> String {
        // Convert messages to Task structs
        // Build dependency flows from edges
        // Serialize to nika/workflow@0.2 format
    }
}
```

### Wiring Points

| From | To | What |
|------|-----|------|
| `ChatView` | `ChatWorkflow` | Add field `workflow: ChatWorkflow` |
| `handle_chat_infer()` | `workflow.add_message_with_mentions()` | Track messages in DAG |
| `sync_dag_from_messages()` | `workflow.dag` | Use ChatWorkflow DAG instead of messages |
| `/export yaml` | `workflow.to_yaml()` | Export to `.nika.yaml` file |

---

## Implementation Plan (CORRECTED)

### Phase 1: Wire ChatWorkflow to ChatView (P0)

**Effort:** 2-3 hours

| Task | Description |
|------|-------------|
| Add `workflow: ChatWorkflow` to `ChatView` struct | Field addition |
| Import `ChatWorkflow` from `nika::runtime::chat_workflow` | Module import |
| Call `workflow.add_message_with_mentions()` on each message | Wire add |
| Update `sync_dag_from_messages()` to use `workflow.dag` | Sync from workflow |
| Tests: verify wiring works | 5+ tests |

### Phase 2: Add to_yaml() Export (P1)

**Effort:** 2-3 hours

| Task | Description |
|------|-------------|
| Add `to_yaml()` method to ChatWorkflow | YAML serialization |
| Convert ChatMessage to ast::Task | Role → verb mapping |
| Build dependencies from edges | Edge → flow |
| Serialize with serde_yaml | Output formatting |
| Add `/export yaml` command variant | TUI integration |

### Phase 3: Full YAML Round-Trip (P2)

**Effort:** 1-2 hours

| Task | Description |
|------|-------------|
| Test exported YAML loads in Studio | Integration test |
| Test exported YAML runs with `nika run` | End-to-end test |
| Document the workflow | Usage guide |

**Total: 5-8 hours** (down from 13 hours — infrastructure already done!)

---

## Success Criteria

- [ ] `/export yaml` outputs valid `.nika.yaml` file
- [ ] Exported workflow can be loaded in Studio
- [ ] Exported workflow can be run with `nika run`
- [ ] DAG panel shows real-time workflow construction
- [ ] @N references create proper task dependencies

---

## Files to Modify

| File | Action |
|------|--------|
| `src/runtime/chat_workflow.rs` | Add `to_yaml()` method |
| `src/tui/views/chat.rs` | Add `workflow: ChatWorkflow` field, wire messages |
| `src/tui/command.rs` | Add `/export yaml` variant |
| `src/tui/app.rs` | Handle export action, call `workflow.to_yaml()` |
| `tests/wiring_checkpoint_*.rs` | Add ChatView ↔ ChatWorkflow wiring tests |

**NOTE:** NO new modules needed — ChatWorkflow exists!

---

## References

- Plan: `2026-02-20-chat-agent-interface.md` - Original chat design
- Plan: `2026-02-23-chat-inline-boxes-wiring.md` - Inline visualization
- Plan: `2026-02-23-tui-views-redesign.md` - 6-views architecture
- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflow Definition
