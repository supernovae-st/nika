# 09 -- Binding and Template System

## Overview

The binding and template system is how data flows between tasks in Nika workflows. It consists of three layers:

1. **with: blocks** -- Declare named data references between tasks
2. **Template resolution** -- Substitute `{{with.alias}}` placeholders at runtime
3. **Pipe transforms** -- Apply transformations to binding values

---

## with: Block Syntax

### Simple Task Reference

```yaml
with:
  data: $task_id
```

The `$` prefix indicates a task reference. The entire output of `task_id` is bound to the alias `data`.

### Path Traversal

```yaml
with:
  name: $task_id.user.name
  email: $task_id.contacts[0].email
  count: $task_id.items.length
```

Paths traverse JSON output using dot notation and bracket indexing.

### Default Values

```yaml
with:
  temp: $task_id.temp ?? 20           # Numeric default
  name: $task_id.name ?? "Anonymous"  # String default (must be quoted)
  active: $task_id.active ?? true     # Boolean default
  cfg: $task_id.cfg ?? {"debug": false}  # Object default
  items: $task_id.items ?? [1, 2, 3]  # Array default
```

The `??` operator provides a fallback value when the path resolves to null or the task has no output.

### Pipe Transforms

```yaml
with:
  upper_name: $task_id.name | upper | trim
  first_three: $task_id.items | sort | first(3)
  csv_line: $task_id.values | join(",")
```

Transforms are chained with `|` and applied left-to-right. See the Transform Reference section below for all 27 transforms.

### Lazy Bindings

```yaml
with:
  lazy_val:
    path: $future_task.result
    lazy: true
  lazy_with_default:
    path: $optional_task.value
    lazy: true
    default: "fallback"
```

Lazy bindings are deferred until template resolution. They allow referencing tasks that may not have completed yet. The binding stores a `Pending` state and resolves from the `RunContext` datastore on access.

---

## Data Flow

```
YAML `with:` block
        |
        v
BindingSpec (parsed in Phase 2)
        |
        +-- Eager (lazy=false): resolve immediately from RunContext
        +-- Lazy (lazy=true): store as Pending
        |
        v
ResolvedBindings
        |
        +-- Resolved: value available
        +-- Pending: deferred until access
        |
        v
Template Substitution ({{with.alias}})
        |
        +-- Resolved: inline value
        +-- Pending: resolve from RunContext, then inline
        |
        v
Pipe Transforms (| upper | trim)
        |
        v
Final resolved text
```

---

## Core Types

### BindingSpec / BindingEntry

```rust
pub struct BindingSpec(pub Vec<BindingEntry>);

pub struct BindingEntry {
    pub alias: String,
    pub source: BindingSource,
}

pub enum BindingSource {
    TaskOutput { task_id: String },
    TaskPath { task_id: String, path: BindingPath },
    Env { var_name: String },
    Literal(serde_json::Value),
}
```

### WithSpec / WithEntry

```rust
pub struct WithSpec(pub Vec<WithEntry>);

pub struct WithEntry {
    pub alias: String,
    pub path: BindingPath,
    pub default: Option<serde_json::Value>,
    pub transforms: Option<TransformExpr>,
    pub lazy: bool,
}
```

### BindingPath

```rust
pub struct BindingPath {
    pub task_id: String,
    pub segments: Vec<PathSegment>,
}

pub enum PathSegment {
    Field(String),    // .name
    Index(usize),     // [0]
}
```

### ResolvedBindings

```rust
pub struct ResolvedBindings {
    bindings: FxHashMap<String, BindingValue>,
}

enum BindingValue {
    Resolved(serde_json::Value),
    Pending(LazyBinding),
}

pub struct LazyBinding {
    pub task_id: String,
    pub path: Option<BindingPath>,
    pub default: Option<serde_json::Value>,
}
```

---

## Template Resolution

Templates use double-brace syntax: `{{expression}}`.

### Template Types

| Pattern | Description |
|---------|-------------|
| `{{with.alias}}` | Binding value |
| `{{with.alias.path.to.field}}` | Path traversal into binding |
| `{{with.alias \| transform}}` | Transform pipe applied to binding |
| `{{context.alias}}` | Context file content |
| `{{inputs.param}}` | Input parameter |
| `{{env.VAR_NAME}}` | Environment variable |

### Resolution Process

The `template::resolve()` function:

1. Scans text for `{{...}}` patterns
2. Parses the expression: alias, path segments, transforms
3. Looks up the alias in `ResolvedBindings`
4. If `Pending`, resolves from `RunContext` datastore
5. Traverses path segments (dot fields, bracket indices)
6. Applies transforms (if any)
7. Converts result to string and replaces the template

### Reference Extraction

```rust
pub fn extract_with_refs(template: &str) -> Vec<String>;
```

Extracts all `with.*` references from a template string. Used by the analyzer to compute implicit dependencies.

### Validation

```rust
pub fn validate_with_refs(template: &str, bindings: &WithSpec) -> Vec<String>;
```

Returns errors for any `{{with.alias}}` that is not declared in the task's `with:` block.

### Shell Escaping

```rust
pub fn resolve_for_shell(template: &str, bindings: &ResolvedBindings, datastore: &RunContext)
    -> Result<Cow<str>, NikaError>;
```

Special resolution for `exec:` commands that escapes values for safe shell interpolation.

---

## JSONPath Support

The binding system supports RFC 9535 JSONPath via the `serde_json_path` crate.

### Simple Paths (Recommended)

```yaml
with:
  name: $task.user.name         # $.user.name
  first: $task.items[0]         # $.items[0]
  nested: $task.data.list[2].id # $.data.list[2].id
```

### JSONPath Queries

```yaml
with:
  all_names: $task.$.users[*].name
```

Complex JSONPath expressions (wildcards, recursive descent, filters) are supported but may produce NIKA-090 for unsupported syntax. Simple paths (`$.a.b`, `$.a[0].b`) are always safe.

---

## Transform Reference (31 Transforms)

### String Transforms

| Transform | Input | Output | Null Behavior |
|-----------|-------|--------|---------------|
| `upper` | `"hello"` | `"HELLO"` | NIKA-153 error |
| `lower` | `"HELLO"` | `"hello"` | NIKA-153 error |
| `trim` | `" hello "` | `"hello"` | NIKA-153 error |
| `trim_start` | `" hello"` | `"hello"` | NIKA-153 error |
| `trim_end` | `"hello "` | `"hello"` | NIKA-153 error |

### Collection Transforms

| Transform | Input | Output | Null Behavior |
|-----------|-------|--------|---------------|
| `length` | `[1,2,3]` | `3` | Propagates null |
| `first` | `[1,2,3]` | `1` | NIKA-153 error |
| `last` | `[1,2,3]` | `3` | NIKA-153 error |
| `first(N)` | `[1,2,3,4]` | `[1,2]` (N=2) | NIKA-153 error |
| `last(N)` | `[1,2,3,4]` | `[3,4]` (N=2) | NIKA-153 error |
| `keys` | `{"a":1,"b":2}` | `["a","b"]` | Propagates null |
| `values` | `{"a":1,"b":2}` | `[1,2]` | Propagates null |
| `flatten` | `[[1,2],[3]]` | `[1,2,3]` | NIKA-153 error |
| `reverse` | `[1,2,3]` | `[3,2,1]` | NIKA-153 error |
| `sort` | `[3,1,2]` | `[1,2,3]` | NIKA-153 error |
| `unique` | `[1,2,2,3]` | `[1,2,3]` | NIKA-153 error |
| `compact` | `[1,null,2]` | `[1,2]` | Removes nulls |

### Type Conversion Transforms

| Transform | Input | Output | Null Behavior |
|-----------|-------|--------|---------------|
| `to_string` | `42` | `"42"` | Propagates null |
| `to_number` | `"42"` | `42` | NIKA-152 if not numeric |
| `to_bool` | `"true"` | `true` | NIKA-152 if not boolean-like |
| `to_json` | any | JSON string | Propagates null |
| `parse_json` | `"{\"a\":1}"` | `{"a":1}` | NIKA-152 on invalid JSON |

### Numeric Transforms

| Transform | Input | Output | Null Behavior |
|-----------|-------|--------|---------------|
| `round(N)` | `3.14159` | `3.14` (N=2) | NIKA-152 if not number |
| `abs` | `-5` | `5` | NIKA-152 if not number |
| `ceil` | `3.2` | `4` | NIKA-152 if not number |
| `floor` | `3.8` | `3` | NIKA-152 if not number |

### Utility Transforms

| Transform | Input | Output | Null Behavior |
|-----------|-------|--------|---------------|
| `default(V)` | null | V | Returns default |
| `type_of` | `42` | `"number"` | Returns `"null"` |
| `join(S)` | `["a","b"]` | `"a,b"` (S=",") | NIKA-153 error |
| `split(S)` | `"a,b,c"` | `["a","b","c"]` | NIKA-153 error |
| `shell` | `"hello world"` | `"'hello world'"` | NIKA-153 error |

### Transform Chaining

Transforms are chained with `|` and apply left-to-right:

```yaml
with:
  result: $task.names | sort | unique | first(5) | join(", ")
```

This sorts the names, removes duplicates, takes the first 5, and joins them with commas.

### Null Handling

Two behaviors:

- **Propagating**: null input produces null output. Safe transforms: `length`, `keys`, `values`, `to_string`, `to_json`, `type_of`.
- **Failing**: null input produces NIKA-153 error. Use `default()` or `??` to handle nulls safely:

```yaml
with:
  safe_name: $task.name ?? "Unknown" | upper
  safe_count: $task.items | default([]) | length
```

---

## Mention System

The `mention` module provides `@task` mention syntax for inline task references:

```yaml
infer:
  prompt: "Based on @gather_data, write a summary"
```

### Mention Resolution

```rust
pub fn parse_mentions(text: &str) -> Vec<Mention>;
pub fn resolve_mention(mention: &Mention, datastore: &RunContext) -> Result<ResolvedMention>;
pub fn text_to_bindings(text: &str) -> Vec<BindingEntry>;
```

Mentions are syntactic sugar that get converted to implicit bindings during analysis.

### Parallel Marker

```yaml
infer:
  prompt: "Compare @task_a || @task_b"
```

The `||` marker indicates that the mentioned tasks can run in parallel.

---

## Binding Validation

### DAG Validation

The `validate_bindings()` function (in `dag/validate.rs`) checks that all `with:` references:

1. **Reference existing tasks**: NIKA-080 if the referenced task does not exist
2. **Are upstream**: NIKA-081 if the referenced task is not a predecessor in the DAG
3. **Do not create cycles**: NIKA-082 if the binding creates a circular dependency

### Template Validation

The `validate_with_refs()` function checks that every `{{with.alias}}` in templates corresponds to a declared `with:` entry.

### Task ID Validation

```rust
pub fn validate_task_id(id: &str) -> Result<(), NikaError>;
```

Task IDs must match `[a-zA-Z][a-zA-Z0-9_-]*`. They cannot start with numbers, contain spaces, or use reserved characters.

---

## Advanced Binding Patterns

### Multi-Level Path Traversal

Bindings support arbitrary depth of JSON traversal:

```yaml
with:
  deep_value: $api_response.data.results[0].metadata.tags[2].name
```

Each segment is resolved left-to-right. If any segment is null or the path does not exist, the result is null (use `??` for defaults).

### Combining Defaults and Transforms

Defaults are resolved before transforms:

```yaml
with:
  safe_name: $task.user.name ?? "Unknown" | upper | trim
```

This first resolves the name (or uses "Unknown" if null), then applies `upper` and `trim`.

### Array-Aware Bindings for for_each

When used with `for_each`, the loop variable binding is injected automatically:

```yaml
- id: process_items
  for_each: $data.items
  as: item
  infer:
    prompt: "Process: {{with.item.name}} - {{with.item.description}}"
```

Each iteration receives its own `ResolvedBindings` with the `item` alias bound to the current array element.

### Environment Variable Bindings

```yaml
with:
  api_key: $env.API_KEY
  home: $env.HOME
```

Environment variables are accessed via the `$env.` prefix. They are resolved at runtime from the process environment.

### Cross-Task Data Aggregation

When multiple tasks feed into a downstream task, each gets its own alias:

```yaml
- id: combine
  infer:
    prompt: |
      Summary A: {{with.summary_a}}
      Summary B: {{with.summary_b}}
      Synthesize these into a single overview.
  with:
    summary_a: $task_a
    summary_b: $task_b
```

Both tasks must complete before `combine` starts. The DAG enforces this via implicit dependencies extracted from the `with:` block.

---

## Performance Considerations

### Binding Resolution Performance

- **FxHashMap**: Used for resolved bindings (faster than std HashMap for string keys)
- **SmallVec**: Path segments use `SmallVec<[PathSegment; 4]>` for stack allocation
- **Arc<str>**: Task IDs use `Arc<str>` for zero-cost cloning
- **Lazy evaluation**: Lazy bindings avoid unnecessary work when paths are not accessed

### Template Resolution Performance

- **Cow<str>**: Template resolution returns `Cow<str>` to avoid allocation when no substitution is needed
- **Single-pass scanning**: Templates are scanned once for `{{...}}` patterns
- **Compiled transforms**: Transform chains are parsed once and stored as `SmallVec<[TransformOp; 2]>`

### Common Pitfalls

1. **Circular with: references**: Task A binds from Task B which binds from Task A. Caught by the analyzer in Phase 2.

2. **Large value binding**: Binding an entire task output (e.g., a 10 MB HTTP response) into a template inflates prompt size. Use path traversal to extract only needed fields.

3. **Null propagation chains**: `$task.a.b.c` where `a` is null produces null at the first missing segment. All subsequent segments are skipped.

4. **Transform type mismatches**: Applying `upper` to a number produces NIKA-152. Use `to_string` first: `$task.count | to_string | upper`.
