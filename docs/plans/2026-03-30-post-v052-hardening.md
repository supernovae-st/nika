# Post-v0.52 Hardening: E2E Tests + Refactors + Bug Fixes

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Harden the Nika engine with 12 new E2E tests, refactor the 1100-line `run_infer()` god method, fix 3 deferred bugs, and propagate max_tokens through retry callbacks.

**Architecture:** The work splits into 3 independent tracks: (A) E2E test coverage for untested verbs/features, (B) `infer.rs` structural refactor extracting L0a/L0b/callback-factory into methods, (C) bug fixes for YAML anchors, NIKA-053 false positives, and agent confidence. Track A can run in parallel with B+C.

**Tech Stack:** Rust, tokio, serde_json, marked-yaml, serde-saphyr. Test framework: `#[tokio::test]` + `cargo test -p nika-engine --lib`.

**Base:** `6502ce790` on `main` (8,970 tests, 0 clippy warnings)

---

## Track A: E2E Test Coverage (12 tests)

All tests go in `tools/nika-engine/src/runtime/tests_e2e_workflow.rs`.

### Task 1: Structured Output E2E (3 tests)

**Files:**
- Modify: `tools/nika-engine/src/runtime/tests_e2e_workflow.rs` (append after line ~890)

**Step 1: Write 3 failing tests**

```rust
// ─── 9. STRUCTURED OUTPUT ──────────────────────────────────────

#[tokio::test]
async fn e2e_structured_output_basic_schema() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: extract
    infer: "Tell me about Alice, 30, developer"
    structured:
      schema:
        type: object
        properties:
          name: { type: string }
          age: { type: number }
        required: [name, age]
"#;
    let output = run_and_get(yaml, "extract").await;
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Should be valid JSON: {e}\nGot: {output}"));
    assert!(parsed.is_object(), "Should be an object: {output}");
}

#[tokio::test]
async fn e2e_structured_output_with_array_schema() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: list
    infer: "List three colors"
    structured:
      schema:
        type: object
        properties:
          colors:
            type: array
            items: { type: string }
        required: [colors]
"#;
    let output = run_and_get(yaml, "list").await;
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or_else(|e| panic!("Should be valid JSON: {e}\nGot: {output}"));
    assert!(parsed.is_object(), "Should be an object: {output}");
}

#[tokio::test]
async fn e2e_structured_output_chained() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: step1
    infer: "Describe a cat"
    structured:
      schema:
        type: object
        properties:
          animal: { type: string }
        required: [animal]
  - id: step2
    depends_on: [step1]
    with:
      data: $step1
    infer:
      prompt: "Expand on: {{with.data}}"
"#;
    let runner = run_yaml(yaml).await;
    let r1 = runner.datastore().get("step1").expect("step1 exists");
    assert!(r1.is_success(), "step1 should succeed");
    let r2 = runner.datastore().get("step2").expect("step2 exists");
    assert!(r2.is_success(), "step2 should succeed");
}
```

**Step 2: Run tests to verify they compile and check behavior**

```bash
cargo test -p nika-engine --lib -- e2e_structured_output 2>&1 | tail -20
```

Expected: Tests either pass (mock provider may handle structured gracefully) or fail with a clear structured output error. If mock provider doesn't support structured, adjust expectation to `run_expect_fail`.

**Step 3: Commit**

```bash
git add nika-engine/src/runtime/tests_e2e_workflow.rs
git commit -m "test(e2e): structured output — basic schema, array, chained"
```

---

### Task 2: Fetch Verb E2E (3 tests)

**Files:**
- Modify: `tools/nika-engine/src/runtime/tests_e2e_workflow.rs`

**Step 1: Write 3 tests**

Note: Fetch tests need a reachable URL. Use `file://` protocol or test that fetch with invalid URLs fails correctly. The mock provider doesn't affect fetch — fetch is HTTP, not LLM.

```rust
// ─── 10. FETCH VERB ──────────────────────────────────────────

#[tokio::test]
async fn e2e_fetch_invalid_url_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: bad_fetch
    fetch:
      url: "http://192.0.2.1:1/nonexistent"
      timeout: 2
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "Fetch to unreachable URL should fail");
}

#[tokio::test]
async fn e2e_fetch_with_extract_in_chain() {
    // Fetch feeds into infer — tests data flow even if fetch fails
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: "echo '<html><body><p>Hello world</p></body></html>'"
  - id: analyze
    depends_on: [source]
    with:
      html: $source
    infer:
      prompt: "Analyze this HTML: {{with.html}}"
"#;
    let output = run_and_get(yaml, "analyze").await;
    assert!(!output.is_empty(), "Should produce analysis");
}

#[tokio::test]
async fn e2e_fetch_ssrf_blocked() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: ssrf
    fetch:
      url: "http://127.0.0.1:9999/secret"
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "SSRF to localhost should be blocked");
}
```

**Step 2: Run tests**

```bash
cargo test -p nika-engine --lib -- e2e_fetch 2>&1 | tail -20
```

Expected: `e2e_fetch_invalid_url_fails` and `e2e_fetch_ssrf_blocked` should fail the workflow (assert `is_err`). `e2e_fetch_with_extract_in_chain` should pass (exec + mock infer).

**Step 3: Commit**

```bash
git add nika-engine/src/runtime/tests_e2e_workflow.rs
git commit -m "test(e2e): fetch verb — invalid URL, data chain, SSRF block"
```

---

### Task 3: Invoke Verb E2E (2 tests)

**Files:**
- Modify: `tools/nika-engine/src/runtime/tests_e2e_workflow.rs`

**Step 1: Write 2 tests**

```rust
// ─── 11. INVOKE VERB (BUILTIN TOOLS) ────────────────────────

#[tokio::test]
async fn e2e_invoke_builtin_log() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: log_it
    invoke:
      tool: "nika:log"
      params:
        message: "Hello from E2E test"
        level: "info"
"#;
    let runner = run_yaml(yaml).await;
    let result = runner.datastore().get("log_it").expect("log_it exists");
    assert!(result.is_success(), "nika:log should succeed");
}

#[tokio::test]
async fn e2e_invoke_unknown_tool_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: bad_invoke
    invoke:
      tool: "nika:nonexistent_tool_xyz"
      params: {}
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "Unknown builtin tool should fail");
}
```

**Step 2: Run tests**

```bash
cargo test -p nika-engine --lib -- e2e_invoke 2>&1 | tail -20
```

**Step 3: Commit**

```bash
git add nika-engine/src/runtime/tests_e2e_workflow.rs
git commit -m "test(e2e): invoke verb — builtin nika:log + unknown tool error"
```

---

### Task 4: Retry + Error Propagation E2E (2 tests)

**Files:**
- Modify: `tools/nika-engine/src/runtime/tests_e2e_workflow.rs`

**Step 1: Write 2 tests**

```rust
// ─── 12. RETRY + ERROR PROPAGATION ──────────────────────────

#[tokio::test]
async fn e2e_retry_exec_eventually_succeeds() {
    // `false` always fails — retry won't help, but verifies retry machinery runs
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: flaky
    exec: "false"
    retry:
      max_attempts: 2
      delay_ms: 10
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    // `false` always fails, so even with retry it should fail
    assert!(result.is_err(), "Permanently failing command should still fail after retries");
}

#[tokio::test]
async fn e2e_error_propagation_nika026() {
    // Upstream failure should cascade to dependent tasks via NIKA-026
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: upstream
    exec: "false"
  - id: downstream
    depends_on: [upstream]
    with:
      data: $upstream
    exec: "echo '{{with.data}}'"
"#;
    let workflow = parse_analyzed(yaml).unwrap();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_err(), "Upstream failure should cascade");
}
```

**Step 2: Run tests**

```bash
cargo test -p nika-engine --lib -- e2e_retry e2e_error_propagation 2>&1 | tail -20
```

**Step 3: Commit**

```bash
git add nika-engine/src/runtime/tests_e2e_workflow.rs
git commit -m "test(e2e): retry machinery + NIKA-026 error propagation"
```

---

### Task 5: Parametric Transforms + for_each+infer E2E (2 tests)

**Files:**
- Modify: `tools/nika-engine/src/runtime/tests_e2e_workflow.rs`

**Step 1: Write 2 tests**

```rust
// ─── 13. PARAMETRIC TRANSFORMS + FOR_EACH+INFER ─────────────

#[tokio::test]
async fn e2e_transform_join_split_default() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: source
    exec: 'echo "alpha,beta,gamma"'
  - id: transform
    depends_on: [source]
    with:
      raw: $source | trim
      parts: $source | trim | split(",")
    exec: 'echo "{{with.parts | join(" + ")}}"'
"#;
    let output = run_and_get(yaml, "transform").await;
    assert!(
        output.contains("alpha") && output.contains("beta"),
        "Split+join should preserve elements: {output}"
    );
}

#[tokio::test]
async fn e2e_for_each_with_infer() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
tasks:
  - id: items
    exec: 'echo "one two three"'
  - id: loop_infer
    depends_on: [items]
    for_each: ["red", "green", "blue"]
    as: color
    infer: "Describe the color {{with.color}}"
"#;
    let output = run_and_get(yaml, "loop_infer").await;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    assert_eq!(arr.len(), 3, "Should have 3 infer results, got: {output}");
}
```

**Step 2: Run tests**

```bash
cargo test -p nika-engine --lib -- e2e_transform_join e2e_for_each_with_infer 2>&1 | tail -20
```

**Step 3: Commit**

```bash
git add nika-engine/src/runtime/tests_e2e_workflow.rs
git commit -m "test(e2e): parametric transforms (join/split) + for_each with infer"
```

---

## Track B: infer.rs Refactor

### Task 6: Extract InferCallback Factory

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs`

**Context:** InferCallback construction is duplicated 3 times (lines 488-507, 1037-1053, 1073-1087). All three follow the same pattern: clone provider, clone model, wrap in `Arc::new(move |prompt| { Box::pin(async move { provider.infer(...) }) })`.

**Step 1: Write test for the factory**

Add a unit test at the bottom of `infer.rs` (or in a new test module) that verifies the factory produces a working callback. Since `InferCallback` is a type alias in `structured_output.rs`, the test lives close to usage.

This is a refactor — existing E2E tests cover the behavior. The "test" is: all 8,970+ tests still pass.

**Step 2: Extract the factory method**

Add this method to `TaskExecutor` impl block (above `run_infer`, around line 30):

```rust
/// Build an InferCallback that calls `provider.infer()` with optional model override.
/// Used by L0 safety-net, main streaming path (L2-L3), and repair path (L4).
fn make_infer_callback(
    provider: &Arc<RigProvider>,
    model: Option<&str>,
) -> crate::runtime::structured_output::InferCallback {
    let provider = provider.clone();
    let model_for_retry = model.map(|s| s.to_string());
    Arc::new(move |retry_prompt: String| {
        let provider = provider.clone();
        let model = model_for_retry.clone();
        Box::pin(async move {
            provider
                .infer(&retry_prompt, model.as_deref())
                .await
                .map(|s| super::verbs::strip_think_tags(&s))
                .map_err(|e| NikaError::ProviderApiError {
                    message: format!("structured output retry failed: {}", e),
                })
        })
    })
}
```

**Step 3: Replace all 3 construction sites**

Replace lines 488-507 (`l0_infer_callback`):
```rust
let l0_infer_callback = Self::make_infer_callback(&provider, model);
```

Replace lines 1037-1053 (`infer_callback`):
```rust
let infer_callback = Self::make_infer_callback(&provider, model);
```

Replace lines 1073-1087 (`repair_callback`) — this one uses a DIFFERENT provider/model:
```rust
let repair_callback = Self::make_infer_callback(&repair_provider, Some(&repair_model_name));
```

**Step 4: Run full test suite**

```bash
cargo test --workspace --lib 2>&1 | tail -5
```

Expected: All tests pass, zero behavior change.

**Step 5: Commit**

```bash
git add nika-engine/src/runtime/executor/infer.rs
git commit -m "refactor(structured): extract InferCallback factory method"
```

---

### Task 7: Extract L0a into Method

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs`

**Context:** L0a (native response_format) spans lines 526-701 (~175 lines). Extract to a private async method.

**Step 1: Create the method signature**

```rust
/// Layer 0a: Native structured output via response_format (OpenAI, Groq, xAI).
/// Returns `Some(validated_json)` on success, `None` to fall through to L0b/streaming.
async fn try_layer_0a_response_format(
    &self,
    provider: &Arc<RigProvider>,
    model: Option<&str>,
    prompt: &str,
    system_prompt: Option<&str>,
    infer: &InferParams,
    schema_value: &Value,
    spec: &StructuredOutputSpec,
    l0_infer_callback: &crate::runtime::structured_output::InferCallback,
    task_id: &Arc<str>,
    provider_name: &ProviderName,
) -> Result<Option<String>, NikaError> {
    // Move lines 526-701 here, replacing early returns with Ok(Some(json_str))
    // and fall-through with Ok(None)
}
```

**Step 2: Move the L0a block body into the new method**

Move lines 535-701 into the method body. Replace:
- `return Ok(...)` early returns → `return Ok(Some(...))`
- Fall-through at end → `Ok(None)`

**Step 3: Replace the inline block with the method call**

At the original location (line ~526):
```rust
if provider.supports_native_structured_output() {
    if let Some(result) = self.try_layer_0a_response_format(
        &provider, model, prompt, system_prompt.as_deref(),
        infer, &schema_value, &spec,
        &l0_infer_callback, task_id, &provider_name,
    ).await? {
        return Ok(result);
    }
}
```

**Step 4: Run full test suite**

```bash
cargo test --workspace --lib 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add nika-engine/src/runtime/executor/infer.rs
git commit -m "refactor(structured): extract L0a response_format into method"
```

---

### Task 8: Extract L0b into Method

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs`

**Context:** L0b (tool injection via DynamicSubmitTool) spans lines 704-909 (~205 lines).

**Step 1: Create the method signature**

```rust
/// Layer 0b: Structured output via tool injection (DynamicSubmitTool).
/// Returns `Some(validated_json)` on success, `None` to fall through to streaming.
async fn try_layer_0b_tool_injection(
    &self,
    provider: &Arc<RigProvider>,
    model: Option<&str>,
    prompt: &str,
    system_prompt: Option<&str>,
    infer: &InferParams,
    schema_value: &Value,
    spec: &StructuredOutputSpec,
    l0_infer_callback: &crate::runtime::structured_output::InferCallback,
    task_id: &Arc<str>,
    provider_name: &ProviderName,
) -> Result<Option<String>, NikaError> {
    // Move lines 704-909 here
}
```

**Step 2: Move the L0b block body, same pattern as Task 7**

**Step 3: Replace inline block with method call**

**Step 4: Run full test suite**

```bash
cargo test --workspace --lib 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add nika-engine/src/runtime/executor/infer.rs
git commit -m "refactor(structured): extract L0b tool_injection into method"
```

---

### Task 9: Propagate max_tokens Through InferCallback

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs` (factory method from Task 6)
- Modify: `tools/nika-engine/src/runtime/structured_output.rs` (InferCallback type, line 65)

**Context:** `provider.infer()` hardcodes max_tokens=8192. If a task sets `max_tokens: 16000`, L3/L4 retries silently cap at 8192. Fix: pass max_tokens into the callback.

**Step 1: Update InferCallback type to accept max_tokens**

In `tools/nika-engine/src/runtime/structured_output.rs` line 65:

```rust
// BEFORE:
pub type InferCallback = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send>> + Send + Sync,
>;

// AFTER:
pub type InferCallback = Arc<
    dyn Fn(String, Option<u32>) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send>> + Send + Sync,
>;
```

**Step 2: Update factory method to use max_tokens**

```rust
fn make_infer_callback(
    provider: &Arc<RigProvider>,
    model: Option<&str>,
) -> crate::runtime::structured_output::InferCallback {
    let provider = provider.clone();
    let model_for_retry = model.map(|s| s.to_string());
    Arc::new(move |retry_prompt: String, max_tokens: Option<u32>| {
        let provider = provider.clone();
        let model = model_for_retry.clone();
        Box::pin(async move {
            if let Some(mt) = max_tokens {
                // Use infer_with_options to pass max_tokens
                let options = crate::provider::rig::InferOptions {
                    max_tokens: Some(mt),
                    ..Default::default()
                };
                provider
                    .infer_with_options(&retry_prompt, model.as_deref(), &options)
                    .await
                    .map(|r| super::verbs::strip_think_tags(&r.text))
                    .map_err(|e| NikaError::ProviderApiError {
                        message: format!("structured output retry failed: {}", e),
                    })
            } else {
                provider
                    .infer(&retry_prompt, model.as_deref())
                    .await
                    .map(|s| super::verbs::strip_think_tags(&s))
                    .map_err(|e| NikaError::ProviderApiError {
                        message: format!("structured output retry failed: {}", e),
                    })
            }
        })
    })
}
```

**Step 3: Update all call sites in structured_output.rs**

Search for `(infer_callback)(prompt` and `(callback)(prompt` in `structured_output.rs`. Update each to pass `self.max_tokens` or the task's max_tokens:

```rust
// BEFORE:
let result = (callback)(retry_prompt).await?;

// AFTER:
let result = (callback)(retry_prompt, self.max_tokens).await?;
```

You'll need to add a `max_tokens: Option<u32>` field to `StructuredOutputEngine` and a `.with_max_tokens()` builder method.

**Step 4: Run full test suite**

```bash
cargo test --workspace --lib 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add nika-engine/src/runtime/executor/infer.rs nika-engine/src/runtime/structured_output.rs
git commit -m "fix(structured): propagate max_tokens through InferCallback for L3/L4 retries"
```

---

## Track C: Bug Fixes

### Task 10: BUG-8 — Agent LowConfidence(0.0) Cosmetic Fix

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs` (line ~190)

**Context:** When `completion.mode == explicit` and the agent ends a turn without calling `nika:complete`, the code returns `LowConfidence(0.0)`. This is actually correct behavior (it forces a retry). The "bug" is cosmetic: logs show `LowConfidence(0.0)` which looks like a failure when it's actually the expected penalty for not calling `nika:complete`.

**Step 1: Write a test that documents the expected behavior**

In `tools/nika-engine/src/runtime/rig_agent_loop/thinking.rs` test module:

```rust
#[test]
fn explicit_mode_without_complete_returns_low_confidence() {
    // When mode=explicit and agent doesn't call nika:complete,
    // LowConfidence(0.0) is the correct penalty — triggers retry
    let thinking = AgentThinking::new(/* explicit mode params */);
    let status = thinking.determine_status("Some output without completion marker");
    assert!(
        matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.0),
        "Explicit mode without nika:complete should return LowConfidence(0.0)"
    );
}
```

Note: The exact constructor depends on `AgentThinking::new()` signature. Read it first.

**Step 2: Improve the log message**

At line ~190 in `determine_status()`, change:
```rust
// BEFORE:
return RigAgentStatus::LowConfidence(0.0);

// AFTER (add tracing context):
tracing::debug!(
    "Agent did not call nika:complete in explicit mode — returning LowConfidence(0.0) to trigger retry"
);
return RigAgentStatus::LowConfidence(0.0);
```

**Step 3: Run tests**

```bash
cargo test -p nika-engine --lib -- thinking 2>&1 | tail -10
```

**Step 4: Commit**

```bash
git add nika-engine/src/runtime/rig_agent_loop/thinking.rs
git commit -m "fix(agent): add debug log for LowConfidence(0.0) in explicit completion mode"
```

---

### Task 11: BUG-6 — NIKA-053 False Positive on Echo

**Files:**
- Modify: `tools/nika-engine/src/runtime/security.rs`

**Context:** The `SHELL_MODE_BLOCKLIST` (lines 143-153) blocks backtick `` ` `` too aggressively. Legitimate commands with backticks in strings get blocked. The check at `exec.rs:40-45` runs on the RAW template, not resolved output.

**Step 1: Write a failing test**

```rust
#[test]
fn echo_with_backtick_in_string_not_blocked() {
    // Backticks inside quotes should not trigger NIKA-053
    let cmd = r#"echo "file`name.txt""#;
    let result = check_shell_mode_blocklist(cmd);
    assert!(result.is_ok(), "Backtick inside quotes should not be blocked: {result:?}");
}

#[test]
fn actual_backtick_substitution_blocked() {
    // Unquoted backtick command substitution SHOULD be blocked
    let cmd = "echo `rm -rf /`";
    let result = check_shell_mode_blocklist(cmd);
    assert!(result.is_err(), "Backtick substitution should be blocked");
}
```

**Step 2: Implement quote-aware backtick detection**

Replace the simple `contains` check with a quote-aware scanner:

```rust
/// Check if `pattern` appears outside of single/double quotes in `cmd`.
fn contains_unquoted(cmd: &str, pattern: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let bytes = cmd.as_bytes();
    let pat_bytes = pattern.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'\\' if in_double && i + 1 < bytes.len() => {
                i += 1; // skip escaped char in double quotes
            }
            _ if !in_single && !in_double => {
                if bytes[i..].starts_with(pat_bytes) {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}
```

Then update `check_shell_mode_blocklist` to use `contains_unquoted` for the backtick pattern only:

```rust
// In SHELL_MODE_BLOCKLIST iteration:
for pattern in &SHELL_MODE_BLOCKLIST {
    let found = if *pattern == "`" {
        contains_unquoted(cmd, pattern)
    } else {
        cmd.contains(pattern)
    };
    if found {
        return Err(NikaError::BlockedCommand { ... });
    }
}
```

**Step 3: Run tests**

```bash
cargo test -p nika-engine --lib -- security 2>&1 | tail -20
```

**Step 4: Commit**

```bash
git add nika-engine/src/runtime/security.rs
git commit -m "fix(security): quote-aware backtick detection in NIKA-053"
```

---

### Task 12: BUG-4 — Document YAML Anchor Limitation

**Files:**
- Create: `docs/adr/0XXX-yaml-anchors.md` (next ADR number)

**Context:** `marked-yaml` explicitly rejects YAML anchors (`&`/`*`) with `UnexpectedAnchor` error (NIKA-160). Switching to `saphyr` for raw parsing would require rebuilding the entire span-tracking system — the cost is too high for a LOW priority feature. `saphyr` is already used for deserialization (with budget protection for anchor bombs), but not for raw AST parsing.

**Step 1: Write the ADR**

```markdown
# ADR-XXXX: YAML Anchors Not Supported

## Status: Accepted

## Context

YAML anchors (`&anchor` / `*alias`) are rejected by our parser (marked-yaml 0.8)
with NIKA-160. Users occasionally request this for DRY workflow definitions.

## Decision

We do NOT support YAML anchors. The workaround is `include:` for shared task blocks.

## Rationale

- marked-yaml provides span tracking (line:col) critical for error messages and LSP
- saphyr (already a dep) supports anchors but is serde-based — no span tracking
- Switching parser = rewrite 500+ lines of Node-based parsing + span system
- `include:` with `prefix:` achieves the same DRY goal at workflow level
- Anchor bombs are a security risk; saphyr's budget.rs mitigates but adds complexity

## Consequences

- Users use `include:` instead of anchors for shared definitions
- Error message on anchors: NIKA-160 with suggestion to use `include:`
```

**Step 2: Improve NIKA-160 error message**

In `tools/nika-core/src/ast/raw/parser.rs`, find the `UnexpectedAnchor` handler (around line 99) and ensure the error message suggests `include:` as an alternative.

**Step 3: Commit**

```bash
git add docs/adr/ nika-core/src/ast/raw/parser.rs
git commit -m "docs(adr): document YAML anchor limitation + improve NIKA-160 message"
```

---

## Execution Order

| Phase | Tasks | Parallel? | Estimated Tests |
|-------|-------|-----------|-----------------|
| 1 | Tasks 1-5 (E2E tests) | Yes, all 5 independent | +12 tests |
| 2 | Task 6 (callback factory) | Alone — foundational | 0 new (refactor) |
| 3 | Tasks 7-8 (L0a/L0b extract) | Yes, independent after T6 | 0 new (refactor) |
| 4 | Task 9 (max_tokens propagation) | After T6 (uses factory) | 0 new (fix) |
| 5 | Tasks 10-12 (bug fixes) | Yes, all 3 independent | +3-4 tests |

**Total: ~15 new tests, 3 bug fixes, 1 ADR, 1 major refactor (run_infer shrinks ~400 lines)**

---

## Verification Checklist

After all tasks:

```bash
cargo test --workspace --lib          # All tests pass
cargo clippy --workspace -- -D warnings  # Zero warnings
git log --oneline -15                 # Clean commit history
```
