# Nika v0.9 Implementation Plan

**Date:** 2026-02-24
**Status:** Approved
**Authors:** Thibaut, Claude
**Codename:** "File-First Agentic Architecture"

---

## Executive Summary

Comprehensive implementation plan for Nika v0.9 following the Claude Code pattern.

**Key Deliverables:**
- Schema v0.6 with `context:`, `agents:`, `skills:` fields
- Agent 3-modes (reference, external, inline) with SOUL pattern
- Skill 3-modes with composition
- Boot sequence (6 phases)
- New YAML files (user, memory, policies, heartbeat)
- Chat-as-DAG with real-time visualization and YAML export
- Unified Runtime (same execution for Chat, Workflow, Heartbeat)

**Metrics:**
- Current: 1,902 tests, ~25,000 LOC
- Target: 2,200+ tests, ~29,000 LOC
- New code: ~4,500 lines across 9 sprints

---

## Core Philosophy: Everything is a DAG

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🎯 UNIFIED ARCHITECTURE: ALL PATHS LEAD TO DAG EXECUTION                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║                         ┌───────────────────────┐                             ║
║                         │    Entry Points       │                             ║
║                         └───────────┬───────────┘                             ║
║                ┌────────────────────┼────────────────────┐                    ║
║                ▼                    ▼                    ▼                    ║
║         ┌──────────┐         ┌──────────┐         ┌──────────┐               ║
║         │ WORKFLOW │         │   CHAT   │         │HEARTBEAT │               ║
║         │ (YAML)   │         │ (Live)   │         │ (Cron)   │               ║
║         └────┬─────┘         └────┬─────┘         └────┬─────┘               ║
║              │                    │                    │                      ║
║              ▼                    ▼                    ▼                      ║
║         Parse YAML          Build ChatDAG         Load workflow              ║
║              │                    │                    │                      ║
║              └────────────────────┼────────────────────┘                      ║
║                                   ▼                                           ║
║                     ┌─────────────────────────┐                               ║
║                     │      FlowGraph (DAG)    │                               ║
║                     │   petgraph::DiGraph     │                               ║
║                     └────────────┬────────────┘                               ║
║                                  │                                            ║
║                     ┌────────────┼────────────┐                               ║
║                     ▼            ▼            ▼                               ║
║               ┌──────────┐ ┌──────────┐ ┌──────────┐                         ║
║               │DataStore │ │ Executor │ │ EventLog │                         ║
║               │HashMap   │ │ 5 verbs  │ │22 events │                         ║
║               └──────────┘ └──────────┘ └──────────┘                         ║
║                     │            │            │                               ║
║                     └────────────┼────────────┘                               ║
║                                  ▼                                            ║
║                     ┌─────────────────────────┐                               ║
║                     │   Session JSON          │                               ║
║                     │   (unified format)      │                               ║
║                     │   .nika/sessions/       │                               ║
║                     └─────────────────────────┘                               ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Key Insight:** Whether user runs `nika workflow.yaml`, `nika chat`, or heartbeat triggers a job, the execution path converges to the same runtime components.

---

## Gap Analysis

### Already Exists (v0.8)

| Component | Location | Status | Reused in v0.9 |
|-----------|----------|--------|----------------|
| Session persistence | `src/tui/session.rs` | ✅ Working | Extended for ChatDAG |
| Config system | `src/tui/config.rs` | ✅ Working | Extended for heartbeat |
| Runner (DAG exec) | `src/runtime/runner.rs` | ✅ Working | Shared by all entry points |
| DataStore | `src/store/mod.rs` | ✅ Working | Shared by Chat + Workflow |
| Executor (5 verbs) | `src/runtime/executor.rs` | ✅ Working | Shared by Chat + Workflow |
| EventLog (22 variants) | `src/event/log.rs` | ✅ Working | Shared by all |
| FlowGraph | `src/dag/flow_graph.rs` | ✅ Working | Shared by all |
| Pause/Resume | `runner.rs` (AtomicBool) | ✅ Working | Shared by all |
| MCP Client | `src/mcp/client.rs` | ✅ Working | Shared by all |
| TUI (4 views) | `src/tui/` | ✅ Working | Extended for DAG panel |
| 6 providers | `src/provider/rig.rs` | ✅ Working | Shared by all |

### To Build (v0.9)

| Feature | New Files | Lines | Purpose |
|---------|-----------|-------|---------|
| Context loading | `src/context/*.rs` | ~450 | File → memory with type inference |
| Agent 3-modes | `src/agent/*.rs` | ~700 | Reference, external, inline + SOUL |
| Skill 3-modes | `src/skill/*.rs` | ~500 | Composable system prompt augments |
| Boot sequence | `src/boot/*.rs` | ~1,100 | 6-phase initialization |
| Discovery | `src/discovery/*.rs` | ~650 | Find agents/skills by name |
| Chat-as-DAG | `src/chat/*.rs` | ~800 | Live DAG + commands + export |
| New YAML files | `src/files/*.rs` | ~900 | user, memory, policies, heartbeat |
| DAG Panel Widget | `src/tui/widgets/dag_panel.rs` | ~350 | Real-time visualization |

**Total new code:** ~5,450 lines

---

## Runtime Architecture Detail

### DataStore — The Shared Memory

```rust
// src/store/mod.rs (EXISTS - v0.8)
pub struct DataStore {
    data: HashMap<String, Value>,  // task_id → output
}

impl DataStore {
    pub fn get(&self, key: &str) -> Option<&Value>;
    pub fn set(&mut self, key: String, value: Value);
    pub fn resolve_binding(&self, path: &str) -> Option<Value>;
}
```

**Used by:**
- Workflow: stores each task's output by task ID
- Chat: stores each message's output by `msg-XXX` ID
- Heartbeat: inherits from triggered workflow

### FlowGraph — The DAG Structure

```rust
// src/dag/flow_graph.rs (EXISTS - v0.8)
pub struct FlowGraph {
    graph: petgraph::DiGraph<TaskNode, ()>,
    node_map: HashMap<String, NodeIndex>,  // task_id → graph index
}

impl FlowGraph {
    pub fn add_task(&mut self, task: Task) -> NodeIndex;
    pub fn add_edge(&mut self, from: &str, to: &str);
    pub fn topological_order(&self) -> Vec<&Task>;
    pub fn ready_tasks(&self, completed: &HashSet<String>) -> Vec<&Task>;
}
```

**Used by:**
- Workflow: built from `flows:` block at parse time
- Chat: built incrementally as messages arrive (linear chain)
- Heartbeat: loads workflow's FlowGraph

### Executor — The 5 Verbs

```rust
// src/runtime/executor.rs (EXISTS - v0.8)
pub struct TaskExecutor {
    mcp_clients: Arc<DashMap<String, McpClient>>,
    http_client: reqwest::Client,
}

impl TaskExecutor {
    pub async fn execute(&self, task: &Task, ctx: &Context) -> TaskResult {
        match &task.action {
            TaskAction::Infer(params) => self.exec_infer(params, ctx).await,
            TaskAction::Exec(params) => self.exec_shell(params, ctx).await,
            TaskAction::Fetch(params) => self.exec_fetch(params, ctx).await,
            TaskAction::Invoke(params) => self.exec_mcp(params, ctx).await,
            TaskAction::Agent(params) => self.exec_agent(params, ctx).await,
        }
    }
}
```

**Same executor for Chat, Workflow, and Heartbeat-triggered workflows.**

---

## Chat ↔ Workflow Unification

### Key Differences Table

```
┌─────────────────────┬───────────────────────────┬───────────────────────────┐
│     ASPECT          │        WORKFLOW           │          CHAT             │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ DAG Creation        │ Parse-time (YAML)         │ Runtime (per message)     │
│                     │ Complete before execution │ Grows dynamically         │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Task IDs            │ Defined in YAML           │ Auto-generated (msg-001)  │
│                     │ "research", "write"       │ "msg-001", "msg-002"      │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ DAG Structure       │ EXPLICIT (flows: block)   │ IMPLICIT (linear chain)   │
│                     │ Branches, parallels       │ Default: sequential       │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Agent Assignment    │ Per-task in YAML          │ /agent command switches   │
│                     │ agent: { use: researcher }│ Global until changed      │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Skill Composition   │ Per-task: skill: [a, b]   │ /skill command adds       │
│                     │                           │ Cumulative until removed  │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Context Loading     │ context: block at start   │ /context command loads    │
│                     │                           │ Additive to conversation  │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Stop Condition      │ DAG complete              │ User ends or /export      │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Reproducibility     │ ✅ Exact replay possible   │ ⚠️ Depends on export      │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ DataStore           │ SAME HashMap<String,Value>│ SAME HashMap<String,Value>│
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ Executor            │ SAME TaskExecutor         │ SAME TaskExecutor         │
├─────────────────────┼───────────────────────────┼───────────────────────────┤
│ EventLog            │ SAME 22 variants          │ SAME 22 variants          │
└─────────────────────┴───────────────────────────┴───────────────────────────┘
```

### Unified Session JSON Format

```json
{
  "id": "session-abc123",
  "type": "chat",           // or "workflow"
  "created_at": "2026-02-24T10:00:00Z",
  "updated_at": "2026-02-24T10:30:00Z",

  "dag": {
    "tasks": ["msg-001", "msg-002", "msg-003"],
    "edges": [
      ["msg-001", "msg-002"],
      ["msg-002", "msg-003"]
    ]
  },

  "datastore": {
    "msg-001": {
      "input": "Research QR trends",
      "output": "## QR Code Market 2026...",
      "tokens": { "input": 150, "output": 1200 },
      "agent": "researcher",
      "skills": ["seo"],
      "duration_ms": 3200
    },
    "msg-002": {
      "input": "Write landing page copy",
      "output": "# Welcome to QR Code AI...",
      "tokens": { "input": 1350, "output": 800 },
      "agent": "researcher",
      "skills": ["seo"],
      "duration_ms": 2100
    }
  },

  "state": {
    "current_agent": "writer",
    "active_skills": ["seo", "brand-voice"],
    "loaded_context": ["brand.md", "persona.json"]
  },

  "history": [
    { "role": "user", "content": "Research QR trends" },
    { "role": "assistant", "content": "## QR Code Market 2026..." },
    { "role": "user", "content": "Write landing page copy" },
    { "role": "assistant", "content": "# Welcome to QR Code AI..." }
  ]
}
```

---

## Heartbeat.yaml — Workflow Trigger System

### Philosophy: Heartbeat is NOT a Workflow

Heartbeat is a **scheduler that triggers workflows**. Each triggered workflow is a DAG that executes through the same runtime.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  💓 HEARTBEAT EXECUTION FLOW                                                  ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌──────────────────┐                                                         ║
║  │  heartbeat.yaml  │                                                         ║
║  │  ┌────────────┐  │                                                         ║
║  │  │ schedules: │  │                                                         ║
║  │  │  - cron:   │──┼──┐                                                      ║
║  │  │    workflow│  │  │                                                      ║
║  │  │  - cron:   │──┼──┤                                                      ║
║  │  │    workflow│  │  │                                                      ║
║  │  └────────────┘  │  │                                                      ║
║  │  ┌────────────┐  │  │                                                      ║
║  │  │ hooks:     │  │  │                                                      ║
║  │  │  on_start: │──┼──┤                                                      ║
║  │  │  on_end:   │──┼──┤                                                      ║
║  │  │  on_error: │──┼──┤                                                      ║
║  │  └────────────┘  │  │                                                      ║
║  └──────────────────┘  │                                                      ║
║                        │                                                      ║
║                        ▼                                                      ║
║           ┌────────────────────────┐                                          ║
║           │  workflows/*.nika.yaml │                                          ║
║           └───────────┬────────────┘                                          ║
║                       │                                                       ║
║                       ▼                                                       ║
║           ┌────────────────────────┐                                          ║
║           │   Runner (same!)       │                                          ║
║           │   FlowGraph → Executor │                                          ║
║           │   DataStore → EventLog │                                          ║
║           └────────────────────────┘                                          ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### heartbeat.yaml Format (v0.6)

```yaml
# .nika/heartbeat.yaml
schema: "nika/heartbeat@0.6"

schedules:
  # ═══════════════════════════════════════════════════════════════════════════
  # CRON-TRIGGERED WORKFLOWS
  # Each schedule triggers a workflow file which is a DAG
  # ═══════════════════════════════════════════════════════════════════════════

  - id: morning-seo-audit
    description: "Daily SEO health check"
    cron: "0 9 * * *"                          # Every day at 9:00 AM
    workflow: workflows/seo-audit.nika.yaml
    context:                                    # Passed to workflow as {{boot.trigger}}
      trigger_type: "scheduled"
      check_scope: "daily"
    enabled: true

  - id: weekly-content-refresh
    description: "Refresh top-performing content"
    cron: "0 10 * * MON"                       # Mondays at 10:00 AM
    workflow: workflows/content-refresh.nika.yaml
    context:
      trigger_type: "scheduled"
      scope: "top_10_pages"
      notify: ["slack", "email"]
    enabled: true

  - id: monthly-competitor-analysis
    description: "Deep competitor research"
    cron: "0 6 1 * *"                          # 1st of month at 6:00 AM
    workflow: workflows/competitor-analysis.nika.yaml
    context:
      trigger_type: "scheduled"
      depth: "comprehensive"
    enabled: true

hooks:
  # ═══════════════════════════════════════════════════════════════════════════
  # EVENT-TRIGGERED WORKFLOWS (non-temporal)
  # Hooks fire on Nika lifecycle events
  # ═══════════════════════════════════════════════════════════════════════════

  on_session_start:
    workflow: workflows/load-user-memory.nika.yaml
    description: "Load user preferences and recent context"

  on_session_end:
    workflow: workflows/save-learnings.nika.yaml
    description: "Persist conversation insights"

  on_workflow_error:
    workflow: workflows/error-notification.nika.yaml
    context:
      notify_channel: "#nika-alerts"
    description: "Alert team on workflow failures"

  on_workflow_complete:
    workflow: workflows/log-metrics.nika.yaml
    description: "Track workflow performance metrics"

settings:
  # ═══════════════════════════════════════════════════════════════════════════
  # SCHEDULER SETTINGS
  # ═══════════════════════════════════════════════════════════════════════════

  timezone: "Europe/Paris"
  max_concurrent_jobs: 3
  retry_failed_jobs: true
  retry_delay_seconds: 300
  log_level: "info"
```

### Use Cases for QRCode-AI

| Schedule | Workflow | Purpose | Frequency |
|----------|----------|---------|-----------|
| `morning-seo-audit` | `seo-audit.nika.yaml` | Check rankings, broken links, index status | Daily 9am |
| `weekly-content-refresh` | `content-refresh.nika.yaml` | Update top pages with fresh data | Weekly Mon |
| `monthly-competitor-analysis` | `competitor-analysis.nika.yaml` | Deep dive on competitor SEO | Monthly 1st |
| `hourly-serp-monitor` | `serp-monitor.nika.yaml` | Track position changes | Every hour |

### Implementation

```rust
// src/files/heartbeat.rs (~250 lines)

#[derive(Debug, Deserialize)]
pub struct HeartbeatConfig {
    pub schema: String,
    pub schedules: Vec<ScheduleEntry>,
    pub hooks: HeartbeatHooks,
    pub settings: HeartbeatSettings,
}

#[derive(Debug, Deserialize)]
pub struct ScheduleEntry {
    pub id: String,
    pub description: Option<String>,
    pub cron: String,                    // cron-parser crate
    pub workflow: PathBuf,
    pub context: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatHooks {
    pub on_session_start: Option<HookEntry>,
    pub on_session_end: Option<HookEntry>,
    pub on_workflow_error: Option<HookEntry>,
    pub on_workflow_complete: Option<HookEntry>,
}

impl HeartbeatConfig {
    /// Check all schedules, return workflows that should run now
    pub fn due_workflows(&self, now: DateTime<Utc>) -> Vec<&ScheduleEntry> {
        self.schedules.iter()
            .filter(|s| s.enabled && s.is_due(now))
            .collect()
    }

    /// Trigger hook if defined
    pub async fn trigger_hook(&self, hook: HookType, ctx: Value) -> Result<(), NikaError> {
        let entry = match hook {
            HookType::SessionStart => &self.hooks.on_session_start,
            HookType::SessionEnd => &self.hooks.on_session_end,
            HookType::WorkflowError => &self.hooks.on_workflow_error,
            HookType::WorkflowComplete => &self.hooks.on_workflow_complete,
        };

        if let Some(hook) = entry {
            // Load workflow → Runner → Execute (same path as manual workflows)
            let workflow = Workflow::load(&hook.workflow)?;
            let mut runner = Runner::new(workflow)?;
            runner.run_with_context(ctx).await?;
        }
        Ok(())
    }
}
```

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
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum AgentDef {
    Reference(String),              // "researcher"
    External { file: PathBuf },     // { file: ./x.yaml }
    Inline(AgentSpec),              // { system: "...", soul: {...} }
}

// SkillDef with same pattern
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SkillDef {
    Reference(String),              // "seo"
    External { file: PathBuf },     // { file: ./skills/seo.yaml }
    Inline(SkillSpec),              // { system_augment: "..." }
}

// Extended Workflow struct
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workflow {
    pub schema: String,
    pub workflow: String,
    pub description: Option<String>,

    // NEW v0.6 fields
    #[serde(default)]
    pub context: Option<ContextSpec>,
    #[serde(default)]
    pub agents: HashMap<String, AgentDef>,
    #[serde(default)]
    pub skills: HashMap<String, SkillDef>,

    // Existing
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub flows: Vec<Flow>,
    #[serde(default)]
    pub mcp: Option<McpConfig>,
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
// src/context/loader.rs
pub struct ContextLoader;

impl ContextLoader {
    pub async fn load_file(path: &Path) -> Result<Value, NikaError> {
        let content = tokio::fs::read_to_string(path).await?;

        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Ok(serde_json::from_str(&content)?),
            Some("yaml") | Some("yml") => Ok(serde_yaml::from_str(&content)?),
            Some("toml") => Ok(toml::from_str(&content)?),
            Some("md") | Some("txt") => Ok(Value::String(content)),
            _ => Ok(Value::String(content)),
        }
    }

    pub async fn load_glob(pattern: &str, base: &Path) -> Result<Vec<(String, Value)>, NikaError> {
        let mut results = Vec::new();
        for entry in glob::glob(&base.join(pattern).to_string_lossy())? {
            let path = entry?;
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let value = Self::load_file(&path).await?;
            results.push((name, value));
        }
        Ok(results)
    }
}

// src/context/store.rs
pub struct ContextStore {
    files: HashMap<String, Value>,       // Named files
    loaded_at: HashMap<String, Instant>, // For cache invalidation
}

impl ContextStore {
    pub fn get(&self, alias: &str) -> Option<&Value>;
    pub fn get_nested(&self, path: &str) -> Option<Value>;  // "brand.tagline"
}
```

**Tests:** ~25 new tests
**Success criteria:** `{{context.files.brand}}` resolves in prompts

---

### Sprint 3: Agent 3-Modes + SOUL Pattern

**Goal:** Load agents from reference, external file, or inline with SOUL sections.

**Dependency:** Sprint 1 (AgentDef exists)

**Files to modify:**
- `src/ast/agent.rs` (+150 lines) - Add SOUL sections
- `src/runtime/rig_agent_loop.rs` (+100 lines) - Build system prompt from SOUL

**New files:**
- `src/agent/mod.rs` (~50 lines)
- `src/agent/soul.rs` (~200 lines) - AgentSoul struct
- `src/agent/loader.rs` (~300 lines) - 3-mode loading
- `src/agent/inheritance.rs` (~150 lines) - Agent inheritance

**SOUL Pattern (8 sections):**
```yaml
# .nika/agents/researcher.agent.yaml
name: researcher
version: 1.0.0
description: "Web research specialist"

soul:
  role: |
    You are a research specialist focused on market intelligence.
  mission: |
    Find accurate, recent information from authoritative sources.
  personality: |
    Thorough, skeptical of claims, cross-references multiple sources.
  values:
    - accuracy_over_speed
    - cite_sources_always
    - prefer_recent_data

rules:
  - "Always cite sources with URLs"
  - "Prefer information from last 6 months"
  - "Cross-reference at least 2 sources"

tools:
  mcp: [perplexity, brave-search]
  internal: [spawn_agent]

workflow: |
  1. Understand the research question
  2. Search for primary sources
  3. Verify with secondary sources
  4. Synthesize findings

handoffs:
  - to: writer
    when: "research_complete"
    context: ["findings", "sources"]

provider: claude
model: claude-sonnet-4-6
max_turns: 15
temperature: 0.3
```

**System Prompt Assembly:**
```rust
// src/agent/soul.rs
impl AgentSoul {
    pub fn to_system_prompt(&self) -> String {
        let mut sections = Vec::new();

        if let Some(role) = &self.role {
            sections.push(format!("# Who You Are\n{}", role));
        }
        if let Some(mission) = &self.mission {
            sections.push(format!("# Your Mission\n{}", mission));
        }
        if let Some(personality) = &self.personality {
            sections.push(format!("# Your Personality\n{}", personality));
        }
        if !self.values.is_empty() {
            let values = self.values.iter()
                .map(|v| format!("- {}", v))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("# Your Values\n{}", values));
        }
        if !self.rules.is_empty() {
            let rules = self.rules.iter()
                .map(|r| format!("- {}", r))
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("# Rules (MUST follow)\n{}", rules));
        }
        if let Some(workflow) = &self.workflow {
            sections.push(format!("# Your Workflow\n{}", workflow));
        }

        sections.join("\n\n")
    }
}
```

**Tests:** ~40 new tests
**Success criteria:** `agents: { researcher: researcher }` discovers `.nika/agents/researcher.agent.yaml`

---

### Sprint 4: Skill 3-Modes + Composition

**Goal:** Load skills and compose multiple skills on one agent.

**Dependency:** Sprint 1 (SkillDef exists), Sprint 3 (Agent system)

**Files to modify:**
- `src/runtime/rig_agent_loop.rs` (+80 lines) - Apply composed skills

**New files:**
- `src/skill/mod.rs` (~50 lines)
- `src/skill/spec.rs` (~150 lines) - SkillSpec struct
- `src/skill/loader.rs` (~200 lines) - 3-mode loading
- `src/skill/composer.rs` (~100 lines) - Skill composition

**Key implementation:**
```rust
// src/skill/composer.rs
pub struct SkillComposer;

impl SkillComposer {
    /// Merge multiple skills into a single system prompt augmentation
    pub fn compose(skills: &[SkillSpec]) -> ComposedSkill {
        let system_augment = skills.iter()
            .map(|s| format!(
                "## Skill: {}\n\n{}\n",
                s.name,
                s.system_augment
            ))
            .collect::<Vec<_>>()
            .join("\n---\n\n");

        let all_rules: Vec<String> = skills.iter()
            .flat_map(|s| s.rules.clone())
            .collect();

        let all_stop_conditions: Vec<String> = skills.iter()
            .flat_map(|s| s.stop_conditions.clone())
            .collect();

        ComposedSkill {
            system_augment,
            rules: all_rules,
            stop_conditions: all_stop_conditions,
        }
    }
}

// Usage in RigAgentLoop
impl RigAgentLoop {
    fn build_system_prompt(&self) -> String {
        let mut prompt = String::new();

        // 1. Agent SOUL (if exists)
        if let Some(soul) = &self.agent_soul {
            prompt.push_str(&soul.to_system_prompt());
            prompt.push_str("\n\n---\n\n");
        }

        // 2. Composed skills
        if !self.skills.is_empty() {
            let composed = SkillComposer::compose(&self.skills);
            prompt.push_str("# Active Skills\n\n");
            prompt.push_str(&composed.system_augment);
        }

        prompt
    }
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

**6 Phases with Token Budget:**
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🚀 BOOT SEQUENCE (6 PHASES)                                                  ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Phase 1: IDENTITY                      ~500 tokens                           ║
║  └── Load .nika/user.yaml                                                     ║
║      ├── User name, role, preferences                                         ║
║      └── Communication style                                                  ║
║                                                                               ║
║  Phase 2: MEMORY                        ~500 tokens                           ║
║  └── Load .nika/memory.yaml                                                   ║
║      ├── Known facts                                                          ║
║      ├── Project context                                                      ║
║      └── Previous learnings                                                   ║
║                                                                               ║
║  Phase 3: RULES                         ~500 tokens                           ║
║  └── Load .nika/policies.yaml                                                 ║
║      ├── Boundaries (forbidden actions)                                       ║
║      ├── Constraints (limits)                                                 ║
║      └── Preferences (defaults)                                               ║
║                                                                               ║
║  Phase 4: TOOLS                         ~500 tokens                           ║
║  └── Connect MCP servers                                                      ║
║      ├── List available tools                                                 ║
║      └── Verify connectivity                                                  ║
║                                                                               ║
║  Phase 5: PERSONA                       ~2000 tokens                          ║
║  └── Inject agent SOUL                                                        ║
║      ├── Role, mission, personality                                           ║
║      ├── Rules, workflow                                                      ║
║      └── Composed skills                                                      ║
║                                                                               ║
║  Phase 6: READY                         —                                     ║
║  └── BootContext complete                                                     ║
║      └── System prompt assembled                                              ║
║                                                                               ║
║  TOTAL: ~4,000 tokens (configurable per BootLevel)                            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Boot Levels:**
```rust
pub enum BootLevel {
    Minimal,   // ~500 tokens  - Identity only
    Standard,  // ~2000 tokens - Identity + Memory + Rules
    Full,      // ~4000 tokens - All 6 phases
}
```

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

**`nika init` creates:**
```
.nika/
├── config.toml           # User preferences
├── user.yaml             # Identity (Phase 1)
├── memory.yaml           # Known facts (Phase 2)
├── policies.yaml         # Boundaries (Phase 3)
├── heartbeat.yaml        # Scheduled jobs (NEW)
├── agents/               # Agent definitions
│   └── default.agent.yaml
├── skills/               # Skill definitions
├── context/              # Memory files
├── sessions/             # Session persistence
├── traces/               # Execution traces
└── cache/                # Embeddings, etc.
```

**Discovery Priority:**
```rust
// src/discovery/agents.rs
impl AgentDiscovery {
    /// Find agent by name, checking multiple locations
    pub fn find(name: &str, workflow_dir: &Path) -> Option<PathBuf> {
        // 1. Check .nika/agents/<name>.agent.yaml
        let project_path = find_project_root()?.join(".nika/agents")
            .join(format!("{}.agent.yaml", name));
        if project_path.exists() {
            return Some(project_path);
        }

        // 2. Check workflow-relative ./agents/<name>.agent.yaml
        let relative_path = workflow_dir.join("agents")
            .join(format!("{}.agent.yaml", name));
        if relative_path.exists() {
            return Some(relative_path);
        }

        // 3. Not found
        None
    }
}
```

**Tests:** ~25 new tests
**Success criteria:** Discovery finds agents/skills by name

---

### Sprint 7: Chat-as-DAG with Real-Time Visualization

**Goal:** Chat messages as DAG nodes with commands and live visualization.

**Dependency:** Sprint 5-6 (Boot, Project)

**Files to modify:**
- `src/tui/views/chat.rs` (+400 lines)
- `src/tui/session.rs` (+150 lines)

**New files:**
- `src/chat/mod.rs` (~50 lines)
- `src/chat/dag.rs` (~300 lines) - ChatDAG struct
- `src/chat/commands.rs` (~250 lines) - /agent, /skill, /context, /export
- `src/chat/export.rs` (~200 lines) - Export to workflow YAML
- `src/tui/widgets/dag_panel.rs` (~350 lines) - Real-time DAG visualization

**ChatDAG Implementation:**
```rust
// src/chat/dag.rs
pub struct ChatDAG {
    tasks: Vec<ChatTask>,
    edges: Vec<(String, String)>,      // (parent_id, child_id)
    datastore: DataStore,               // Shared with workflows!
    current_agent: Option<String>,
    current_skills: Vec<String>,
    loaded_context: Vec<String>,
}

pub struct ChatTask {
    pub id: String,                     // "msg-001"
    pub role: Role,                     // User | Assistant
    pub content: String,
    pub agent: Option<String>,
    pub skills: Vec<String>,
    pub parent: Option<String>,
    pub tokens: Option<TokenUsage>,
    pub duration_ms: Option<u64>,
}

impl ChatDAG {
    /// Called on each user message
    pub fn add_message(&mut self, content: &str) -> ChatTask {
        let id = format!("msg-{:03}", self.tasks.len() + 1);

        let task = ChatTask {
            id: id.clone(),
            role: Role::User,
            content: content.to_string(),
            agent: self.current_agent.clone(),
            skills: self.current_skills.clone(),
            parent: self.tasks.last().map(|t| t.id.clone()),
            tokens: None,
            duration_ms: None,
        };

        // Add edge from previous task (linear chain)
        if let Some(parent) = &task.parent {
            self.edges.push((parent.clone(), id.clone()));
        }

        self.tasks.push(task.clone());
        task
    }

    /// Execute task through shared runtime
    pub async fn execute(&mut self, task: &ChatTask, executor: &TaskExecutor) -> Result<String, NikaError> {
        // Convert ChatTask → Task (5-verb format)
        let workflow_task = task.to_workflow_task();

        // Build context from datastore (same as workflow!)
        let ctx = self.build_context(&task)?;

        // Execute through same executor
        let result = executor.execute(&workflow_task, &ctx).await?;

        // Store result in shared datastore
        self.datastore.set(task.id.clone(), result.output.clone());

        Ok(result.output)
    }

    /// Export to workflow YAML
    pub fn to_workflow(&self) -> Workflow {
        Workflow {
            schema: "nika/workflow@0.6".into(),
            workflow: format!("chat-export-{}", self.session_id()),
            description: Some("Auto-generated from chat session".into()),

            // Collect unique agents used
            agents: self.collect_agents(),

            // Collect unique skills used
            skills: self.collect_skills(),

            // Convert chat tasks to workflow tasks
            tasks: self.tasks.iter()
                .filter(|t| t.role == Role::User)  // Only user prompts
                .map(|t| t.to_workflow_task())
                .collect(),

            // Convert edges to flows
            flows: self.edges.iter()
                .map(|(s, t)| Flow { source: s.clone(), target: t.clone() })
                .collect(),

            ..Default::default()
        }
    }
}
```

**Chat Commands:**
```rust
// src/chat/commands.rs
pub enum ChatCommand {
    Agent(String),           // /agent researcher
    Skill(String),           // /skill seo
    SkillRemove(String),     // /skill -seo
    Context(String),         // /context brand
    Export(ExportFormat),    // /export yaml
    Agents,                  // /agents (list)
    Skills,                  // /skills (list)
    Memory,                  // /memory (list loaded)
    Help,                    // /help
}

impl ChatCommand {
    pub fn parse(input: &str) -> Option<ChatCommand> {
        if !input.starts_with('/') {
            return None;
        }

        let parts: Vec<&str> = input[1..].split_whitespace().collect();
        match parts.as_slice() {
            ["agent", name] => Some(ChatCommand::Agent(name.to_string())),
            ["skill", name] if name.starts_with('-') =>
                Some(ChatCommand::SkillRemove(name[1..].to_string())),
            ["skill", name] => Some(ChatCommand::Skill(name.to_string())),
            ["context", name] => Some(ChatCommand::Context(name.to_string())),
            ["export", "yaml"] => Some(ChatCommand::Export(ExportFormat::Yaml)),
            ["export", "json"] => Some(ChatCommand::Export(ExportFormat::Json)),
            ["agents"] => Some(ChatCommand::Agents),
            ["skills"] => Some(ChatCommand::Skills),
            ["memory"] => Some(ChatCommand::Memory),
            ["help"] => Some(ChatCommand::Help),
            _ => None,
        }
    }
}
```

**Real-Time DAG Panel:**
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🎬 LIVE DAG VISUALIZATION                                                    ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌─────────────────────────────────────┬───────────────────────────────────┐  ║
║  │  CHAT VIEW                          │  DAG PANEL (updates live)         │  ║
║  ├─────────────────────────────────────┼───────────────────────────────────┤  ║
║  │                                     │                                   │  ║
║  │  /agent researcher                  │         (empty)                   │  ║
║  │  ✓ Agent: researcher                │                                   │  ║
║  │                                     │                                   │  ║
║  │  /skill seo                         │                                   │  ║
║  │  ✓ Skill: +seo                      │                                   │  ║
║  │                                     │                                   │  ║
║  │  > Research QR trends               │      ╭───────────╮               │  ║
║  │                                     │      │  msg-001  │ ← NEW         │  ║
║  │  ╭──────────────────────────────╮   │      │researcher │               │  ║
║  │  │ ⚡ Researching...            │   │      │   +seo    │               │  ║
║  │  ╰──────────────────────────────╯   │      ╰─────┬─────╯               │  ║
║  │                                     │            │                      │  ║
║  │  ## QR Code Market 2026...          │            │                      │  ║
║  │                                     │            │                      │  ║
║  │  > Write landing page copy          │      ╭─────┴─────╮               │  ║
║  │                                     │      │  msg-002  │ ← NEW         │  ║
║  │  ╭──────────────────────────────╮   │      │researcher │               │  ║
║  │  │ ⚡ Writing...                │   │      │   +seo    │               │  ║
║  │  ╰──────────────────────────────╯   │      ╰─────┬─────╯               │  ║
║  │                                     │            │                      │  ║
║  │  /agent writer                      │            │                      │  ║
║  │  ✓ Agent: writer                    │            │                      │  ║
║  │                                     │            │                      │  ║
║  │  > Refine the headline              │      ╭─────┴─────╮               │  ║
║  │                                     │      │  msg-003  │ ← NEW         │  ║
║  │  ╭──────────────────────────────╮   │      │  writer   │ ← Changed!    │  ║
║  │  │ ⚡ Refining...               │   │      ╰───────────╯               │  ║
║  │  ╰──────────────────────────────╯   │                                   │  ║
║  │                                     │  Tasks: 3 | Edges: 2             │  ║
║  │  /export yaml                       │  Agent: writer | Skills: seo     │  ║
║  │  ✓ Exported to chat-export.yaml     │                                   │  ║
║  │                                     │                                   │  ║
║  └─────────────────────────────────────┴───────────────────────────────────┘  ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Tests:** ~40 new tests
**Success criteria:**
- `/agent researcher` switches agent
- DAG panel updates in real-time
- `/export yaml` generates valid workflow

---

### Sprint 8: New YAML Files

**Goal:** user.yaml, memory.yaml, policies.yaml, heartbeat.yaml

**Dependency:** Sprint 5 (Boot expects these), Sprint 7 (Heartbeat hooks)

**New files:**
- `src/files/mod.rs` (~50 lines)
- `src/files/user.rs` (~150 lines) - UserProfile
- `src/files/memory.rs` (~200 lines) - MemoryStore
- `src/files/policies.rs` (~200 lines) - PolicySet
- `src/files/heartbeat.rs` (~300 lines) - HeartbeatConfig

**File Formats:**

```yaml
# .nika/user.yaml
schema: "nika/user@0.6"

identity:
  name: "Thibaut"
  role: "Developer & Founder"
  company: "SuperNovae Studio"

preferences:
  language: "fr"                    # Conversation language
  response_style: "concise"         # concise | detailed | balanced
  code_style: "typescript"          # Preferred code examples

communication:
  tone: "professional_friendly"
  formality: "tu"                   # tu | vous
  emoji_usage: "minimal"
```

```yaml
# .nika/memory.yaml
schema: "nika/memory@0.6"

project:
  name: "QRCode-AI"
  domain: "qrcode-ai.com"
  description: "AI-powered QR code generator"
  tech_stack: ["Next.js", "TypeScript", "Neo4j"]

facts:
  - "Primary market: France (fr-FR)"
  - "Target: SMBs needing QR codes"
  - "Competitor: QR Code Generator Pro"

context_files:
  brand: ".nika/context/brand.md"
  persona: ".nika/context/persona.json"
  seo_keywords: ".nika/context/keywords.yaml"
```

```yaml
# .nika/policies.yaml
schema: "nika/policies@0.6"

boundaries:
  forbidden:
    - "Never execute rm -rf commands"
    - "Never expose API keys in output"
    - "Never make external API calls without confirmation"

  require_confirmation:
    - "Database migrations"
    - "File deletions"
    - "Git pushes to main"

constraints:
  max_tokens_per_response: 4000
  max_concurrent_mcp_calls: 5
  timeout_seconds: 120

preferences:
  default_agent: "assistant"
  default_provider: "claude"
  auto_save_sessions: true
```

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
4. Update JSON Schema for v0.6
5. Clippy + fmt + coverage check
6. Changelog + version bump
7. Benchmark performance

**Integration tests:**
```rust
// tests/integration/
├── test_boot_sequence.rs        // 6-phase boot
├── test_context_loading.rs      // File → memory
├── test_agent_3_modes.rs        // Reference, external, inline
├── test_skill_composition.rs    // skill: [a, b, c]
├── test_chat_commands.rs        // /agent, /skill, /context
├── test_chat_dag_export.rs      // /export yaml
├── test_workflow_v06.rs         // New schema fields
├── test_discovery.rs            // Agent/skill lookup
├── test_policy_enforcement.rs   // Boundaries
├── test_heartbeat_scheduling.rs // Cron triggers
├── test_backward_compat.rs      // v0.5 still works
├── test_unified_runtime.rs      // Chat + Workflow same execution
```

**Success criteria:**
- 2,200+ tests passing
- Zero clippy warnings
- All v0.5 workflows still work
- Documentation complete
- Heartbeat cron parsing works

---

## Dependencies Graph

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  📊 SPRINT DEPENDENCIES                                                       ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Sprint 1 (Schema v0.6)                                                       ║
║       │                                                                       ║
║       ├────────────────┬────────────────┐                                     ║
║       ▼                ▼                ▼                                     ║
║  Sprint 2         Sprint 3         Sprint 4                                   ║
║  (Context)        (Agent)          (Skill)                                    ║
║       │                │                │                                     ║
║       └────────────────┼────────────────┘                                     ║
║                        ▼                                                      ║
║                   Sprint 5 (Boot)                                             ║
║                        │                                                      ║
║       ┌────────────────┼────────────────┐                                     ║
║       ▼                ▼                ▼                                     ║
║  Sprint 6         Sprint 7         Sprint 8                                   ║
║  (Discovery)      (Chat-DAG)       (YAML Files)                               ║
║       │                │                │                                     ║
║       └────────────────┼────────────────┘                                     ║
║                        ▼                                                      ║
║                   Sprint 9 (Polish)                                           ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

Parallelizable:
- Sprint 2, 3, 4 can run in parallel (all depend only on Sprint 1)
- Sprint 6, 7, 8 can run in parallel (all depend on Sprint 5)
```

---

## Success Metrics

| Metric | v0.8 | v0.9 Target |
|--------|------|-------------|
| Tests | 1,902 | 2,200+ |
| LOC | ~25,000 | ~30,000 |
| Schema | v0.5 | v0.6 |
| Clippy warnings | 0 | 0 |
| v0.5 compat | N/A | 100% |
| Boot phases | 0 | 6 |
| Agent modes | 1 (inline) | 3 (ref, ext, inline) |
| Skill modes | 0 | 3 (ref, ext, inline) |
| Chat commands | 0 | 8 |
| YAML file types | 1 (workflow) | 5 (workflow, user, memory, policies, heartbeat) |

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

// Deny unknown fields for strict parsing
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrictConfig {
    required_field: String,
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
    persona: ./context/persona.json

agents:
  researcher: researcher              # Reference mode
  writer: { file: ./agents/writer.yaml }  # External mode

skills:
  seo: seo
  tdd: { file: ./skills/tdd.yaml }

tasks:
  - id: research
    agent:
      use: researcher
      skill: [seo]
      prompt: "Research with {{context.files.brand}}"

  - id: write
    agent:
      use: writer
      skill: [seo, tdd]
      prompt: |
        Write content based on:
        {{use.research}}

flows:
  - source: research
    target: write
```

---

## References

- [v0.9 Consolidated Design](./2026-02-24-v09-consolidated-design.md)
- [Project Structure Design](./2026-02-24-nika-project-structure.md)
- [Memory & Agents Design](./2026-02-24-memory-and-agents-design.md)
- [Chat as DAG Design](./2026-02-24-chat-as-workflow-dag.md)

---

## Appendix A: Complete File Structure After v0.9

```
.nika/
├── config.toml              # Editor + session settings (v0.8)
├── user.yaml                # Identity (v0.9)
├── memory.yaml              # Known facts (v0.9)
├── policies.yaml            # Boundaries (v0.9)
├── heartbeat.yaml           # Scheduled jobs (v0.9)
├── agents/
│   ├── researcher.agent.yaml
│   ├── writer.agent.yaml
│   └── reviewer.agent.yaml
├── skills/
│   ├── seo.skill.yaml
│   ├── tdd.skill.yaml
│   └── brand-voice.skill.yaml
├── context/
│   ├── brand.md
│   ├── persona.json
│   └── keywords.yaml
├── sessions/
│   ├── chat-abc123.json
│   └── workflow-def456.json
├── traces/
│   └── <workflow>-<timestamp>.ndjson
└── cache/
    └── embeddings.db
```

---

## Appendix B: Event Flow Diagram

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  📡 EVENT FLOW (22 EventKind variants)                                        ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  User Input                                                                   ║
║       │                                                                       ║
║       ▼                                                                       ║
║  ┌─────────────────┐                                                          ║
║  │  Entry Point    │                                                          ║
║  │  Chat/Workflow/ │                                                          ║
║  │  Heartbeat      │                                                          ║
║  └────────┬────────┘                                                          ║
║           │                                                                   ║
║           ▼                                                                   ║
║  ┌─────────────────┐     ┌─────────────────────────────────────────────────┐  ║
║  │ WorkflowStarted │────►│                   EventLog                      │  ║
║  └────────┬────────┘     │                                                 │  ║
║           │              │  Events written as NDJSON:                      │  ║
║           ▼              │  .nika/traces/<id>-<timestamp>.ndjson           │  ║
║  ┌─────────────────┐     │                                                 │  ║
║  │  TaskScheduled  │────►│  {"kind":"TaskStarted","task_id":"msg-001",...} │  ║
║  └────────┬────────┘     │  {"kind":"ProviderCalled","model":"claude",...} │  ║
║           │              │  {"kind":"AgentTurn","turn":1,"response":"..."}  │  ║
║           ▼              │  {"kind":"TaskCompleted","task_id":"msg-001"...} │  ║
║  ┌─────────────────┐     │                                                 │  ║
║  │   TaskStarted   │────►│                                                 │  ║
║  └────────┬────────┘     └─────────────────────────────────────────────────┘  ║
║           │                                                                   ║
║           ▼                                                                   ║
║  ┌─────────────────┐                                                          ║
║  │ ProviderCalled  │  (infer: verb)                                           ║
║  │ McpInvoke       │  (invoke: verb)                                          ║
║  │ AgentStart      │  (agent: verb)                                           ║
║  │ AgentTurn       │  (each agent turn)                                       ║
║  │ AgentSpawned    │  (spawn_agent tool)                                      ║
║  └────────┬────────┘                                                          ║
║           │                                                                   ║
║           ▼                                                                   ║
║  ┌─────────────────┐                                                          ║
║  │  TaskCompleted  │                                                          ║
║  └────────┬────────┘                                                          ║
║           │                                                                   ║
║           ▼                                                                   ║
║  ┌─────────────────────┐                                                      ║
║  │ WorkflowCompleted   │                                                      ║
║  │ (or WorkflowFailed) │                                                      ║
║  └─────────────────────┘                                                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Appendix C: TUI UX Design — 6-View Architecture

### C.1 View Architecture (v0.9 CONFIRMED)

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🎨 NIKA v0.9 TUI — 6-View DAG-First Architecture                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   [1]           [2]         [3]          [4]          [5]           [6]       ║
║  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐  ║
║  │ EXPLORER │→│   CHAT   │→│  EDITOR  │→│  RUNNER  │→│SCHEDULER │→│SETTINGS│  ║
║  │ Fichiers │ │ Convers. │ │   YAML   │ │ Exécut.  │ │  Jobs    │ │ Config │  ║
║  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘ └────────┘  ║
║   (default)                                                                   ║
║                                                                               ║
║  6 VIEWS SUMMARY:                                                             ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║                                                                               ║
║  ┌─────┬────────────┬─────────────────────────┬─────────────────────────────┐ ║
║  │  #  │ View       │ Purpose                 │ Layout                      │ ║
║  ├─────┼────────────┼─────────────────────────┼─────────────────────────────┤ ║
║  │  1  │ EXPLORER   │ File tree + sessions    │ 30/70 (tree | preview)      │ ║
║  │  2  │ CHAT       │ Conversational workflow │ 70/30 (messages | DAG)      │ ║
║  │  3  │ EDITOR     │ YAML editing            │ 60/40 (code | DAG)          │ ║
║  │  4  │ RUNNER     │ Execution monitoring    │ 4-panel (DAG|Task|Agent|Log)│ ║
║  │  5  │ SCHEDULER  │ Heartbeat/cron jobs     │ 50/50 (jobs | history)      │ ║
║  │  6  │ SETTINGS   │ Configuration           │ List (sections)             │ ║
║  └─────┴────────────┴─────────────────────────┴─────────────────────────────┘ ║
║                                                                               ║
║  NAVIGATION:                                                                  ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Tab         → cycle forward                                                  ║
║  Shift+Tab   → cycle backward                                                 ║
║  1-6         → direct jump                                                    ║
║  e/c/d/r/s/g → letter shortcuts                                               ║
║  Ctrl+H      → help overlay                                                   ║
║  Ctrl+P      → fuzzy search (any view)                                        ║
║  Ctrl+Q      → quit                                                           ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### C.2 EXPLORER View (Vue 1 — Default)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA EXPLORER                                              qrcode-ai/.nika    │
├───────────────────────────────┬─────────────────────────────────────────────────┤
│                               │                                                 │
│  📁 PROJECT             [30%] │  📋 PREVIEW                               [70%] │
│  ─────────────────────────    │  ───────────────────────────────────────────    │
│                               │                                                 │
│  ▼ 📂 .nika/                  │  ┌─────────────────────────────────────────────┐│
│    ▼ 📂 agents/               │  │ researcher.agent.yaml                       ││
│      ▸ 📄 researcher.agent    │  │ ─────────────────────────────────────────── ││
│        writer.agent.yaml      │  │                                             ││
│        reviewer.agent.yaml    │  │ name: researcher                            ││
│    ▼ 📂 skills/               │  │ description: "Web research specialist"      ││
│        seo.skill.yaml         │  │                                             ││
│        tdd.skill.yaml         │  │ provider: claude                            ││
│    ▶ 📂 context/              │  │ model: claude-sonnet-4-6                    ││
│    ▶ 📂 sessions/             │  │ mcp: [perplexity]                           ││
│      config.toml              │  │ max_turns: 15                               ││
│                               │  │                                             ││
│  ▼ 📂 workflows/              │  │ system: |                                   ││
│    ▸ 📄 content-pipeline ⚡3  │  │   You are a research specialist...          ││
│      research-seo.nika.yaml   │  │                                             ││
│      deploy-pages.nika.yaml   │  └─────────────────────────────────────────────┘│
│                               │                                                 │
│  ─────────────────────────    │  ─────────────────────────────────────────────  │
│                               │                                                 │
│  🕐 RECENT SESSIONS           │  📊 DAG PREVIEW (workflows only)                │
│  ─────────────────────────    │  ───────────────────────────────────────────    │
│                               │                                                 │
│  Chat #abc123     2m ago  ▶   │       ╭───────────╮                             │
│  │ researcher +seo            │       │  research │                             │
│  │ 5 msgs, 12K tokens         │       ╰─────┬─────╯                             │
│                               │             │                                   │
│  Workflow run    15m ago  ✓   │       ╭─────┴─────╮                             │
│  │ content-pipeline           │       │   write   │                             │
│  │ 3 tasks, 45s               │       ╰─────┬─────╯                             │
│                               │             │                                   │
│                               │       ╭─────┴─────╮                             │
│                               │       │  publish  │                             │
│                               │       ╰───────────╯                             │
│                               │                                                 │
├───────────────────────────────┴─────────────────────────────────────────────────┤
│  ↑↓ navigate │ Enter open │ R run │ N new │ Ctrl+P search │ Tab→Chat │ ?help   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**EXPLORER Panels:**

| Panel | Position | Size | Content |
|-------|----------|------|---------|
| File Tree | Left top | 30% width | .nika/ + workflows/ tree view |
| Recent Sessions | Left bottom | 30% width | Chat/workflow history |
| Preview | Right top | 70% width | File content preview |
| DAG Preview | Right bottom | 70% width | Workflow graph (if .nika.yaml) |

**EXPLORER Shortcuts:**

| Key | Action |
|-----|--------|
| `↑↓` | Navigate tree |
| `Enter` | Open in Editor |
| `R` | Run workflow directly |
| `N` | New workflow (template) |
| `D` | Delete (with confirm) |
| `Space` | Toggle preview |
| `Ctrl+P` | Fuzzy search all files |
| `Tab` | Switch to Chat view |

**EXPLORER File Icons:**

| Icon | Meaning |
|------|---------|
| 📂 | Directory (▶ collapsed, ▼ expanded) |
| 📄 | Regular file |
| ⚡ | Workflow with N tasks |
| 🤖 | Agent file |
| 🎯 | Skill file |
| 📝 | Context file |

---

### C.3 CHAT View (Vue 2)

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA CHAT                                          [researcher] +seo  🟢 MCP  │
├─────────────────────────────────────────┬───────────────────────────────────────┤
│                                         │                                       │
│  /agent researcher                      │      ╭─────────────╮                  │
│  ✓ Agent: researcher (claude-sonnet-4)  │      │   START     │                  │
│                                         │      ╰──────┬──────╯                  │
│  /skill seo                             │             │                         │
│  ✓ Skill applied: seo (+12 rules)       │      ╭──────┴──────╮                  │
│                                         │      │  msg-001    │ ◐               │
│  /context brand                         │      │ researcher  │                  │
│  ✓ Context: brand.md (2.3KB)            │      │ +seo        │                  │
│                                         │      ╰──────┬──────╯                  │
│  > Recherche les tendances QR codes     │             │                         │
│                                         │      ╭──────┴──────╮                  │
│  ╭────────────────────────────────────╮ │      │  msg-002    │ ●               │
│  │ ⚡ msg-001          ✓ 12.3s       │ │      │ researcher  │                  │
│  │ 🧠 claude-sonnet-4  📊 2K→1.5K    │ │      ╰─────────────╯                  │
│  │ 📤 "## Tendances QR 2026..."      │ │                                       │
│  ╰────────────────────────────────────╯ │      ─────────────────               │
│                                         │      LÉGENDE:                         │
│  ╭────────────────────────────────────╮ │      ● Running                        │
│  │ ⚡ msg-002          ◐ 5.2s...     │ │      ◐ Streaming                      │
│  │ 🧠 claude-sonnet-4  📊 1.2K→...   │ │      ✓ Complete                       │
│  │ 💭 "Generating content..."        │ │      ✗ Failed                         │
│  ╰────────────────────────────────────╯ │                                       │
│                                         │      Context: brand.md               │
│  > _                                    │      Agent: researcher                │
│                                         │      Skills: seo                      │
├─────────────────────────────────────────┴───────────────────────────────────────┤
│  📊 2 tasks | 1 layer | 3.5K tokens | $0.02 | /help | /export yaml             │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Chat Commands:**

| Command | Action | DAG Effect |
|---------|--------|------------|
| `/agent <name>` | Switch agent | Updates node style |
| `/skill <name>` | Apply skill | Adds badge to nodes |
| `/context <file>` | Load context | Shows in sidebar |
| `/export yaml` | Export to workflow | Opens Studio |
| `/branch` | Create parallel branch | Forks DAG |
| `/merge` | Merge branches | Joins paths |
| `/dag` | Toggle DAG panel | Show/hide |

### C.3 Studio View Mockup

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA STUDIO                    workflow.nika.yaml              [▶ RUN] [DAG]  │
├─────────────────────────────────────────┬───────────────────────────────────────┤
│   1│ schema: "nika/workflow@0.6"        │                                       │
│   2│ workflow: content-pipeline         │      ╭──────────────╮                 │
│   3│                                    │      │   research   │                 │
│   4│ context:                           │      │   ⚡ infer   │                 │
│   5│   files:                           │      ╰──────┬───────╯                 │
│   6│     brand: brand.md                │             │                         │
│   7│                                    │      ╭──────┴───────╮                 │
│   8│ agents:                            │      │    write     │                 │
│   9│   researcher: researcher           │      │   ⚡ infer   │                 │
│  10│                                    │      ╰──────┬───────╯                 │
│  11│ tasks:                             │             │                         │
│  12│   - id: research                   │      ╭──────┴───────╮                 │
│  13│     agent:                         │      │   publish    │                 │
│  14│       use: researcher              │      │   📟 exec    │                 │
│  15│       prompt: |                    │      ╰──────────────╯                 │
│  16│         Research QR trends         │                                       │
│  17│     use.ctx: research_result       │      ─────────────────                │
│  18│                                    │      3 tasks | 2 layers              │
│  19│   - id: write                      │      Estimated: ~45s                  │
│  20│     agent:                         │                                       │
│  21│       use: writer                  │      ⚠️ Line 15: Long prompt         │
│  22│       skill: [seo]                 │      ✓ Schema valid                   │
│  23│       prompt: |                    │      ✓ DAG acyclic                    │
│  24│         Write content...           │                                       │
│~ ~ ~│                                   │                                       │
├─────────────────────────────────────────┴───────────────────────────────────────┤
│  Ln 15, Col 8 | YAML | UTF-8 | Ctrl+S save | Ctrl+Z undo | F5 run | Tab→Chat   │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Studio Features:**

| Shortcut | Action |
|----------|--------|
| `F5` | Run workflow with live DAG update |
| `Ctrl+D` | Toggle DAG panel |
| `Ctrl+Shift+V` | Validate schema + DAG |
| `Ctrl+I` | Import from chat export |
| `Ctrl+Z/Y` | Undo/Redo (v0.8) |

**DAG Sync Behavior:**
- Cursor on task → Highlight corresponding node
- Edit task → Node updates in real-time
- Add flow → Edge animates into place
- Delete task → Node fades out

### C.4 Home View Mockup

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  NIKA HOME                                              qrcode-ai/.nika        │
├─────────────────────────────────────────┬───────────────────────────────────────┤
│                                         │                                       │
│  📁 WORKFLOWS              [↑↓ select]  │  🕐 RECENT SESSIONS                   │
│  ─────────────────────────────────────  │  ─────────────────────────────────    │
│                                         │                                       │
│  ▸ content-pipeline.nika.yaml      ⚡3  │   Chat #abc123        2m ago     ▶    │
│    research-keywords.nika.yaml     ⚡2  │   │ researcher +seo                   │
│    seo-analysis.nika.yaml          ⚡4  │   │ 5 messages, 12K tokens            │
│    deploy-pages.nika.yaml          📟2  │   └─ Export: pipeline-draft.yaml     │
│                                         │                                       │
│  📂 locales/                            │   Workflow run        15m ago    ✓    │
│    fr-FR-pipeline.nika.yaml        ⚡5  │   │ content-pipeline.nika.yaml        │
│    en-US-pipeline.nika.yaml        ⚡5  │   │ 3 tasks, 45s, $0.08               │
│                                         │   └─ Trace: abc123.ndjson            │
│                                         │                                       │
│  ─────────────────────────────────────  │   Chat #def456        1h ago     ▶    │
│                                         │   │ default agent                     │
│  ⚡ QUICK ACTIONS           [1-4 jump]  │   │ 12 messages, 8K tokens            │
│  ─────────────────────────────────────  │                                       │
│                                         │  ─────────────────────────────────    │
│  [1] 💬 New Chat                        │                                       │
│  [2] 📝 New Workflow                    │  📊 DAG PREVIEW                       │
│  [3] ▶️  Run Selected                   │  ─────────────────────────────────    │
│  [4] 🔍 Search (Ctrl+P)                 │                                       │
│                                         │      ╭───╮   ╭───╮   ╭───╮           │
│                                         │      │ 1 │───│ 2 │───│ 3 │           │
│                                         │      ╰───╯   ╰───╯   ╰───╯           │
│                                         │      content-pipeline (hover)         │
├─────────────────────────────────────────┴───────────────────────────────────────┤
│  6 workflows | 3 sessions | Tab→Chat | Enter→Open | Space→Preview | ?→Help     │
└─────────────────────────────────────────────────────────────────────────────────┘
```

**Home Navigation:**

| Key | Action |
|-----|--------|
| `↑↓` | Navigate workflows |
| `Enter` | Open in Studio |
| `Space` | Preview DAG |
| `1-4` | Quick actions |
| `Tab` | Cycle to Chat |
| `Ctrl+P` | Fuzzy search |

### C.5 /export Flow — Chat → Studio → Run

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  /EXPORT YAML FLOW                                                            ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  ┌──────────────┐         ┌──────────────┐         ┌──────────────┐          ║
║  │   CHAT       │ /export │   DIALOG     │ confirm │   STUDIO     │          ║
║  │  [DAG Live]  │────────▶│  Save as...  │────────▶│  [Editor]    │          ║
║  └──────────────┘         └──────────────┘         └──────────────┘          ║
║                                  │                        │                   ║
║                                  │                        │ F5               ║
║                                  ▼                        ▼                   ║
║                           .nika/exports/          ┌──────────────┐           ║
║                           chat-<id>.nika.yaml     │   MONITOR    │           ║
║                                                   │  [Execution] │           ║
║                                                   └──────────────┘           ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

GENERATED WORKFLOW EXAMPLE:
────────────────────────────

# Auto-generated from chat session abc123
schema: "nika/workflow@0.6"
workflow: chat-export-abc123
generated_from:
  session: abc123
  timestamp: "2026-02-24T10:30:00Z"

context:
  files:
    brand: brand.md           # From /context brand

agents:
  researcher: researcher      # From /agent researcher

tasks:
  - id: msg-001
    agent:
      use: researcher
      skill: [seo]            # From /skill seo
      prompt: "Recherche les tendances QR codes"
    use.ctx: msg_001_result

  - id: msg-002
    agent:
      use: researcher
      prompt: "Génère le contenu"
      context: "{{use.msg-001}}"

flows:
  - source: msg-001
    target: msg-002
```

### C.6 View Navigation State Machine

```
                              ┌─────────────────────────────────────────────┐
                              │                   NIKA TUI                  │
                              └─────────────────────────────────────────────┘
                                                    │
                                                    │ startup
                                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                                                                 │
│    ┌──────────┐    Tab    ┌──────────┐    Tab    ┌──────────┐    Tab           │
│    │   HOME   │◀─────────▶│   CHAT   │◀─────────▶│  STUDIO  │◀─────────┐       │
│    │    1     │           │    2     │           │    3     │          │       │
│    └────┬─────┘           └────┬─────┘           └────┬─────┘          │       │
│         │                      │                      │                │       │
│         │ Enter                │ /export             │ F5             │       │
│         │ (workflow)           │                      │                │       │
│         ▼                      ▼                      ▼                │       │
│    ┌──────────┐           ┌──────────┐          ┌──────────┐          │       │
│    │  STUDIO  │           │  STUDIO  │          │ MONITOR  │──────────┘       │
│    │ (loaded) │           │ (export) │          │    4     │    Tab           │
│    └──────────┘           └──────────┘          └──────────┘                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

KEYBOARD SHORTCUTS (Global):
────────────────────────────
Tab         → Cycle: Home → Chat → Studio → Monitor → Home
1/2/3/4     → Direct jump to view
Ctrl+H      → Help overlay
Ctrl+P      → Fuzzy search (any view)
Ctrl+Q      → Quit
```

### C.7 Implementation Impact

```
FILES TO MODIFY:
────────────────

src/tui/views/
├── chat.rs         +300 lines  (DAG panel, commands, export)
├── studio.rs       +200 lines  (DAG preview, import, sync)
├── home.rs         +150 lines  (Sessions panel, DAG preview)
└── mod.rs          +50 lines   (View navigation state)

src/tui/widgets/
├── dag_panel.rs    NEW 400 lines (Shared DAG widget)
├── session_list.rs NEW 150 lines (Session browser)
└── export_dialog.rs NEW 100 lines (Export modal)

src/runtime/
├── chat_dag.rs     NEW 300 lines (ChatDAG struct)
└── export.rs       NEW 200 lines (Chat → Workflow converter)

TOTAL: ~1,850 new lines of code
```

### C.8 DagPanel Widget (Shared Component)

```rust
// src/tui/widgets/dag_panel.rs
pub struct DagPanel {
    nodes: Vec<DagNode>,
    edges: Vec<DagEdge>,
    selected: Option<String>,
    animation_state: AnimationState,
}

pub struct DagNode {
    id: String,
    label: String,
    status: NodeStatus,       // Pending, Running, Streaming, Complete, Failed
    verb_icon: char,          // ⚡ 📟 🛰️ 🔌 🐔 🐤
    agent_name: Option<String>,
    skills: Vec<String>,
    position: (u16, u16),     // Computed by layout algorithm
}

pub enum NodeStatus {
    Pending,                  // Gray, dashed border
    Running,                  // Blue, pulsing ●
    Streaming,                // Blue, spinner ◐
    Complete,                 // Green, solid ✓
    Failed,                   // Red, solid ✗
}

impl DagPanel {
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // 1. Compute node positions (topological sort + layer assignment)
        // 2. Draw edges (ASCII lines with arrows)
        // 3. Draw nodes (boxes with status indicators)
        // 4. Highlight selected node
        // 5. Draw legend
    }

    pub fn update(&mut self, event: &EventKind) {
        match event {
            EventKind::TaskStarted { task_id, .. } => {
                self.set_status(task_id, NodeStatus::Running);
            }
            EventKind::AgentTurn { .. } => {
                self.set_status(task_id, NodeStatus::Streaming);
            }
            EventKind::TaskCompleted { task_id, .. } => {
                self.set_status(task_id, NodeStatus::Complete);
            }
            EventKind::TaskFailed { task_id, .. } => {
                self.set_status(task_id, NodeStatus::Failed);
            }
            _ => {}
        }
    }
}
```
