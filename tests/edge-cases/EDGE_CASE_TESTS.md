# Nika Edge Case & Stress Test Workflows

Comprehensive test suite for Nika workflow engine covering boundary conditions, error handling, and performance limits. All tests use `provider: mock` for deterministic execution.

---

## Test Suite Overview

| # | Workflow | Focus | Expected Result | Error Code | Files |
|---|----------|-------|-----------------|-----------|-------|
| 1 | Empty workflow | Schema validation | FAIL | NIKA-010 | `edge-case-001-empty-workflow.nika.yaml` |
| 2 | 100-task linear chain | DAG depth stress | PASS | — | `edge-case-002-linear-chain-100.nika.yaml` |
| 3 | Diamond DAG (fan-out 10) | Parallel execution | PASS | — | `edge-case-003-diamond-dag.nika.yaml` |
| 4 | for_each (50 items, concurrency: 5) | Concurrency limits | PASS | — | `edge-case-004-foreach-concurrency.nika.yaml` |
| 5 | for_each (fail_fast: false, 3/10 failures) | Partial results | PASS | — | `edge-case-005-foreach-partial-failure.nika.yaml` |
| 6 | Nested templates & transforms | Template resolution | PASS | — | `edge-case-006-nested-templates.nika.yaml` |
| 7 | All 31 pipe transforms | Transform coverage | PASS | — | `edge-case-007-all-transforms.nika.yaml` |
| 8 | CJK/emoji Unicode | Character encoding | PASS | — | `edge-case-008-cjk-unicode.nika.yaml` |
| 9 | Max retry exhaustion | Error propagation | FAIL | NIKA-026 | `edge-case-009-max-retry-exhaustion.nika.yaml` |
| 10 | Invalid schema in structured | Schema validation | FAIL | NIKA-300 | `edge-case-010-invalid-schema.nika.yaml` |
| 11 | Circular dependency | Cycle detection | FAIL | NIKA-020 | `edge-case-011-circular-dependency.nika.yaml` |
| 12 | Unknown field in task | Parser validation | FAIL | NIKA-010 | `edge-case-012-unknown-field.nika.yaml` |
| 13 | Missing model for infer | Analyzer error | FAIL | NIKA-041 | `edge-case-013-missing-model-infer.nika.yaml` |
| 14 | Env var with default | Null safety | PASS | — | `edge-case-014-env-var-default.nika.yaml` |
| 15 | Provider timeout | Timeout handling | PASS/FAIL* | NIKA-045 | `edge-case-015-timeout-error.nika.yaml` |

*Test 15: Behavior depends on actual provider implementation.

---

## Detailed Test Descriptions

### Test 1: Empty Workflow
**File**: `edge-case-001-empty-workflow.nika.yaml`

**Purpose**: Verify graceful error handling for empty task lists.

**Configuration**:
- 0 tasks
- provider: mock

**Expected Behavior**: FAIL during schema validation
- Should reject workflow with empty `tasks: []`
- Error code: NIKA-010 (Schema validation error)
- Message should indicate minimum task requirement

**Validation Command**:
```bash
nika check edge-case-001-empty-workflow.nika.yaml
```

---

### Test 2: 100-Task Linear Chain
**File**: `edge-case-002-linear-chain-100.nika.yaml`

**Purpose**: Stress test DAG depth and sequential dependency resolution.

**Configuration**:
- 100 tasks: task_001 → task_002 → ... → task_100
- Each task depends on previous one
- provider: mock

**Expected Behavior**: PASS
- All 100 tasks execute sequentially
- Total execution time: ~100ms (mock provider)
- Final result: mock output from task_100
- DAG analysis: depth = 100, width = 1

**Validation Commands**:
```bash
nika check edge-case-002-linear-chain-100.nika.yaml
nika run edge-case-002-linear-chain-100.nika.yaml --dry-run
nika workflow graph edge-case-002-linear-chain-100.nika.yaml
```

---

### Test 3: Diamond DAG (Fan-Out 10, Fan-In)
**File**: `edge-case-003-diamond-dag.nika.yaml`

**Purpose**: Test parallel execution and multi-source data merging.

**Configuration**:
- start (1 task) → branch_001...branch_010 (10 parallel) → final_merge (1 task)
- All 10 branches depend on start
- final_merge depends on all 10 branches
- provider: mock

**Expected Behavior**: PASS
- start executes first
- All 10 branches execute in parallel (scheduler permitting)
- final_merge waits for all 10 to complete
- Receives array with 10 results bound as b1...b10
- Total tasks: 12

**Validation Commands**:
```bash
nika check edge-case-003-diamond-dag.nika.yaml
nika run edge-case-003-diamond-dag.nika.yaml
```

**DAG Visualization**:
```
       start
         |
    /----|----\
    |    |    |
   b001 b002  ... b010
    |    |    |
    \----+----/
         |
    final_merge
```

---

### Test 4: for_each with 50 Items, Concurrency: 5
**File**: `edge-case-004-foreach-concurrency.nika.yaml`

**Purpose**: Test concurrency limit enforcement and loop batching.

**Configuration**:
- inputs.items: 50 distinct items (item_001 to item_050)
- process_items task with for_each:
  - items: {{inputs.items}}
  - concurrency: 5
  - fail_fast: false
- summarize task consumes array result

**Expected Behavior**: PASS
- 50 items processed in 10 batches of 5
- Execution time: ~50ms (10 batches × 5ms per batch)
- Result: array of 50 strings
- summarize task receives array, applies transforms:
  - length: 50
  - first: item_001
  - last: item_050

**Key Assertions**:
- Output is array, not scalar
- Array has exactly 50 elements
- Elements maintain order
- Concurrency never exceeds 5

---

### Test 5: for_each with fail_fast: false, 3/10 Failures
**File**: `edge-case-005-foreach-partial-failure.nika.yaml`

**Purpose**: Test partial result collection when some iterations fail.

**Configuration**:
- inputs.items: [good_1, bad_1, good_2, good_3, bad_2, good_4, good_5, bad_3, good_6, good_7]
- for_each logic:
  - if item contains 'bad': simulate error
  - else: simulate success
- fail_fast: false (collect all results, even with failures)
- concurrency: 2

**Expected Behavior**: PASS
- All 10 items processed despite failures
- Result array contains:
  - 7 success objects
  - 3 error objects (with error status/message)
- analyze_results task counts: {{with.results | length}} = 10
- No early termination

**Key Assertions**:
- fail_fast: false prevents cascade failure
- Partial results collected in array
- Each error has error_status or similar marker
- Downstream task can process mixed results

---

### Test 6: Deeply Nested Templates & Transforms
**File**: `edge-case-006-nested-templates.nika.yaml`

**Purpose**: Test complex path resolution and chained transforms.

**Template Examples**:
```yaml
{{with.nested_data.data.nested.array[0].field | upper | trim}}
{{with.nested_data.data.nested.array[1].id | default(0) | to_number}}
{{with.results | keys | join(', ')}}
```

**Configuration**:
- create_nested_data: generates structured object with array property
- extract_nested_value: resolves nested paths and applies transforms
- complex_pipeline: chains keys → join transforms

**Expected Behavior**: PASS
- Path resolution: data.nested.array[0].field → string value
- Array indexing: [0], [1] work correctly
- Transform chains: upper → trim apply in order
- Default guards prevent null errors
- keys/join parametric transforms work

**Key Assertions**:
- first_field is uppercased and trimmed
- second_item_id defaults to 0 if missing
- All fields accessible via path notation

---

### Test 7: All 31 Pipe Transforms
**File**: `edge-case-007-all-transforms.nika.yaml`

**Purpose**: Comprehensive coverage of all available pipe transforms.

**Transforms Tested**:

**String (7)**:
```yaml
{{text | upper}}              # ABC
{{text | lower}}              # abc
{{text | trim}}               # "abc"
{{text | trim_start}}         # "abc "
{{text | trim_end}}           # " abc"
{{text | length}}             # 5
{{42 | to_string}}            # "42"
```

**Array (9)**:
```yaml
{{numbers | first}}           # 1
{{numbers | last}}            # 50
{{nested | flatten}}          # [1,2,3,4...]
{{numbers | reverse}}         # [50,49...]
{{numbers | sort}}            # [1,2,3...]
{{numbers | unique}}          # [1,2,3...] (no dupes)
{{numbers | compact}}         # [] (remove nulls)
{{obj | keys}}                # ["a","b","c"]
{{obj | values}}              # ["val1","val2"]
```

**Numeric (5)**:
```yaml
{{'42' | to_number}}          # 42
{{3.7 | round}}               # 4
{{-5 | abs}}                  # 5
{{3.2 | ceil}}                # 4
{{3.7 | floor}}               # 3
```

**Type (5)**:
```yaml
{{'true' | to_bool}}          # true
{{obj | to_json}}             # "{...}"
{{json_str | parse_json}}     # {...}
{{value | type_of}}           # "string"
```

**Parametric (3)**:
```yaml
{{nums | join(', ')}}         # "1, 2, 3"
{{text | split(' ')}}         # ["word1","word2"]
{{missing | default('fallback')}}  # "fallback"
```

**Configuration**:
- Tasks for each transform category
- prepare_data generates test data matching all types
- test_*_transforms isolate each category
- finalize task confirms all 31 working

**Expected Behavior**: PASS
- All 31 transforms execute without error
- Output types match expectations
- Chained transforms (a | b | c) work correctly
- Null safety via default() prevents crashes

---

### Test 8: CJK & Unicode Handling
**File**: `edge-case-008-cjk-unicode.nika.yaml`

**Purpose**: Verify multi-byte character encoding in templates and data.

**Test Data**:
```yaml
chinese_text: "你好，这是一个测试工作流。"      # Chinese
japanese_text: "こんにちは、これはテストです。"   # Japanese
korean_text: "안녕하세요, 이것은 테스트입니다."   # Korean
emoji_text: "🚀 Workflow 🎉 Test 🌟"            # Emoji
mixed_text: "Hello 世界 🌍 مرحبا мир"           # Mixed (AR, Cyrillic)
```

**Configuration**:
- inputs with CJK text
- process_cjk: template resolution with CJK
- transform_cjk: upper, length, trim transforms on CJK
- create_cjk_array: structured output with CJK strings
- process_array: array transforms on CJK items

**Expected Behavior**: PASS
- All CJK characters preserved in templates
- length transform counts characters correctly (multi-byte aware)
- upper/lower/trim work on CJK (where applicable)
- JSON serialization handles UTF-8 correctly
- join/split work with CJK delimiters
- No encoding errors or data corruption

**Key Assertions**:
- {{inputs.chinese_text}} renders correctly
- {{cn_length}} = correct character count (not byte count)
- {{with.joined}} preserves all CJK + emoji
- {{with.data | to_json}} valid UTF-8

---

### Test 9: Max Retry Exhaustion
**File**: `edge-case-009-max-retry-exhaustion.nika.yaml`

**Purpose**: Test retry mechanism and error propagation when all retries fail.

**Configuration**:
```yaml
failing_task:
  retry:
    max_attempts: 3
    delay_ms: 100
    backoff: 1.0
  infer: "This will always fail"

downstream_blocked:
  depends_on: [failing_task]
  infer: "Blocked by upstream failure"
```

**Expected Behavior**: FAIL
- failing_task attempts 3 times, all fail
- delay_ms: 100 → total retry time ~300ms
- backoff: 1.0 → no exponential backoff (constant delay)
- downstream_blocked never executes
- Workflow fails with NIKA-026 (dependency chain failed)
- Error message indicates upstream failure

**Key Assertions**:
- Exactly 3 retry attempts made
- Total delay = 100ms × 2 = 200ms (3 attempts, 2 intervals)
- downstream_blocked skipped (not executed)
- Error propagates to downstream

**Validation Commands**:
```bash
nika run edge-case-009-max-retry-exhaustion.nika.yaml 2>&1 | grep "NIKA-026"
```

---

### Test 10: Invalid Schema in Structured Output
**File**: `edge-case-010-invalid-schema.nika.yaml`

**Purpose**: Test schema validation and LLM repair failure handling.

**Configuration**:
```yaml
bad_structured:
  infer: "Generate data"
  structured:
    schema:
      type: object
      properties:
        required_field: {type: string, minLength: 1}
        count: {type: integer, minimum: 1}
      required: [required_field, count]
      additionalProperties: false
    enable_repair: true
    max_retries: 2
```

**Violation Scenario**:
- LLM generates `{}` or `{required_field: ""}` (missing/empty required field)
- JSON validator rejects
- LLM repair attempt 1: still invalid
- LLM repair attempt 2: still invalid
- max_retries exceeded → NIKA-300

**Expected Behavior**: FAIL
- bad_structured fails with NIKA-300
- downstream_never_runs skipped (NIKA-026)
- Error indicates schema violation: missing required_field, invalid count

**Key Assertions**:
- enable_repair: true triggered (LLM repair attempted)
- max_retries: 2 enforced (exactly 2 repair attempts)
- Error includes schema details
- Task marked as Failed (not Skipped)

---

### Test 11: Circular Dependency Detection
**File**: `edge-case-011-circular-dependency.nika.yaml`

**Purpose**: Test DAG cycle detection during parsing.

**Configuration**:
```yaml
task_a:
  depends_on: [task_c]
  infer: "Task A"

task_b:
  depends_on: [task_a]
  infer: "Task B"

task_c:
  depends_on: [task_b]
  infer: "Task C"
```

**Cycle**: A ← C ← B ← A

**Expected Behavior**: FAIL
- Cycle detected during DAG construction
- Error code: NIKA-020 (DAG cycle detected)
- Error occurs before task execution begins
- Message identifies cycle path: task_a → task_c → task_b → task_a

**Validation Commands**:
```bash
nika check edge-case-011-circular-dependency.nika.yaml 2>&1 | grep "NIKA-020"
nika run edge-case-011-circular-dependency.nika.yaml 2>&1 | grep "NIKA-020"
```

---

### Test 12: Unknown Field in Task
**File**: `edge-case-012-unknown-field.nika.yaml`

**Purpose**: Test schema validation for unknown/unsupported fields.

**Configuration**:
```yaml
tasks:
  - id: task_with_unknown_field
    unknown_field: "This field does not exist"
    infer: "Test task"
```

**Expected Behavior**: FAIL
- Parser rejects unknown_field
- Error code: NIKA-010 (Schema validation error)
- Error indicates unknown key at task level
- Workflow doesn't execute

**Validation Command**:
```bash
nika check edge-case-012-unknown-field.nika.yaml 2>&1 | grep "NIKA-010"
```

**Note**: Strictness depends on parser implementation:
- Strict mode: fail on unknown fields (recommended)
- Lenient mode: warn and ignore unknown fields

---

### Test 13: Missing Model for infer
**File**: `edge-case-013-missing-model-infer.nika.yaml`

**Purpose**: Test analyzer validation when required field (model) is missing.

**Configuration**:
```yaml
schema: "nika/workflow@0.12"
provider: mock  # No model: field at workflow level

tasks:
  - id: infer_no_model
    infer:
      prompt: "Generate something"
      temperature: 0.7
      # Missing: model field
```

**Expected Behavior**: FAIL
- Analyzer detects missing model
- Error code: NIKA-041 (template resolution error) or similar
- Or provider validation error if provider: mock doesn't support model-less infer
- Error message: "model required for infer verb"

**Key Constraints**:
- model required at task or workflow level for all infer verbs
- No implicit default

---

### Test 14: Environment Variable with Default
**File**: `edge-case-014-env-var-default.nika.yaml`

**Purpose**: Test null-safe binding with environment variable fallback.

**Configuration**:
```yaml
tasks:
  - id: test_missing_env
    with:
      api_key: "{{$env.NONEXISTENT_API_KEY | default('test-key-123')}}"
      timeout: "{{$env.REQUEST_TIMEOUT | default('30')}}"
      debug: "{{$env.DEBUG_MODE | default('false') | to_bool}}"
```

**Environment**:
- NONEXISTENT_API_KEY: not set
- REQUEST_TIMEOUT: not set
- DEBUG_MODE: not set
- (But HOME, USER likely exist on Unix)

**Expected Behavior**: PASS
- Missing vars use default values
- with.api_key = "test-key-123"
- with.timeout = "30"
- with.debug = false (after to_bool)
- No null reference errors
- Existing vars (HOME, USER) resolve correctly

**Key Assertions**:
- default() guard prevents null errors
- Chained transforms after default work: default(...) | to_bool
- Both missing and existing vars handled

---

### Test 15: Provider Timeout Error
**File**: `edge-case-015-timeout-error.nika.yaml`

**Purpose**: Test timeout handling and error codes across different verbs.

**Configuration**:
```yaml
fetch_with_timeout:
  fetch:
    url: "https://httpbin.org/delay/5"
    timeout: 1  # 1 second, but endpoint delays 5 seconds

exec_with_timeout:
  exec:
    command: "sleep 10"
    timeout: 2  # 2 seconds, but command sleeps 10

invoke_with_timeout:
  invoke:
    tool: "nika:dimensions"
    params: {path: "/tmp/large_file.bin"}
    timeout: 1
```

**Expected Behavior**: Depends on Provider & Network
- **With Real Network**: FAIL with timeout errors
  - fetch_with_timeout: NIKA-045 (fetch error/timeout)
  - exec_with_timeout: NIKA-053 (blocked command) or timeout
  - invoke_with_timeout: likely PASS (tool call is fast)

- **With mock Provider**: PASS (mock doesn't actually timeout)
  - All timeouts ignored
  - mock provider returns instant responses

**Validation**:
For real testing, use `provider: anthropic` (or other) and ensure actual timeout:
```bash
nika run edge-case-015-timeout-error.nika.yaml --provider anthropic
```

---

## Running the Test Suite

### Run All Tests
```bash
cd <project-root>/docs/tests

# Validate all
for f in edge-case-*.nika.yaml; do
  echo "Validating $f..."
  nika check "$f" || echo "EXPECTED FAIL: $f"
done

# Run all (with mock provider)
for f in edge-case-*.nika.yaml; do
  echo "Running $f..."
  nika run "$f" --provider mock || echo "EXPECTED FAIL: $f"
done
```

### Run Individual Tests
```bash
# Test 2: Linear chain stress test
nika run edge-case-002-linear-chain-100.nika.yaml --provider mock

# Test 3: Diamond DAG
nika workflow graph edge-case-003-diamond-dag.nika.yaml
nika run edge-case-003-diamond-dag.nika.yaml

# Test 4: Concurrency
nika run edge-case-004-foreach-concurrency.nika.yaml

# Test 7: All transforms
nika run edge-case-007-all-transforms.nika.yaml

# Test 8: CJK Unicode
nika run edge-case-008-cjk-unicode.nika.yaml

# Test 11: Cycle detection
nika check edge-case-011-circular-dependency.nika.yaml
```

### Validation Helpers
```bash
# Analyze DAG
nika workflow graph edge-case-003-diamond-dag.nika.yaml

# Dry run (validate without executing)
nika run edge-case-004-foreach-concurrency.nika.yaml --dry-run

# Check with strict MCP validation
nika check edge-case-015-timeout-error.nika.yaml --strict

# View error details
nika run edge-case-009-max-retry-exhaustion.nika.yaml --verbose
```

---

## Expected Error Codes Reference

| Code | Test | Meaning |
|------|------|---------|
| NIKA-010 | 1, 12 | Schema validation error (empty workflow, unknown field) |
| NIKA-020 | 11 | DAG cycle detected |
| NIKA-026 | 9, 10 | Dependency chain failed (upstream failed) |
| NIKA-041 | 13 | Template resolution error (missing model) |
| NIKA-045 | 15 | Fetch error / Timeout |
| NIKA-300 | 10 | Structured output validation failed |

---

## Test Design Rationale

### Why Mock Provider?
- **Deterministic**: Same results every run
- **Fast**: ~5-50ms per workflow
- **No API keys**: Runs without credentials
- **Isolation**: No network dependency
- **Cost**: $0

### Why These Edge Cases?
1. **Empty workflow** → Minimum validation
2. **100-task chain** → DAG depth scalability
3. **Diamond DAG** → Parallel execution safety
4. **for_each concurrency** → Resource limits
5. **Partial failures** → Resilience without fail_fast
6. **Nested templates** → Binding complexity
7. **All transforms** → Feature coverage
8. **CJK Unicode** → Internationalization
9. **Retry exhaustion** → Error propagation
10. **Schema violation** → Repair failure
11. **Circular dependency** → Cycle detection
12. **Unknown fields** → Schema strictness
13. **Missing model** → Required field validation
14. **Env var defaults** → Null safety
15. **Provider timeout** → Timeout handling

---

## Future Extensions

### Additional Test Cases
- **Memory stress**: 1000-task workflow
- **Fan-out 100**: Extreme parallelism
- **Nested for_each**: Loop within loop
- **Binary artifacts**: Media pipeline edge cases
- **Vision with timeout**: Image processing timeout
- **Guardrail exhaustion**: Agent guardrail violation
- **Cost limit exceeded**: Budget overflow
- **Custom stop sequences**: Stop token handling

### Automation
- CI/CD integration with GitHub Actions
- Regression test suite
- Performance benchmarking
- Coverage analysis
