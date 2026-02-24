# Nika v0.9 Implementation Plan

**Date:** 2026-02-24
**Status:** Approved
**Authors:** Thibaut, Claude

---

## Executive Summary

Comprehensive implementation plan for Nika v0.9 "File-First Agentic Architecture" following the Claude Code pattern.

**Key Deliverables:**
- Schema v0.6 with `context:`, `agents:`, `skills:` fields
- Agent 3-modes (reference, external, inline) with SOUL pattern
- Skill 3-modes with composition
- Boot sequence (6 phases)
- New YAML files (user, memory, policies, heartbeat)
- Chat-as-DAG with commands

**Metrics:**
- Current: 1,902 tests, ~25,000 LOC
- Target: 2,200+ tests, ~29,000 LOC
- New code: ~4,000 lines across 9 sprints

---

## Gap Analysis

### Already Exists (v0.8)

| Component | Location | Status |
|-----------|----------|--------|
| Session persistence | `src/tui/session.rs` | ✅ Working |
| Config system | `src/config.rs` | ✅ Working |
| Runner (DAG exec) | `src/runtime/runner.rs` | ✅ Working |
| Pause/Resume | `runner.rs` (AtomicBool) | ✅ Working |
| EventLog (22 variants) | `src/event/log.rs` | ✅ Working |
| 5 verbs | `src/ast/action.rs` | ✅ Working |
| MCP Client | `src/mcp/client.rs` | ✅ Working |
| TUI (4 views) | `src/tui/` | ✅ Working |
| 6 providers | `src/provider/rig.rs` | ✅ Working |

### To Build (v0.9)

| Feature | New Files | Lines |
|---------|-----------|-------|
| Context loading | `src/context/*.rs` | ~450 |
| Agent 3-modes | `src/agent/*.rs` | ~700 |
| Skill 3-modes | `src/skill/*.rs` | ~500 |
| Boot sequence | `src/boot/*.rs` | ~1,100 |
| Discovery | `src/discovery/*.rs` | ~650 |
| Chat-as-DAG | `src/chat/*.rs` | ~650 |
| New YAML files | `src/files/*.rs` | ~850 |

---

## Sprint Breakdown

### Sprint 1: Schema v0.6 Foundation

**Goal:** Add `context:`, `agents:`, `skills:` fields to Workflow struct.

**Files to modify:**
- `src/ast/workflow.rs` (+50 lines)
- `src/ast/mod.rs` (+20 lines)
- `schemas/nika-workflow.schema.json` (+100 lines)

**New files:**
- `src/ast/context.rs` (~150 lines) - ContextSpec, PathOrGlob
- `src/ast/agent_def.rs` (~200 lines) - AgentsSpec, AgentDef enum
- `src/ast/skill_def.rs` (~150 lines) - SkillsSpec, SkillDef enum

**Key implementation:**
```rust
// AgentDef with serde untagged for 3 modes
#[derive(Deserialize)]
#[serde(untagged)]
pub enum AgentDef {
    Reference(String),              // "researcher"
    External { file: PathBuf },     // { file: ./x.yaml }
    Inline(AgentSpec),              // { system: "..." }
}
```

**Tests:** ~30 new tests
**Success criteria:** v0.5 workflows still parse, v0.6 fields recognized

---

### Sprint 2: Context System

**Goal:** Load files into memory with type inference.

**Dependency:** Sprint 1 (ContextSpec exists)

**New files:**
- `src/context/mod.rs` (~50 lines)
- `src/context/loader.rs` (~250 lines) - Type inference by extension
- `src/context/store.rs` (~150 lines) - ContextStore struct

**Key implementation:**
```rust
pub async fn load_file(path: &Path) -> Result<Value, NikaError> {
    let content = tokio::fs::read_to_string(path).await?;
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Ok(serde_json::from_str(&content)?),
        Some("yaml") | Some("yml") => Ok(serde_yaml::from_str(&content)?),
        Some("md") | Some("txt") => Ok(Value::String(content)),
        _ => Ok(Value::String(content)),
    }
}
```

**Tests:** ~25 new tests
**Success criteria:** `{{memory.files.brand}}` resolves in prompts

---

### Sprint 3: Agent 3-Modes + SOUL Pattern

**Goal:** Load agents from reference, external file, or inline with SOUL sections.

**Dependency:** Sprint 1 (AgentDef exists)

**Files to modify:**
- `src/ast/agent.rs` (+150 lines) - Add SOUL sections
- `src/runtime/rig_agent_loop.rs` (+100 lines) - Build system prompt

**New files:**
- `src/agent/mod.rs` (~50 lines)
- `src/agent/soul.rs` (~200 lines) - AgentSoul struct
- `src/agent/loader.rs` (~300 lines) - 3-mode loading
- `src/agent/inheritance.rs` (~150 lines) - Agent inheritance

**SOUL sections:**
1. `soul.role` - Who you ARE
2. `soul.mission` - Your PURPOSE
3. `soul.personality` - HOW you behave
4. `soul.values` - WHAT matters
5. `rules` - CONSTRAINTS
6. `tools` - CAPABILITIES
7. `workflow` - PROCESS
8. `handoffs` - DELEGATION

**Tests:** ~40 new tests
**Success criteria:** `agents: { researcher: researcher }` discovers `.nika/agents/researcher.agent.yaml`

---

### Sprint 4: Skill 3-Modes + Composition

**Goal:** Load skills and compose multiple skills on one agent.

**Dependency:** Sprint 1 (SkillDef exists), Sprint 3 (Agent system)

**Files to modify:**
- `src/runtime/rig_agent_loop.rs` (+80 lines) - Apply skills

**New files:**
- `src/skill/mod.rs` (~50 lines)
- `src/skill/spec.rs` (~150 lines) - SkillSpec struct
- `src/skill/loader.rs` (~200 lines) - 3-mode loading
- `src/skill/composer.rs` (~100 lines) - Skill composition

**Key implementation:**
```rust
pub fn compose_skills(skills: &[SkillSpec]) -> ComposedSkill {
    let system_augment = skills.iter()
        .map(|s| format!("## {}\n{}", s.name, s.system_augment))
        .collect::<Vec<_>>()
        .join("\n\n");
    ComposedSkill { system_augment, ... }
}
```

**Tests:** ~30 new tests
**Success criteria:** `skill: [seo, tdd]` composes both skills

---

### Sprint 5: Boot Sequence

**Goal:** 6-phase initialization with progressive disclosure.

**Dependency:** Sprint 2-4 (Context, Agent, Skill systems)

**New files:**
- `src/boot/mod.rs` (~50 lines)
- `src/boot/sequence.rs` (~400 lines) - 6-phase boot
- `src/boot/context.rs` (~200 lines) - BootContext struct
- `src/boot/user.rs` (~150 lines)
- `src/boot/memory.rs` (~150 lines)
- `src/boot/policies.rs` (~150 lines)

**6 Phases:**
1. **Identity** - Load user.yaml (~500 tokens)
2. **Memory** - Load memory.yaml (~500 tokens)
3. **Rules** - Load policies.yaml (~500 tokens)
4. **Tools** - Connect MCP servers (~500 tokens)
5. **Persona** - Inject SOUL (~2000 tokens)
6. **Ready** - BootContext complete

**Tests:** ~35 new tests
**Success criteria:** `nika chat` loads user.yaml automatically

---

### Sprint 6: Project Structure & Discovery

**Goal:** Enhanced `nika init` and agent/skill discovery.

**Dependency:** Sprint 3-4 (Agent/Skill loaders)

**Files to modify:**
- `src/commands/init.rs` (+150 lines)
- `src/config.rs` (+100 lines)

**New files:**
- `src/discovery/mod.rs` (~50 lines)
- `src/discovery/project.rs` (~200 lines) - Project root detection
- `src/discovery/agents.rs` (~150 lines)
- `src/discovery/skills.rs` (~150 lines)
- `src/discovery/context.rs` (~100 lines)

**nika init creates:**
```
.nika/
├── config.toml
├── agents/
├── skills/
├── context/
├── sessions/
├── traces/
└── cache/
```

**Tests:** ~25 new tests
**Success criteria:** Discovery finds agents/skills by name

---

### Sprint 7: Chat-as-DAG

**Goal:** Chat messages as DAG nodes with commands.

**Dependency:** Sprint 5-6 (Boot, Project)

**Files to modify:**
- `src/tui/views/chat.rs` (+300 lines)
- `src/tui/session.rs` (+150 lines)

**New files:**
- `src/chat/mod.rs` (~50 lines)
- `src/chat/dag.rs` (~250 lines) - ChatDAG struct
- `src/chat/commands.rs` (~200 lines) - /agent, /skill, /context
- `src/chat/export.rs` (~150 lines) - Export to workflow
- `src/tui/widgets/dag_panel.rs` (~300 lines)

**Commands:**
- `/agent <name>` - Switch agent
- `/skill <name>` - Apply skill
- `/skill -<name>` - Remove skill
- `/context <name>` - Load context
- `/export yaml` - Export to workflow

**Tests:** ~30 new tests
**Success criteria:** `/agent researcher` switches agent

---

### Sprint 8: New YAML Files

**Goal:** user.yaml, memory.yaml, policies.yaml, heartbeat.yaml

**Dependency:** Sprint 5 (Boot expects these)

**New files:**
- `src/files/mod.rs` (~50 lines)
- `src/files/user.rs` (~150 lines) - UserProfile
- `src/files/memory.rs` (~200 lines) - MemoryStore with facts
- `src/files/policies.rs` (~200 lines) - PolicySet with boundaries
- `src/files/heartbeat.rs` (~250 lines) - Scheduled jobs

**Tests:** ~40 new tests
**Success criteria:** All 4 files load and integrate with boot

---

### Sprint 9: Polish & Ship

**Goal:** Integration tests, documentation, examples.

**Dependency:** All previous sprints

**Tasks:**
1. 10 integration tests (end-to-end scenarios)
2. Update CLAUDE.md for v0.9
3. Update examples/ with v0.6 workflows
4. Update JSON Schema
5. Clippy + fmt + coverage check
6. Changelog + version bump
7. Benchmark performance

**Integration tests:**
- `test_boot_sequence.rs`
- `test_context_loading.rs`
- `test_agent_3_modes.rs`
- `test_skill_composition.rs`
- `test_chat_commands.rs`
- `test_workflow_v06.rs`
- `test_discovery.rs`
- `test_policy_enforcement.rs`
- `test_heartbeat_scheduling.rs`
- `test_backward_compat.rs`

**Success criteria:**
- 2,200+ tests passing
- Zero clippy warnings
- All v0.5 workflows still work
- Documentation complete

---

## Dependencies Graph

```
Sprint 1 ────┬──► Sprint 2 (Context)
             │
             ├──► Sprint 3 (Agent) ──┬──► Sprint 5 (Boot) ──┬──► Sprint 8 (Files)
             │                       │                      │
             └──► Sprint 4 (Skill) ──┘                      ├──► Sprint 6 (Project)
                                                            │
                                                            └──► Sprint 7 (Chat)
                                                                      │
                                                                      ▼
                                                               Sprint 9 (Polish)
```

---

## Serde Patterns (from Context7)

Use these patterns for flexible YAML parsing:

```rust
// Optional with default
#[serde(default)]
pub context: Option<ContextSpec>,

// 3-mode enum via untagged
#[derive(Deserialize)]
#[serde(untagged)]
pub enum AgentDef {
    Reference(String),
    External { file: PathBuf },
    Inline(AgentSpec),
}

// Flatten for composition
#[derive(Deserialize)]
pub struct ExtendedConfig {
    #[serde(flatten)]
    base: BaseConfig,
    extra: String,
}
```

---

## Backward Compatibility

All new fields use `#[serde(default)]` making them optional:

```yaml
# v0.5 workflow - STILL WORKS
schema: "nika/workflow@0.5"
workflow: old-workflow
tasks:
  - id: step1
    infer: "Do something"

# v0.6 workflow - NEW FEATURES
schema: "nika/workflow@0.6"
workflow: new-workflow
context:
  files:
    brand: ./context/brand.md
agents:
  researcher: researcher
skills:
  seo: seo
tasks:
  - id: step1
    agent:
      use: researcher
      skill: [seo]
      prompt: "Research with {{memory.files.brand}}"
```

---

## Success Metrics

| Metric | v0.8 | v0.9 Target |
|--------|------|-------------|
| Tests | 1,902 | 2,200+ |
| LOC | ~25,000 | ~29,000 |
| Schema | v0.5 | v0.6 |
| Clippy warnings | 0 | 0 |
| v0.5 compat | N/A | 100% |

---

## References

- [v0.9 Consolidated Design](./2026-02-24-v09-consolidated-design.md)
- [Project Structure Design](./2026-02-24-nika-project-structure.md)
- [Memory & Agents Design](./2026-02-24-memory-and-agents-design.md)
- [Chat as DAG Design](./2026-02-24-chat-as-workflow-dag.md)
