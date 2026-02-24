# Nika v0.9.x Release Plan

**Codename:** "File-First Agentic Architecture"
**DAG-First Principle:** StableGraph everywhere (workflow + chat + heartbeat)
**Prerequisite:** v0.9.0 (current stable)

---

## Version Breakdown

```
v0.9.x — File-First Agentic Architecture
├── v0.9.1 — Context System + StableGraph Migration
├── v0.9.2 — Agent 3-Modes + Skill 3-Modes
├── v0.9.3 — New YAML Files + Boot Sequence
├── v0.9.4 — Chat-as-DAG Core
└── v0.9.5 — Integration + Polish
```

---

## v0.9.1 — Context System + StableGraph

**Focus:** Foundation + Unified DAG model
**Effort:** ~1,400 LOC | 5-7 days | 120 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B1.1** StableGraph Migration | Refactor FlowGraph → petgraph::StableGraph | 25 | 6-8 |
| **B1.2** Context Files | context.files + context.session in schema | 20 | 4-5 |
| **B1.3** Context Resolver | ContextResolver with path patterns | 15 | 3-4 |
| **B1.4** Auto-Import | @import directive in YAML | 10 | 2-3 |
| **B1.5** Integration Tests | End-to-end context loading | 50 | 4-5 |

### B1.1 StableGraph Migration — Detailed Tasks

```yaml
tasks:
  - id: add-petgraph-dep
    description: Add petgraph = "0.6" to Cargo.toml
    file: Cargo.toml
    effort: 5min

  - id: refactor-flowgraph-struct
    description: Replace FxHashMap adjacency with StableGraph<Arc<str>, ()>
    file: src/dag/flow.rs
    lines: 1-50
    effort: 2hr
    depends_on: [add-petgraph-dep]

  - id: add-node-index-mapping
    description: Add id_to_node (FxHashMap<Arc<str>, NodeIndex>) and node_to_id (Vec<Arc<str>>)
    file: src/dag/flow.rs
    lines: 20-40
    effort: 1hr
    depends_on: [refactor-flowgraph-struct]

  - id: update-from-workflow
    description: Update FlowGraph::from_workflow() to build StableGraph
    file: src/dag/flow.rs
    lines: 50-100
    effort: 2hr
    depends_on: [add-node-index-mapping]

  - id: add-mutation-methods
    description: Add add_task(), add_flow(), remove_task() methods
    file: src/dag/flow.rs
    effort: 1.5hr
    depends_on: [update-from-workflow]

  - id: update-path-queries
    description: Update has_path() to use petgraph::algo::has_path_connecting
    file: src/dag/flow.rs
    lines: 120-150
    effort: 30min
    depends_on: [refactor-flowgraph-struct]

  - id: update-cycle-detection
    description: Update detect_cycles() to use petgraph::algo::is_cyclic_directed
    file: src/dag/flow.rs
    lines: 170-220
    effort: 30min
    depends_on: [refactor-flowgraph-struct]

  - id: stablegraph-tests
    description: Add tests for node addition/removal stability
    file: src/dag/flow.rs
    lines: 250-350
    effort: 1.5hr
    depends_on: [add-mutation-methods]
```

### B1.2 Context Files — Detailed Tasks

```yaml
tasks:
  - id: context-schema
    description: Add context.files and context.session to workflow schema
    file: schemas/nika-workflow.schema.json
    effort: 30min

  - id: context-ast
    description: Add ContextSpec struct to AST
    file: src/ast/workflow.rs
    effort: 1hr
    depends_on: [context-schema]

  - id: context-parsing
    description: Parse context: block in YAML
    file: src/ast/workflow.rs
    effort: 1hr
    depends_on: [context-ast]

  - id: context-tests
    description: Unit tests for context parsing
    file: src/ast/workflow.rs
    effort: 1hr
    depends_on: [context-parsing]
```

### Deliverables

- [ ] `petgraph` dependency added
- [ ] `FlowGraph` uses `StableGraph<Arc<str>, ()>`
- [ ] Node addition/removal methods available
- [ ] `context:` block parsed in YAML
- [ ] 120 new tests passing
- [ ] Zero clippy warnings

---

## v0.9.2 — Agent 3-Modes + Skill 3-Modes

**Focus:** Agent and Skill definition patterns
**Effort:** ~1,200 LOC | 4-5 days | 100 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B2.1** Agent Reference Mode | agents: [research, code-review] (by name) | 20 | 3-4 |
| **B2.2** Agent External Mode | agents: [{path: "./agents/custom.yaml"}] | 20 | 3-4 |
| **B2.3** Agent Inline Mode | agents: [{inline: {prompt: "...", tools: []}}] | 25 | 4-5 |
| **B2.4** Skill 3-Modes | Same pattern for skills: | 25 | 4-5 |
| **B2.5** SOUL Pattern | Agent identity sections (role, mission, values) | 10 | 2-3 |

### B2.1 Agent Reference Mode — Detailed Tasks

```yaml
tasks:
  - id: agent-registry
    description: Create AgentRegistry with built-in agents
    file: src/agent/registry.rs (new)
    effort: 2hr

  - id: agent-resolver
    description: Resolve agent name → AgentSpec
    file: src/agent/resolver.rs (new)
    effort: 1.5hr
    depends_on: [agent-registry]

  - id: agent-schema
    description: Add agents: to workflow schema (string[] | object[])
    file: schemas/nika-workflow.schema.json
    effort: 30min

  - id: agent-ast
    description: Add AgentRef enum (Name, External, Inline)
    file: src/ast/agent.rs (new)
    effort: 1hr
    depends_on: [agent-schema]

  - id: agent-tests
    description: Unit tests for agent resolution
    file: src/agent/registry.rs
    effort: 1hr
    depends_on: [agent-resolver]
```

### Deliverables

- [ ] `agents:` field in workflow schema
- [ ] 3 agent modes: reference, external, inline
- [ ] `skills:` field with same 3 modes
- [ ] SOUL pattern for inline agents
- [ ] 100 new tests passing

---

## v0.9.3 — New YAML Files + Boot Sequence

**Focus:** File-first configuration + initialization
**Effort:** ~1,000 LOC | 4-5 days | 80 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B3.1** user.yaml | Operator profile (name, preferences, timezone) | 15 | 2-3 |
| **B3.2** memory.yaml | Long-term facts storage | 20 | 3-4 |
| **B3.3** policies.yaml | Guardrails and constraints | 15 | 2-3 |
| **B3.4** heartbeat.yaml | Cron-style automation triggers | 15 | 3-4 |
| **B3.5** Boot Sequence | 6-phase initialization (Identity→Ready) | 15 | 3-4 |

### B3.5 Boot Sequence — Detailed Tasks

```yaml
tasks:
  - id: boot-module
    description: Create src/boot/mod.rs with BootSequence struct
    file: src/boot/mod.rs (new)
    effort: 1hr

  - id: phase1-identity
    description: Load user.yaml, detect capabilities
    file: src/boot/identity.rs (new)
    effort: 1hr
    depends_on: [boot-module]

  - id: phase2-memory
    description: Load memory.yaml, session state
    file: src/boot/memory.rs (new)
    effort: 1hr
    depends_on: [phase1-identity]

  - id: phase3-policies
    description: Load policies.yaml, set guardrails
    file: src/boot/policies.rs (new)
    effort: 1hr
    depends_on: [phase2-memory]

  - id: phase4-tools
    description: Parallel init: tools + MCP servers
    file: src/boot/tools.rs (new)
    effort: 1.5hr
    depends_on: [phase3-policies]

  - id: phase5-persona
    description: Load SOUL.md, set agent identity
    file: src/boot/persona.rs (new)
    effort: 1hr
    depends_on: [phase4-tools]

  - id: phase6-ready
    description: Final readiness check, emit BootComplete event
    file: src/boot/ready.rs (new)
    effort: 30min
    depends_on: [phase5-persona]

  - id: boot-integration
    description: Wire boot sequence into main.rs
    file: src/main.rs
    effort: 1hr
    depends_on: [phase6-ready]
```

### Deliverables

- [ ] `.nika/user.yaml` parsed
- [ ] `.nika/memory.yaml` parsed
- [ ] `.nika/policies.yaml` parsed
- [ ] `.nika/heartbeat.yaml` parsed
- [ ] 6-phase boot sequence
- [ ] 80 new tests passing

---

## v0.9.4 — Chat-as-DAG Core

**Focus:** Conversational DAG with unified runtime
**Effort:** ~1,500 LOC | 5-6 days | 150 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B4.1** ChatDAG Structure | Messages as nodes, references as edges | 30 | 4-5 |
| **B4.2** @Mention Syntax | @1, @2, @last, @prev, @all parsing | 25 | 3-4 |
| **B4.3** Fork Syntax | // prefix for parallel tasks | 20 | 3-4 |
| **B4.4** Unified Executor | Workflow + Chat → same FlowGraph | 35 | 5-6 |
| **B4.5** YAML Export | Export chat session as workflow YAML | 20 | 3-4 |
| **B4.6** DAG Panel Widget | TUI visualization of chat DAG | 20 | 4-5 |

### B4.1 ChatDAG Structure — Detailed Tasks

```yaml
tasks:
  - id: chatdag-struct
    description: Create ChatDAG using StableGraph<ChatMessage, MessageRef>
    file: src/chat/dag.rs (new)
    effort: 2hr

  - id: chat-message-node
    description: ChatMessage struct (id, content, role, timestamp, verb)
    file: src/chat/message.rs (new)
    effort: 1hr
    depends_on: [chatdag-struct]

  - id: message-ref-edge
    description: MessageRef enum (Explicit(@1), Implicit(prev), Fork)
    file: src/chat/message.rs
    effort: 30min
    depends_on: [chat-message-node]

  - id: chatdag-add-message
    description: ChatDAG::add_message() with auto-indexing
    file: src/chat/dag.rs
    effort: 1hr
    depends_on: [chatdag-struct]

  - id: chatdag-tests
    description: Tests for message addition, reference resolution
    file: src/chat/dag.rs
    effort: 1.5hr
    depends_on: [chatdag-add-message]
```

### Deliverables

- [ ] `ChatDAG` using `StableGraph`
- [ ] `@mention` syntax parsed
- [ ] `//` fork syntax parsed
- [ ] Unified executor for workflow + chat
- [ ] YAML export from chat session
- [ ] DAG panel in TUI
- [ ] 150 new tests passing

---

## v0.9.5 — Integration + Polish

**Focus:** Final integration and quality
**Effort:** ~400 LOC | 2-3 days | 50 tests

### Batches

| Batch | Tasks | Tests | Hours |
|-------|-------|-------|-------|
| **B5.1** Full Integration | End-to-end workflows using all features | 20 | 3-4 |
| **B5.2** Documentation | Update CLAUDE.md, README, docs/ | 10 | 2-3 |
| **B5.3** Examples | Create example workflows for all features | 10 | 2-3 |
| **B5.4** Performance | Benchmark and optimize hot paths | 10 | 2-3 |

### Deliverables

- [ ] All v0.9.x features integrated
- [ ] Documentation updated
- [ ] 10+ example workflows
- [ ] Performance benchmarks passing
- [ ] **v0.9.5 Release Ready**

---

## Core Plans (Detailed Specs)

| File | Description | Status |
|------|-------------|--------|
| `v091-consolidated-design.md` | Full specification (Schema v0.6, Context, Agents, Skills, Boot) | Draft |
| `v091-implementation-plan.md` | 9-sprint breakdown with dependencies | Approved |
| `chat-as-workflow-dag.md` | Chat-as-DAG design (@mentions, //, bindings) | Draft |
| `chat-dag-implementation-plan.md` | Implementation details for Chat-as-DAG | Draft |

## Supporting Plans

| File | Description | Status |
|------|-------------|--------|
| `memory-and-agents-design.md` | Agent SOUL pattern, memory.yaml, policies.yaml | Draft |
| `nika-project-structure.md` | .nika/ directory structure, new YAML files | Draft |
| `nika-meta-execution-plan.md` | Meta-plan for executing v0.9.1 | Draft |

## Research (2026-02-24)

| File | Source | Key Topics |
|------|--------|------------|
| `2026-02-24-agentic-architecture-research.md` | Perplexity Sonar Pro | File-first, SOUL, Boot, Memory/Skills, Chat-as-DAG, MCP, TUI |

**Research Summary:**
- **File-First**: Manus `.manus/SKILL.md` pattern, event streams, plans as files
- **SOUL Pattern**: Identity, Communication, Rules, Capabilities in structured Markdown
- **Boot Sequence**: 7 phases (Args → Config → Doctor → Tools/MCP → Context → Model → Ready)
- **Memory**: Short-term (buffer), Working (injection), Long-term (vector + files)
- **LangGraph**: StateGraph API for nodes/edges, conditional branching, message reducers
- **MCP**: Transport abstraction, lifecycle management, multi-server orchestration

---

## Available Resources

### Skills (High Relevance)

| Skill | Usage |
|-------|-------|
| `/nika-yaml` | YAML authoring (5 verbs, for_each, bindings) |
| `/nika-arch` | Architecture diagrams |
| `/rust` | Rust patterns (ownership, error handling) |
| `/rust-async` | Tokio, channels, select!/join! |
| `/rust-agentic` | DAG workflows with petgraph |
| `/test-driven-development` | TDD methodology |
| `/brainstorming` | Design refinement |
| `/writing-plans` | Detailed task breakdown |

### MCP Tools

| Tool | Usage |
|------|-------|
| `novanet_generate` | Multi-locale content generation |
| `novanet_traverse` | Graph exploration for decompose |
| `novanet_describe` | Agent bootstrap with schema |
| `perplexity_*` | Web research |
| `firecrawl_*` | Advanced scraping |

### Agents

| Agent | Usage |
|-------|-------|
| `code-reviewer` | Rust code quality review |
| `rust-pro` | Rust implementation |
| `rust-async-expert` | Tokio patterns |

---

## Metrics

| Metric | v0.9.0 | v0.9.5 Target |
|--------|--------|---------------|
| Tests | 1,902 | 2,400+ |
| LOC | ~25,000 | ~30,500 |
| New code | - | ~5,500 lines |
| TUI Views | 4 | 4 (6 in v0.10) |

---

## Quality Gates (2026-02-24)

**Mandatory checkpoints based on Rust expert review (3 agents, 254KB analysis).**

### Gate Structure

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  QUALITY GATE WORKFLOW                                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Batch N ──► Unit Tests ──► Code Review ──► E2E Tests ──► Ralph Wiggum     │
│     │            │              │               │              │            │
│     │         (TDD)         (rust-pro)      (integration)   (parallel      │
│     │                                                        audit)        │
│     ▼                                                          │           │
│  Batch N+1 ◄───────────────────────────────────────────────────┘           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Critical Issues (Fix BEFORE Implementation)

From rust-pro, rust-async-expert, rust-architect agents:

| Issue | Severity | Batch | Fix Required |
|-------|----------|-------|--------------|
| `StableGraph<Arc<str>>` cloning overhead | 🔴 CRITICAL | B1.1 | Use `String` + `parking_lot::RwLock` |
| `ChatWorkflow` not thread-safe | 🔴 CRITICAL | B4.1 | Wrap in `Arc<Mutex<ChatWorkflow>>` |
| `MentionParser` semantics undefined | 🔴 CRITICAL | B4.2 | Use explicit `MentionRef` enum |
| Task ID collisions in parallel | 🔴 CRITICAL | B4.4 | Use `AtomicU32` (lock-free) |
| Locks held across `.await` | 🔴 CRITICAL | B4.4 | Release lock before execute |
| Unbounded event queue | 🟡 MEDIUM | B4.6 | Use bounded broadcast (1000 events) |
| Event subscription missing | 🟡 MEDIUM | B4.6 | Add `EventLog::subscribe()` |
| View routing tight coupling | 🟡 MEDIUM | v0.10 | Use `ViewState` trait |

### Per-Batch Quality Gates

#### v0.9.1 Gates

| After Batch | Gate | Criteria | Agent |
|-------------|------|----------|-------|
| B1.1 | **StableGraph Unit** | 25 tests, node index stability verified | rust-pro |
| B1.1 | **Code Review** | `parking_lot::RwLock` pattern confirmed | spn-rust:rust-pro |
| B1.2 | **Context E2E** | `context.files` loads from 3 formats | feature-dev:code-reviewer |
| B1.5 | **Ralph Wiggum** | 6 parallel agents audit all v0.9.1 code | nika-deep-verify |

#### v0.9.2 Gates

| After Batch | Gate | Criteria | Agent |
|-------------|------|----------|-------|
| B2.1 | **Agent Registry** | 3 built-in agents resolve correctly | rust-pro |
| B2.3 | **SOUL Validation** | Inline agents parse all SOUL fields | code-reviewer |
| B2.5 | **Integration** | Agent 3-modes work with existing workflows | E2E |

#### v0.9.3 Gates

| After Batch | Gate | Criteria | Agent |
|-------------|------|----------|-------|
| B3.4 | **Heartbeat Parse** | Cron expressions validate correctly | rust-pro |
| B3.5 | **Boot Sequence** | 6 phases complete in <500ms | rust-perf |
| B3.5 | **Ralph Wiggum** | Boot sequence audit (all phases) | nika-deep-verify |

#### v0.9.4 Gates

| After Batch | Gate | Criteria | Agent |
|-------------|------|----------|-------|
| B4.1 | **Thread Safety** | `Arc<Mutex<ChatWorkflow>>` pattern | rust-async-expert |
| B4.2 | **Mention Parse** | All @mention variants resolve | rust-pro |
| B4.4 | **Unified Executor** | Same runtime for workflow + chat | E2E |
| B4.4 | **Race Condition Audit** | No locks across `.await` | rust-async-expert |
| B4.6 | **DAG Panel Perf** | 60fps with 100+ nodes | rust-perf |
| B4.6 | **Ralph Wiggum** | Full Chat-as-DAG audit | nika-deep-verify |

#### v0.9.5 Gates

| After Batch | Gate | Criteria | Agent |
|-------------|------|----------|-------|
| B5.1 | **Full E2E** | 10 example workflows pass | E2E |
| B5.4 | **Performance** | Benchmarks within targets | rust-perf |
| B5.4 | **Final Ralph Wiggum** | Complete v0.9.x audit | nika-deep-verify |
| B5.4 | **Release Review** | Zero clippy warnings, docs complete | code-reviewer |

### Ralph Wiggum Protocol

Launch 6 parallel verification agents after each major batch:

```bash
# Example: After B1.5 completion
/nika-deep-verify

# Agents launched:
# 1. verify-spec     — SPEC.md alignment
# 2. verify-code     — Rust patterns + idioms
# 3. verify-docs     — CLAUDE.md + README sync
# 4. verify-logic    — Business logic consistency
# 5. verify-rust-conventions — Best practices
# 6. nika-sync       — Full spec-code-docs alignment
```

### Code Review Checklist

Per-batch code review requirements:

- [ ] **Ownership** — No unnecessary `Arc<str>` cloning
- [ ] **Thread Safety** — `parking_lot` over `std::sync`
- [ ] **Async** — No locks held across `.await`
- [ ] **Errors** — `NikaError` with codes, not `anyhow`
- [ ] **Tests** — TDD (failing test first)
- [ ] **Docs** — CLAUDE.md updated if API changed
- [ ] **Clippy** — Zero warnings with `-D warnings`

### E2E Test Requirements

Minimum E2E coverage per version:

| Version | E2E Tests | Coverage |
|---------|-----------|----------|
| v0.9.1 | 10 | Context loading, StableGraph mutations |
| v0.9.2 | 15 | Agent 3-modes, Skill 3-modes |
| v0.9.3 | 12 | Boot sequence, YAML file loading |
| v0.9.4 | 25 | Chat-as-DAG, @mentions, unified executor |
| v0.9.5 | 10 | Full integration workflows |

**Total: 72 new E2E tests for v0.9.x**

### Agent Review Documents (Generated 2026-02-24)

| Document | Location | Size |
|----------|----------|------|
| `README-RUST-REVIEW.md` | `nika/` | 5.5 KB |
| `RUST-REVIEW-V091.md` | `nika/` | 32 KB |
| `RUST-PATTERNS-V091.md` | `nika/` | 27 KB |
| `ASYNC-REVIEW.md` | `docs/plans/v0.9.1/` | 96 KB |
| `ASYNC-IMPLEMENTATION-GUIDE.md` | `docs/plans/v0.9.1/` | Production code |
| `v0.10-tui-architecture-review.md` | `docs/architecture/` | 86 KB |

---

## Context7 Verification (2026-02-24)

### petgraph StableGraph Confirmed

```rust
// StableGraph preserves NodeIndex after deletion (verified via Context7)
let mut g_stable = StableUnGraph::<i32, ()>::from_edges(&[(0, 1), (1, 2)]);
g_stable.remove_node(1.into());
// Output: [NodeIndex(0), NodeIndex(2)] — index 1 gone, 0 and 2 stable
```

**Key insight:** StableGraph is correct for @1, @2, @last references — indices remain valid after message deletion.

---

## Related Documents

- **v0.10+ Plans:** `../v0.10+/` (6-Views, Provider Modal v2)
- **v0.8 Archive:** `../archive-v0.8/` (completed work)
- **ADRs:** `../../tools/nika/.claude/rules/adr/`
- **Agent Reviews:** See "Agent Review Documents" section above
