# v0.51 Bug Fix, Refactor & Hardening Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix remaining bugs from v0.50 audit, harden media pipeline, improve telemetry, clean dead code, document limitations, and refactor for maintainability.

**Architecture:** 4 waves — security/correctness first, then telemetry, then documentation, then refactoring. Each wave is independently shippable.

**Tests at start:** 8633 passed, 0 failures

---

## Wave 1: Bug Fixes (Security + Correctness)

### Task 1.1: MCP cache hit race — return cache status in result

**Files:**
- Modify: `tools/nika-mcp/src/client.rs`

**Problem:** `last_cache_hit: AtomicBool` (line 447) is shared across concurrent tool calls. Thread A's cache hit gets overwritten by Thread B's miss at line 947.

**Fix:** Add `was_cached: bool` to `ToolCallResult` struct. Set it in `call_tool()` and `call_tool_with_retry_events()` instead of the shared AtomicBool. Remove the AtomicBool field. Update `was_last_call_cached()` to log deprecation or remove it. Update caller in `invoke.rs:450`.

**Test:** Write concurrent tool call test with 2 cached + 1 uncached call, verify each reports correct cache status.

---

### Task 1.2: MCP reconnect validator cache rebuild

**Files:**
- Modify: `tools/nika-mcp/src/client.rs:807-827`

**Problem:** `reconnect()` calls `disconnect()` which clears validator schema cache (line 784), then re-establishes transport but never calls `list_tools()` to rebuild schemas.

**Fix:** After `adapter.reconnect()` (line 824) and `connected.store(true)`, add the same validator population block from `connect()` (lines 741-756):

```rust
if let Some(ref validator) = self.validator {
    let tools = adapter.list_tools().await?;
    validator.cache().populate(&self.name, &tools)?;
    tracing::debug!(mcp_server = %self.name, tools_cached = tools.len(),
        "Re-populated tool schemas after reconnect");
}
```

**Test:** Mock reconnect scenario, verify schema cache is non-empty after reconnect.

---

### Task 1.3: Resource blob error tracking

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/invoke.rs:364-397`

**Problem:** Resource blob processing errors are logged but not tracked as fatal. Tool call media errors (lines 294-310) fail the task on non-recoverable errors, but resource blob errors (lines 384-391) silently continue.

**Fix:** Add `let mut fatal_error: Option<MediaError> = None;` before the loop. In the `Err` arm, add `if fatal_error.is_none() && !error.is_recoverable() { fatal_error = Some(error); }`. After the loop, `if let Some(error) = fatal_error { return Err(NikaError::MediaError(error)); }`.

**Test:** Mock resource with blob that fails processing, verify task fails instead of succeeding.

---

### Task 1.4: Streaming timeout gap

**Files:**
- Modify: `tools/nika-engine/src/provider/rig.rs`

**Problem:** `infer_stream()` (line 1468) and `infer_stream_with_options()` (line 1684) have NO overall timeout — only 60s per-chunk. A slow stream (1 chunk every 59s) never times out. Non-streaming paths have 300s total.

**Fix:** Wrap the entire streaming execution in `tokio::time::timeout(INFER_TIMEOUT, ...)`:

```rust
const STREAM_TOTAL_TIMEOUT: Duration = Duration::from_secs(600); // 10 min for streaming

let stream_result = tokio::time::timeout(STREAM_TOTAL_TIMEOUT, async {
    // existing streaming logic
}).await.map_err(|_| RigInferError::Timeout {
    duration_ms: STREAM_TOTAL_TIMEOUT.as_millis() as u64,
})??;
```

Use 600s (10 min) for streaming since streams legitimately take longer than single calls.

**Test:** Existing timeout tests should still pass. Add test that verifies overall timeout fires.

---

### Task 1.5: Remove dead media-compression cfg guard

**Files:**
- Modify: `tools/nika-engine/src/runtime/runner.rs:558-585`

**Problem:** `#[cfg(not(feature = "media-compression"))]` guard on size integrity check is dead code — `media-compression` is a default feature. The check never runs.

**Fix:** Remove the entire `#[cfg(not(feature = "media-compression"))]` block and the `match std::fs::metadata()` inside it. The comment correctly explains that compressed files have different sizes, and the existence check at line 548 is sufficient. Dead code → delete it.

---

## Wave 2: Telemetry Hardening

### Task 2.1: Emit ProviderResponded on agent limit-exceeded paths

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs`

**Problem:** When `LimitAction::Fail` fires, the method returns `Err(...)` WITHOUT emitting a ProviderResponded event, creating orphaned ProviderCalled events. Affects all 7 provider methods at ~10 locations.

**Fix:** Before returning the error in each limit-exceeded `Fail` branch, emit:

```rust
self.event_log.emit(EventKind::ProviderResponded {
    task_id: Arc::from(self.task_id.as_str()),
    provider: provider_name.to_string(),
    model: model_name.clone(),
    input_tokens: total_input_tokens,
    output_tokens: total_output_tokens,
    cache_read_tokens: total_cached_input_tokens,
    cost_usd: self.limit_tracker.cost_usd(),
    ttft_ms: None,
    elapsed_ms: start.elapsed().as_millis() as u64,
    finish_reason: format!("limit_exceeded:{}", exceeded.limit_type),
});
```

**Test:** Run agent with `max_turns: 1` and `on_limit_reached: { action: fail }`, verify ProviderResponded event exists in trace.

---

### Task 2.2: Emit McpRetry events

**Files:**
- Modify: `tools/nika-mcp/src/client.rs` — find retry logic in `call_tool_with_retry_events()`

**Problem:** `McpRetry` event (log.rs:315-328) is defined with full structure but never emitted despite MCP having retry logic.

**Fix:** In the retry loop of `call_tool_with_retry_events()`, emit `EventKind::McpRetry` before each retry attempt.

---

### Task 2.3: Remove 5 dead event types

**Files:**
- Modify: `tools/nika-event/src/log.rs`

**Problem:** 5 event types are defined but never emitted and have no implementation path: `McpConnected`, `McpError`, `BindingDefaultApplied`, `BindingTransformApplied`, `BindingEnvResolved`.

**Fix:** Remove the 5 variants. Keep `NativeModelLoaded` (will be used in native inference boot). Keep `GuardrailEscalation` (partially used).

---

### Task 2.4: Capture TTFT for agent verb

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs`

**Problem:** Agent turns always report `ttft_ms: None` despite streaming API being available. The `stream_result.ttft_ms` field exists but is not captured in agent provider methods.

**Fix:** In each provider's ProviderResponded emission, set `ttft_ms: stream_result.ttft_ms` instead of `None`. The streaming infrastructure already calculates TTFT.

---

## Wave 3: Documentation & Validation

### Task 3.1: LLM guardrails — add validation warning

**Files:**
- Modify: `tools/nika-core/src/ast/guardrails.rs`
- Modify: `tools/nika-core/src/ast/analyzer/analyze.rs`

**Problem:** `type: llm` guardrails are parsed and validated but silently skipped at runtime. No warning, no error. Users think they work.

**Fix:** In the analyzer, when a task has `guardrails:` with `type: llm`, emit a warning:

```rust
if guardrails.iter().any(|g| matches!(g, GuardrailConfig::Llm(_))) {
    warnings.push(AnalyzeWarning::new(
        "LLM guardrails (type: llm) are not yet supported — will be silently skipped. Coming in v0.51.",
        span,
    ));
}
```

Also update the docstring on `run_sync_guardrails()` to say "LLM guardrails are parsed but not executed."

---

### Task 3.2: Extended thinking — add tools conflict warning

**Files:**
- Modify: `tools/nika-engine/src/ast/agent.rs:346-420` (AgentParams::validate)

**Problem:** `extended_thinking: true` with `tools:` silently ignores all tools. No warning.

**Fix:** In `validate()`, add:

```rust
if self.extended_thinking == Some(true) && !self.tools.is_empty() {
    tracing::warn!(
        tools = ?self.tools,
        "extended_thinking: true disables tool calling — tools will be ignored. \
         Extended thinking is single-turn, text-only mode."
    );
}
```

Also add module-level doc comment documenting limitations:
- No tool calling
- Single turn only (no multi-turn agent loop)
- Temperature forced to 1.0
- No confidence retries

---

## Wave 4: Refactoring & Cleanup

### Task 4.1: Split rig.rs into focused modules

**Files:**
- Split: `tools/nika-engine/src/provider/rig.rs` (3,598 LOC) into:
  - `tools/nika-engine/src/provider/rig/mod.rs` — RigProvider enum + auto() factory
  - `tools/nika-engine/src/provider/rig/tool.rs` — NikaMcpTool implementation
  - `tools/nika-engine/src/provider/rig/stream.rs` — StreamResult + consume_rig_stream
  - `tools/nika-engine/src/provider/rig/error.rs` — McpToolError, RigInferError

**Effort:** 6-8 hours. No behavior change, pure code organization.

---

### Task 4.2: Consolidate provider runner methods

**Files:**
- Modify: `tools/nika-engine/src/runtime/rig_agent_loop/providers.rs`

**Problem:** `run_mistral()`, `run_groq()`, `run_deepseek()`, `run_gemini()`, `run_xai()` are all 13-line methods that extract model name + create client + delegate to `run_generic_provider_impl()`.

**Fix:** Create a macro or factory method:

```rust
async fn run_standard_provider(
    &mut self,
    provider: ProviderKind,
    fallback_model: &str,
    create_client: impl FnOnce() -> Client,
) -> Result<RigAgentLoopResult, NikaError> {
    let model_name = self.params.model.clone()
        .unwrap_or_else(|| fallback_model.to_string());
    let client = create_client();
    self.run_generic_provider_impl(client, &model_name, Some(provider)).await
}
```

**Effort:** 1-2 hours. -100 LOC.

---

### Task 4.3: Merge duplicate executor test files

**Files:**
- Merge: `tests_extraction_e2e.rs` + `tests_extract_e2e.rs` → single file
- Verify: no duplicate test names or overlapping coverage

**Effort:** 30 min.

---

### Task 4.4: Consolidate media test PR files (post-stabilization)

**Files:**
- Merge: `tests_pr3b_tools.rs`, `tests_pr4_pipelines.rs`, `tests_pr5_integration.rs` → `tests_comprehensive.rs`

**Effort:** 4-6 hours. LOW priority — do after all PR branches merge.

---

## Commit Strategy

```
# Wave 1: Bug Fixes
fix(mcp): return cache_hit in ToolCallResult — eliminate AtomicBool race
fix(mcp): rebuild validator cache after reconnect
fix(runtime): track fatal errors in resource blob processing
fix(provider): add overall timeout to streaming inference paths
chore(runtime): remove dead media-compression cfg guard

# Wave 2: Telemetry
fix(telemetry): emit ProviderResponded on agent limit-exceeded paths
fix(telemetry): emit McpRetry events in MCP retry loop
chore(telemetry): remove 5 dead event types
fix(telemetry): capture TTFT for agent verb turns

# Wave 3: Documentation
fix(ast): warn on unsupported LLM guardrails in analyzer
fix(ast): warn when extended_thinking + tools conflict

# Wave 4: Refactoring
refactor(provider): split rig.rs into focused modules
refactor(agent): consolidate provider runner methods
chore(test): merge duplicate executor test files
```

---

## Priority Matrix

| Task | Severity | Effort | Dependencies |
|------|----------|--------|-------------|
| 1.1 MCP cache race | HIGH | 2h | None |
| 1.2 MCP reconnect | HIGH | 1h | None |
| 1.3 Resource blob | MEDIUM | 30m | None |
| 1.4 Streaming timeout | MEDIUM | 1h | None |
| 1.5 Dead cfg guard | LOW | 15m | None |
| 2.1 ProviderResponded | HIGH | 2h | None |
| 2.2 McpRetry events | MEDIUM | 1h | None |
| 2.3 Dead events | LOW | 30m | None |
| 2.4 Agent TTFT | MEDIUM | 1h | None |
| 3.1 LLM guardrails | MEDIUM | 1h | None |
| 3.2 Extended thinking | MEDIUM | 30m | None |
| 4.1 Split rig.rs | LOW | 6-8h | None |
| 4.2 Provider runners | LOW | 1-2h | None |
| 4.3 Test merge | LOW | 30m | None |

**Total estimated effort:** ~20-24 hours across 4 waves

**Wave 1 alone** (fixes only): ~5 hours — ship as v0.50.1
**Waves 1-3** (fixes + docs): ~12 hours — ship as v0.51.0
**All 4 waves**: ~24 hours — ship as v0.51.0 with refactoring
