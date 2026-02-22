# Unified `use:` Syntax Design

> Simplify the 3-form `use:` system into a single unified syntax.

**Status:** Approved
**Date:** 2026-01-02
**Affects:** `src/binding/entry.rs`, `src/binding/resolve.rs`, `src/dag/validate.rs`

---

## Problem

The current `use:` block has 3 forms:

```yaml
# Form 1: Path (simple)
forecast: weather.summary

# Form 2: Batch (magic aliases)
flights.cheapest: [price, airline]  # Creates use.price, use.airline

# Form 3: Advanced (verbose)
temp:
  from: weather
  path: data.temp
  default: 20
```

**Issues:**

1. **Cognitive load** - 3 different syntaxes to learn
2. **Code complexity** - `enum UseEntry` with 3 variants
3. **Form 2 magic** - aliases appear without explicit declaration
4. **Inconsistent** - different patterns for same concept

---

## Solution

**One unified syntax:**

```
alias: task.path [?? default]
```

### Examples

```yaml
use:
  # Simple path (unchanged)
  forecast: weather.summary

  # Nested path
  temp: weather.data.temperature

  # Array index
  first_item: results.items[0].name

  # Entire task output
  raw_data: weather

  # With default (number)
  score: game.stats.score ?? 0

  # With default (string - MUST be quoted)
  name: user.profile.name ?? "Anonymous"

  # With default (object)
  config: settings.options ?? {"debug": false, "verbose": true}

  # With default (array)
  tags: metadata.tags ?? ["untagged"]
```

### Migration from 3 forms

| Old Form | New Syntax |
|----------|------------|
| `forecast: weather.summary` | `forecast: weather.summary` (unchanged) |
| `flights.cheapest: [price, airline]` | `price: flights.cheapest.price`<br>`airline: flights.cheapest.airline` |
| `temp: { from: weather, path: data.temp, default: 20 }` | `temp: weather.data.temp ?? 20` |

---

## Parsing Rules

### Task ID Validation

```regex
^[a-z][a-z0-9_]*$
```

| Task ID | Valid | Reason |
|---------|-------|--------|
| `weather` | Yes | |
| `get_data` | Yes | underscore OK |
| `fetch_api` | Yes | underscore OK |
| `fetch-api` | No | dash forbidden |
| `myTask` | No | uppercase forbidden |
| `weather.api` | No | **dot forbidden** |

**Rationale:** Dots are reserved for path separator in `task.field.subfield`.

### Default Value Parsing

The default value after `??` is parsed as **JSON literal**:

| Syntax | Parsed Value | Type |
|--------|--------------|------|
| `?? 0` | `0` | number |
| `?? "0"` | `"0"` | string |
| `?? true` | `true` | bool |
| `?? "true"` | `"true"` | string |
| `?? "Anonymous"` | `"Anonymous"` | string |
| `?? Anonymous` | ERROR | invalid JSON |
| `?? {"a": 1}` | `{"a": 1}` | object |
| `?? ["a", "b"]` | `["a", "b"]` | array |
| `?? null` | `null` | null |

**Rule:** Strings MUST be quoted. This avoids ambiguity and enables edge cases like `?? "What?? Really??"`.

### Quote-Aware Parsing

```
Input:  user.name ?? "What?? Really??"

Step 1: Find ?? OUTSIDE quotes
        user.name ?? "What?? Really??"
        ^^^^^^^^^^   ^^^^^^^^^^^^^^^^^^
        outside      inside quotes (ignored)
                  ^
                  Split here

Step 2: Extract
        path    = "user.name"
        default = "\"What?? Really??\""

Step 3: Parse default as JSON
        serde_json::from_str("\"What?? Really??\"")
        → Value::String("What?? Really??")
```

---

## Implementation

### Rust Types (Before)

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum UseEntry {
    Path(String),
    Batch(Vec<String>),
    Advanced(UseAdvanced),
}

pub struct UseAdvanced {
    pub from: String,
    pub path: Option<String>,
    pub default: Option<Value>,
}
```

### Rust Types (After)

```rust
/// Unified use entry - single form
#[derive(Debug, Clone)]
pub struct UseEntry {
    /// Full path: "task.field.subfield" or "task" for entire output
    pub path: String,
    /// Optional default value (JSON)
    pub default: Option<Value>,
}
```

### Custom Deserializer

```rust
impl<'de> Deserialize<'de> for UseEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        parse_use_entry(&s).map_err(serde::de::Error::custom)
    }
}

fn parse_use_entry(s: &str) -> Result<UseEntry, NikaError> {
    if let Some(idx) = find_operator_outside_quotes(s, "??") {
        let path = s[..idx].trim().to_string();
        let default_str = s[idx + 2..].trim();

        let default: Value = serde_json::from_str(default_str)
            .map_err(|e| NikaError::InvalidDefault {
                raw: default_str.to_string(),
                reason: e.to_string(),
            })?;

        Ok(UseEntry { path, default: Some(default) })
    } else {
        Ok(UseEntry { path: s.trim().to_string(), default: None })
    }
}

fn find_operator_outside_quotes(s: &str, op: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut chars = s.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if !in_quotes && s[i..].starts_with(op) {
            return Some(i);
        }
    }
    None
}
```

### Task ID Validation

```rust
use once_cell::sync::Lazy;
use regex::Regex;

static TASK_ID_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-z][a-z0-9_]*$").unwrap()
});

pub fn validate_task_id(id: &str) -> Result<(), NikaError> {
    if !TASK_ID_REGEX.is_match(id) {
        return Err(NikaError::InvalidTaskId {
            id: id.to_string(),
            reason: "must be snake_case: lowercase letters, digits, underscores only".into(),
        });
    }
    Ok(())
}
```

---

## Error Codes

| Code | Error | Example |
|------|-------|---------|
| NIKA-050 | Invalid path syntax | `use: { x: "no.task" }` where task doesn't exist |
| NIKA-055 | Invalid task ID | `id: "my-task"` (dash) or `id: "myTask"` (uppercase) |
| NIKA-056 | Invalid default JSON | `name: user ?? Anonymous` (unquoted string) |
| NIKA-072 | Null value without default | Path resolves to null, no `??` provided |

---

## Files to Modify

1. **`src/binding/entry.rs`** - Replace enum with struct, add custom deserializer
2. **`src/binding/resolve.rs`** - Simplify to single code path
3. **`src/dag/validate.rs`** - Remove Form 2/3 handling, add task ID validation
4. **`src/ast/workflow.rs`** - Add task ID validation on parse
5. **`src/error.rs`** - Add NIKA-055, NIKA-056 error variants
6. **`examples/use-output-demo.nika.yaml`** - Fix broken `pick:` example

---

## Testing Checklist

- [ ] Parse simple path: `forecast: weather.summary`
- [ ] Parse with default number: `score: game.score ?? 0`
- [ ] Parse with default string: `name: user.name ?? "Anon"`
- [ ] Parse with default object: `cfg: x ?? {"a": 1}`
- [ ] Parse with default array: `tags: x ?? ["a"]`
- [ ] Parse with `??` inside quotes: `x: y ?? "What?? Really??"`
- [ ] Reject unquoted string default: `x: y ?? Anonymous` → ERROR
- [ ] Reject invalid task ID: `my-task` → NIKA-055
- [ ] Reject task ID with dot: `weather.api` → NIKA-055
- [ ] Resolve path at runtime
- [ ] Use default when path not found
- [ ] Use default when value is null
- [ ] Template substitution: `{{use.forecast}}`
