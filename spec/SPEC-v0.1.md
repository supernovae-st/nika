# Nika v0.1 Specification

> **Version**: 0.1 | **Status**: Draft | **Date**: 2025-01-01

---

## TL;DR

```yaml
tasks:
  - id: analyze
    use:                                    # Explicit data wiring
      summary: weather.forecast             # Form 1: alias: task.path
      flights.cheapest: [price, airline]    # Form 2: batch extraction
      fallback:                             # Form 3: advanced
        from: weather
        path: backup
        default: "N/A"
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

Three syntactic forms for different needs:

### Form 1: Simple Path (String)

```yaml
use:
  weather: forecast.summary      # use.weather = "Sunny 25C"
  price: flights.cheapest.price  # use.price = 89
```

### Form 2: Batch Extraction (Array)

```yaml
use:
  flights.cheapest: [price, airline, departure]
  # Creates: use.price, use.airline, use.departure
```

### Form 3: Advanced (Object)

```yaml
use:
  forecast:
    from: weather_task          # Source task (required)
    path: data.summary          # Optional: path within output
    default: "Unknown"          # Optional: fallback if null/missing
```

**Path syntax:** `task_id.field.subfield.0` (dot-separated, numeric = array index)

---

## 3. Strict Mode (v0.1 Default)

Nika v0.1 uses **strict null handling** to prevent silent bugs:

| Scenario | Behavior |
|----------|----------|
| Path resolves to `null` | Error NIKA-072 (unless `default:` provided) |
| Path not found | Error NIKA-052 (unless `default:` provided) |
| Traverse non-object | Error NIKA-073 |
| Unknown template alias | Error NIKA-071 |

**With default:**
```yaml
use:
  price:
    from: flight
    path: cost
    default: 0        # Used if null or missing
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
      forecast: weather.summary
      flights.cheapest: [price, airline]
    infer:
      prompt: |
        Weather: {{use.forecast}}
        Best: {{use.airline}} for ${{use.price}}

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
- Path syntax valid
- No duplicate aliases
- All `{{use.X}}` have corresponding `use:` entry
- Referenced tasks exist in DAG

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
| NIKA-052 | Path not found | Add `default:` or fix output |
| NIKA-060 | Invalid JSON | Ensure valid JSON output |
| NIKA-061 | Schema failed | Fix output to match schema |
| NIKA-070 | Duplicate alias | Use unique names in `use:` |
| NIKA-071 | Unknown alias | Declare in `use:` before referencing |
| NIKA-072 | Null value | Provide `default:` or ensure non-null |
| NIKA-073 | Invalid traversal | Cannot access `.field` on primitive |
| NIKA-074 | Template parse error | Check `{{use.alias}}` syntax |

---

## Appendix A: Rust Types

```rust
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Use block - map of alias to entry
pub type UseBlock = HashMap<String, UseEntry>;

/// Three forms (order matters for serde untagged)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UseEntry {
    /// Form 1: "alias: task.path"
    Path(String),
    /// Form 2: "task.path: [field1, field2]"
    Batch(Vec<String>),
    /// Form 3: "alias: { from, path, default }"
    Advanced(UseAdvanced),
}

#[derive(Debug, Clone, Deserialize)]
pub struct UseAdvanced {
    /// Source task ID (required)
    pub from: String,
    /// Optional path within output
    #[serde(default)]
    pub path: Option<String>,
    /// Optional default if null/missing
    #[serde(default)]
    pub default: Option<Value>,
}
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
