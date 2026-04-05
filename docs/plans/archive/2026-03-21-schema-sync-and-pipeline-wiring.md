# Schema Sync & Pipeline Wiring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the JSON schema drift that blocks 7 implemented features, wire 3 agent features through the AST pipeline, and add gate examples + tests for all fixed features.

**Architecture:** The JSON schema (`schemas/nika-workflow.schema.json`) runs as a hard gate BEFORE Rust parsing in both `nika check` and `nika run`. Features exist in the Rust AST but are blocked by `additionalProperties: false` in the schema. Part 1 fixes the schema (4 features). Part 2 wires 3 agent features through the raw parser -> analyzed AST -> lower pipeline. Part 3 adds gate examples and tests.

**Tech Stack:** JSON Schema Draft-07, Rust (serde, marked_yaml), nika AST 3-phase pipeline

**Key files:**
- Schema: `tools/nika/schemas/nika-workflow.schema.json`
- Raw AST: `tools/nika/src/ast/raw/action.rs`
- Raw Parser: `tools/nika/src/ast/raw/parser.rs`
- Analyzed AST: `tools/nika/src/ast/analyzed/task.rs`
- Analyzer: `tools/nika/src/ast/analyzer/analyze.rs`
- Lower: `tools/nika/src/ast/lower.rs`
- Runtime: `tools/nika/src/ast/agent.rs`
- Schema Validator Tests: `tools/nika/src/ast/schema_validator.rs`
- Gate Examples: `tools/nika/examples/gates/feature/`

---

## Part 1: Schema-Only Fixes (4 features)

These features have COMPLETE Rust pipelines. Only the JSON schema blocks them.

### Task 1: Add `GuardrailConfig` $def to JSON schema

**Files:**
- Modify: `tools/nika/schemas/nika-workflow.schema.json`

**Step 1: Add GuardrailConfig definition**

Add a new `$def` entry after `LogConfig` (after line ~941, before `AgentDef`). The Rust source is at `src/ast/guardrails.rs:88-102` -- a `#[serde(tag = "type")]` enum with 4 variants (length, schema, regex, llm).

```json
"GuardrailConfig": {
  "type": "object",
  "description": "Guardrail for validating outputs (v0.12+)",
  "required": ["type"],
  "properties": {
    "type": {
      "type": "string",
      "enum": ["length", "schema", "regex", "llm"]
    },
    "id": { "type": "string" },
    "on_failure": {
      "type": "string",
      "enum": ["retry", "escalate", "fail"],
      "default": "retry"
    },
    "min_words": { "type": "integer", "minimum": 0 },
    "max_words": { "type": "integer", "minimum": 0 },
    "min_chars": { "type": "integer", "minimum": 0 },
    "max_chars": { "type": "integer", "minimum": 0 },
    "message": { "type": "string" },
    "json_schema": { "type": "object" },
    "pattern": { "type": "string" },
    "negate": { "type": "boolean", "default": false },
    "judge_prompt": { "type": "string" },
    "pass_pattern": { "type": "string" },
    "judge_model": { "type": "string" },
    "judge_provider": { "type": "string" }
  }
}
```

**Step 2: Verify the schema is valid JSON**

Run: `python3 -c "import json; json.load(open('tools/nika/schemas/nika-workflow.schema.json'))"`
Expected: No output (valid JSON)

---

### Task 2: Add `guardrails` to InferParams in JSON schema

**Files:**
- Modify: `tools/nika/schemas/nika-workflow.schema.json` (InferParams object variant, lines 344-387)

**Step 1: Add guardrails property to InferParams**

In the InferParams object variant (inside `oneOf[1].properties`, after `content` at line ~387), add:

```json
"guardrails": {
  "type": "array",
  "description": "Guardrails for validating infer output (v0.12+)",
  "items": { "$ref": "#/$defs/GuardrailConfig" }
}
```

**Step 2: Add provider and model to InferParams**

In the same properties block, add:

```json
"provider": {
  "type": "string",
  "description": "Provider override for this infer task"
},
"model": {
  "type": "string",
  "description": "Model override for this infer task"
}
```

The Rust source confirms these exist: `src/ast/raw/action.rs` has `provider` (line 194) and `model` (line 197) in `RawInferAction` (wait -- actually those are in RawAgentAction, let me check). Actually the raw parser `parse_infer_action` does NOT parse provider/model from inside infer. They are parsed at task level by `parse_task`. So `provider`/`model` in `InferParams` full-form is a Rust AST field (`action.rs:60-62`) but the raw parser reads them at task level, not inside infer. The JSON schema already has them at task level (schema line 273-282). Adding them inside InferParams too would be for consistency with the runtime struct, but the raw parser would ignore them. **Skip adding provider/model to InferParams** -- they work correctly at task level already.

**Step 3: Verify**

Run: `python3 -c "import json; json.load(open('tools/nika/schemas/nika-workflow.schema.json'))"`

---

### Task 3: Add `resource` to InvokeParams in JSON schema

**Files:**
- Modify: `tools/nika/schemas/nika-workflow.schema.json` (InvokeParams, lines 550-594)

**Step 1: Add resource property**

In InvokeParams properties (after `timeout` at line ~575), add:

```json
"resource": {
  "type": "string",
  "description": "MCP resource URI to read (mutually exclusive with 'tool')"
}
```

**Step 2: Add resource-based oneOf variants**

In the `oneOf` array (lines 576-593), add two new variants for resource:

```json
{ "required": ["mcp", "resource"] },
{ "required": ["server", "resource"] }
```

**Step 3: Verify JSON validity**

Run: `python3 -c "import json; json.load(open('tools/nika/schemas/nika-workflow.schema.json'))"`

---

### Task 4: Add `CompletionConfig` and `LimitsConfig` $defs to JSON schema

Even though the pipeline is not wired yet (Part 2), we add the schema definitions now so they're ready.

**Files:**
- Modify: `tools/nika/schemas/nika-workflow.schema.json`

**Step 1: Add CompletionConfig $def**

Rust source: `src/ast/completion.rs:56` -- struct with mode, signal, patterns, confidence, instruction.

```json
"CompletionConfig": {
  "type": "object",
  "description": "Agent completion behavior (v0.12+)",
  "properties": {
    "mode": {
      "type": "string",
      "enum": ["explicit", "natural", "pattern"],
      "default": "natural"
    },
    "signal": {
      "type": "object",
      "properties": {
        "tool": { "type": "string", "default": "nika:complete" },
        "fields": {
          "type": "object",
          "properties": {
            "required": { "type": "array", "items": { "type": "string" } },
            "optional": { "type": "array", "items": { "type": "string" } }
          }
        }
      }
    },
    "patterns": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "regex": { "type": "string" },
          "name": { "type": "string" }
        }
      }
    },
    "confidence": {
      "type": "object",
      "properties": {
        "threshold": { "type": "number", "minimum": 0, "maximum": 1, "default": 0.7 },
        "max_retries": { "type": "integer", "minimum": 0, "default": 2 }
      }
    }
  }
}
```

**Step 2: Add LimitsConfig $def**

Rust source: `src/ast/limits.rs:41`.

```json
"LimitsConfig": {
  "type": "object",
  "description": "Agent execution limits for cost control (v0.12+)",
  "properties": {
    "max_turns": { "type": "integer", "minimum": 0 },
    "max_tokens": { "type": "integer", "minimum": 0 },
    "max_cost_usd": { "type": "number", "minimum": 0 },
    "max_duration_secs": { "type": "integer", "minimum": 0 },
    "on_limit_reached": {
      "type": "object",
      "properties": {
        "action": {
          "type": "string",
          "enum": ["complete_partial", "fail", "escalate"],
          "default": "complete_partial"
        },
        "save_progress": { "type": "boolean", "default": true },
        "message": { "type": "string" }
      }
    }
  }
}
```

**Step 3: Add all three to AgentParams properties**

In AgentParams properties (after `scope` at line ~691), add:

```json
"completion": {
  "$ref": "#/$defs/CompletionConfig"
},
"guardrails": {
  "type": "array",
  "items": { "$ref": "#/$defs/GuardrailConfig" }
},
"limits": {
  "$ref": "#/$defs/LimitsConfig"
}
```

**Step 4: Verify JSON validity**

Run: `python3 -c "import json; json.load(open('tools/nika/schemas/nika-workflow.schema.json'))"`

---

### Task 5: Schema validation tests

**Files:**
- Modify: `tools/nika/src/ast/schema_validator.rs` (tests section, after line ~712)

**Step 1: Add test for infer guardrails**

```rust
#[test]
fn test_valid_infer_with_guardrails_passes() {
    let yaml = r#"
schema: nika/workflow@0.12
provider: mock
tasks:
  - id: guarded
    infer:
      prompt: "Generate content"
      guardrails:
        - type: length
          min_words: 100
          max_words: 500
          on_failure: retry
        - type: regex
          pattern: "(?i)conclusion"
          on_failure: fail
"#;
    let validator = WorkflowSchemaValidator::new().unwrap();
    assert!(validator.validate_yaml(yaml).is_ok());
}
```

**Step 2: Add test for agent with completion + limits + guardrails**

```rust
#[test]
fn test_valid_agent_with_completion_limits_guardrails_passes() {
    let yaml = r#"
schema: nika/workflow@0.12
provider: mock
tasks:
  - id: advanced_agent
    agent:
      prompt: "Research topic"
      tools: [builtin]
      max_turns: 10
      completion:
        mode: explicit
      limits:
        max_cost_usd: 1.0
        max_duration_secs: 120
        on_limit_reached:
          action: complete_partial
      guardrails:
        - type: length
          min_words: 200
          on_failure: retry
"#;
    let validator = WorkflowSchemaValidator::new().unwrap();
    assert!(validator.validate_yaml(yaml).is_ok());
}
```

**Step 3: Add test for invoke with resource**

```rust
#[test]
fn test_valid_invoke_with_resource_passes() {
    let yaml = r#"
schema: nika/workflow@0.12
provider: mock
tasks:
  - id: read_resource
    invoke:
      mcp: novanet
      resource: "schema://entities"
"#;
    let validator = WorkflowSchemaValidator::new().unwrap();
    assert!(validator.validate_yaml(yaml).is_ok());
}
```

**Step 4: Run schema validator tests**

Run: `cd tools/nika && cargo test --lib schema_validator`
Expected: All tests pass including the 3 new ones.

**Step 5: Commit**

```bash
git add tools/nika/schemas/nika-workflow.schema.json tools/nika/src/ast/schema_validator.rs
git commit -m "fix(schema): sync JSON schema with Rust AST -- add guardrails, completion, limits, resource

Add missing $defs: GuardrailConfig, CompletionConfig, LimitsConfig.
Add guardrails to InferParams and AgentParams.
Add completion and limits to AgentParams.
Add resource to InvokeParams.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Part 2: Pipeline Wiring (3 agent features)

Wire `guardrails`, `completion`, and `limits` through the agent AST pipeline:
Raw parser -> Analyzed AST -> Lower -> Runtime

### Task 6: Add fields to RawAgentAction

**Files:**
- Modify: `tools/nika/src/ast/raw/action.rs` (RawAgentAction struct, lines 172-228)

**Step 1: Add the 3 fields to RawAgentAction**

After `scope` (line 227), before the closing `}` (line 228), add:

```rust
    /// Guardrails for validating agent outputs.
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
    /// Completion behavior configuration.
    pub completion: Option<crate::ast::completion::CompletionConfig>,
    /// Execution limits for cost control.
    pub limits: Option<crate::ast::limits::LimitsConfig>,
```

Note: `RawAgentAction` derives `Default`, so `Vec` defaults to empty and `Option` defaults to `None`. No changes to Default impl needed.

**Step 2: Verify it compiles**

Run: `cd tools/nika && cargo check 2>&1 | head -20`
Expected: Compilation errors in `parse_agent_action` (missing fields) -- this is expected, we fix it in Task 7.

---

### Task 7: Parse the 3 fields in parse_agent_action

**Files:**
- Modify: `tools/nika/src/ast/raw/parser.rs` (parse_agent_action, lines 837-884)

**Step 1: Add parsing logic**

In `parse_agent_action`, after `scope` (line 882), before the closing `})` (line 883), add:

```rust
        guardrails: parse_guardrails_field(file, m)?,
        completion: parse_optional_serde_field(file, m, "completion")?,
        limits: parse_optional_serde_field(file, m, "limits")?,
```

**Step 2: Add helper function `parse_optional_serde_field`**

After `parse_guardrails_field` (line 1169), add a generic helper for serde-deserializable fields:

```rust
fn parse_optional_serde_field<T: serde::de::DeserializeOwned>(
    file: FileId,
    map: &marked_yaml::types::MarkedMappingNode,
    field_name: &str,
) -> Result<Option<T>, ParseError> {
    match map.get_node(field_name) {
        Some(node) => {
            let span = node_to_span(file, node);
            let json_value = node_to_json(node);
            let parsed = serde_json::from_value(json_value).map_err(|e| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("invalid {field_name} config: {e}"),
            })?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}
```

This follows the exact same pattern as `parse_guardrails_field` (line 1153-1169) but returns `Option<T>` instead of `Vec<T>`.

**Step 3: Verify it compiles**

Run: `cd tools/nika && cargo check 2>&1 | head -20`
Expected: Errors in `analyze_agent` (missing fields) -- expected, fix in Task 8.

---

### Task 8: Add fields to AnalyzedAgentAction and analyze_agent

**Files:**
- Modify: `tools/nika/src/ast/analyzed/task.rs` (AnalyzedAgentAction, lines 282-335)
- Modify: `tools/nika/src/ast/analyzer/analyze.rs` (analyze_agent, lines 778-814)

**Step 1: Add fields to AnalyzedAgentAction**

After `scope` (line 331), before `span` (line 334), add:

```rust
    /// Guardrails for validating agent outputs.
    pub guardrails: Vec<crate::ast::guardrails::GuardrailConfig>,
    /// Completion behavior configuration.
    pub completion: Option<crate::ast::completion::CompletionConfig>,
    /// Execution limits for cost control.
    pub limits: Option<crate::ast::limits::LimitsConfig>,
```

**Step 2: Wire in analyze_agent**

In `analyze_agent` (line 778), after `scope` (line 811), before `span` (line 812), add:

```rust
        guardrails: raw.guardrails.clone(),
        completion: raw.completion.clone(),
        limits: raw.limits.clone(),
```

These pass through directly since `GuardrailConfig`, `CompletionConfig`, and `LimitsConfig` don't contain `Spanned` wrappers (they use serde deserialization, not the span-tracking parser).

**Step 3: Verify it compiles**

Run: `cd tools/nika && cargo check 2>&1 | head -20`
Expected: Errors in `lower_agent` about field mismatch -- expected, fix in Task 9.

---

### Task 9: Wire lower_agent to forward instead of hardcoding

**Files:**
- Modify: `tools/nika/src/ast/lower.rs` (lower_agent, lines 286-288)

**Step 1: Replace hardcoded stubs**

Change lines 286-288 from:

```rust
        completion: None,
        guardrails: Vec::new(),
        limits: None,
```

To:

```rust
        completion: agent.completion,
        guardrails: agent.guardrails,
        limits: agent.limits,
```

**Step 2: Verify it compiles and all tests pass**

Run: `cd tools/nika && cargo check && cargo test --lib`
Expected: All compile, all tests pass.

**Step 3: Commit**

```bash
git add tools/nika/src/ast/raw/action.rs tools/nika/src/ast/raw/parser.rs \
        tools/nika/src/ast/analyzed/task.rs tools/nika/src/ast/analyzer/analyze.rs \
        tools/nika/src/ast/lower.rs
git commit -m "feat(ast): wire guardrails, completion, limits through agent pipeline

Add guardrails, completion, limits fields to RawAgentAction.
Parse them in parse_agent_action via serde deserialization.
Carry through AnalyzedAgentAction in analyze_agent.
Forward in lower_agent instead of hardcoding None/empty.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Part 3: Gate Examples & E2E Tests

### Task 10: Gate examples for all fixed features

**Files:**
- Create: `tools/nika/examples/gates/feature/infer-guardrails.nika.yaml`
- Create: `tools/nika/examples/gates/feature/agent-guardrails.nika.yaml`
- Create: `tools/nika/examples/gates/feature/agent-completion-explicit.nika.yaml`
- Create: `tools/nika/examples/gates/feature/agent-limits.nika.yaml`
- Create: `tools/nika/examples/gates/feature/invoke-resource.nika.yaml`
- Create: `tools/nika/examples/gates/feature/response-format-json.nika.yaml`
- Create: `tools/nika/examples/gates/feature/tool-choice-required.nika.yaml`

**Step 1: Create infer-guardrails.nika.yaml**

```yaml
# FEATURE: guardrails on infer verb
schema: nika/workflow@0.12
workflow: feat-infer-guardrails
tasks:
  - id: guarded
    infer:
      prompt: "Write a short conclusion about AI workflow engines."
      guardrails:
        - type: length
          min_words: 10
          max_words: 100
          on_failure: retry
        - type: regex
          pattern: "(?i)(workflow|engine|ai)"
          on_failure: fail
    provider: openai
    model: gpt-4.1-mini
    max_tokens: 200
```

**Step 2: Create agent-guardrails.nika.yaml**

```yaml
# FEATURE: guardrails on agent verb
schema: nika/workflow@0.12
workflow: feat-agent-guardrails
tasks:
  - id: guarded_agent
    agent:
      prompt: "Write exactly 3 bullet points about Rust. Then call nika_complete."
      tools: [builtin]
      max_turns: 3
      max_tokens: 200
      guardrails:
        - type: length
          min_words: 20
          max_words: 200
          on_failure: retry
    provider: openai
    model: gpt-4.1-mini
```

**Step 3: Create agent-completion-explicit.nika.yaml**

```yaml
# FEATURE: completion mode on agent
schema: nika/workflow@0.12
workflow: feat-agent-completion
tasks:
  - id: explicit_agent
    agent:
      prompt: "Say hello and call nika_complete with your greeting."
      tools: [builtin]
      max_turns: 3
      max_tokens: 100
      completion:
        mode: explicit
    provider: openai
    model: gpt-4.1-mini
```

**Step 4: Create agent-limits.nika.yaml**

```yaml
# FEATURE: limits on agent
schema: nika/workflow@0.12
workflow: feat-agent-limits
tasks:
  - id: limited_agent
    agent:
      prompt: "Say hello and call nika_complete."
      tools: [builtin]
      max_turns: 5
      max_tokens: 100
      limits:
        max_cost_usd: 0.50
        max_duration_secs: 60
        on_limit_reached:
          action: complete_partial
    provider: openai
    model: gpt-4.1-mini
```

**Step 5: Create invoke-resource.nika.yaml**

```yaml
# FEATURE: invoke with resource (MCP resource read)
schema: nika/workflow@0.12
workflow: feat-invoke-resource
provider: mock
mcp:
  novanet:
    command: echo
    args: ["test"]
tasks:
  - id: read_resource
    invoke:
      mcp: novanet
      resource: "schema://entities"
```

**Step 6: Create response-format-json.nika.yaml**

```yaml
# FEATURE: response_format on infer
schema: nika/workflow@0.12
workflow: feat-response-format
tasks:
  - id: json_response
    infer:
      prompt: "Return a JSON object with key 'greeting' and value 'hello'"
      response_format: json
    provider: openai
    model: gpt-4.1-mini
    max_tokens: 50
```

**Step 7: Create tool-choice-required.nika.yaml**

```yaml
# FEATURE: tool_choice on agent
schema: nika/workflow@0.12
workflow: feat-tool-choice
tasks:
  - id: forced_tool
    agent:
      prompt: "Call nika_complete immediately with result 'done'"
      tools: [builtin]
      max_turns: 2
      max_tokens: 100
      tool_choice: required
    provider: openai
    model: gpt-4.1-mini
```

**Step 8: Validate all gate examples**

Run: `for f in tools/nika/examples/gates/feature/{infer-guardrails,agent-guardrails,agent-completion-explicit,agent-limits,invoke-resource,response-format-json,tool-choice-required}.nika.yaml; do echo "=== $f ===" && nika check "$f" 2>&1 | tail -3; done`
Expected: All pass validation.

**Step 9: Commit**

```bash
git add tools/nika/examples/gates/feature/
git commit -m "test(gates): add gate examples for guardrails, completion, limits, resource, response_format, tool_choice

7 new gate examples covering features that were missing test coverage.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 11: Unit tests for pipeline wiring

**Files:**
- Modify: `tools/nika/src/ast/raw/parser.rs` (test module)
- Modify: `tools/nika/src/ast/lower.rs` (test module)

**Step 1: Add parser test for agent guardrails**

In `parser.rs` test module (after existing agent tests around line ~2319):

```rust
#[test]
fn test_parse_agent_with_guardrails() {
    let yaml = r#"
schema: nika/workflow@0.12
provider: mock
tasks:
  - id: guarded
    agent:
      prompt: "Test"
      guardrails:
        - type: length
          min_words: 10
          on_failure: retry
"#;
    let wf = parse(yaml).unwrap();
    let task = &wf.tasks[0];
    match &task.action {
        Some(crate::ast::raw::task::RawTaskAction::Agent(a)) => {
            assert_eq!(a.guardrails.len(), 1);
        }
        _ => panic!("expected agent action"),
    }
}
```

**Step 2: Add lower test for agent with guardrails**

In `lower.rs` test module (after `lower_agent_task` test around line ~1046):

```rust
#[test]
fn lower_agent_with_guardrails_forwards() {
    // Test that guardrails survive the lower phase
    let yaml = wrap("
  - id: g
    agent:
      prompt: test
      guardrails:
        - type: length
          min_words: 10
    ");
    let wf = ok(&yaml);
    let task = &wf.tasks[0];
    match &task.action {
        TaskAction::Agent(a) => {
            assert_eq!(a.guardrails.len(), 1);
        }
        _ => panic!("expected agent"),
    }
}
```

**Step 3: Run all tests**

Run: `cd tools/nika && cargo test --lib`
Expected: All pass.

**Step 4: Commit**

```bash
git add tools/nika/src/ast/raw/parser.rs tools/nika/src/ast/lower.rs
git commit -m "test(ast): add unit tests for agent guardrails/completion/limits pipeline

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 12: E2E validation with real providers

**Step 1: Run nika check on all new gates**

```bash
cd tools/nika
for f in examples/gates/feature/{infer-guardrails,agent-guardrails,agent-completion-explicit,agent-limits,invoke-resource,response-format-json,tool-choice-required}.nika.yaml; do
  echo "CHECK: $f"
  nika check "$f" 2>&1 | tail -1
done
```
Expected: All pass.

**Step 2: Run real e2e with OpenAI (cheapest reliable provider)**

```bash
# These cost real money (~$0.01 each with gpt-4.1-mini)
nika run examples/gates/feature/response-format-json.nika.yaml
nika run examples/gates/feature/tool-choice-required.nika.yaml
nika run examples/gates/feature/infer-guardrails.nika.yaml
```
Expected: All succeed with real LLM output.

**Step 3: Run full cargo test suite**

```bash
cd tools/nika && cargo test --lib
```
Expected: All 6846+ tests pass, including new ones.

**Step 4: Run clippy**

```bash
cd tools/nika && cargo clippy -- -D warnings
```
Expected: Zero warnings.

**Step 5: Final commit if any fixes needed**

---

## Summary

| Part | Tasks | Files Modified | Features Fixed |
|------|-------|---------------|----------------|
| **Part 1** | 1-5 | schema.json, schema_validator.rs | guardrails (infer), resource (invoke), schema defs |
| **Part 2** | 6-9 | action.rs, parser.rs, task.rs, analyze.rs, lower.rs | guardrails+completion+limits (agent pipeline) |
| **Part 3** | 10-12 | 7 gate examples, parser.rs tests, lower.rs tests | E2E validation + real provider runs |

**Total: 12 tasks, ~14 files modified/created**

**Estimated cost: ~$0.05 in API calls for e2e tests with gpt-4.1-mini**
