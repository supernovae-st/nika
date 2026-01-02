# Nika Specification

> DAG workflow runner for AI tasks

| Version | Schema | Status | Last Updated | Code Alignment |
|---------|--------|--------|--------------|----------------|
| **0.1** | `nika/workflow@0.1` | Stable | 2025-01-02 | **Aligned** |

**Source of Truth:** This spec. Code follows spec.

---

## Quick Reference

```yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: weather
    infer:
      prompt: "Get Paris weather as JSON"
    output:
      format: json

  - id: recommend
    use:
      forecast: weather.summary
      temp: weather.temp ?? 20
    infer:
      prompt: "Weather: {{use.forecast}} at {{use.temp}}C"

flows:
  - source: weather
    target: recommend
```

---

## 1. Unified Vocabulary

Same terms everywhere: Code, Spec, Docs, CLI.

| Term | Rust Type | YAML Key | Description |
|------|-----------|----------|-------------|
| Workflow | `Workflow` | root | Workflow definition |
| Task | `Task` | `tasks[].id` | Single unit of work |
| Action | `TaskAction` | `infer`/`exec`/`fetch` | What the task does |
| Flow | `Flow` | `flows[]` | DAG edge |
| Use | `UseWiring` | `use:` | Data dependencies |
| Output | `OutputPolicy` | `output:` | Format & validation |
| Store | `DataStore` | (runtime) | Task outputs |
| Result | `TaskResult` | (runtime) | Execution result |
| Error | `NikaError` | NIKA-XXX | Error with code |

---

## 2. Workflow

```yaml
schema: "nika/workflow@0.1"    # Required - must match exactly
provider: claude               # Default: "claude"
model: claude-sonnet-4-20250514  # Optional override
tasks: [...]                   # Required
flows: [...]                   # Optional (default: [])
```

### Providers

| Provider | API Key Env | Models |
|----------|-------------|--------|
| `claude` | `ANTHROPIC_API_KEY` | claude-sonnet-4-*, claude-haiku-* |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4o-mini |
| `mock` | - | (any) |

### Rust Type

```rust
pub struct Workflow {
    pub schema: String,
    pub provider: String,
    pub model: Option<String>,
    pub tasks: Vec<Arc<Task>>,
    pub flows: Vec<Flow>,
}
```

---

## 3. Task

```yaml
- id: my_task              # Required: snake_case
  use:                     # Optional: data dependencies
    alias: other.path
  infer:                   # Required: one action
    prompt: "..."
  output:                  # Optional: format & validation
    format: json
```

### Task ID Rules

Pattern: `^[a-z][a-z0-9_]*$` (snake_case)

| Valid | Invalid |
|-------|---------|
| `weather`, `get_data` | `Weather`, `get-data`, `123task` |

### Rust Type

```rust
pub struct Task {
    pub id: String,
    pub use_wiring: Option<UseWiring>,
    pub output: Option<OutputPolicy>,
    pub action: TaskAction,
}
```

---

## 4. Actions

Each task has exactly one action: `infer`, `exec`, or `fetch`.

### infer (LLM call)

```yaml
infer:
  prompt: "Recommend a restaurant in {{use.city}}"
  provider: openai     # Optional override
  model: gpt-4o-mini   # Optional override
```

### exec (shell command)

```yaml
exec:
  command: "npm run build"
```

### fetch (HTTP request)

```yaml
fetch:
  url: "https://api.example.com/data"
  method: POST           # Default: GET
  headers:
    Authorization: "Bearer {{use.token}}"
  body: '{"name": "{{use.name}}"}'
```

### Rust Type

```rust
pub enum TaskAction {
    Infer { infer: InferParams },
    Exec { exec: ExecParams },
    Fetch { fetch: FetchParams },
}

pub struct InferParams {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
}

pub struct ExecParams {
    pub command: String,
}

pub struct FetchParams {
    pub url: String,
    pub method: String,  // Default: "GET"
    pub headers: FxHashMap<String, String>,
    pub body: Option<String>,
}
```

---

## 5. Flow (DAG)

```yaml
flows:
  - source: task_a
    target: task_b

  # Fan-out
  - source: start
    target: [a, b, c]

  # Fan-in
  - source: [a, b]
    target: aggregate
```

### Execution Order

- Tasks with no dependencies run immediately (parallel)
- Tasks wait for ALL upstream dependencies
- Tasks only run if ALL dependencies succeeded

### Rust Type

```rust
pub struct Flow {
    pub source: FlowEndpoint,
    pub target: FlowEndpoint,
}

pub enum FlowEndpoint {
    Single(String),
    Multiple(Vec<String>),
}
```

---

## 6. Use Block

Declares data dependencies. Syntax: `alias: task.path [?? default]`

```yaml
use:
  # Simple path
  forecast: weather.summary

  # Nested path
  price: flights.cheapest.price

  # Entire task output
  raw: weather

  # Array index
  first: results.items.0

  # With defaults
  score: game.score ?? 0
  name: user.name ?? "Anonymous"
  config: 'settings ?? {"debug": false}'
```

### Path Syntax

| Pattern | Example | Description |
|---------|---------|-------------|
| `task` | `weather` | Entire output |
| `task.field` | `weather.summary` | Direct field |
| `task.a.b.c` | `weather.data.temp` | Nested path |
| `task.a.0` | `items.0` | Array index |

**Not supported (v0.1):** filters `[?()]`, wildcards `[*]`, slices `[0:5]`

### Default Values

| Type | Syntax |
|------|--------|
| Number | `?? 0`, `?? -1`, `?? 3.14` |
| Boolean | `?? true`, `?? false` |
| String | `?? "Anonymous"` (quoted!) |
| Object | `?? {"a": 1}` |
| Array | `?? ["x"]` |

### DAG Validation

Referenced task must:
1. Exist in workflow
2. Be upstream of consuming task

### Rust Type

```rust
pub type UseWiring = FxHashMap<String, UseEntry>;

pub struct UseEntry {
    pub path: String,
    pub default: Option<Value>,
}

impl UseEntry {
    pub fn task_id(&self) -> &str {
        self.path.split('.').next().unwrap_or(&self.path)
    }
}
```

---

## 7. Template

Syntax: `{{use.alias}}` or `{{use.alias.field}}`

```yaml
use:
  city: location.name
  temp: weather.temp
infer:
  prompt: |
    City: {{use.city}}
    Temperature: {{use.temp}}C
    Full data: {{use.weather}}
```

### Value Conversion

| Type | Output |
|------|--------|
| String | As-is |
| Number | `to_string()` |
| Boolean | `"true"` / `"false"` |
| Null | Error NIKA-072 |
| Object/Array | Compact JSON |

### Static Validation

All `{{use.X}}` must have corresponding `use:` declarations.

---

## 8. Output

```yaml
output:
  format: json                        # text (default) | json
  schema: .nika/schemas/result.json   # Optional JSON Schema
```

### Format

| Format | Stored As | Path Access |
|--------|-----------|-------------|
| `text` | `Value::String` | No |
| `json` | `Value::Object` | Yes |

**Rule:** Use `format: json` when downstream tasks need path access.

### Rust Type

```rust
pub struct OutputPolicy {
    pub format: OutputFormat,
    pub schema: Option<String>,
}

pub enum OutputFormat {
    #[default]
    Text,
    Json,
}
```

---

## 9. Runtime

### Data Flow

```
Task A → DataStore → use: block → {{use.alias}} → Task B
```

1. Task A completes, output stored in DataStore
2. Task B declares `use: { alias: taskA.path }`
3. Bindings resolved from DataStore
4. Templates substituted in prompt
5. Task B executes

### Key Types

```rust
pub struct DataStore {
    results: Arc<DashMap<Arc<str>, TaskResult>>,
}

pub struct TaskResult {
    pub output: Arc<Value>,
    pub duration: Duration,
    pub status: TaskStatus,
}

pub enum TaskStatus {
    Success,
    Failed(String),
}
```

---

## 10. Strict Mode (Default)

| Scenario | Behavior |
|----------|----------|
| Path resolves to `null` | Error NIKA-072 (unless `??` provided) |
| Path not found | Error NIKA-052 (unless `??` provided) |
| Traverse non-object | Error NIKA-073 |
| Unknown template alias | Error NIKA-071 |

---

## 11. Error Codes

### Schema (010)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-010 | Invalid schema version | Use `"nika/workflow@0.1"` |

### Path (050-056)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-050 | Invalid path syntax | Use `task.field.subfield` |
| NIKA-051 | Task not found | Verify task exists |
| NIKA-052 | Path not found | Add `?? default` or fix output |
| NIKA-055 | Invalid task ID | Use snake_case |
| NIKA-056 | Invalid default | Strings must be quoted |

### Output (060-061)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-060 | Invalid JSON | Ensure valid JSON output |
| NIKA-061 | Schema failed | Fix output to match schema |

### Use Block (070-074)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-070 | Duplicate alias | Use unique names |
| NIKA-071 | Unknown alias | Declare in `use:` block |
| NIKA-072 | Null value | Add `?? default` |
| NIKA-073 | Invalid traversal | Cannot access `.field` on primitive |
| NIKA-074 | Template parse error | Check `{{use.alias}}` syntax |

### DAG (080-082)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-080 | Task not in DAG | Verify task exists |
| NIKA-081 | Task not upstream | Add flow or change source |
| NIKA-082 | Circular dependency | Remove cycle |

### JSONPath (090-092)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-090 | Unsupported syntax | Use `$.a.b` or `$.a[0].b` |
| NIKA-091 | No match | Check path exists |
| NIKA-092 | Non-JSON output | Add `format: json` |

---

## 12. Code Architecture

```
┌─────────────────────────────────────────┐
│           DOMAIN MODEL (ast/)           │
│  Workflow, Task, TaskAction, Output     │
└─────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│         APPLICATION LAYER               │
│  runtime/  → Runner, TaskExecutor       │
│  dag/      → FlowGraph, validation      │
│  binding/  → UseWiring, templates       │
└─────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│        INFRASTRUCTURE LAYER             │
│  store/    → DataStore, TaskResult      │
│  event/    → EventLog, EventKind        │
│  provider/ → Claude, OpenAI, Mock       │
│  util/     → jsonpath, interner         │
└─────────────────────────────────────────┘
```

---

## Complete Example

```yaml
schema: "nika/workflow@0.1"
provider: claude

tasks:
  - id: weather
    infer:
      prompt: "Get Paris weather as JSON: {summary, temp, humidity}"
    output:
      format: json

  - id: flights
    fetch:
      url: "https://api.flights.com/paris"
    output:
      format: json

  - id: recommend
    use:
      forecast: weather.summary
      temp: weather.temp ?? 20
      price: flights.cheapest.price
      airline: flights.cheapest.airline
    infer:
      prompt: |
        Weather: {{use.forecast}} at {{use.temp}}C
        Flight: {{use.airline}} for ${{use.price}}

        Create a travel recommendation.
    output:
      format: json
      schema: .nika/schemas/recommendation.json

flows:
  - source: [weather, flights]
    target: recommend
```
