# Nika v0.10 → v0.12 Meta-Execution Plan

**Date:** 2026-02-24
**Status:** Active
**Authors:** Thibaut, Claude
**Purpose:** Consolidate ALL design documents and organize implementation execution

---

## Overview

This document serves as the **single source of truth** for Nika's v0.10 → v0.12 development, consolidating 10+ design documents into an executable plan with clear dependencies, skills, and methodologies.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA v0.10 → v0.12 IMPLEMENTATION ROADMAP                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  v0.10.0 ──► v0.10.1 ──► v0.11.0 ──► v0.11.1 ──► v0.12.0                     ║
║  Explorer    Chat-DAG    Runner      Provider     Polish                      ║
║  +Editor     +Bindings   +Scheduler  Modal v2     +Ship                       ║
║                                                                               ║
║  Timeline: ~9 sprints (~18 weeks)                                             ║
║  Tests: 1,902 → 2,200+ (300+ new tests)                                       ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Consolidated Design Documents

### Core Architecture Documents

| Document | Scope | Version Target |
|----------|-------|----------------|
| `v09-consolidated-design.md` | File-first agentic architecture, SOUL pattern | v0.10+ |
| `v09-implementation-plan.md` | 9-sprint implementation breakdown | v0.10-v0.12 |
| `tui-v09-6-views-design.md` | 6-view TUI architecture | v0.10-v0.12 |
| `nika-project-structure.md` | `.nika/` directory layout | v0.10 |
| `memory-and-agents-design.md` | Memory system + External agents | v0.10 |
| `chat-as-workflow-dag.md` | Chat-as-DAG concept | v0.10.1 |
| `chat-dag-implementation-plan.md` | 5-phase Chat-DAG implementation | v0.10.1 |

### Feature-Specific Documents

| Document | Feature | Priority |
|----------|---------|----------|
| `provider-modal-v2.md` | Provider Settings Panel v2 | v0.11.1 |
| `provider-modal-v2-implementation.md` | Implementation details | v0.11.1 |
| `task-boxes-design.md` | Verb-colored task boxes | v0.10.1 |
| `taskbox-wiring-plan.md` | TaskBox integration | v0.10.1 |
| `connection-verification-v082.md` | Provider verification | v0.8.2 (done) |
| `startup-verification-p0.md` | Startup checks | v0.8.2 (done) |

### Testing & Quality

| Document | Scope |
|----------|-------|
| `comprehensive-testing-plan.md` | 2,200+ test target strategy |
| `tui-home-dag-preview-fixes.md` | Bug fixes |
| `timeout-fixes-plan.md` | MCP timeout handling |

---

## Version Roadmap (Detailed)

### v0.10.0 — Foundation (Sprints 1-3)

**Focus:** Schema v0.6 + Context System + Explorer View

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  v0.10.0 DELIVERABLES                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Schema v0.6                                                                    │
│  ├── memory: { files, session }                                                 │
│  ├── agents: { name: AgentDef }                                                 │
│  ├── skills: { name: path }                                                     │
│  └── Backward compatibility with v0.5                                           │
│                                                                                 │
│  Context System (3 Layers)                                                      │
│  ├── L1: Project files (./context/*.md|json|yaml)                               │
│  ├── L2: Session context (.nika/sessions/)                                      │
│  └── L3: Long-term memory (future v1.0)                                         │
│                                                                                 │
│  Explorer View (Claude Code-inspired)                                           │
│  ├── 3-panel layout: Tree | Preview | Details                                   │
│  ├── NovaNet tree effects: breadcrumb, minimap                                  │
│  ├── Quick actions: [▶ Run] [✏ Edit] [👁 Preview] [📋 Copy]                    │
│  └── Fuzzy search with Ctrl+P                                                   │
│                                                                                 │
│  Editor View (Studio upgrade)                                                   │
│  ├── Schema v0.6 syntax highlighting                                            │
│  ├── Agent/Skill autocomplete                                                   │
│  └── Memory template validation                                                 │
│                                                                                 │
│  Tests: +100 (1,902 → 2,002)                                                    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Key Files to Create/Modify:**
- `src/ast/memory.rs` (NEW)
- `src/ast/agent_def.rs` (NEW)
- `src/ast/skill.rs` (NEW)
- `src/ast/workflow.rs` (MODIFY for v0.6)
- `src/tui/views/explorer.rs` (NEW or RENAME from home.rs)
- `src/tui/views/editor.rs` (RENAME from studio.rs)

### v0.10.1 — Chat-as-DAG (Sprints 4-5)

**Focus:** Chat messages as DAG tasks with real-time visualization

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  v0.10.1 DELIVERABLES                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Chat-as-DAG Core                                                               │
│  ├── ChatWorkflow struct (extends Workflow)                                     │
│  ├── Messages become Tasks with IDs (msg-001, msg-002)                          │
│  ├── DataStore for chat message outputs                                         │
│  └── EventLog for real-time updates                                             │
│                                                                                 │
│  @Mention Binding System                                                        │
│  ├── @1, @2, @last, @prev, @all                                                 │
│  ├── @msg-001 (explicit ID reference)                                           │
│  ├── MentionParser with completion suggestions                                  │
│  └── Highlight mentions in chat input                                           │
│                                                                                 │
│  Fork Syntax                                                                    │
│  ├── `//` prefix for parallel tasks                                             │
│  └── Auto-join with `@all`                                                      │
│                                                                                 │
│  DAG Panel (Split view)                                                         │
│  ├── Mini-DAG in Chat view right panel                                          │
│  ├── Real-time node updates (⏳→🔄→✅)                                          │
│  └── Click-to-focus node                                                        │
│                                                                                 │
│  TaskBox Full Mode                                                              │
│  ├── Verb-colored headers (see task-boxes-design.md)                            │
│  ├── Streaming progress indicator                                               │
│  └── Token/timing metadata                                                      │
│                                                                                 │
│  Tests: +73 (2,002 → 2,075)                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Key Files to Create/Modify:**
- `src/runtime/chat_workflow.rs` (NEW)
- `src/binding/mention.rs` (NEW)
- `src/tui/widgets/dag_mini.rs` (NEW)
- `src/tui/widgets/task_box.rs` (ENHANCE)
- `src/tui/views/chat.rs` (MAJOR MODIFY)

### v0.11.0 — Execution Views (Sprints 6-7)

**Focus:** Runner + Scheduler views for workflow execution

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  v0.11.0 DELIVERABLES                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Runner View (4-panel layout)                                                   │
│  ├── DAG Panel: Real-time graph with verb icons                                 │
│  ├── Output Panel: Streaming task outputs                                       │
│  ├── Timeline Panel: Execution timeline with markers                            │
│  └── Details Panel: Selected task metadata                                      │
│                                                                                 │
│  Scheduler View                                                                 │
│  ├── Cron-like scheduling UI                                                    │
│  ├── Workflow queue management                                                  │
│  ├── History with stats (runs, success rate)                                    │
│  └── Integration with heartbeat.yaml (v0.9 design)                              │
│                                                                                 │
│  Tests: +50 (2,075 → 2,125)                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Key Files to Create/Modify:**
- `src/tui/views/runner.rs` (RENAME from monitor.rs + enhance)
- `src/tui/views/scheduler.rs` (NEW)
- `src/scheduler/` (NEW module)

### v0.11.1 — Provider Modal v2 (Sprint 8)

**Focus:** Enhanced provider settings with 4-tab architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  v0.11.1 DELIVERABLES                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Provider Modal v2 (4 Tabs)                                                     │
│  ├── Cloud Tab: Claude, OpenAI, Mistral, Groq, DeepSeek                         │
│  │   └── ProviderCard with verification status                                  │
│  ├── Ollama Tab: Local models with download management                          │
│  │   ├── ModelCard with size, quantization info                                 │
│  │   └── DownloadGauge with progress                                            │
│  ├── Keys Tab: API key management                                               │
│  │   └── Keyring integration (secure storage)                                   │
│  └── Config Tab: Provider preferences                                           │
│       └── Default model, timeout, retry settings                                │
│                                                                                 │
│  Settings View                                                                  │
│  ├── Theme selection (Light/Dark/Solarized)                                     │
│  ├── Editor preferences                                                         │
│  ├── Session settings                                                           │
│  └── MCP server management                                                      │
│                                                                                 │
│  Tests: +25 (2,125 → 2,150)                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Key Files to Create/Modify:**
- `src/tui/widgets/provider_modal.rs` (MAJOR ENHANCE)
- `src/tui/views/settings.rs` (NEW)
- `src/provider/ollama.rs` (ENHANCE for download management)

### v0.12.0 — Polish & Ship (Sprint 9)

**Focus:** Quality, documentation, release preparation

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  v0.12.0 DELIVERABLES                                                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Quality                                                                        │
│  ├── All clippy warnings resolved                                               │
│  ├── All TODO/FIXME addressed or tracked                                        │
│  ├── Test coverage >80% for new code                                            │
│  └── Performance profiling (flamegraph)                                         │
│                                                                                 │
│  Documentation                                                                  │
│  ├── Updated README with 6-view architecture                                    │
│  ├── CHANGELOG for v0.10-v0.12                                                  │
│  ├── Updated CLAUDE.md with new features                                        │
│  └── Example workflows for all new features                                     │
│                                                                                 │
│  Release                                                                        │
│  ├── Version bump to v0.12.0                                                    │
│  ├── cargo publish (if applicable)                                              │
│  └── GitHub release with notes                                                  │
│                                                                                 │
│  Tests: +50 (2,150 → 2,200)                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Skills & Methodologies

### Superpowers Skills (MUST USE)

| Skill | When to Use | Phase |
|-------|-------------|-------|
| `spn-powers:brainstorming` | Before any design decision | All |
| `spn-powers:writing-plans` | Creating implementation tasks | All |
| `spn-powers:test-driven-development` | All implementation work | All |
| `spn-powers:verification-before-completion` | Before any PR/commit | All |
| `spn-powers:systematic-debugging` | When bugs occur | All |
| `spn-powers:using-git-worktrees` | Feature isolation | v0.10+ |
| `spn-powers:code-reviewer` | After major features | All |

### Rust-Specific Skills (spn-rust)

| Skill | When to Use |
|-------|-------------|
| `rust-core` | Ownership, error handling, type patterns |
| `rust-async` | Tokio tasks, channels, select!/join! |
| `rust-agentic` | Agent orchestration patterns |

### Claude Code Documentation

| Resource | Purpose |
|----------|---------|
| `claude-code-docs` skill | Search 270 official docs |
| CLAUDE.md patterns | Agent configuration, hooks, skills |
| MCP documentation | Tool integration |

### Development Methodology

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  DEVELOPMENT WORKFLOW (Per Feature)                                           ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. BRAINSTORM                                                                ║
║     └── Use `spn-powers:brainstorming` skill                                  ║
║     └── One question at a time, explore alternatives                          ║
║                                                                               ║
║  2. PLAN                                                                      ║
║     └── Use `spn-powers:writing-plans` skill                                  ║
║     └── Create bite-sized tasks with exact file paths                         ║
║                                                                               ║
║  3. IMPLEMENT (TDD)                                                           ║
║     └── Use `spn-powers:test-driven-development` skill                        ║
║     └── RED → GREEN → REFACTOR cycle                                          ║
║     └── Use `spn-rust:rust-*` skills for Rust patterns                        ║
║                                                                               ║
║  4. VERIFY                                                                    ║
║     └── Use `spn-powers:verification-before-completion` skill                 ║
║     └── cargo test, clippy, fmt — ALL must pass                               ║
║                                                                               ║
║  5. REVIEW                                                                    ║
║     └── Use `spn-powers:code-reviewer` agent                                  ║
║     └── Check against original plan                                           ║
║                                                                               ║
║  6. COMMIT                                                                    ║
║     └── Conventional commits: type(scope): description                        ║
║     └── Co-Authored-By headers                                                ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Sprint Dependencies

```
                    ┌─────────────┐
                    │  Sprint 1   │ Schema v0.6 Foundation
                    │  (v0.10.0)  │
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
    │  Sprint 2   │ │  Sprint 3   │ │  Sprint 4   │
    │  Context    │ │  Agent      │ │  Skill      │
    │  System     │ │  3-Modes    │ │  3-Modes    │
    └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
           │               │               │
           └───────────────┼───────────────┘
                           ▼
                    ┌─────────────┐
                    │  Sprint 5   │ Boot Sequence
                    │  (v0.10.0)  │
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           ▼               ▼               ▼
    ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
    │  Sprint 6   │ │  Sprint 7   │ │  Sprint 8   │
    │  Project    │ │  Chat-DAG   │ │  New YAML   │
    │  Structure  │ │  (v0.10.1)  │ │  Files      │
    └──────┬──────┘ └──────┬──────┘ └──────┬──────┘
           │               │               │
           └───────────────┼───────────────┘
                           ▼
                    ┌─────────────┐
                    │  Sprint 9   │ Polish & Ship
                    │  (v0.12.0)  │
                    └─────────────┘
```

**Parallelization Opportunities:**
- Sprints 2, 3, 4 can run in parallel (independent features)
- Sprints 6, 7, 8 can run in parallel (independent views)

---

## Test Strategy

### Test Distribution Target

| Module | Current | Target | Delta |
|--------|---------|--------|-------|
| nika-core | 460 | 500 | +40 |
| nika-mcp | 130 | 150 | +20 |
| nika-provider | 35 | 50 | +15 |
| nika-runtime | 185 | 220 | +35 |
| nika-tui | 730 | 880 | +150 |
| nika-cli | 362 | 400 | +38 |
| **Total** | **1,902** | **2,200** | **+298** |

### Test Types

| Type | Purpose | Location |
|------|---------|----------|
| Unit | Function-level | `#[cfg(test)]` modules |
| Integration | Module interaction | `tests/*.rs` |
| Snapshot | Output stability | `insta` crate |
| Property | Fuzzing | `proptest` crate |
| E2E | Full workflow | `examples/test-*.nika.yaml` |

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| Schema v0.6 breaks existing workflows | HIGH | Backward compatibility tests |
| Chat-DAG performance with large conversations | MEDIUM | Pagination, lazy loading |
| Provider Modal complexity | LOW | Incremental tab implementation |
| Test count regression | MEDIUM | CI check for test count |

---

## Success Criteria (Checklist)

### v0.10.0
- [ ] Schema v0.6 parses with memory, agents, skills
- [ ] Context files load and resolve in templates
- [ ] External agent files parse and inherit correctly
- [ ] Skill files augment system prompts
- [ ] Explorer view with 3-panel layout
- [ ] Editor view with v0.6 autocomplete
- [ ] 100+ new tests passing

### v0.10.1
- [ ] Chat messages become DAG tasks
- [ ] @mention syntax works with completion
- [ ] Fork syntax creates parallel tasks
- [ ] DAG panel updates in real-time
- [ ] TaskBox shows verb-colored headers
- [ ] 73+ new tests passing

### v0.11.0
- [ ] Runner view with 4-panel layout
- [ ] Scheduler view with cron-like UI
- [ ] Workflow queue management works
- [ ] 50+ new tests passing

### v0.11.1
- [ ] Provider Modal v2 with 4 tabs
- [ ] Cloud providers show verification status
- [ ] Ollama tab shows downloadable models
- [ ] Keys tab secures API keys
- [ ] Settings view configurable
- [ ] 25+ new tests passing

### v0.12.0
- [ ] Zero clippy warnings
- [ ] All TODO/FIXME resolved
- [ ] Documentation updated
- [ ] 2,200+ tests passing
- [ ] Release tagged and published

---

## Quick Reference: Which Plan for What?

| Task | Primary Document |
|------|------------------|
| Schema v0.6 fields | `v09-consolidated-design.md` |
| Memory system | `memory-and-agents-design.md` |
| Agent/Skill files | `memory-and-agents-design.md` |
| `.nika/` structure | `nika-project-structure.md` |
| Chat-as-DAG | `chat-dag-implementation-plan.md` |
| TUI 6 views | `tui-v09-6-views-design.md` |
| TaskBox design | `task-boxes-design.md` |
| Provider Modal v2 | `provider-modal-v2.md` |
| Sprint breakdown | `v09-implementation-plan.md` |

---

## Execution Commands

```bash
# Start a sprint
cd nika/tools/nika
git checkout -b feature/v0.10.0-schema-v06

# Run tests continuously
cargo watch -x "nextest run"

# Check clippy
cargo clippy -- -D warnings

# Format code
cargo fmt

# Run benchmarks
cargo bench

# Create release
git tag -a v0.10.0 -m "Release v0.10.0: Schema v0.6 + Explorer + Editor"
```

---

## References

- [v09-consolidated-design.md](./2026-02-24-v09-consolidated-design.md)
- [v09-implementation-plan.md](./2026-02-24-v09-implementation-plan.md)
- [chat-dag-implementation-plan.md](./2026-02-24-chat-dag-implementation-plan.md)
- [tui-v09-6-views-design.md](./2026-02-24-tui-v09-6-views-design.md)
- [memory-and-agents-design.md](./2026-02-24-memory-and-agents-design.md)
- [nika-project-structure.md](./2026-02-24-nika-project-structure.md)
- [provider-modal-v2.md](./2026-02-24-provider-modal-v2.md)
- [task-boxes-design.md](./2026-02-24-task-boxes-design.md)

---

**Last Updated:** 2026-02-24
**Next Review:** After v0.10.0 release
