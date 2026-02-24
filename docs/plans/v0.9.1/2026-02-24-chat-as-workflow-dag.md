# Chat as Workflow DAG — Design Document

**Date:** 2026-02-24
**Status:** Draft
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

## Open Questions

1. **Session Recovery:** How to restore DAG state on app restart?
   - Option: Persist to `.nika/sessions/chat-dag.json`

2. **Max DAG Size:** What happens with 100+ messages?
   - Option: Collapse old nodes, show only recent N

3. **Error Recovery:** How to handle failed tasks in the DAG?
   - Option: Allow retry from DAG panel (click node → retry)

4. **Export to YAML:** Should we allow exporting chat DAG as .nika.yaml?
   - Option: Yes, for reproducibility

---

## References

- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflow Definition
- `src/tui/widgets/dag_node_box.rs` — Current NodeBox implementation
- `src/tui/widgets/dag_ascii.rs` — Current DAG rendering
- `src/tui/widgets/mention_system.rs` — @mention autocomplete
