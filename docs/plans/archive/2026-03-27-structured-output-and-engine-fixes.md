# Structured Output Pipeline + Engine Fixes — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix the 5-layer structured output defense system so it works with ALL providers (especially OpenAI), rename `imports:` → `include:` with zero legacy, add template resolution to `model:` field, and make `extended_thinking` gracefully degrade instead of crashing.

**Architecture:** 5 independent fixes in the Nika engine. Each is self-contained with its own tests. The structured output fix is the biggest — it requires adding OpenAI-native `response_format` support since rig-core v0.32 doesn't expose it, and the current tool_injection fallback (Layer 0) fails with `MaxTurnError(0)` for OpenAI. The other 4 fixes are surgical: rename a YAML key, add `template_resolve()` to 2 lines, and convert a crash to a WARN.

**Tech Stack:** Rust, rig-core 0.32, serde, jsonschema, tokio, insta (snapshots), wiremock (HTTP mocking)

**Testing:** `cargo test -p nika-engine --lib` (3870+ tests) + `cargo test -p nika-core --lib`

**WARNING:** Always use `--lib` to avoid macOS Keychain popups.

---

## Context: The 5-Layer Structured Output Defense

```
Layer 0: Tool Injection (DynamicSubmitTool + tool_choice: Required)
         → Works for Claude/Gemini. FAILS for OpenAI (MaxTurnError).
         → File: runtime/executor/infer.rs:280-552

Layer 1: rig Extractor (Rust types via schemars)
         → NOT IMPLEMENTED. Placeholder only.

Layer 2: Extract + Validate (JSON extraction + schema validation)
         → Works for all providers. Post-processing step.
         → File: runtime/structured_output.rs:367-410

Layer 3: Retry with Feedback (re-prompt with validation errors)
         → Works but relies on text-only instruction. Weak for OpenAI.
         → File: runtime/structured_output.rs:421-590

Layer 4: LLM Repair (dedicated repair call)
         → Works but text-only. Same weakness as Layer 3.
         → File: runtime/structured_output.rs:592-747
```

**Root cause of OpenAI failure:** rig-core v0.32 sends `tool_choice: "required"` but does NOT send `response_format: { type: "json_schema" }`. OpenAI needs BOTH for reliable structured output. When Layer 0 fails, Layers 2-4 fall back to text-only schema instructions in the prompt, which OpenAI follows ~70-80% of the time — not enough for production.

---

## Task 1: OpenAI Structured Output — Add `response_format` via `additional_params`

The biggest fix. We need to send `response_format` to OpenAI alongside (or instead of) tool injection.

**Files:**
- Modify: `tools/nika-engine/src/provider/rig.rs`
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs`
- Test: `tools/nika-engine/src/provider/rig.rs` (existing test module)
- Test: `tools/nika-engine/src/runtime/executor/tests.rs`
- Test: `tools/nika-engine/src/runtime/executor/tests_wiremock.rs`

### Step 1: Understand the rig-core API surface

Check if rig-core 0.32 exposes `additional_params` on CompletionModel or AgentBuilder:

```bash
grep -rn "additional_params\|AdditionalParams\|response_format\|extra_params" \
  tools/nika-engine/src/provider/rig.rs
```

Also check rig-core's source for the OpenAI completion model:
```bash
grep -rn "additional_params\|response_format" \
  target/debug/.fingerprint/rig-core-*/
# Or check Cargo.lock for the rig-core source path
```

If rig-core 0.32 does NOT support `additional_params`: we need a **direct HTTP bypass** for structured output — construct the OpenAI API call manually with reqwest, adding `response_format` ourselves.

If rig-core 0.32 DOES have `additional_params` (check `CompletionRequestBuilder`): use it to inject `response_format`.

### Step 2: Write failing test — OpenAI structured output with response_format

Create a wiremock test that verifies the OpenAI API call includes `response_format`:

```rust
// In tests_wiremock.rs or a new test file
#[tokio::test]
async fn test_openai_structured_output_sends_response_format() {
    // Setup wiremock server pretending to be OpenAI
    let mock_server = MockServer::start().await;

    // Expect a POST to /v1/chat/completions with response_format in body
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_json_contains(json!({
            "response_format": {
                "type": "json_schema"
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{
                "message": {
                    "content": "{\"name\": \"test\", \"score\": 42}"
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    // Run infer with structured: schema through the OpenAI provider
    // pointed at mock_server.uri()
    // Assert: request was matched (response_format was sent)
    // Assert: structured output validation passes
}
```

Run: `cargo test -p nika-engine --lib test_openai_structured_output_sends_response_format`
Expected: FAIL (response_format not sent yet)

### Step 3: Implement OpenAI response_format injection

**Option A — If rig-core has additional_params:**

In `provider/rig.rs`, modify `infer_with_tools()` or create a new `infer_structured()` method:

```rust
/// Infer with structured output — uses native response_format for OpenAI
pub async fn infer_structured(
    &self,
    prompt: &str,
    schema: &serde_json::Value,
    model: Option<&str>,
    max_tokens: Option<u32>,
    system: Option<&str>,
) -> Result<String, RigInferError> {
    match self {
        RigProvider::OpenAI(client) => {
            // Use additional_params to inject response_format
            let model_id = model.unwrap_or(&self.default_model);
            let mut builder = client.completion_model(model_id)
                .completion_request(prompt);

            if let Some(sys) = system {
                builder = builder.preamble(sys);
            }
            if let Some(mt) = max_tokens {
                builder = builder.max_tokens(mt as u64);
            }

            // Inject OpenAI native structured output
            builder = builder.additional_params(json!({
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "structured_output",
                        "strict": true,
                        "schema": schema
                    }
                }
            }));

            let response = builder.send().await?;
            Ok(response.content)
        }
        // For Claude/Gemini/etc: fall back to tool injection (existing behavior)
        _ => self.infer_with_tools(prompt, /* tools */, model, max_tokens, system).await,
    }
}
```

**Option B — If rig-core does NOT have additional_params (direct HTTP):**

Add a raw reqwest call in `provider/rig.rs`:

```rust
/// Direct OpenAI API call with response_format (bypasses rig-core)
async fn openai_structured_direct(
    &self,
    api_key: &str,
    base_url: &str,
    model: &str,
    prompt: &str,
    system: Option<&str>,
    schema: &serde_json::Value,
    max_tokens: Option<u32>,
) -> Result<String, RigInferError> {
    let client = reqwest::Client::new();

    let mut messages = vec![];
    if let Some(sys) = system {
        messages.push(json!({"role": "system", "content": sys}));
    }
    messages.push(json!({"role": "user", "content": prompt}));

    let mut body = json!({
        "model": model,
        "messages": messages,
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": "structured_output",
                "strict": true,
                "schema": schema
            }
        }
    });

    if let Some(mt) = max_tokens {
        body["max_tokens"] = json!(mt);
    }

    let resp = client
        .post(format!("{}/chat/completions", base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| RigInferError::RequestFailed(e.to_string()))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| RigInferError::ParseFailed(e.to_string()))?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| RigInferError::ParseFailed("No content in response".into()))
}
```

### Step 4: Wire structured output into the infer executor

In `runtime/executor/infer.rs`, modify the Layer 0 path (around line 365-395):

```rust
// LAYER 0: Structured output
if policy.is_structured() {
    let schema_value = /* resolve schema */;

    // Try native response_format for OpenAI-compatible providers
    let provider_name = /* resolved provider name */;
    let supports_response_format = matches!(
        provider_name, "openai" | "groq" | "deepseek" | "xai"
    );

    if supports_response_format {
        // Use native response_format (Option A or B from above)
        match provider.infer_structured(&prompt, &schema_value, model, max_tokens, system).await {
            Ok(result) => {
                self.event_log.emit(EventKind::StructuredOutputAttempt {
                    layer: 0,
                    layer_name: "response_format".to_string(),
                    success: true,
                    error: None,
                });
                // Validate result through Layer 2 (still validate even with native)
                let engine = StructuredOutputEngine::new(spec, ...);
                return engine.validate(task_id, &result).await;
            }
            Err(e) => {
                self.event_log.emit(EventKind::StructuredOutputAttempt {
                    layer: 0,
                    layer_name: "response_format".to_string(),
                    success: false,
                    error: Some(e.to_string()),
                });
                // Fall through to tool injection or streaming
            }
        }
    }

    // Existing tool injection path for Claude/Gemini/Mistral
    let submit_tool = DynamicSubmitTool::new(schema_value.clone());
    // ... existing code ...
}
```

### Step 5: Run test, verify it passes

Run: `cargo test -p nika-engine --lib test_openai_structured_output`
Expected: PASS

### Step 6: Write additional tests

```rust
#[tokio::test]
async fn test_structured_output_fallback_to_streaming_on_response_format_error() {
    // response_format fails → falls through to Layer 2-4
}

#[tokio::test]
async fn test_claude_still_uses_tool_injection_not_response_format() {
    // Verify Claude path unchanged
}

#[tokio::test]
async fn test_response_format_result_still_validated_by_layer2() {
    // Even with native response_format, Layer 2 validates
}
```

### Step 7: Run full test suite

Run: `cargo test -p nika-engine --lib`
Expected: 3870+ tests pass, 0 failures

### Step 8: Commit

```bash
git add tools/nika-engine/src/provider/rig.rs \
        tools/nika-engine/src/runtime/executor/infer.rs \
        tools/nika-engine/src/runtime/executor/tests_wiremock.rs
git commit -m "feat(provider): OpenAI native response_format for structured output

Layer 0 now uses response_format: json_schema for OpenAI-compatible
providers instead of tool_choice: Required (which fails with MaxTurnError).
Claude/Gemini/Mistral still use tool injection.

Fixes NIKA-061 failures when provider: openai with structured: blocks.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 2: Rename `imports:` → `include:` — Zero Legacy

Complete removal of `imports:` from YAML, parser, schema, and all code. The YAML key becomes `include:` everywhere.

**Files to modify (complete list from research):**

### nika-core (parser + AST):

1. `tools/nika-core/src/ast/raw/workflow.rs:51` — rename field `imports` → `include`
2. `tools/nika-core/src/ast/raw/parser.rs:1296,1345,1487-1543` — rename function `parse_imports` → `parse_include`, change string literal `"imports"` → `"include"` in `get_node()` and `known_workflow_keys`
3. `tools/nika-core/src/ast/raw/parser.rs:2217-2246` — update test YAML from `imports:` to `include:`
4. `tools/nika-core/src/ast/schema.rs:135,138,198,284` — update doc comments
5. `tools/nika-core/src/ast/analyzed/workflow.rs:57,94` — rename field `imports` → `include`

### nika-engine (loader + runtime):

6. `tools/nika-engine/src/ast/mod.rs:101,134,189,191,201,248,250,269` — rename `import_loader` → `include_loader`, `expand_imports` → `expand_include`
7. `tools/nika-engine/src/ast/import_loader.rs` — **rename entire file** to `include_loader.rs`, rename `expand_imports` → `expand_include`, `expand_imports_recursive` → `expand_include_recursive`, update ~45 test functions (YAML strings from `imports:` to `include:`)
8. `tools/nika-engine/src/ast/lower.rs:466,644` — rename `lower_imports` → `lower_include`
9. `tools/nika-engine/src/runtime/runner.rs:2458,2596,2743,3483,5579,5866` — rename field assignments `imports:` → `include:`
10. `tools/nika-engine/src/ast/workflow.rs:62` — field already named `include` (serde) ✓

### JSON Schema:

11. `tools/nika/schemas/nika-workflow.schema.json` — already uses `include:` ✓ (no change)
12. `tools/nika-engine/schemas/nika-workflow.schema.json` — same ✓

### Step 1: Write failing test

```rust
#[test]
fn test_parse_include_not_imports() {
    let yaml = r#"
schema: "nika/workflow@0.12"
include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
tasks:
  - id: main
    infer: "hello"
"#;
    let workflow = parse(yaml, FileId(0)).unwrap();
    // Should parse the include block
    assert!(workflow.include.is_some()); // Field renamed from imports
    assert_eq!(workflow.include.as_ref().unwrap().value.len(), 1);
}

#[test]
fn test_imports_key_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.12"
imports:
  - path: ./partials/setup.nika.yaml
tasks:
  - id: main
    infer: "hello"
"#;
    // Should FAIL — imports: is no longer accepted
    let result = parse(yaml, FileId(0));
    assert!(result.is_err());
}
```

Run: `cargo test -p nika-core --lib test_parse_include_not_imports`
Expected: FAIL (parser still looks for "imports")

### Step 2: Rename in parser

In `tools/nika-core/src/ast/raw/parser.rs`:

```rust
// Line ~1345: Update known_workflow_keys
// FROM: "imports"
// TO: "include"

// Line ~1487: Update get_node call
// FROM: let imports_node = match map.get_node("imports") {
// TO:   let include_node = match map.get_node("include") {

// Line ~1296: Update field assignment
// FROM: workflow.imports = parse_imports(file_id, map)?;
// TO:   workflow.include = parse_include(file_id, map)?;

// Rename function:
// FROM: fn parse_imports(...)
// TO:   fn parse_include(...)
```

In `tools/nika-core/src/ast/raw/workflow.rs`:

```rust
// Line 51:
// FROM: pub imports: Option<Spanned<Vec<Spanned<RawImportSpec>>>>,
// TO:   pub include: Option<Spanned<Vec<Spanned<RawIncludeSpec>>>>,
```

**Note:** Also rename `RawImportSpec` → `RawIncludeSpec` for consistency.

### Step 3: Rename in engine

Rename file: `tools/nika-engine/src/ast/import_loader.rs` → `tools/nika-engine/src/ast/include_loader.rs`

In the renamed file, rename all functions:
- `expand_imports()` → `expand_include()`
- `expand_imports_recursive()` → `expand_include_recursive()`
- Update all ~45 test YAML strings from `imports:` to `include:`

In `tools/nika-engine/src/ast/mod.rs`:
```rust
// FROM: pub mod import_loader;
// TO:   pub mod include_loader;

// FROM: pub use import_loader::expand_imports;
// TO:   pub use include_loader::expand_include;
```

In `tools/nika-engine/src/ast/lower.rs`:
```rust
// FROM: fn lower_imports(...)
// TO:   fn lower_include(...)
```

In `tools/nika-engine/src/runtime/runner.rs` (6 locations):
```rust
// FROM: imports: vec![],
// TO:   include: vec![],
```

### Step 4: Run tests

Run: `cargo test --workspace --lib`
Expected: ALL pass (including the new test_parse_include_not_imports)

### Step 5: Commit

```bash
git add -A
git commit -m "refactor(ast): rename imports: → include: — zero legacy

BREAKING: YAML key is now 'include:', not 'imports:'.
No backward compatibility. No deprecation warning.
Aligns parser with JSON schema (which already used 'include:').

Renamed: RawImportSpec → RawIncludeSpec
Renamed: import_loader.rs → include_loader.rs
Renamed: expand_imports() → expand_include()
Updated: ~45 tests, 10 source files

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 3: Template Resolution for `model:` Field

The `model:` field in infer and agent tasks doesn't resolve templates like `{{inputs.deep_model}}`.

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs:229`
- Modify: `tools/nika-engine/src/runtime/executor/agent.rs:126-129`
- Test: `tools/nika-engine/src/runtime/executor/tests.rs`

### Step 1: Write failing test

```rust
#[tokio::test]
async fn test_infer_model_template_resolved() {
    // Create workflow with:
    //   inputs: { my_model: "gpt-4o" }
    //   task with model: "{{inputs.my_model}}"
    //   with: {} (empty bindings)
    //
    // Assert: the resolved model passed to the provider is "gpt-4o"
    // not the literal string "{{inputs.my_model}}"
}
```

### Step 2: Add template resolution to infer.rs

In `tools/nika-engine/src/runtime/executor/infer.rs`, around line 229:

```rust
// BEFORE (broken):
let model = infer.model.as_deref().or(self.default_model.as_deref());

// AFTER (fixed):
let resolved_model = match infer.model.as_deref() {
    Some(m) if m.contains("{{") => {
        let resolved = template_resolve(m, bindings, datastore)?;
        Some(resolved.into_owned())
    }
    Some(m) => Some(m.to_string()),
    None => None,
};
let model = resolved_model.as_deref().or(self.default_model.as_deref());
```

### Step 3: Add template resolution to agent.rs

In `tools/nika-engine/src/runtime/executor/agent.rs`, around line 126:

```rust
// BEFORE (broken):
model: resolved_agent.model.clone()
    .or_else(|| self.default_model.as_ref().map(|m| m.to_string())),

// AFTER (fixed):
model: {
    let raw = resolved_agent.model.clone()
        .or_else(|| self.default_model.as_ref().map(|m| m.to_string()));
    match raw {
        Some(m) if m.contains("{{") => {
            Some(template_resolve(&m, bindings, datastore)?.into_owned())
        }
        other => other,
    }
},
```

### Step 4: Run tests

Run: `cargo test -p nika-engine --lib`
Expected: PASS

### Step 5: Commit

```bash
git add tools/nika-engine/src/runtime/executor/infer.rs \
        tools/nika-engine/src/runtime/executor/agent.rs \
        tools/nika-engine/src/runtime/executor/tests.rs
git commit -m "fix(runtime): resolve templates in model: field for infer + agent

model: '{{inputs.deep_model}}' now resolves to the actual model name.
Previously, the literal template string was passed to the provider.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 4: Extended Thinking — Graceful Degradation

Instead of crashing when `extended_thinking: true` is used with a non-Claude provider, emit a WARN and continue without extended thinking.

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs` (post-resolution validation)
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs` (agent runtime)
- Modify: `tools/nika-core/src/ast/analyzer/analyze.rs` (if parse-time validation exists)
- Test: existing test files

### Step 1: Write failing test

```rust
#[tokio::test]
async fn test_extended_thinking_with_openai_degrades_gracefully() {
    // Create workflow with:
    //   provider: openai
    //   extended_thinking: true
    //
    // Assert: NO crash/error
    // Assert: WARN event emitted
    // Assert: inference completes normally (without thinking)
}
```

### Step 2: Change validation from error to warning

In `tools/nika-engine/src/runtime/executor/infer.rs`, after provider resolution (~line 230):

```rust
// Graceful degradation: extended_thinking with non-Claude provider
if infer.extended_thinking == Some(true) {
    let is_claude = provider_name == "claude";
    if !is_claude {
        tracing::warn!(
            task_id = %task_id,
            provider = %provider_name,
            "extended_thinking: true ignored — only supported by Claude provider"
        );
        self.event_log.emit(EventKind::FeatureSkipped {
            task_id: Arc::clone(task_id),
            feature: "extended_thinking".to_string(),
            reason: format!("Not supported by provider '{}'", provider_name),
        });
        // Clear the flag so downstream code doesn't try to use it
        infer.extended_thinking = Some(false);
    }
}
```

**Note:** May need to add `EventKind::FeatureSkipped` variant if it doesn't exist. If not, use `EventKind::Warning` or similar existing variant.

### Step 3: Same for agent verb

In `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs`, at the provider dispatch:

```rust
// If extended_thinking requested but provider isn't Claude:
if self.params.extended_thinking == Some(true) && !is_claude_provider {
    tracing::warn!("extended_thinking ignored for non-Claude provider");
    // Fall through to normal inference
}
```

### Step 4: Remove crash validation from AST

In `tools/nika-core/src/ast/analyzer/analyze.rs` (and/or `ast/action.rs`):

Find the validation that returns `Err(NikaError::ValidationError { ... "extended_thinking only supported..." })` and change it to a warning diagnostic instead of a hard error.

### Step 5: Run tests

Run: `cargo test --workspace --lib`
Expected: PASS (old tests that expected error now expect warn)

### Step 6: Commit

```bash
git add -A
git commit -m "fix(runtime): extended_thinking gracefully degrades for non-Claude providers

Instead of crashing with ValidationError, emit WARN and continue
without extended thinking. Users can write provider-agnostic workflows
with extended_thinking: true — it activates on Claude, silently
degrades on OpenAI/Gemini/etc.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Task 5: Telemetry + Error Reporting Improvements

Make structured output failures more transparent to users.

**Files:**
- Modify: `tools/nika-engine/src/runtime/structured_output.rs`
- Modify: `tools/nika-engine/src/display/format_event.rs`

### Step 1: Improve NIKA-061 error message

Currently: `[NIKA-061] Schema validation failed: Output validation failed: - : "brand" is a required property`

Should include: which layer failed, how many retries were attempted, and a suggestion.

```rust
// In structured_output.rs, the final error return:
Err(NikaError::StructuredOutputFailed {
    code: "NIKA-061",
    message: format!(
        "Structured output failed after {} attempts (Layer 0: {}, Layer 3: {} retries, Layer 4: {}). \
         Last validation errors: {}. \
         Suggestion: simplify the schema or increase max_retries.",
        total_attempts,
        layer0_status,
        layer3_retries,
        layer4_status,
        last_errors,
    ),
})
```

### Step 2: Add structured output telemetry events

Each layer attempt should emit a detailed event visible in `nika trace`:

```rust
self.event_log.emit(EventKind::StructuredOutputAttempt {
    task_id: task_id.clone(),
    layer: 3,
    layer_name: "retry_with_feedback".to_string(),
    attempt: retry_num,
    success: false,
    error: Some(validation_errors.clone()),
    raw_output_preview: Some(raw_output[..200.min(raw_output.len())].to_string()),
});
```

### Step 3: Display layer status in CLI output

In `display/format_event.rs`, format structured output events clearly:

```
⬡ L0: tool_injection ✗ (MaxTurnError — provider doesn't support tool_choice)
⬡ L2: validate ✗ (missing required: "brand", "score")
⬡ L3: retry 1/3 ✗ (still missing: "brand")
⬡ L3: retry 2/3 ✗ (still missing: "brand")
⬡ L4: repair ✗ (repair model returned non-JSON)
✗ NIKA-061: Structured output failed after 5 attempts
  → Suggestion: simplify schema or increase max_retries
```

### Step 4: Run tests + commit

Run: `cargo test -p nika-engine --lib`

```bash
git commit -m "feat(telemetry): detailed structured output layer tracking

Each layer attempt now emits events with layer number, success/fail,
error details, and output preview. NIKA-061 error message includes
attempt count, layer statuses, and actionable suggestion.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Verification: Full Test Suite

After all 5 tasks are complete:

```bash
# Full workspace tests (safe — no keychain)
cargo test --workspace --lib

# Clippy zero warnings
cargo clippy --workspace -- -D warnings

# Test the GEO-SEO workflow with OpenAI
cd /Users/thibaut/Desktop/aaayayaa
nika check main.nika.yaml
nika run main.nika.yaml \
  --input site_url="https://qrcode-ai.com" \
  --input brand="QR Code AI" \
  --provider openai \
  --no-live
```

Expected: All 37 tasks complete, structured output works with OpenAI, no crashes.

---

## Skills to Use

- **@rust-core** — For Rust patterns, error handling, serde
- **@rust-async** — For tokio async patterns in the provider code
- **@test-driven-development** — TDD for every task (RED → GREEN → REFACTOR)
- **@spn-powers:executing-plans** — To execute this plan task-by-task
