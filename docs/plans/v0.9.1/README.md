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

See [ROADMAP.md](./ROADMAP.md) for the complete version breakdown.

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
v0.9.2 — @mention Bindings (10 tasks, 40 tests)
    │    @N parser, reference resolution
    ▼
v0.9.3 — Builtin Tools (10 tasks, 45 tests)
    │    6 nika:* tools (export, history, etc.)
    │
━━━ PHASE B: TaskBox & 6-Views → See v0.10/ and v0.11/ ━━━━━━━━━━━━━━━━━━━━━━━━━
    ▼
v0.10.x — TaskBox widgets, DAG Panel, Animation Polish
    │    See docs/plans/v0.10/ for full breakdown
    ▼
v0.11.x — Six Views Architecture
    │    Explorer, Editor, Runner, Scheduler, Settings
    │    See docs/plans/v0.11/ for full breakdown
    ▼
v0.12.x — Providers Wiring
    │    Keyring, Ollama handlers, Provider auto-select
    │    See docs/plans/v0.12/ for full breakdown
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
| v0.9.2 @mention Bindings | 10 | 40 |
| v0.9.3 Builtin Tools | 10 | 45 |
| **v0.9.x TOTAL** | **32** | **131** |
| Existing tests | — | 1,902 |
| **FINAL (after v0.9.x)** | — | **2,033** |

> **Note:** TaskBox widgets (58 tasks, 210 tests) moved to [v0.10/](../v0.10/) as separate release.

### Test Specification Approach

> **Note:** Test counts are targets. Each plan document specifies primary tests per task (Step 1: Write failing test). Additional edge-case tests are added during TDD implementation.

**Test categories per task:**
1. **Primary test**: Specified in plan (fails until implementation)
2. **Edge cases**: Added during implementation (error paths, boundaries)
3. **Integration tests**: Added for WIRING checkpoints

**WIRING checkpoint tests** are in `WIRING-CHECKPOINTS.md` and validate component integration.

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
| [ROADMAP.md](./ROADMAP.md) | Version-by-version breakdown |
| [6-VIEWS-DESIGN.md](../v0.10+/2026-02-24-v010-v012-6-views-design.md) | **TUI 6-Views Architecture (v0.10+)** |
| [v0.8.9-DX-Preparation.md](./v0.8.9-DX-Preparation.md) | Pre-flight DX tasks |
| [v0.9.0-StableGraph.md](./v0.9.0-StableGraph.md) | DAG foundation |
| [../v0.10/](../v0.10/) | TaskBox widget plans |
| [../v0.11/](../v0.11/) | Six Views architecture |
| [../v0.12/](../v0.12/) | Providers wiring |

---

## Key Decisions

1. **petgraph::StableGraph** — NodeIndex stable après suppression
2. **ChatWorkflow wraps StableFlowGraph** — Même structure que FlowGraph
3. **@N = NodeIndex** — Référence stable, pas position
4. **5 verbes = 5 TaskBox** — Mapping 1:1
5. **Export = reconstruction** — Chat → YAML via DAG traversal
6. **6 Views** — Explorer-first (VS Code style), Settings en standby (modale)

### Canonical Type Definitions

| Type | Location | Version |
|------|----------|---------|
| `ChatWorkflow` | [v0.9.1-ChatWorkflow.md](./v0.9.1-ChatWorkflow.md) → `src/runtime/chat_workflow.rs` | v0.9.1 |
| `ChatMessage` | [v0.9.1-ChatWorkflow.md](./v0.9.1-ChatWorkflow.md) → `src/runtime/chat_workflow.rs` | v0.9.1 |
| `Mention` | [v0.9.2-MentionBindings.md](./v0.9.2-MentionBindings.md) → `src/binding/mention.rs` | v0.9.2 |
| `TaskBox*` | [v0.10/archive/](../v0.10/archive/) → `src/tui/widgets/task_box.rs` | v0.10.x |

> **Note:** TaskBox widget specs moved to v0.10/archive/. See [v0.10/INDEX.md](../v0.10/INDEX.md) for navigation.

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
