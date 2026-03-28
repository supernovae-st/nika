# Session E: Test Strengthening (~3-4h)

## Context
Nika workflow engine. 8600+ tests but many are weak (assert is_ok without value check).
Test quality audit found 3 CRITICAL, 7 HIGH, 9 MEDIUM issues.

## Mission: Eliminate tautological tests. Strengthen weak assertions. Add missing coverage.

---

### Part 1: Fix CRITICAL tautological tests (~30min)

**CR2+CR3: Agent tests that test nothing**
File: `nika-engine/src/runtime/rig_agent_loop/tests.rs`

- `test_rig_agent_status_variants` (line 12): Tests `PartialEq` derive. Replace with real status transition test.
- `test_rig_agent_loop_result_debug` (line 21): Tests `Debug` format. Replace with test that runs agent and checks result.

**AD3: Extended thinking constructor-only tests (lines 900-957)**
4 tests only call `RigAgentLoop::new()`. Add at minimum:
- Verify extended_thinking params are stored correctly
- Verify a mock run with thinking produces thinking output

### Part 2: Fix SchemaGuardrail paper tiger (CR1) (~1h)

**File**: `nika-core/src/ast/guardrails.rs:332`
The `check()` method only validates required fields, NOT types/patterns/enums.

Replace the manual check with `jsonschema` (already a dependency):
```rust
pub fn check(&self, output: &str) -> GuardrailResult {
    let parsed: Value = match serde_json::from_str(output) {
        Ok(v) => v,
        Err(e) => return GuardrailResult::failed(/*...*/),
    };
    let validator = match jsonschema::validator_for(&self.json_schema) {
        Ok(v) => v,
        Err(e) => return GuardrailResult::failed(/*...*/),
    };
    if let Err(errors) = validator.validate(&parsed) {
        let messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return GuardrailResult::failed(messages.join("; "));
    }
    GuardrailResult::passed()
}
```

**Tests**:
- `{"age": "not_a_number"}` against schema `{age: number}` → MUST FAIL (currently passes!)
- `{"count": -5}` against `{count: {type: integer, minimum: 0}}` → MUST FAIL
- Valid JSON against schema → passes

### Part 3: Strengthen top 50 `assert!(is_ok())` (~1h)

Priority files (most bug-prone):
1. `nika-engine/src/runtime/executor/tests.rs` — verify invoke template resolution
2. `nika-engine/src/runtime/artifact_processor.rs` — verify artifact content
3. `nika-engine/src/binding/template.rs` — verify resolved values
4. `nika-core/src/ast/analyzer/analyze.rs` — verify analyzed AST

Pattern: change `assert!(result.is_ok())` to:
```rust
let result = result.unwrap();
assert_eq!(result.field, expected_value);
```

### Part 4: Add missing agent test coverage (~1h)

**AD2: token_budget / limits tests**
```rust
#[test]
fn test_agent_limit_max_turns_enforced() {
    let params = AgentParams { max_turns: Some(1), ..Default::default() };
    // Run with mock → verify turns == 1
}

#[test]
fn test_agent_limit_max_cost_enforced() {
    let params = AgentParams {
        limits: Some(AgentLimits { max_cost_usd: 0.001, .. }),
        ..Default::default()
    };
    // Run with mock → verify CostLimitReached
}
```

### Part 5: Add event emission tests (~30min)

**EV3+EV4: SecurityCritical events untested**
```rust
#[test]
fn test_policy_blocked_emits_event() {
    // Create executor with SSRF-blocked URL
    // Verify PolicyBlocked event emitted
}

#[test]
fn test_exec_blocked_emits_event() {
    // Run blocked command
    // Verify PolicyBlocked event emitted
}
```

### Part 6: Remaining v0.51 schema bugs (~30min)

- M-orig6: `{{for_each.index}}` in artifact paths
- M-orig3: `manifest: true` → write artifacts.json
- M-orig8: Temperature validation per-provider

---

## After All Fixes
1. `cargo test --workspace --lib` — expect 8700+
2. `cargo clippy --workspace -- -D warnings` — 0 warnings
3. Run `cargo mutants` on guardrails.rs → 0 surviving
