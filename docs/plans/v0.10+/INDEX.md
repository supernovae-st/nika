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

v0.11.x — Advanced Features ⚠️ DRAFT (needs review)
├── v0.11.0 — Provider Modal v2 (full)
├── v0.11.1 — Ollama Native Client
├── v0.11.2 — Keyring Integration
└── v0.11.3 — Advanced Builtin Tools (nika:checkpoint, nika:cache, nika:artifact, nika:notify, nika:todo)

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

## v0.11.3 — Advanced Builtin Tools ⚠️ DRAFT

**Status:** NEEDS REVIEW — These tools may be unnecessary or overlap with existing functionality.

**Focus:** Advanced workflow automation (TIER 3 builtin tools from v0.9.1 research)
**Effort:** TBD | Requires design review

### Tools Under Review

| Tool | Purpose | Review Question |
|------|---------|-----------------|
| `nika:checkpoint` | Save execution state | Does EventLog already provide this? |
| `nika:cache` | Cache expensive results | What's the cache invalidation strategy? |
| `nika:artifact` | Store file outputs | Is this better than simple file writes? |
| `nika:notify` | External notifications | Should this be MCP instead (Slack, email)? |
| `nika:todo` | Task tracking | Does EventLog::TaskCreated suffice? |

### Review Criteria

Before implementing, each tool must answer:

1. **Necessity:** Can existing tools/patterns achieve this?
2. **Scope:** Is this internal (builtin) or external (MCP)?
3. **Complexity:** Is the implementation cost justified?
4. **Consistency:** Does it fit the `nika:*` pattern?

### Decision

**Defer until v0.11.x planning phase.** Current focus is v0.9.1 → v0.10.3.

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

---

## Quality Gates (2026-02-24)

**Based on rust-architect review (86KB analysis, 5 patterns).**

### Phase 0 — Pre-v0.10 Foundation (9.5 hours)

Must complete BEFORE v0.10.0 development:

| Pattern | Effort | Issue | Fix |
|---------|--------|-------|-----|
| **ViewState Trait** | 2h | Tight coupling between views | Trait-based view abstraction |
| **Event Coalescing** | 2h | 150 allocations/frame | Batch keyboard events |
| **DAG Caching** | 4h | O(n²) layout recalc | Cache layout positions |
| **Memory Bounds** | 1.5h | Unbounded growth | Ring buffers for history |

**ROI:** 9.5h now vs 20-30h refactoring later (3:1 return)

### Per-Version Quality Gates

#### v0.10.x Gates

| After | Gate | Criteria | Agent |
|-------|------|----------|-------|
| B10.1 | **ViewState Trait** | All 6 views implement trait | rust-architect |
| B10.3 | **Editor Perf** | 60fps with 1000+ line YAML | rust-perf |
| B10.5 | **Ralph Wiggum** | Full v0.10.0 audit | nika-deep-verify |
| B10.7 | **DAG Cache** | 4x faster for 500+ nodes | rust-perf |
| B10.9 | **v0.10.1 E2E** | Runner + DAG panel integration | E2E |

#### v0.11.x Gates

| After | Gate | Criteria | Agent |
|-------|------|----------|-------|
| B11.1 | **Provider Trait** | 83% fewer states | rust-architect |
| B11.3 | **Connection Test** | All 6 providers tested | E2E |
| B11.9 | **Ollama E2E** | Local-first mode works | E2E |
| B11.12 | **Security Audit** | Keyring integration secure | rust-security |

#### v0.12.x Gates

| After | Gate | Criteria | Agent |
|-------|------|----------|-------|
| B12.5 | **60fps Verified** | Flamegraph shows no hot paths | rust-perf |
| B12.8 | **Crash Recovery** | State persists across restarts | E2E |
| B12.9 | **Final Audit** | Production ready | nika-deep-verify |

### Architecture Patterns from rust-architect

**Recommended patterns for v0.10+ (86KB analysis):**

```rust
// Pattern 1: ViewState Trait (2h to implement)
pub trait ViewState: Send + Sync {
    fn render(&self, frame: &mut Frame, area: Rect);
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action>;
    fn on_enter(&mut self);
    fn on_leave(&mut self);
}

// Pattern 2: Event Coalescing (2h)
// Batch rapid keystrokes into single update

// Pattern 3: DAG Layout Caching (4h)
// Cache node positions, invalidate on structure change

// Pattern 4: Ring Buffer History (1.5h)
// Bounded command history, no unbounded Vec growth
```

### Performance Targets

| Metric | v0.10 | v0.11 | v0.12 |
|--------|-------|-------|-------|
| Frame time | <16.7ms | <16.7ms | <10ms |
| DAG render (100 nodes) | <5ms | <3ms | <2ms |
| Memory (idle) | <50MB | <50MB | <50MB |
| Memory (100 tasks) | <100MB | <80MB | <60MB |

### Agent Review Documents

| Document | Location | Content |
|----------|----------|---------|
| `v0.10-tui-architecture-review.md` | `docs/architecture/` | Deep technical analysis |
| `v0.10-implementation-checklist.md` | `docs/architecture/` | 100+ tasks |
| `v0.10-patterns-visual-guide.md` | `docs/architecture/` | Before/after diagrams |
| `PHASE-0-EXECUTIVE-SUMMARY.md` | `docs/architecture/` | Business case |
