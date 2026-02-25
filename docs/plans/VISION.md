# Nika v0.9-v0.12 — Vision Complète

> **For Claude:** Ce document est la source de vérité pour la vision produit. Lire AVANT tout travail sur v0.9+.

---

## Executive Summary

**Nika v0.9-v0.12** transforme Nika d'un simple workflow runner en une **plateforme conversationnelle unifiée** où chaque interaction avec l'IA devient un workflow reproductible.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║   VISION: "Chaque conversation avec l'IA est un workflow en construction."   ║
║                                                                               ║
║   Tu parles     → DAG se construit                                            ║
║   Tu @mention   → Arcs se créent                                              ║
║   Tu exportes   → YAML reproductible                                          ║
║   Tu schedules  → Automation                                                  ║
║                                                                               ║
║   Le Chat n'est plus éphémère. C'est un artefact versionnable.               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Pourquoi Ce Plan ?

### Le Problème (v0.8.x)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ARCHITECTURE ACTUELLE — Deux mondes séparés                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│    ┌─────────────────────┐              ┌─────────────────────┐                 │
│    │        CHAT         │              │      WORKFLOW       │                 │
│    │    Vec<Message>     │              │      Dag      │                 │
│    ├─────────────────────┤              ├─────────────────────┤                 │
│    │                     │              │                     │                 │
│    │  • Liste linéaire   │              │  • DAG structuré    │                 │
│    │  • Pas de refs      │              │  • Dépendances      │                 │
│    │  • Non exportable   │              │  • YAML statique    │                 │
│    │  • Éphémère         │              │  • Pas d'historique │                 │
│    │                     │              │                     │                 │
│    └──────────┬──────────┘              └──────────┬──────────┘                 │
│               │                                    │                            │
│               │         ❌ PAS DE LIEN             │                            │
│               │         ❌ CODE DUPLIQUÉ           │                            │
│               │         ❌ EXPÉRIENCES SÉPARÉES    │                            │
│               ▼                                    ▼                            │
│                                                                                 │
│         Messages perdus                     YAML sans contexte                  │
│         dans le vide                        de création                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Problèmes concrets:**

| Problème | Impact |
|----------|--------|
| Chat = `Vec<Message>` linéaire | Impossible de référencer un message précédent |
| Pas de références entre messages | "Traduis ça" → ça quoi ? |
| Conversation non exportable | Workflow créé en chat = perdu |
| Code dupliqué Chat vs Workflow | Maintenance double, bugs doubles |
| TaskBox non standardisés | Chaque verbe a un rendu différent ad-hoc |
| 4 vues basiques | Navigation limitée, pas de scheduler |

### La Solution (v0.9+)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NOUVELLE ARCHITECTURE — Unification sous StableGraph                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│                      ┌───────────────────────────────────┐                      │
│                      │     petgraph::StableGraph         │                      │
│                      │     (NodeIndex stable)            │                      │
│                      │                                   │                      │
│                      │  • Index ne change pas après      │                      │
│                      │    suppression d'autres nœuds     │                      │
│                      │  • Permet @N références stables   │                      │
│                      │  • Base commune Chat & Workflow   │                      │
│                      └───────────────┬───────────────────┘                      │
│                                      │                                          │
│              ┌───────────────────────┼───────────────────────┐                  │
│              │                       │                       │                  │
│              ▼                       ▼                       ▼                  │
│       ┌─────────────┐         ┌─────────────┐         ┌─────────────┐           │
│       │ ChatWorkflow│         │  Dag  │         │ Export YAML │           │
│       │  <ChatMsg>  │         │   <Task>    │         │             │           │
│       ├─────────────┤         ├─────────────┤         ├─────────────┤           │
│       │             │         │             │         │             │           │
│       │ Conversation│ ◄─────► │  Workflow   │ ───────►│ .nika.yaml  │           │
│       │ interactive │         │  statique   │         │ reproductible│          │
│       │             │         │             │         │             │           │
│       └─────────────┘         └─────────────┘         └─────────────┘           │
│              │                       │                       │                  │
│              │    MÊME STRUCTURE     │    INTERCHANGEABLE    │                  │
│              │    MÊME MOTEUR        │    VERSIONNABLE       │                  │
│              └───────────────────────┴───────────────────────┘                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Bénéfices:**

| Avant | Après |
|-------|-------|
| Chat = messages perdus | Chat = DAG traçable |
| Workflow = fichiers séparés | Workflow = même structure |
| Pas de lien entre les deux | Export Chat → YAML |
| Code dupliqué | Un seul moteur DAG |
| 4 vues basiques | 6 vues VS Code-like |
| Pas de visual feedback | TaskBox animés par verbe |
| Providers manuel | Keyring + auto-select |

---

## Les 6 Vues

### Vue d'Ensemble

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA TUI — 6 VIEWS ARCHITECTURE                                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌───────┬───────┬───────┬───────┬───────┬───────┐                             │
│   │   1   │   2   │   3   │   4   │   5   │   6   │  ← Hotkeys numériques       │
│   │   e   │   c   │   d   │   r   │   s   │   ,   │  ← Hotkeys lettres          │
│   ├───────┴───────┴───────┴───────┴───────┴───────┤                             │
│   │                                               │                             │
│   │  EXPLORER   CHAT    EDITOR   RUNNER  SCHED  SET│                            │
│   │     📁       💬       ✏️       ▶️      📅     ⚙️ │                            │
│   │                                               │                             │
│   └───────────────────────────────────────────────┘                             │
│                                                                                 │
│   Navigation: Tab / Shift+Tab pour cycler                                       │
│               1-6 ou e/c/d/r/s/, pour accès direct                              │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Détail de Chaque Vue

#### [1] Explorer View (`e`)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  EXPLORER VIEW — File Browser + DAG Preview                                     │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─ Files ──────────────────────┬─ DAG Preview ────────────────────────────┐   │
│   │                              │                                          │   │
│   │  📁 workflows/               │      ┌────┐      ┌────┐                  │   │
│   │  ├── 📄 build.nika.yaml      │      │ t1 │──────│ t2 │                  │   │
│   │  ├── 📄 deploy.nika.yaml     │      └────┘      └──┬─┘                  │   │
│   │  ├── 📄 test.nika.yaml ◄     │                     │                    │   │
│   │  └── 📁 templates/           │               ┌─────┴─────┐              │   │
│   │      ├── 📄 seo.nika.yaml    │               ▼           ▼              │   │
│   │      └── 📄 i18n.nika.yaml   │            ┌────┐      ┌────┐            │   │
│   │                              │            │ t3 │      │ t4 │            │   │
│   │  📁 sessions/                │            └────┘      └────┘            │   │
│   │  └── 📄 chat-2026-02-25.yaml │                                          │   │
│   │                              │  Selected: test.nika.yaml                │   │
│   │                              │  Tasks: 4 | Tests: 12                    │   │
│   │                              │                                          │   │
│   ├──────────────────────────────┴──────────────────────────────────────────┤   │
│   │ [Enter] Open in Editor  [Space] Preview  [c] Open Chat  [r] Run         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   FONCTIONNALITÉS                                                               │
│   ───────────────                                                               │
│   • Browse .nika.yaml files in project                                          │
│   • Tree view with folders                                                      │
│   • DAG preview on selection                                                    │
│   • Quick actions: Open, Preview, Run                                           │
│   • Fuzzy search: Ctrl+P                                                        │
│   • Recent files list                                                           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### [2] Chat View (`c`)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CHAT VIEW — Conversational Agent + Live DAG                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─ Conversation ───────────────────────────────────┬─ Live DAG ───────────┐   │
│   │                                                  │                      │   │
│   │  @1 USER: Analyse ce rapport                     │    ┌────┐            │   │
│   │      └─ [📎 report.pdf]                          │    │ @1 │            │   │
│   │                                                  │    │ 👤 │            │   │
│   │  @2 ASSISTANT:                                   │    └──┬─┘            │   │
│   │      ┌─ ⚡ INFER ───────────────────────────┐    │       │              │   │
│   │      │ claude-sonnet-4            streaming │    │       ▼              │   │
│   │      │ ████████████████░░░░░░  78%          │    │    ┌────┐            │   │
│   │      │ Tokens: 1.2K in / 890 out            │    │    │ @2 │            │   │
│   │      └──────────────────────────────────────┘    │    │ ⚡ │◐           │   │
│   │      Voici mon analyse du rapport...             │    └──┬─┘            │   │
│   │                                                  │       │              │   │
│   │  @3 USER: Traduis @2 en français                 │       ▼              │   │
│   │           ▲                                      │    ┌────┐            │   │
│   │           └── @mention référence                 │    │ @3 │            │   │
│   │                                                  │    │ 👤 │            │   │
│   │  @4 ASSISTANT:                                   │    └──┬─┘            │   │
│   │      ┌─ ⚡ INFER ───────────────────────────┐    │       │              │   │
│   │      │ ✅ Success (2.1s)                    │    │       ▼              │   │
│   │      │ context: { from: @2.result }         │    │    ┌────┐            │   │
│   │      └──────────────────────────────────────┘    │    │ @4 │            │   │
│   │      Voici la traduction...                      │    │ ⚡ │●           │   │
│   │                                                  │    └────┘            │   │
│   ├──────────────────────────────────────────────────┴──────────────────────┤   │
│   │ > Message... (@mention, /cmd)    [Ctrl+E] Export  [Ctrl+D] Toggle DAG   │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   FONCTIONNALITÉS                                                               │
│   ───────────────                                                               │
│   • Conversation naturelle avec l'agent                                         │
│   • @N pour référencer n'importe quel message                                   │
│   • TaskBox inline pour chaque action                                           │
│   • Live DAG panel (toggle avec Ctrl+D)                                         │
│   • Export YAML (Ctrl+E)                                                        │
│   • Attachments (fichiers, images)                                              │
│   • /commands (/help, /clear, /export, /history)                                │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### [3] Editor View (`d`)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  EDITOR VIEW — YAML Editing + DAG Sync                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─ YAML Editor ────────────────────────────────────┬─ DAG View ───────────┐   │
│   │                                                  │                      │   │
│   │  1│ schema: nika/workflow@0.5                    │    ┌──────┐          │   │
│   │  2│ workflow: seo-content                        │    │ fetch │          │   │
│   │  3│                                              │    │ data  │          │   │
│   │  4│ tasks:                                       │    └───┬───┘          │   │
│   │  5│   - id: fetch_entity                         │        │              │   │
│   │  6│     invoke: novanet_describe                 │        ▼              │   │
│   │  7│     params:                                  │    ┌──────┐          │   │
│   │  8│       entity: "qr-code"                      │    │generate│◄ selected│   │
│   │  9│     use.ctx: entity                          │    │content │          │   │
│   │ 10│                                              │    └───┬───┘          │   │
│   │ 11│   - id: generate_content  ◄── cursor        │        │              │   │
│   │ 12│     infer:                                   │        ▼              │   │
│   │ 13│       prompt: "Generate SEO content"         │    ┌──────┐          │   │
│   │ 14│       context: $entity                       │    │ save  │          │   │
│   │ 15│     depends_on: [fetch_entity]               │    │output │          │   │
│   │ 16│     use.result: content                      │    └──────┘          │   │
│   │ 17│                                              │                      │   │
│   │ 18│   - id: save_output                          │  ✅ Valid YAML       │   │
│   │ 19│     exec: "echo $content > output.md"        │  ✅ Schema OK        │   │
│   │ 20│     depends_on: [generate_content]           │  ✅ DAG acyclic      │   │
│   │                                                  │                      │   │
│   ├──────────────────────────────────────────────────┴──────────────────────┤   │
│   │ [Ctrl+S] Save  [Ctrl+R] Run  [Ctrl+Z] Undo  [F5] Validate               │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   FONCTIONNALITÉS                                                               │
│   ───────────────                                                               │
│   • Syntax highlighting YAML                                                    │
│   • Live DAG sync (sélection sync)                                              │
│   • Schema validation en temps réel                                             │
│   • Error diagnostics inline (miette)                                           │
│   • Undo/Redo (Ctrl+Z/Y) avec coalescing                                        │
│   • Auto-format on save                                                         │
│   • Jump to definition                                                          │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### [4] Runner View (`r`)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RUNNER VIEW — Execution Monitor + TaskBox Animations                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─ Progress ──────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │  Workflow: seo-content.nika.yaml                                        │   │
│   │  Started: 2026-02-25 14:32:01                                           │   │
│   │  ████████████████████████░░░░░░░░░░  60%  (3/5 tasks)                   │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   ┌─ Tasks ─────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │  ┌─ 🔌 INVOKE ─────────────────────────────────────────────────────────┐│   │
│   │  │ fetch_entity @ novanet                                          [●] ││   │
│   │  │ ✅ Success (340ms)                                                  ││   │
│   │  │ Result: Entity "qr-code" with 12 fields                             ││   │
│   │  └─────────────────────────────────────────────────────────────────────┘│   │
│   │                                                                         │   │
│   │  ┌─ ⚡ INFER ──────────────────────────────────────────────────────────┐│   │
│   │  │ generate_content                                                [◐] ││   │
│   │  │ claude-sonnet-4                                                     ││   │
│   │  │ ████████████████████░░░░░░░░░░  67%  streaming...                   ││   │
│   │  │ Tokens: 2,341 in / 1,567 out   Cost: $0.0089                        ││   │
│   │  │                                                                     ││   │
│   │  │ ░▒▓█ Generating SEO-optimized content for QR Code AI...             ││   │
│   │  │ The landing page should focus on...█                                ││   │
│   │  └─────────────────────────────────────────────────────────────────────┘│   │
│   │                                                                         │   │
│   │  ┌─ 📟 EXEC ───────────────────────────────────────────────────────────┐│   │
│   │  │ save_output                                                     [○] ││   │
│   │  │ Waiting for: generate_content                                       ││   │
│   │  └─────────────────────────────────────────────────────────────────────┘│   │
│   │                                                                         │   │
│   ├─────────────────────────────────────────────────────────────────────────┤   │
│   │ [Esc] Cancel  [R] Retry Failed  [L] View Logs  [T] View Trace           │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   FONCTIONNALITÉS                                                               │
│   ───────────────                                                               │
│   • Progress bar global                                                         │
│   • TaskBox animés par verbe                                                    │
│   • Streaming output en temps réel                                              │
│   • DecryptEffect (░▒▓█) pour le texte                                          │
│   • Cancel/Retry controls                                                       │
│   • Log viewer                                                                  │
│   • Trace export                                                                │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### [5] Scheduler View (`s`)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  SCHEDULER VIEW — Cron & Queue Management                                       │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─ Scheduled Jobs ────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │  WORKFLOW              SCHEDULE           NEXT RUN          STATUS      │   │
│   │  ─────────────────────────────────────────────────────────────────────  │   │
│   │  daily-seo.nika.yaml   0 9 * * *          2026-02-26 09:00  ✅ Active  │   │
│   │  weekly-report.yaml    0 0 * * 1          2026-03-03 00:00  ✅ Active  │   │
│   │  backup.nika.yaml      0 */6 * * *        2026-02-25 18:00  ⏸️ Paused  │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   ┌─ Timeline ──────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │  Today                 Tomorrow             Next Week                   │   │
│   │  ├─────────────────────┼─────────────────────┼─────────────────────     │   │
│   │  │     ●               │  ●                  │  ●                       │   │
│   │  │   daily-seo         │daily-seo            │weekly-report             │   │
│   │  │     14:00           │  09:00              │  00:00                   │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   ┌─ Recent Runs ───────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │  TIME                  WORKFLOW              DURATION    STATUS         │   │
│   │  ───────────────────────────────────────────────────────────────────    │   │
│   │  2026-02-25 09:00      daily-seo.nika.yaml   4.2s        ✅ Success    │   │
│   │  2026-02-24 09:00      daily-seo.nika.yaml   3.8s        ✅ Success    │   │
│   │  2026-02-24 00:00      weekly-report.yaml    12.1s       ❌ Failed     │   │
│   │                                                                         │   │
│   ├─────────────────────────────────────────────────────────────────────────┤   │
│   │ [A] Add Schedule  [E] Edit  [P] Pause/Resume  [D] Delete  [H] History   │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   FONCTIONNALITÉS                                                               │
│   ───────────────                                                               │
│   • Cron expression editor                                                      │
│   • Visual timeline                                                             │
│   • Run history with status                                                     │
│   • Pause/Resume jobs                                                           │
│   • Manual trigger                                                              │
│   • Failure notifications                                                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### [6] Settings View (`,`)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  SETTINGS VIEW — Configuration + Provider Management                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ┌─ Tabs ──────────────────────────────────────────────────────────────────┐   │
│   │  [Providers]  [Theme]  [Editor]  [MCP]  [Sessions]  [About]             │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   ┌─ Providers ─────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │  CLOUD PROVIDERS                                                        │   │
│   │  ───────────────                                                        │   │
│   │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│   │  │  🟣 Claude (Anthropic)                              [Default ✓] │    │   │
│   │  │  API Key: ••••••••••••sk-ant-xxx                    [Change]    │    │   │
│   │  │  Status: ✅ Connected                                           │    │   │
│   │  └─────────────────────────────────────────────────────────────────┘    │   │
│   │                                                                         │   │
│   │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│   │  │  🟢 OpenAI                                                      │    │   │
│   │  │  API Key: ••••••••••••sk-xxx                        [Change]    │    │   │
│   │  │  Status: ✅ Connected                                           │    │   │
│   │  └─────────────────────────────────────────────────────────────────┘    │   │
│   │                                                                         │   │
│   │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│   │  │  🔵 Mistral                                                     │    │   │
│   │  │  API Key: Not configured                            [Add Key]   │    │   │
│   │  │  Status: ⚪ Not connected                                       │    │   │
│   │  └─────────────────────────────────────────────────────────────────┘    │   │
│   │                                                                         │   │
│   │  LOCAL PROVIDERS                                                        │   │
│   │  ───────────────                                                        │   │
│   │  ┌─────────────────────────────────────────────────────────────────┐    │   │
│   │  │  🦙 Ollama                                                      │    │   │
│   │  │  URL: http://localhost:11434                                    │    │   │
│   │  │  Status: ✅ Connected (3 models)                                │    │   │
│   │  │  Models: llama3.2, mistral, codellama                           │    │   │
│   │  │  [Pull Model]  [Delete Model]  [Refresh]                        │    │   │
│   │  └─────────────────────────────────────────────────────────────────┘    │   │
│   │                                                                         │   │
│   ├─────────────────────────────────────────────────────────────────────────┤   │
│   │ [Enter] Select Default  [Tab] Next Tab  [Esc] Close                     │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   FONCTIONNALITÉS                                                               │
│   ───────────────                                                               │
│   • Provider management (6 providers)                                           │
│   • API key storage via NikaKeyring                                             │
│   • Ollama model pull/delete                                                    │
│   • Theme selection (Light/Dark/Solarized)                                      │
│   • Editor preferences                                                          │
│   • MCP server configuration                                                    │
│   • Session management                                                          │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Relations Entre les Vues

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  FLUX DE NAVIGATION — Comment les vues interagissent                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│                                                                                 │
│          ┌──────────────────────────────────────────────────────────┐           │
│          │                                                          │           │
│          │    ┌────────────┐         ┌────────────┐                 │           │
│          │    │  EXPLORER  │──Open──▶│   EDITOR   │                 │           │
│          │    │    [1]     │         │    [3]     │                 │           │
│          │    └─────┬──────┘         └─────┬──────┘                 │           │
│          │          │                      │                        │           │
│          │    Open Chat              Run   │  Export                │           │
│          │          │                      │                        │           │
│          │          ▼                      ▼                        │           │
│          │    ┌────────────┐         ┌────────────┐                 │           │
│          │    │    CHAT    │◀─Export─│   RUNNER   │                 │           │
│          │    │    [2]     │         │    [4]     │                 │           │
│          │    └─────┬──────┘         └─────┬──────┘                 │           │
│          │          │                      │                        │           │
│          │    Settings              Schedule                        │           │
│          │          │                      │                        │           │
│          │          ▼                      ▼                        │           │
│          │    ┌────────────┐         ┌────────────┐                 │           │
│          │    │  SETTINGS  │         │ SCHEDULER  │                 │           │
│          │    │    [6]     │         │    [5]     │                 │           │
│          │    └────────────┘         └────────────┘                 │           │
│          │                                                          │           │
│          └──────────────────────────────────────────────────────────┘           │
│                                                                                 │
│                                                                                 │
│   ACTIONS CROSS-VIEW                                                            │
│   ──────────────────                                                            │
│                                                                                 │
│   Explorer → Editor      Double-click ou Enter sur un fichier                   │
│   Explorer → Chat        "c" sur un fichier ouvre Chat avec contexte            │
│   Explorer → Runner      "r" sur un fichier lance l'exécution                   │
│                                                                                 │
│   Chat → Editor          Ctrl+E exporte en YAML et ouvre Editor                 │
│   Chat → Settings        Changer de provider via commande                       │
│                                                                                 │
│   Editor → Runner        Ctrl+R lance le workflow                               │
│   Editor → Chat          Importer un YAML comme contexte Chat                   │
│                                                                                 │
│   Runner → Scheduler     "s" sur un workflow terminé propose scheduling         │
│   Runner → Editor        "e" ouvre le YAML source pour debug                    │
│                                                                                 │
│   Scheduler → Runner     Trigger manuel lance le Runner                         │
│   Scheduler → Editor     "e" ouvre le YAML pour modification                    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Les 5 TaskBox

### Vue d'Ensemble

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  TASKBOX — Un widget visuel par verbe sémantique                                │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   Chaque action dans Nika utilise UN des 5 verbes sémantiques.                  │
│   Chaque verbe a son widget visuel (TaskBox) avec couleur et icône uniques.     │
│                                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │   ⚡ INFER      📟 EXEC       🛰️ FETCH      🔌 INVOKE      🐔 AGENT    │   │
│   │   Violet        Amber         Cyan          Emerald        Rose         │   │
│   │   #8b5cf6       #f59e0b       #06b6d4       #10b981        #f43f5e      │   │
│   │                                                                         │   │
│   │   LLM call      Shell cmd     HTTP req      MCP tool       Multi-turn   │   │
│   │   streaming     execution     with retry    invocation     agentic      │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│   + 🐤 SUBAGENT (Rose clair, spawned par agent:, montre la profondeur)         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Détail de Chaque TaskBox

#### ⚡ InferBox (Violet)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ⚡ INFERBOX — LLM Text Generation                                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   MODE COMPACT (dans Chat)                                                      │
│   ┌─ ⚡ INFER ─────────────────────────────────────────────────────────────┐    │
│   │ claude-sonnet-4-20250514                                           [▼] │    │
│   │ ████████████████████████████░░░░░░  78%  streaming...                  │    │
│   │ Tokens: 1,892 in / 1,245 out   Cost: $0.0067   Elapsed: 4.2s           │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   MODE EXPANDED (dans Runner)                                                   │
│   ┌─ ⚡ INFER ─────────────────────────────────────────────────────────────┐    │
│   │ claude-sonnet-4-20250514                                           [▼] │    │
│   │ ████████████████████████████░░░░░░  78%  streaming...                  │    │
│   │                                                                        │    │
│   │ ░▒▓█ The landing page for QR Code AI should emphasize the             │    │
│   │ simplicity and power of generating custom QR codes. Key               │    │
│   │ messaging points include...█                                          │    │
│   │                                                                        │    │
│   │ ┌─ Extended Thinking ──────────────────────────────────────────────┐   │    │
│   │ │ I need to analyze the target audience for QR Code AI...          │   │    │
│   │ │ The main value propositions are:                                 │   │    │
│   │ │ 1. Easy generation                                               │   │    │
│   │ │ 2. Customization options                                         │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ Tokens: 1,892 in / 1,245 out   Cost: $0.0067   Elapsed: 4.2s           │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   ÉLÉMENTS                                                                      │
│   ────────                                                                      │
│   • Model name (claude-sonnet-4, gpt-4o, etc.)                                  │
│   • Progress bar avec pourcentage                                               │
│   • Token counter (input / output)                                              │
│   • Cost estimation                                                             │
│   • Elapsed time                                                                │
│   • Streaming text avec DecryptEffect (░▒▓█)                                    │
│   • Extended thinking panel (Claude only)                                       │
│                                                                                 │
│   ÉTATS                                                                         │
│   ─────                                                                         │
│   ○ Queued      — En attente de dépendances                                     │
│   ◐ Running     — Streaming en cours (spinner braille ⣾⣽⣻⢿⡿⣟⣯⣷)               │
│   ● Success     — Terminé avec succès                                          │
│   ✕ Failed      — Erreur (affiche message d'erreur)                             │
│   ⊘ Skipped     — Ignoré (dépendance échouée)                                   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### 📟 ExecBox (Amber)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  📟 EXECBOX — Shell Command Execution                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   MODE COMPACT                                                                  │
│   ┌─ 📟 EXEC ──────────────────────────────────────────────────────────────┐    │
│   │ npm run build                                                      [▼] │    │
│   │ ✅ Exit 0 (4.2s)                                                       │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   MODE EXPANDED                                                                 │
│   ┌─ 📟 EXEC ──────────────────────────────────────────────────────────────┐    │
│   │ npm run build                                                      [▼] │    │
│   │ ████████████████████████████████████████  100%                         │    │
│   │                                                                        │    │
│   │ ┌─ stdout ─────────────────────────────────────────────────────────┐   │    │
│   │ │ > nika@0.8.0 build                                               │   │    │
│   │ │ > tsc && esbuild src/index.ts --bundle                           │   │    │
│   │ │                                                                  │   │    │
│   │ │ Built 42 files in 2.1s                                           │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ ✅ Exit 0   Duration: 4.2s   Output: 156 lines                         │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   MODE ERREUR                                                                   │
│   ┌─ 📟 EXEC ──────────────────────────────────────────────────────────────┐    │
│   │ npm run build                                                      [▼] │    │
│   │ ❌ Exit 1 (2.1s)                                                       │    │
│   │                                                                        │    │
│   │ ┌─ stderr ─────────────────────────────────────────────────────────┐   │    │
│   │ │ error: Cannot find module 'missing-dep'                          │   │    │
│   │ │   at node:internal/modules/cjs/loader:1147:27                    │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   ÉLÉMENTS                                                                      │
│   ────────                                                                      │
│   • Command line                                                                │
│   • Exit code (0 = success, autre = error)                                      │
│   • Duration                                                                    │
│   • stdout/stderr panels                                                        │
│   • Line count                                                                  │
│   • Truncation indicator si output trop long                                    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### 🛰️ FetchBox (Cyan)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🛰️ FETCHBOX — HTTP Requests                                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   MODE COMPACT                                                                  │
│   ┌─ 🛰️ FETCH ─────────────────────────────────────────────────────────────┐    │
│   │ GET https://api.example.com/data                                   [▼] │    │
│   │ ✅ 200 OK (340ms) — Response: 2.1KB JSON                               │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   MODE EXPANDED                                                                 │
│   ┌─ 🛰️ FETCH ─────────────────────────────────────────────────────────────┐    │
│   │ GET https://api.example.com/data                                   [▼] │    │
│   │                                                                        │    │
│   │ ┌─ Request ────────────────────────────────────────────────────────┐   │    │
│   │ │ Headers:                                                         │   │    │
│   │ │   Authorization: Bearer •••••••                                  │   │    │
│   │ │   Content-Type: application/json                                 │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ ┌─ Response ───────────────────────────────────────────────────────┐   │    │
│   │ │ Status: 200 OK                                                   │   │    │
│   │ │ Time: 340ms                                                      │   │    │
│   │ │ Size: 2.1KB                                                      │   │    │
│   │ │                                                                  │   │    │
│   │ │ {                                                                │   │    │
│   │ │   "data": [...],                                                 │   │    │
│   │ │   "meta": { "total": 42 }                                        │   │    │
│   │ │ }                                                                │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   ÉLÉMENTS                                                                      │
│   ────────                                                                      │
│   • Method (GET, POST, PUT, DELETE, etc.)                                       │
│   • URL                                                                         │
│   • Status code avec couleur (2xx=vert, 4xx=orange, 5xx=rouge)                  │
│   • Response time                                                               │
│   • Response size                                                               │
│   • Headers preview                                                             │
│   • Body preview (JSON formatted)                                               │
│   • Retry indicator si retry automatique                                        │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### 🔌 InvokeBox (Emerald)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🔌 INVOKEBOX — MCP Tool Calls                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   MODE COMPACT                                                                  │
│   ┌─ 🔌 INVOKE ────────────────────────────────────────────────────────────┐    │
│   │ novanet_describe @ novanet                                         [▼] │    │
│   │ ├─ entity: "qr-code"                                                   │    │
│   │ └─ ✅ Success (280ms) — Result: 2.1KB JSON                             │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   MODE EXPANDED                                                                 │
│   ┌─ 🔌 INVOKE ────────────────────────────────────────────────────────────┐    │
│   │ novanet_describe @ novanet                                         [▼] │    │
│   │                                                                        │    │
│   │ ┌─ Parameters ─────────────────────────────────────────────────────┐   │    │
│   │ │ entity: "qr-code"                                                │   │    │
│   │ │ locale: "fr-FR"                                                  │   │    │
│   │ │ forms: ["text", "title"]                                         │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ ┌─ Result ─────────────────────────────────────────────────────────┐   │    │
│   │ │ {                                                                │   │    │
│   │ │   "entity": {                                                    │   │    │
│   │ │     "key": "qr-code",                                            │   │    │
│   │ │     "native": {                                                  │   │    │
│   │ │       "text": "QR Code",                                         │   │    │
│   │ │       "title": "Code QR"                                         │   │    │
│   │ │     }                                                            │   │    │
│   │ │   }                                                              │   │    │
│   │ │ }                                                                │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ ✅ Success   Duration: 280ms   Server: novanet                         │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   ÉLÉMENTS                                                                      │
│   ────────                                                                      │
│   • Tool name                                                                   │
│   • Server name (@ notation)                                                    │
│   • Parameters tree view                                                        │
│   • Result JSON formatted                                                       │
│   • Duration                                                                    │
│   • Retry button si erreur                                                      │
│   • Server status indicator                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

#### 🐔 AgentBox (Rose)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  🐔 AGENTBOX — Multi-turn Agentic Loop                                          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   MODE COMPACT                                                                  │
│   ┌─ 🐔 AGENT ─────────────────────────────────────────────────────────────┐    │
│   │ Research competitors and summarize                                 [▼] │    │
│   │ Turn 3/5   ████████████░░░░░░░░   60%                                  │    │
│   │ Tokens: 4,521 in / 2,103 out   Cost: $0.0189                           │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   MODE EXPANDED                                                                 │
│   ┌─ 🐔 AGENT ─────────────────────────────────────────────────────────────┐    │
│   │ Research competitors and summarize                                 [▼] │    │
│   │ Turn 3/5   ████████████░░░░░░░░   60%                                  │    │
│   │                                                                        │    │
│   │ ┌─ Turn History ───────────────────────────────────────────────────┐   │    │
│   │ │                                                                  │   │    │
│   │ │ Turn 1:                                                          │   │    │
│   │ │   ├─ 🔌 novanet_search (✅ 420ms)                                │   │    │
│   │ │   │   └─ query: "qr code competitors"                            │   │    │
│   │ │   └─ ⚡ Analyzing search results...                              │   │    │
│   │ │                                                                  │   │    │
│   │ │ Turn 2:                                                          │   │    │
│   │ │   ├─ 🛰️ fetch https://competitor1.com (✅ 890ms)                 │   │    │
│   │ │   └─ 🛰️ fetch https://competitor2.com (✅ 720ms)                 │   │    │
│   │ │                                                                  │   │    │
│   │ │ Turn 3 (current):                                                │   │    │
│   │ │   └─ ⚡ Generating summary... (streaming)                        │   │    │
│   │ │                                                                  │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ ┌─ Current Output ─────────────────────────────────────────────────┐   │    │
│   │ │ Based on my research, the main competitors are:                  │   │    │
│   │ │ 1. QRCode.com - Focus on simplicity...█                          │   │    │
│   │ └──────────────────────────────────────────────────────────────────┘   │    │
│   │                                                                        │    │
│   │ Tokens: 4,521 in / 2,103 out   Cost: $0.0189   Elapsed: 12.4s          │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   SUBAGENT (spawned par spawn_agent)                                            │
│   ┌─ 🐤 SUBAGENT [depth: 2] ───────────────────────────────────────────────┐    │
│   │ Analyze competitor pricing                                         [▼] │    │
│   │ Turn 2/3   ████████░░░░   66%                                          │    │
│   │ Parent: main-agent                                                     │    │
│   └────────────────────────────────────────────────────────────────────────┘    │
│                                                                                 │
│   ÉLÉMENTS                                                                      │
│   ────────                                                                      │
│   • Agent goal/prompt                                                           │
│   • Turn counter (current/max)                                                  │
│   • Progress bar                                                                │
│   • Turn history avec nested TaskBox                                            │
│   • Tool call timeline                                                          │
│   • Current streaming output                                                    │
│   • Token/cost totaux                                                           │
│   • Depth indicator pour subagents                                              │
│   • Stop conditions status                                                      │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### États et Animations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  TASKBOX — États et Animations                                                  │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   ÉTATS (BoxState)                                                              │
│   ────────────────                                                              │
│                                                                                 │
│   ○ Queued       Gris        En attente de dépendances                          │
│   ◐ Running      Couleur     Spinner braille: ⣾⣽⣻⢿⡿⣟⣯⣷                        │
│   ● Success      Vert        Checkmark ✅                                        │
│   ✕ Failed       Rouge       Croix ❌ + message d'erreur                         │
│   ⊘ Skipped      Gris        Barré (dépendance échouée)                         │
│                                                                                 │
│                                                                                 │
│   ANIMATIONS                                                                    │
│   ──────────                                                                    │
│                                                                                 │
│   DecryptEffect (streaming text)                                                │
│   ░░░░░░░░░░ → ░▒░░░░░░░░ → ░▒▓░░░░░░░ → ░▒▓█░░░░░░ → Hello█                    │
│                                                                                 │
│   Progress Bar                                                                  │
│   ░░░░░░░░░░ → ███░░░░░░░ → ██████░░░░ → ██████████                              │
│                                                                                 │
│   Braille Spinner (60fps)                                                       │
│   ⣾ → ⣽ → ⣻ → ⢿ → ⡿ → ⣟ → ⣯ → ⣷ → (repeat)                                      │
│                                                                                 │
│   State Transitions                                                             │
│   Queued ──start──▶ Running ──complete──▶ Success                               │
│                         │                                                       │
│                         └──error──▶ Failed                                      │
│                                                                                 │
│                                                                                 │
│   RENDER MODES                                                                  │
│   ────────────                                                                  │
│                                                                                 │
│   Compact     4-10 lignes    Dans Chat inline                                   │
│   Expanded    15-60 lignes   Dans Runner, expandable                            │
│   Full        Illimité       Détail complet, scrollable                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## @Mention et Bindings

### Syntaxe @Mention

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  @MENTION — Référencer n'importe quel message                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   SYNTAXE                                                                       │
│   ───────                                                                       │
│                                                                                 │
│   @N              Contenu du message N                                          │
│   @N.result       Résultat du TaskBox dans message N                            │
│   @N.thinking     Extended thinking (Claude only)                               │
│   @N.error        Message d'erreur si échec                                     │
│   @last           Dernier message                                               │
│   @last.result    Résultat du dernier TaskBox                                   │
│                                                                                 │
│                                                                                 │
│   EXEMPLE DE CONVERSATION                                                       │
│   ───────────────────────                                                       │
│                                                                                 │
│   @1 USER: Analyse ce fichier                                                   │
│       └─ [📎 report.pdf]                                                        │
│                                                                                 │
│   @2 ASSISTANT: Voici l'analyse...                                              │
│       └─ [⚡ INFER — tokens: 2.1K, cost: $0.008]                                │
│       └─ result: { summary: "...", key_points: [...] }                          │
│                                                                                 │
│   @3 USER: Traduis @2 en français                                               │
│            ▲                                                                    │
│            └── Le système sait que @2 = message ASSISTANT avec result           │
│                                                                                 │
│   @4 ASSISTANT: Voici la traduction...                                          │
│       └─ context: { from: @2.result }   ◄── Binding automatique                 │
│                                                                                 │
│   @5 USER: Compare @2 et @4                                                     │
│            ▲     ▲                                                              │
│            │     └── Multi-référence possible                                   │
│            └──────── Le DAG crée des arcs vers @2 et @4                         │
│                                                                                 │
│                                                                                 │
│   POURQUOI STABLEGRAPH ?                                                        │
│   ──────────────────────                                                        │
│                                                                                 │
│   Avec petgraph::StableGraph, le NodeIndex est STABLE.                          │
│   Si on supprime le message @3, les index @4 et @5 ne changent PAS.             │
│                                                                                 │
│   Avant (Vec):   @1 @2 @3 @4 @5                                                 │
│   Delete @3:     @1 @2 @3 @4  ← @4 devient @3, confusion!                       │
│                                                                                 │
│   Avec StableGraph:                                                             │
│   Avant:         @1 @2 @3 @4 @5                                                 │
│   Delete @3:     @1 @2 __ @4 @5  ← @4 reste @4, stable!                         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Comment les Bindings Fonctionnent

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  BINDINGS — Data flow entre tasks                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   DANS LE CHAT                                                                  │
│   ────────────                                                                  │
│                                                                                 │
│   User tape: "Traduis @2 en français"                                           │
│                                                                                 │
│   1. Parser détecte @2                                                          │
│   2. Système résout @2 → NodeIndex(2)                                           │
│   3. Récupère @2.result (output du TaskBox)                                     │
│   4. Injecte dans le contexte du prochain infer:                                │
│                                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐       │
│   │  Prompt envoyé au LLM:                                              │       │
│   │                                                                     │       │
│   │  System: You are translating content.                               │       │
│   │  Context: { "source": "<contenu de @2.result>" }                    │       │
│   │  User: Traduis en français                                          │       │
│   └─────────────────────────────────────────────────────────────────────┘       │
│                                                                                 │
│                                                                                 │
│   DANS LE YAML EXPORTÉ                                                          │
│   ────────────────────                                                          │
│                                                                                 │
│   tasks:                                                                        │
│     - id: msg-002-infer                                                         │
│       infer: "Analyse ce fichier"                                               │
│       use.result: analysis                      ◄── Output binding              │
│                                                                                 │
│     - id: msg-004-infer                                                         │
│       infer: "Traduis en français"                                              │
│       context: $analysis                        ◄── Input binding               │
│       depends_on: [msg-002-infer]               ◄── DAG edge                    │
│                                                                                 │
│                                                                                 │
│   TYPES DE BINDINGS                                                             │
│   ─────────────────                                                             │
│                                                                                 │
│   use.alias: task.result    Eager binding (résolu immédiatement)                │
│                                                                                 │
│   use:                      Lazy binding (résolu à l'accès)                     │
│     alias:                                                                      │
│       path: task.result                                                         │
│       lazy: true                                                                │
│       default: "fallback"                                                       │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Export YAML

### Le Processus d'Export

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  EXPORT YAML — Conversation → Workflow reproductible                            │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   CHAT SESSION                           YAML EXPORTÉ                           │
│   ════════════                           ════════════                           │
│                                                                                 │
│   @1 USER: Get QR code entity            schema: nika/workflow@0.5              │
│                                          workflow: chat-export-2026-02-25       │
│   @2 ASSISTANT:                          description: |                         │
│     🔌 novanet_describe                    Exported from chat session           │
│     entity: qr-code ✅                     at 14:32:01                          │
│                                                                                 │
│   @3 USER: Generate landing              mcp:                                   │
│            page using @2                   servers:                             │
│                                              novanet:                           │
│   @4 ASSISTANT:                                command: node                    │
│     ⚡ claude-sonnet-4 ✅                      args: [novanet-mcp/index.js]     │
│                                                                                 │
│   @5 USER: Save to file                  tasks:                                 │
│                                            - id: msg-002-invoke                 │
│   @6 ASSISTANT:                              invoke: novanet_describe           │
│     📟 echo > output.md ✅                   params:                            │
│                                                entity: "qr-code"                │
│         │                                    use.ctx: entity_data               │
│         │                                                                       │
│         │  [Ctrl+E]                        - id: msg-004-infer                  │
│         │                                    infer:                             │
│         ▼                                      prompt: "Generate landing..."    │
│   ══════════════                               context: $entity_data            │
│                                              depends_on: [msg-002-invoke]       │
│                                              use.result: content                │
│                                                                                 │
│                                            - id: msg-006-exec                   │
│                                              exec: "echo $content > output.md" │
│                                              depends_on: [msg-004-infer]        │
│                                                                                 │
│                                                                                 │
│   RÈGLES DE CONVERSION                                                          │
│   ────────────────────                                                          │
│                                                                                 │
│   1. Chaque message ASSISTANT avec TaskBox → task                               │
│   2. Les @mentions → depends_on + context binding                               │
│   3. Les USER messages → ignorés (sauf attachments)                             │
│   4. Les MCP servers utilisés → block mcp:                                      │
│   5. IDs générés: msg-{index}-{verb}                                            │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Cycle de Vie Complet

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  CYCLE DE VIE — Du Chat au Scheduling                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│                                                                                 │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │   1. CRÉATION (Chat View)                                               │   │
│   │   ─────────────────────────                                             │   │
│   │                                                                         │   │
│   │   User: "Analyse ce fichier et génère un rapport"                       │   │
│   │                     │                                                   │   │
│   │                     ▼                                                   │   │
│   │   ┌─────────────────────────────────────────┐                           │   │
│   │   │     DAG se construit en temps réel      │                           │   │
│   │   │     @1 ──▶ @2 ──▶ @3 ──▶ @4             │                           │   │
│   │   └─────────────────────────────────────────┘                           │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                  │
│                              │ Ctrl+E                                           │
│                              ▼                                                  │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │   2. EXPORT (→ Editor View)                                             │   │
│   │   ─────────────────────────                                             │   │
│   │                                                                         │   │
│   │   workflow: report-generator.nika.yaml                                  │   │
│   │                     │                                                   │   │
│   │                     ▼                                                   │   │
│   │   ┌─────────────────────────────────────────┐                           │   │
│   │   │     YAML avec tasks et depends_on       │                           │   │
│   │   │     Éditable, versionnable              │                           │   │
│   │   └─────────────────────────────────────────┘                           │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                  │
│                              │ Ctrl+R ou Save                                   │
│                              ▼                                                  │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │   3. EXÉCUTION (Runner View)                                            │   │
│   │   ──────────────────────────                                            │   │
│   │                                                                         │   │
│   │   ████████████████░░░░░░░░  60%                                         │   │
│   │                     │                                                   │   │
│   │                     ▼                                                   │   │
│   │   ┌─────────────────────────────────────────┐                           │   │
│   │   │     TaskBox animés, output live         │                           │   │
│   │   │     Trace NDJSON générée                │                           │   │
│   │   └─────────────────────────────────────────┘                           │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                  │
│                              │ Success → Schedule                               │
│                              ▼                                                  │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │   4. AUTOMATION (Scheduler View)                                        │   │
│   │   ──────────────────────────────                                        │   │
│   │                                                                         │   │
│   │   Cron: 0 9 * * *  (tous les jours à 9h)                                │   │
│   │                     │                                                   │   │
│   │                     ▼                                                   │   │
│   │   ┌─────────────────────────────────────────┐                           │   │
│   │   │     Exécution automatique               │                           │   │
│   │   │     Historique des runs                 │                           │   │
│   │   └─────────────────────────────────────────┘                           │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                              │                                                  │
│                              │ Itération                                        │
│                              ▼                                                  │
│   ┌─────────────────────────────────────────────────────────────────────────┐   │
│   │                                                                         │   │
│   │   5. AMÉLIORATION (Chat → Edit → Run → ...)                             │   │
│   │   ─────────────────────────────────────────                             │   │
│   │                                                                         │   │
│   │   Le cycle recommence avec le workflow amélioré                         │   │
│   │                                                                         │   │
│   └─────────────────────────────────────────────────────────────────────────┘   │
│                                                                                 │
│                                                                                 │
│   RÉSULTAT: Une conversation éphémère devient un artefact permanent            │
│   ─────────────────────────────────────────────────────────────────            │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Roadmap des Releases

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  ROADMAP v0.9 → v0.12                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│   v0.9.x "Chat-as-DAG"                                                          │
│   ════════════════════                                                          │
│   32 tasks, 131 tests                                                           │
│                                                                                 │
│   v0.9.0  StableGraph        petgraph migration, unified DAG                    │
│   v0.9.1  ChatWorkflow       Chat as DAG wrapper                                │
│   v0.9.2  @mention Parser    @N syntax, reference resolution                    │
│   v0.9.3  Builtin Tools      6 nika:* tools (export, history, etc.)             │
│                                                                                 │
│           │                                                                     │
│           ▼                                                                     │
│   ═══════════════════════════════════════════════════════════════════════════   │
│                                                                                 │
│   v0.10.x "TaskBox"                    v0.12.x "Providers" (PARALLEL)           │
│   ═════════════════                    ═══════════════════════════════          │
│   22 tasks, 75 tests                   15 tasks, 45 tests                       │
│                                                                                 │
│   v0.10.0  NodeBox Widget              v0.12.0  Keyring Wiring                  │
│   v0.10.1  EdgeLine Widget             v0.12.1  Env Migration                   │
│   v0.10.2  TaskQueue Widget            v0.12.2  Provider Auto-Select            │
│   v0.10.3  ChatDagPanel                v0.12.3  Ollama Enhancement              │
│   v0.10.4  Animation Polish                                                     │
│                                                                                 │
│           │                                   │                                 │
│           └───────────────┬───────────────────┘                                 │
│                           ▼                                                     │
│   ═══════════════════════════════════════════════════════════════════════════   │
│                                                                                 │
│   v0.11.x "Six Views"                                                           │
│   ═══════════════════                                                           │
│   30 tasks, 90 tests                                                            │
│                                                                                 │
│   v0.11.0  Explorer View      File browser + DAG preview                        │
│   v0.11.1  Editor View        YAML editor + DAG sync                            │
│   v0.11.2  Runner View        Execution monitor + TaskBox                       │
│   v0.11.3  Scheduler View     Cron + queue management                           │
│   v0.11.4  Settings View      Providers + config (74% reuse)                    │
│   v0.11.5  Navigation Update  TuiView enum → 6 variants                         │
│                                                                                 │
│                                                                                 │
│   ═══════════════════════════════════════════════════════════════════════════   │
│   TOTAL: 99 tasks, 341 tests, ~18 sessions (~9 days)                            │
│   ═══════════════════════════════════════════════════════════════════════════   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Résumé Exécutif

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                                                                               ║
║   NIKA v0.9-v0.12 EN UNE PAGE                                                ║
║   ═══════════════════════════                                                 ║
║                                                                               ║
║   PROBLÈME: Chat et Workflow sont deux mondes séparés                         ║
║   SOLUTION: Unifier sous un DAG unique (StableGraph)                          ║
║                                                                               ║
║   ┌─────────────────────────────────────────────────────────────────────┐     ║
║   │                                                                     │     ║
║   │   CHAT                    DAG                     YAML              │     ║
║   │   ─────                   ───                     ────              │     ║
║   │   Tu parles        →      Se construit     →      Exportable        │     ║
║   │   @mention         →      Arcs se créent   →      depends_on        │     ║
║   │   TaskBox inline   →      Nœuds visuels    →      tasks:            │     ║
║   │                                                                     │     ║
║   └─────────────────────────────────────────────────────────────────────┘     ║
║                                                                               ║
║   6 VUES (VS Code-like)                                                       ║
║   ─────────────────────                                                       ║
║   [1] Explorer   — Browse files                                               ║
║   [2] Chat       — Conversational agent + Live DAG                            ║
║   [3] Editor     — YAML editing + validation                                  ║
║   [4] Runner     — Execution monitor + TaskBox                                ║
║   [5] Scheduler  — Cron automation                                            ║
║   [6] Settings   — Providers + config                                         ║
║                                                                               ║
║   5 TASKBOX (un par verbe)                                                    ║
║   ────────────────────────                                                    ║
║   ⚡ INFER    — LLM generation (violet)                                       ║
║   📟 EXEC     — Shell commands (amber)                                        ║
║   🛰️ FETCH    — HTTP requests (cyan)                                          ║
║   🔌 INVOKE   — MCP tool calls (emerald)                                      ║
║   🐔 AGENT    — Multi-turn loops (rose)                                       ║
║                                                                               ║
║   LE RÉSULTAT                                                                 ║
║   ───────────                                                                 ║
║   Chaque conversation avec l'IA devient un workflow reproductible.            ║
║   Le Chat n'est plus éphémère. C'est un artefact versionnable.               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Documents Associés

| Document | Emplacement | Purpose |
|----------|-------------|---------|
| **INDEX v0.9** | [v0.9.1/INDEX.md](./v0.9.1/INDEX.md) | Plans Chat-as-DAG |
| **INDEX v0.10** | [v0.10/INDEX.md](./v0.10/INDEX.md) | Plans TaskBox |
| **INDEX v0.11** | [v0.11/INDEX.md](./v0.11/INDEX.md) | Plans Six Views |
| **INDEX v0.12** | [v0.12/INDEX.md](./v0.12/INDEX.md) | Plans Providers |
| **ROADMAP** | [v0.9.1/ROADMAP.md](./v0.9.1/ROADMAP.md) | Roadmap consolidée |
| **6-Views Design** | [v0.10+/2026-02-24-v010-v012-6-views-design.md](./v0.10+/2026-02-24-v010-v012-6-views-design.md) | Design complet TUI |

---

**NO v1.0 — We stay in 0.XX versioning.**
