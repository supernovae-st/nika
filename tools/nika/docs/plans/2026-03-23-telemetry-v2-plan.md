# Telemetry v2 — Full Observability Plan

> **Status**: DRAFT
> **Author**: Claude + Thibaut
> **Date**: 2026-03-23
> **Scope**: 14 new EventKind variants, ~15 files, 5 phases
> **Baseline**: 43 events (post-telemetry-v1 session)
> **Target**: 57 events — every verb, binding, DAG step, boot phase observable

---

## Architecture Overview

```
Workflow YAML
    |
    v
[Boot Sequence] -----> Phase 2: BootPhaseCompleted, NativeModelLoaded
    |
    v
[DAG Scheduler] -----> Phase 4: ForEachStarted/Completed, Decompose*
    |
    v
[Task Executor]
    |--- infer: -----> already covered (ProviderCalled/Responded)
    |--- exec:  -----> already covered (ExecCompleted, PolicyBlocked)
    |--- fetch: -----> Phase 5: ExtractApplied, LlmTxtProbe
    |--- invoke: ----> already covered (McpInvoke/McpResponse)
    |--- agent: -----> Phase 5: BuiltinToolInvoked
    |
    v
[Binding Resolution] -> Phase 3: BindingDefaultApplied, TransformApplied, EnvResolved
    |
    v
[Structured Output] --> Phase 1: StructuredOutputProviderCall (CRITICAL)
    |
    v
[Artifact Pipeline] --> Phase 5: ArtifactFormatted
    |
    v
[Trace NDJSON] -------> all events land here
```

---

## Phase 1 — CRITICAL: Invisible LLM Calls in Structured Output

**Risk**: CRITICAL — cost tracking broken, LLM calls unaccounted
**Files**: 2 | **Estimated complexity**: Medium
**Commit**: `fix(event): emit ProviderCalled/Responded for structured output retry/repair`

### Problem

`structured_output.rs` Layer 3 (retry with feedback) and Layer 4 (LLM repair) call
`infer_fn()` — a closure over `provider.infer()` — without emitting ProviderCalled or
ProviderResponded. These LLM calls are invisible: no token count, no cost, no request_id.

### Solution

Instead of a new EventKind, **wrap the `infer_fn` callback** to emit events around each call.

#### File 1: `nika-engine/src/runtime/structured_output.rs`

The `StructuredOutputEngine::run()` method receives `infer_fn: F` where
`F: Fn(String) -> Fut<Result<String, NikaError>>`.

**Approach**: Add `event_log: &EventLog` and `task_id: &Arc<str>` parameters to `run()`.
Before each `infer_fn(prompt).await` call (lines ~388 and ~513):

```rust
// EMIT: ProviderCalled (Layer 3 retry)
event_log.emit(EventKind::ProviderCalled {
    task_id: Arc::clone(task_id),
    provider: provider_name.to_string(),
    model: model_name.to_string(),
    prompt_len: retry_prompt.len(),
});

let infer_start = Instant::now();
let new_output = infer_fn(retry_prompt).await?;
let infer_duration = infer_start.elapsed();

// EMIT: ProviderResponded (Layer 3 retry)
// Note: We don't have token counts from infer_fn — use estimate_tokens()
event_log.emit(EventKind::ProviderResponded {
    task_id: Arc::clone(task_id),
    request_id: None,
    input_tokens: estimate_tokens(retry_prompt.len()),
    output_tokens: estimate_tokens(new_output.len()),
    cache_read_tokens: 0,
    ttft_ms: None,
    finish_reason: "structured_output_retry".to_string(),
    cost_usd: 0.0, // Cannot compute without model pricing — acceptable
});
```

Same pattern for Layer 4 repair at line ~513.

#### File 2: `nika-engine/src/runtime/executor/verbs.rs`

Update the `InferCallback` call site (~line 523) to pass `event_log` and `task_id`
to `StructuredOutputEngine::run()`.

### Tests

- [ ] Unit test: `structured_output_retry_emits_provider_events` — run Layer 3 with a
      deliberately failing schema, verify ProviderCalled + ProviderResponded emitted
- [ ] Unit test: `structured_output_repair_emits_provider_events` — same for Layer 4

### Code Review Checklist

- [ ] `event_log` lifetime doesn't create borrow conflicts with `infer_fn` closure
- [ ] `estimate_tokens()` used consistently (same as vision path)
- [ ] No double-counting: the original `infer:` verb ProviderResponded is separate
- [ ] `finish_reason` clearly distinguishes retry vs repair vs normal

---

## Phase 2 — CRITICAL: Boot Telemetry

**Risk**: CRITICAL — startup hangs undiagnosable
**Files**: 3 | **Estimated complexity**: Medium
**Commit**: `fix(event): boot phase telemetry + native model load events`

### Problem

`BootSequence::run()` executes 7 phases with zero events. If boot hangs on MCP startup
or secrets loading, users see nothing. Boot warnings (no config, no daemon, no API keys)
are collected in `PhaseResult.warnings` but never surfaced as events.

### New EventKind Variants (2)

```rust
// ═══════════════════════════════════════════
// BOOT EVENTS
// ═══════════════════════════════════════════
/// Boot phase completed (one per phase: config, secrets, mcp, providers, ready)
BootPhaseCompleted {
    /// Phase name: "config_discovery", "config_validation", "memory_loading",
    /// "secrets_loading", "mcp_startup", "provider_validation", "ready"
    phase: String,
    /// Whether the phase succeeded
    success: bool,
    /// Phase duration in milliseconds
    duration_ms: u64,
    /// Warnings produced during this phase (e.g., "config.toml not found")
    warnings: Vec<String>,
},

/// Native (local) model loaded successfully
NativeModelLoaded {
    /// Model identifier (path for GGUF, HF ID for HuggingFace)
    model: String,
    /// Model kind: "gguf" or "huggingface"
    kind: String,
    /// Model file size in bytes (0 for HuggingFace)
    size_bytes: u64,
    /// Load duration in milliseconds
    duration_ms: u64,
    /// Whether model has vision capabilities
    is_vision: bool,
},
```

### File Changes

#### File 1: `nika-event/src/log.rs`
- Add 2 new variants to EventKind enum
- BootPhaseCompleted: NO task_id (boot is pre-workflow)
- NativeModelLoaded: NO task_id (model loading is global)
- Update `task_id()` match: these return `None`
- Add serialization round-trip tests

#### File 2: `nika-engine/src/runtime/boot.rs`
- Add `event_log: &EventLog` parameter to `BootSequence::run()`
- After each phase result, emit `BootPhaseCompleted`:

```rust
let result = Self::phase_config_discovery(&mut ctx).await;
event_log.emit(EventKind::BootPhaseCompleted {
    phase: "config_discovery".to_string(),
    success: result.success,
    duration_ms: result.duration.as_millis() as u64,
    warnings: result.warnings.clone(),
});
ctx.phases.push(result);
```

Repeat for all 7 phases.

- **Caller update**: `runner.rs` or wherever `BootSequence::run()` is called must pass EventLog.
  If EventLog doesn't exist yet at boot time, create it early.

#### File 3: `nika-engine/src/provider/native/runtime.rs`
- After GGUF load (~line 654): emit `NativeModelLoaded`
- After HuggingFace load (~line 724): emit `NativeModelLoaded`
- **Threading**: NativeRuntime needs access to EventLog.
  Option A: Pass `&EventLog` to `load()`.
  Option B: Store `Arc<EventLog>` in NativeRuntime.
  Prefer Option A (explicit dependency).

### Tests

- [ ] Unit test: `boot_phase_completed_serializes` — round-trip test
- [ ] Unit test: `native_model_loaded_serializes` — round-trip test
- [ ] Integration: Mock boot sequence, verify 7 BootPhaseCompleted events emitted
- [ ] Integration: Verify warnings are captured (test with missing config.toml)

### Code Review Checklist

- [ ] EventLog created before boot sequence starts
- [ ] All 7 phases emit events (not just success cases)
- [ ] Warnings Vec is cloned (not moved) since PhaseResult also stores them
- [ ] NativeModelLoaded duration is measured correctly (Instant before/after)
- [ ] No panic paths between Instant::now() and emit

---

## Phase 3 — HIGH: Binding Telemetry

**Risk**: HIGH — silent fallbacks hide data source issues
**Files**: 3 | **Estimated complexity**: High (threading EventLog through resolve)
**Commit**: `fix(event): binding resolution telemetry — defaults, transforms, env vars`

### New EventKind Variants (3)

```rust
// ═══════════════════════════════════════════
// BINDING EVENTS
// ═══════════════════════════════════════════
/// Binding default value applied (via ?? operator)
BindingDefaultApplied {
    task_id: Arc<str>,
    /// Alias name in with: block
    alias: String,
    /// Original binding path that was null/missing
    path: String,
    /// Default value that was used
    default_value: Value,
},

/// Binding transform chain applied (e.g., |upper|trim|sort)
BindingTransformApplied {
    task_id: Arc<str>,
    /// Alias name
    alias: String,
    /// Transform expression (e.g., "upper | trim")
    transform_chain: String,
    /// Value before transforms (truncated to 200 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    input_preview: Option<String>,
    /// Value after transforms (truncated to 200 chars)
    #[serde(skip_serializing_if = "Option::is_none")]
    output_preview: Option<String>,
},

/// Environment variable resolved via $env.VAR_NAME binding
BindingEnvResolved {
    task_id: Arc<str>,
    /// Environment variable name
    var_name: String,
    /// Whether the env var was found
    found: bool,
},
```

### Architecture Challenge: Threading EventLog

`ResolvedBindings::from_with_spec()` in `resolve.rs` does NOT have access to EventLog.
Call chain: `runner.rs` -> `from_with_spec()` -> `resolve_entry()` -> `resolve_binding_path()`.

**Solution**: Add `event_log: Option<&EventLog>` parameter to `from_with_spec()` and
propagate to inner resolve functions. Use `Option` so tests can pass `None`.

Alternatively, collect events as a `Vec<EventKind>` and emit them after resolution
returns to `runner.rs`. This avoids threading EventLog through 5 function signatures.

**Recommended approach**: Collect-and-emit pattern:

```rust
// In runner.rs, after resolving bindings:
let (bindings, binding_events) = ResolvedBindings::from_with_spec_traced(...)?;
for event in binding_events {
    event_log.emit(event);
}
```

This keeps resolve.rs pure (returns data, no side effects) and avoids the threading problem.

### File Changes

#### File 1: `nika-event/src/log.rs`
- Add 3 variants + task_id() matches + serde tests

#### File 2: `nika-engine/src/binding/resolve.rs`
- Add `binding_events: Vec<EventKind>` accumulator
- In `resolve_entry()` at line ~451 (default fallback): push `BindingDefaultApplied`
- In `resolve_with_entry()` at line ~509 (transform): push `BindingTransformApplied`
- In `resolve_binding_path()` at line ~616 (env var): push `BindingEnvResolved`
- Return events alongside bindings

#### File 3: `nika-engine/src/runtime/runner.rs`
- At binding resolution call site: capture events and emit them

### Tests

- [ ] Unit: `binding_default_applied_when_null` — with: alias with ?? default, verify event
- [ ] Unit: `binding_transform_emits_event` — with: alias with |upper, verify chain captured
- [ ] Unit: `binding_env_resolved_found` — $env.PATH resolves, found=true
- [ ] Unit: `binding_env_resolved_missing` — $env.NONEXISTENT, found=false
- [ ] Integration: Workflow with ?? fallback, check trace.ndjson

### Code Review Checklist

- [ ] `input_preview` and `output_preview` truncated to 200 chars (no memory bomb)
- [ ] Env var VALUE never included in events (security: no API key leaks)
- [ ] Events collected correctly in all code paths (lazy, eager, context)
- [ ] `from_with_spec_traced` doesn't break existing callers (backward compat)

---

## Phase 4 — HIGH: DAG / for_each / Decompose Telemetry

**Risk**: HIGH — parallel execution invisible
**Files**: 3 | **Estimated complexity**: Medium
**Commit**: `fix(event): DAG orchestration telemetry — for_each, decompose lifecycle`

### New EventKind Variants (4)

```rust
// ═══════════════════════════════════════════
// DAG ORCHESTRATION EVENTS
// ═══════════════════════════════════════════
/// Decompose modifier expansion started
DecomposeStarted {
    task_id: Arc<str>,
    /// Strategy: "semantic", "static", "nested"
    strategy: String,
    /// Max depth for nested traversal
    #[serde(skip_serializing_if = "Option::is_none")]
    max_depth: Option<u32>,
    /// Max items limit
    #[serde(skip_serializing_if = "Option::is_none")]
    max_items: Option<u32>,
},

/// Decompose modifier expansion completed
DecomposeCompleted {
    task_id: Arc<str>,
    /// Strategy used
    strategy: String,
    /// Number of items produced
    item_count: usize,
    /// Expansion duration in milliseconds
    duration_ms: u64,
},

/// for_each iteration batch started
ForEachStarted {
    task_id: Arc<str>,
    /// Number of items to iterate over
    item_count: usize,
    /// Concurrency level (1 = sequential)
    concurrency: usize,
    /// Whether fail_fast is enabled
    fail_fast: bool,
},

/// for_each iteration batch completed with aggregated results
ForEachCompleted {
    task_id: Arc<str>,
    /// Total iterations attempted
    total: u32,
    /// Successful iterations
    succeeded: u32,
    /// Failed iterations
    failed: u32,
    /// Total duration across all iterations (ms)
    duration_ms: u64,
},
```

### File Changes

#### File 1: `nika-event/src/log.rs`
- Add 4 variants + task_id() matches + serde tests

#### File 2: `nika-engine/src/runtime/runner.rs`
- Decompose section (~line 1329):
  - Before expansion: emit `DecomposeStarted`
  - After expansion: emit `DecomposeCompleted`
  - On error/timeout: emit `DecomposeCompleted { item_count: 0, ... }`
- for_each section (~line 1708):
  - Before iteration loop: emit `ForEachStarted`
  - After aggregation (~line 2060): emit `ForEachCompleted`

#### File 3: `nika-engine/src/runtime/executor/decompose.rs`
- Pass task_id through for event context
- No direct EventLog access needed (events emitted from runner.rs)

### Tests

- [ ] Unit: `decompose_started_serializes` — round-trip
- [ ] Unit: `for_each_started_serializes` — round-trip
- [ ] Integration: Workflow with `for_each: [1,2,3]`, verify ForEachStarted(item_count=3)
      and ForEachCompleted(total=3, succeeded=3)
- [ ] Integration: Workflow with decompose, verify DecomposeStarted + DecomposeCompleted

### Code Review Checklist

- [ ] DecomposeCompleted emitted even on failure (item_count=0)
- [ ] ForEachCompleted counts match actual iteration results
- [ ] Concurrency value comes from task config (not hardcoded)
- [ ] Duration measured from start to aggregation complete

---

## Phase 5 — HIGH: Remaining Visibility Gaps

**Risk**: HIGH (mixed) | **Files**: 5 | **Estimated complexity**: Low-Medium
**Commit**: `fix(event): provider init, builtin tools, extract mode, artifact format`

### New EventKind Variants (5)

```rust
// ═══════════════════════════════════════════
// PROVIDER EVENTS
// ═══════════════════════════════════════════
/// Provider initialized (first use or cache miss)
ProviderInitialized {
    /// Provider name (anthropic, openai, mistral, etc.)
    provider: String,
    /// Default model for this provider
    model: String,
    /// Whether this was served from cache
    cached: bool,
},

// ═══════════════════════════════════════════
// BUILTIN TOOL EVENTS
// ═══════════════════════════════════════════
/// Builtin tool invoked by agent (nika:read, nika:write, etc.)
BuiltinToolInvoked {
    task_id: Arc<str>,
    /// Tool name: "nika:read", "nika:write", "nika:search", etc.
    tool_name: String,
    /// Call duration in milliseconds
    duration_ms: u64,
    /// Whether the call succeeded
    success: bool,
},

// ═══════════════════════════════════════════
// FETCH EXTRACT EVENTS
// ═══════════════════════════════════════════
/// Extraction mode applied to fetch response
ExtractApplied {
    task_id: Arc<str>,
    /// Extract mode: "css", "jq", "text", "markdown", "llm_txt"
    mode: String,
    /// CSS/jq selector used (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    selector: Option<String>,
    /// Input body length (bytes)
    input_len: usize,
    /// Output length after extraction (bytes)
    output_len: usize,
},

/// llm.txt probe attempted on a URL
LlmTxtProbe {
    task_id: Arc<str>,
    /// Base URL probed
    base_url: String,
    /// Whether any llm.txt file was found
    found: bool,
    /// Which path succeeded (e.g., "/.well-known/llm.txt")
    #[serde(skip_serializing_if = "Option::is_none")]
    found_path: Option<String>,
    /// Number of paths tried before success/exhaustion
    paths_tried: u32,
},

// ═══════════════════════════════════════════
// ARTIFACT FORMAT EVENTS
// ═══════════════════════════════════════════
/// Artifact output formatted (JSON pretty-print, YAML conversion, etc.)
ArtifactFormatted {
    task_id: Arc<str>,
    /// Format applied: "json", "yaml", "text", "binary"
    format: String,
    /// Input size (raw output bytes)
    input_len: usize,
    /// Output size (formatted bytes)
    output_len: usize,
},
```

### File Changes

#### File 1: `nika-event/src/log.rs`
- Add 5 variants + task_id() + serde tests
- ProviderInitialized has NO task_id (global scope) — returns None from task_id()

#### File 2: `nika-engine/src/runtime/executor/mod.rs` (~line 282)
- In `get_rig_provider()`: emit `ProviderInitialized` on cache miss (Vacant entry)
- Need EventLog access — add `&EventLog` field to TaskExecutor struct

#### File 3: `nika-engine/src/runtime/builtin/rig_adapter.rs` (~line 104)
- In `NikaBuiltinToolAdapter::call()`: emit `BuiltinToolInvoked` for ALL tools
- Currently only log/emit get special handling
- Add timing around the `self.router.dispatch()` call

#### File 4: `nika-engine/src/runtime/executor/verbs.rs` (~line 1594)
- Before `apply_extract()` return: emit `ExtractApplied`
- In llm_txt section (~line 1526): emit `LlmTxtProbe` after loop completes

#### File 5: `nika-engine/src/runtime/artifact_processor.rs` (~line 325)
- After `format_output()`: emit `ArtifactFormatted`

### Tests

- [ ] Unit: 5 serialization round-trip tests
- [ ] Unit: `provider_initialized_on_cache_miss` — first call emits, second doesn't
- [ ] Unit: `builtin_tool_invoked_for_read` — agent calls nika:read, event emitted
- [ ] Unit: `extract_applied_css` — fetch with extract:css, event has selector + lengths
- [ ] Integration: Workflow with `extract: markdown`, verify ExtractApplied in trace

### Code Review Checklist

- [ ] ProviderInitialized NOT emitted on cache hit (only first creation)
- [ ] BuiltinToolInvoked doesn't duplicate McpInvoke (only in agent context)
- [ ] ExtractApplied emitted BEFORE return (not after — function returns early)
- [ ] LlmTxtProbe.paths_tried counts actual HTTP attempts (not skipped ones)
- [ ] ArtifactFormatted.input_len vs output_len shows formatting overhead

---

## E2E Validation Plan

### Test Workflow: `telemetry_e2e.nika.yaml`

Create a workflow that exercises ALL 14 new events in one run:

```yaml
nika: workflow@0.12
name: telemetry-e2e-validation

tasks:
  # Phase 1: Structured output retry → StructuredOutputProviderCall
  validate_json:
    infer:
      prompt: "Return a JSON object with name and age"
      provider: anthropic
      model: claude-3-haiku-20240307
    output:
      format: json
      schema:
        type: object
        required: [name, age]
        properties:
          name: { type: string }
          age: { type: integer }

  # Phase 3: Binding events
  with_defaults:
    exec:
      command: "echo done"
    with:
      fallback_val: $validate_json.nonexistent ?? "default_value"
      upper_name: $validate_json.name | upper

  # Phase 4: for_each
  iterate:
    exec:
      command: "echo processing {{with.item}}"
    with:
      item: $iterate
    for_each: ["alpha", "beta", "gamma"]
    concurrency: 2

  # Phase 5: fetch with extract
  fetch_docs:
    fetch:
      url: "https://example.com"
      extract: text
```

### Validation Script

```bash
#!/bin/bash
# Run workflow and validate trace events
nika run telemetry_e2e.nika.yaml 2>/dev/null

# Check trace file for expected events
TRACE=".nika/trace.ndjson"

check_event() {
  if grep -q "\"$1\"" "$TRACE"; then
    echo "  OK  $1"
  else
    echo "  FAIL  $1"
    FAILED=1
  fi
}

echo "=== Telemetry v2 E2E Validation ==="
check_event "boot_phase_completed"
check_event "binding_default_applied"
check_event "binding_transform_applied"
check_event "for_each_started"
check_event "for_each_completed"
check_event "extract_applied"
check_event "provider_initialized"

[ -z "$FAILED" ] && echo "ALL PASS" || echo "SOME FAILED"
```

---

## Commit Sequence

| Order | Commit Message | Files | Tests |
|-------|---------------|-------|-------|
| 1 | `fix(event): emit ProviderCalled/Responded for structured output retry/repair` | structured_output.rs, verbs.rs | 2 unit |
| 2 | `fix(event): boot phase telemetry + native model load events` | log.rs, boot.rs, runtime.rs | 4 unit + 1 integration |
| 3 | `fix(event): binding resolution telemetry — defaults, transforms, env vars` | log.rs, resolve.rs, runner.rs | 5 unit + 1 integration |
| 4 | `fix(event): DAG orchestration — for_each, decompose lifecycle` | log.rs, runner.rs, decompose.rs | 4 unit + 2 integration |
| 5 | `fix(event): provider init, builtin tools, extract mode, artifact format` | log.rs, mod.rs, rig_adapter.rs, verbs.rs, artifact_processor.rs | 6 unit + 1 integration |
| 6 | `test(event): telemetry v2 e2e validation workflow` | telemetry_e2e.nika.yaml, validation script | 1 e2e |

**Code review**: After commits 1-2 (CRITICAL) and after commits 3-5 (HIGH).

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| EventLog threading through boot | Create EventLog early, before boot |
| EventLog threading through resolve.rs | Collect-and-emit pattern (no threading) |
| Performance: too many events | All new events are LOW frequency (boot=7, bindings=per-task, for_each=per-batch) |
| Structured output event_log borrow conflict | Pass EventLog ref, not closure capture |
| Breaking existing tests | New fields use `#[serde(skip_serializing_if)]`, pattern matches use `..` |
| NativeRuntime doesn't have EventLog | Pass as parameter to `load()` |

---

## Success Criteria

After all 5 phases:
- `cargo test --workspace --lib` — 0 failures
- `cargo clippy --workspace` — 0 warnings
- E2E validation script — ALL PASS
- Trace file contains all 14 new event types when exercised
- TUI handles new events gracefully (no crash, event_handler catch-all)
- Total event count: **57 EventKind variants**
