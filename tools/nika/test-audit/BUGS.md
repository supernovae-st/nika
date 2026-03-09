# Nika Core Audit - Bug Report

**Date:** 2026-03-09
**Version Tested:** v0.22.3
**Auditor:** Claude Opus 4.5 (automated audit)
**Completed:** 2026-03-09 (All 7 phases complete)

---

## Executive Summary

| Metric | Value |
|--------|-------|
| **Phases Tested** | 7/7 |
| **Test Workflows Run** | 23 |
| **Critical Bugs Found** | 3 (BUG-003, BUG-004, BUG-005) |
| **Bugs Fixed Prior** | 2 (BUG-001, BUG-002) |
| **Features Working** | Bindings, Built-ins, Artifacts, MCP, Providers |
| **Features Broken** | for_each with bindings, implicit depends_on, DAG terminal selection |

---

## Fixed Bugs (v0.22.1-v0.22.3)

### BUG-001: `agent:` verb broken with OpenAI provider [FIXED v0.22.1]

**Severity:** 🔴 CRITICAL
**Status:** 🟢 FIXED
**Location:** `src/runtime/builtin/complete.rs:177`

**Problem:**
The `nika_complete` tool schema has `additionalProperties: true` on the `metadata` object, but OpenAI's function calling API requires `additionalProperties: false` on all nested objects.

**Fix:** Changed to `"additionalProperties": false`

---

### BUG-002: `for_each` binding expression doesn't parse JSON strings [FIXED v0.22.2]

**Severity:** 🔴 HIGH
**Status:** 🟢 FIXED
**Location:** `src/runtime/runner.rs:930-948`

**Problem:**
When a task outputs a JSON array as a string, `for_each` couldn't iterate because `value.as_array()` returns `None` for string values.

**Fix:** Added JSON string parsing fallback in for_each resolution.

---

## Open Bugs (Found in v0.22.3 Audit)

### BUG-003: `use:` block does NOT create implicit `depends_on`

**Severity:** 🔴 HIGH
**Phase:** 1 - Bindings
**Feature:** use: bindings
**Status:** 🔴 OPEN
**Location:** `src/dag/builder.rs` and `src/dag/validation.rs`

**Problem:**
The `use:` block only declares a data binding but does NOT automatically create a DAG edge (dependency) to the referenced task. Users must add explicit `depends_on: [task_id]`.

**Expected Behavior:**
```yaml
tasks:
  - id: step1
    infer: "..."
  - id: step2
    use:
      data: step1   # Should implicitly depend on step1
    infer: "{{use.data}}"
```
`step2` should automatically wait for `step1` to complete.

**Actual Behavior:**
```
Error: [NIKA-081] use.data.from='step1' is not upstream of task 'step2'
```

**Workaround:**
Add explicit `depends_on`:
```yaml
  - id: step2
    depends_on: [step1]  # REQUIRED!
    use:
      data: step1
    infer: "{{use.data}}"
```

**Fix Location:** `src/dag/builder.rs` - auto-add edges for `use:` references

**Impact:** Every workflow using `use:` without explicit `depends_on` will fail.

---

### BUG-004: Workflow final output picks wrong terminal task

**Severity:** 🔴 HIGH
**Phase:** 1 - Bindings
**Feature:** Workflow output
**Status:** 🔴 OPEN
**Location:** `src/runtime/runner.rs` (final output selection)

**Problem:**
When a workflow has multiple terminal nodes (tasks with no downstream dependencies), the workflow picks an arbitrary one for the final output instead of the "last" completed task.

**Reproduction:**
```yaml
tasks:
  - id: source
    infer: "Return: SuperNovae"
  - id: branch_a
    depends_on: [source]
    use: { data: source }
    infer: "Branch A: {{use.data}}"
  - id: branch_b
    depends_on: [source]
    use: { data: source }
    infer: "Branch B: {{use.data}}"
  - id: final
    depends_on: [branch_b]
    use: { data: branch_b }
    infer: "Final: {{use.data}}"
```

**Expected:** `final_output` = "Final: SuperNovae" (from task `final`)
**Actual:** `final_output` = "Branch A: SuperNovae" (from task `branch_a`)

**Fix Options:**
1. Pick terminal node with highest topological sort order
2. Return a map of all terminal node outputs
3. Require explicit `workflow.output: task_id` declaration

**Impact:** Workflows with branching DAGs return incorrect/unpredictable final outputs.

---

### BUG-005: for_each with binding expression ($items) fails - as: alias not resolved

**Severity:** 🔴 HIGH
**Phase:** 3 - Control Flow
**Feature:** for_each with binding
**Status:** 🔴 OPEN
**Location:** `src/runtime/runner.rs` (for_each expansion)

**Problem:**
When using `for_each: $items` with a binding expression that references another task's output, the loop variable defined by `as:` is not resolved in the task's prompt.

**Reproduction:**
```yaml
tasks:
  - id: generate_items
    infer: 'Return EXACTLY: ["red", "green", "blue"]'

  - id: process_items
    depends_on: [generate_items]
    use:
      items: generate_items
    for_each: $items
    as: color
    infer: "Process color: {{use.color}}"  # FAILS
```

**Error:**
```
[NIKA-041] Template error in 'color': Alias(es) not resolved.
```

**Root Cause:**
The for_each expansion resolves the array correctly but fails to inject the `as:` variable into the task's use block for template resolution.

**Fix:** Ensure `as` variable is added to task's bindings before template expansion.

**Impact:** Dynamic for_each workflows (common pattern) are broken.

---

## Phase Testing Results

### Phase 1: Bindings & Templates ⚠️

| Test | Result | Notes |
|------|--------|-------|
| Basic use: binding | PASS | With explicit depends_on |
| Template resolution | PASS | {{use.alias}} works |
| Multi-task chain | PASS | 4-task chain with bindings |
| Parallel bindings | PASS | Multiple tasks use same source |
| Shorthand infer | PASS | `infer: "prompt"` works |
| Branching DAG | FAIL | BUG-004: Wrong terminal selected |

**Bugs Found:** BUG-003 (implicit depends_on), BUG-004 (terminal selection)

---

### Phase 2: Built-in Tools ✅

| Tool | Result | Notes |
|------|--------|-------|
| nika:log | PASS | All log levels work |
| nika:emit | PASS | Custom events captured |
| nika:assert | PASS | Passes/fails correctly |
| nika:sleep | PASS | Delays execution |
| nika:run | PASS | Sub-workflow execution |

**Bugs Found:** None

---

### Phase 3: Control Flow ⚠️

| Test | Result | Notes |
|------|--------|-------|
| depends_on | PASS | Tasks execute in order |
| depends_on multiple | PASS | Multiple deps work |
| for_each literal array | PASS | `["a","b","c"]` works |
| for_each binding | FAIL | BUG-005: as: not resolved |

**Bugs Found:** BUG-005 (for_each binding)

---

### Phase 4: Artifacts & Output Schema ✅

| Test | Result | Notes |
|------|--------|-------|
| Basic artifact write | PASS | File created |
| Template paths | PASS | {{task_id}}, {{date}} work |
| Output schema | PASS | JSON validation works |
| Retry on invalid JSON | PASS | LLM retries with schema |

**Bugs Found:** None

---

### Phase 5: Workflow Composition ✅

| Test | Result | Notes |
|------|--------|-------|
| include: basic | PASS | Tasks merged correctly |
| include: with prefix | PASS | setup_ prefix applied |
| nika:run subworkflow | PASS | Executes and returns |

**Bugs Found:** None (path must be relative to workflow file)

---

### Phase 6: MCP Integration ✅

| Test | Result | Notes |
|------|--------|-------|
| Perplexity search | PASS | Real API call works |
| MCP tool invocation | PASS | invoke: verb works |

**Bugs Found:** None (correct package: @perplexity-ai/mcp-server)

---

### Phase 7: Provider Features ✅

| Test | Result | Notes |
|------|--------|-------|
| temperature | PASS | Different outputs at 0.0 vs 1.0 |
| max_tokens | PASS | Output truncated correctly |
| system prompt | PASS | Pirate persona applied |
| model override | PASS | gpt-4o-mini and gpt-4o both work |

**Bugs Found:** None

---

## Minor Issues

### ISSUE-001: `.output` suffix not supported

**Severity:** 🟡 LOW
**Status:** DOCUMENTED

Using `task.output` in `use:` block fails with NIKA-052. Use `task` or `$task` directly.

---

### ISSUE-002: All verb outputs stored as strings

**Severity:** 🟡 INFO
**Status:** BY DESIGN

`exec:`, `fetch:`, and `infer:` store output as JSON strings. Nested path access doesn't work unless explicitly structured.

---

## Test Workflow Files Created

```
test-audit/
├── BUGS.md (this file)
├── MASTER-PLAN.md
├── phase1-bindings/
│   ├── 01-basic-use-openai.nika.yaml
│   ├── 02-chain-binding.nika.yaml
│   ├── 03-parallel-binding.nika.yaml
│   ├── 04-nested-binding.nika.yaml
│   ├── 05-branching-dag.nika.yaml
│   └── 06-shorthand-infer.nika.yaml
├── phase2-builtins/
│   ├── 01-log-levels-fixed.nika.yaml
│   ├── 02-emit-events-fixed.nika.yaml
│   ├── 03-assert-fixed.nika.yaml
│   ├── 04-sleep-fixed.nika.yaml
│   └── 05-run-subworkflow-fixed.nika.yaml
├── phase3-control-flow/
│   ├── 01-depends-on-single.nika.yaml
│   ├── 02-depends-on-multiple.nika.yaml
│   ├── 03-for-each-binding.nika.yaml
│   └── 04-for-each-literal.nika.yaml
├── phase4-artifacts/
│   ├── 01-basic-artifact.nika.yaml
│   └── 02-output-schema.nika.yaml
├── phase5-composition/
│   ├── 01-include-basic.nika.yaml
│   ├── 01-include-fixed.nika.yaml
│   └── partial-setup.nika.yaml
├── phase6-mcp/
│   ├── 01-perplexity-fixed.nika.yaml
│   └── 01-perplexity-search.nika.yaml
└── phase7-providers/
    ├── 01-temperature.nika.yaml
    ├── 02-max-tokens.nika.yaml
    ├── 03-system-prompt.nika.yaml
    └── 04-model-override.nika.yaml
```

---

## Recommendations

### Immediate Fixes (v0.22.4)

1. **BUG-003:** Add implicit depends_on for use: references
   - Location: `src/dag/builder.rs`
   - Impact: All workflows more intuitive

2. **BUG-005:** Inject as: variable into task bindings during for_each
   - Location: `src/runtime/runner.rs`
   - Impact: Dynamic iteration patterns work

### Medium Priority (v0.23.0)

3. **BUG-004:** Define deterministic terminal task selection
   - Option A: Highest topological order wins
   - Option B: Add `workflow.output: task_id` field

### Documentation Updates

4. Add explicit warning about `depends_on` requirement with `use:`
5. Document that `for_each: $binding` is currently broken
6. Add working examples for all control flow patterns

---

## Conclusion

Nika v0.22.3 has **solid core functionality** but **three significant bugs** in the data flow layer:

| Bug | Severity | User Impact |
|-----|----------|-------------|
| BUG-003 | HIGH | Every new user will hit this |
| BUG-004 | HIGH | Branching workflows unpredictable |
| BUG-005 | HIGH | Common pattern completely broken |

The **good news**: Built-in tools, MCP integration, artifacts, provider features, and basic execution all work correctly. The bugs are concentrated in the binding/DAG layer and can be fixed with targeted changes.

**Recommended Action:** Fix BUG-003 and BUG-005 before any marketing push, as they represent the most common developer friction points.
