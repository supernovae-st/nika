# Nika Transforms & Bindings - Quick Reference

## The 31 Pipe Transforms

### String (7)
```
upper    lower    trim    trim_start    trim_end    length    to_string
```

### Array (9)
```
first    last    flatten    reverse    sort    unique    compact    keys    values
```

### Numeric (5)
```
to_number    round    abs    ceil    floor
```

### Type (4)
```
type_of    to_bool    to_json    parse_json
```

### Parametric (3)
```
join(sep)    split(sep)    default(val)
```

## Transform Syntax

### Single Transform
```yaml
{{with.text | upper}}
```

### Chained Transforms
```yaml
{{with.csv | split(',') | unique | sort | join(', ')}}
```

### With Default (Null Safety)
```yaml
{{with.maybe_null | default('N/A') | upper}}
```

## Binding Types

### Input Binding
```yaml
with:
  user: $inputs
infer: "Name: {{with.user.name}}"
```

### Task Binding
```yaml
depends_on: [task_a]
with:
  data: $task_a
infer: "Data: {{with.data}}"
```

### Environment Binding
```yaml
with:
  home: $env.HOME
infer: "Home: {{with.home}}"
```

### Path Access
```yaml
{{with.user.profile.name}}           # Nested objects
{{with.items[0]}}                    # Array indexing
{{with.user.tags[0] | upper}}        # Combined
```

## Real-World Examples

### CSV Processing
```yaml
with:
  csv: "alice,bob,charlie"
infer: |
  Unique: {{with.csv | split(',') | unique | join(', ')}}
```

Output: `Unique: alice, bob, charlie`

### User Data Transform
```yaml
with:
  user: $inputs
infer: |
  Name: {{with.user.name | upper}}
  Email length: {{with.user.email | length}}
```

### Array Statistics
```yaml
with:
  scores: [95, 87, 92, 88]
infer: |
  Total: {{with.scores | length}}
  Sorted: {{with.scores | sort | join(', ')}}
  First: {{with.scores | first}}
```

### Type Checking
```yaml
with:
  data: $some_task
infer: |
  Type: {{with.data | type_of}}
  JSON: {{with.data | to_json}}
```

### Cross-Task Flow
```yaml
- id: task_a
  infer: "Generated data"

- id: task_b
  depends_on: [task_a]
  with:
    prev: $task_a
  infer: "Previous: {{with.prev}}"
```

### Null Safety
```yaml
with:
  maybe_missing: $null
infer: "Value: {{with.maybe_missing | default('N/A')}}"
```

## Transform Decision Tree

**Need to transform a string?**
- Uppercase/lowercase → `upper` / `lower`
- Remove whitespace → `trim` / `trim_start` / `trim_end`
- Get length → `length`
- Convert to string → `to_string`

**Need to transform an array?**
- Get first/last → `first` / `last`
- Flatten nested → `flatten`
- Change order → `reverse` / `sort`
- Remove duplicates → `unique`
- Remove nulls → `compact`
- Get keys/values → `keys` / `values`

**Need to transform a number?**
- Convert from string → `to_number`
- Round/ceil/floor → `round` / `ceil` / `floor`
- Absolute value → `abs`

**Need to transform a type?**
- Check type → `type_of`
- Convert to boolean → `to_bool`
- Serialize to JSON → `to_json`
- Parse from JSON → `parse_json`

**Need to combine values?**
- Join array → `join(sep)`
- Split string → `split(sep)`

**Have a null value?**
- Always use → `default(fallback)`

## Common Patterns

### Pattern 1: Data Cleaning
```yaml
{{with.text | trim | lower | length}}
```

### Pattern 2: Array Processing
```yaml
{{with.items | unique | sort | reverse | first}}
```

### Pattern 3: CSV Parsing
```yaml
{{with.csv | split(',') | unique | sort | join(' | ')}}
```

### Pattern 4: Type Safety
```yaml
{{with.data | type_of}}
{{with.json_str | parse_json | type_of}}
```

### Pattern 5: Null Handling
```yaml
{{with.maybe_null | default('N/A') | upper}}
```

### Pattern 6: Deep Path Access
```yaml
{{with.user.profile.settings.theme}}
```

### Pattern 7: Array Element Access
```yaml
{{with.items[0]}}           # First
{{with.items[items.length-1]}}  # Last (via length transform)
```

### Pattern 8: Multi-Step Transform
```yaml
{{with.users | length}} users,
{{with.users[0].name | upper}} is first
```

## Test Files Quick Map

| Need | File |
|------|------|
| String transforms | `transforms-string.nika.yaml` |
| Array transforms | `transforms-array.nika.yaml` |
| Numeric transforms | `transforms-numeric.nika.yaml` |
| Type transforms | `transforms-type.nika.yaml` |
| Parametric transforms | `transforms-parametric.nika.yaml` |
| Complex chains | `transforms-chains.nika.yaml` |
| Basic bindings | `bindings-basic.nika.yaml` |
| Task references | `bindings-cross-task.nika.yaml` |
| Environment vars | `bindings-env.nika.yaml` |
| Edge cases | `bindings-edge-cases.nika.yaml` |
| Everything together | `comprehensive-test-suite.nika.yaml` |

## Key Rules

1. **Null safety first**: Always use `default()` when null is possible
2. **Transform chains**: Pipe multiple transforms together
3. **Path notation**: Use dots for nested objects, brackets for arrays
4. **Task refs**: Use `$task_id` to reference task outputs
5. **Env vars**: Access via `$env.VAR_NAME` in `with:` blocks
6. **Inputs**: Reference via `{{inputs.field}}` or `with: { val: $inputs }`

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `{{data}}` | `{{with.data}}` |
| `{{item}}` in for_each | `{{with.item}}` |
| `with: { x: task_a }` | `with: { x: $task_a }` |
| No default on null | `with.null \| default('val')` |
| `item.field[0]` | `item[0].field` |
| `use: task_a` | `with: { x: $task_a }` |

## Running Tests

```bash
# All tests
./run-all-tests.sh

# Single test
nika run transforms-string.nika.yaml --provider mock

# Validate only
./run-all-tests.sh --dry-run

# Verbose output
./run-all-tests.sh --verbose
```

---

**Reference**: Nika v0.12+ | 31 transforms | Complete binding system
