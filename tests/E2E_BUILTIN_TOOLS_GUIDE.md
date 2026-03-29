# E2E Tests for All 18 Builtin Nika:* Tools

## Overview

Comprehensive end-to-end test suite covering all 18 builtin nika:* tools organized into 3 categories:

- **Core Tools (7):** Execution control, logging, event emission
- **File Tools (5):** File system operations
- **Introspection Tools (6):** Workflow metrics and analysis

### Tools Tested

| Category | Tool | Purpose | Status |
|----------|------|---------|--------|
| **CORE (7)** | nika:sleep | Pause execution | Implemented |
| | nika:log | Emit log events | Implemented |
| | nika:emit | Custom event emission | Implemented |
| | nika:assert | Condition validation (pass/fail) | Implemented |
| | nika:run | Nested workflow execution | Implemented |
| | nika:complete | Agent completion signal | Implemented |
| | *(prompt)* | *HITL input - skipped in CI* | *N/A* |
| **FILE (5)** | nika:write | Create/overwrite files | Implemented |
| | nika:read | Read files with line numbers | Implemented |
| | nika:edit | Modify file content | Implemented |
| | nika:glob | Find files by pattern | Implemented |
| | nika:grep | Search content by regex | Implemented |
| **INTROSPECTION (6)** | nika:cost | Token/cost metrics | Implemented |
| | nika:records | Workflow record queries | Implemented |
| | nika:dag_info | DAG structure analysis | Implemented |
| | nika:task_status | Task execution metrics | Implemented |
| | nika:threads | Active thread listing | Implemented |
| | nika:orchestrate | Execution round tracking | Implemented |

## Test Files

### 1. `e2e-builtin-tools.nika.yaml` — Unified Test Suite
**Single workflow testing all 18 tools with centralized validation.**

- **Tasks:** 40 tasks
- **Coverage:** All 18 tools (comprehensive)
- **Provider:** mock (CI-compatible, no API calls)
- **Final Step:** Agent-based cross-tool validation

**Run:**
```bash
nika run tests/e2e-builtin-tools.nika.yaml
```

**Structure:**
- Phase 1: Core tools (7 tests)
- Phase 2: File tools (5 tests)
- Phase 3: Introspection tools (6 tests)
- Agent validation: Cross-tool consistency check

### 2. `e2e-core-tools.nika.yaml` — Focused Core Tools Tests
**Deep testing of 7 core tools with multiple scenarios.**

- **Tasks:** 24 tasks
- **Coverage:** sleep, log, emit, assert, run, complete
- **Provider:** mock
- **Validation:** Per-tool + summary report

**Run:**
```bash
nika run tests/e2e-core-tools.nika.yaml
```

**Test Scenarios:**
- nika:sleep: 100ms, 1s
- nika:log: info, debug, warn, error levels
- nika:emit: startup, progress, completion events
- nika:assert: true (pass) + false (fail with error)
- nika:run: simple inline + multi-step workflow
- nika:complete: basic + with metadata

### 3. `e2e-file-tools.nika.yaml` — File Operations Tests
**Comprehensive file tool testing with read/write/edit/glob/grep workflows.**

- **Tasks:** 27 tasks
- **Coverage:** write, read, edit, glob, grep
- **Provider:** mock
- **Test Files:** Multiple .txt and .md files

**Run:**
```bash
nika run tests/e2e-file-tools.nika.yaml
```

**Test Scenarios:**
- nika:write: 3 files (primary, secondary, markdown)
- nika:read: Read all files with line number verification
- nika:edit: 2 sequential edits + read verification after each
- nika:glob: *.txt, *.md, * patterns
- nika:grep: keyword search, pattern matching, cross-file search

**Combined Workflow:**
- Edit + grep chain: Modify file then verify changes with grep

### 4. `e2e-introspection-tools.nika.yaml` — Introspection & Metrics
**Testing workflow introspection and metrics collection.**

- **Tasks:** 30 tasks
- **Coverage:** cost, records, dag_info, task_status, threads, orchestrate
- **Provider:** mock
- **Setup:** 3 setup tasks to generate event log entries

**Run:**
```bash
nika run tests/e2e-introspection-tools.nika.yaml
```

**Test Scenarios:**
- nika:cost: basic query, by_provider, by_model
- nika:records: all, filtered (completed), recent (sorted)
- nika:dag_info: full with deps, nodes only, with metrics
- nika:task_status: Query 3 setup tasks for status/timing
- nika:threads: current, completed filter, sorted by duration
- nika:orchestrate: timeline, rounds, waves, with metrics

**Cross-tool Validation:**
- Agent verifies consistency across all 6 tools
- Checks: task IDs, status matching, timing consistency, cost totals

### 5. `e2e-builtin-tools-master.nika.yaml` — Master Test Orchestrator
**High-level test runner executing all 3 test phases sequentially.**

- **Tasks:** 6 tasks
- **Coverage:** Orchestrates phases 1-3 + final validation
- **Provider:** mock
- **Output:** Comprehensive E2E report

**Run:**
```bash
nika run tests/e2e-builtin-tools-master.nika.yaml
```

**Phases:**
1. **Phase 1 (Core):** Run inline e2e-core-tools workflow
2. **Phase 2 (File):** Run inline e2e-file-tools workflow
3. **Phase 3 (Introspection):** Run inline introspection workflow
4. **Final Validation:** Agent consolidates results
5. **Master Report:** Summary with statistics
6. **Emit Event:** Completion event for monitoring

## Test Execution Patterns

### Pattern 1: Single Tool Test
```yaml
- id: test_sleep
  invoke:
    tool: "nika:sleep"
    params:
      duration: "100ms"

- id: verify_sleep
  depends_on: [test_sleep]
  with:
    result: $test_sleep
  infer:
    prompt: "Verify sleep response contains slept_for_ms field"
```

### Pattern 2: Tool Chain (Read/Edit/Verify)
```yaml
- id: write_file
  invoke:
    tool: "nika:write"
    params:
      file_path: "/tmp/test.txt"
      content: "original"

- id: edit_file
  depends_on: [write_file]
  invoke:
    tool: "nika:edit"
    params:
      file_path: "/tmp/test.txt"
      old_string: "original"
      new_string: "modified"

- id: verify_edit
  depends_on: [edit_file]
  invoke:
    tool: "nika:read"
    params:
      file_path: "/tmp/test.txt"
```

### Pattern 3: Multi-Tool Consistency Check
```yaml
- id: query_multiple_tools
  depends_on: [setup_task]
  parallel_invoke:
    - tool: "nika:cost"
    - tool: "nika:records"
    - tool: "nika:task_status"

- id: validate_consistency
  depends_on: [query_multiple_tools]
  agent:
    prompt: "Verify all introspection results are consistent"
```

## Running Tests in CI/CD

### Quick Smoke Test (< 30s)
```bash
nika check tests/e2e-core-tools.nika.yaml
```

### Full E2E Suite (< 2m)
```bash
nika run tests/e2e-builtin-tools-master.nika.yaml --provider mock
```

### Individual Category Tests
```bash
# Core tools only
nika run tests/e2e-core-tools.nika.yaml

# File tools only
nika run tests/e2e-file-tools.nika.yaml

# Introspection tools only
nika run tests/e2e-introspection-tools.nika.yaml
```

### With Live Logging
```bash
nika run tests/e2e-builtin-tools.nika.yaml --log debug
```

## Expected Results

### Core Tools Expected Output
```
✓ nika:sleep - returned slept_for_ms >= duration
✓ nika:log - all 4 log levels emitted successfully
✓ nika:emit - custom events recorded in event log
✓ nika:assert - pass case succeeded, fail case triggered error
✓ nika:run - nested workflows executed with results
✓ nika:complete - completion signal recognized
```

### File Tools Expected Output
```
✓ nika:write - 3 files created with content
✓ nika:read - all files read with line numbers
✓ nika:edit - sequential edits applied correctly
✓ nika:glob - pattern matching returned correct file lists
✓ nika:grep - regex searches found expected matches
```

### Introspection Tools Expected Output
```
✓ nika:cost - total_tokens, input_tokens, output_tokens, cost_usd
✓ nika:records - array of task records with status/timestamp
✓ nika:dag_info - nodes array with dependencies
✓ nika:task_status - duration_ms, tokens_used, status
✓ nika:threads - thread_id, elapsed_ms, task_id, status
✓ nika:orchestrate - timeline/rounds/waves with metrics
```

## Tool-Specific Test Details

### nika:sleep
- **Tests:** Duration verification (100ms, 1s)
- **Validation:** Response contains `slept_for_ms` field
- **Expected:** Actual sleep >= requested duration

### nika:log
- **Tests:** All log levels (info, debug, warn, error)
- **Validation:** Each log returns `{logged: true}`
- **Expected:** Events recorded in EventLog

### nika:emit
- **Tests:** Custom event with payload
- **Validation:** Response contains `{emitted: true}`
- **Expected:** Event recorded with provided metadata

### nika:assert
- **Tests:** True condition (pass), False condition (fail)
- **Validation:** Pass returns `{passed: true}`, fail throws error
- **Expected:** Correct pass/fail behavior

### nika:run
- **Tests:** Inline workflow execution (simple + multi-step)
- **Validation:** Nested workflow completes successfully
- **Expected:** Nested task results available

### nika:complete
- **Tests:** Completion signal with summary/context
- **Validation:** Signal recognized by runtime
- **Expected:** Agent stop signal triggered

### nika:write
- **Tests:** Create multiple files with various content
- **Validation:** Files exist with correct content
- **Expected:** File paths returned, content persisted

### nika:read
- **Tests:** Read files with line numbers
- **Validation:** Content matches original + line numbers present
- **Expected:** Line-numbered output for all files

### nika:edit
- **Tests:** Sequential edits on same file
- **Validation:** Changes applied correctly
- **Expected:** File content modified as specified

### nika:glob
- **Tests:** Pattern matching (*.txt, *.md, *)
- **Validation:** Correct files returned
- **Expected:** File lists match glob patterns

### nika:grep
- **Tests:** Regex search, cross-file search
- **Validation:** Matches found with file/line reference
- **Expected:** Correct lines returned with context

### nika:cost
- **Tests:** Basic query, by provider, by model
- **Validation:** Metrics include tokens and USD cost
- **Expected:** Aggregated cost data with breakdowns

### nika:records
- **Tests:** All records, filtered, sorted
- **Validation:** Correct record structure
- **Expected:** Task record arrays with metadata

### nika:dag_info
- **Tests:** Full DAG, nodes only, with metrics
- **Validation:** Dependency graph structure correct
- **Expected:** Task dependencies and execution order

### nika:task_status
- **Tests:** Status query for multiple tasks
- **Validation:** Duration and token counts present
- **Expected:** Per-task execution metrics

### nika:threads
- **Tests:** Current threads, completed filter, sorted
- **Validation:** Thread IDs and status correct
- **Expected:** Active/completed thread list

### nika:orchestrate
- **Tests:** Timeline, rounds, waves formats
- **Validation:** Execution coordination visible
- **Expected:** Orchestration data with metrics

## Validation Strategy

Each test file follows this 3-step validation pattern:

1. **Tool Execution:** Call builtin tool with parameters
2. **Result Verification:** LLM validates response structure (prompt-based)
3. **Summary:** Final report of PASS/FAIL for each tool

### Agent-Based Validation
The master test and some individual tests use agents for nuanced validation:
- Consistency checks across multiple tools
- Structured data validation
- Complex result interpretation

### Regression Detection
Tests are designed to catch:
- Missing response fields
- Invalid JSON structure
- Incorrect tool parameter handling
- Tool execution errors
- Cross-tool data inconsistencies

## Troubleshooting

### Test Fails: "Unknown builtin tool"
- Ensure nika-engine includes tool implementation
- Check tool name spelling (lowercase, nika: prefix)

### Test Fails: "File not found"
- File tools need ToolContext with working directory
- Ensure test directory is writable (/tmp on Unix)

### Test Fails: "Assertion error on false condition"
- This is **expected** for nika:assert false tests
- Use `retry: { max_attempts: 1 }` to handle expected errors

### Test Fails: "Template binding error"
- All `with:` aliases must be declared before use in prompts
- Use exact task ID names in $ references

### Tool Returns Empty/Null
- Mock provider returns deterministic responses
- For real data, use actual API providers
- Check EventLog for task completion status

## Integration with CI/CD

### GitHub Actions Example
```yaml
name: E2E Builtin Tools

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: moonrepo/setup-rust@v1
      - run: nika check tests/e2e-*.nika.yaml
      - run: nika run tests/e2e-builtin-tools-master.nika.yaml
```

## Performance Characteristics

| Test Suite | Duration | Tasks | Provider |
|-----------|----------|-------|----------|
| e2e-core-tools | ~30s | 24 | mock |
| e2e-file-tools | ~30s | 27 | mock |
| e2e-introspection-tools | ~40s | 30 | mock |
| e2e-builtin-tools | ~50s | 40 | mock |
| e2e-builtin-tools-master | ~120s | 6 | mock |

Mock provider skips actual LLM calls, making tests deterministic and fast.

## Future Extensions

1. **Media Tools (N):** Add tests for thumbnail, convert, strip, etc.
2. **Custom Tools:** Template for testing custom MCP tool integration
3. **Error Path Tests:** Negative test cases (invalid params, missing files, etc.)
4. **Performance Benchmarks:** Measure tool execution timing
5. **Stress Tests:** High concurrency, large file operations
6. **Integration Tests:** Cross-tool workflows (e.g., write→glob→grep→read)

## Related Documentation

- [`/tools/nika-engine/src/runtime/builtin/mod.rs`](../tools/nika-engine/src/runtime/builtin/mod.rs) — Builtin tool implementations
- [`/tools/nika-engine/src/runtime/builtin/router.rs`](../tools/nika-engine/src/runtime/builtin/router.rs) — Tool dispatch and registration
- [`CLAUDE.md`](../CLAUDE.md) — Nika workflow syntax and schema reference
