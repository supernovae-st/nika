---
name: nika-testing
description: >-
  Expert at testing Nika workflows without real API calls and validating
  outputs. Covers mock provider (deterministic, no keys, instant), --dry-run
  validation, nika test with golden snapshots, nika eval with assertion
  datasets, and intelligent test patterns (programmatic validation, EventLog
  checks, E2E pipeline). Use when testing Nika workflows, setting up CI, or
  writing eval datasets (schema nika/workflow@0.12).
globs:
  - "**/*.nika.yaml"
---

# Nika Workflow Testing

Test workflows without real API calls, validate outputs programmatically.

## Quick: mock Provider — Zero API Calls

```yaml
# In the workflow file:
schema: "nika/workflow@0.12"
provider: mock                # Deterministic responses, no key needed, instant

tasks:
  - id: step1
    infer: "Test prompt"      # Returns mock JSON immediately
```

Or override at runtime without changing the file:
```bash
nika run workflow.nika.yaml --provider mock
nika run workflow.nika.yaml --dry-run    # Validate only, never executes
```

## Test Modes

| Command | What it does | Use when |
|---------|-------------|---------|
| `--dry-run` | Validate YAML + DAG, no execution | Quick syntax check |
| `--provider mock` | Execute with deterministic mock responses | Test workflow logic |
| `nika test` | Full test run with mock provider | CI pipelines |
| `nika test --golden` | Compare output to snapshot file | Regression testing |
| `nika eval --dataset` | Run assertions on test cases | Quality evaluation |
| `nika check --strict` | Validate + test MCP connections | Pre-deploy validation |

## nika test — Snapshot Testing

```bash
# First run: create the golden snapshot
nika test workflow.nika.yaml --golden snap.json

# Subsequent runs: compare output to snapshot
nika test workflow.nika.yaml --golden snap.json

# Update the snapshot when output intentionally changes
nika test workflow.nika.yaml --golden snap.json --update-snapshot
```

**Golden file format** (auto-generated):
```json
{
  "task_outputs": {
    "step1": "expected output here",
    "step2": { "key": "expected value" }
  },
  "workflow_result": "final output"
}
```

## nika eval — Dataset-Based Evaluation

```bash
nika eval workflow.nika.yaml --dataset tests/data.json
nika eval workflow.nika.yaml --dataset data.json --provider anthropic --format json
```

**Dataset format:**
```json
[
  {
    "inputs": { "topic": "AI workflow engines" },
    "assertions": [
      { "task": "classify", "field": "category", "equals": "technology" },
      { "task": "summarize", "contains": "workflow" },
      { "task": "score", "field": "rating", "min": 1, "max": 5 }
    ]
  }
]
```

## Intelligent Testing — What to Validate

**Never test superficially:**

```yaml
# ❌ WEAK — proves nothing
assert!(result.is_ok())
assert!(!output.is_empty())

# ✅ STRONG — validate type, enum, range, structure
```

**Programmatic output validation:**

```yaml
# Prompt: natural language ONLY — never mention JSON
- id: classify
  infer: "Parle-moi d'Alice, 30 ans, développeuse Rust"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        age: { type: number, minimum: 0, maximum: 150 }
        skills: { type: array, items: { type: string }, minItems: 1 }
      required: [name, age, skills]
```

Then in tests:
```rust
// In Rust tests (nika-engine or integration):
let output: serde_json::Value = serde_json::from_str(&result)?;
assert!(output["name"].is_string());
assert!(output["age"].as_f64().unwrap() >= 0.0);
assert!(output["skills"].as_array().unwrap().len() >= 1);
```

## E2E Test Pattern

Test the complete pipeline: parse → analyze → execute → validate → verify events.

```bash
# 1. Validate syntax + DAG
nika check workflow.nika.yaml

# 2. Test with mock (fast, no API)
nika test workflow.nika.yaml

# 3. Lint for best practices and security
nika lint workflow.nika.yaml

# 4. Evaluate with real provider (CI gate)
nika eval workflow.nika.yaml --dataset tests/cases.json --provider anthropic
```

## CI Pipeline Setup

```yaml
# .github/workflows/nika-test.yml
- name: Validate all workflows
  run: nika check **/*.nika.yaml --strict

- name: Lint workflows
  run: nika lint **/*.nika.yaml

- name: Test with mock provider
  run: nika test workflows/*.nika.yaml

- name: Evaluate with assertions
  run: nika eval workflows/main.nika.yaml --dataset tests/eval-data.json
  env:
    ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

## Provider-Agnostic Testing

Test the same workflow on ALL providers — a failure on one is an ENGINE bug, not a provider limitation:

```bash
for provider in anthropic openai groq gemini; do
  echo "Testing with $provider..."
  nika eval workflow.nika.yaml --dataset tests/data.json --provider $provider
done
```

## EventLog — Testing for Correct Events

For Rust tests in nika-engine:

```rust
// Check events, not just output
let events = event_log.all();
assert!(events.iter().any(|e| matches!(e, Event::StructuredOutputSuccess { .. })));
// If StructuredOutputRepaired fires → schema failed first attempt (acceptable but track)
// If StructuredOutputFailed fires → NIKA-300, needs investigation
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| JSON mentioned in prompt for structured output | Keep prompts natural language only |
| Testing with only one provider | Test on ALL providers — failures = engine bugs |
| `assert!(!is_empty())` | Assert on type, enum, range, structure |
| No --golden snapshots in CI | Add `nika test --golden` to catch regressions |
| Committing `.nika/traces/` | Always gitignore `.nika/` — traces may contain secrets |

## Related Skills

- `/nika-structured` — 4-layer defense, prompt purity rule, schema patterns
- `/nika-validate` — nika check, nika lint, error parsing, auto-fix
- `/nika-security` — NIKA-380..389, API key management, trace security
