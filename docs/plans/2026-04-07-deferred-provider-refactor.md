# Deferred Provider Refactor — Detailed Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.
>
> **Prerequisite:** The Templatable refactor must be merged first. These changes conflict with it.

**Goal:** Fix B5 (agent streaming sends wrong token param for reasoning models) and split the 1964-line god file `rig/mod.rs` into focused modules.

**Architecture:** Extract inference, streaming, and construction code into separate files. Fix the rig-core `.max_tokens()` limitation for reasoning models.

**Tech Stack:** Rust (nika-engine)

---

## Part A: Fix B5 — Agent Streaming max_completion_tokens

### Problem

The agent streaming paths in `streaming.rs` use rig-core's `.max_tokens()` builder method which always serializes as `"max_tokens"` in the JSON body. For OpenAI reasoning models (o-series, gpt-5.x), the API requires `"max_completion_tokens"` instead, causing HTTP 400.

**rig-core limitation:** `additional_params` uses `#[serde(flatten)]` which ADDS fields but does NOT remove existing ones. So injecting `max_completion_tokens` via additional_params would send BOTH fields — OpenAI still rejects it.

### Affected Code Paths

There are **3 paths** where rig-core's `.max_tokens()` is used for OpenAI models:

1. **`streaming.rs:82-83`** — Completion stream path (generic model)
   ```rust
   let request = request_builder.max_tokens(effective_max_tokens).build();
   ```

2. **`streaming.rs:273`** — Agent tool-use path (AgentBuilder)
   ```rust
   let mut builder = AgentBuilder::new(model)
       .preamble(&preamble)
       .tools(tools)
       .max_tokens(effective_max_tokens);
   ```

3. **`rig/mod.rs:1326`** — `infer_with_options` (non-streaming)
   ```rust
   let mut builder = $client.agent(model_id).max_tokens(max_tokens as u64);
   ```

### Research Needed First

**Step 1: Verify rig-core serialization behavior**

```bash
# Find rig-core OpenAI completion request struct
grep -rn "max_tokens" ~/.cargo/registry/src/*/rig-core-0.33.*/src/providers/openai/ | head -20

# Check if max_tokens: None skips serialization
grep -n "skip_serializing_if" ~/.cargo/registry/src/*/rig-core-0.33.*/src/providers/openai/completion/mod.rs
```

Key question: If we DON'T call `.max_tokens()`, does rig-core:
- (a) Send `"max_tokens": null` → OpenAI might reject
- (b) Omit `max_tokens` entirely → we can inject `max_completion_tokens` via additional_params
- (c) Send a default value → wrong

**Step 2: If option (b) — the clean fix**

```rust
// streaming.rs ~line 82
let model_id = self.params.model.as_deref().unwrap_or("");
let caps = nika_core::catalogs::model_capabilities(
    self.params.provider.as_deref().unwrap_or("openai"),
    model_id,
);

let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;

let request = if caps.token_limit_param == TokenLimitParam::MaxCompletionTokens {
    // Don't call .max_tokens() — inject via additional_params instead
    request_builder
        .additional_params(serde_json::json!({
            "max_completion_tokens": effective_max_tokens
        }))
        .build()
} else {
    request_builder.max_tokens(effective_max_tokens).build()
};
```

Apply the same pattern to all 3 paths.

**Step 3: If option (a) or (c) — the raw HTTP fallback**

If rig-core always sends `max_tokens` even when not explicitly set, we need to bypass rig-core entirely for reasoning models and use `raw_openai_compat_infer` (manual HTTP).

This is more invasive: the agent streaming paths would need a complete fork for reasoning models.

### Task A.1: Research rig-core max_tokens serialization

**Files:** Check `~/.cargo/registry/src/*/rig-core-0.33.*/src/`

**Step 1:** Find the CompletionRequest struct and check if `max_tokens` uses `skip_serializing_if = "Option::is_none"`.

**Step 2:** Write a minimal test that builds a request without calling `.max_tokens()` and serializes it to JSON. Check if `max_tokens` appears.

**Step 3:** Document the finding.

**Expected outcome:** If `Option<u64>` with `skip_serializing_if`, then option (b) works and the fix is straightforward.

### Task A.2: Apply fix to streaming.rs (path 1 — completion stream)

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs:78-83`

```rust
use nika_core::catalogs::capabilities::{model_capabilities, TokenLimitParam};

let model_id = self.params.model.as_deref().unwrap_or("");
let provider_id = self.params.provider.as_deref().unwrap_or("openai");
let caps = model_capabilities(provider_id, model_id);
let effective_max_tokens = self.params.effective_max_tokens().unwrap_or(8192) as u64;

let request = match caps.token_limit_param {
    TokenLimitParam::MaxCompletionTokens => {
        request_builder
            .additional_params(serde_json::json!({
                "max_completion_tokens": effective_max_tokens
            }))
            .build()
    }
    TokenLimitParam::MaxTokens => {
        request_builder.max_tokens(effective_max_tokens).build()
    }
};
```

### Task A.3: Apply fix to streaming.rs (path 2 — agent builder)

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/streaming.rs:269-273`

Same pattern — skip `.max_tokens()` for reasoning models, inject via additional_params.

### Task A.4: Apply fix to rig/mod.rs (path 3 — infer_with_options)

**Files:**
- Modify: `tools/nika-engine/src/provider/rig/mod.rs` (~line 1326, inside `build_and_prompt!` macro)

```rust
macro_rules! build_and_prompt {
    ($client:expr) => {{
        let mut builder = $client.agent(model_id);
        // Token limit: use correct param name based on model capabilities
        match caps.token_limit_param {
            TokenLimitParam::MaxCompletionTokens => {
                builder = builder.additional_params(serde_json::json!({
                    "max_completion_tokens": max_tokens
                }));
            }
            TokenLimitParam::MaxTokens => {
                builder = builder.max_tokens(max_tokens as u64);
            }
        }
        // ... rest unchanged
    }};
}
```

Note: `caps` would need to be computed before the macro. Currently the function doesn't have provider context — it operates on `RigProvider` enum. Add a `provider_name()` method or use `self.name()`.

### Task A.5: Test

```bash
cargo check -p nika-engine
cargo test -p nika-core --lib capabilities -v
```

### Task A.6: Commit

```
fix(agent): use max_completion_tokens in streaming paths for reasoning models

rig-core's .max_tokens() always serializes as "max_tokens" which
OpenAI reasoning models (o-series, gpt-5.x) reject with HTTP 400.
Skip .max_tokens() for these models and inject max_completion_tokens
via additional_params instead.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Part B: Split rig/mod.rs God File (1964 lines → ~4 focused modules)

### Current State

```
rig/mod.rs       1964 lines   37 public functions   GOD FILE
rig/tests.rs     2024 lines   mirrors mod.rs structure
rig/stream.rs     248 lines   StreamChunk + consume_rig_stream
rig/tool.rs       245 lines   NikaMcpTool
rig/error.rs      147 lines   RigInferError
```

### Target State

```
rig/mod.rs          ~350 lines   RigProvider enum, dispatch_rig!, public re-exports
rig/construction.rs ~300 lines   from_name, from_name_with_endpoints, constructors, auto
rig/inference.rs    ~500 lines   infer, infer_vision, infer_with_tools, infer_with_options
rig/streaming.rs    ~400 lines   infer_stream, infer_stream_with_options (merge with stream.rs)
rig/compat.rs       ~150 lines   raw_chat_completion, raw_openai_compat_infer
rig/capabilities.rs ~100 lines   is_reasoning_model, effective_temperature_for_model,
                                  supports_native_structured_output, build_response_format_params
rig/tool.rs          245 lines   (unchanged)
rig/error.rs         147 lines   (unchanged)
rig/tests.rs        2024 lines   (restructure test modules to match new files)
```

### Extraction Rules

1. **All methods stay on `impl RigProvider`** — just move the `impl` blocks to separate files
2. **`dispatch_rig!` macro stays in mod.rs** — used across modules
3. **Each file gets its own `impl RigProvider { ... }` block** — Rust allows multiple impl blocks
4. **No public API change** — all re-exported from mod.rs

### Task B.1: Extract rig/capabilities.rs

**Files:**
- Create: `tools/nika-engine/src/provider/rig/capabilities.rs`
- Modify: `tools/nika-engine/src/provider/rig/mod.rs`

Move these functions out of mod.rs:
- `pub fn supports_native_structured_output(provider_name: &str)` (line 68)
- `pub fn build_response_format_params(schema: &Value)` (line 75)
- `pub fn is_reasoning_model(model_id: &str)` (line 88)
- `fn effective_temperature_for_model(model_id: &str, ...)` (line 101)
- `pub fn supports_native_structured_output(&self)` (impl method, line 1549)
- `pub fn is_anthropic(&self)` (line 1562)
- `pub fn supports_vision(&self)` (line 1568)
- `pub fn supports_thinking(&self)` (line 1573)
- `pub fn cost_provider_kind(&self)` (line 605)

In mod.rs, add:
```rust
mod capabilities;
pub use capabilities::{
    is_reasoning_model, supports_native_structured_output,
    build_response_format_params, effective_temperature_for_model,
};
```

**~150 lines extracted.**

### Task B.2: Extract rig/construction.rs

**Files:**
- Create: `tools/nika-engine/src/provider/rig/construction.rs`
- Modify: `tools/nika-engine/src/provider/rig/mod.rs`

Move:
- `pub fn from_name(name: &str)` (line 277)
- `pub fn from_name_with_endpoints(...)` (line 328)
- `pub fn from_name_with_key(...)` (line 351)
- `pub fn claude() -> Self` ... `pub fn xai() -> Self` (lines 431-467)
- `pub fn openai_compat(...)` (line 475)
- `pub fn native() -> Self` (line 516)
- `pub fn auto() -> Option<Self>` (line 1417)
- `pub fn is_configured(&self)` (line 1519)
- `pub fn name(&self)` (line 623)
- `pub fn default_model(&self)` (line 650)
- `OPENAI_COMPAT_PROVIDERS` static table (line 203)
- The `Debug` impl

**~350 lines extracted.**

### Task B.3: Extract rig/compat.rs

**Files:**
- Create: `tools/nika-engine/src/provider/rig/compat.rs`
- Modify: `tools/nika-engine/src/provider/rig/mod.rs`

Move:
- `fn raw_chat_completion(...)` (line 648)
- `fn raw_openai_compat_infer(...)` (line 715)

These are `impl RigProvider` methods but only used for the `OpenAiCompat` variant. Clean separation.

**~120 lines extracted.**

### Task B.4: Extract rig/inference.rs

**Files:**
- Create: `tools/nika-engine/src/provider/rig/inference.rs`
- Modify: `tools/nika-engine/src/provider/rig/mod.rs`

Move:
- `pub async fn infer(...)` (line 780)
- `pub async fn infer_vision(...)` (line 866)
- `pub async fn infer_vision_stream(...)` (line 982)
- `pub async fn infer_with_tools(...)` (line 1140)
- `pub async fn infer_with_options(...)` (line 1293)
- `pub async fn verify(...)` (line 1456)

These need access to `dispatch_rig!` macro — import from `super`.

**~500 lines extracted.**

### Task B.5: Merge streaming into rig/streaming.rs

**Files:**
- Modify: `tools/nika-engine/src/provider/rig/stream.rs` → rename to `streaming.rs`
- Or create new `rig/streaming.rs` and merge

Move from mod.rs:
- `pub async fn infer_stream(...)` (line 1588)
- `pub async fn infer_stream_with_options(...)` (line 1724)
- `async fn infer_stream_with_options_inner(...)` (line 1750)

Merge with existing `stream.rs` content (StreamChunk enum, consume_rig_stream).

**~400 lines total (250 existing + 150 extracted).**

### Task B.6: Update mod.rs with module declarations

After all extractions, mod.rs should contain only:
- Module declarations (`mod capabilities; mod construction; ...`)
- Re-exports (`pub use capabilities::*; pub use construction::*; ...`)
- `RigProvider` enum definition + `dispatch_rig!` macro
- `InferOptions` struct
- Native model loading methods (feature-gated, small)

**Target: ~350 lines.**

### Task B.7: Restructure tests

The test file mirrors mod.rs structure. After extraction, organize tests into submodules:
```rust
// tests.rs
mod capabilities_tests { ... }
mod construction_tests { ... }
mod inference_tests { ... }
mod streaming_tests { ... }
```

No test logic changes — just reorganization.

### Task B.8: Verify

```bash
cargo check --workspace
cargo test -p nika-core --lib -q
cargo clippy -p nika-engine --lib
```

### Task B.9: Commit

```
refactor(provider): split rig/mod.rs god file into focused modules

Extract 1600+ lines from the 1964-line god file into 5 modules:
- rig/capabilities.rs — model checks, provider capability flags
- rig/construction.rs — provider instantiation, auto-detection
- rig/compat.rs — raw OpenAI-compatible HTTP inference
- rig/inference.rs — infer, vision, tools, options
- rig/streaming.rs — streaming inference (merged with stream.rs)

Zero public API changes. All re-exported from mod.rs.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Execution Order

```
A.1  Research rig-core serialization       ~10 min   MUST DO FIRST
A.2  Fix completion stream path            ~10 min
A.3  Fix agent builder path                ~10 min
A.4  Fix infer_with_options path           ~10 min
A.5  Test                                  ~5 min
A.6  Commit                               ~2 min

B.1  Extract capabilities.rs              ~15 min
B.2  Extract construction.rs              ~20 min
B.3  Extract compat.rs                    ~10 min
B.4  Extract inference.rs                 ~20 min
B.5  Merge streaming.rs                   ~15 min
B.6  Update mod.rs                        ~5 min
B.7  Restructure tests                    ~15 min
B.8  Verify                               ~5 min
B.9  Commit                               ~2 min
```

**Total: ~2.5h**

**Prerequisite:** Templatable refactor must be merged and test suite green.

---

## Verification Criteria

- [ ] `cargo check --workspace` passes
- [ ] `cargo test -p nika-core --lib -q` passes
- [ ] `cargo test -p nika-engine --lib -q` passes (including runner tests after Templatable lands)
- [ ] `cargo clippy --all-targets --all-features` clean
- [ ] `rig/mod.rs` is under 400 lines
- [ ] No public API changes (all re-exported)
- [ ] Agent streaming sends `max_completion_tokens` for reasoning models
- [ ] Agent streaming sends `max_tokens` for standard models
- [ ] Custom endpoint models always get `max_tokens` (safe default)
