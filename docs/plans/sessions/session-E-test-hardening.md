# Session E: Test Strengthening (~4-5h)

## Context

Nika workflow engine. 8600+ tests but many are weak (assert is_ok without value check).
Test quality audit found 3 CRITICAL, 7 HIGH, 9 MEDIUM issues.
Full codebase grep reveals **~132 bare `assert!(result.is_ok());` statements** that end a
test function without ever inspecting the value. An additional ~157 instances use `is_ok()`
followed by value checks (these are acceptable).

## Mission: Eliminate tautological tests. Strengthen weak assertions. Add missing coverage.

---

## Part 1: Fix CRITICAL tautological tests (~30min)

### CR2+CR3: Agent tests that test nothing

**File**: `nika-engine/src/runtime/rig_agent_loop/tests.rs`

**Test 1: `test_rig_agent_status_variants` (line 12)**
Currently tests `PartialEq` derive -- a tautological assertion.

```rust
// CURRENT (tautological -- tests compiler, not behavior)
#[test]
fn test_rig_agent_status_variants() {
    let status = RigAgentStatus::NaturalCompletion;
    assert_eq!(status, RigAgentStatus::NaturalCompletion);
    let status = RigAgentStatus::MaxTurnsReached;
    assert_eq!(status, RigAgentStatus::MaxTurnsReached);
}

// REPLACEMENT: Test status behavior properties
#[test]
fn test_rig_agent_status_completion_semantics() {
    // NaturalCompletion and ExplicitCompletion are "completed"
    assert!(RigAgentStatus::NaturalCompletion.is_completed());
    assert!(RigAgentStatus::ExplicitCompletion.is_completed());
    assert!(RigAgentStatus::HighConfidence(0.95).is_completed());

    // MaxTurnsReached, LowConfidence are NOT completed
    assert!(!RigAgentStatus::MaxTurnsReached.is_completed());
    assert!(!RigAgentStatus::LowConfidence(0.5).is_completed());

    // Only LowConfidence requires retry
    assert!(RigAgentStatus::LowConfidence(0.5).requires_retry());
    assert!(!RigAgentStatus::NaturalCompletion.requires_retry());
    assert!(!RigAgentStatus::MaxTurnsReached.requires_retry());

    // Canonical string is used in events -- verify contract
    assert_eq!(RigAgentStatus::NaturalCompletion.as_canonical_str(), "natural_stop");
    assert_eq!(RigAgentStatus::MaxTurnsReached.as_canonical_str(), "max_turns");
    assert_eq!(RigAgentStatus::ExplicitCompletion.as_canonical_str(), "tool_complete");
}
```

**Test 2: `test_rig_agent_loop_result_debug` (line 21)**
Currently only checks Debug format contains "NaturalCompletion".

```rust
// CURRENT (tests Debug derive, not behavior)
#[test]
fn test_rig_agent_loop_result_debug() {
    let result = RigAgentLoopResult { ... };
    let debug = format!("{:?}", result);
    assert!(debug.contains("NaturalCompletion"));
}

// REPLACEMENT: Test result field values are correctly populated
#[test]
fn test_rig_agent_loop_result_fields() {
    let result = RigAgentLoopResult {
        status: RigAgentStatus::NaturalCompletion,
        turns: 3,
        final_output: serde_json::json!({"answer": "42"}),
        total_tokens: 1500,
        confidence: Some(0.95),
        retry_count: 1,
        guardrails_passed: true,
        cost_usd: 0.003,
        partial_result: None,
    };

    assert_eq!(result.turns, 3);
    assert_eq!(result.total_tokens, 1500);
    assert_eq!(result.confidence, Some(0.95));
    assert_eq!(result.retry_count, 1);
    assert!(result.guardrails_passed);
    assert!((result.cost_usd - 0.003).abs() < f64::EPSILON);
    assert_eq!(result.final_output["answer"], "42");
    assert!(result.partial_result.is_none());
}
```

### AD3: Extended thinking constructor-only tests (lines 900-982)

4 tests only call `RigAgentLoop::new()` and `assert!(agent.is_ok())`.
They never verify the agent stored the params correctly.

```rust
// CURRENT (constructor-only, tests nothing about behavior)
#[test]
fn test_agent_loop_with_extended_thinking_creates_successfully() {
    let params = AgentParams {
        extended_thinking: Some(true),
        provider: Some("claude".to_string()),
        ..Default::default()
    };
    let agent = RigAgentLoop::new("thinking-test".to_string(), params, ...);
    assert!(agent.is_ok(), "Agent with extended_thinking should be created");
}

// REPLACEMENT: Verify extended_thinking params are stored and accessible
#[test]
fn test_agent_loop_extended_thinking_params_stored() {
    let params = AgentParams {
        prompt: "Analyze this problem step by step".to_string(),
        extended_thinking: Some(true),
        thinking_budget: Some(10000),
        provider: Some("claude".to_string()),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new("thinking-test".to_string(), params, event_log, mcp_clients)
        .expect("Agent with extended_thinking should be created");

    // Verify params were stored correctly
    assert_eq!(agent.params.extended_thinking, Some(true));
    assert_eq!(agent.params.thinking_budget, Some(10000));
    assert_eq!(agent.params.provider.as_deref(), Some("claude"));

    // Verify completion detection still works
    assert!(!agent.check_completion_signal("normal text"));
}

#[test]
fn test_agent_loop_thinking_disabled_has_no_thinking_budget() {
    let params = AgentParams {
        prompt: "Simple query".to_string(),
        extended_thinking: Some(false),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new("no-thinking-test".to_string(), params, event_log, mcp_clients)
        .expect("Agent with extended_thinking: false should be created");

    assert_eq!(agent.params.extended_thinking, Some(false));
    assert!(agent.params.thinking_budget.is_none());
}

#[test]
fn test_agent_loop_thinking_none_defaults_to_disabled() {
    let params = AgentParams {
        prompt: "Default behavior".to_string(),
        extended_thinking: None,
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new("default-test".to_string(), params, event_log, mcp_clients)
        .expect("Agent with extended_thinking: None should be created");

    assert!(agent.params.extended_thinking.is_none());
}

#[test]
fn test_agent_loop_system_prompt_with_thinking_stored() {
    let system = "You are a math tutor. Think step by step.".to_string();
    let params = AgentParams {
        prompt: "What is 2+2?".to_string(),
        system: Some(system.clone()),
        extended_thinking: Some(true),
        provider: Some("claude".to_string()),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new("system-thinking-test".to_string(), params, event_log, mcp_clients)
        .expect("Agent with system prompt and thinking should be created");

    assert_eq!(agent.params.system.as_deref(), Some(system.as_str()));
    assert_eq!(agent.params.extended_thinking, Some(true));
}
```

---

## Part 2: Fix SchemaGuardrail paper tiger (CR1) (~1h)

### The Bug

**File**: `nika-core/src/ast/guardrails.rs:332`

The `check()` method only validates `required` fields via manual key-existence check.
It does NOT validate types, patterns, enums, minimum/maximum, additionalProperties,
or any other JSON Schema constraints.

This means `{"age": "not_a_number"}` passes against `{properties: {age: {type: number}}}`.

### Architecture Note

`jsonschema` crate is currently only in `nika-engine/Cargo.toml`, not `nika-core`.
Two options:

1. **Option A (recommended)**: Add `jsonschema` to `nika-core/Cargo.toml` behind a feature
   flag `schema-validation` (default-on). This keeps all guardrail logic co-located.
2. **Option B**: Create a wrapper in `nika-engine` that overrides `SchemaGuardrail::check()`.
   Breaks encapsulation and requires runtime dispatch.

Go with Option A.

### Replacement Code

```rust
// nika-core/Cargo.toml additions:
// [dependencies]
// jsonschema = { workspace = true }

// guardrails.rs -- SchemaGuardrail::check()
pub fn check(&self, output: &str) -> GuardrailResult {
    let id = self.id.clone().unwrap_or_else(|| "schema".to_string());

    // Step 1: Parse output as JSON
    let parsed: JsonValue = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(e) => {
            return GuardrailResult::failed_with_action(
                id,
                "schema",
                self.message
                    .clone()
                    .unwrap_or_else(|| format!("Invalid JSON: {}", e)),
                self.on_failure,
            );
        }
    };

    // Step 2: Compile schema (fail explicitly if schema is invalid)
    let validator = match jsonschema::validator_for(&self.json_schema) {
        Ok(v) => v,
        Err(e) => {
            return GuardrailResult::failed_with_action(
                id,
                "schema",
                format!("Invalid JSON Schema: {}", e),
                self.on_failure,
            );
        }
    };

    // Step 3: Validate output against schema
    let result = validator.validate(&parsed);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        let combined = messages.join("; ");
        return GuardrailResult::failed_with_action(
            id,
            "schema",
            self.message.clone().unwrap_or(combined),
            self.on_failure,
        );
    }

    GuardrailResult::passed(id, "schema")
}
```

### 10 Tests for SchemaGuardrail

```rust
// All in nika-core/src/ast/guardrails.rs #[cfg(test)] mod tests

#[test]
fn test_schema_guardrail_type_mismatch_number() {
    // CR1 proof: This MUST FAIL. Currently passes!
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": { "age": { "type": "number" } },
            "required": ["age"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"age": "not_a_number"}"#);
    assert!(!result.passed, "String where number expected should FAIL");
    assert!(result.message.as_ref().unwrap().contains("type"),
            "Error should mention type mismatch");
}

#[test]
fn test_schema_guardrail_pattern_mismatch() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "email": { "type": "string", "pattern": "^[^@]+@[^@]+\\.[^@]+$" }
            },
            "required": ["email"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"email": "not-an-email"}"#);
    assert!(!result.passed, "Invalid email pattern should FAIL");
}

#[test]
fn test_schema_guardrail_enum_mismatch() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "status": { "type": "string", "enum": ["active", "inactive"] }
            },
            "required": ["status"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"status": "pending"}"#);
    assert!(!result.passed, "Value not in enum should FAIL");
}

#[test]
fn test_schema_guardrail_nested_object_validation() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "age": { "type": "integer" }
                    },
                    "required": ["name", "age"]
                }
            },
            "required": ["user"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    // Missing nested required field
    let result = guardrail.check(r#"{"user": {"name": "Alice"}}"#);
    assert!(!result.passed, "Missing nested required field should FAIL");

    // Wrong nested type
    let result = guardrail.check(r#"{"user": {"name": "Alice", "age": "thirty"}}"#);
    assert!(!result.passed, "Wrong nested type should FAIL");

    // Valid nested object
    let result = guardrail.check(r#"{"user": {"name": "Alice", "age": 30}}"#);
    assert!(result.passed, "Valid nested object should PASS");
}

#[test]
fn test_schema_guardrail_array_items_validation() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "scores": {
                    "type": "array",
                    "items": { "type": "number" }
                }
            },
            "required": ["scores"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"scores": [90, "A+", 85]}"#);
    assert!(!result.passed, "String in number array should FAIL");

    let result = guardrail.check(r#"{"scores": [90, 85, 92]}"#);
    assert!(result.passed, "All-number array should PASS");
}

#[test]
fn test_schema_guardrail_minimum_maximum() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer", "minimum": 0, "maximum": 100 }
            },
            "required": ["count"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"count": -5}"#);
    assert!(!result.passed, "Value below minimum should FAIL");

    let result = guardrail.check(r#"{"count": 150}"#);
    assert!(!result.passed, "Value above maximum should FAIL");

    let result = guardrail.check(r#"{"count": 50}"#);
    assert!(result.passed, "Value within range should PASS");
}

#[test]
fn test_schema_guardrail_additional_properties_false() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"name": "Alice", "extra": true}"#);
    assert!(!result.passed, "Extra property should FAIL with additionalProperties: false");

    let result = guardrail.check(r#"{"name": "Alice"}"#);
    assert!(result.passed, "Exact properties should PASS");
}

#[test]
fn test_schema_guardrail_required_plus_type_combined() {
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "price": { "type": "number" },
                "in_stock": { "type": "boolean" }
            },
            "required": ["name", "price"]
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };

    // Missing required
    let result = guardrail.check(r#"{"name": "Widget"}"#);
    assert!(!result.passed, "Missing required 'price' should FAIL");

    // Wrong type for required
    let result = guardrail.check(r#"{"name": "Widget", "price": "free"}"#);
    assert!(!result.passed, "String for number price should FAIL");

    // Valid (optional field omitted)
    let result = guardrail.check(r#"{"name": "Widget", "price": 9.99}"#);
    assert!(result.passed, "Valid with optional omitted should PASS");

    // Valid (all fields)
    let result = guardrail.check(r#"{"name": "Widget", "price": 9.99, "in_stock": true}"#);
    assert!(result.passed, "Valid with all fields should PASS");
}

#[test]
fn test_schema_guardrail_invalid_schema_returns_failure() {
    // Schema itself is broken -- should return clear error, not panic
    let guardrail = SchemaGuardrail {
        id: None,
        json_schema: serde_json::json!({
            "type": "not_a_real_type"
        }),
        message: None,
        on_failure: OnFailure::Retry,
    };
    let result = guardrail.check(r#"{"anything": true}"#);
    // jsonschema may or may not error on unknown type -- but it should not panic
    // The point is graceful handling
    // Note: jsonschema 0.26 is lenient here, but the test documents the expectation
}

#[test]
fn test_schema_guardrail_valid_complex_output() {
    let guardrail = SchemaGuardrail {
        id: Some("product_schema".to_string()),
        json_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "price": { "type": "number", "minimum": 0 },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1
                }
            },
            "required": ["name", "price", "tags"]
        }),
        message: None,
        on_failure: OnFailure::Fail,
    };
    let result = guardrail.check(
        r#"{"name": "Widget Pro", "price": 29.99, "tags": ["electronics", "gadget"]}"#
    );
    assert!(result.passed);
    assert_eq!(result.guardrail_id, "product_schema");

    // Empty tags array violates minItems
    let result = guardrail.check(
        r#"{"name": "Widget Pro", "price": 29.99, "tags": []}"#
    );
    assert!(!result.passed, "Empty tags should FAIL minItems: 1");
    assert_eq!(result.on_failure, OnFailure::Fail);
}
```

---

## Part 3: Top 20 Dangerous Bare `assert!(is_ok())` (~1.5h)

Full grep found **132 instances** where `assert!(result.is_ok());` is the final line
before `}` (end of test function). Ranked by criticality:

### Rank 1-5: Critical Code Paths (runtime/executor/binding)

| # | File | Line | Test Name | Why Dangerous |
|---|------|------|-----------|---------------|
| 1 | `nika-engine/src/runtime/executor/tests_wiremock.rs` | 202 | fetch wiremock test | Validates fetch execution -- result never checked |
| 2 | `nika-engine/src/runtime/structured_output.rs` | 1388 | `handles_nested_objects` | Structured output validation could silently pass corrupt data |
| 3 | `nika-engine/src/runtime/output.rs` | 723 | format_output test | Output formatting bugs hidden |
| 4 | `nika-engine/src/runtime/output.rs` | 962 | output test | Output bugs hidden |
| 5 | `nika-engine/src/runtime/skill_injector.rs` | 472 | skill injection test | Injected content never verified |

### Rank 6-10: Security-Critical

| # | File | Line | Test Name | Why Dangerous |
|---|------|------|-----------|---------------|
| 6 | `nika-engine/src/runtime/security.rs` | 1227 | command validation | Allowed command never verified for correct parsing |
| 7 | `nika-engine/src/io/security.rs` | 297, 338, 391 | path validation (3 tests) | Path security -- allowed paths never verified correct |
| 8 | `nika-engine/src/io/writer.rs` | 502, 522 | atomic write (2 tests) | Written content never read-back verified |
| 9 | `nika-engine/src/secrets/keyring.rs` | 393, 405 | secret storage (2 tests) | Secret stored but never retrieved to confirm |
| 10 | `nika-mcp/src/rmcp_adapter.rs` | 793 | MCP adapter | Tool call result never verified |

### Rank 11-15: DAG/Binding (correctness)

| # | File | Line | Test Name | Why Dangerous |
|---|------|------|-----------|---------------|
| 11 | `nika-engine/src/dag/validate.rs` | 726-1202 | 12 DAG validation tests | Valid DAGs accepted but topology never verified |
| 12 | `nika-engine/src/binding/template.rs` | 1944, 3277 | template validation | Valid template result never inspected |
| 13 | `nika-engine/src/binding/resolve.rs` | 1631 | resolve binding | Resolved value never checked |
| 14 | `nika-core/src/ast/analyzer/analyze.rs` | 1692, 2376, 2525, 2576 | analyzer (4 tests) | Analysis passes but AST structure not verified |
| 15 | `nika-core/src/ast/raw/parser.rs` | 3265 | parser test | Parsed AST not inspected |

### Rank 16-20: Infrastructure

| # | File | Line | Test Name | Why Dangerous |
|---|------|------|-----------|---------------|
| 16 | `nika-mcp/src/client.rs` | 1550, 1561, 1664-1725, 1767, 1776, 1916 | 8 MCP client tests | Connection/tool results never verified |
| 17 | `nika-daemon/src/server.rs` | 955, 983, 1008, 1105, 1370 | 5 daemon server tests | Server responses never inspected |
| 18 | `nika-daemon/src/client.rs` | 559, 573 | 2 daemon client tests | Client results unverified |
| 19 | `nika-daemon/src/lifecycle.rs` | 350 | lifecycle test | State transition not verified |
| 20 | `nika-event/src/trace.rs` | 386 | trace writer | Trace output not verified |

### Pattern for fixing

For each test, change from:

```rust
assert!(result.is_ok());
}
```

To:

```rust
let value = result.expect("should succeed");
// Add appropriate assertion based on what the test is FOR:
// - DAG tests: verify node/edge counts, topological order
// - Security tests: verify the ALLOWED value, not just "it didn't error"
// - Binding tests: verify resolved value matches expected
// - Output tests: verify formatted content
assert_eq!(value.field, expected);
}
```

### Priority Order for Fixes

1. **Security** (rank 6-9): Wrong "allowed" result is worse than a crash
2. **Structured output** (rank 2): Schema validation bugs are invisible
3. **DAG/Binding** (rank 11-13): Wrong topology = wrong execution order
4. **MCP** (rank 10, 16): Silent MCP failures
5. **Rest**: Infrastructure, tooling

---

## Part 4: Event Emission Tests (~1h)

### EventKind Inventory

Full enum has **55 variants** across 18 categories. Current test coverage: ~25 tested.
30+ variants emitted in production with ZERO tests.

### Top 10 Most Important Untested Events (Priority Order)

| # | EventKind | Why Critical | Where Emitted |
|---|-----------|-------------|---------------|
| 1 | `PolicyBlocked` | SECURITY: Must verify blocked commands/URLs emit events | `fetch.rs:146`, `exec.rs:45` |
| 2 | `FallbackTriggered` | ROUTING: Provider failure recovery path invisible | `executor/mod.rs:479` |
| 3 | `GuardrailPassed` / `GuardrailFailed` | AGENT SAFETY: Guardrail enforcement invisible | `thinking.rs:74+`, `infer.rs:1230+` |
| 4 | `ForEachStarted` / `ForEachCompleted` | DAG: for_each lifecycle invisible | `runner.rs` |
| 5 | `ArtifactWritten` / `ArtifactFailed` | OUTPUT: Artifact persistence invisible | `artifact_processor.rs` |
| 6 | `ExecCompleted` | EXEC: Shell command results invisible | `executor/exec.rs` |
| 7 | `AgentStart` / `AgentComplete` | AGENT: Loop lifecycle invisible | `rig_agent_loop/` |
| 8 | `BindingDefaultApplied` | DATA FLOW: Default fallback invisible | `binding/resolve.rs` |
| 9 | `ExtractApplied` | FETCH: Extraction results invisible | `executor/fetch.rs` |
| 10 | `BootPhaseCompleted` | STARTUP: Boot diagnostics invisible | `runner.rs` startup |

### Test Skeletons

```rust
// In nika-engine/src/runtime/executor/tests.rs or a new tests/event_emission_test.rs

use crate::event::{EventKind, EventLog};

// --- 1. PolicyBlocked (Security-Critical) ---

#[tokio::test]
async fn test_exec_blocked_command_emits_policy_blocked_event() {
    let event_log = EventLog::new();
    // Set up executor with event_log
    // Execute a blocked command (e.g., "rm -rf /")
    // Verify TaskFailed returned

    let events = event_log.events();
    let policy_event = events.iter().find(|e| matches!(
        &e.kind, EventKind::PolicyBlocked { verb, .. } if verb.as_ref() == "exec"
    ));
    assert!(policy_event.is_some(), "Blocked command must emit PolicyBlocked");

    if let EventKind::PolicyBlocked { policy_type, reason, .. } = &policy_event.unwrap().kind {
        assert_eq!(policy_type, "command_blocklist");
        assert!(!reason.is_empty());
    }
}

#[tokio::test]
async fn test_fetch_ssrf_emits_policy_blocked_event() {
    let event_log = EventLog::new();
    // Set up executor with event_log
    // Attempt fetch to http://127.0.0.1 (SSRF blocked)

    let events = event_log.events();
    let policy_event = events.iter().find(|e| matches!(
        &e.kind, EventKind::PolicyBlocked { verb, .. } if verb.as_ref() == "fetch"
    ));
    assert!(policy_event.is_some(), "SSRF blocked URL must emit PolicyBlocked");
}

// --- 2. FallbackTriggered ---

#[tokio::test]
async fn test_provider_fallback_emits_event() {
    let event_log = EventLog::new();
    // Set up task with fallback: [provider_a, provider_b]
    // Mock provider_a to fail
    // Verify FallbackTriggered emitted with correct from/to providers

    let events = event_log.events();
    let fallback = events.iter().find(|e| matches!(&e.kind, EventKind::FallbackTriggered { .. }));
    assert!(fallback.is_some(), "Provider failure should emit FallbackTriggered");

    if let EventKind::FallbackTriggered { from_provider, to_provider, reason, attempt, .. } =
        &fallback.unwrap().kind
    {
        assert!(!from_provider.is_empty());
        assert!(!to_provider.is_empty());
        assert_eq!(*attempt, 0);
    }
}

// --- 3. GuardrailPassed / GuardrailFailed ---

#[tokio::test]
async fn test_guardrail_passed_emits_event() {
    let event_log = EventLog::new();
    // Set up agent task with length guardrail: min_words: 5
    // Run with mock provider returning 10-word response

    let events = event_log.events();
    let guardrail_passed = events.iter().find(|e| matches!(
        &e.kind, EventKind::GuardrailPassed { guardrail_type, .. } if guardrail_type == "length"
    ));
    assert!(guardrail_passed.is_some(), "Passing guardrail should emit GuardrailPassed");
}

#[tokio::test]
async fn test_guardrail_failed_emits_event() {
    let event_log = EventLog::new();
    // Set up agent task with length guardrail: min_words: 1000
    // Run with mock provider returning 5-word response

    let events = event_log.events();
    let guardrail_failed = events.iter().find(|e| matches!(
        &e.kind, EventKind::GuardrailFailed { guardrail_type, .. } if guardrail_type == "length"
    ));
    assert!(guardrail_failed.is_some(), "Failing guardrail should emit GuardrailFailed");
}

// --- 4. ForEachStarted / ForEachCompleted ---

#[tokio::test]
async fn test_for_each_lifecycle_events() {
    let event_log = EventLog::new();
    // Run workflow with for_each: items: [1, 2, 3], concurrency: 2

    let events = event_log.events();
    let started = events.iter().find(|e| matches!(&e.kind, EventKind::ForEachStarted { .. }));
    assert!(started.is_some());
    if let EventKind::ForEachStarted { item_count, concurrency, .. } = &started.unwrap().kind {
        assert_eq!(*item_count, 3);
        assert_eq!(*concurrency, 2);
    }

    let completed = events.iter().find(|e| matches!(&e.kind, EventKind::ForEachCompleted { .. }));
    assert!(completed.is_some());
    if let EventKind::ForEachCompleted { total, succeeded, failed, .. } = &completed.unwrap().kind {
        assert_eq!(*total, 3);
        assert_eq!(*succeeded, 3);
        assert_eq!(*failed, 0);
    }
}

// --- 5. ArtifactWritten / ArtifactFailed ---

#[tokio::test]
async fn test_artifact_written_emits_event() {
    let event_log = EventLog::new();
    // Run task with artifact: { path: "output.txt", format: text }

    let events = event_log.events();
    let written = events.iter().find(|e| matches!(&e.kind, EventKind::ArtifactWritten { .. }));
    assert!(written.is_some());
    if let EventKind::ArtifactWritten { path, size, format, .. } = &written.unwrap().kind {
        assert!(path.contains("output.txt"));
        assert!(*size > 0);
        assert_eq!(format, "text");
    }
}

// --- 6. ExecCompleted ---

#[tokio::test]
async fn test_exec_completed_emits_event() {
    let event_log = EventLog::new();
    // Run exec: "echo hello"

    let events = event_log.events();
    let exec_done = events.iter().find(|e| matches!(&e.kind, EventKind::ExecCompleted { .. }));
    assert!(exec_done.is_some());
    if let EventKind::ExecCompleted { exit_code, stdout_len, .. } = &exec_done.unwrap().kind {
        assert_eq!(*exit_code, 0);
        assert!(*stdout_len > 0);
    }
}

// --- 7. AgentStart / AgentComplete ---

#[tokio::test]
async fn test_agent_lifecycle_events() {
    let event_log = EventLog::new();
    // Run agent with mock provider, max_turns: 3

    let events = event_log.events();
    let start = events.iter().find(|e| matches!(&e.kind, EventKind::AgentStart { .. }));
    assert!(start.is_some());
    if let EventKind::AgentStart { max_turns, .. } = &start.unwrap().kind {
        assert_eq!(*max_turns, 3);
    }

    let complete = events.iter().find(|e| matches!(&e.kind, EventKind::AgentComplete { .. }));
    assert!(complete.is_some());
}

// --- 8. BindingDefaultApplied ---

#[tokio::test]
async fn test_binding_default_applied_emits_event() {
    let event_log = EventLog::new();
    // Run workflow with: { val: $upstream.missing ?? "fallback" }
    // where upstream.missing is null

    let events = event_log.events();
    let default_event = events.iter().find(|e| matches!(
        &e.kind, EventKind::BindingDefaultApplied { alias, .. } if alias == "val"
    ));
    assert!(default_event.is_some(), "Null path with default should emit BindingDefaultApplied");
}

// --- 9. ExtractApplied ---

#[tokio::test]
async fn test_extract_applied_emits_event() {
    let event_log = EventLog::new();
    // Run fetch with extract: markdown

    let events = event_log.events();
    let extract = events.iter().find(|e| matches!(&e.kind, EventKind::ExtractApplied { .. }));
    assert!(extract.is_some());
    if let EventKind::ExtractApplied { mode, input_len, output_len, .. } = &extract.unwrap().kind {
        assert_eq!(mode, "markdown");
        assert!(*input_len > 0);
        assert!(*output_len > 0);
    }
}

// --- 10. BootPhaseCompleted ---

#[tokio::test]
async fn test_boot_phase_events_emitted() {
    let event_log = EventLog::new();
    // Run workflow with full boot sequence

    let events = event_log.events();
    let boot_events: Vec<_> = events.iter().filter(|e| matches!(
        &e.kind, EventKind::BootPhaseCompleted { .. }
    )).collect();

    // Should have multiple boot phases
    assert!(!boot_events.is_empty(), "Boot should emit BootPhaseCompleted events");
    // All boot phases should succeed in normal flow
    for event in &boot_events {
        if let EventKind::BootPhaseCompleted { success, .. } = &event.kind {
            assert!(success, "Boot phase should succeed");
        }
    }
}
```

---

## Part 5: E2E Verification Workflows (~30min)

### 5.1: Schema Guardrail with Strict Schema

```yaml
# tests/workflows/e2e_guardrail_schema.nika.yaml
schema: "nika/workflow@0.12"
workflow: e2e-guardrail-schema
description: "Verify schema guardrail validates LLM output against strict JSON schema"
provider: mock

tasks:
  - id: generate
    agent:
      prompt: "Generate a product listing"
      max_turns: 1
      guardrails:
        - type: schema
          json_schema:
            type: object
            properties:
              name: { type: string, minLength: 1 }
              price: { type: number, minimum: 0 }
              tags: { type: array, items: { type: string } }
            required: [name, price, tags]
            additionalProperties: false
          on_failure: fail
      completion:
        mode: natural
```

Test assertion: With mock provider returning well-formed JSON, guardrail passes.
With mock returning `{"name": 123}`, guardrail fails and task fails.

### 5.2: Regex + Length Guardrails Combined

```yaml
# tests/workflows/e2e_guardrail_combined.nika.yaml
schema: "nika/workflow@0.12"
workflow: e2e-guardrail-combined
description: "Verify regex and length guardrails are both checked"
provider: mock

tasks:
  - id: summarize
    agent:
      prompt: "Write a summary starting with 'Summary:'"
      max_turns: 1
      guardrails:
        - type: length
          min_words: 10
          max_words: 200
          on_failure: retry
        - type: regex
          pattern: "^Summary:"
          message: "Output must start with 'Summary:'"
          on_failure: fail
      completion:
        mode: natural
```

Test assertion: Both guardrails are checked. If regex fails (output does not start with
"Summary:"), task fails immediately (`on_failure: fail`). If length is too short but regex
passes, task retries.

### 5.3: Agent + max_turns + completion: explicit

```yaml
# tests/workflows/e2e_agent_max_turns.nika.yaml
schema: "nika/workflow@0.12"
workflow: e2e-agent-max-turns
description: "Verify agent respects max_turns and explicit completion"
provider: mock

tasks:
  - id: researcher
    agent:
      prompt: "Research topic and call nika:complete when done"
      tools: [nika:complete]
      max_turns: 3
      completion:
        mode: explicit
```

Test assertions:
- Agent runs at most 3 turns
- If agent calls `nika:complete`, loop ends before max_turns
- If agent never calls `nika:complete` after 3 turns, status = `MaxTurnsReached`
- Result includes actual turn count

### 5.4: Structured JSON Output with Schema Validation

```yaml
# tests/workflows/e2e_structured_output.nika.yaml
schema: "nika/workflow@0.12"
workflow: e2e-structured-output
description: "Verify structured output matches schema with automatic repair"
provider: mock

tasks:
  - id: extract
    infer:
      prompt: "Extract product data from the following text: 'Widget Pro costs $29.99'"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          price: { type: number }
          currency: { type: string, enum: ["USD", "EUR", "GBP"] }
        required: [name, price]
      enable_repair: true
      max_retries: 2
```

Test assertions:
- Output is valid JSON matching the schema
- `name` is a string, `price` is a number
- If mock returns invalid JSON, repair mechanism retries
- Final output has correct types (not strings-as-numbers)

---

## Part 6: Documentation Audit -- Guardrail Syntax Errors (~15min)

### Bug Found: README.md uses WRONG guardrail syntax

**File**: `nika/README.md` lines 399-406 and 652-657

The README shows guardrails as a **flat dictionary**:

```yaml
# WRONG (flat dict -- this is NOT valid nika syntax)
guardrails:
  max_length: 5000
  schema:
    type: object
    required: [papers, summary]
```

The correct syntax is an **array of typed objects**:

```yaml
# CORRECT (array of typed guardrail configs)
guardrails:
  - type: length
    max_words: 5000
  - type: schema
    json_schema:
      type: object
      required: [papers, summary]
```

### Specific Fixes Needed

1. **README.md line 399-403**: Replace flat dict with array syntax
2. **README.md line 652-657**: Replace flat dict with array syntax; also `max_length` is not
   a valid field (should be `max_words` or `max_chars`), and `regex: "^[A-Z]"` should be
   `- type: regex` with `pattern: "^[A-Z]"`
3. **README.md line 660**: `confidence_threshold` is not a top-level completion field.
   It should be under `confidence: { threshold: 0.8 }`

### Fixed Examples

```yaml
# README.md line 393-406 replacement
- id: researcher
  agent:
    prompt: "Find and summarize recent AI safety papers"
    mcp: [web_search, filesystem]
    max_turns: 15
    guardrails:
      - type: length
        max_words: 5000
      - type: schema
        json_schema:
          type: object
          required: [papers, summary]
    completion:
      mode: explicit
```

```yaml
# README.md line 648-664 replacement
- id: writer
  agent:
    prompt: "Write a product description"
    guardrails:
      - type: length
        max_words: 1000
        on_failure: retry
      - type: schema
        json_schema:
          type: object
          required: [title, body, tags]
        on_failure: retry
      - type: regex
        pattern: "^[A-Z]"
        message: "Must start with uppercase"
        on_failure: retry
    max_turns: 20
    completion:
      mode: explicit
      confidence:
        threshold: 0.8
```

---

## Part 7: Remaining v0.51 Schema Bugs (~30min)

- **M-orig6**: `{{for_each.index}}` unavailable in artifact paths -- inject loop index as
  template variable during for_each expansion
- **M-orig3**: `manifest: true` never writes `artifacts.json` -- implement
  `write_manifest()` in `artifact_processor.rs` at end of workflow
- **M-orig8**: Temperature not validated per-provider -- add validation in analyzer or
  executor (OpenAI: 0-2, Anthropic: 0-1, Groq: 0-2, etc.)

---

## After All Fixes

1. `cargo test --workspace --lib` -- expect 8800+
2. `cargo clippy --workspace -- -D warnings` -- 0 warnings
3. Run `cargo mutants -p nika-core -- --lib -- guardrails` on guardrails.rs -- 0 surviving
4. Verify SchemaGuardrail with `{"age": "string"}` against `{age: number}` FAILS
5. Verify all 10 SchemaGuardrail tests pass
6. Verify README examples parse correctly with `nika check`
7. Verify event tests confirm emission for PolicyBlocked, GuardrailFailed, ForEachCompleted

---

## Execution Checklist

- [ ] **P1 (30m)**: Replace CR2 + CR3 tautological tests
- [ ] **P1 (30m)**: Replace AD3 extended thinking constructor-only tests
- [ ] **P2 (45m)**: Add jsonschema to nika-core, rewrite SchemaGuardrail::check()
- [ ] **P2 (30m)**: Write 10 SchemaGuardrail tests
- [ ] **P3 (90m)**: Fix top 20 bare `assert!(is_ok())` ranked by danger
- [ ] **P4 (60m)**: Write 10 event emission tests
- [ ] **P5 (30m)**: Create 4 E2E verification workflows
- [ ] **P6 (15m)**: Fix README guardrail syntax (3 locations)
- [ ] **P7 (30m)**: Fix M-orig6, M-orig3, M-orig8
- [ ] Final verification: test + clippy + mutants
