# Session L: P-MODEL Complete -- Agent Presets on All Verbs + nika:cost (~4-5h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-28-phase1-model.md` -- READ IT FIRST.
Master plan: `docs/plans/2026-03-28-v1-master-plan.md` for Phase 1.1 scope.
Dev reference: `tools/nika/CLAUDE.md` for conventions.

Depends on: Session J (preset: wiring) and Session K (fallback chains).

Key files:
- `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC) -- YAML parsing
- `tools/nika-core/src/ast/raw/task.rs` (209 LOC) -- RawTask struct
- `tools/nika-engine/src/runtime/runner.rs` (6524 LOC) -- task dispatch
- `tools/nika-engine/src/runtime/resolver.rs` (764 LOC) -- ResolvedAgent
- `tools/nika-engine/src/runtime/executor/infer.rs` (1283 LOC) -- infer execution
- `tools/nika-engine/src/runtime/builtin/router.rs` (560 LOC) -- tool registration
- `tools/nika-event/src/log.rs` (3961 LOC) -- event definitions

## Mission: Complete P-MODEL with parser disambiguation, preset.rs, and nika:cost tool

Session J added the `preset:` field for explicit preset references. This session completes
P-MODEL by: (1) enabling `agent: think` (string) syntax via parser disambiguation from
`agent: { prompt: "..." }` (verb), (2) creating a standalone `preset.rs` module for clean
preset application, (3) adding the `nika:cost` introspection tool, and (4) adding the
`AgentPresetUsed` event.

### Methodology
For EVERY change: read code -> write failing test -> fix -> verify -> commit.
`cargo test --workspace --lib` (always --lib). 1 fix = 1 commit.

---

## PART 1: Parser Disambiguation -- `agent:` as String vs Mapping

### Task 1: Detect `agent:` scalar in parser

**File**: `tools/nika-core/src/ast/raw/parser.rs`
**Problem**: Currently `agent:` always resolves to `RawAgentAction` (the verb). We need:
- `agent: think` (scalar string) -> store as `agent_preset`, NOT a verb
- `agent: { prompt: "..." }` (mapping) -> parse as agent verb (current behavior)

**Fix**: In `parse_action()`, when encountering `agent:` key, check if value is a scalar string.
If scalar: do NOT create a RawAgentAction. Instead return `None` for action, and store the
string in a new `agent_preset` field on `RawTask`.

**File**: `tools/nika-core/src/ast/raw/task.rs` (209 LOC)
Add: `pub agent_preset: Option<Spanned<String>>`

**Tests**:
- `agent: think` parsed as preset, not verb
- `agent: { prompt: "Hello" }` parsed as verb (regression)
- `agent: think` + `infer: "..."` coexist (preset + verb)

**Estimated LOC**: ~35
**Commit**: `feat(parser): disambiguate agent: as preset (string) vs verb (mapping)`

### Task 2: Propagate agent_preset through AST pipeline

**File**: `tools/nika-core/src/ast/analyzed/task.rs` (599 LOC)
Add: `pub agent_preset: Option<String>`

**File**: `tools/nika-core/src/ast/analyzer/` (analyzer module)
Validate: if `agent_preset` is set, name must exist in `workflow.agents`. Emit error if not found.

**File**: `tools/nika-engine/src/ast/workflow.rs` (1021 LOC)
Add: `pub agent_preset: Option<String>` to Task struct.

**File**: `tools/nika-engine/src/ast/lower.rs` (2716 LOC)
Propagate `agent_preset` into lowered Task.

**Tests**: Analyzer validates preset name exists in agents block.

**Estimated LOC**: ~25
**Commit**: `feat(ast): propagate agent_preset through analyzer and lower`

### Task 3: Merge agent_preset with existing preset field

After Session J, tasks have `preset: Option<String>`. After this task, tasks also have
`agent_preset: Option<String>`. These need to be unified:

In the executor, if `agent_preset` is set, use it as the preset (overrides `preset:` field).
Rationale: `agent: think` is the ergonomic form, `preset: think` is the explicit form.
Both resolve through the same `ResolvedAgent` lookup.

**File**: `tools/nika-engine/src/runtime/executor/infer.rs`
**File**: `tools/nika-engine/src/runtime/executor/agent.rs`
**File**: `tools/nika-engine/src/runtime/executor/mod.rs`

```rust
fn effective_preset(task: &Task) -> Option<&str> {
    task.agent_preset.as_deref().or(task.preset.as_deref())
}
```

**Estimated LOC**: ~15
**Commit**: `feat(runtime): unify agent_preset and preset into effective_preset()`

---

## PART 2: Standalone preset.rs Module

### Task 4: Create preset.rs with apply_preset_to_action

**File**: `tools/nika-engine/src/runtime/preset.rs` (NEW, ~160 LOC)

```rust
/// Apply agent preset values to a task action.
///
/// Precedence: task-level > preset > workflow defaults.
/// Handles each verb differently:
/// - Infer: provider, model, temperature, system, max_tokens
/// - Agent: provider, model, temperature, system
/// - Exec: no-op (no LLM fields)
/// - Fetch: no-op (no LLM fields)
/// - Invoke: no-op (no LLM fields)
pub fn apply_preset_to_action(action: &TaskAction, preset: &ResolvedAgent) -> TaskAction {
    match action {
        TaskAction::Infer(params) => {
            let mut params = params.clone();
            if params.provider.is_none() { params.provider = Some(preset.provider.clone()); }
            if params.model.is_none() { params.model = preset.model.clone(); }
            // ... temperature, system, max_tokens
            TaskAction::Infer(params)
        }
        TaskAction::Agent(params) => { /* similar pattern */ }
        other => other.clone(), // No LLM fields to apply
    }
}
```

**Tests** (10 tests):
1. `test_preset_applies_provider_to_infer`
2. `test_preset_applies_model_to_infer`
3. `test_preset_applies_temperature_to_infer`
4. `test_preset_applies_system_to_infer`
5. `test_task_override_wins_over_preset`
6. `test_preset_applies_to_agent_verb`
7. `test_exec_ignores_preset`
8. `test_fetch_ignores_preset`
9. `test_invoke_ignores_preset`
10. `test_missing_preset_returns_error`

**Estimated LOC**: ~160 (module) + ~120 (tests)
**Commit**: `feat(runtime): create preset.rs with apply_preset_to_action`

### Task 5: Wire preset.rs into runner

**File**: `tools/nika-engine/src/runtime/runner.rs` (6524 LOC)
In the task dispatch loop (around line 950-975), before calling `executor.execute()`:
1. Resolve effective preset name
2. Look up `ResolvedAgent` from `self.resolved_assets`
3. Call `apply_preset_to_action()` to produce a modified action
4. Pass modified action to executor

**Estimated LOC**: ~50
**Commit**: `feat(runtime): wire preset resolution into runner task dispatch`

---

## PART 3: AgentPresetUsed Event

### Task 6: Add AgentPresetUsed event

**File**: `tools/nika-event/src/log.rs` (3961 LOC)

```rust
AgentPresetUsed {
    task_id: Arc<str>,
    preset_name: String,
    provider: String,
    model: String,
},
```

Update `task_id()` match arm. Emit in runner when preset is applied.

**File**: `tools/nika-engine/src/display/format_event.rs` (739 LOC)
Add formatting for AgentPresetUsed.

**Tests**: Serialization round-trip, task_id() accessor.

**Estimated LOC**: ~25
**Commit**: `feat(event): add AgentPresetUsed event for preset tracking`

---

## PART 4: nika:cost Introspection Tool

### Task 7: Create CostTool builtin

**File**: `tools/nika-engine/src/runtime/builtin/cost.rs` (NEW, ~80 LOC)

```rust
pub struct CostTool {
    event_log: EventLog,
}
```

The tool iterates `EventLog` events, sums `ProviderResponded` events, returns:
```json
{
    "total_tokens": 15000,
    "total_input_tokens": 10000,
    "total_output_tokens": 5000,
    "total_cost_usd": 0.045,
    "per_model": {
        "claude-sonnet-4-6": { "input_tokens": 8000, "output_tokens": 4000, "cost_usd": 0.039 }
    }
}
```

**Tests**:
1. `test_cost_tool_empty_log` -- returns zeros
2. `test_cost_tool_single_provider_call` -- correct totals
3. `test_cost_tool_multiple_providers` -- per_model breakdown
4. `test_cost_tool_with_cached_tokens` -- cache_read_tokens included
5. `test_cost_tool_name_and_schema` -- tool metadata correct

**Estimated LOC**: ~80 (impl) + ~60 (tests)
**Commit**: `feat(builtin): add nika:cost introspection tool`

### Task 8: Register CostTool in router

**File**: `tools/nika-engine/src/runtime/builtin/mod.rs` (156 LOC)
Add `mod cost; pub use cost::CostTool;`

**File**: `tools/nika-engine/src/runtime/builtin/router.rs` (560 LOC)
Add `with_cost_tool(event_log: EventLog)` method.

**File**: `tools/nika-engine/src/runtime/executor/mod.rs` (623 LOC)
Call `.with_cost_tool(event_log.clone())` after creating `BuiltinToolRouter`.

**Estimated LOC**: ~15
**Commit**: `feat(builtin): register nika:cost in builtin tool router`

---

## PART 5: Backward Compatibility Tests

### Task 9: Regression suite

4 tests ensuring no behavior change for existing workflows:
1. `test_no_agent_field_works_as_before` -- task without agent/preset, current behavior
2. `test_no_provider_chain_works_as_before` -- single provider string
3. `test_workflow_without_agents_block_works` -- no agents: block
4. `test_existing_agent_verb_unaffected` -- `agent: { prompt: "..." }` still works

**Commit**: `test(runtime): backward compatibility for preset and fallback features`

### Task 10: Integration tests (5 tests)

1. `test_full_preset_flow_with_mock_provider` -- workflow with agents:, task with agent: think
2. `test_preset_plus_fallback_chain` -- preset defines primary, task defines fallback
3. `test_preset_system_prompt_injection` -- preset's system prompt appears in provider call
4. `test_multiple_tasks_different_presets` -- two tasks with different presets
5. `test_preset_resolution_event_emitted` -- EventLog contains AgentPresetUsed

**Commit**: `test(runtime): integration tests for complete P-MODEL flow`

---

## E2E Verification Workflows

### test-agent-as-preset.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-agent-as-preset
description: "E2E: agent: think (string) resolves as preset"
provider: mock

agents:
  think:
    system: "You are a deep thinker."
    provider: mock
    model: mock-think
    temperature: 0.3

tasks:
  - id: plan
    agent: think
    infer: "Plan the architecture"
    # Expected: resolved as preset, uses mock/mock-think/0.3

  - id: multi_turn
    agent:
      prompt: "Research the topic"
      max_turns: 3
      tools: []
    # Expected: resolved as agent VERB (multi-turn), not preset
```
**Run**: `nika run test-agent-as-preset.nika.yaml`

### test-cost-tool.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-cost-tool
provider: mock

tasks:
  - id: generate
    infer: "Hello world"

  - id: check_cost
    depends_on: [generate]
    agent:
      prompt: "What is the total cost so far?"
      tools: [nika:cost]
      max_turns: 2
      completion:
        mode: natural
```
**Run**: `nika run test-cost-tool.nika.yaml --provider mock`

---

## After All Fixes

```bash
cd tools && cargo test --workspace --lib       # All pass
cd tools && cargo clippy --workspace -- -D warnings  # Zero warnings
# Parser test: agent: "think" parsed as preset
# Parser test: agent: { prompt: "..." } parsed as verb
# Integration: full workflow with presets + fallback + cost tool
```

---

## Commit Strategy (11 commits)

```
# Part 1: Parser disambiguation
feat(parser): disambiguate agent: as preset (string) vs verb (mapping)
feat(ast): propagate agent_preset through analyzer and lower
feat(runtime): unify agent_preset and preset into effective_preset()

# Part 2: preset.rs
feat(runtime): create preset.rs with apply_preset_to_action
feat(runtime): wire preset resolution into runner task dispatch

# Part 3: Event
feat(event): add AgentPresetUsed event for preset tracking

# Part 4: nika:cost
feat(builtin): add nika:cost introspection tool
feat(builtin): register nika:cost in builtin tool router

# Part 5: Tests
test(runtime): backward compatibility for preset and fallback features
test(runtime): integration tests for complete P-MODEL flow
docs(examples): update agents-preset example with agent: string syntax
```
