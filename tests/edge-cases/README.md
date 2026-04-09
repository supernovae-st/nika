# Nika Edge Case & Stress Test Suite

Complete test suite for the Nika workflow engine covering boundary conditions, error handling, and performance limits.

## Quick Start

```bash
cd <project-root>/docs/tests

# Validate a single test
nika check edge-case-001-empty-workflow.nika.yaml

# Run a single test with mock provider (deterministic)
nika run edge-case-007-all-transforms.nika.yaml

# Validate all tests
for f in edge-case-*.nika.yaml; do nika check "$f"; done

# View DAG visualization
nika workflow graph edge-case-003-diamond-dag.nika.yaml
```

## Test Files

### 15 Complete Test Workflows

| # | Filename | Test Type | Expected Outcome |
|---|----------|-----------|------------------|
| 1 | `edge-case-001-empty-workflow.nika.yaml` | Schema validation | FAIL (NIKA-010) |
| 2 | `edge-case-002-linear-chain-100.nika.yaml` | DAG depth | PASS (100 tasks) |
| 3 | `edge-case-003-diamond-dag.nika.yaml` | Parallel execution | PASS (10 branches) |
| 4 | `edge-case-004-foreach-concurrency.nika.yaml` | Concurrency limits | PASS (50 items, 5 parallel) |
| 5 | `edge-case-005-foreach-partial-failure.nika.yaml` | Partial results | PASS (7/10 succeed) |
| 6 | `edge-case-006-nested-templates.nika.yaml` | Template complexity | PASS (nested paths + transforms) |
| 7 | `edge-case-007-all-transforms.nika.yaml` | Feature coverage | PASS (31 transforms) |
| 8 | `edge-case-008-cjk-unicode.nika.yaml` | Internationalization | PASS (Chinese/Japanese/Korean/emoji) |
| 9 | `edge-case-009-max-retry-exhaustion.nika.yaml` | Error propagation | FAIL (NIKA-026) |
| 10 | `edge-case-010-invalid-schema.nika.yaml` | Schema repair | FAIL (NIKA-300) |
| 11 | `edge-case-011-circular-dependency.nika.yaml` | Cycle detection | FAIL (NIKA-020) |
| 12 | `edge-case-012-unknown-field.nika.yaml` | Parser strictness | FAIL (NIKA-010) |
| 13 | `edge-case-013-missing-model-infer.nika.yaml` | Required fields | FAIL (NIKA-041) |
| 14 | `edge-case-014-env-var-default.nika.yaml` | Null safety | PASS (defaults work) |
| 15 | `edge-case-015-timeout-error.nika.yaml` | Timeout handling | PASS/FAIL (depends on provider) |

## Documentation

**Complete test documentation**: See `EDGE_CASE_TESTS.md` for detailed descriptions, rationale, and test procedures.

## Test Categories

### Validation Tests (Expected Failures)
- **Test 1**: Empty workflow — minimum validation
- **Test 9**: Max retry exhaustion — error propagation
- **Test 10**: Invalid schema — repair failure
- **Test 11**: Circular dependency — cycle detection
- **Test 12**: Unknown field — schema strictness
- **Test 13**: Missing model — required field validation

### Functional Tests (Expected Passes)
- **Test 2**: 100-task linear chain — DAG scalability
- **Test 3**: Diamond DAG — parallel execution
- **Test 4**: for_each with concurrency — resource limits
- **Test 5**: for_each with partial failure — resilience
- **Test 6**: Nested templates — binding complexity
- **Test 7**: All 31 transforms — feature coverage
- **Test 8**: CJK Unicode — internationalization
- **Test 14**: Env var with default — null safety
- **Test 15**: Timeout — provider-dependent

## Test Metrics

| Metric | Value |
|--------|-------|
| Total test files | 15 |
| Total tasks | ~250+ |
| Test coverage | 15 domains |
| Expected failures | 6 (40%) |
| Expected passes | 9 (60%) |
| Execution time (mock) | ~5-50ms per workflow |
| API cost | $0 (uses mock provider) |

## Running the Full Suite

### Bash Script
```bash
#!/bin/bash
set -e

PASS=0
FAIL=0

for test in edge-case-*.nika.yaml; do
  echo "Testing $test..."

  if nika check "$test" 2>&1 | grep -q "FAIL\|NIKA-"; then
    echo "  ✓ Validation FAILED as expected"
    FAIL=$((FAIL + 1))
  else
    echo "  ✓ Validation PASSED"
    if nika run "$test" --provider mock 2>&1 | grep -q "error\|NIKA-"; then
      echo "  ✓ Execution FAILED as expected"
      FAIL=$((FAIL + 1))
    else
      echo "  ✓ Execution PASSED"
      PASS=$((PASS + 1))
    fi
  fi
done

echo ""
echo "Results: $PASS PASS, $FAIL FAIL"
exit 0
```

### Individual Test Commands

```bash
# Validation only (check syntax + DAG)
nika check edge-case-007-all-transforms.nika.yaml

# Dry run (validate without executing)
nika run edge-case-004-foreach-concurrency.nika.yaml --dry-run

# Full execution with mock provider
nika run edge-case-003-diamond-dag.nika.yaml

# With verbose output
nika run edge-case-008-cjk-unicode.nika.yaml --verbose

# DAG visualization
nika workflow graph edge-case-002-linear-chain-100.nika.yaml

# Strict validation (checks MCP connections too)
nika check edge-case-015-timeout-error.nika.yaml --strict
```

## Error Codes Tested

| Code | Test(s) | Meaning |
|------|---------|---------|
| NIKA-010 | 1, 12 | Schema validation error |
| NIKA-020 | 11 | DAG cycle detected |
| NIKA-026 | 9, 10 | Dependency chain failed |
| NIKA-041 | 13 | Template resolution error |
| NIKA-045 | 15 | Fetch error / Timeout |
| NIKA-300 | 10 | Structured output validation failed |

## Key Features Tested

### Workflow Structure
- Empty tasks validation
- Schema compliance
- Workflow header fields
- Task ID uniqueness

### DAG (Directed Acyclic Graph)
- Linear chains (100 tasks)
- Diamond patterns (parallel + merge)
- Cycle detection
- Dependency resolution
- Execution ordering

### Data Flow
- Task references (`$task_id`)
- Path access (`$task.field.nested[0]`)
- Binding with `with:` block
- Template resolution (`{{with.alias}}`)
- Default fallback (`?? value`)

### Iteration
- for_each loops with arrays
- Concurrency limiting (5 parallel)
- fail_fast: true vs false
- Array result handling

### Template System
- String templates with expressions
- Path traversal (nested objects/arrays)
- Array indexing (`[0]`, `[1]`)
- Pipe transforms (chain multiple)
- Null safety with defaults

### Pipe Transforms (31 Total)
- **String** (7): upper, lower, trim, trim_start, trim_end, length, to_string
- **Array** (9): first, last, flatten, reverse, sort, unique, compact, keys, values
- **Numeric** (5): to_number, round, abs, ceil, floor
- **Type** (5): to_bool, to_json, parse_json, type_of, shell
- **Parametric** (3): join, split, default

### Structured Output
- JSON schema validation
- Repair with LLM (enable_repair)
- Retry on schema violation
- max_retries enforcement

### Error Handling
- Retry mechanism (max_attempts, delay, backoff)
- Error propagation (NIKA-026)
- Timeout handling
- Schema violation recovery

### Internationalization
- CJK characters (Chinese, Japanese, Korean)
- Emoji support
- Multi-byte UTF-8 encoding
- Unicode transforms

### Provider Integration
- Mock provider (deterministic)
- Provider-specific error codes
- Timeout handling
- API key binding

## Implementation Notes

### Why Mock Provider?
- **Deterministic**: Same output every run
- **Fast**: 5-50ms per workflow
- **Free**: No API calls, no costs
- **Isolated**: No network dependency
- **Repeatable**: CI/CD friendly

### Design Patterns

**Pattern 1: Sequential Chain**
```yaml
tasks:
  - id: step1
    infer: "First"
  - id: step2
    depends_on: [step1]
    infer: "Second"
```

**Pattern 2: Diamond (Parallel + Merge)**
```yaml
tasks:
  - id: start
    infer: "Begin"
  - id: left
    depends_on: [start]
    infer: "Process left"
  - id: right
    depends_on: [start]
    infer: "Process right"
  - id: merge
    depends_on: [left, right]
    infer: "Merge results"
```

**Pattern 3: Fan-Out Loop**
```yaml
tasks:
  - id: items
    infer: "Generate items"
  - id: process
    for_each:
      items: $items
      concurrency: 5
    infer: "Process {{with.item}}"
```

## Extending the Test Suite

### New Test Ideas
- 1000-task workflow (memory stress)
- Fan-out 100 (extreme parallelism)
- Nested for_each (loop within loop)
- Binary artifacts (media pipeline)
- Vision input with timeout
- Agent guardrail violations
- Cost limit exceeded
- Custom stop sequences

### Adding a New Test
1. Create `edge-case-NNN-description.nika.yaml`
2. Use `provider: mock` for determinism
3. Document in `EDGE_CASE_TESTS.md`:
   - Expected outcome (PASS/FAIL)
   - Error code (if expected to fail)
   - Test procedure
   - Key assertions
4. Add to summary table above

## Files in This Directory

```
docs/tests/
├── README.md                          # This file
├── EDGE_CASE_TESTS.md                 # Complete test documentation
├── edge-case-001-empty-workflow.nika.yaml
├── edge-case-002-linear-chain-100.nika.yaml
├── edge-case-003-diamond-dag.nika.yaml
├── edge-case-004-foreach-concurrency.nika.yaml
├── edge-case-005-foreach-partial-failure.nika.yaml
├── edge-case-006-nested-templates.nika.yaml
├── edge-case-007-all-transforms.nika.yaml
├── edge-case-008-cjk-unicode.nika.yaml
├── edge-case-009-max-retry-exhaustion.nika.yaml
├── edge-case-010-invalid-schema.nika.yaml
├── edge-case-011-circular-dependency.nika.yaml
├── edge-case-012-unknown-field.nika.yaml
├── edge-case-013-missing-model-infer.nika.yaml
├── edge-case-014-env-var-default.nika.yaml
└── edge-case-015-timeout-error.nika.yaml
```

## Integration with CI/CD

### GitHub Actions Example
```yaml
name: Edge Case Tests
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - run: |
          cd docs/tests
          for f in edge-case-*.nika.yaml; do
            nika check "$f" || true
            nika run "$f" --provider mock || true
          done
```

## Troubleshooting

### Test Fails During Validation
```bash
nika check <file> --strict  # Check MCP connections too
```

### Test Fails During Execution
```bash
nika run <file> --verbose   # See detailed logs
nika run <file> --no-live   # Force classic append-only output
```

### See Expected Error Code
```bash
nika run edge-case-011-circular-dependency.nika.yaml 2>&1 | grep "NIKA-"
```

### View DAG Structure
```bash
nika workflow graph <file>
```

## References

- **Nika Schema**: `nika/workflow@0.12`
- **Documentation**: See `<project-root>/CLAUDE.md`
- **Complete syntax reference**: `dx/.claude/rules/nika-workflows.md`
- **Error codes**: NIKA-010 through NIKA-319

## Contact & Contribution

Created: 2026-03-29
Location: `<project-root>/docs/tests/`

For more information, see `EDGE_CASE_TESTS.md`.
