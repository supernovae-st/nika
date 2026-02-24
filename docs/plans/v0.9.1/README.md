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

## Architecture 6-Views (v0.10+)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                        NIKA TUI v0.10-v0.12 — 6 VIEWS                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   [1]           [2]           [3]           [4]           [5]           [6]     │
│ EXPLORER      CHAT        EDITOR       RUNNER      SCHEDULER    SETTINGS       │
│                                                                                 │
│  📁 Files    💬 Agent     ✏️ YAML      ▶️ Execute   📅 Cron      ⚙️ Config      │
│  🦋 Browse   🐔 Talk      🚂 Edit      📊 Monitor   🔄 Queue     🎨 Theme       │
│                                                                                 │
│   DEFAULT     Tab →        Tab →        Tab →        Tab →        Tab →         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Détail des 6 vues

| # | Vue | Key | Purpose | Actions clés |
|---|-----|-----|---------|--------------|
| 1 | **Explorer** | `1` / `e` | File browser + DAG preview | Browse, Preview, Open |
| 2 | **Chat** | `2` / `c` | AI agent conversation | Prompt, @mention, inline TaskBox |
| 3 | **Editor** | `3` / `d` | YAML editing with DAG sync | Edit, Validate, DAG preview |
| 4 | **Runner** | `4` / `r` | Execution monitor | Progress, Live output, Cancel |
| 5 | **Scheduler** | `5` / `s` | Cron/queue management | Schedule, Timeline, History |
| 6 | **Settings** | `6` / `,` | Configuration | Theme, Providers, MCP, Sessions |

> **Note v0.10.0:** La vue **Settings** est en standby — elle ouvre la modale Provider existante pour l'instant. Le design complet sera implémenté en v0.11.1.

### Relations entre vues

```
  EXPLORER ───Open File───► EDITOR ───Run───► RUNNER
      │                         │                 │
      │ Open Chat               │ Export          │ Schedule
      ▼                         ▼                 ▼
    CHAT ─────Export YAML─────► EDITOR        SCHEDULER
      │                         │                 │
      │ Live DAG                │                 │
      └─────────────────────────┴─────────────────┘
                                │
                                ▼
                  ┌─────────────────────────────┐
                  │     StableGraph (DAG)       │
                  │     Source of Truth         │
                  └─────────────────────────────┘
```

### Évolution des vues

```
v0.8.x (4 vues)                     v0.10.x+ (6 vues)
───────────────                     ─────────────────
[1] Chat                            [1] Explorer (NEW - VS Code style)
[2] Home                      →     [2] Chat (enhanced with Live DAG)
[3] Studio                          [3] Editor (ex-Studio + DAG sync)
[4] Monitor                         [4] Runner (ex-Monitor + animations)
                                    [5] Scheduler (NEW - cron/queue)
                                    [6] Settings (NEW - standby → modal)
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
- Modifié dans Editor View
- Partagé avec l'équipe
- Versionné dans git
- Rejoué dans Runner

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
━━━ PHASE B: 6-Views Architecture ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ▼
v0.10.0 — Explorer + Editor (4 views → 6 views)
    │    File browser, DAG sync, VS Code style
    ▼
v0.10.1 — Chat-as-DAG Integration
    │    Live DAG, YAML preview, @mentions, // fork syntax
    ▼
v0.11.0 — Runner + Scheduler
    │    Animated execution, cron management, timeline view
    ▼
v0.11.1 — Settings View + Provider Modal v2
    │    Full Settings view, Ollama client, keyring integration
    ▼
v0.12.0 — Polish + Performance
    │    NovaNet tree effects, minimap, 60fps animations
    │
━━━ PHASE C+: Future ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    ▼
v0.13.x — Context discovery, boot sequence
v0.14.x — Multi-agent orchestration
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
| **6-Views** | Navigation fluide Tab / 1-6 |
| **DX** | Undo/Redo, sessions, fuzzy search |

---

## Quick Navigation

| Document | Purpose |
|----------|---------|
| [INDEX.md](./INDEX.md) | Implementation plans index |
| [ROADMAP-v09x.md](./ROADMAP-v09x.md) | Version-by-version breakdown |
| [5-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md) | **TUI 6-Views Architecture (v0.10+)** |
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
6. **6 Views** — Explorer-first (VS Code style), Settings en standby (modale)

---

## Commands

```bash
# Development
cargo test                           # Run 1,902 tests
cargo run -- chat                    # Chat view
cargo run -- studio                  # Editor view

# After v0.10.x
nika                                 # Explorer view (default)
nika chat                            # Chat view
nika editor workflow.yaml            # Editor view
nika run workflow.yaml               # Runner view
nika scheduler                       # Scheduler view
nika settings                        # Settings (opens modal for now)
```

---

**NO v1.0 — We stay in 0.XX versioning.**
