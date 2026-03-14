# Nika Evolution — Revised Implementation Plan

> Concrete implementation details for the 5 evolution priorities.
> Produced from deep codebase audit + scientific literature + competitive analysis.
> Date: 2026-03-14 | Nika v0.27.0 | NovaNet v0.20.0

---

## Philosophy Anchors

These principles are **non-negotiable**. Every design decision must pass all 7:

```
+---+------------------------------+----------------------------------------------+
| # | Principle                    | Violation Example                            |
+---+------------------------------+----------------------------------------------+
| 1 | YAML-first                   | Requiring Rust code to configure routing     |
| 2 | 5 Verbs Only                 | Adding a `strategy:` verb                    |
| 3 | DAG Integrity                | Mutating the running DAG in-place            |
| 4 | MCP Boundary (ADR-003)       | Direct Neo4j access from Nika                |
| 5 | Event Sourcing               | Silent model escalation without events       |
| 6 | Security Model               | exec: defaulting to shell: true              |
| 7 | Forever 0.x.x                | Releasing v1.0.0                             |
+---+------------------------------+----------------------------------------------+
```

---

## Wave Structure

```
WAVE 0  (v0.27.x patches)  Architectural debt cleanup
  |
WAVE 1  (v0.28.0)          P2: Multi-model routing + P5: DAG introspection
  |
WAVE 2  (v0.29.0-v0.30.0)  P3: ConfidenceRouter + P1: Dynamic sub-DAG
  |
WAVE 3  (v0.31.0)          P4: Episodic memory (cross-project)
```

Rationale for ordering:
- P2 is prerequisite for P3 (routing) and P1 (tactics model selection)
- P5 is standalone, low risk, builds confidence
- P3 extends P2 naturally (routing = try cheap model first)
- P1 is highest complexity, benefits from P2+P3 stability
- P4 crosses the Nika/NovaNet boundary, needs the most stable foundation

---

## Wave 0: Architectural Debt (v0.27.x)

Discovered during deep audit. Should be resolved before evolution work.

### 0.1 Binding System Unification

**Problem:** Two binding systems coexist — `use:` (legacy, UseEntry/WiringSpec) and `with:` (v0.28, WithEntry/WithSpec). Separate code paths in executor, template engine, and resolver.

**Plan:**
- v0.27.x: Mark `use:` as deprecated in schema docs (still functional)
- v0.28.0: `with:` becomes canonical. `use:` auto-converts to `with:` in analyzer
- v0.29.0: Remove `use:` code paths

**Files:**
- `src/binding/entry.rs` — Add `From<UseEntry> for WithEntry` conversion
- `src/ast/analyzer/analyze.rs` — Emit deprecation warning for `use:` blocks
- `src/runtime/executor/mod.rs` — Unify dispatch to WithSpec only

### 0.2 DataStore Eviction

**Problem:** DataStore (DashMap) grows unbounded. For long-running workflows with many for_each iterations, memory accumulates.

**Plan:** Add optional TTL or LRU eviction.

**Files:**
- `src/store/mod.rs` — Add `EvictionPolicy` enum (None, Lru(usize), Ttl(Duration))
- `src/store/mod.rs` — Wrap DashMap entries with timestamp for TTL

### 0.3 Context File Size Limits

**Problem:** `context: files:` loading has no size limit. A glob matching 1000 files or a 500MB JSON could OOM.

**Plan:**
- Add `max_file_size` (default 10MB) and `max_total_size` (default 50MB) to ContextConfig
- Add `max_glob_results` (default 100) limit

**Files:**
- `src/ast/context.rs` — Add limit fields to ContextConfig
- `src/runtime/context_loader.rs` — Enforce limits during loading

---

## Wave 1: Foundation (v0.28.0)

### P2: Multi-Model Routing

**Goal:** Per-task `provider:` and `model:` overrides. A workflow can mix Claude for planning, Groq for speed tasks, and Gemini for code review.

#### YAML Syntax

```yaml
schema: nika/workflow@0.12
provider: claude                    # workflow-level default

tasks:
  - id: plan
    infer:
      prompt: "Create a detailed plan for {{use.goal}}"
      provider: claude              # task-level override
      model: claude-sonnet-4-6     # explicit model

  - id: execute_fast
    with:
      plan: $plan
    infer:
      prompt: "Execute step: {{with.plan}}"
      provider: groq               # cheap + fast
      model: llama-3.3-70b-versatile

  - id: review
    with:
      result: $execute_fast
    infer:
      prompt: "Review quality of: {{with.result}}"
      # inherits workflow-level provider: claude

  - id: research_agent
    agent:
      prompt: "Research the topic in depth"
      provider: openai             # agent can have its own provider
      model: gpt-4o
      max_turns: 10
```

#### AST Changes

**File: `src/ast/raw/action.rs`**

```rust
// Existing RawInferParams — add 2 fields
pub struct RawInferParams {
    pub prompt: Option<Spanned<String>>,
    pub model: Option<Spanned<String>>,
    pub temperature: Option<Spanned<f64>>,
    pub system: Option<Spanned<String>>,
    pub max_tokens: Option<Spanned<u32>>,
    pub extended_thinking: Option<Spanned<bool>>,
    pub thinking_budget: Option<Spanned<u32>>,
    pub provider: Option<Spanned<String>>,     // NEW
    // model already exists, just needs provider context
}

// Existing RawAgentParams — add 2 fields
pub struct RawAgentParams {
    pub prompt: Option<Spanned<String>>,
    pub model: Option<Spanned<String>>,
    pub max_turns: Option<Spanned<u32>>,
    pub depth_limit: Option<Spanned<u32>>,
    pub mcp: Option<Vec<Spanned<String>>>,
    pub tools: Option<Vec<Spanned<String>>>,
    pub provider: Option<Spanned<String>>,     // NEW
    // ... other existing fields
}
```

**File: `src/ast/analyzed/action.rs`**

```rust
pub struct AnalyzedInferAction {
    pub prompt: String,
    pub model: Option<String>,
    pub provider: Option<String>,     // NEW — validated against KNOWN_PROVIDERS
    // ... existing fields
}

pub struct AnalyzedAgentAction {
    pub prompt: String,
    pub model: Option<String>,
    pub provider: Option<String>,     // NEW
    // ... existing fields
}
```

**File: `src/ast/analyzer/analyze.rs`**

```rust
// New validation in analyze_task():
if let Some(provider) = &raw_task.infer.provider {
    if !core::providers::is_known_provider(&provider.value) {
        errors.push(AnalyzeError::new(
            NIKA_150_UNKNOWN_PROVIDER,
            provider.span,
            format!("Unknown provider '{}'. Known: {:?}", provider.value, core::providers::llm_names()),
        ));
    }
}
```

**New error codes:**
- `NIKA-150`: UnknownProvider — provider name not in KNOWN_PROVIDERS
- `NIKA-151`: UnknownModel — model not valid for given provider
- `NIKA-152`: ProviderUnavailable — env var / API key not configured

#### Runtime Changes

**File: `src/runtime/executor/mod.rs`**

```rust
// New method: resolve provider for a task
fn resolve_provider(&self, task: &AnalyzedTask) -> Result<RigProvider> {
    // 1. Check task-level provider override
    let provider_name = task.action.provider()
        .or_else(|| self.default_provider_name.as_deref());

    let model_name = task.action.model();

    match provider_name {
        Some(name) => {
            // Cache key is (provider, model) tuple
            let cache_key = format!("{}:{}", name, model_name.unwrap_or("default"));
            self.rig_provider_cache
                .entry(cache_key)
                .or_try_insert_with(|| RigProvider::from_name(name, model_name))
                .map(|v| v.clone())
        }
        None => {
            // Auto-detect from environment (existing behavior)
            RigProvider::auto()
                .ok_or(NikaError::ProviderNotConfigured { ... })
        }
    }
}
```

**File: `src/provider/rig.rs`**

```rust
impl RigProvider {
    /// Create a provider by name with optional model override
    pub fn from_name(name: &str, model: Option<&str>) -> Result<Self> {
        match name {
            "claude" | "anthropic" => {
                let key = resolve_key("ANTHROPIC_API_KEY")?;
                let client = rig::providers::anthropic::Client::new(&key);
                let model = model.unwrap_or("claude-sonnet-4-6");
                Ok(RigProvider::Claude { client, model: model.to_string() })
            }
            "openai" => { /* similar */ }
            "mistral" => { /* similar */ }
            "groq" => { /* similar */ }
            "deepseek" => { /* similar */ }
            "gemini" => { /* similar */ }
            "native" => {
                let model_path = model.ok_or(NikaError::MissingField {
                    field: "model path required for native provider"
                })?;
                Ok(RigProvider::Native { runtime: NativeRuntime::new(model_path)? })
            }
            _ => Err(NikaError::UnknownProvider { name: name.to_string() })
        }
    }
}
```

#### Schema Changes

Bump schema version to `nika/workflow@0.12`. Feature-gate provider/model fields.

#### Test Strategy

| Category | Count | Description |
|----------|-------|-------------|
| AST parsing | 15+ | Parse provider/model from YAML, shorthand + full form |
| Analyzer validation | 10+ | Unknown provider, unknown model, missing key |
| Executor resolution | 10+ | Cascade: task → workflow → auto-detect |
| Provider factory | 7+ | from_name() for each provider + error cases |
| Integration | 3+ | Multi-provider workflow with mock providers |
| **Total** | **45+** | |

#### Events

New event metadata:
```rust
EventKind::TaskStarted {
    task_id: String,
    verb: Verb,
    provider: Option<String>,   // NEW — which provider was resolved
    model: Option<String>,      // NEW — which model was used
}
```

---

### P5: DAG Introspection

**Goal:** 4 new builtin tools that let agents query the DAG and task state at runtime. Enables self-aware agents that can reason about workflow progress.

#### New Tools

**File: `src/tools/dag_tools.rs` (NEW)**

```rust
use crate::dag::Dag;
use crate::store::DataStore;
use std::sync::Arc;

/// nika:dag_info — Static DAG structure
pub struct DagInfoTool {
    dag: Arc<Dag>,
}

/// nika:dag_metrics — Runtime execution metrics
pub struct DagMetricsTool {
    dag: Arc<Dag>,
    store: Arc<DataStore>,
}

/// nika:task_status — Single task status query
pub struct TaskStatusTool {
    store: Arc<DataStore>,
}

/// nika:task_output — Single task output retrieval
pub struct TaskOutputTool {
    store: Arc<DataStore>,
}
```

#### Tool Schemas

**nika:dag_info** (no params)
```json
{
  "task_count": 12,
  "layer_count": 5,
  "tasks": [
    { "id": "step1", "verb": "infer", "depends_on": [] },
    { "id": "step2", "verb": "exec", "depends_on": ["step1"] }
  ],
  "critical_path": ["step1", "step3", "step5"],
  "parallel_groups": [["step2", "step4"]]
}
```

**nika:dag_metrics** (no params)
```json
{
  "completed": 7,
  "running": 2,
  "pending": 3,
  "failed": 0,
  "skipped": 0,
  "total_duration_ms": 4523,
  "estimated_remaining_ms": 2000
}
```

**nika:task_status** (params: `{ "task_id": "step1" }`)
```json
{
  "task_id": "step1",
  "status": "completed",
  "verb": "infer",
  "provider": "claude",
  "started_at": "2026-03-14T10:00:00Z",
  "finished_at": "2026-03-14T10:00:02Z",
  "duration_ms": 2150
}
```

**nika:task_output** (params: `{ "task_id": "step1" }`)
```json
{
  "task_id": "step1",
  "output": "The generated content..."
}
```

#### Runtime Integration

**File: `src/tools/mod.rs`**

```rust
// Extend BuiltinToolRouter
impl BuiltinToolRouter {
    pub fn with_dag_tools(
        dag: Arc<Dag>,
        store: Arc<DataStore>,
    ) -> Self {
        let mut router = Self::new();
        router.register("nika:dag_info", Box::new(DagInfoTool { dag: dag.clone() }));
        router.register("nika:dag_metrics", Box::new(DagMetricsTool { dag, store: store.clone() }));
        router.register("nika:task_status", Box::new(TaskStatusTool { store: store.clone() }));
        router.register("nika:task_output", Box::new(TaskOutputTool { store }));
        router
    }
}
```

**File: `src/runtime/runner.rs`**

```rust
// In Runner::new() or Runner::run():
let builtin_router = BuiltinToolRouter::with_dag_tools(
    Arc::new(self.flow_graph.clone()),
    self.datastore.clone(),
);
// Pass to executor
```

#### YAML Usage

```yaml
tasks:
  - id: monitor_agent
    agent:
      prompt: |
        You are a workflow monitor. Check the DAG progress
        and report any bottlenecks or failures.
      tools:
        - nika:dag_info
        - nika:dag_metrics
        - nika:task_status
        - nika:task_output
      max_turns: 3
```

#### Test Strategy

| Category | Count | Description |
|----------|-------|-------------|
| DagInfoTool | 5+ | Empty DAG, linear, parallel, complex |
| DagMetricsTool | 5+ | Various completion states |
| TaskStatusTool | 5+ | Each status variant, unknown task |
| TaskOutputTool | 5+ | Success, failure, pending, missing |
| Integration | 3+ | Agent using dag tools in workflow |
| **Total** | **23+** | |

---

## Wave 2: Intelligence (v0.29.0-v0.30.0)

### P3: ConfidenceRouter + Guardrails (v0.29.0)

**Goal:** Tiered model escalation (try cheap first, escalate if confidence < threshold) + pre/post guardrail checks.

**Requires:** P2 (multi-model routing) must be complete.

#### YAML Syntax

```yaml
tasks:
  - id: generate
    routing:
      confidence_threshold: 0.85
      tiers:
        - provider: groq
          model: llama-3.3-70b-versatile    # Tier 0: fast + cheap
        - provider: claude
          model: claude-haiku-4-20250514    # Tier 1: medium
        - provider: claude
          model: claude-sonnet-4-6          # Tier 2: expensive + smart
    guardrails:
      pre:
        - type: token_budget
          max_input_tokens: 4000
      post:
        - type: json_valid
        - type: not_empty
        - type: max_length
          chars: 5000
    infer:
      prompt: "Generate a product description"
```

#### AST Types

**File: `src/ast/raw/action.rs`**

```rust
#[derive(Debug, Clone)]
pub struct RawRoutingSpec {
    pub confidence_threshold: Option<Spanned<f64>>,   // 0.0-1.0
    pub tiers: Vec<RawRoutingTier>,
    pub max_escalations: Option<Spanned<u32>>,        // default: tiers.len()
}

#[derive(Debug, Clone)]
pub struct RawRoutingTier {
    pub provider: Spanned<String>,
    pub model: Option<Spanned<String>>,
}

#[derive(Debug, Clone)]
pub struct RawGuardrailSpec {
    pub pre: Option<Vec<RawGuardrailCheck>>,
    pub post: Option<Vec<RawGuardrailCheck>>,
}

#[derive(Debug, Clone)]
pub struct RawGuardrailCheck {
    pub check_type: Spanned<String>,  // "token_budget", "json_valid", "not_empty", etc.
    pub params: Option<FxHashMap<String, Value>>,
}
```

#### Runtime Modules

**File: `src/runtime/routing.rs` (NEW)**

```rust
pub struct ConfidenceRouter {
    tiers: Vec<RoutingTier>,
    threshold: f64,
}

impl ConfidenceRouter {
    /// Try each tier in order until confidence >= threshold
    pub async fn route(
        &self,
        prompt: &str,
        executor: &TaskExecutor,
    ) -> Result<(String, RoutingDecision)> {
        for (i, tier) in self.tiers.iter().enumerate() {
            let provider = executor.create_provider(&tier.provider, tier.model.as_deref())?;
            let result = provider.infer(prompt, None).await?;
            let confidence = self.assess_confidence(&result);

            if confidence >= self.threshold || i == self.tiers.len() - 1 {
                return Ok((result, RoutingDecision {
                    tier_used: i,
                    confidence,
                    escalations: i,
                }));
            }
            // Escalate to next tier
        }
        unreachable!()
    }

    fn assess_confidence(&self, result: &str) -> f64 {
        // Heuristics: response length, contains hedging phrases,
        // structured output validation, etc.
        // Future: LLM-as-judge confidence scoring
    }
}
```

**File: `src/runtime/guardrails.rs` (NEW)**

```rust
pub struct GuardrailRunner {
    pre_checks: Vec<Box<dyn GuardrailCheck>>,
    post_checks: Vec<Box<dyn GuardrailCheck>>,
}

pub trait GuardrailCheck: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self, input: &GuardrailInput) -> Result<GuardrailResult>;
}

pub enum GuardrailResult {
    Pass,
    Warn(String),
    Fail(String),
}

// Built-in checks
pub struct TokenBudgetCheck { max_tokens: u32 }
pub struct JsonValidCheck;
pub struct NotEmptyCheck;
pub struct MaxLengthCheck { chars: usize }
pub struct RegexMatchCheck { pattern: String }
pub struct LocaleMatchCheck { locale: String }
```

#### Events

```rust
EventKind::RoutingDecision {
    task_id: String,
    tier_used: usize,
    provider: String,
    model: String,
    confidence: f64,
    escalations: usize,
}

EventKind::GuardrailResult {
    task_id: String,
    phase: GuardrailPhase,  // Pre or Post
    check_name: String,
    result: GuardrailResult,
}
```

#### Test Strategy

| Category | Count | Description |
|----------|-------|-------------|
| ConfidenceRouter | 10+ | Single tier, escalation, max escalation, all fail |
| GuardrailCheck | 15+ | Each check type with pass/warn/fail |
| Integration | 5+ | Full routing + guardrails pipeline |
| **Total** | **30+** | |

---

### P1: Dynamic Sub-DAG (v0.30.0)

**Goal:** An agent with `strategy: true` can generate YAML sub-workflows at runtime. The sub-DAG is validated, executed by a nested Runner, and results folded back.

**Requires:** P2 (multi-model, for tactics provider) + P3 (confidence routing, for quality).

**Key design decision:** We do NOT mutate the main DAG. Instead, we execute an **isolated sub-DAG** via a nested Runner. This preserves DAG immutability.

#### YAML Syntax

```yaml
tasks:
  - id: strategist
    agent:
      prompt: |
        You are a strategic planner. Decompose this goal into
        concrete steps. Generate a YAML sub-workflow.
        Goal: {{with.goal}}
      strategy: true                    # NEW: enables sub-DAG generation
      tactics_provider: groq            # NEW: cheaper model for executing steps
      tactics_model: llama-3.3-70b      # NEW: explicit tactics model
      fold: true                        # NEW: compress sub-DAG results
      max_sub_tasks: 15                 # NEW: safety limit (default: 20)
      max_turns: 5
```

#### How It Works

```
1. Strategist agent receives goal
2. Agent generates YAML via structured output:
   {"tasks": [...], "flows": [...]}
3. Runtime parses → SubDagSpec
4. Validates: cycle-free, valid verbs, bounded size, no agent: recursion
5. Creates nested Runner with tactics_provider
6. Nested Runner executes sub-DAG
7. If fold: true → LLM summarizes combined outputs
8. Folded result returned as strategist's output
```

#### AST Types

**File: `src/ast/raw/action.rs`**

```rust
// Extend RawAgentParams
pub struct RawAgentParams {
    // ... existing fields
    pub strategy: Option<Spanned<bool>>,             // NEW
    pub tactics_provider: Option<Spanned<String>>,    // NEW
    pub tactics_model: Option<Spanned<String>>,       // NEW
    pub fold: Option<Spanned<bool>>,                  // NEW
    pub max_sub_tasks: Option<Spanned<u32>>,          // NEW (default: 20)
}
```

#### Runtime Module

**File: `src/runtime/strategy.rs` (NEW)**

```rust
use crate::ast::analyzed::AnalyzedWorkflow;
use crate::dag::Dag;
use crate::runtime::runner::Runner;

/// Represents a dynamically generated sub-workflow
#[derive(Debug, Deserialize)]
pub struct SubDagSpec {
    pub tasks: Vec<SubTask>,
    pub flows: Option<Vec<SubFlow>>,
}

#[derive(Debug, Deserialize)]
pub struct SubTask {
    pub id: String,
    pub infer: Option<String>,
    pub exec: Option<String>,
    pub fetch: Option<FetchSpec>,
    pub invoke: Option<InvokeSpec>,
    // NO agent: — prevents recursive strategy
}

/// Validate and execute a sub-DAG
pub struct StrategyExecutor {
    tactics_provider: String,
    tactics_model: Option<String>,
    fold: bool,
    max_sub_tasks: usize,
    max_timeout: Duration,
}

impl StrategyExecutor {
    pub async fn execute(&self, spec: SubDagSpec) -> Result<String> {
        // 1. Validate
        self.validate(&spec)?;

        // 2. Convert to AnalyzedWorkflow
        let workflow = self.to_workflow(spec)?;

        // 3. Build DAG
        let dag = Dag::from_workflow(&workflow)?;

        // 4. Create nested Runner with tactics provider
        let runner = Runner::new(workflow, dag, ...);

        // 5. Execute with timeout
        let result = timeout(self.max_timeout, runner.run()).await??;

        // 6. Fold if requested
        if self.fold {
            self.fold_results(result).await
        } else {
            Ok(self.collect_results(result))
        }
    }

    fn validate(&self, spec: &SubDagSpec) -> Result<()> {
        // Max tasks check
        if spec.tasks.len() > self.max_sub_tasks {
            return Err(NikaError::SubDagTooLarge { ... });
        }
        // No agent: tasks (prevents recursion)
        // Valid task IDs
        // Cycle-free
        Ok(())
    }

    async fn fold_results(&self, results: Vec<TaskResult>) -> Result<String> {
        // Use the strategist's provider (not tactics) for folding
        let prompt = format!(
            "Summarize these {} task results into a coherent response:\n{}",
            results.len(),
            results.iter().map(|r| r.output.as_str()).collect::<Vec<_>>().join("\n---\n")
        );
        let provider = RigProvider::from_name(&self.tactics_provider, None)?;
        provider.infer(&prompt, None).await
    }
}
```

#### Safety Constraints

| Constraint | Default | Config |
|-----------|---------|--------|
| Max sub-tasks | 20 | `max_sub_tasks` in YAML |
| Max nesting depth | 1 | Hardcoded (no recursive strategy) |
| Timeout per sub-DAG | 5 min | Internal constant |
| No agent: in sub-tasks | Enforced | Validation rule |
| No include: in sub-DAG | Enforced | Validation rule |

#### Events

```rust
EventKind::SubDagGenerated {
    parent_task_id: String,
    sub_tasks: Vec<String>,   // sub-task IDs
    sub_flows: usize,
}

EventKind::SubDagCompleted {
    parent_task_id: String,
    completed: usize,
    failed: usize,
    folded: bool,
    duration_ms: u64,
}
```

#### Test Strategy

| Category | Count | Description |
|----------|-------|-------------|
| SubDagSpec parsing | 10+ | Valid YAML, invalid, edge cases |
| Validation | 10+ | Too many tasks, agent: blocked, cycles |
| Nested execution | 5+ | Linear, parallel, with failures |
| Folding | 5+ | With/without fold, large results |
| Safety | 5+ | Timeout, max tasks, nesting prevention |
| **Total** | **35+** | |

---

## Wave 3: Memory (v0.31.0)

### P4: Episodic Memory

**Goal:** Cross-session, cross-workflow learning. Agents remember past experiences and retrieve relevant episodes for new tasks. NovaNet is the memory backend (per golden rule: Knowing -> NovaNet).

**Cross-project:** Requires schema changes in both NovaNet and Nika.

#### NovaNet Schema Changes

**New NodeClass: `AgentEpisode`**

```yaml
# brain/models/node-classes/agent-episode.yaml
name: AgentEpisode
realm: org
layer: output
description: "Record of an agent's task execution, stored for cross-session recall"
properties:
  - name: task_summary
    type: string
    required: true
    description: "Compressed summary of what the agent did"
  - name: key_findings
    type: string[]
    description: "Bullet-point key findings or decisions"
  - name: tools_used
    type: string[]
    description: "MCP tools and builtins used during execution"
  - name: model_used
    type: string
    description: "LLM provider and model used"
  - name: duration_ms
    type: integer
    description: "Execution duration in milliseconds"
  - name: success
    type: boolean
    required: true
    description: "Whether the task completed successfully"
  - name: workflow_name
    type: string
    description: "Source workflow file name"
  - name: error_summary
    type: string
    description: "Error description if failed"
```

**New ArcClasses:**

```yaml
# EPISODE_OF: links episode to the entity it worked on
- name: EPISODE_OF
  family: semantic
  from: AgentEpisode
  to: Entity
  cardinality: many-to-one

# SIMILAR_TO: links episodes with similar task patterns
- name: SIMILAR_TO
  family: semantic
  from: AgentEpisode
  to: AgentEpisode
  cardinality: many-to-many
  properties:
    - name: similarity_score
      type: float

# PRECEDED_BY: temporal ordering of episodes
- name: PRECEDED_BY
  family: semantic
  from: AgentEpisode
  to: AgentEpisode
  cardinality: many-to-one
```

#### Nika Module

**File: `src/memory/mod.rs` (NEW module)**

```rust
pub mod episode;
pub mod recall;
pub mod config;

pub use episode::Episode;
pub use recall::RecallResult;
pub use config::MemoryConfig;

/// MemoryManager coordinates episode storage and recall via NovaNet MCP
pub struct MemoryManager {
    mcp_pool: Arc<McpClientPool>,
    config: MemoryConfig,
    event_log: Arc<EventLog>,
}

impl MemoryManager {
    /// Store an episode after task completion
    pub async fn store_episode(&self, episode: Episode) -> Result<String> {
        // 1. Compress agent trace into summary (via LLM)
        let summary = self.compress_trace(&episode).await?;

        // 2. Write to NovaNet via MCP
        // novanet_write(operation: upsert_node, class: AgentEpisode, ...)
        let episode_key = self.write_episode(&summary).await?;

        // 3. Link to entities via EPISODE_OF arcs
        for entity_key in &episode.entity_keys {
            self.create_arc("EPISODE_OF", &episode_key, entity_key).await?;
        }

        // 4. Link to previous episode via PRECEDED_BY
        if let Some(prev) = &episode.previous_episode_key {
            self.create_arc("PRECEDED_BY", &episode_key, prev).await?;
        }

        // 5. Emit event
        self.event_log.emit(EventKind::EpisodeStored {
            episode_key: episode_key.clone(),
            task_id: episode.task_id.clone(),
            entity_keys: episode.entity_keys.clone(),
        });

        Ok(episode_key)
    }

    /// Recall relevant past episodes for context injection
    pub async fn recall(
        &self,
        query: &str,
        entity_keys: &[String],
        limit: usize,
    ) -> Result<Vec<RecallResult>> {
        // novanet_search(query, kinds: ["AgentEpisode"], limit)
        let results = self.search_episodes(query, entity_keys, limit).await?;

        self.event_log.emit(EventKind::EpisodeRecalled {
            query: query.to_string(),
            results_count: results.len(),
        });

        Ok(results)
    }

    async fn compress_trace(&self, episode: &Episode) -> Result<CompressedEpisode> {
        // Use LLM to summarize the full agent trace into a concise episode
        let prompt = format!(
            "Compress this agent execution into a concise episode summary.\n\
             Task: {}\n\
             Turns: {}\n\
             Tools used: {:?}\n\
             Result: {}\n\n\
             Provide: task_summary (1-2 sentences), key_findings (3-5 bullets)",
            episode.task_description,
            episode.turn_count,
            episode.tools_used,
            episode.result_preview,
        );
        // Use cheap model for compression
        let provider = RigProvider::from_name("groq", None)
            .or_else(|_| RigProvider::auto())?;
        let response = provider.infer(&prompt, None).await?;
        serde_json::from_str(&response).map_err(|e| NikaError::ParseError { ... })
    }
}
```

#### YAML Syntax

```yaml
tasks:
  - id: research
    agent:
      prompt: "Research topic: {{with.topic}}"
      memory:
        store: true           # Persist episode to NovaNet after completion
        recall: true          # Search for similar past episodes before starting
        max_recall: 5         # Max episodes to inject as context
        entity_keys:          # Link episode to these entities
          - "{{with.entity}}"
      max_turns: 10
```

#### Context Injection

When `recall: true`, before the agent starts its first turn:

```
1. MemoryManager.recall(prompt, entity_keys, max_recall)
2. Returns 0-N past episodes
3. Episodes formatted as context and prepended to agent system prompt:
   "You have relevant experience from past tasks:
    - [Episode 1 summary] (3 days ago, succeeded)
    - [Episode 2 summary] (1 week ago, failed — avoid X)
    Use these experiences to inform your approach."
4. Agent proceeds with enriched context
```

#### Events

```rust
EventKind::EpisodeStored {
    episode_key: String,
    task_id: String,
    entity_keys: Vec<String>,
}

EventKind::EpisodeRecalled {
    query: String,
    results_count: usize,
}

EventKind::EpisodeInjected {
    task_id: String,
    episode_keys: Vec<String>,
    total_tokens: usize,
}
```

#### Test Strategy

| Category | Count | Description |
|----------|-------|-------------|
| Episode compression | 5+ | Various trace sizes, failure cases |
| MCP write integration | 5+ | Store, arc creation, error handling |
| MCP search integration | 5+ | Recall with/without results |
| Context injection | 5+ | Formatting, token limits, ordering |
| End-to-end | 3+ | Full store → recall → inject cycle |
| **Total** | **23+** | |

---

## Cross-Cutting: Context Compression

Not a standalone priority but woven into P1 and P4:

| Where | Mechanism | Implementation |
|-------|-----------|---------------|
| P1 sub-DAG | `fold: true` → LLM summarization of sub-DAG outputs | `StrategyExecutor::fold_results()` |
| P4 episodes | Trace compression → concise episode summary | `MemoryManager::compress_trace()` |
| Future | Agent turn history rolling window | Not in this plan |
| Future | Child agent result folding | Extension of P1 fold |

---

## Version Mapping

```
+----------+---------------------+------------------------------------------+
| Version  | Priorities          | Key Deliverables                         |
+----------+---------------------+------------------------------------------+
| v0.27.x  | Wave 0              | Binding unification prep, DataStore      |
|          |                     | eviction, context file limits             |
+----------+---------------------+------------------------------------------+
| v0.28.0  | P2 + P5             | Per-task provider/model, 4 DAG tools     |
|          |                     | Schema @0.12, ~70 new tests              |
+----------+---------------------+------------------------------------------+
| v0.29.0  | P3                  | ConfidenceRouter, Guardrails, ~30 tests  |
+----------+---------------------+------------------------------------------+
| v0.30.0  | P1                  | Dynamic sub-DAG, strategy/tactics,       |
|          |                     | result folding, ~35 tests                |
+----------+---------------------+------------------------------------------+
| v0.31.0  | P4                  | Episodic memory, NovaNet AgentEpisode,   |
|          |                     | recall + inject, ~23 tests               |
+----------+---------------------+------------------------------------------+
```

---

## New Error Codes

| Code | Priority | Description |
|------|----------|-------------|
| NIKA-150 | P2 | UnknownProvider — provider name not recognized |
| NIKA-151 | P2 | UnknownModel — model not valid for provider |
| NIKA-152 | P2 | ProviderUnavailable — API key not configured |
| NIKA-160 | P5 | DagToolError — DAG introspection failure |
| NIKA-170 | P3 | RoutingExhausted — all tiers failed |
| NIKA-171 | P3 | GuardrailFailed — pre/post check blocked |
| NIKA-180 | P1 | SubDagTooLarge — exceeds max_sub_tasks |
| NIKA-181 | P1 | SubDagInvalid — validation failed |
| NIKA-182 | P1 | SubDagTimeout — execution exceeded limit |
| NIKA-183 | P1 | SubDagRecursion — agent: verb in sub-tasks |
| NIKA-190 | P4 | EpisodeStoreFailed — NovaNet write failed |
| NIKA-191 | P4 | EpisodeRecallFailed — NovaNet search failed |

---

## New Event Variants

| Event | Priority | Description |
|-------|----------|-------------|
| TaskStarted (extended) | P2 | +provider, +model fields |
| RoutingDecision | P3 | Tier used, confidence, escalations |
| GuardrailResult | P3 | Check name, phase, pass/warn/fail |
| SubDagGenerated | P1 | Sub-task IDs, flow count |
| SubDagCompleted | P1 | Completed/failed counts, duration |
| EpisodeStored | P4 | Episode key, entity links |
| EpisodeRecalled | P4 | Query, result count |
| EpisodeInjected | P4 | Episode keys injected, token count |

---

## File Impact Summary

### New Files

| File | Priority | Description |
|------|----------|-------------|
| `src/tools/dag_tools.rs` | P5 | 4 DAG introspection tools |
| `src/runtime/routing.rs` | P3 | ConfidenceRouter |
| `src/runtime/guardrails.rs` | P3 | GuardrailRunner + built-in checks |
| `src/runtime/strategy.rs` | P1 | SubDagSpec + StrategyExecutor |
| `src/memory/mod.rs` | P4 | MemoryManager |
| `src/memory/episode.rs` | P4 | Episode types |
| `src/memory/recall.rs` | P4 | Recall logic |
| `src/memory/config.rs` | P4 | MemoryConfig |

### Modified Files

| File | Priority | Changes |
|------|----------|---------|
| `src/ast/raw/action.rs` | P2, P1 | +provider/model fields, +strategy fields |
| `src/ast/analyzed/action.rs` | P2, P1 | +provider/model, +strategy |
| `src/ast/analyzer/analyze.rs` | P2, P3 | +provider validation, +routing/guardrail validation |
| `src/runtime/executor/mod.rs` | P2, P3 | +resolve_provider(), +routing/guardrail integration |
| `src/runtime/runner.rs` | P5, P4 | +Arc<Dag> sharing, +memory manager |
| `src/provider/rig.rs` | P2 | +from_name() factory |
| `src/tools/mod.rs` | P5 | +dag tools registration |
| `src/event/log.rs` | All | +8 new EventKind variants |
| `schemas/nika-workflow.schema.json` | All | +new fields per priority |

---

## Competitive Gap Closure

After all 5 priorities:

```
+----------------------------+--------+-------+----------+
| Capability                 | Before | After | vs Slate |
+----------------------------+--------+-------+----------+
| Multi-model routing        |   No   |  Yes  |  Parity  |
| Context compression        |   No   | Partial| Partial |
| Episodic memory            |   No   |  Yes  |  Parity  |
| Strategy/tactics           |   No   |  Yes  |  Parity  |
| DAG introspection          |   No   |  Yes  | Nika-only|
| Confidence routing         |   No   |  Yes  | Nika-only|
| Guardrails                 |   No   |  Yes  | Nika-only|
| Knowledge graph            |  Yes   |  Yes  | Nika-only|
| YAML-first workflows       |  Yes   |  Yes  | Nika-only|
| 200+ locales               |  Yes   |  Yes  | Nika-only|
| Structured output (4-layer)|  Yes   |  Yes  | Nika-only|
| Event sourcing (34 types)  |  Yes   |  42+  | Nika-only|
+----------------------------+--------+-------+----------+
```

Nika gains parity with Slate on the 4 gaps while retaining 6+ unique advantages.

---

## Sources

| Source | Used For |
|--------|----------|
| Codebase audit (371 files) | Module mapping, type definitions, integration points |
| RLM (MIT 2025) | Reference semantics validation, dynamic DAG inspiration |
| CodeAct (ICML 2024) | Code execution gap analysis |
| THREAD (IJCAI 2025) | Strategy/tactics pattern, per-task model routing |
| Context-Folding (arXiv:2510.11967) | Result folding design for P1 |
| LLM Swarms (arXiv:2506.14496) | Hybrid DAG+LLM validation |
| Memory-R1 (2025) | Episodic memory recall patterns |
| Slate v1.0.15 analysis | Competitive gap identification |
| 05-evolution-roadmap.md | Original priority definitions and wave structure |
| 04-nika-novanet-overlap.md | Boundary rules and synergy opportunities |
