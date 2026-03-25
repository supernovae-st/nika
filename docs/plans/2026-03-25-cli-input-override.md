# Plan: CLI Input Override for `nika run`

> Pass workflow input parameters directly from the CLI.
> ~80 LOC in `main.rs` only — zero engine changes.

## Background

The `inputs:` workflow field is fully implemented (parser, analyzer, runner,
template resolution). But there's no way to override input defaults from the CLI.
The course documentation already references `--input name=value` as a planned feature.

## Syntax

```bash
nika run workflow.nika.yaml \
  -i locale=fr-FR \
  -i count=5 \
  -i active=true \
  --input-file params.yaml \
  --input-file -              # stdin
```

## Merge Order (lowest → highest priority)

1. Workflow YAML `inputs:` defaults
2. `--input-file` values
3. `-i` / `--input` flags (CLI always wins)

---

## Task 1: Add CLI flags to `Commands::Run`

**File:** `nika/src/main.rs` (Commands enum)

```rust
Run {
    file: String,
    #[arg(short, long)]
    provider: Option<String>,
    #[arg(short, long)]
    model: Option<String>,
    /// Override workflow input: -i key=value (repeatable)
    #[arg(short = 'i', long = "input", value_name = "KEY=VALUE")]
    inputs: Vec<String>,
    /// Load inputs from JSON/YAML file (or "-" for stdin)
    #[arg(long, value_name = "FILE")]
    input_file: Option<String>,
    #[arg(long, default_value = "accept-edits")]
    permission: String,
}
```

Update the match arm to pass new fields to `run_workflow()`.

---

## Task 2: `parse_input_value()` — smart type coercion

**File:** `nika/src/main.rs` (new function)

```rust
fn parse_input_value(s: &str) -> serde_json::Value {
    match s {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ => {
            if let Ok(n) = s.parse::<i64>() {
                return json!(n);
            }
            if let Ok(n) = s.parse::<f64>() {
                return json!(n);
            }
            if (s.starts_with('{') || s.starts_with('['))
                && serde_json::from_str::<Value>(s).is_ok()
            {
                return serde_json::from_str(s).unwrap();
            }
            Value::String(s.to_string())
        }
    }
}
```

Rules (in order):
1. `"true"` / `"false"` → Bool
2. `"null"` → Null
3. Parseable as i64 → integer
4. Parseable as f64 → float
5. Starts with `{` or `[` → try JSON, fallback string
6. Everything else → String

---

## Task 3: `parse_cli_inputs()` — parse `-i key=value` flags

**File:** `nika/src/main.rs` (new function)

```rust
fn parse_cli_inputs(
    raw: &[String],
) -> Result<IndexMap<String, Value>, NikaError> {
    let mut result = IndexMap::new();
    for item in raw {
        let (key, value) = item.split_once('=').ok_or_else(|| {
            NikaError::ValidationError {
                reason: format!(
                    "Invalid input format '{}', expected KEY=VALUE", item
                ),
            }
        })?;
        result.insert(key.to_string(), parse_input_value(value));
    }
    Ok(result)
}
```

---

## Task 4: `load_input_file()` — load `--input-file`

**File:** `nika/src/main.rs` (new async function)

```rust
async fn load_input_file(
    path: &str,
) -> Result<IndexMap<String, Value>, NikaError> {
    let content = if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf).map_err(|e| {
            NikaError::IoError { details: format!("stdin: {}", e) }
        })?;
        buf
    } else {
        tokio::fs::read_to_string(path).await.map_err(|e| {
            NikaError::IoError { details: format!("'{}': {}", path, e) }
        })?
    };

    // Auto-detect format
    let value: Value = if path.ends_with(".json") || path == "-" {
        serde_json::from_str(&content).or_else(|_| {
            serde_yaml::from_str(&content)
        }).map_err(|e| NikaError::ParseError {
            details: format!("Invalid JSON/YAML in '{}': {}", path, e),
        })?
    } else {
        serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
            details: format!("Invalid YAML in '{}': {}", path, e),
        })?
    };

    // Must be a mapping at top level
    let map = value.as_object().ok_or_else(|| NikaError::ValidationError {
        reason: format!("Input file '{}' must be a JSON/YAML mapping", path),
    })?;

    Ok(map.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}
```

---

## Task 5: Wire into `run_workflow()`

**File:** `nika/src/main.rs` (modify existing function)

After `parse_workflow()` and before creating the Runner:

```rust
// Merge CLI input overrides (YAML < file < CLI)
if let Some(ref input_file_path) = input_file {
    let file_inputs = load_input_file(input_file_path).await?;
    for (k, v) in file_inputs {
        workflow.inputs.insert(k, v);
    }
}
if !cli_inputs.is_empty() {
    let parsed = parse_cli_inputs(&cli_inputs)?;
    for (k, v) in parsed {
        workflow.inputs.insert(k, v);
    }
}
```

---

## Task 6: Tests

Unit tests for `parse_input_value()`:
- `"hello"` → String
- `"5"` → integer
- `"3.14"` → float
- `"true"` / `"false"` → Bool
- `"null"` → Null
- `'{"a":1}'` → Object
- `'["x"]'` → Array
- `"{broken"` → String (fallback)
- `"5 apples"` → String

Integration test for `parse_cli_inputs()`:
- Valid `["k=v", "n=5"]` → correct map
- Missing `=` → error

---

## Task 7: Gate workflow + course doc update

New gate: `nika/examples/gates/feature/cli-input-override.nika.yaml`

```yaml
schema: "nika/workflow@0.12"
workflow: feat-cli-input-override
provider: mock
model: mock

inputs:
  greeting: "Hello"
  target: "World"
  count: 1

tasks:
  - id: greet
    exec: "echo '{{inputs.greeting}} {{inputs.target}} (x{{inputs.count}})'"
```

Update course exercise docs to remove "planned" note.

---

## Non-Goals

- Dot notation (`-i config.debug=true`) — YAGNI
- Input validation (warn on unknown keys) — V2
- Multiple `--input-file` flags — V2
- Environment variable inputs (`NIKA_INPUT_KEY=value`) — V2
