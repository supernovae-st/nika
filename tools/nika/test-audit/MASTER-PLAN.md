# Nika Comprehensive Feature Audit - Master Plan

**Version**: v0.22.3
**Date**: 2026-03-09
**Objective**: Identify silent bugs in ALL Nika core features through real execution

## Testing Philosophy

> "Je veux que tu lances vraiment de manière socratique et tu regardes tes résultats
> quand tu lances les workflows et que tu dis pas que parce que ça compile ça marche."

- **NO "it compiles therefore it works"**
- **Real execution with real providers**
- **Examine actual JSON outputs, traces, artifacts**
- **Document ALL discrepancies**

---

## Phase Overview

| Phase | Category | Priority | Status |
|-------|----------|----------|--------|
| 1 | Bindings & Templates | CRITICAL | 🔴 Pending |
| 2 | Built-in Tools | CRITICAL | 🔴 Pending |
| 3 | Control Flow | HIGH | 🔴 Pending |
| 4 | Artifacts & Output Schema | HIGH | 🔴 Pending |
| 5 | Workflow Composition | MEDIUM | 🔴 Pending |
| 6 | MCP Integration | MEDIUM | 🔴 Pending |
| 7 | Provider Features | LOW | 🔴 Pending |

---

## Phase 1: Bindings & Templates (CRITICAL)

### Features to Test
1. **use: block basic** - `use: { alias: task_id }`
2. **$task shorthand** - `use: { alias: $task_id }`
3. **{{use.alias}} templates** - In prompts and params
4. **{{context.files.x}}** - Context file bindings
5. **Nested field access** - `$task.field.nested`
6. **Array indexing** - `$task.items[0]` (v0.22 feature)
7. **Cross-task value passing** - Task A output → Task B input
8. **Default values** - When binding is undefined

### Test Workflows
- `phase1-bindings/01-basic-use.nika.yaml`
- `phase1-bindings/02-shorthand.nika.yaml`
- `phase1-bindings/03-templates.nika.yaml`
- `phase1-bindings/04-context-files.nika.yaml`
- `phase1-bindings/05-nested-access.nika.yaml`
- `phase1-bindings/06-array-indexing.nika.yaml`
- `phase1-bindings/07-cross-task.nika.yaml`
- `phase1-bindings/08-defaults.nika.yaml`

### Expected Bugs
- Template resolution failures
- Nested path access errors
- Array index out of bounds
- Context file loading issues

---

## Phase 2: Built-in Tools (CRITICAL)

### Features to Test
1. **nika:log** - Log levels, message formatting
2. **nika:emit** - Custom events in trace
3. **nika:assert** - Condition validation
4. **nika:sleep** - Duration parsing
5. **nika:prompt** - HITL input (skip in automated tests)
6. **nika:run** - Sub-workflow execution

### Test Workflows
- `phase2-builtins/01-log-levels.nika.yaml`
- `phase2-builtins/02-emit-events.nika.yaml`
- `phase2-builtins/03-assert-conditions.nika.yaml`
- `phase2-builtins/04-sleep-durations.nika.yaml`
- `phase2-builtins/05-run-subworkflow.nika.yaml`

### Expected Bugs
- nika:log not emitting to trace
- nika:emit payload serialization issues
- nika:assert error messages unclear
- nika:run path resolution issues

---

## Phase 3: Control Flow (HIGH)

### Features to Test
1. **for_each** - Array iteration
2. **for_each with $binding** - Dynamic arrays
3. **concurrency** - Parallel execution
4. **fail_fast** - Error propagation
5. **depends_on** - Explicit dependencies
6. **flows:** - DAG edges
7. **retry:** - Retry on failure

### Test Workflows
- `phase3-control-flow/01-for-each-array.nika.yaml`
- `phase3-control-flow/02-for-each-binding.nika.yaml`
- `phase3-control-flow/03-concurrency.nika.yaml`
- `phase3-control-flow/04-fail-fast.nika.yaml`
- `phase3-control-flow/05-depends-on.nika.yaml`
- `phase3-control-flow/06-flows-dag.nika.yaml`
- `phase3-control-flow/07-retry.nika.yaml`

### Expected Bugs
- for_each with bindings not resolving
- Concurrency race conditions
- fail_fast not stopping other tasks
- DAG order not respected

---

## Phase 4: Artifacts & Output Schema (HIGH)

### Features to Test
1. **output:** - Task output declaration
2. **output_schema:** - JSON Schema validation
3. **artifacts:** - File persistence
4. **{{task_id}} in paths** - Template variables
5. **JSON/YAML artifact formats** - Serialization

### Test Workflows
- `phase4-artifacts/01-basic-output.nika.yaml`
- `phase4-artifacts/02-output-schema.nika.yaml`
- `phase4-artifacts/03-artifact-write.nika.yaml`
- `phase4-artifacts/04-path-templates.nika.yaml`
- `phase4-artifacts/05-json-yaml-formats.nika.yaml`

### Expected Bugs
- output_schema not enforced
- Artifact paths not created
- JSON serialization failures
- Template variables not expanded

---

## Phase 5: Workflow Composition (MEDIUM)

### Features to Test
1. **include:** - DAG fusion
2. **include: prefix:** - Task ID namespacing
3. **skills:** - Skill file loading
4. **nika:run** - Sub-workflow execution
5. **Circular include detection** - Error handling

### Test Workflows
- `phase5-composition/01-include-basic.nika.yaml`
- `phase5-composition/02-include-prefix.nika.yaml`
- `phase5-composition/03-skills-loading.nika.yaml`
- `phase5-composition/04-nika-run.nika.yaml`
- `phase5-composition/05-circular-detection.nika.yaml`

### Expected Bugs
- Include path resolution
- Prefix not applied consistently
- Skill merging duplicates
- Circular detection false positives

---

## Phase 6: MCP Integration (MEDIUM)

### Features to Test
1. **invoke:** - MCP tool call
2. **agent: with tools** - Agent tool use
3. **Multiple MCP servers** - Server management
4. **MCP error handling** - Timeout, failures
5. **MCP secrets** - ${spn:provider} resolution

### Test Workflows
- `phase6-mcp/01-invoke-basic.nika.yaml`
- `phase6-mcp/02-agent-tools.nika.yaml`
- `phase6-mcp/03-multi-server.nika.yaml`
- `phase6-mcp/04-error-handling.nika.yaml`
- `phase6-mcp/05-secrets.nika.yaml`

### Expected Bugs
- MCP server startup failures
- Tool parameter serialization
- Timeout not respected
- Secret resolution failures

---

## Phase 7: Provider Features (LOW)

### Features to Test
1. **temperature** - LLM temperature control
2. **max_tokens** - Output length limit
3. **system:** - System prompt
4. **extended_thinking** - Claude thinking capture
5. **model:** override - Per-task model
6. **Streaming** - Real-time token delivery

### Test Workflows
- `phase7-providers/01-temperature.nika.yaml`
- `phase7-providers/02-max-tokens.nika.yaml`
- `phase7-providers/03-system-prompt.nika.yaml`
- `phase7-providers/04-extended-thinking.nika.yaml`
- `phase7-providers/05-model-override.nika.yaml`
- `phase7-providers/06-streaming.nika.yaml`

### Expected Bugs
- Temperature ignored
- max_tokens not enforced
- System prompt not sent
- Thinking not captured

---

## Bug Report Format

```markdown
## BUG-XXX: [Short Title]

**Severity**: CRITICAL | HIGH | MEDIUM | LOW
**Phase**: N
**Feature**: [Feature name]
**Status**: 🔴 Open | 🟡 In Progress | 🟢 Fixed

### Description
[What is broken]

### Expected Behavior
[What should happen]

### Actual Behavior
[What actually happens]

### Reproduction
```yaml
# Minimal workflow to reproduce
```

### Trace Evidence
```json
// Relevant trace entries
```

### Root Cause
[Analysis of why]

### Fix
[Location and change needed]
```

---

## Execution Plan

1. **Phase 1-2 (CRITICAL)**: Run immediately, examine ALL traces
2. **Phase 3-4 (HIGH)**: Run after fixing Phase 1-2 bugs
3. **Phase 5-7 (MEDIUM/LOW)**: Run after core is stable

## Provider Configuration

Using real providers (user confirmed "on paye"):
- **Claude**: claude-sonnet-4-6 (primary)
- **OpenAI**: gpt-4o (secondary)
- **Perplexity MCP**: Real server

## Output Artifacts

All test results saved to:
- `test-audit/traces/` - NDJSON trace files
- `test-audit/results/` - Analysis reports
- `test-audit/BUGS.md` - Bug registry
