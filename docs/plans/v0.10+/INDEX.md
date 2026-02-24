# Nika v0.10.x+ Release Plan

**Codename:** "6-View Production TUI"
**Prerequisite:** v0.9.5 (File-First Agentic complete)

---

## Version Roadmap

```
v0.10.x — 6-View Production TUI
├── v0.10.0 — Explorer View + Editor View
├── v0.10.1 — Runner View + Live DAG Panel
├── v0.10.2 — Scheduler View + heartbeat.yaml
└── v0.10.3 — Settings View + Provider Modal v2

v0.11.x — Advanced Features
├── v0.11.0 — Provider Modal v2 (full)
├── v0.11.1 — Ollama Native Client
└── v0.11.2 — Keyring Integration

v0.12.x — Polish & Performance
├── v0.12.0 — NovaNet Tree Effects
├── v0.12.1 — Minimap + 60fps Animations
└── v0.12.2 — Production Hardening
```

---

## v0.10.0 — Explorer View + Editor View

**Focus:** File navigation + YAML editing
**Effort:** ~1,200 LOC | 4-5 days | 100 tests

### 6-View Architecture

```
[1] EXPLORER → [2] CHAT → [3] EDITOR → [4] RUNNER → [5] SCHEDULER → [6] SETTINGS
     📁           💬          ✏️           ▶️            📅             ⚙️
   DEFAULT      Tab →       Tab →        Tab →        Tab →          Tab →
```

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B10.1** View Navigation | Tab/number keys switch views, enum-based routing | 20 | 3-4 |
| **B10.2** Explorer View | File tree, .nika.yaml discovery, recent runs | 25 | 4-5 |
| **B10.3** Editor View | YAML editing, syntax highlighting, live validation | 30 | 5-6 |
| **B10.4** View State | Per-view state preservation across switches | 15 | 2-3 |
| **B10.5** Keybinding Unification | Consistent shortcuts across all views | 10 | 2-3 |

### B10.1 View Navigation — Detailed Tasks

```yaml
tasks:
  - id: view-enum
    description: Create TuiView enum (Explorer, Chat, Editor, Runner, Scheduler, Settings)
    file: src/tui/views/mod.rs
    effort: 30min

  - id: view-router
    description: ViewRouter with current_view and switch logic
    file: src/tui/router.rs (new)
    effort: 1.5hr
    depends_on: [view-enum]

  - id: tab-navigation
    description: Tab key cycles through views (1→2→3→4→5→6→1)
    file: src/tui/app.rs
    effort: 1hr
    depends_on: [view-router]

  - id: number-shortcuts
    description: 1-6 keys jump directly to view
    file: src/tui/app.rs
    effort: 30min
    depends_on: [view-router]

  - id: status-bar-view
    description: Show current view in status bar
    file: src/tui/widgets/status_bar.rs
    effort: 30min
    depends_on: [view-enum]
```

### Deliverables

- [ ] 6-view enum and router
- [ ] Tab navigation between views
- [ ] Number key shortcuts (1-6)
- [ ] Explorer view with file tree
- [ ] Editor view with syntax highlighting
- [ ] 100 new tests passing

---

## v0.10.1 — Runner View + Live DAG Panel

**Focus:** Workflow execution visualization
**Effort:** ~1,000 LOC | 4-5 days | 80 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B10.6** Runner View | Workflow execution, real-time output | 25 | 4-5 |
| **B10.7** Live DAG Panel | StableGraph visualization during execution | 30 | 5-6 |
| **B10.8** Task Status Icons | Verb-specific icons (⚡🔌📟🛰️🐔) | 15 | 2-3 |
| **B10.9** Progress Indicators | Per-task progress bars, spinners | 10 | 2-3 |

### Deliverables

- [ ] Runner view with live output
- [ ] DAG panel showing execution progress
- [ ] Verb-specific task icons
- [ ] Progress indicators
- [ ] 80 new tests passing

---

## v0.10.2 — Scheduler View + heartbeat.yaml

**Focus:** Cron automation management
**Effort:** ~800 LOC | 3-4 days | 60 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B10.10** Scheduler View | List scheduled workflows, next run times | 20 | 3-4 |
| **B10.11** Heartbeat Parser | Parse heartbeat.yaml cron expressions | 20 | 3-4 |
| **B10.12** Timeline Widget | Visual timeline of scheduled runs | 15 | 3-4 |
| **B10.13** Manual Trigger | Run scheduled workflow on demand | 5 | 1-2 |

### Deliverables

- [ ] Scheduler view listing heartbeats
- [ ] Cron expression parsing
- [ ] Visual timeline
- [ ] Manual trigger capability
- [ ] 60 new tests passing

---

## v0.10.3 — Settings View + Provider Modal v2 Preview

**Focus:** Configuration UI
**Effort:** ~600 LOC | 2-3 days | 40 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B10.14** Settings View | Config display, theme selection | 15 | 2-3 |
| **B10.15** Provider List | Show configured providers with status | 15 | 2-3 |
| **B10.16** Config Editor | Edit .nika/config.toml inline | 10 | 2-3 |

### Deliverables

- [ ] Settings view with config display
- [ ] Provider list with status indicators
- [ ] Inline config editing
- [ ] 40 new tests passing
- [ ] **v0.10.3 Release Ready**

---

## v0.11.0 — Provider Modal v2 (Full)

**Focus:** Advanced provider management
**Effort:** ~1,200 LOC | 5-6 days | 100 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B11.1** Tabbed Provider Modal | Tabs for each provider type | 20 | 3-4 |
| **B11.2** Provider Configuration | API key entry, model selection | 25 | 4-5 |
| **B11.3** Connection Testing | Test provider connectivity | 20 | 3-4 |
| **B11.4** Cost Estimation | Show estimated costs per provider | 15 | 2-3 |
| **B11.5** Default Selection | Set default provider/model | 10 | 2-3 |

### Deliverables

- [ ] Tabbed provider modal (6 providers)
- [ ] Per-provider configuration
- [ ] Connection testing
- [ ] Cost estimation display
- [ ] Default provider selection
- [ ] 100 new tests passing

---

## v0.11.1 — Ollama Native Client

**Focus:** Local LLM support
**Effort:** ~600 LOC | 3-4 days | 50 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B11.6** Ollama Discovery | Auto-detect Ollama at localhost:11434 | 15 | 2-3 |
| **B11.7** Model Listing | List available Ollama models | 15 | 2-3 |
| **B11.8** Pull Progress | Show model download progress | 15 | 3-4 |
| **B11.9** Local-First Mode | Prefer Ollama when available | 5 | 1-2 |

### Deliverables

- [ ] Ollama auto-discovery
- [ ] Model listing and selection
- [ ] Download progress display
- [ ] Local-first mode
- [ ] 50 new tests passing

---

## v0.11.2 — Keyring Integration

**Focus:** Secure credential storage
**Effort:** ~400 LOC | 2-3 days | 30 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B11.10** Keyring Abstraction | Cross-platform keyring access | 15 | 3-4 |
| **B11.11** API Key Storage | Store API keys in system keyring | 10 | 2-3 |
| **B11.12** Migration | Migrate .env keys to keyring | 5 | 1-2 |

### Deliverables

- [ ] Cross-platform keyring support
- [ ] API keys stored securely
- [ ] Migration from .env
- [ ] 30 new tests passing
- [ ] **v0.11.2 Release Ready**

---

## v0.12.0 — NovaNet Tree Effects

**Focus:** Visual polish
**Effort:** ~500 LOC | 2-3 days | 30 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B12.1** Tree Animations | Expand/collapse with smooth transitions | 15 | 3-4 |
| **B12.2** Node Highlighting | Hover/selection effects | 10 | 2-3 |
| **B12.3** Connection Lines | Curved lines between DAG nodes | 5 | 2-3 |

---

## v0.12.1 — Minimap + 60fps Animations

**Focus:** Performance and polish
**Effort:** ~600 LOC | 3-4 days | 40 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B12.4** DAG Minimap | Small overview of full DAG | 20 | 4-5 |
| **B12.5** 60fps Target | Optimize render loop | 15 | 3-4 |
| **B12.6** Animation System | Consistent animation timing | 5 | 2-3 |

---

## v0.12.2 — Production Hardening

**Focus:** Stability and robustness
**Effort:** ~400 LOC | 2-3 days | 50 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B12.7** Error Recovery | Graceful handling of all error types | 20 | 3-4 |
| **B12.8** State Persistence | Crash recovery, session restore | 15 | 2-3 |
| **B12.9** Telemetry | Optional usage analytics | 15 | 2-3 |

### Deliverables

- [ ] All animations at 60fps
- [ ] DAG minimap
- [ ] Crash recovery
- [ ] **v0.12.2 Production Ready**

---

## Plans (Detailed Specs)

| File | Target | Description |
|------|--------|-------------|
| `v010-v012-6-views-design.md` | v0.10-v0.12 | 6-view TUI architecture design |
| `provider-modal-v2.md` | v0.11.0 | Provider modal redesign spec |
| `provider-modal-v2-implementation.md` | v0.11.0 | Implementation details |
| `provider-modal-v085-to-v090.md` | v0.11.x | Migration path |

---

## Dependencies on v0.9.x

v0.10+ requires these v0.9.x features:

| v0.9.x Feature | Required By |
|----------------|-------------|
| StableGraph | DAG Panel (v0.10.1) |
| Chat-as-DAG | Chat view integration |
| Boot Sequence | All views (initialization) |
| context: block | Editor view (context display) |
| agents: / skills: | Settings view (configuration) |
| heartbeat.yaml | Scheduler view (v0.10.2) |

---

## Metrics

| Metric | v0.9.5 | v0.12.2 Target |
|--------|--------|----------------|
| Tests | 2,400 | 3,000+ |
| LOC | ~30,500 | ~37,000 |
| TUI Views | 4 | 6 |
| Providers | 6 | 6 + Ollama native |
