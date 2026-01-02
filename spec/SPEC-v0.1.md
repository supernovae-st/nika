# Nika v0.1 Specification

> **Version**: 0.1 | **Status**: Draft | **Date**: 2025-01-01

---

## TL;DR

```yaml
tasks:
  - id: analyze
    use:                                    # Explicit data wiring
      summary: weather.forecast             # Simple path
      price: flights.cheapest.price         # Nested path
      fallback: weather.backup ?? "N/A"     # With default
    infer:
      prompt: "{{use.summary}} - ${{use.price}}"
    output:                                 # Output validation
      format: json
      schema: .nika/schemas/result.json
```

| Concept | Purpose | Optional? |
|---------|---------|-----------|
| `use:` | Declare data dependencies | Yes |
| `output:` | Format + validate output | Yes |
| `{{use.x}}` | Access resolved data | - |

---

## 1. Core Concepts

**Three concerns, cleanly separated:**

| Block | Purpose | Example |
|-------|---------|---------|
| `flows:` | Execution order (DAG) | `source: A, target: B` |
| `use:` | Data wiring (inputs) | `alias: task.path` |
| `output:` | Format + validation | `format: json` |

**Data flow:**
```
Task A → DataStore.outputs["A"] → use: block → {{use.alias}} → Task B
```

---

## 2. The `use:` Block

**Unified syntax:** `alias: task.path [?? default]`

```yaml
use:
  # Simple path
  forecast: weather.summary

  # Nested path
  price: flights.cheapest.price
  airline: flights.cheapest.airline

  # Array index
  first_item: results.items[0].name

  # Entire task output
  raw_data: weather

  # With default (number)
  score: game.stats.score ?? 0

  # With default (string - MUST be quoted as JSON)
  name: user.profile.name ?? "Anonymous"

  # With default (object - needs YAML quoting)
  config: 'settings.options ?? {"debug": false}'

  # With default (array)
  tags: 'metadata.tags ?? ["untagged"]'
```

**Task ID rules:**
- Pattern: `^[a-z][a-z0-9_]*$` (snake_case)
- Valid: `weather`, `get_data`, `fetch_api`
- Invalid: `fetch-api` (dash), `myTask` (uppercase), `weather.api` (dot)

**Default value rules:**
- Parsed as JSON literal
- Strings MUST be quoted: `?? "Anonymous"` not `?? Anonymous`
- Numbers, bools, null: `?? 0`, `?? true`, `?? null`
- Objects/arrays: `?? {"a": 1}`, `?? ["x"]`

**Path syntax (v0.1 minimal):**

| Syntax | Example | Description |
|--------|---------|-------------|
| `task` | `weather` | Entire task output |
| `task.field` | `weather.summary` | Direct field |
| `task.a.b.c` | `weather.data.temp` | Nested path |
| `task.a[0]` | `results.items[0]` | Array index |
| `task.a[0].b` | `results.items[0].name` | Combined |

**Not supported in v0.1:** filters `[?(@.x)]`, wildcards `[*]`, slices `[0:5]`, recursive `..`

---

## 3. Strict Mode (v0.1 Default)

Nika v0.1 uses **strict null handling** to prevent silent bugs:

| Scenario | Behavior |
|----------|----------|
| Path resolves to `null` | Error NIKA-072 (unless `??` default provided) |
| Path not found | Error NIKA-052 (unless `??` default provided) |
| Traverse non-object | Error NIKA-073 |
| Unknown template alias | Error NIKA-071 |

**With default (`??` operator):**
```yaml
use:
  price: flight.cost ?? 0           # Used if null or missing
  name: user.name ?? "Anonymous"    # String default (quoted)
```

---

## 4. Template Syntax

Single interpolation syntax: `{{use.alias}}` or `{{use.alias.field}}`

```yaml
prompt: |
  Weather: {{use.weather}}
  Flight: {{use.airline}} for ${{use.price}}
  Full object: {{use.flight_info}}
  Nested: {{use.flight_info.departure}}
```

**Static validation:** All `{{use.X}}` references must have corresponding `use:` declarations.

**Value conversion:**
| Type | Output |
|------|--------|
| String | As-is |
| Number | `to_string()` |
| Boolean | `"true"` / `"false"` |
| Null | Error NIKA-072 |
| Object/Array | Compact JSON |

---

## 5. The `output:` Block

```yaml
output:
  format: json                   # text (default) | json
  schema: .nika/schemas/x.json   # Optional JSON Schema
```

**When to use:**
- `format: json` - Enable path access in downstream `use:` blocks
- `schema:` - Validate output structure

---

## 6. Complete Example

```yaml
schema: "nika/workflow@0.1"

tasks:
  - id: weather
    infer:
      prompt: "Get Paris weather as JSON"
    output:
      format: json

  - id: flights
    fetch:
      url: "https://api.flights.com/paris"
    output:
      format: json

  - id: recommend
    use:
      # Simple paths
      forecast: weather.summary
      city: weather.city

      # Nested paths
      price: flights.cheapest.price
      airline: flights.cheapest.airline

      # With defaults
      rating: weather.rating ?? 5
      notes: 'weather.notes ?? "No notes"'
    infer:
      prompt: |
        Weather in {{use.city}}: {{use.forecast}}
        Rating: {{use.rating}}/5
        Notes: {{use.notes}}

        Best flight: {{use.airline}} for ${{use.price}}

        Create a recommendation.
    output:
      format: json
      schema: .nika/schemas/recommendation.json

flows:
  - source: [weather, flights]
    target: recommend
```

---

## 7. Validation

### Static (nika validate)
- Path syntax valid (`task.field.subfield`)
- Task IDs are snake_case (`^[a-z][a-z0-9_]*$`)
- No duplicate aliases
- All `{{use.X}}` have corresponding `use:` entry
- Referenced tasks exist in DAG
- **DAG validation:** Referenced task must exist AND be upstream of consuming task
- **Default syntax:** JSON literal (strings quoted)

### DAG Validation Rules

The `use:` block declares data dependencies. These must respect the DAG:

```yaml
flows:
  - source: weather
    target: recommend

tasks:
  - id: recommend
    use:
      forecast: weather.summary    # OK: weather is upstream
      data: other_task.value       # ERROR: other_task not upstream
```

**Rule:** Every task referenced in a `use:` path must:
1. Exist in the workflow
2. Be upstream of the consuming task in the DAG
3. Have a valid snake_case ID (`^[a-z][a-z0-9_]*$`)

### Runtime
- Paths resolve to actual values
- Null values trigger error (strict mode)
- Output matches schema (if declared)

---

## 8. Error Codes

| Code | Error | Fix |
|------|-------|-----|
| NIKA-050 | Invalid path syntax | Check format: `task.field.subfield` |
| NIKA-051 | Task not found | Verify task_id exists |
| NIKA-052 | Path not found | Add `?? default` or fix output |
| NIKA-055 | Invalid task ID | Use snake_case: `fetch_api` not `fetch-api` |
| NIKA-056 | Invalid default JSON | Strings must be quoted: `?? "Anonymous"` |
| NIKA-060 | Invalid JSON | Ensure valid JSON output |
| NIKA-061 | Schema failed | Fix output to match schema |
| NIKA-070 | Duplicate alias | Use unique names in `use:` |
| NIKA-071 | Unknown alias | Declare in `use:` before referencing |
| NIKA-072 | Null value | Add `?? default` or ensure non-null |
| NIKA-073 | Invalid traversal | Cannot access `.field` on primitive |
| NIKA-074 | Template parse error | Check `{{use.alias}}` syntax |
| NIKA-080 | Task not found in DAG | Check path references existing task |
| NIKA-081 | Task not upstream | Referenced task must be upstream in DAG |
| NIKA-082 | Circular dependency | Check DAG for cycles |
| NIKA-090 | JSONPath unsupported | Use `a.b` or `a[0].b` only |
| NIKA-091 | JSONPath invalid index | Array index must be non-negative integer |
| NIKA-092 | JSONPath empty | Path cannot be empty string |

---

## Appendix A: Rust Types

```rust
use serde_json::Value;
use rustc_hash::FxHashMap;

/// Use block - map of alias to entry
pub type UseWiring = FxHashMap<String, UseEntry>;

/// Unified use entry
///
/// Syntax: `task.path [?? default]`
/// - path: "task.field.subfield" or "task" for entire output
/// - default: Optional JSON literal after ??
#[derive(Debug, Clone)]
pub struct UseEntry {
    /// Full path: "task.field.subfield"
    pub path: String,
    /// Optional default value (JSON literal)
    pub default: Option<Value>,
}

impl UseEntry {
    /// Extract the task ID from the path (first segment before '.')
    pub fn task_id(&self) -> &str {
        self.path.split('.').next().unwrap_or(&self.path)
    }
}

/// Task ID validation
/// Pattern: ^[a-z][a-z0-9_]*$ (snake_case)
pub fn validate_task_id(id: &str) -> Result<(), NikaError>;
```

---

## Appendix B: Template Resolution (Strict Mode)

```rust
/// Convert JSON Value to string (strict mode)
fn value_to_string(value: &Value, path: &str, alias: &str) -> Result<String, NikaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Err(NikaError::NullValue {
            path: path.to_string(),
            alias: alias.to_string(),
        }),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        other => Ok(other.to_string()), // Compact JSON
    }
}
```

---

## Appendix C: Static Validation

```rust
/// Extract all alias references from a template
pub fn extract_refs(template: &str) -> Vec<(String, String)>;

/// Validate all refs exist in declared aliases
pub fn validate_refs(
    template: &str,
    declared_aliases: &HashSet<String>,
    task_id: &str,
) -> Result<(), NikaError>;
```

---

## Appendix D: JSONPath (Minimal)

```rust
/// Path segment (field or array index)
pub enum Segment {
    Field(String),
    Index(usize),
}

/// Parse JSONPath string into segments
/// Supports: $.a.b, $.a[0].b, a.b (without $)
pub fn parse(path: &str) -> Result<Vec<Segment>, NikaError>;

/// Apply segments to JSON value
pub fn apply(value: &Value, segments: &[Segment]) -> Option<Value>;

/// Resolve path in one step (parse + apply)
pub fn resolve(value: &Value, path: &str) -> Result<Option<Value>, NikaError>;
```

**Examples:**
```
$.price.currency  → [Field("price"), Field("currency")]
$.items[0].name   → [Field("items"), Index(0), Field("name")]
data.temp         → [Field("data"), Field("temp")]
```
