# Chat as Workflow DAG — Design Document

**Date:** 2026-02-24
**Status:** Complete (Open Questions Resolved)
**Authors:** Thibaut, Claude

---

## Executive Summary

Unifier le chat TUI avec le système de workflow DAG pour que chaque message soit une vraie Task avec DataStore, EventLog, bindings, et traces NDJSON.

**Vision:** Le chat devient un "workflow builder" interactif où le DAG se construit message par message.

---

## Brainstorm Summary

### Problème Initial

Actuellement, deux chemins d'exécution complètement séparés :

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  YAML WORKFLOW                        │  CHAT TUI (actuel)                  │
├───────────────────────────────────────┼─────────────────────────────────────┤
│  workflow.nika.yaml                   │  User: "Hello"                      │
│       ↓                               │       ↓                             │
│  Runner::run()                        │  ChatAgent::infer() ← DIRECT        │
│       ↓                               │       ↓                             │
│  Executor::execute_task()             │  RigProvider.infer()                │
│       ↓                               │       ↓                             │
│  DataStore.insert(task_id, result)    │  Vec<ChatMessage>.push() ← RAM      │
│       ↓                               │                                     │
│  EventLog.emit(TaskCompleted)         │  (pas d'EventLog)                   │
│       ↓                               │  (pas de DataStore)                 │
│  Binding: {{use.task1.output}}        │  (pas de bindings)                  │
└───────────────────────────────────────┴─────────────────────────────────────┘
```

**Preuve:** `grep DataStore src/tui/` retourne 0 résultats.

### Solution Proposée

Chaque message dans le chat = une Task qui s'ajoute au DAG en temps réel.

```
Message 1: "Décris QR Code"           → Task msg-001 (infer)
Message 2: "Génère un titre"          → Task msg-002 (infer) use: prev=msg-001
Message 3: "// Fetch trends"          → Task msg-003 (fetch) indépendant
Message 4: "Combine @1 @2 @3"         → Task msg-004 (infer) fan-in

DAG résultant:
    msg-001 ─────┐
                 ├──► msg-004
    msg-002 ─────┤
                 │
    msg-003 ─────┘
```

---

## Design Decisions

### 1. Mode de Connexion: Séquentiel Intelligent ✅

**Choix:** Option A — Séquentiel par défaut avec overrides explicites.

| Input | Comportement | Binding généré |
|-------|--------------|----------------|
| Message normal | Dépend du précédent | `use: { prev: "msg-N-1" }` |
| Message avec `@N` | Dépend des @mentions | `use: { m1: "msg-001", m3: "msg-003" }` |
| Message avec `//` | Indépendant (fork) | `use: {}` |

**Justification:**
- Zero learning curve (chat normal fonctionne comme attendu)
- Progressive disclosure (power features opt-in)
- Intention explicite (@mentions et // sont clairs)

**Exemples:**

```
# Session normale (90% des cas)
> "Décris QR Code"                     msg-001 (infer)
> "Génère un titre basé sur ça"        msg-002 → dépend de 001
> "Maintenant le body"                 msg-003 → dépend de 002

DAG: msg-001 ──► msg-002 ──► msg-003

# Session avec fork
> "Décris QR Code"                     msg-001
> // "Fetch les trends SEO"            msg-002 (indépendant)
> // "Fetch les competitors"           msg-003 (indépendant)
> "Combine @1 @2 @3 pour stratégie"    msg-004 (fan-in)

DAG: msg-001 ─────┐
     msg-002 ─────┼──► msg-004
     msg-003 ─────┘
```

### 2. Node Box: Version Enrichie (Full) ✅

**Choix:** Supprimer le mode Minimal, garder uniquement Expanded et l'enrichir.

**Actuel (NodeBoxMode::Expanded):**
```
┌────────────────────────────────────────┐
│ ⚡ task-001              ~2s        ○  │  ← icon + id + estimate + badge
│ claude-sonnet-4                        │  ← model
│ "Generate landing page..."             │  ← prompt preview
└────────────────────────────────────────┘
```

**Proposé (NodeBoxMode::Full):**
```
╭━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╮
┃ ⚡ msg-001                                               ◐ 2.3s   ┃
┃────────────────────────────────────────────────────────────────────┃
┃ 🧠 claude-sonnet-4              📊 1.2K in → 856 out              ┃
┃────────────────────────────────────────────────────────────────────┃
┃ 💬 "Génère un titre pour QR Code AI basé sur les trends..."       ┃
┃────────────────────────────────────────────────────────────────────┃
┃ 📤 "QR Code AI: Transform Links into Scannable Art"               ┃
┃────────────────────────────────────────────────────────────────────┃
┃ 🔗 use: @1.output, @3.data                                        ┃
╰━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━╯
```

**Infos ajoutées:**
- 📊 Tokens (input → output)
- 📤 Output preview (résultat de la task)
- 🔗 Bindings utilisés (@mentions résolues)
- ⏱️ Duration réelle (pas estimate)
- Status animé plus visible

### 3. Layout: Sidebar Droite Fixe ✅

**Choix:** Option A — DAG preview en sidebar droite permanente.

```
┌─────────────────────────────┬──────────────────┐
│ Chat                        │ DAG Live         │
│                             │                  │
│ > User: "Hello"             │   ╭─────╮        │
│ ╭──────────────────╮        │   │ 001 │        │
│ │ ⚡ msg-001       │        │   ╰──┬──╯        │
│ │ "Bonjour!..."    │        │      │           │
│ ╰──────────────────╯        │      ▼           │
│                             │   ╭━━━━━╮        │
│ > User: "Continue"          │   ┃ 002 ┃        │
│ ╭──────────────────╮        │   ╰━━━━━╯        │
│ │ ⚡ msg-002   ◐   │        │                  │
│ ╰──────────────────╯        │                  │
│                             │                  │
│ > _                         │ 2 tasks 2 layers │
└─────────────────────────────┴──────────────────┘
```

**Justification:**
- Toujours visible (pas de toggle)
- Feedback immédiat sur la structure
- Synchronisation visuelle chat ↔ DAG

---

## Architecture Technique

### Composants Existants à Réutiliser

| Composant | Fichier | Usage |
|-----------|---------|-------|
| `DataStore` | `src/store/datastore.rs` | Stockage résultats tasks |
| `EventLog` | `src/event/log.rs` | Observabilité (22 variants) |
| `Executor` | `src/runtime/executor.rs` | Exécution des 5 verbs |
| `NodeBox` | `src/tui/widgets/dag_node_box.rs` | Rendu des nodes |
| `DagAscii` | `src/tui/widgets/dag_ascii.rs` | Rendu du DAG complet |
| `MentionSystem` | `src/tui/widgets/mention_system.rs` | @mentions autocomplete |
| `InferStreamBox` | `src/tui/widgets/infer_stream_box.rs` | Visualisation streaming |
| `McpCallBox` | `src/tui/widgets/mcp_call_box.rs` | Visualisation MCP |

### Nouveaux Composants à Créer

| Composant | Description |
|-----------|-------------|
| `ChatWorkflow` | Workflow incrémental (ajoute nodes à la volée) |
| `ChatNode` | Enum wrapper pour UserInput ou Task |
| `NodeType` | `enum { Task, UserInput, SystemMessage }` |
| `ChatDagBuilder` | Construit le DAG message par message |
| `MentionToBinding` | Convertit @1 en `use: { m1: "msg-001" }` |
| `ChatDagPanel` | Widget sidebar avec DAG live |
| `BuiltinRegistry` | Registre des nika:* builtin tools |

### NodeType Architecture (2026-02-24 Decision)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NodeType enum — Distingue les types de nodes dans le DAG                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  pub enum NodeType {                                                            │
│      Task {                          // Réponse agent (5 verbs)                 │
│          verb: TaskVerb,             // infer, exec, fetch, invoke, agent       │
│          output: Value,              // Résultat de l'exécution                 │
│      },                                                                         │
│      UserInput {                     // Message utilisateur                     │
│          content: String,            // Texte brut                              │
│          output: String,             // = content (pour bindings uniformes)     │
│      },                                                                         │
│      SystemMessage {                 // Instructions système                    │
│          content: String,                                                       │
│      },                                                                         │
│  }                                                                              │
│                                                                                 │
│  BINDING UNIFORME:                                                              │
│  {{use.msg-001.output}} fonctionne pour Task ET UserInput                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Pourquoi pas un 6ème verb `input:`?**
- Un verb = action exécutée par l'agent
- Un message user = donnée entrante, pas une action
- NodeType::UserInput garde la distinction sémantique claire

### Data Flow

```
User Input (text)
    ↓
Parse: detect //, @mentions, /verb
    ↓
┌────────────────────────────────────┬────────────────────────────────────────┐
│  Is user message (no /verb)?       │  Is agent action (/verb prefix)?       │
├────────────────────────────────────┼────────────────────────────────────────┤
│                                    │                                        │
│  Create UserInput node {           │  Create Task node {                    │
│      id: "msg-XXX",                │      id: "msg-XXX",                    │
│      content: text,                │      verb: TaskAction,                 │
│      output: text,  ← same         │      use_wiring: from @mentions,       │
│  }                                 │  }                                     │
│          ↓                         │          ↓                             │
│  ChatDagBuilder.add_user_input()   │  ChatDagBuilder.add_task()             │
│          ↓                         │          ↓                             │
│  DataStore.insert(id, content)     │  Executor.execute_task()               │
│          ↓                         │          ↓                             │
│  EventLog.emit(UserInput)          │  DataStore.insert(id, result)          │
│                                    │          ↓                             │
│                                    │  EventLog.emit(TaskCompleted)          │
│                                    │                                        │
└────────────────────────────────────┴────────────────────────────────────────┘
    ↓                                        ↓
    └────────────────────┬───────────────────┘
                         ↓
              ChatDagPanel.refresh()
                         ↓
              UI: Message + DAG updated

BINDING UNIFORM: {{use.msg-001.output}} works for BOTH node types
```

### Syntaxe @mentions

| Syntax | Résolution |
|--------|------------|
| `@1` | `msg-001` (1er message) |
| `@2` | `msg-002` (2ème message) |
| `@last` | Dernier message |
| `@prev` | Message précédent (= @last en séquentiel) |
| `@all` | Tous les messages précédents |
| `@msg-001` | ID explicite |

### Syntaxe Prefix

| Prefix | Effet |
|--------|-------|
| (aucun) | Séquentiel (dépend du précédent) |
| `//` | Parallèle (indépendant) |
| `/infer` | Force verb infer |
| `/exec` | Force verb exec |
| `/fetch` | Force verb fetch |
| `/invoke` | Force verb invoke |
| `/agent` | Force verb agent |

---

## UI/UX Details

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Enter` | Envoyer message |
| `Tab` | Autocomplete @mention |
| `Ctrl+D` | Toggle DAG panel width (narrow/wide) |
| `Ctrl+E` | Expand/collapse all nodes in DAG |
| `↑` `↓` | Navigate history |
| `Esc` | Cancel current input |

### Status Indicators

| Status | Border | Badge | Animation |
|--------|--------|-------|-----------|
| Pending | `┄┄┄` dashed | `○` | None |
| Running | `━━━` bold | `◐◓◑◒` | Spinner + pulse |
| Success | `═══` double | `✓` | Brief glow |
| Failed | `═══` double red | `✗` | Shake |

### Edge Rendering

```
Simple edge (séquentiel):
    ╭─────╮
    │ 001 │
    ╰──┬──╯
       │
       ▼ {{use.prev}}
    ╭─────╮
    │ 002 │
    ╰─────╯

Fan-out (fork):
    ╭─────╮
    │ 001 │
    ╰──┬──╯
       │
    ┌──┴──┐
    ▼     ▼
  ╭───╮ ╭───╮
  │002│ │003│
  ╰───╯ ╰───╯

Fan-in (@mentions):
  ╭───╮ ╭───╮
  │001│ │003│
  ╰─┬─╯ ╰─┬─╯
    │ @1  │ @3
    ╰──┬──╯
       ▼
    ╭─────╮
    │ 004 │
    ╰─────╯
```

---

## Migration Path

### Phase 1: Infrastructure (no UI changes)
- [ ] Create `ChatWorkflow` struct with StableGraph
- [ ] Create `NodeType` enum (Task, UserInput, SystemMessage)
- [ ] Wire `DataStore` into chat execution
- [ ] Wire `EventLog` into chat execution

### Phase 2: Binding System
- [ ] Implement @mention parser
- [ ] Implement `MentionToBinding` converter
- [ ] Implement `//` prefix detection
- [ ] Wire bindings resolution
- [ ] **UserInput nodes expose .output for uniform bindings**

### Phase 3: Builtin Tools (NEW - 2026-02-24)
- [ ] Create `BuiltinTool` trait
- [ ] Create `BuiltinRegistry` for nika:* tools
- [ ] Implement `nika:prompt` (confirm, text, select, multiselect)
- [ ] Implement `nika:run` (workflow composition)
- [ ] Implement `nika:sleep`, `nika:log`, `nika:assert`, `nika:emit`
- [ ] Wire invoke: to check nika:* prefix before MCP dispatch

### Phase 4: DAG Panel
- [ ] Create `ChatDagPanel` widget
- [ ] Add sidebar layout to chat view
- [ ] Wire live DAG updates
- [ ] Implement node click → scroll to message
- [ ] **Different visual for UserInput vs Task nodes**

### Phase 5: Enhanced NodeBox
- [ ] Add tokens display
- [ ] Add output preview
- [ ] Add bindings display
- [ ] Remove Minimal mode

### Phase 6: Polish
- [ ] Animations (pulse, glow, shake)
- [ ] Keyboard shortcuts
- [ ] Resize handle for sidebar
- [ ] Session persistence for DAG state

---

## Success Criteria

1. **Unified Execution:** Chat uses same `Executor` as YAML workflows
2. **Full Observability:** Every message generates EventLog entries
3. **Binding Support:** `{{use.msg-001.output}}` works in chat
4. **Live DAG:** Sidebar shows DAG updating in real-time
5. **Trace Export:** Can export chat session as NDJSON trace
6. **Replay:** Can replay a chat session from trace file

---

## Thread-Safety Architecture (CRITICAL)

### Required Patterns

```rust
// ChatWorkflow must be thread-safe for TUI + async execution
pub struct ChatWorkflow {
    // StableGraph wrapped for concurrent access
    dag: Arc<parking_lot::Mutex<StableGraph<ChatNode, (), Directed>>>,

    // Lock-free ID generation
    next_node_id: AtomicU32,

    // EventLog is already thread-safe (Arc<Mutex<Vec<Event>>>)
    event_log: EventLog,

    // DataStore is Arc-wrapped internally
    data_store: DataStore,
}

impl ChatWorkflow {
    /// Add node without holding lock across await
    pub fn add_node(&self, node: ChatNode) -> NodeIndex {
        let mut dag = self.dag.lock(); // parking_lot: no poisoning
        let idx = dag.add_node(node);
        // Lock released here before any async work
        idx
    }

    /// Execute task - acquires lock briefly, releases before await
    pub async fn execute_task(&self, node_idx: NodeIndex) -> Result<Value, NikaError> {
        // 1. Get task data (brief lock)
        let task = {
            let dag = self.dag.lock();
            dag[node_idx].clone()
        }; // Lock released

        // 2. Execute (no lock held during await)
        let result = self.executor.execute(&task).await?;

        // 3. Update result (brief lock)
        {
            let mut dag = self.dag.lock();
            dag[node_idx].set_result(result.clone());
        } // Lock released

        Ok(result)
    }
}
```

### Anti-Patterns to AVOID

```rust
// ❌ NEVER hold lock across .await
async fn bad_pattern(&self) {
    let mut dag = self.dag.lock();
    let result = self.executor.execute(&task).await; // DEADLOCK RISK
    dag[idx].set_result(result);
}

// ✅ Release lock before await
async fn good_pattern(&self) {
    let task = {
        let dag = self.dag.lock();
        dag[idx].clone()
    };
    let result = self.executor.execute(&task).await;
    {
        let mut dag = self.dag.lock();
        dag[idx].set_result(result);
    }
}
```

### Mutex Choice

| Mutex | When to Use |
|-------|-------------|
| `parking_lot::Mutex` | Default choice - faster, no poisoning |
| `tokio::sync::Mutex` | ONLY if you must hold across await (avoid!) |
| `std::sync::Mutex` | Never use in async context |

---

## Performance Targets

### Frame Rate

| Metric | Target | Rationale |
|--------|--------|-----------|
| Frame time | <16.7ms | 60 FPS rendering |
| DAG render (100 nodes) | <5ms | Must not block UI |
| Node add | <1ms | Real-time feel |
| Lock hold time | <100µs | No contention |

### Memory

| Metric | Target | Strategy |
|--------|--------|----------|
| Per-node overhead | <1KB | Arc<str> interning |
| Max nodes in memory | 500 | Older nodes virtualized |
| Event buffer | 1000 events | Ring buffer with overflow |

### Scaling

| DAG Size | Rendering Strategy |
|----------|--------------------|
| 1-50 nodes | Full render every frame |
| 50-200 nodes | Dirty region tracking |
| 200+ nodes | Virtualized viewport |

---

## Resolved Questions (2026-02-24)

### 1. Session Recovery ✅ RESOLVED

**Decision:** Persist DAG state to `.nika/sessions/<session-id>.json`

**Implementation:**

```rust
// Session file structure
#[derive(Serialize, Deserialize)]
pub struct ChatDagSession {
    pub version: &'static str,  // "1.0"
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub nodes: Vec<SerializedNode>,
    pub edges: Vec<(u32, u32)>,  // (source_idx, target_idx)
    pub data_store_snapshot: HashMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
pub struct SerializedNode {
    pub index: u32,
    pub node_type: NodeType,
    pub status: TaskStatus,
    pub result: Option<Value>,
    pub created_at: DateTime<Utc>,
}
```

**Behavior:**
- Auto-save every 5 seconds (debounced)
- Auto-save on app exit (graceful shutdown)
- Auto-restore on app start (if session file exists)
- Manual "New Session" clears DAG and creates fresh file
- Session files cleaned up after 7 days (configurable)

**File location:**
```
.nika/sessions/
├── chat-<timestamp>.json     # Session files
├── current -> chat-xxx.json  # Symlink to active session
└── metadata.json             # Session index
```

### 2. Max DAG Size ✅ RESOLVED

**Decision:** Virtualized rendering with node collapsing at 100+ nodes

**Implementation:**

| DAG Size | Strategy | Visual |
|----------|----------|--------|
| 1-100 | Full render | All nodes visible |
| 101-500 | Viewport virtualization | Only visible nodes rendered |
| 500+ | Collapse + archive | Old nodes collapsed into "..." summary |

**Collapsed Node:**
```
╭─────────────────────────────────────────╮
│ 📦 Messages 1-50 (collapsed)            │
│    50 tasks • 12 errors • 45.2K tokens  │
│    Click to expand                      │
╰─────────────────────────────────────────╯
```

**Memory Management:**
```rust
pub struct ChatWorkflow {
    // Active nodes in StableGraph
    dag: Arc<Mutex<StableGraph<ChatNode, (), Directed>>>,

    // Archived nodes (serialized, not in graph)
    archived: Vec<ArchivedNodeRange>,

    // Config
    max_active_nodes: usize,  // Default: 100
}

impl ChatWorkflow {
    fn maybe_archive_old_nodes(&mut self) {
        if self.dag.lock().node_count() > self.max_active_nodes {
            // Archive oldest 50 nodes
            self.archive_range(0..50);
        }
    }
}
```

### 3. Error Recovery ✅ RESOLVED

**Decision:** Click-to-retry from DAG panel with cascading rerun option

**Implementation:**

```rust
pub enum RetryStrategy {
    /// Retry only this task
    Single,
    /// Retry this task and all dependents
    Cascade,
    /// Skip this task, continue with dependents using cached context
    SkipContinue,
}

impl ChatDagPanel {
    pub fn handle_node_click(&mut self, node_idx: NodeIndex) {
        let node = &self.dag[node_idx];

        if node.status == TaskStatus::Failed {
            // Show retry popup
            self.show_retry_options(node_idx, vec![
                RetryStrategy::Single,
                RetryStrategy::Cascade,
                RetryStrategy::SkipContinue,
            ]);
        } else {
            // Scroll chat to this message
            self.scroll_to_message(node_idx);
        }
    }

    pub async fn retry_task(&mut self, node_idx: NodeIndex, strategy: RetryStrategy) {
        match strategy {
            RetryStrategy::Single => {
                // Reset status to Pending
                self.dag[node_idx].status = TaskStatus::Pending;
                // Re-execute
                self.execute_single(node_idx).await;
            }
            RetryStrategy::Cascade => {
                // Find all dependents
                let dependents = self.dag.get_transitive_successors(node_idx);
                // Reset all to Pending
                for idx in std::iter::once(node_idx).chain(dependents) {
                    self.dag[idx].status = TaskStatus::Pending;
                }
                // Re-execute in topological order
                self.execute_from(node_idx).await;
            }
            RetryStrategy::SkipContinue => {
                // Mark as Skipped
                self.dag[node_idx].status = TaskStatus::Skipped;
                // Continue with dependents
                let dependents = self.dag.get_direct_successors(node_idx);
                for idx in dependents {
                    self.execute_single(idx).await;
                }
            }
        }
    }
}
```

**UI:**
```
┌──────────────────────────────────────────┐
│ ❌ msg-005 failed                        │
│ "API rate limit exceeded"                │
│                                          │
│ [Retry] [Retry + Dependents] [Skip]      │
└──────────────────────────────────────────┘
```

### 4. Export to YAML ✅ RESOLVED

**Decision:** Yes, export chat DAG as `.nika.yaml` workflow for reproducibility

**Implementation:**

```rust
impl ChatWorkflow {
    /// Export current DAG as executable workflow YAML
    pub fn export_to_yaml(&self) -> String {
        let mut tasks = Vec::new();
        let dag = self.dag.lock();

        // Topological order
        for node_idx in petgraph::algo::toposort(&*dag, None).unwrap() {
            let node = &dag[node_idx];
            let task = match &node.node_type {
                NodeType::UserInput { content, .. } => {
                    // Convert to nika:prompt or comment
                    TaskYaml {
                        id: node.id.clone(),
                        comment: Some(format!("User: {}", content)),
                        // Option: Convert to nika:prompt for replay
                        invoke: if self.export_options.interactive {
                            Some(InvokeYaml {
                                tool: "nika:prompt".into(),
                                params: json!({
                                    "type": "text",
                                    "message": content,
                                }),
                            })
                        } else {
                            None
                        },
                        ..Default::default()
                    }
                }
                NodeType::Task { verb, .. } => {
                    // Convert verb to YAML representation
                    self.convert_task_to_yaml(node)
                }
                NodeType::SystemMessage { .. } => continue, // Skip system messages
            };
            tasks.push(task);
        }

        // Generate YAML
        let workflow = WorkflowYaml {
            schema: "nika/workflow@0.6".into(),
            workflow: format!("chat-export-{}", self.session_id),
            description: Some("Exported from chat session".into()),
            tasks,
            flows: self.generate_flows(),
        };

        serde_yaml::to_string(&workflow).unwrap()
    }
}
```

**Export Options:**

| Option | Effect |
|--------|--------|
| `--interactive` | UserInput → `nika:prompt` (for replay with user input) |
| `--static` | UserInput → hardcoded values (for deterministic replay) |
| `--include-results` | Add `expected_output` for testing |

**Command:**
```bash
# In TUI
Ctrl+Shift+E → Export dialog

# CLI
nika chat export --session <id> --output workflow.nika.yaml --interactive
```

**Example Output:**
```yaml
schema: nika/workflow@0.6
workflow: chat-export-2026-02-24-001
description: "Exported from chat session"

tasks:
  - id: msg-001
    # User: "Décris QR Code"
    invoke:
      tool: nika:prompt
      params:
        type: text
        message: "Décris QR Code"
        default: "Décris QR Code"  # For non-interactive replay

  - id: msg-002
    infer: "Generate description based on user request"
    use:
      prev: msg-001.output

  - id: msg-003
    # User: "Génère un titre @1"
    invoke:
      tool: nika:prompt
      params:
        type: text
        message: "Génère un titre @1"

  - id: msg-004
    infer: "Generate title based on description"
    use:
      desc: msg-002.output
      request: msg-003.output

flows:
  - msg-001 -> msg-002
  - msg-002 -> msg-003
  - msg-001 -> msg-004
  - msg-003 -> msg-004
```

---

## References

- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflow Definition
- `src/tui/widgets/dag_node_box.rs` — Current NodeBox implementation
- `src/tui/widgets/dag_ascii.rs` — Current DAG rendering
- `src/tui/widgets/mention_system.rs` — @mention autocomplete
