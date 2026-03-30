# Phase 1.3: P-ORCHESTRATE — Goal-Based Orchestration (v0.53)

## Implementation Plan: Nika Phase 1.3 — P-ORCHESTRATE (v0.53)

### Architectural Summary

The design uses **Option B: sub-workflows via `nika:run`**. The DAG remains immutable. The orchestrator is an `agent:` verb task that generates `.nika.yaml` content as strings and executes them through the existing `nika:run` builtin tool. No `DynamicDag`, no DAG mutation.

---

### Part 1: The `goal:` Field

**What changes**: A new optional `goal: Option<Spanned<String>>` field on the workflow AST, threaded through all three phases of the AST pipeline.

**Files to touch (4 files across 2 crates)**:

1. **`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/workflow.rs`** -- Add `pub goal: Option<Spanned<String>>` to `RawWorkflow`. This is the raw YAML-parsed representation.

2. **`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs`** -- Two changes:
   - Add `"goal"` to `known_workflow_keys` at line 1335 (the array that gates NIKA-163 unknown field errors).
   - Parse the `goal:` value in the workflow parsing function (likely around line 1310-1330, alongside other optional fields like `description`).

3. **`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzed/workflow.rs`** -- Add `pub goal: Option<String>` to `AnalyzedWorkflow`. The analyzer passes it through after stripping the `Spanned` wrapper.

4. **`/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzer/analyze.rs`** -- Copy `goal` from `RawWorkflow` to `AnalyzedWorkflow` during analysis.

**Downstream lowering**: The `goal:` field does NOT need to appear on the runtime `Workflow` struct in `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs`. The orchestrator transformation happens BEFORE lowering -- the engine detects `goal:` on the `AnalyzedWorkflow` and rewrites the workflow into an orchestrator agent task before it reaches `lower()`.

**Schema stays @0.12**: Additive field only, no breaking change.

**Estimated LOC**: ~40 across 4 files.

---

### Part 2: How Orchestration Works (Option B Design)

**The Core Flow**:

1. User writes a workflow with `goal:` + `agents:` + task templates.
2. Engine detects `goal:` on `AnalyzedWorkflow` in Runner initialization.
3. Engine wraps the entire workflow execution: instead of running the user's tasks directly, it creates a single orchestrator `agent:` task.
4. The orchestrator agent receives: goal text, task template descriptions, and an empty records accumulator.
5. The orchestrator generates sub-workflow YAML strings and calls `nika:run` (extended to accept inline YAML).
6. Each sub-workflow executes and returns results.
7. The orchestrator reviews results via `nika:records`, decides whether to continue.
8. Repeats until `nika:complete` is called or `max_rounds` is reached.

**Where the orchestrator is constructed**: A new module `orchestrate.rs` in `nika-engine/src/runtime/` that:
- Takes the `AnalyzedWorkflow` with `goal:` set
- Extracts task templates as metadata (id, description, preset, expected inputs/outputs)
- Builds an `AgentParams` for the orchestrator agent
- Injects the orchestrator system prompt
- Configures `tools:` to include `nika:run`, `nika:records`, `nika:cost`, `nika:orchestrate`, `nika:complete`
- Sets `completion: { mode: explicit }` so the orchestrator must call `nika:complete`
- Sets `max_turns` based on `max_rounds * estimated_turns_per_round`

**Key insight**: The orchestrator is a regular `agent:` verb task with a carefully crafted system prompt and specific builtin tools. No new execution mode is needed -- the existing `RigAgentLoop` handles everything.

**Where the interception happens**: In `Runner::run()` at `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs`, early in the function (around line 1134), after loading context but before DAG execution. If `self.workflow.goal.is_some()`, call the orchestrator path instead of the normal DAG path.

**Alternative (cleaner) interception point**: In `Runner::new()` or `Runner::with_event_log()` at line 183-197, rewrite the `AnalyzedWorkflow` to replace its tasks with a single orchestrator agent task. This way the existing DAG execution path handles everything transparently.

I recommend the rewrite approach: `orchestrate::wrap_as_orchestrator(workflow: AnalyzedWorkflow) -> AnalyzedWorkflow` produces a new workflow with one task whose action is the orchestrator agent. This is cleaner because it means zero changes to `Runner::run()`.

---

### Part 3: Orchestrator System Prompt Engineering

The orchestrator's system prompt is the most critical piece. It must be constructed dynamically from the workflow's goal, agents, and task templates.

**Prompt structure** (built by `orchestrate::build_orchestrator_prompt()`):

```
You are a Nika workflow orchestrator. Your goal is to achieve the following:

<goal>
{goal_text}
</goal>

You have access to these task templates that you can compose into sub-workflows:

<templates>
- research (agent: search): Research a topic. Inputs: topic (string). Outputs: key_findings, statistics.
- write_section (agent: lite): Write a content section. Inputs: section (string), context (string). Outputs: content.
- review (agent: think): Review and critique content. Inputs: draft (string). Outputs: issues (array), score (number).
</templates>

You have access to these agent definitions that sub-workflows can use:

<agents>
- think: provider=anthropic, model=claude-sonnet-4-6 (strategic, expensive)
- lite: provider=groq, model=llama-3.3-70b-versatile (fast, cheap)
- search: provider=deepseek, model=deepseek-chat (research, cheap)
</agents>

## How to work

1. Call `nika:orchestrate` to check your current round and budget.
2. Plan your next action based on accumulated records.
3. Generate a valid .nika.yaml workflow as a YAML string.
4. Call `nika:run` with the `yaml_content` parameter to execute it.
5. Call `nika:records` to review results from the sub-workflow.
6. Repeat until the goal is achieved with sufficient confidence.
7. Call `nika:complete` with your final result and confidence score.

## YAML Generation Rules

- Always use `schema: "nika/workflow@0.12"`
- Use `agents:` block to define which models each task uses
- Use `depends_on:` for task ordering
- Use `with: { alias: $task_id }` for data flow between tasks
- Use `record: { compress: true }` on tasks whose output will be passed downstream
- Generated workflows must be valid Nika YAML

## Budget Awareness

Call `nika:cost` before each round to check remaining budget.
If budget is low, use cheaper agents (lite, search) instead of expensive ones (think).

## Completion Criteria

Call `nika:complete` when:
- The goal has been achieved
- Confidence is above the target threshold
- OR max_rounds is approaching and you have the best result so far
```

**Template extraction**: Each task in the original workflow becomes a template description. The orchestrator sees task metadata (id, description, agent name, input/output shape from structured output schema if present) but does NOT see the raw `infer:` prompts -- those are implementation details the orchestrator does not need.

---

### Part 4: Sub-Workflow Generation and `nika:run` Inline YAML

**Critical change**: `nika:run` currently only accepts a file path. For Option B, it must also accept inline YAML content.

**File**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/run.rs`

**Changes to `RunParams`**:
```rust
pub struct RunParams {
    /// Path to the workflow file to execute (mutually exclusive with yaml_content)
    pub workflow: Option<String>,  // Changed from required to optional
    /// Inline YAML content to execute (mutually exclusive with workflow path)
    pub yaml_content: Option<String>,
    // ... existing fields unchanged
}
```

**Changes to `parameters_schema()`**: Add `yaml_content` property. Change `required` from `["workflow"]` to `[]` (neither required, but one must be present). Add validation logic.

**Changes to `call()`**: After parameter parsing, branch:
- If `yaml_content` is present: parse directly with `parse_analyzed(&yaml_content)`
- If `workflow` is present: read file, then parse (existing behavior)
- If neither: error
- If both: error (or: `yaml_content` takes priority)

**Security considerations for inline YAML**:
- Same depth limiting applies (already exists via `WORKFLOW_DEPTH`)
- Same timeout applies
- The inline content is validated by `parse_analyzed()` which runs the full 2-phase AST pipeline
- No file path to canonicalize, which actually simplifies security

**Estimated LOC**: ~60 changes to run.rs, ~10 new tests.

**Agents inheritance**: When the orchestrator generates a sub-workflow, it should include the parent workflow's `agents:` block in the generated YAML. The orchestrator prompt instructs the LLM to copy agent definitions. Alternatively, the `nika:run` tool could inject parent agents automatically -- but this adds complexity. For v0.53, having the orchestrator include agents in generated YAML is simpler and more transparent.

---

### Part 5: Round Tracking and Limits

**New fields on `RawWorkflow` / `AnalyzedWorkflow`** (alongside `goal:`):

- `max_rounds: Option<u32>` -- Maximum orchestration rounds (default: 10). Added to the same 4 files as `goal:`.
- `record_budget: Option<u64>` -- Max total tokens across all records.
- Cost limit: Already exists via `limits: { max_cost_usd: N }` on `AgentParams`. The orchestrator agent inherits this from the workflow-level config or a new `orchestrate:` block.

**Round tracking implementation**: The orchestrator agent's turn count IS the round count. Each `nika:run` call = approximately 1 round. The `nika:orchestrate` tool tracks this by querying internal state.

**New orchestration config block** (optional, alternative to top-level fields):

```yaml
orchestrate:
  max_rounds: 10
  confidence_target: 0.85
  agent: think           # Which agent definition the orchestrator uses
  max_cost_usd: 5.0
```

This is cleaner than scattering fields at the top level. The orchestrator config is parsed into a new `OrchestrateConfig` struct.

**File for config**: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/orchestrate.rs` (NEW, ~40 LOC)

**Events** (added to `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs`):

```rust
OrchestrateStarted { goal: String, max_rounds: u32 },
OrchestrateRoundStarted { round: u32, max_rounds: u32 },
OrchestrateRoundCompleted { round: u32, records_count: u32, confidence: Option<f64> },
OrchestrateGoalAchieved { rounds_used: u32, confidence: f64, total_cost: f64 },
OrchestrateGoalFailed { rounds_used: u32, reason: String },
```

**Estimated LOC**: ~100 for config + events.

---

### Part 6: `nika:orchestrate` Introspection Tool

**New file**: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/orchestrate_tool.rs`

**Returns**:
```json
{
  "round": 3,
  "max_rounds": 10,
  "records_count": 7,
  "goal": "Generate a French landing page for QR Code AI",
  "confidence_target": 0.85,
  "cost_used_usd": 1.23,
  "cost_limit_usd": 5.0
}
```

**Challenge**: The tool needs access to orchestration state (round counter, accumulated records). Since `BuiltinTool::call()` only takes `args: String`, the tool needs shared state. Options:

1. **Shared state via `Arc<OrchestrateState>`**: The orchestrate module creates `Arc<OrchestrateState>` and the tool holds a clone. The state is updated by the orchestrator wrapper.

2. **State injected via args**: The orchestrator agent's system prompt instructs the LLM to pass round info, but this is unreliable.

3. **State via `BuiltinToolRouter` context**: Extend `BuiltinToolRouter` to accept stateful tools (tools that carry context). This requires a minor refactor to the router.

**Recommended approach**: Option 1. Create `OrchestrateState` with interior mutability (`Arc<Mutex<OrchestrateStateInner>>`). The `OrchestrateTool` holds `Arc<OrchestrateState>`. The orchestrator wrapper updates state after each round. The tool reads state on each call.

```rust
pub struct OrchestrateState {
    inner: std::sync::Mutex<OrchestrateStateInner>,
}

struct OrchestrateStateInner {
    round: u32,
    max_rounds: u32,
    records_count: u32,
    goal: String,
    confidence_target: f64,
    cost_used_usd: f64,
    cost_limit_usd: f64,
}
```

**Registration**: When the orchestrator is created, build a custom `BuiltinToolRouter` with the `OrchestrateTool` registered. The router's `register()` method already supports this pattern.

**Estimated LOC**: ~120 for tool + state.

---

### Part 7: YAML Examples

**Complete orchestrate workflow**:

```yaml
schema: "nika/workflow@0.12"
workflow: landing-page-generator

goal: |
  Generate a complete French landing page for QR Code AI.
  Research current trends, write 4 sections (hero, features, pricing, CTA),
  review quality, and iterate until confidence >= 0.85.

orchestrate:
  max_rounds: 8
  confidence_target: 0.85
  agent: think
  max_cost_usd: 5.0

agents:
  think:
    system: "You are a strategic content orchestrator."
    provider: anthropic
    model: claude-sonnet-4-20250514
    temperature: 0.3
  lite:
    system: "You are a fast French copywriter."
    provider: groq
    model: llama-3.3-70b-versatile
  search:
    system: "You are a market research analyst."
    provider: deepseek
    model: deepseek-chat

# Task templates: the orchestrator can compose these into sub-workflows
tasks:
  - id: research
    description: "Research a topic and return key findings"
    agent: search
    infer: "Research: {{with.topic}}"
    record: { compress: true, max_tokens: 300 }

  - id: write_section
    description: "Write a landing page section in French"
    agent: lite
    infer: "Write: {{with.section}} using context: {{with.context}}"
    record: { compress: true, retain: [content], max_tokens: 800 }

  - id: review
    description: "Review content quality and provide a score"
    agent: think
    infer: "Review and critique: {{with.draft}}"
    record: { compress: true, retain: [issues, score] }
    structured:
      schema:
        type: object
        properties:
          issues: { type: array, items: { type: string } }
          score: { type: number }
        required: [issues, score]
```

**What the orchestrator generates internally (Round 1)**:

```yaml
schema: "nika/workflow@0.12"
workflow: __orchestrator_round_1

agents:
  search:
    system: "You are a market research analyst."
    provider: deepseek
    model: deepseek-chat

tasks:
  - id: research_trends
    agent: search
    infer: "Research QR code adoption trends in France for 2026. Include market size, growth rate, and key players."
    record: { compress: true, max_tokens: 400 }
```

**What the orchestrator generates (Round 3, after research is done)**:

```yaml
schema: "nika/workflow@0.12"
workflow: __orchestrator_round_3

agents:
  lite:
    system: "You are a fast French copywriter."
    provider: groq
    model: llama-3.3-70b-versatile

tasks:
  - id: write_hero
    agent: lite
    infer: |
      Write the hero section for a French landing page about QR Code AI.
      Use these research findings: {research_summary_injected_by_orchestrator}
    record: { compress: true, retain: [content] }

  - id: write_features
    agent: lite
    infer: |
      Write the features section for a French landing page about QR Code AI.
      Key features to highlight: {features_from_research}
    record: { compress: true, retain: [content] }

  - id: write_pricing
    agent: lite
    depends_on: [write_hero]
    infer: "Write pricing section in French matching the tone of: {{with.hero}}"
    with: { hero: $write_hero }
    record: { compress: true, retain: [content] }

  - id: write_cta
    agent: lite
    depends_on: [write_hero]
    infer: "Write CTA section in French matching the tone of: {{with.hero}}"
    with: { hero: $write_hero }
    record: { compress: true, retain: [content] }
```

---

### Part 8: Tests (35 estimated)

**AST / Parsing (8 tests)**:
1. `test_goal_field_parsed_from_yaml` -- goal: string appears on RawWorkflow
2. `test_goal_field_multiline` -- goal: with YAML literal block
3. `test_goal_field_optional` -- workflow without goal: still parses
4. `test_goal_field_in_known_keys` -- "goal" not rejected as unknown field
5. `test_goal_field_analyzed` -- goal propagated to AnalyzedWorkflow
6. `test_orchestrate_config_parsed` -- orchestrate: block with all fields
7. `test_orchestrate_config_defaults` -- missing fields get defaults
8. `test_orchestrate_config_optional` -- workflow without orchestrate: is fine

**Orchestrator Rewrite (6 tests)**:
9. `test_goal_triggers_orchestrator_wrap` -- AnalyzedWorkflow with goal gets rewritten
10. `test_orchestrator_has_single_agent_task` -- rewritten workflow has 1 task
11. `test_orchestrator_system_prompt_contains_goal` -- prompt includes goal text
12. `test_orchestrator_system_prompt_contains_templates` -- prompt includes task descriptions
13. `test_orchestrator_tools_include_run_records_cost_complete` -- correct tools registered
14. `test_no_goal_skips_orchestrator` -- normal workflows unchanged

**`nika:run` Inline YAML (8 tests)**:
15. `test_run_inline_yaml_executes` -- yaml_content with valid workflow
16. `test_run_inline_yaml_returns_output` -- output accessible from inline run
17. `test_run_inline_yaml_invalid_yaml_errors` -- malformed YAML returns error
18. `test_run_inline_yaml_depth_limiting` -- depth tracking works for inline
19. `test_run_inline_yaml_timeout` -- timeout applies to inline execution
20. `test_run_mutual_exclusion` -- both workflow + yaml_content is error
21. `test_run_neither_path_nor_content_errors` -- neither provided is error
22. `test_run_inline_preserves_file_path_behavior` -- existing file-based tests still pass

**`nika:orchestrate` Tool (5 tests)**:
23. `test_orchestrate_tool_name` -- name is "orchestrate"
24. `test_orchestrate_tool_schema` -- JSON schema has all properties
25. `test_orchestrate_tool_returns_state` -- returns round, max_rounds, goal, etc.
26. `test_orchestrate_state_updates` -- state changes after round increments
27. `test_orchestrate_tool_properties_have_type` -- OpenAI strict mode compliance

**Events (4 tests)**:
28. `test_orchestrate_started_event` -- emitted when orchestration begins
29. `test_orchestrate_round_events` -- round start/complete events emitted
30. `test_orchestrate_goal_achieved_event` -- emitted on successful completion
31. `test_orchestrate_goal_failed_event` -- emitted on max_rounds exhaustion

**Integration (4 tests)**:
32. `test_orchestrate_end_to_end_with_exec_tasks` -- goal workflow with exec: tasks (no LLM needed)
33. `test_orchestrate_max_rounds_enforcement` -- stops at max_rounds
34. `test_orchestrate_cost_limit_enforcement` -- stops when cost exceeded
35. `test_orchestrate_confidence_threshold` -- completes when confidence met

---

### Part 9: Timeline (3 weeks with Option B)

**Week 1: Foundation (Days 1-5)**
- Day 1-2: Add `goal:` and `orchestrate:` to AST pipeline (4 files in nika-core, lower.rs in nika-engine). 8 parsing tests.
- Day 3-4: Extend `nika:run` with `yaml_content` parameter. 8 tests.
- Day 5: Create `OrchestrateState` + `OrchestrateTool`. 5 tests.

**Week 2: Orchestrator Core (Days 6-10)**
- Day 6-7: Build `orchestrate.rs` -- the `wrap_as_orchestrator()` function that rewrites workflows with `goal:` into a single orchestrator agent task. 6 tests.
- Day 8-9: System prompt engineering. Build `build_orchestrator_prompt()` with template extraction, agent descriptions, YAML generation instructions.
- Day 10: Add 5 new EventKind variants and event emission points. 4 tests.

**Week 3: Integration and Polish (Days 11-15)**
- Day 11-12: Integration testing with real LLM calls (or mocked). 4 integration tests.
- Day 13: Display/renderer support for orchestrate events (spinner text, round indicators).
- Day 14: LSP support -- `goal:` field completion, orchestrate: block validation.
- Day 15: Documentation, CLAUDE.md updates, showcase workflows.

---

### Part 10: File Summary with LOC Estimates

**New Files**:

| File | Purpose | LOC |
|------|---------|-----|
| `nika-engine/src/runtime/orchestrate.rs` | Orchestrator rewrite logic + prompt builder | ~350 |
| `nika-engine/src/runtime/builtin/orchestrate_tool.rs` | `nika:orchestrate` introspection tool | ~120 |
| `nika-core/src/ast/raw/orchestrate.rs` | `RawOrchestrateConfig` struct | ~40 |
| `nika-core/src/ast/analyzed/orchestrate.rs` | `OrchestrateConfig` (analyzed) | ~30 |

**Modified Files**:

| File | Change | LOC Delta |
|------|--------|-----------|
| `nika-core/src/ast/raw/workflow.rs` | Add `goal:`, `orchestrate:` fields | +5 |
| `nika-core/src/ast/raw/parser.rs` | Parse `goal:`, `orchestrate:`, add to known_keys | +30 |
| `nika-core/src/ast/analyzed/workflow.rs` | Add `goal:`, `orchestrate:` fields | +5 |
| `nika-core/src/ast/analyzer/analyze.rs` | Thread `goal:`, `orchestrate:` through analysis | +15 |
| `nika-engine/src/runtime/builtin/run.rs` | Add `yaml_content` parameter, dual-mode dispatch | +60 |
| `nika-engine/src/runtime/builtin/router.rs` | Register `orchestrate` tool when in orchestrate mode | +5 |
| `nika-engine/src/runtime/runner.rs` | Detect `goal:` and call `wrap_as_orchestrator()` | +10 |
| `nika-event/src/log.rs` | 5 new EventKind variants | +20 |
| `nika-engine/src/runtime/mod.rs` | Add `pub mod orchestrate;` | +1 |
| `nika-engine/src/runtime/builtin/mod.rs` | Add `mod orchestrate_tool;` + re-export | +3 |
| `nika-engine/src/display/format_event.rs` | Format new orchestrate events | +30 |

**Total estimate**: ~540 new LOC + ~185 modified LOC = ~725 production LOC, plus ~600 LOC of tests.

---

### Dependencies and Prerequisites

Phase 1.3 depends on:
- **Phase 1.1 (P-MODEL)**: The `agents:` block must be wirable to tasks via an `agent:` shorthand. Currently, `agents:` exists in the AST but tasks cannot reference them via a simple `agent: think` field. This must be implemented first.
- **Phase 1.2 (P-RECORD)**: The `record:` field and `nika:records` tool must exist for the orchestrator to review results between rounds. The `RecordCompressor` feeds the orchestrator's decision loop.
- **`nika:cost` tool**: Must exist (planned for P-MODEL v0.51) for budget awareness.

If those are not yet implemented, the orchestrator can still work in a degraded mode (no records compression, no cost introspection), but the quality of orchestration will be significantly lower.

---

### Potential Challenges

1. **LLM YAML generation quality**: The orchestrator must generate valid `.nika.yaml`. Modern LLMs (Claude Sonnet 4, GPT-4o) are good at YAML but not perfect. Mitigations: strict schema validation in `parse_analyzed()` catches errors immediately; the orchestrator can retry on parse failures; the system prompt includes concrete YAML examples.

2. **Context window management**: Each orchestrator round accumulates records. After 5+ rounds, the context may grow large. Mitigation: the orchestrator system prompt instructs the LLM to use `nika:records` selectively rather than accumulating everything in the conversation history.

3. **Inline YAML in `nika:run`**: The YAML content may be very large (multi-KB). The tool's JSON parameters must handle this. LLMs can generate large JSON string values, but there is a practical limit. Mitigation: sub-workflows should be concise (3-10 tasks).

4. **Testing without LLMs**: Integration tests that exercise the full orchestrator loop require LLM calls. Mitigation: mock the agent loop for unit tests; have a small number of integration tests that use exec: tasks (shell commands, no LLM needed) to verify the orchestrator wrapping and round tracking.

### Critical Files for Implementation
- `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs` (known_workflow_keys + goal parsing)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/run.rs` (inline YAML support for nika:run)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` (orchestrator detection and rewrite hook)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzed/workflow.rs` (AnalyzedWorkflow gets goal + orchestrate fields)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs` (new EventKind variants for orchestration)
