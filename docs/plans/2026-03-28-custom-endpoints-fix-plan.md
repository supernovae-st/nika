# Custom Endpoints Fix Plan — Post-Session Fixes

**Date**: 2026-03-28
**Context**: After a mega session deploying VPS + H100 + vLLM + NLLB, 5 code review agents found 2 CRITICAL + 7 IMPORTANT + 10 schema gaps in the custom endpoints/provider system.
**Goal**: Fix everything to make `provider: qwen` + custom endpoints production-ready.

---

## Priority 1 — CRITICAL (Blocks correct operation)

### Fix 1.1: Box::leak memory leak in RigProvider::name() and default_model()

**Files**: `tools/nika-engine/src/provider/rig.rs` lines 460, 491
**Problem**: `Box::leak(format!("openai-compat:{}", endpoint_name).into_boxed_str())` permanently leaks memory on every call for custom endpoint providers. Called on every ProviderCalled event, every ProviderResponded event, and during verify().
**Fix**: Store the formatted name eagerly when the OpenAiCompat variant is constructed. Change the `name()` method return type from `&'static str` to `Cow<'static, str>` or `String`, OR store the name as a field in the variant.
**Approach**: Add `name: String` field to `OpenAiCompat` variant, set it during construction. Return `&self.name` from `name()`.
**Tests**: Verify no memory growth in a loop creating 1000 OpenAiCompat providers.

### Fix 1.2: Custom endpoint cost always reported as $0.00

**Files**: `tools/nika-engine/src/runtime/executor/infer.rs` lines 690, 796, 1097 + `tools/nika-engine/src/provider/cost.rs`
**Problem**: `ProviderKind::parse("qwen")` returns `None` for custom endpoint names. All cost calculations silently return 0.0.
**Fix**: When provider is OpenAiCompat, check if it wraps a known provider (OpenAI-compatible = use OpenAI pricing as estimate). Add `ProviderKind::OpenAiCompat` variant or resolve cost through the RigProvider enum directly.
**Approach**: Add a `cost_kind()` method to `RigProvider` that returns `ProviderKind::OpenAi` for `OpenAiCompat` variants (since they use OpenAI-compatible API, pricing is roughly similar). This gives a cost estimate rather than $0.00.
**Tests**: Verify cost > 0 when using a custom endpoint name.

### Fix 1.3: Custom endpoints NOT wired in Runner (main.rs)

**Files**: `tools/nika/src/main.rs` ~line 1680, `tools/nika-engine/src/runtime/runner.rs`, `tools/nika-tui/src/app/routing.rs`, `tools/nika-tui/src/lib.rs`
**Problem**: `NikaConfig::load().resolve_endpoints()` returns the endpoint map, but `Runner::new()` in main.rs does NOT pass it to the TaskExecutor. The endpoints are loaded then dropped.
**Fix**: Add `with_endpoints()` builder to Runner. Wire it in main.rs, TUI routing.rs, and lib.rs.
**Approach**:
1. Add `custom_endpoints: Option<Arc<CustomEndpointMap>>` to `Runner`
2. Add `Runner::with_endpoints(map)` builder method
3. In main.rs: `NikaConfig::load()?.with_env().resolve_endpoints()? -> runner.with_endpoints(map)`
4. Same pattern in TUI routing.rs and lib.rs
**Tests**: Integration test: create Runner with mock endpoints, run workflow with custom provider name, verify it resolves.

### Fix 1.4: Schema JSON mismatches (10 gaps)

**File**: `tools/nika/schemas/nika-workflow.schema.json` + `tools/nika-engine/schemas/nika-workflow.schema.json`
**Problems found by schema review agent**:
1. ~~Provider enum hardcoded~~ (DONE — enum removed)
2. Schema says `imports`, parser reads `include` — rename in schema
3. ~~`base_url` missing~~ (DONE — added at workflow level, need task level too)
4. `pkg` block missing from schema
5. Decompose-only tasks fail oneOf validation
6. MCP `url`/`transport` missing, `command` wrongly required
7. InvokeParams oneOf blocks `server::tool` pattern
**Fix**: Update schema to match parser/AST exactly. Generate a test that validates all 498 existing workflows against the schema.
**Tests**: `cargo test` that loads every .nika.yaml in the repo and validates against schema.

---

## Priority 2 — IMPORTANT (Wrong behavior, not blocking)

### Fix 2.1: NIKA-035/036 error codes collapsed to NIKA-030/031

**File**: `tools/nika-engine/src/error_domains.rs` lines 70-77
**Problem**: `From<ProviderError> for NikaError` maps `EndpointNotFound` to `ProviderNotConfigured` (NIKA-030) and `EndpointConnectionFailed` to `ProviderApiError` (NIKA-031). The specific error codes NIKA-035/036 are documented but never emitted.
**Fix**: Add `NikaError::EndpointNotFound` (NIKA-035) and `NikaError::EndpointConnectionFailed` (NIKA-036) variants. Update the `From` impl.
**Tests**: Test that custom endpoint not found produces NIKA-035, not NIKA-030.

### Fix 2.2: Thinking mode content not stripped from responses

**File**: `tools/nika-engine/src/runtime/executor/infer.rs` (streaming path)
**Problem**: Qwen3.5 with thinking mode ON (default) emits `<think>...</think>` blocks in the response. Nika displays these raw. Even with thinking OFF via vLLM, some content leaks through. The `--default-chat-template-kwargs` only affects the chat template, not the model's internal behavior.
**Fix**: Add a post-processing step in the streaming response handler that strips `<think>...</think>` blocks from the final output. This should be provider-agnostic (any model might emit thinking blocks).
**Approach**: After collecting the full streamed response, apply `regex::Regex::new(r"<think>[\s\S]*?</think>").unwrap().replace_all(&response, "").trim()`.
**Tests**: Test with a response containing `<think>thoughts</think>Actual response` → should return `Actual response`.

### Fix 2.3: ProviderCalled event missing endpoint URL

**File**: `tools/nika-event/src/log.rs` line 222
**Problem**: `ProviderCalled` has no `base_url` or `endpoint_label` field. Traces and display can't show which server was used.
**Fix**: Add `endpoint_url: Option<String>` to `ProviderCalled` and `ProviderResponded`. Populate from the resolved provider in infer.rs.
**Tests**: Verify event serialization includes endpoint_url when set.

### Fix 2.4: TUI never shows provider name, only model

**File**: `tools/nika-tui/src/widgets/task_box/infer.rs` line 338, `tools/nika-tui/src/state/event_handler/provider.rs`
**Problem**: `task.provider` is stored but never rendered. The task box only shows `model: <icon> <model_name>`. Icon heuristic misses Qwen (shows generic 💬).
**Fix**: Show `provider / model` in the task box. Add `"qwen"` to the icon heuristic (use 🌐 or similar).
**Tests**: Snapshot test for task box with custom endpoint provider.

### Fix 2.5: Summary ProviderCallStat missing provider/model fields

**File**: `tools/nika-engine/src/display/renderer.rs` lines 152-159
**Problem**: `ProviderCallStat` only has `task_id`, token counts, cost, TTFT. No provider or model. The summary table can't show which endpoint each task used.
**Fix**: Add `provider: String` and `model: String` fields. Populate from ProviderCalled events.
**Tests**: Test summary table rendering with provider column.

### Fix 2.6: supports_native_structured_output free function diverges from method

**File**: `tools/nika-engine/src/provider/rig.rs` line 139
**Problem**: Free function `supports_native_structured_output(provider_name: &str)` only matches known providers. Returns false for custom endpoints even though they support response_format. The method on `RigProvider` correctly returns true for `OpenAiCompat`.
**Fix**: Remove or deprecate the free function. Use the method everywhere.
**Tests**: Test that custom endpoint reports supports_native_structured_output = true.

### Fix 2.7: verify() uses 5-minute timeout for custom endpoints

**File**: `tools/nika-engine/src/provider/rig.rs` line 1029
**Problem**: Health check via `verify()` calls `self.infer("Hi", None)` with INFER_TIMEOUT (5 min). For a custom endpoint that's down, user waits 5 minutes.
**Fix**: Add a dedicated `health_check()` method with 10s timeout that calls GET /v1/models instead of POST /v1/chat/completions.
**Tests**: Test health check with a mock server that takes >10s → should timeout.

### Fix 2.8: config.rs has_any_key() blind to non-Anthropic/OpenAI providers

**File**: `tools/nika-engine/src/config.rs` lines 125-141
**Problem**: `ApiKeys` struct only has `anthropic` and `openai`. `has_any_key()` and `default_provider()` don't check env vars for Mistral/Groq/DeepSeek/Gemini/xAI.
**Fix**: Make `has_any_key()` also check the 7 provider env vars directly. Or expand `ApiKeys` struct.
**Tests**: Test `has_any_key()` returns true when only GEMINI_API_KEY is set.

---

## Priority 3 — NICE TO HAVE (Polish)

### Fix 3.1: Add health check at workflow startup for custom endpoints
### Fix 3.2: Add per-endpoint timeout from config.toml
### Fix 3.3: Add fallback chain config (endpoint.fallback = "claude")
### Fix 3.4: Add EndpointHealthCheck event kind
### Fix 3.5: Improve inline base_url to also check NIKA_INLINE_API_KEY env var

---

## Build Sequence

```
Phase A: Critical fixes (compile + test)
  1.1 Box::leak fix
  1.2 Cost tracking fix
  1.3 Runner wiring
  1.4 Schema sync

Phase B: Important fixes
  2.1 NIKA-035/036 error codes
  2.2 Think-tag stripping
  2.3 Event endpoint URL
  2.4 TUI provider display
  2.5 Summary table
  2.6 Structured output function
  2.7 verify() timeout
  2.8 config has_any_key()

Phase C: Polish
  3.1-3.5 health check, timeout, fallback
```

## Test Strategy

- Run `cargo test -p nika-engine --lib` after each fix (8400+ tests must pass)
- Add specific tests for each fix
- Run `cargo clippy --workspace -- -D warnings` at the end
- Integration test: Nika workflow with `provider: qwen` resolving to custom endpoint
