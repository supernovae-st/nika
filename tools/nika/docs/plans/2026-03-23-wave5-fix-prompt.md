# FIX: Wave 5 bugs — Nika v0.41.2

> **Copy-paste everything below into a fresh Claude Code chat.**

---

# FIX: 20 bugs from Wave 5 deep hunt — Nika v0.41.1+ → v0.41.2

## Context

Nika is a semantic YAML workflow engine for AI tasks. Rust workspace at
`/Users/thibaut/dev/supernovae/nika/tools/` with 11 crates. Current: **v0.41.1+**.

In the previous session, 5 specialized agents scanned the codebase and found **2 CRITICAL +
7 HIGH + 11 MEDIUM** bugs. ALL bugs verified as still present.

Memory file: `project_v041_wave5_findings.md`

## WHAT WAS ALREADY FIXED (don't redo)

- SSRF: CIDR blocklist, redirect policy, host matching, fallback client
- Token budget: atomic reserve/adjust, checked arithmetic, Layer 0 leak
- Event lifecycle: TaskFailed on cancellation + artifact failure
- Security: $env allowlist, TemplateResolved redaction
- DAG: cycle detection in Runner::new(), MCP shutdown_all
- TUI: provider selector bounds, wizard underflow
- Tests: LimitTracker 46 tests, for_each concurrent fail_fast
- Pricing: Claude 4.5/4.6, Gemini 2.5, OpenAI o3/o4-mini
- Dead code: ~8400 LOC removed, 12 deps removed

---

## PHASE 1 — 2 CRITICAL BUGS

### CRITICAL-1: chat_continue 5 providers missing full config

**File**: `nika-engine/src/runtime/rig_agent_loop/chat.rs`

**The bug**: `chat_continue_claude()` (line 148) and `chat_continue_openai()` (line 255) use
the full `AgentBuilder` pattern with preamble, temperature, tool_choice, stop_sequences, and
skills injection. But `chat_continue_mistral()` (line 359), `chat_continue_groq()`,
`chat_continue_deepseek()`, `chat_continue_gemini()`, and `chat_continue_xai()` use a
bare `client.agent(model_name).max_tokens(...).tools(tools).build()` — missing ALL of:
- `inject_skills_into_prompt()` → `.preamble(&preamble)`
- `.temperature(temp)`
- `.tool_choice(tool_choice)`
- `stop_sequences_params()` → `.additional_params(stop_params)`

**Current bare pattern** (mistral, line 375-379):
```rust
let agent = client
    .agent(model_name)
    .max_tokens(effective_max_tokens)
    .tools(tools)
    .build();
```

**Correct full pattern** (claude, line 175-204):
```rust
let preamble = self.inject_skills_into_prompt().await?;
let mut builder = AgentBuilder::new(model)
    .preamble(&preamble)
    .max_tokens(effective_max_tokens);
if let Some(temp) = self.params.effective_temperature() {
    builder = builder.temperature(f64::from(temp));
}
if self.params.has_explicit_tool_choice() {
    let tool_choice = self.params.effective_tool_choice();
    builder = builder.tool_choice(tool_choice.into());
}
if let Some(stop_params) = Self::stop_sequences_params(
    &self.params.provider.clone().unwrap_or_default(),
    &self.params.stop_sequences,
) {
    builder = builder.additional_params(stop_params);
}
let tools = self.tools_as_boxed();
let agent = builder.tools(tools).build();
```

**Fix**: For each of the 5 bare providers, replace the `client.agent()` pattern with
the full `AgentBuilder::new(model)` pattern. The client-specific part is only the `model`
creation — e.g., `rig::providers::mistral::Client::from_env().completion_model(model_name)`.
Everything else (preamble, temperature, tools, etc.) is identical.

Better approach: extract a `build_chat_agent()` helper that takes the `CompletionModel` and
returns the built agent. All 7 providers call this one helper.

---

### CRITICAL-2: MCP reconnect skips cache clearing

**File**: `nika-mcp/src/client.rs:805-821`

**The bug**: `reconnect()` calls `adapter.reconnect()` directly. But `disconnect()` (line 770-787)
is the method that clears the validation schema cache (line 782) and response cache (line 777).
Since `reconnect()` never calls `disconnect()`, after a server crash + reconnect, stale tool
schemas persist and validation gives wrong results.

**Current code** (line 805-821):
```rust
pub async fn reconnect(&self) -> Result<()> {
    if self.is_mock { ... }
    let adapter = self.adapter.as_ref().ok_or_else(|| ...)?;
    adapter.reconnect().await?;
    self.connected.store(true, Ordering::SeqCst);
    Ok(())
}
```

**Fix**: Replace with disconnect + connect pattern:
```rust
pub async fn reconnect(&self) -> Result<()> {
    if self.is_mock {
        self.connected.store(true, Ordering::SeqCst);
        return Ok(());
    }
    // Disconnect clears validator cache + response cache
    self.disconnect().await?;
    self.connect().await
}
```

---

## PHASE 2 — 4 HIGH BUGS

### HIGH-3: Structured output retry uses stale data

**File**: `nika-engine/src/runtime/structured_output.rs:249-267`

**The bug**: Layer 3 retry loop always passes `raw_output` (the ORIGINAL LLM output) to
`try_layer_3()`. Each retry sees the same stale validation errors, not the LLM's corrected
attempt. Retries 2+ are wasted LLM calls.

**Current code** (line 252-256):
```rust
for retry in 1..=max_retries {
    total_attempts += 1;
    let layer_result = self
        .try_layer_3(&task_id, raw_output, &schema, retry, total_attempts)
        .await;
```

**Fix**: Track `current_output`, update after each LLM response:
```rust
let mut current_output = raw_output.to_string();
for retry in 1..=max_retries {
    total_attempts += 1;
    let layer_result = self
        .try_layer_3(&task_id, &current_output, &schema, retry, total_attempts)
        .await;
    match &layer_result {
        Ok(_) => { /* success — will return below */ }
        Err(_) => {
            // try_layer_3 internally gets the LLM's new output.
            // We need to capture it. Check try_layer_3's internals.
        }
    }
```
Note: `try_layer_3` runs the LLM and validates the response internally. You may need to
refactor it to return the raw LLM output even on validation failure, so the next retry
can use the latest attempt. Check the function signature and return type.

### HIGH-4: Wire calculate_cost_with_cache everywhere

**Files**: `nika-engine/src/runtime/rig_agent_loop/providers.rs` (~15 call sites)

**The bug**: `calculate_cost()` is used at all ~15 call sites in providers.rs despite
`calculate_cost_with_cache()` existing in cost.rs. Cached tokens tracked through the
pipeline but discarded at cost calculation time.

**Fix**: Global search-replace in providers.rs:
```
calculate_cost(pk, model, input_tokens, output_tokens)
→
calculate_cost_with_cache(pk, model, input_tokens, output_tokens, cached_tokens)
```
Where `cached_tokens` comes from the streaming result's `.cached_input_tokens` field.
For calls where cached data isn't available, pass `0`.

Also update the import at the top of providers.rs to include `calculate_cost_with_cache`.

### HIGH-5: chat_continue reports $0 cost

**File**: `nika-engine/src/runtime/rig_agent_loop/chat.rs`

**The bug**: All 7 chat_continue methods return `total_tokens: 0, cost_usd: 0.0` in
`RigAgentLoopResult`. Multi-turn agents silently bypass cost limits.

**Fix**: After each `agent.chat()` returns the response string, estimate tokens:
```rust
let est_input = crate::runtime::executor::verbs::estimate_tokens(prompt.len());
let est_output = crate::runtime::executor::verbs::estimate_tokens(response.len());
let provider_name = self.params.provider.as_deref().unwrap_or("unknown");
let model_name_str = model_name; // already in scope
let cost = crate::provider::cost::ProviderKind::parse(provider_name)
    .map(|pk| crate::provider::cost::calculate_cost(pk, model_name_str, est_input, est_output))
    .unwrap_or(0.0);

// In the return:
Ok(RigAgentLoopResult {
    total_tokens: (est_input + est_output) as usize,
    cost_usd: cost,
    // ... rest unchanged
})
```
Note: `estimate_tokens` may not be pub. If so, make it `pub(crate)` or duplicate the
`len.div_ceil(4) as u64` inline.

Apply to ALL 7 chat_continue methods.

### HIGH-6: MCP invoke params string coercion

**File**: `nika-engine/src/runtime/executor/verbs.rs:1725-1746`

**The bug**: After template resolution, JSON params are all strings. `{{with.count}}`
resolving to 42 becomes `"42"` (string), not `42` (number). MCP tools expecting integers fail.

**Fix**: After the template resolve + re-parse, walk the Value tree and coerce string
values that look like numbers/booleans back to their native JSON types:

```rust
/// Coerce string values that look like numbers/booleans back to native JSON types.
/// Applied after template resolution which stringifies all substituted values.
fn coerce_json_types(value: &mut serde_json::Value) {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            for v in map.values_mut() {
                coerce_json_types(v);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                coerce_json_types(v);
            }
        }
        Value::String(s) => {
            // Try integer first (more specific), then float
            if let Ok(n) = s.parse::<i64>() {
                *value = Value::Number(n.into());
            } else if let Ok(n) = s.parse::<f64>() {
                if n.is_finite() {
                    *value = serde_json::json!(n);
                }
            } else if s == "true" {
                *value = Value::Bool(true);
            } else if s == "false" {
                *value = Value::Bool(false);
            } else if s == "null" {
                *value = Value::Null;
            }
            // Otherwise leave as string
        }
        _ => {}
    }
}
```

Apply after the `serde_json::from_str` at the end of the template resolution block:
```rust
let mut resolved = serde_json::from_str::<Value>(&resolved_str)?;
coerce_json_types(&mut resolved);
Some(resolved)
```

---

## PHASE 3 — 3 HIGH + MEDIUM

### HIGH-7: for_each + decompose validation warning
**File**: `nika-core/src/ast/analyzer/analyze.rs`
**Fix**: In the task analysis loop, after parsing both `for_each` and `decompose`, add:
```rust
if task.for_each.is_some() && task.decompose.is_some() {
    ctx.warn(AnalyzeWarning::new(
        "for_each and decompose are both present — decompose takes priority, "
        + "for_each concurrency/fail_fast settings will be ignored",
        task_span,
    ));
}
```
Check if `AnalyzeWarning` exists or if warnings go through a different mechanism.

### HIGH-8: is_connection_error misses TransportClosed
**File**: `nika-mcp/src/client.rs:827-835`
**Fix**: Add `"transport closed"` and `"transport send"` to the substring check list.

### HIGH-9: Structured output Layer 3/4 cost_usd: 0.0
**File**: `nika-engine/src/runtime/structured_output.rs:437,591`
**Fix**: Use `ProviderKind::parse()` + `calculate_cost()` like verbs.rs does.
The engine has `self.provider_name` and `self.model_name` fields.

### MEDIUM batch:
- `verbs.rs:1864`: Remove dead `is_error = false` variable
- `runner.rs:2188`: Split ForEachCompleted into `failed` + `skipped` counts
- `analyze.rs`: Warn when `schema:` present without `format: json`

---

## HOW TO WORK

1. Read this plan FIRST
2. Read memory `project_v041_wave5_findings.md`
3. Phase 1: Launch 2 parallel agents for 2 CRITICAL fixes (independent files)
4. Phase 2: Launch 4 agents for HIGH fixes
5. Phase 3: Remaining HIGH + MEDIUM
6. `cd /Users/thibaut/dev/supernovae/nika/tools && cargo check --workspace && cargo test --workspace --lib`
7. Full autonomy, no questions, ultrathink
8. Commits: `type(scope): desc` with both co-authors:
   ```
   Co-Authored-By: Claude <noreply@anthropic.com>
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
   ```
9. Push after each phase
10. Tag v0.41.2 when all green

## COMMIT PLAN (7 commits)

| # | Message | Files |
|---|---------|-------|
| 1 | `fix(provider): unify chat_continue with full AgentBuilder config` | chat.rs |
| 2 | `fix(mcp): clear caches on reconnect + detect TransportClosed` | client.rs |
| 3 | `fix(runtime): use latest output in structured output retry loop` | structured_output.rs |
| 4 | `fix(provider): wire calculate_cost_with_cache + chat_continue cost estimates` | providers.rs, chat.rs |
| 5 | `fix(runtime): coerce MCP invoke params to native JSON types` | verbs.rs |
| 6 | `fix(ast): warn for_each+decompose coexistence + schema without format:json` | analyze.rs |
| 7 | `fix(runtime): structured output Layer 3/4 cost + dead code cleanup` | structured_output.rs, runner.rs, verbs.rs |

Review after 1-2, after 3-5, after 6-7. Tag v0.41.2.
