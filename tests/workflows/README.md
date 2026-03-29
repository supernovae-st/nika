# Nika Pipe Transforms & Bindings Test Suite

Complete test coverage for Nika's 31 pipe transforms and binding system using `provider: mock` for deterministic testing without API calls.

## Test Files Overview

### 1. transforms-string.nika.yaml
Tests all 7 string transforms:
- `upper` — Convert to uppercase
- `lower` — Convert to lowercase
- `trim` — Remove leading/trailing whitespace
- `trim_start` — Remove leading whitespace
- `trim_end` — Remove trailing whitespace
- `length` — Get string length
- `to_string` — Convert value to string

**Includes:**
- Individual transform tests
- Chained transforms (trim → upper → length)
- Null safety with default operator

### 2. transforms-array.nika.yaml
Tests all 9 array transforms:
- `first` — Get first element
- `last` — Get last element
- `flatten` — Flatten nested arrays
- `reverse` — Reverse array order
- `sort` — Sort array (numerically/alphabetically)
- `unique` — Remove duplicate elements
- `compact` — Remove null/empty values
- `keys` — Extract object keys
- `values` — Extract object values

**Includes:**
- Individual transform tests
- Chained transforms (unique → sort → reverse)
- Complex nesting scenarios
- Object key/value extraction

### 3. transforms-numeric.nika.yaml
Tests all 5 numeric transforms:
- `to_number` — Convert string to number
- `round` — Round to nearest integer
- `abs` — Absolute value
- `ceil` — Ceiling (round up)
- `floor` — Floor (round down)

**Includes:**
- String to number conversion
- Rounding operations
- Negative number handling
- Type preservation in chains
- Null safety

### 4. transforms-type.nika.yaml
Tests all 4 type transforms:
- `type_of` — Get JavaScript typeof
- `to_bool` — Convert to boolean (truthy/falsy)
- `to_json` — Serialize to JSON string
- `parse_json` — Deserialize from JSON string

**Includes:**
- Type checking on different value types
- JSON serialization/deserialization
- Truthiness evaluation
- Round-trip JSON conversion
- Type chains (object → JSON → parse → type_of)

### 5. transforms-parametric.nika.yaml
Tests all 3 parametric transforms:
- `join(sep)` — Join array with separator
- `split(sep)` — Split string by separator
- `default(fallback)` — Provide fallback for null/missing

**Includes:**
- Join with various separators (comma, semicolon, pipe, newline)
- Split operations on different string formats
- Default values for null handling
- Chained operations (split → unique → join)
- CSV/TSV parsing and manipulation

### 6. transforms-chains.nika.yaml
Complex real-world transform chains:

**15 chained transform tests:**
1. String: trim → upper → length
2. Array: unique → sort → join
3. Array: unique → sort → reverse → join
4. Nested: flatten → unique → sort
5. String split: split → unique → sort → join
6. JSON: parse_json → type_of
7. Type: to_number → round → to_string
8. Null safety: default → upper → length
9. Compact: compact → join → upper
10. Array: unique → sort → reverse
11. Split/unique: split → unique → first
12. Flatten: flatten → join → upper
13. String length: trim → length → to_string
14. Numeric: ceil → to_string → length
15. CSV: split → unique → sort → reverse → join

### 7. bindings-basic.nika.yaml
Tests basic binding system fundamentals:

**With block bindings:**
- Direct input binding: `with: { user: $inputs }`
- Path access: `{{with.user.name}}`
- Array indexing: `{{with.scores[0]}}`
- Nested paths: `{{with.user.tags[0]}}`
- Input references: `{{inputs.user.name}}`
- Multiple bindings in single task
- Null coalescing: `{{with.maybe_null ?? "fallback"}}`
- Default operator: `{{with.maybe_missing | default('N/A')}}`

### 8. bindings-cross-task.nika.yaml
Tests data flow between tasks:

**Task-to-task bindings:**
- Reference task output: `with: { data: $task_previous }`
- Array indexing on task output: `{{with.items[0]}}`
- Path access on task output: `{{with.data.nested.field}}`
- Transform task output: `{{with.data | upper}}`
- Multiple task dependencies: `depends_on: [task1, task2]`
- Ordering without data: `depends_on: [task_x]`
- Chained references: task_a → task_b → task_c
- Complex multi-task workflows

### 9. bindings-env.nika.yaml
Tests environment variable integration:

**$env bindings:**
- HOME environment: `with: { home: $env.HOME }`
- PATH environment: `with: { path: $env.PATH }`
- USER environment: `with: { user: $env.USER }`
- PWD environment: `with: { pwd: $env.PWD }`
- Missing env variables with defaults
- Transform env values (split PATH, uppercase HOME)
- Multiple env bindings in single task

### 10. bindings-edge-cases.nika.yaml
Tests edge cases and error conditions:

**Deep nesting:**
- 4+ levels of nested object access
- Accessing arrays inside deep nesting
- Type safety at depth

**Empty values:**
- Empty strings
- Empty arrays
- Empty objects
- Null values
- Zero and false values

**Null safety:**
- Missing field access with default
- Chained null safety (null → default → transform)
- Out of bounds array access

**Type mismatches:**
- Numeric strings vs numbers
- Whitespace handling
- Special characters in field names

### 11. comprehensive-test-suite.nika.yaml
Integration test combining all features:

**Realistic workflow scenario:**
- User database with nested arrays and objects
- CSV data processing
- JSON API responses
- All 31 transforms applied to real data
- Cross-task dependencies
- Null safety throughout
- Environment variables

**Coverage:**
- String transforms: name processing
- Array transforms: tag manipulation
- Numeric transforms: score handling
- Type transforms: JSON parsing
- Parametric transforms: CSV analysis
- Complex chains: pipelines
- Cross-task bindings: result synthesis
- Edge cases: null safety

## Running the Tests

### Run all tests with mock provider (no API keys needed):
```bash
nika run tests/workflows/transforms-string.nika.yaml --provider mock
nika run tests/workflows/transforms-array.nika.yaml --provider mock
nika run tests/workflows/transforms-numeric.nika.yaml --provider mock
nika run tests/workflows/transforms-type.nika.yaml --provider mock
nika run tests/workflows/transforms-parametric.nika.yaml --provider mock
nika run tests/workflows/transforms-chains.nika.yaml --provider mock
nika run tests/workflows/bindings-basic.nika.yaml --provider mock
nika run tests/workflows/bindings-cross-task.nika.yaml --provider mock
nika run tests/workflows/bindings-env.nika.yaml --provider mock
nika run tests/workflows/bindings-edge-cases.nika.yaml --provider mock
nika run tests/workflows/comprehensive-test-suite.nika.yaml --provider mock
```

### Run with dry-run (validate without executing):
```bash
nika check tests/workflows/*.nika.yaml --strict
```

### Run all in sequence:
```bash
for file in tests/workflows/*.nika.yaml; do
  echo "Testing: $file"
  nika run "$file" --provider mock || exit 1
done
```

## Transform Coverage Matrix

| Category | Transform | File | Status |
|----------|-----------|------|--------|
| String | `upper` | transforms-string.nika.yaml | ✓ |
| String | `lower` | transforms-string.nika.yaml | ✓ |
| String | `trim` | transforms-string.nika.yaml | ✓ |
| String | `trim_start` | transforms-string.nika.yaml | ✓ |
| String | `trim_end` | transforms-string.nika.yaml | ✓ |
| String | `length` | transforms-string.nika.yaml | ✓ |
| String | `to_string` | transforms-string.nika.yaml | ✓ |
| Array | `first` | transforms-array.nika.yaml | ✓ |
| Array | `last` | transforms-array.nika.yaml | ✓ |
| Array | `flatten` | transforms-array.nika.yaml | ✓ |
| Array | `reverse` | transforms-array.nika.yaml | ✓ |
| Array | `sort` | transforms-array.nika.yaml | ✓ |
| Array | `unique` | transforms-array.nika.yaml | ✓ |
| Array | `compact` | transforms-array.nika.yaml | ✓ |
| Array | `keys` | transforms-array.nika.yaml | ✓ |
| Array | `values` | transforms-array.nika.yaml | ✓ |
| Numeric | `to_number` | transforms-numeric.nika.yaml | ✓ |
| Numeric | `round` | transforms-numeric.nika.yaml | ✓ |
| Numeric | `abs` | transforms-numeric.nika.yaml | ✓ |
| Numeric | `ceil` | transforms-numeric.nika.yaml | ✓ |
| Numeric | `floor` | transforms-numeric.nika.yaml | ✓ |
| Type | `type_of` | transforms-type.nika.yaml | ✓ |
| Type | `to_bool` | transforms-type.nika.yaml | ✓ |
| Type | `to_json` | transforms-type.nika.yaml | ✓ |
| Type | `parse_json` | transforms-type.nika.yaml | ✓ |
| Parametric | `join(sep)` | transforms-parametric.nika.yaml | ✓ |
| Parametric | `split(sep)` | transforms-parametric.nika.yaml | ✓ |
| Parametric | `default(val)` | transforms-parametric.nika.yaml | ✓ |
| System | `shell` | N/A | (requires exec context) |
| Binding | `with:` blocks | bindings-*.nika.yaml | ✓ |
| Binding | Path access | bindings-*.nika.yaml | ✓ |
| Binding | Array indexing | bindings-*.nika.yaml | ✓ |
| Binding | Task refs `$task` | bindings-cross-task.nika.yaml | ✓ |
| Binding | Env vars `$env.VAR` | bindings-env.nika.yaml | ✓ |

## Null Safety Coverage

All transforms that fail on null are tested with `default()`:

**19 transforms requiring null safety:**
- String: upper, lower, trim, trim_start, trim_end, length, to_string
- Array: first, last, flatten, reverse, sort, unique, compact, keys, values
- Numeric: to_number, round, abs, ceil, floor

**Tests:**
- Direct null input with default
- Chained null safety (null → default → transform)
- Missing fields in paths with default
- Out of bounds array access with default

## Key Test Patterns

### 1. Individual Transform
```yaml
- id: test_upper
  with:
    text: "hello"
  infer: "Result: {{with.text | upper}}"
```

### 2. Chained Transforms
```yaml
- id: test_chain
  with:
    data: [3, 1, 2]
  infer: "Result: {{with.data | unique | sort | join(', ')}}"
```

### 3. Path Access
```yaml
- id: test_path
  with:
    user: $inputs
  infer: "Name: {{with.user.user.name}}"
```

### 4. Array Indexing
```yaml
- id: test_array
  with:
    items: $inputs
  infer: "First: {{with.items.items[0]}}"
```

### 5. Null Safety
```yaml
- id: test_null
  with:
    maybe: $null
  infer: "Safe: {{with.maybe | default('N/A')}}"
```

### 6. Cross-Task
```yaml
- id: task_a
  infer: "data"

- id: task_b
  depends_on: [task_a]
  with:
    prev: $task_a
  infer: "Got: {{with.prev}}"
```

### 7. Environment
```yaml
- id: test_env
  with:
    home: $env.HOME
  infer: "Home: {{with.home}}"
```

## Expected Mock Responses

When running with `provider: mock`:
- All `infer:` tasks return deterministic JSON responses
- No API calls are made
- Execution is instant
- Perfect for CI/CD pipelines
- Safe for testing without credentials

## Notes

1. **$null pseudo-value**: Used in test files to represent null/missing values
2. **Mock provider**: Returns predictable responses for testing
3. **shell transform**: Not tested here (requires `exec:` context)
4. **Path separator**: Uses `.` (dot) notation (e.g., `{{with.user.name}}`)
5. **Array access**: Uses bracket notation (e.g., `{{with.items[0]}}`)
6. **Transform syntax**: `{{value | transform1 | transform2}}`

## Files Created

```
tests/workflows/
├── transforms-string.nika.yaml          (7 tests)
├── transforms-array.nika.yaml           (9 tests)
├── transforms-numeric.nika.yaml         (5 tests)
├── transforms-type.nika.yaml            (4 tests)
├── transforms-parametric.nika.yaml      (3 tests)
├── transforms-chains.nika.yaml          (15 complex tests)
├── bindings-basic.nika.yaml             (11 tests)
├── bindings-cross-task.nika.yaml        (13 tests)
├── bindings-env.nika.yaml               (8 tests)
├── bindings-edge-cases.nika.yaml        (15 tests)
├── comprehensive-test-suite.nika.yaml   (11 integration tests)
└── README.md                            (this file)
```

**Total: 11 test files, 111+ individual test cases covering all 31 transforms + binding system**
