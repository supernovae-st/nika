# Phase 1.1: P-MODEL -- Agent Presets (v0.51)

**Date**: 2026-03-28
**Depends on**: Phase 0 (agents: wiring)
**Schema**: Stays @0.12 (additive field)
**Estimated**: ~800-1000 LOC across 15 files, 30-50 new tests, 2 weeks

---

## Part 1: Agent Preset Resolution

### 1.1 Problem Statement

The `agents:` block exists in the AST (`Workflow.agents: Option<FxHashMap<String, AgentDef>>`), and agents are resolved at workflow start via `ResolvedAssets` (containing `ResolvedAgent` with system, provider, model, max_turns, temperature). However, there is NO mechanism for a task to say `agent: think` and inherit the preset's provider+model+temperature+system. Tasks currently specify `provider:` and `model:` directly, or fall back to workflow defaults.

### 1.2 New YAML Syntax

```yaml
agents:
  think: { system: "You are a deep thinker.", provider: anthropic, model: claude-sonnet-4-6, temperature: 0.3 }
  lite: { system: "Be concise.", provider: groq, model: llama-3.3-70b-versatile, temperature: 0.7 }

tasks:
  - id: plan
    agent: think           # <-- NEW FIELD: references preset name
    infer: "Plan the structure"

  - id: generate
    agent: lite
    temperature: 0.9       # <-- task-level override wins
    infer: "Generate content"
```

### 1.3 Resolution Chain (3 levels)

```
task-level field (provider:, model:, temperature:, system:)
         |
         v  (override if present)
agent preset (resolved from agents: block)
         |
         v  (fallback if not present)
workflow defaults (Workflow.provider, Workflow.model)
```

### 1.4 AST Changes

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/task.rs`** (line ~16, RawTask struct)
- Add field: `pub agent_preset: Option<Spanned<String>>` -- the string name referencing an agent in the `agents:` block
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs`** (line ~1683, KNOWN_TASK_KEYS)
- Add `"agent_preset"` to KNOWN_TASK_KEYS (note: cannot use `"agent"` since that is already a verb key; the YAML surface syntax will use `use_agent:` to avoid collision with the `agent:` verb)
- Actually, re-examining: the verb key `"agent"` is already in KNOWN_TASK_KEYS for the `agent:` verb (line 1707). A separate task-level property must use a different name. Best options: `use_agent:`, `preset:`, or `agent_preset:`. Given the master plan says `agent: think` syntax, we need to disambiguate: when `agent:` value is a plain string (not a mapping), it is a preset reference; when it is a mapping with `prompt:`, it is the agent verb. The parser already handles this via `parse_action()` which checks for the `agent:` key as a mapping. We need to add: if `agent:` is a scalar string AND no `prompt:` key inside, treat it as a preset reference rather than a verb.

**REVISED APPROACH**: Use the `agent:` key itself. The parser at line ~1780 calls `parse_action()` which looks for verb keys. Currently, `agent:` always resolves to `RawAgentAction`. We need to split: if `agent:` value is a simple string, it becomes `agent_preset` (not a verb). If it is a mapping (with `prompt:` inside), it stays as the agent verb.

This requires changes in:

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs`**
- `parse_action()` function: when encountering `agent:` key, check if value is a scalar string. If so, do NOT create a RawAgentAction -- instead, return None for action and store the string as `agent_preset`.
- `parse_task()` function (line ~1752): after `parse_action()`, if action is None, also check for `agent:` scalar and populate `task.agent_preset`.
- Actually, the cleanest approach: add a separate task-level parsing step that checks for `agent:` as a scalar BEFORE `parse_action()`, and if found, stores it as `agent_preset` and skips treating it as a verb.
- ~30 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzed/task.rs`** (line ~21, AnalyzedTask struct)
- Add field: `pub agent_preset: Option<String>`
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzer/`** (wherever tasks are analyzed)
- Validate that if `agent_preset` is set, the name exists in `workflow.agents`. Emit error if not found.
- ~15 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs`** (line ~141, Task struct)
- Add field: `pub agent_preset: Option<String>`
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/lower.rs`** (line ~126, lower_task function)
- Propagate `task.agent_preset` into the lowered Task.
- ~2 LOC

### 1.5 Executor Changes for Preset Resolution

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/resolver.rs`** (line ~44, ResolvedAgent struct)
- Already has: system, provider, model, max_turns, temperature, source. This is sufficient.
- Add `pub max_tokens: Option<u32>` to match InferParams/AgentParams needs.
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs`**
- The runner currently resolves assets at line ~1191 and stores them in `self.resolved_assets`.
- When dispatching a task, the runner needs to merge preset values into the lowered action BEFORE calling `executor.execute()`.
- New function: `apply_agent_preset(task: &Task, action: &mut TaskAction, resolved_assets: &ResolvedAssets)` that reads `task.agent_preset`, looks up the `ResolvedAgent`, and applies provider/model/temperature/system to the action's params (InferParams, AgentParams) if they are not already set at task level.
- This function goes in the runner's task dispatch loop (~line 950-975), just before calling `executor.execute()`.
- ~50 LOC

**New file: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/preset.rs`**
- `pub fn apply_preset_to_action(action: &TaskAction, preset: &ResolvedAgent) -> TaskAction`
- Handles each verb:
  - `Infer`: set provider, model, temperature, system, max_tokens if task-level is None
  - `Agent`: set provider, model, temperature, system if task-level is None
  - `Exec`: no LLM fields, but could set timeout from preset (future)
  - `Fetch`: no LLM fields (future: timeout, headers)
  - `Invoke`: no LLM fields (future: MCP server context)
- ~80 LOC

### 1.6 New EventKind Variant

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs`** (after line ~145, EventKind enum)
- Add:
```rust
AgentPresetUsed {
    task_id: Arc<str>,
    preset_name: String,
    provider: String,
    model: String,
},
```
- Emit this event in the runner when a preset is applied.
- ~10 LOC + ~5 LOC in runner emission

---

## Part 2: Inference Routing with Fallback

### 2.1 New Syntax

```yaml
- id: generate
  agent: lite
  provider: [groq, deepseek, anthropic]   # Fallback chain
  infer: "Generate content"
```

The `provider:` field becomes `String | Vec<String>`. If it is a list, try each in order; on provider error, fall back to the next.

### 2.2 AST Changes for Fallback Chain

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/task.rs`** (line ~27, RawTask)
- Change `provider: Option<Spanned<String>>` to represent either a single string or array.
- Actually, keep `provider` as `Option<Spanned<String>>` and add `pub provider_chain: Option<Spanned<Vec<Spanned<String>>>>`.
- The parser detects if `provider:` is a sequence and populates `provider_chain` instead.
- ~5 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs`**
- In `parse_task()`, after extracting `provider` as a string, also check if the node is a Sequence and parse as `provider_chain`.
- ~15 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/analyzed/task.rs`** (AnalyzedTask)
- Add: `pub provider_chain: Option<Vec<String>>`
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/workflow.rs`** (Task struct)
- Add: `pub provider_chain: Option<Vec<String>>`
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/lower.rs`**
- Propagate `provider_chain` through lowering.
- ~3 LOC

### 2.3 InferParams and AgentParams Changes

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/action.rs`** (InferParams, line ~57)
- Add: `pub provider_chain: Option<Vec<String>>`
- Also propagate through InferParamsHelper deserialization.
- ~10 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/ast/agent.rs`** (AgentParams)
- Add: `pub provider_chain: Option<Vec<String>>`
- ~3 LOC

### 2.4 Executor Fallback Logic

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/infer.rs`** (run_infer, line ~163)
- Currently: `let provider_name = resolved_provider.as_deref().unwrap_or(&self.default_provider);`
- Change to: if `infer.provider_chain` is Some, iterate through the chain. Try the first provider; on `ProviderError` (MissingApiKey, RateLimit, Timeout), try the next. If all fail, return the last error.
- New helper function: `try_providers_with_fallback(&self, chain: &[String], task_id, prompt, ...) -> Result<String, NikaError>`
- ~60 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/agent.rs`** (run_agent, line ~134)
- Same pattern: if `resolved_agent.provider_chain` is Some, try each provider.
- ~40 LOC

### 2.5 Cost Tracking Per Provider Attempt

Each fallback attempt emits its own `ProviderCalled` + `ProviderResponded` (or `ProviderFailed`) events. The existing `cost_usd` field on `ProviderResponded` already tracks per-call cost. Failed attempts still emit `ProviderResponded` with 0 tokens but with the error reason.

### 2.6 New EventKind: ProviderFallback

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs`**
- Add:
```rust
ProviderFallback {
    task_id: Arc<str>,
    from: String,
    to: String,
    reason: String,
},
```
- ~8 LOC

### 2.7 Interaction with Agent Presets

When a task has both `agent: think` (preset defines `provider: anthropic`) and `provider: [groq, anthropic]` (task-level fallback chain), the task-level chain takes precedence over the preset's single provider. The preset's provider becomes the default first entry if no chain is specified.

---

## Part 3: Agent Preset on ALL Verbs

### 3.1 `infer:` -- Full LLM Parameter Inheritance

The preset applies: provider, model, temperature, system, max_tokens.

**Implementation**: Already covered by `apply_preset_to_action()` in Part 1. The function pattern-matches on `TaskAction::Infer` and applies fields from `ResolvedAgent` where InferParams fields are None.

### 3.2 `fetch:` -- No LLM Needed (Future Extension)

For v0.51, `fetch:` ignores agent presets entirely. In future versions, a preset could set timeout and default headers. The `apply_preset_to_action()` function will have a no-op match arm for `TaskAction::Fetch` with a comment marking it as future work.

### 3.3 `exec:` -- No LLM Needed (Future Extension)

Same as fetch. No-op in v0.51. Future: preset could set env vars, timeout, cwd.

### 3.4 `invoke:` -- No LLM Needed (Future Extension)

Same. No-op in v0.51. Future: preset could set MCP server context.

### 3.5 `agent:` -- Already Works, Needs Preset Merging

The `agent:` verb already has its own `provider`, `model`, `temperature`, `system` fields in `AgentParams`. The preset merge applies the same pattern: if the AgentParams field is None, fill it from the preset.

Additionally, the `agent:` verb supports `from:` on `AgentDef` to load external definitions. This already works via the resolver. The change here is that when a task says `agent: think` (string, not mapping), it references the workflow-level preset. When `agent: { prompt: "...", from: ./agent.yaml }` is used, it is the verb form.

**Key disambiguation**: The parser needs to detect `agent:` as a plain string (preset reference) vs mapping (verb). This is the critical parser change in Part 1.

---

## Part 4: `nika:cost` Introspection Tool

### 4.1 Design

A new builtin tool `nika:cost` accessible to agent: verb tasks. Returns accumulated cost data from the EventLog's `ProviderResponded` events.

### 4.2 Response Schema

```json
{
  "total_tokens": 15000,
  "total_input_tokens": 10000,
  "total_output_tokens": 5000,
  "total_cost_usd": 0.045,
  "per_model": {
    "claude-sonnet-4-6": { "input_tokens": 8000, "output_tokens": 4000, "cost_usd": 0.039 },
    "llama-3.3-70b-versatile": { "input_tokens": 2000, "output_tokens": 1000, "cost_usd": 0.006 }
  }
}
```

### 4.3 Implementation

**New file: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/cost.rs`**
- Struct `CostTool` holding `EventLog` reference.
- Implements `BuiltinTool` trait.
- `name()` returns `"cost"`.
- `description()` returns `"Returns accumulated token usage and cost across all tasks"`.
- `call()` iterates over the event log, summing `ProviderResponded` events.
- ~80 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/mod.rs`**
- Add `mod cost;` and `pub use cost::CostTool;`
- ~2 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/router.rs`**
- The `CostTool` needs access to the `EventLog`. The router currently takes `ToolContext` and `MediaToolContext`. We need to either:
  (a) Make CostTool hold an `EventLog` and register it when building the executor, or
  (b) Add an `EventLog` parameter to the router constructor.
- Approach (a) is cleaner: add `pub fn with_cost_tool(mut self, event_log: EventLog) -> Self` on `BuiltinToolRouter`.
- ~5 LOC

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/mod.rs`** (TaskExecutor constructor, line ~189)
- After creating the `BuiltinToolRouter`, call `.with_cost_tool(event_log.clone())`.
- ~3 LOC

### 4.4 Challenge: EventLog Access

The `CostTool.call()` method needs to read the `EventLog`. The `BuiltinTool::call()` signature only takes `&self` and `args: String`. The `CostTool` struct must hold `EventLog` (which is `Arc<RwLock<Vec<Event>>>` under the hood, cheap to clone).

---

## Part 5: Tests

### 5.1 Preset Resolution Chain Tests (10 tests)

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/preset.rs`** (new file, test module)

1. `test_preset_applies_provider_to_infer` -- infer with no provider, preset has provider, result uses preset
2. `test_preset_applies_model_to_infer` -- same for model
3. `test_preset_applies_temperature_to_infer` -- same for temperature
4. `test_preset_applies_system_to_infer` -- same for system
5. `test_task_override_wins_over_preset` -- task has provider set, preset also has it, task wins
6. `test_preset_applies_to_agent_verb` -- agent verb gets preset's provider/model
7. `test_exec_ignores_preset` -- exec task with preset, no LLM fields applied
8. `test_fetch_ignores_preset` -- fetch task with preset, no LLM fields applied
9. `test_invoke_ignores_preset` -- invoke task with preset, no LLM fields applied
10. `test_missing_preset_returns_error` -- task references nonexistent preset name

### 5.2 Fallback Routing Tests (8 tests)

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/tests.rs`**

11. `test_fallback_first_provider_succeeds` -- chain of 3, first works, no fallback
12. `test_fallback_first_fails_second_succeeds` -- first fails, second works
13. `test_fallback_all_fail_returns_last_error` -- all providers fail
14. `test_fallback_emits_provider_fallback_event` -- check event log for ProviderFallback
15. `test_fallback_single_provider_no_chain` -- provider_chain with 1 entry = same as plain provider
16. `test_fallback_chain_with_mock_provider` -- mock provider in chain
17. `test_fallback_preserves_model_per_provider` -- each provider can use different model
18. `test_fallback_cost_tracked_per_attempt` -- each attempt has its own ProviderResponded

### 5.3 Cost Tool Tests (5 tests)

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/builtin/cost.rs`** (test module)

19. `test_cost_tool_empty_log` -- no events, returns zeros
20. `test_cost_tool_single_provider_call` -- one ProviderResponded, correct totals
21. `test_cost_tool_multiple_providers` -- two different models, per_model breakdown
22. `test_cost_tool_with_cached_tokens` -- cache_read_tokens included
23. `test_cost_tool_name_and_schema` -- tool metadata correct

### 5.4 Parser Tests (7 tests)

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs`** (test module)

24. `test_agent_as_string_is_preset_not_verb` -- `agent: think` parsed as preset
25. `test_agent_as_mapping_is_verb` -- `agent: { prompt: "..." }` parsed as verb
26. `test_provider_as_array_is_chain` -- `provider: [a, b, c]` parsed as chain
27. `test_provider_as_string_is_single` -- `provider: anthropic` parsed normally
28. `test_agent_preset_unknown_key_rejected` -- task has unknown key warning

### 5.5 Event Tests (3 tests)

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs`** (test module)

29. `test_agent_preset_used_event_serializes` -- AgentPresetUsed round-trips
30. `test_provider_fallback_event_serializes` -- ProviderFallback round-trips
31. `test_provider_fallback_has_task_id` -- task_id() accessor works

### 5.6 Backward Compatibility Tests (4 tests)

32. `test_no_agent_field_works_as_before` -- task without agent: field, current behavior preserved
33. `test_no_provider_chain_works_as_before` -- single provider string, current behavior preserved
34. `test_workflow_without_agents_block_works` -- no agents: block, tasks run normally
35. `test_existing_agent_verb_unaffected` -- `agent: { prompt: "..." }` still works identically

### 5.7 Integration Tests (5 tests)

**File: `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/tests.rs`**

36. `test_full_preset_flow_with_mock_provider` -- workflow with agents:, task with agent: think, mock provider
37. `test_preset_plus_fallback_chain` -- preset defines primary, task defines fallback chain
38. `test_preset_system_prompt_injection` -- preset's system prompt appears in provider call
39. `test_multiple_tasks_different_presets` -- two tasks with different presets route to different providers
40. `test_preset_resolution_event_emitted` -- check EventLog contains AgentPresetUsed

Estimated total: **40 tests** (in the 30-50 range target).

---

## Part 6: YAML Examples

### Before (v0.49 -- every task repeats provider/model)

```yaml
schema: "nika/workflow@0.12"
provider: anthropic
model: claude-sonnet-4-6

tasks:
  - id: plan
    provider: anthropic
    model: claude-sonnet-4-6
    temperature: 0.3
    system: "You are a strategic planner."
    infer: "Plan the landing page structure"

  - id: research
    provider: deepseek
    model: deepseek-chat
    system: "You are a research assistant."
    infer: "Research QR code trends 2026"

  - id: generate_hero
    provider: groq
    model: llama-3.3-70b-versatile
    temperature: 0.7
    system: "Be concise and creative."
    infer: |
      Generate hero section content using:
      Plan: {{with.plan}}
      Research: {{with.research}}
    with:
      plan: $plan
      research: $research
    depends_on: [plan, research]

  - id: generate_features
    provider: groq
    model: llama-3.3-70b-versatile
    temperature: 0.7
    system: "Be concise and creative."
    infer: |
      Generate features section using:
      Plan: {{with.plan}}
    with:
      plan: $plan
    depends_on: [plan]

  - id: review
    provider: anthropic
    model: claude-sonnet-4-6
    temperature: 0.1
    system: "You are a quality reviewer. Be strict."
    infer: |
      Review content quality:
      Hero: {{with.hero}}
      Features: {{with.features}}
    with:
      hero: $generate_hero
      features: $generate_features
    depends_on: [generate_hero, generate_features]
```

### After (v0.51 -- agent presets, DRY, fallback routing)

```yaml
schema: "nika/workflow@0.12"

agents:
  think:
    system: "You are a strategic planner."
    provider: anthropic
    model: claude-sonnet-4-6
    temperature: 0.3

  search:
    system: "You are a research assistant."
    provider: deepseek
    model: deepseek-chat

  lite:
    system: "Be concise and creative."
    provider: groq
    model: llama-3.3-70b-versatile
    temperature: 0.7

  judge:
    system: "You are a quality reviewer. Be strict."
    provider: anthropic
    model: claude-sonnet-4-6
    temperature: 0.1

tasks:
  - id: plan
    agent: think
    infer: "Plan the landing page structure"

  - id: research
    agent: search
    infer: "Research QR code trends 2026"

  - id: generate_hero
    agent: lite
    provider: [groq, deepseek, anthropic]    # Fallback chain
    with: { plan: $plan, research: $research }
    depends_on: [plan, research]
    infer: |
      Generate hero section content using:
      Plan: {{with.plan}}
      Research: {{with.research}}

  - id: generate_features
    agent: lite
    with: { plan: $plan }
    depends_on: [plan]
    infer: |
      Generate features section using:
      Plan: {{with.plan}}

  - id: review
    agent: judge
    with: { hero: $generate_hero, features: $generate_features }
    depends_on: [generate_hero, generate_features]
    infer: |
      Review content quality:
      Hero: {{with.hero}}
      Features: {{with.features}}
```

**Reduction**: 5 tasks went from 70 lines to 45 lines. Provider/model/system/temperature defined once in agents block (4 presets = 16 lines vs 25 lines of per-task duplication).

---

## Part 7: Timeline

### Week 1: AST Changes + Resolver (Days 1-5)

| Day | Task | Files | LOC | Verification |
|-----|------|-------|-----|-------------|
| 1 | Add `agent_preset` field to RawTask, parser disambiguation | `nika-core/src/ast/raw/task.rs`, `nika-core/src/ast/raw/parser.rs` | ~35 | Parser tests 24-28 pass |
| 1 | Add `provider_chain` to RawTask + parser | Same files | ~20 | Parser tests 26-27 pass |
| 2 | Propagate `agent_preset` and `provider_chain` through analyzer | `nika-core/src/ast/analyzer/*.rs`, `nika-core/src/ast/analyzed/task.rs` | ~20 | Analyzer validates preset name exists |
| 2 | Propagate through lower.rs | `nika-engine/src/ast/lower.rs`, `nika-engine/src/ast/workflow.rs` | ~10 | Lowered Task has fields |
| 3 | Create preset.rs with apply_preset_to_action | `nika-engine/src/runtime/preset.rs` (new) | ~80 | Tests 1-10 pass |
| 3 | Add max_tokens to ResolvedAgent | `nika-engine/src/runtime/resolver.rs` | ~5 | Existing resolver tests pass |
| 4 | Wire preset resolution into runner | `nika-engine/src/runtime/runner.rs` | ~50 | Integration tests 36-40 pass |
| 4 | Add AgentPresetUsed + ProviderFallback events | `nika-event/src/log.rs` | ~20 | Event tests 29-31 pass |
| 5 | Backward compatibility tests + full test suite | All test files | ~30 | Tests 32-35 pass, `cargo test --workspace --lib` green |

### Week 2: Executor Wiring + Fallback Routing + nika:cost (Days 6-10)

| Day | Task | Files | LOC | Verification |
|-----|------|-------|-----|-------------|
| 6 | Implement fallback routing in run_infer | `nika-engine/src/runtime/executor/infer.rs` | ~60 | Tests 11-14 pass (with mock) |
| 6 | Implement fallback routing in run_agent | `nika-engine/src/runtime/executor/agent.rs` | ~40 | Tests 15-18 pass |
| 7 | InferParams + AgentParams: add provider_chain field | `nika-engine/src/ast/action.rs`, `nika-engine/src/ast/agent.rs` | ~15 | Serialization round-trip tests |
| 7 | Wire provider_chain from Task into InferParams/AgentParams via lower.rs | `nika-engine/src/ast/lower.rs` | ~10 | End-to-end with mock |
| 8 | Create CostTool builtin | `nika-engine/src/runtime/builtin/cost.rs` (new) | ~80 | Tests 19-23 pass |
| 8 | Register CostTool in router + executor | `nika-engine/src/runtime/builtin/router.rs`, `mod.rs`, `executor/mod.rs` | ~10 | `nika:cost` appears in tool list |
| 9 | Integration testing: full workflow with presets + fallback + cost | New integration test file | ~50 | Full pipeline works |
| 9 | Update JSON schema | `nika-engine/schemas/nika-workflow.schema.json` | ~20 | Schema validation passes |
| 10 | Documentation, examples, final review | Docs + CLAUDE.md updates | ~30 | `cargo clippy --workspace -- -D warnings` zero warnings |

**Total estimated LOC**: ~635 production + ~300 tests = ~935 LOC

### Dependencies Between Tasks

```
Day 1 (Parser) ──► Day 2 (Analyzer+Lower) ──► Day 3 (Preset resolution)
                                                       │
Day 4 (Events) ◄────────────────────────────────────────┘
       │
       ▼
Day 4 (Runner wiring) ──► Day 5 (Backward compat)
                                  │
Day 6 (Fallback infer) ◄─────────┘
Day 7 (Fallback agent + action changes)
       │
       ▼
Day 8 (CostTool) ──► Day 9 (Integration) ──► Day 10 (Polish)
```

---

## File Summary by Crate

### nika-core (4 files modified)

| File | Change | LOC |
|------|--------|-----|
| `src/ast/raw/task.rs` | Add `agent_preset`, `provider_chain` fields | ~5 |
| `src/ast/raw/parser.rs` | Parse `agent:` as string vs mapping, parse `provider:` as array, update KNOWN_TASK_KEYS | ~50 |
| `src/ast/analyzed/task.rs` | Add `agent_preset`, `provider_chain` fields | ~5 |
| `src/ast/analyzer/` (task analysis) | Validate agent_preset references, propagate provider_chain | ~20 |

### nika-engine (10 files modified, 2 new)

| File | Change | LOC |
|------|--------|-----|
| `src/ast/workflow.rs` | Add `agent_preset`, `provider_chain` to Task | ~5 |
| `src/ast/lower.rs` | Propagate new fields through lowering | ~10 |
| `src/ast/action.rs` | Add `provider_chain` to InferParams + deserialization | ~15 |
| `src/ast/agent.rs` | Add `provider_chain` to AgentParams | ~5 |
| `src/runtime/resolver.rs` | Add `max_tokens` to ResolvedAgent | ~5 |
| `src/runtime/runner.rs` | Wire preset resolution before execute() | ~50 |
| **`src/runtime/preset.rs`** (NEW) | `apply_preset_to_action()` + tests | ~160 |
| `src/runtime/executor/infer.rs` | Fallback routing loop | ~60 |
| `src/runtime/executor/agent.rs` | Fallback routing loop | ~40 |
| `src/runtime/executor/mod.rs` | Register CostTool | ~5 |
| **`src/runtime/builtin/cost.rs`** (NEW) | CostTool implementation + tests | ~100 |
| `src/runtime/builtin/router.rs` | `with_cost_tool()` method | ~10 |
| `src/runtime/builtin/mod.rs` | `mod cost; pub use cost::CostTool;` | ~3 |

### nika-event (1 file modified)

| File | Change | LOC |
|------|--------|-----|
| `src/log.rs` | Add `AgentPresetUsed`, `ProviderFallback` variants + task_id() match arms + tests | ~30 |

---

### Critical Files for Implementation
- `/Users/thibaut/dev/supernovae/nika/tools/nika-core/src/ast/raw/parser.rs` (parser disambiguation: `agent:` string vs mapping, `provider:` string vs array)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/infer.rs` (fallback routing loop in `run_infer`)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner.rs` (wiring preset resolution into task dispatch, ~line 950-975)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/resolver.rs` (ResolvedAgent struct that presets resolve to)
- `/Users/thibaut/dev/supernovae/nika/tools/nika-event/src/log.rs` (new EventKind variants: AgentPresetUsed, ProviderFallback)
