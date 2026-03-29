# Nika Transforms & Bindings Test Suite - Index

## Start Here

1. **Quick Reference**: [`QUICK-REFERENCE.md`](QUICK-REFERENCE.md) - 5-minute lookup guide
2. **Full README**: [`README.md`](README.md) - Complete test documentation
3. **Run Tests**: `./run-all-tests.sh` - Execute entire test suite

## What's Inside

### Test Files (11 total)

#### Transform Tests
1. [`transforms-string.nika.yaml`](transforms-string.nika.yaml) - String transforms (upper, lower, trim, etc.)
2. [`transforms-array.nika.yaml`](transforms-array.nika.yaml) - Array transforms (first, last, flatten, etc.)
3. [`transforms-numeric.nika.yaml`](transforms-numeric.nika.yaml) - Numeric transforms (round, ceil, floor, etc.)
4. [`transforms-type.nika.yaml`](transforms-type.nika.yaml) - Type transforms (type_of, to_json, parse_json, etc.)
5. [`transforms-parametric.nika.yaml`](transforms-parametric.nika.yaml) - Parametric transforms (join, split, default)
6. [`transforms-chains.nika.yaml`](transforms-chains.nika.yaml) - Complex chained transforms (15 scenarios)

#### Binding Tests
7. [`bindings-basic.nika.yaml`](bindings-basic.nika.yaml) - Basic with/path/index bindings
8. [`bindings-cross-task.nika.yaml`](bindings-cross-task.nika.yaml) - Task-to-task data flow
9. [`bindings-env.nika.yaml`](bindings-env.nika.yaml) - Environment variable bindings
10. [`bindings-edge-cases.nika.yaml`](bindings-edge-cases.nika.yaml) - Null safety and edge cases

#### Integration
11. [`comprehensive-test-suite.nika.yaml`](comprehensive-test-suite.nika.yaml) - Everything combined

### Documentation

- [`README.md`](README.md) - Main documentation with file descriptions and matrix
- [`QUICK-REFERENCE.md`](QUICK-REFERENCE.md) - Quick lookup guide for transforms and patterns
- [`TRANSFORMS-BINDINGS-TEST-SUMMARY.md`](TRANSFORMS-BINDINGS-TEST-SUMMARY.md) - Comprehensive overview
- [`VALIDATION-CHECKLIST.md`](VALIDATION-CHECKLIST.md) - Complete coverage checklist
- [`INDEX.md`](INDEX.md) - This file

### Scripts

- [`run-all-tests.sh`](run-all-tests.sh) - Test runner with color output

## Coverage Summary

**31 Pipe Transforms** (100% covered)
- 7 String: upper, lower, trim, trim_start, trim_end, length, to_string
- 9 Array: first, last, flatten, reverse, sort, unique, compact, keys, values
- 5 Numeric: to_number, round, abs, ceil, floor
- 4 Type: type_of, to_bool, to_json, parse_json
- 3 Parametric: join(sep), split(sep), default(val)

**Binding System** (100% covered)
- Basic with blocks and path access
- Array indexing and nested paths
- Cross-task data flow (depending on other tasks)
- Environment variable bindings ($env.VAR)
- Null safety with default operator
- Deep nesting (4+ levels)

**Edge Cases**
- Null values and missing fields
- Empty strings, arrays, objects
- Zero and false values
- Type mismatches
- Out of bounds access
- Whitespace handling
- Special characters

## Quick Stats

| Metric | Count |
|--------|-------|
| Total test files | 11 |
| Total test cases | 111+ |
| Total lines of test code | 1800+ |
| Transform coverage | 31/31 (100%) |
| Binding coverage | 100% |
| Mock provider tests | All |
| Documentation files | 5 |

## How to Use

### Run Everything
```bash
./run-all-tests.sh
```

### Run a Specific Test
```bash
nika run transforms-string.nika.yaml --provider mock
```

### Validate Syntax Only
```bash
nika check transforms-string.nika.yaml --strict
```

### Run All with Verbose Output
```bash
./run-all-tests.sh --verbose
```

### Dry-Run (Validation Only)
```bash
./run-all-tests.sh --dry-run
```

## Key Features

1. **Deterministic**: Uses `provider: mock` for predictable results
2. **Offline**: No API keys or network access required
3. **Fast**: Mock provider executes instantly
4. **Comprehensive**: All 31 transforms + complete binding system
5. **Well-documented**: Multiple guides at different detail levels
6. **Easy to extend**: Simple patterns for adding new tests
7. **CI/CD ready**: Suitable for automated testing pipelines

## Transform Decision Tree

Need a transform? Use this to find the right one:

**Manipulate strings?** → See `transforms-string.nika.yaml`
- Uppercase/lowercase → `upper` / `lower`
- Remove whitespace → `trim` / `trim_start` / `trim_end`
- Get length → `length`
- Convert to string → `to_string`

**Manipulate arrays?** → See `transforms-array.nika.yaml`
- Get first/last → `first` / `last`
- Flatten nested → `flatten`
- Change order → `reverse` / `sort`
- Remove duplicates → `unique`
- Remove nulls → `compact`
- Get keys/values → `keys` / `values`

**Manipulate numbers?** → See `transforms-numeric.nika.yaml`
- Convert from string → `to_number`
- Round/ceil/floor → `round` / `ceil` / `floor`
- Absolute value → `abs`

**Check or convert types?** → See `transforms-type.nika.yaml`
- Check type → `type_of`
- Convert to boolean → `to_bool`
- Serialize to JSON → `to_json`
- Parse from JSON → `parse_json`

**Combine or split values?** → See `transforms-parametric.nika.yaml`
- Join array → `join(sep)`
- Split string → `split(sep)`
- Provide fallback → `default(fallback)`

**Need complex operations?** → See `transforms-chains.nika.yaml`
- Multiple transforms combined
- Real-world scenarios

**Need binding help?** → See `bindings-*.nika.yaml`
- Basic paths → `bindings-basic.nika.yaml`
- Cross-task → `bindings-cross-task.nika.yaml`
- Environment → `bindings-env.nika.yaml`
- Edge cases → `bindings-edge-cases.nika.yaml`

## Common Patterns

### Pattern: Clean and transform
```yaml
{{with.text | trim | lower | length}}
```

### Pattern: Parse CSV
```yaml
{{with.csv | split(',') | unique | sort | join(' | ')}}
```

### Pattern: Array manipulation
```yaml
{{with.items | unique | sort | reverse | first}}
```

### Pattern: Type checking
```yaml
{{with.data | type_of}}
```

### Pattern: Null safety
```yaml
{{with.maybe_null | default('N/A') | upper}}
```

### Pattern: Cross-task
```yaml
depends_on: [task_a]
with:
  data: $task_a
```

### Pattern: Environment access
```yaml
with:
  home: $env.HOME
```

## File Organization

```
tests/workflows/
├── transforms-string.nika.yaml              Test file
├── transforms-array.nika.yaml               Test file
├── transforms-numeric.nika.yaml             Test file
├── transforms-type.nika.yaml                Test file
├── transforms-parametric.nika.yaml          Test file
├── transforms-chains.nika.yaml              Test file
├── bindings-basic.nika.yaml                 Test file
├── bindings-cross-task.nika.yaml            Test file
├── bindings-env.nika.yaml                   Test file
├── bindings-edge-cases.nika.yaml            Test file
├── comprehensive-test-suite.nika.yaml       Integration test
├── run-all-tests.sh                         Test runner
├── README.md                                Full documentation
├── QUICK-REFERENCE.md                       Quick lookup
├── TRANSFORMS-BINDINGS-TEST-SUMMARY.md      Comprehensive summary
├── VALIDATION-CHECKLIST.md                  Coverage checklist
└── INDEX.md                                 This file
```

## Next Steps

1. Read [`QUICK-REFERENCE.md`](QUICK-REFERENCE.md) for a quick overview
2. Run `./run-all-tests.sh` to execute all tests
3. Check [`README.md`](README.md) for detailed information
4. Extend tests with your own scenarios
5. Use as reference for your workflows

## Notes

- All test files use `provider: mock` for deterministic testing
- No API keys are required
- Tests execute instantly without network calls
- Perfect for CI/CD pipelines
- All YAML follows schema `nika/workflow@0.12`

---

**Version**: 1.0
**Created**: 2026-03-29
**Target**: Nika v0.12+
**Status**: Complete and ready to use
