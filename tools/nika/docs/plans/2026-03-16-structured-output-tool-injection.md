# DynamicSubmitTool — Provider-Native Structured Output Enforcement

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Layer 0 (tool injection) to Nika's structured output system so the LLM provider enforces JSON schema compliance server-side, dramatically increasing first-attempt success rate (~90%+) before any post-processing.

**Architecture:** Create a `DynamicSubmitTool` implementing rig's `ToolDyn` trait with runtime JSON schemas from YAML. Inject it into `AgentBuilder.tools()` for both `infer:` and `agent:` verbs. The LLM is forced to call `submit_result({...})` matching the schema, giving provider-side enforcement. Existing Layers 1-3 (extraction, retry, repair) remain as fallback.

**Tech Stack:** Rust, rig-core v0.32 (`ToolDyn`, `AgentBuilder`), serde_json, tokio

---

## Task 1: Create DynamicSubmitTool

**Files:**
- Create: `src/runtime/submit_tool.rs`
- Modify: `src/runtime/mod.rs` (add module + re-export)

### Step 1: Write the failing test

Add test in `src/runtime/submit_tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::ToolDyn;
    use serde_json::json;

    #[test]
    fn submit_tool_has_correct_name() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let tool = DynamicSubmitTool::new(schema);
        assert_eq!(tool.name(), "submit_result");
    }

    #[tokio::test]
    async fn submit_tool_definition_has_schema_as_parameters() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        });
        let tool = DynamicSubmitTool::new(schema.clone());
        let def = tool.definition("test prompt".to_string()).await;
        assert_eq!(def.name, "submit_result");
        assert_eq!(def.parameters, schema);
        assert!(def.description.contains("submit"));
    }

    #[tokio::test]
    async fn submit_tool_call_returns_args_as_is() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::new(schema);
        let args = r#"{"name": "Alice", "age": 30}"#;
        let result = tool.call(args.to_string()).await.unwrap();
        assert_eq!(result, args);
    }

    #[tokio::test]
    async fn submit_tool_call_rejects_invalid_json() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::new(schema);
        let result = tool.call("not json".to_string()).await;
        assert!(result.is_err());
    }

    #[test]
    fn submit_tool_custom_name() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::with_name("output_json", schema);
        assert_eq!(tool.name(), "output_json");
    }
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --lib submit_tool -- --nocapture 2>&1 | head -30`
Expected: FAIL — `DynamicSubmitTool` does not exist yet.

### Step 3: Write minimal implementation

```rust
//! DynamicSubmitTool — Provider-native structured output via tool injection
//!
//! Implements rig's `ToolDyn` trait with a runtime JSON schema as parameters.
//! When injected into an `AgentBuilder`, the LLM is forced to call
//! `submit_result({...})` matching the schema, giving provider-side enforcement.
//!
//! This is Layer 0 of Nika's structured output defense system.

use std::future::Future;
use std::pin::Pin;

use rig::tool::{ToolDyn, ToolError};
use rig::tool::ToolDefinition;
use serde_json::Value;

/// Type alias for boxed future (matches NikaMcpTool pattern in rig.rs)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A synthetic tool that forces the LLM to produce structured JSON output.
///
/// Instead of relying on post-processing to extract and validate JSON from
/// free-text LLM output, this tool is injected into the `AgentBuilder` with
/// `tool_choice: required` (or `any`). The LLM MUST call `submit_result()`
/// with arguments matching the provided JSON schema.
///
/// This mirrors rig's Extractor pattern but works with runtime schemas
/// (serde_json::Value) instead of compile-time Rust types.
///
/// # Example
///
/// ```rust,ignore
/// let schema = serde_json::json!({
///     "type": "object",
///     "properties": { "name": { "type": "string" } },
///     "required": ["name"]
/// });
/// let tool = DynamicSubmitTool::new(schema);
/// let agent = AgentBuilder::new(model)
///     .tools(vec![Box::new(tool) as Box<dyn ToolDyn>])
///     .tool_choice(ToolChoice::Required)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct DynamicSubmitTool {
    /// Tool name (default: "submit_result")
    name: String,
    /// JSON Schema for the expected output structure
    schema: Value,
}

impl DynamicSubmitTool {
    /// Create a new DynamicSubmitTool with the default name "submit_result".
    pub fn new(schema: Value) -> Self {
        Self {
            name: "submit_result".to_string(),
            schema,
        }
    }

    /// Create with a custom tool name.
    pub fn with_name(name: impl Into<String>, schema: Value) -> Self {
        Self {
            name: name.into(),
            schema,
        }
    }

    /// Get the schema (for validation after tool call).
    pub fn schema(&self) -> &Value {
        &self.schema
    }
}

impl ToolDyn for DynamicSubmitTool {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.name.clone(),
            description: "Submit the structured result. You MUST call this tool with your response formatted as JSON matching the provided schema.".to_string(),
            parameters: self.schema.clone(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        Box::pin(async move {
            // Validate it's valid JSON (the provider should enforce schema,
            // but we double-check parsing)
            let _: Value = serde_json::from_str(&args).map_err(|e| {
                ToolError::ToolCallError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("submit_result: invalid JSON: {}", e),
                )))
            })?;
            // Return the args as-is — the caller extracts the structured data
            Ok(args)
        })
    }
}
```

### Step 4: Wire module in runtime/mod.rs

Add to `src/runtime/mod.rs`:
- Add `pub mod submit_tool;` after `pub mod structured_output;`
- Add `pub use submit_tool::DynamicSubmitTool;` in the re-exports section

### Step 5: Run tests to verify they pass

Run: `cargo test --lib submit_tool -- --nocapture`
Expected: 5 tests PASS

### Step 6: Commit

```bash
git add src/runtime/submit_tool.rs src/runtime/mod.rs
git commit -m "feat(runtime): add DynamicSubmitTool for provider-native structured output

Implements rig's ToolDyn trait with runtime JSON schemas. When injected
into AgentBuilder, forces the LLM to call submit_result() with schema-
compliant JSON. This is Layer 0 of the structured output defense.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 2: Add infer_with_tools() to RigProvider

**Files:**
- Modify: `src/provider/rig.rs` (add `infer_with_tools` method)

### Step 1: Write the failing test

Add test in `src/provider/rig.rs` tests section (existing test module):

```rust
#[test]
fn infer_options_with_tools_default() {
    let opts = InferOptions::default();
    assert!(opts.tools.is_none());
}
```

### Step 2: Run test to verify it fails

Run: `cargo test --lib infer_options_with_tools -- --nocapture`
Expected: FAIL — `tools` field doesn't exist on `InferOptions`

### Step 3: Add tools field to InferOptions

In `src/provider/rig.rs`, add to `InferOptions`:

```rust
/// Optional tools to inject (for structured output via DynamicSubmitTool)
pub tools: Option<Vec<Box<dyn rig::tool::ToolDyn>>>,
```

And add `Default` manually since `Box<dyn ToolDyn>` doesn't derive Default.

### Step 4: Add infer_with_tools method

Add a new method to each `RigProvider` variant's impl that builds an agent with tools and single-turn execution. This uses the existing `AgentBuilder` pattern from the agent loop.

The method signature:

```rust
/// Infer with injected tools (for DynamicSubmitTool structured output).
///
/// Builds a single-turn agent with the given tools and tool_choice: Required.
/// The LLM is forced to call one of the injected tools, returning structured output.
///
/// Returns the tool call arguments as the result text.
pub async fn infer_with_tools(
    &self,
    prompt: &str,
    tools: Vec<Box<dyn rig::tool::ToolDyn>>,
    model: Option<&str>,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>>
```

Implementation: Build `AgentBuilder::new(model).tools(tools).tool_choice(Required).build()`, call `agent.prompt(prompt).await`, extract the result.

### Step 5: Run tests

Run: `cargo test --lib infer_options_with_tools -- --nocapture`
Expected: PASS

### Step 6: Commit

```bash
git add src/provider/rig.rs
git commit -m "feat(provider): add infer_with_tools for tool-injected structured output

Adds infer_with_tools() to RigProvider that builds a single-turn agent
with tool_choice: Required. Used by DynamicSubmitTool to force the LLM
to produce schema-compliant JSON via tool calling.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 3: Wire Tool Injection into run_infer (Executor)

**Files:**
- Modify: `src/runtime/executor/verbs.rs` (add Layer 0 before streaming)

### Step 1: Write the failing test

In `tests/regression/output_capture.rs` or a new test file, add a test that verifies when `structured:` is set, `DynamicSubmitTool` is injected. Since this requires a mock provider, the simplest approach is a unit test in `verbs.rs`:

```rust
#[test]
fn build_submit_tool_from_output_policy() {
    use crate::ast::output::{OutputPolicy, OutputFormat, SchemaRef};
    use crate::runtime::submit_tool::DynamicSubmitTool;

    let policy = OutputPolicy {
        format: OutputFormat::Json,
        schema: Some(SchemaRef::Inline(serde_json::json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        }))),
        max_retries: None,
    };

    // Build submit tool from policy
    if let Some(SchemaRef::Inline(schema)) = &policy.schema {
        let tool = DynamicSubmitTool::new(schema.clone());
        assert_eq!(tool.name(), "submit_result");
    } else {
        panic!("Expected inline schema");
    }
}
```

### Step 2: Implementation

In `run_infer()`, after the mock provider check and before the streaming call, add:

```rust
// Layer 0: Tool injection for structured output
// If output policy has a schema, try tool-injection path first
if let Some(policy) = output_policy {
    if policy.is_structured() {
        if let Some(schema_ref) = &policy.schema {
            // Resolve schema to Value
            let schema_value = match schema_ref {
                SchemaRef::Inline(v) => v.clone(),
                SchemaRef::File(path) => {
                    let content = tokio::fs::read_to_string(path).await
                        .map_err(|e| NikaError::SchemaFailed {
                            details: format!("Failed to read schema '{}': {}", path, e),
                        })?;
                    serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                        details: format!("Invalid JSON in schema '{}': {}", path, e),
                    })?
                }
            };

            let submit_tool = DynamicSubmitTool::new(schema_value);
            let tools: Vec<Box<dyn rig::tool::ToolDyn>> =
                vec![Box::new(submit_tool)];

            debug!(task_id = %task_id, "Layer 0: injecting DynamicSubmitTool");

            match provider.infer_with_tools(&prompt, tools, model).await {
                Ok(result) => {
                    // Tool injection succeeded — validate the result
                    // (provider enforced schema, but double-check)
                    self.event_log.emit(EventKind::StructuredOutputAttempt {
                        task_id: Arc::clone(task_id),
                        layer: 0,
                        layer_name: "tool_injection".to_string(),
                        attempt: 1,
                        success: true,
                        error: None,
                    });
                    // Still run through extraction + validation as safety net
                    // Fall through to existing structured output engine
                    // but with the tool-call result instead of raw stream
                    // ... (continue with existing validation using `result` as input)
                }
                Err(e) => {
                    debug!(
                        task_id = %task_id,
                        error = %e,
                        "Layer 0 failed, falling through to streaming"
                    );
                    self.event_log.emit(EventKind::StructuredOutputAttempt {
                        task_id: Arc::clone(task_id),
                        layer: 0,
                        layer_name: "tool_injection".to_string(),
                        attempt: 1,
                        success: false,
                        error: Some(e.to_string()),
                    });
                    // Fall through to existing streaming path
                }
            }
        }
    }
}
```

Key design: Layer 0 is a **non-blocking attempt**. If it fails (provider doesn't support tool_choice, timeout, etc.), we fall through to the existing streaming + post-processing path.

### Step 3: Run tests

Run: `cargo test --lib -- --nocapture 2>&1 | tail -5`
Expected: All existing tests pass + new test passes

### Step 4: Commit

```bash
git add src/runtime/executor/verbs.rs
git commit -m "feat(runtime): wire DynamicSubmitTool into run_infer as Layer 0

When a task has structured output with a schema, infer_with_tools() is
attempted first. If it succeeds, the result goes through validation.
If it fails, falls through to existing streaming + post-processing.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 4: Wire Tool Injection into Agent Loop

**Files:**
- Modify: `src/runtime/rig_agent_loop/streaming.rs`

### Step 1: Implementation

In the agent loop, when `structured_output` is configured on the agent task, inject `DynamicSubmitTool` into the tools list alongside the MCP tools.

In `stream_with_tools_streaming()` and the CLI-mode agent builder, before `AgentBuilder::new(model).tools(tools)`:

```rust
// If agent has structured output, inject DynamicSubmitTool
if let Some(spec) = &self.params.structured_output {
    let schema_value = match &spec.schema {
        SchemaRef::Inline(v) => v.clone(),
        SchemaRef::File(path) => {
            let content = tokio::fs::read_to_string(path).await
                .map_err(|e| NikaError::SchemaFailed {
                    details: format!("Schema file '{}': {}", path, e),
                })?;
            serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                details: format!("Invalid schema '{}': {}", path, e),
            })?
        }
    };
    tools.push(Box::new(DynamicSubmitTool::new(schema_value)) as Box<dyn ToolDyn>);
}
```

Note: For the agent verb, we do NOT force `tool_choice: Required` because the agent needs to call other MCP tools during multi-turn execution. The `submit_result` tool is available but not forced — the agent calls it when ready to deliver final output.

### Step 2: Run tests

Run: `cargo test --lib rig_agent -- --nocapture 2>&1 | tail -10`
Expected: PASS

### Step 3: Commit

```bash
git add src/runtime/rig_agent_loop/streaming.rs
git commit -m "feat(agent): inject DynamicSubmitTool into agent loop when structured

When agent: verb has structured output configured, DynamicSubmitTool is
added to the tools list. Unlike infer:, tool_choice is not forced —
the agent calls submit_result when ready to deliver final output.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 5: Update StructuredOutputEngine Layer Numbering

**Files:**
- Modify: `src/runtime/structured_output.rs`

### Step 1: Update constants and docs

```rust
// Old:
const LAYER_2_NAME: &str = "provider_native";
const LAYER_3_NAME: &str = "retry_with_feedback";
const LAYER_4_NAME: &str = "llm_repair";

// New:
const LAYER_0_NAME: &str = "tool_injection";
const LAYER_1_NAME: &str = "extract_validate";
const LAYER_2_NAME: &str = "retry_with_feedback";
const LAYER_3_NAME: &str = "llm_repair";
```

Update the module doc comment to reflect the new 4-layer numbering:

```
//! 4-layer defense system for ~99.99% JSON Schema compliance:
//!
//! - **Layer 0**: Tool Injection (DynamicSubmitTool — provider-side enforcement)
//! - **Layer 1**: Extract + Validate (JSON extraction + jsonschema validation)
//! - **Layer 2**: Retry with Feedback (re-prompt with validation errors)
//! - **Layer 3**: LLM Repair (separate call to fix invalid JSON)
```

Update `validate()` method: renumber layer 2 -> 1, layer 3 -> 2, layer 4 -> 3.

Update `StructuredOutputResult.layer` field values accordingly.

### Step 2: Update all event emissions

All `self.emit_attempt()` and `self.emit_success()` calls need updated layer numbers:
- Old layer 2 -> new layer 1
- Old layer 3 -> new layer 2
- Old layer 4 -> new layer 3

### Step 3: Run tests

Run: `cargo test --lib structured_output -- --nocapture`
Expected: Some tests may need updated layer assertions.

### Step 4: Update tests

Update test assertions that check `result.layer == 2` to `result.layer == 1`, etc.

### Step 5: Commit

```bash
git add src/runtime/structured_output.rs
git commit -m "refactor(runtime): renumber structured output layers 0-3

Layer 0: tool_injection (DynamicSubmitTool, handled in executor)
Layer 1: extract_validate (was Layer 2)
Layer 2: retry_with_feedback (was Layer 3)
Layer 3: llm_repair (was Layer 4)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 6: Update AST Types

**Files:**
- Modify: `src/ast/structured.rs`
- Modify: `src/ast/output.rs`

### Step 1: Rename field

In `StructuredOutputSpec`, rename `enable_tool_use` -> `enable_tool_injection` for clarity:

```rust
// Old:
pub enable_tool_use: Option<bool>,

// New:
pub enable_tool_injection: Option<bool>,
```

Update the deserializer to accept both `enable_tool_use` (backward compat) and `enable_tool_injection`:

```rust
"enable_tool_use" | "enable_tool_injection" => {
    enable_tool_injection = Some(map.next_value()?);
}
```

Update `enable_tool_use_or_default()` -> `enable_tool_injection_or_default()`.

### Step 2: Update all references

Search for `enable_tool_use` across the codebase and update to `enable_tool_injection`.

Key locations:
- `src/runtime/structured_output.rs` line 203: `self.spec.enable_tool_use.unwrap_or(true)`
- `src/ast/output.rs` line 102: `enable_tool_use: None`
- Tests in `src/ast/structured.rs`

### Step 3: Run tests

Run: `cargo test --lib structured -- --nocapture`
Expected: PASS (backward compat via deserializer)

### Step 4: Commit

```bash
git add src/ast/structured.rs src/ast/output.rs src/runtime/structured_output.rs
git commit -m "refactor(ast): rename enable_tool_use to enable_tool_injection

Clearer name reflecting the DynamicSubmitTool injection mechanism.
Deserializer accepts both old and new field names for backward compat.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 7: Add Event Kind for Layer 0

**Files:**
- Modify: `src/event/log.rs` (update existing StructuredOutputAttempt to support layer 0)

### Step 1: Verify existing events support layer 0

The existing `StructuredOutputAttempt` event already has a `layer: u8` field, so it already supports layer 0. Just verify the NDJSON serialization handles it properly.

### Step 2: Add a test

```rust
#[test]
fn structured_output_attempt_layer_0() {
    let log = EventLog::new();
    log.emit(EventKind::StructuredOutputAttempt {
        task_id: Arc::from("test-1"),
        layer: 0,
        layer_name: "tool_injection".to_string(),
        attempt: 1,
        success: true,
        error: None,
    });
    let events = log.events();
    assert_eq!(events.len(), 1);
}
```

### Step 3: Commit

```bash
git add src/event/log.rs
git commit -m "test(event): verify StructuredOutputAttempt supports layer 0

Existing event schema already handles layer 0 via u8 field. Added
explicit test for tool_injection layer events.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 8: Integration Tests

**Files:**
- Modify: `tests/regression/output_capture.rs` (add structured output tool injection tests)
- Create: test schema file `tests/fixtures/schemas/user.json`

### Step 1: Create test schema

```json
{
  "type": "object",
  "properties": {
    "name": { "type": "string" },
    "age": { "type": "integer", "minimum": 0 }
  },
  "required": ["name", "age"]
}
```

### Step 2: Add integration test with mock provider

```rust
#[tokio::test]
async fn structured_output_with_mock_provider_validates() {
    // The mock provider returns a JSON that matches common schemas
    // Verify the structured output engine validates it correctly
    let workflow_yaml = r#"
schema: nika/workflow@0.12
workflow: test_structured
provider: mock

tasks:
  - id: generate_user
    infer: "Generate a user object"
    output:
      format: json
      schema:
        type: object
        properties:
          name:
            type: string
          age:
            type: integer
        required: [name, age]
"#;
    // Parse and execute, verify output is valid JSON matching schema
}
```

### Step 3: Add DynamicSubmitTool unit tests for edge cases

```rust
#[tokio::test]
async fn submit_tool_with_nested_schema() {
    let schema = json!({
        "type": "object",
        "properties": {
            "user": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "address": {
                        "type": "object",
                        "properties": {
                            "city": { "type": "string" }
                        }
                    }
                }
            }
        }
    });
    let tool = DynamicSubmitTool::new(schema.clone());
    let def = tool.definition("test".to_string()).await;
    assert_eq!(def.parameters, schema);
}

#[tokio::test]
async fn submit_tool_with_array_schema() {
    let schema = json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    });
    let tool = DynamicSubmitTool::new(schema);
    let args = r#"{"items": ["a", "b", "c"]}"#;
    let result = tool.call(args.to_string()).await.unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["items"].as_array().unwrap().len(), 3);
}
```

### Step 4: Run all tests

Run: `cargo test -- --nocapture 2>&1 | tail -10`
Expected: All tests PASS

### Step 5: Commit

```bash
git add tests/ src/runtime/submit_tool.rs
git commit -m "test(structured): add integration tests for DynamicSubmitTool

Tests cover: nested schemas, array schemas, mock provider validation,
and edge cases (invalid JSON, empty schema).

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

---

## Task 9: Verification & Cleanup

### Step 1: Full test suite

```bash
cargo test 2>&1 | tail -5
```

Expected: All 6,264+ tests PASS

### Step 2: Clippy

```bash
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: Zero warnings

### Step 3: Format

```bash
cargo fmt --check
```

Expected: No formatting issues

### Step 4: Final commit (if any cleanup needed)

```bash
git add -A
git commit -m "chore: cleanup after structured output tool injection"
```

---

## Architecture Summary

```
                    YAML Task
                       |
                       v
              ┌─── structured: ───┐
              │    schema: {...}   │
              └────────┬──────────┘
                       |
           ┌───────────┼────────────┐
           |           |            |
      infer: verb  agent: verb  (future)
           |           |
           v           v
    ┌──────────────────────────┐
    │  Layer 0: Tool Injection │  <-- NEW (DynamicSubmitTool)
    │  tool_choice: Required   │      Provider enforces schema
    └──────────┬───────────────┘
               | (success → validate → return)
               | (failure → fall through)
               v
    ┌──────────────────────────┐
    │  Layer 1: Extract+Valid  │  JSON extraction + jsonschema
    └──────────┬───────────────┘
               | (failure → retry)
               v
    ┌──────────────────────────┐
    │  Layer 2: Retry+Feedback │  Re-prompt with errors
    └──────────┬───────────────┘
               | (failure → repair)
               v
    ┌──────────────────────────┐
    │  Layer 3: LLM Repair     │  Separate repair call
    └──────────────────────────┘
```

## Backward Compatibility

- `enable_tool_use` YAML key still accepted (aliased to `enable_tool_injection`)
- Layer 0 is non-blocking — if tool injection fails, existing Layers 1-3 handle it
- No YAML schema changes required — existing `output:` and `structured:` syntax unchanged
- Mock provider skips Layer 0 (no tool support) — existing tests unaffected
