# Nika v0.9.x — Chat-as-DAG Architecture

> **For Claude:** This is the master overview. Read this FIRST, then INDEX.md for implementation details.

---

## Vision

**Nika v0.9.x unifie Chat et Workflow sous une architecture DAG unique.**

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║   AVANT (v0.8.x)                      APRÈS (v0.9.x)                          ║
║   ──────────────                      ──────────────                          ║
║                                                                               ║
║   ┌─────────┐    ┌─────────┐          ┌─────────────────────────────┐         ║
║   │  Chat   │    │Workflow │          │    petgraph::StableGraph    │         ║
║   │Vec<Msg> │    │FlowGraph│          │    (NodeIndex stable)       │         ║
║   └─────────┘    └─────────┘          └─────────────────────────────┘         ║
║        │              │                            │                          ║
║        │   Séparés    │               ┌────────────┴────────────┐             ║
║        │   Pas de     │               ▼                         ▼             ║
║        │   lien       │         ┌───────────┐            ┌───────────┐        ║
║        ▼              ▼         │ChatWorkflow│            │FlowGraph  │        ║
║   Pas d'export   YAML statique  │<ChatMsg>   │            │<Task>     │        ║
║   Pas de @ref    Pas de chat    └───────────┘            └───────────┘        ║
║                                       │                         │             ║
║                                       └────────┬────────────────┘             ║
║                                                ▼                              ║
║                                         MÊME STRUCTURE                        ║
║                                         EXPORT YAML                           ║
║                                         @N RÉFÉRENCES                         ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Pourquoi ce changement ?

| Problème v0.8.x | Solution v0.9.x |
|-----------------|-----------------|
| Chat = `Vec<Message>` linéaire | Chat = DAG avec `StableGraph` |
| Pas de références entre messages | `@N` pour référencer n'importe quel message |
| Conversation non exportable | Export YAML de toute session |
| Code dupliqué Chat vs Workflow | UN moteur DAG unifié |
| TaskBox non standardisés | 5 widgets = 5 verbes sémantiques |

---

## Architecture 5-Views

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  [1] CHAT   [2] STUDIO   [3] MONITOR   [4] DAG   [5] HISTORY      Tab / 1-5    │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
│             │ │             │ │             │ │             │ │             │
│    CHAT     │ │   STUDIO    │ │   MONITOR   │ │     DAG     │ │   HISTORY   │
│             │ │             │ │             │ │             │ │             │
│  💬 Agent   │ │  📝 YAML    │ │  🔄 Live    │ │  🕸️ Graph   │ │  📜 Traces  │
│  interactif │ │   Editor    │ │  Execution  │ │   Visual    │ │   Browser   │
│             │ │             │ │             │ │             │ │             │
└─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘
      │               │               │               │               │
      └───────────────┴───────────────┴───────────────┴───────────────┘
                                      │
                                      ▼
                       ┌─────────────────────────────┐
                       │     StableGraph (DAG)       │
                       │     Source of Truth         │
                       └─────────────────────────────┘
```

### Détail des vues

| Vue | Raccourci | Purpose | Actions clés |
|-----|-----------|---------|--------------|
| **Chat** | `1` | Conversation agent interactive | Prompt, @mention, inline TaskBox |
| **Studio** | `2` | Éditeur YAML avec preview | Edit, Validate, DAG preview |
| **Monitor** | `3` | Exécution temps réel | Progress, Live output, Cancel |
| **DAG** | `4` | Visualisation graphe | Navigate nodes, Expand details |
| **History** | `5` | Historique des traces | Browse, Replay, Compare |

### Relations entre vues

```
     CHAT ───Export YAML───► STUDIO
       │                        │
       │ Show DAG               │ Run Workflow
       ▼                        ▼
      DAG ◄───Sync───────► MONITOR
       │                        │
       └───► HISTORY ◄──────────┘
                │
                └── Replay → CHAT ou MONITOR
```

---

## 5 Verbes = 5 TaskBox

Chaque action utilise UN des 5 verbes sémantiques. Chaque verbe a son widget visuel.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                                                                 │
│   ⚡ INFER         📟 EXEC         🛰️ FETCH        🔌 INVOKE        🐔 AGENT   │
│   Violet           Amber           Cyan            Emerald          Rose        │
│   #8b5cf6          #f59e0b         #06b6d4         #10b981          #f43f5e     │
│                                                                                 │
│   LLM call         Shell cmd       HTTP req        MCP tool         Multi-turn  │
│   streaming        execution       with retry      invocation       agentic     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

    + 🐤 SUBAGENT (Rose, spawned by agent:, shows depth)
```

### TaskBox Compact (dans Chat)

```
┌─ ⚡ INFER ────────────────────────────────────────────────────────────────┐
│  claude-sonnet-4-20250514                                            [▼] │
│  ████████████████████████████████████░░░░  87%  streaming...             │
│  Tokens: 1,892 in / 1,245 out   Cost: $0.0067   Elapsed: 4.2s            │
└──────────────────────────────────────────────────────────────────────────┘

┌─ 🔌 INVOKE ──────────────────────────────────────────────────────────────┐
│  novanet_describe @ novanet                                          [▼] │
│  ├─ entity: "qr-code"                                                    │
│  ├─ locale: "fr-FR"                                                      │
│  └─ ✅ Success (280ms) — Result: 2.1KB JSON                              │
└──────────────────────────────────────────────────────────────────────────┘

┌─ 🐔 AGENT ───────────────────────────────────────────────────────────────┐
│  Research competitors and summarize                                  [▼] │
│  Turn 3/5   ████████████░░░░░░░░   60%                                   │
│  ├─ 🔌 novanet_search (✅ 420ms)                                         │
│  ├─ 🛰️ fetch competitor site (✅ 890ms)                                  │
│  └─ ⚡ analyzing... (streaming)                                          │
│  Tokens: 4,521 in / 2,103 out   Cost: $0.0189                            │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## @Mention Bindings

Grâce à `StableGraph`, chaque message a un `NodeIndex` **stable** (ne change pas si on supprime d'autres messages).

### Syntaxe

| Pattern | Description |
|---------|-------------|
| `@N` | Contenu du message N |
| `@N.result` | Résultat du TaskBox dans message N |
| `@N.thinking` | Extended thinking (Claude) |
| `@last` | Dernier message |
| `@last.result` | Résultat du dernier TaskBox |

### Exemple

```
@1 USER: Analyse ce fichier
    └─ [attachment: report.pdf]

@2 ASSISTANT: Voici l'analyse...
    └─ [⚡ INFER — analysis result]

@3 USER: Traduis @2 en français           ◄── Référence @2

@4 ASSISTANT: Voici la traduction...
    └─ context: { from: @2.result }       ◄── Binding automatique

@5 USER: Compare @2 et @4                 ◄── Multi-référence
```

---

## Export YAML

**Toute conversation Chat peut être exportée en YAML workflow.**

```
┌─────────────────────────────────┬─────────────────────────────────────────┐
│  Chat Session                   │  Exported YAML                          │
├─────────────────────────────────┼─────────────────────────────────────────┤
│                                 │                                         │
│  @1 USER: Get QR code entity    │  schema: nika/workflow@0.5              │
│                                 │  workflow: chat-export-2026-02-24       │
│  @2 ASSISTANT:                  │                                         │
│    🔌 novanet_describe          │  tasks:                                 │
│    entity: qr-code ✅           │    - id: msg-002-invoke                 │
│                                 │      invoke: novanet_describe           │
│  @3 USER: Generate landing      │      params:                            │
│           using @2              │        entity: "qr-code"                │
│                                 │      use.ctx: entity_data               │
│  @4 ASSISTANT:                  │                                         │
│    ⚡ claude-sonnet-4 ✅        │    - id: msg-004-infer                  │
│                                 │      infer:                             │
│                                 │        prompt: "Generate..."            │
│  [Ctrl+E] Export ───────────►   │        context: $entity_data            │
│                                 │      depends_on: [msg-002-invoke]       │
│                                 │                                         │
└─────────────────────────────────┴─────────────────────────────────────────┘
```

Le YAML exporté peut être :
- Sauvegardé comme workflow réutilisable
- Modifié dans Studio View
- Partagé avec l'équipe
- Versionné dans git
- Rejoué dans Monitor

---

## Roadmap

```
━━━ PHASE 0: DX Preparation ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
v0.8.9 — DX Infrastructure (8 tasks, 56 tests)
    │    Type simplification, test harness, performance baseline
    │
    ▼ MUST COMPLETE BEFORE v0.9.0
━━━ PHASE A: Chat-as-DAG Core ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
v0.9.0 — StableGraph (6 tasks, 25 tests)
    │    petgraph migration, unified DAG foundation
    ▼
v0.9.1 — ChatWorkflow (6 tasks, 21 tests)
    │    Chat as DAG wrapper, auto-generated message IDs
    ▼
v0.9.2 — @mention Bindings (10 tasks, 35 tests)
    │    @N parser, reference resolution
    ▼
v0.9.3 — Builtin Tools (10 tasks, 45 tests)
    │    6 nika:* tools (export, history, etc.)
    ▼
v0.9.4 — DAG Panel + TaskBox (8 tasks + 58 widget tasks, 25 + 208 tests)
    │    TUI visualization, 5 verb widgets
    ▼
v0.9.5 — Polish & Export (6 tasks, 18 tests)
    │    Animations, YAML export from chat
    │
━━━ PHASE B: File-First Architecture ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ▼
v0.10.x — Project structure, user profile, long-term memory, policies
    │
━━━ PHASE C+: Future ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ▼
v0.11.x — Context discovery, boot sequence
v0.12.x — Multi-agent orchestration
```

---

## Test Counts

| Component | Tasks | Tests |
|-----------|-------|-------|
| v0.8.9 DX Preparation | 8 | 56 |
| v0.9.0 StableGraph | 6 | 25 |
| v0.9.1 ChatWorkflow | 6 | 21 |
| v0.9.2 @mention Bindings | 10 | 35 |
| v0.9.3 Builtin Tools | 10 | 45 |
| v0.9.4 DAG Panel | 8 | 25 |
| v0.9.5 Polish | 6 | 18 |
| **v0.9.x Core Subtotal** | **46** | **169** |
| TaskBox Foundation (v0.9.4a) | 15 | 60 |
| InferBox (v0.9.4b) | 10 | 35 |
| ExecBox (v0.9.4c) | 9 | 30 |
| FetchBox (v0.9.4d) | 8 | 28 |
| InvokeBox (v0.9.4e) | 8 | 25 |
| AgentBox (v0.9.4f) | 8 | 30 |
| **TaskBox Subtotal** | **58** | **208** |
| **TOTAL NEW** | **112** | **433** |
| Existing tests | — | 1,902 |
| **FINAL** | — | **2,335** |

---

## Success Criteria

| Critère | Description |
|---------|-------------|
| **Unification** | Chat et Workflow = même moteur DAG |
| **Traçabilité** | Chaque action = nœud dans le DAG |
| **@mention** | Référencer n'importe quel message/résultat |
| **Export YAML** | Conversation → Workflow réutilisable |
| **Visual Feedback** | TaskBox widgets pour chaque verbe |
| **5-Views** | Navigation fluide Tab / 1-5 |
| **DX** | Undo/Redo, sessions, fuzzy search |

---

## Quick Navigation

| Document | Purpose |
|----------|---------|
| [INDEX.md](./INDEX.md) | Implementation plans index |
| [ROADMAP-v09x.md](./ROADMAP-v09x.md) | Version-by-version breakdown |
| [v0.8.9-DX-Preparation.md](./v0.8.9-DX-Preparation.md) | Pre-flight DX tasks |
| [v0.9.0-StableGraph.md](./v0.9.0-StableGraph.md) | DAG foundation |
| [v0.9.4a-TaskBoxFoundation.md](./v0.9.4a-TaskBoxFoundation.md) | Widget system |

---

## Key Decisions

1. **petgraph::StableGraph** — NodeIndex stable après suppression
2. **ChatWorkflow wraps StableFlowGraph** — Même structure que FlowGraph
3. **@N = NodeIndex** — Référence stable, pas position
4. **5 verbes = 5 TaskBox** — Mapping 1:1
5. **Export = reconstruction** — Chat → YAML via DAG traversal

---

## Commands

```bash
# Development
cargo test                           # Run 1,902 tests
cargo run -- chat                    # Chat view
cargo run -- studio                  # Studio view

# After v0.9.x
nika chat                            # Interactive agent
nika chat --export workflow.yaml     # Export session
nika studio workflow.yaml            # Edit YAML
nika run workflow.yaml               # Execute
```

---

**NO v1.0 — We stay in 0.XX versioning.**
