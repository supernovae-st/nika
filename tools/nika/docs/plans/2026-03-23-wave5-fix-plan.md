# Wave 5 Bug Fix Plan — v0.41.2

> **Copy-paste into a fresh Claude Code chat.**

## Context

Nika v0.41.1+. 5-agent deep hunt found 2 CRITICAL + 7 HIGH + 11 MEDIUM bugs.
Memory: `project_v041_wave5_findings.md` — full details with file:line.

## PHASE 1 — 2 CRITICAL (parallel agents, independent files)

### CRITICAL-1: chat_continue provider parity
**Files**: `nika-engine/src/runtime/rig_agent_loop/chat.rs`
**Bug**: 5 providers (mistral/groq/deepseek/gemini/xai) use bare `client.agent(model)`
in chat_continue, missing preamble/temperature/skills/tool_choice/stop_sequences.
**Fix**: Refactor to `chat_continue_generic_impl()` matching `run_generic_provider_impl()`
pattern. All 7 providers should use the same AgentBuilder path with full configuration.

### CRITICAL-2: MCP reconnect stale cache
**Files**: `nika-mcp/src/client.rs:805-821`
**Bug**: `reconnect()` calls `adapter.reconnect()` directly, bypassing `self.disconnect()`
which clears validator cache + response cache.
**Fix**: Call `self.disconnect()` then `self.connect()` instead of `adapter.reconnect()`.
Or: add cache clearing to the existing reconnect path.

## PHASE 2 — 4 HIGH (batched agents)

### HIGH-3: Structured output stale retry
**File**: `nika-engine/src/runtime/structured_output.rs:249-267`
**Bug**: Layer 3 retry loop passes original `raw_output` every time, not the LLM's
corrected output from the previous attempt.
**Fix**: Track `current_output` variable, update after each LLM response:
```rust
let mut current_output = raw_output.to_string();
for retry in 1..=max_retries {
    let layer_result = self.try_layer_3(&task_id, &current_output, &schema, retry, total_attempts).await;
    match layer_result {
        Ok(corrected) => { current_output = corrected; /* validate */ }
        Err(e) => { /* use current_output for next retry */ }
    }
}
```

### HIGH-4: Wire calculate_cost_with_cache
**Files**: `nika-engine/src/runtime/rig_agent_loop/providers.rs` (~15 call sites)
**Bug**: `calculate_cost()` used everywhere despite `calculate_cost_with_cache()` existing.
**Fix**: Replace all `calculate_cost(pk, model, input, output)` calls with
`calculate_cost_with_cache(pk, model, input, output, cached)` where `cached` comes from
the streaming result's `cached_input_tokens` field. For calls without cached data, pass 0.

### HIGH-5: chat_continue $0 cost + limit bypass
**File**: `nika-engine/src/runtime/rig_agent_loop/chat.rs`
**Bug**: All chat_continue methods return `total_tokens: 0, cost_usd: 0.0`.
**Fix**: After `agent.chat()` returns, estimate tokens from prompt+response length:
```rust
let est_input = estimate_tokens(prompt.len());
let est_output = estimate_tokens(response.len());
let cost = calculate_cost(pk, model_name, est_input, est_output);
// Return in RigAgentLoopResult
total_tokens: est_input + est_output,
cost_usd: cost,
```

### HIGH-6: MCP template param string coercion
**File**: `nika-engine/src/runtime/executor/verbs.rs:1725-1746`
**Bug**: Template resolution is string-based — `{{with.count}}` resolving to 42 produces
string "42" not number 42 in JSON params.
**Fix**: After template_resolve + from_str, walk the resolved Value tree and attempt
type coercion for values that look like numbers/booleans:
```rust
fn coerce_json_strings(value: &mut Value) {
    match value {
        Value::Object(map) => { for (_, v) in map { coerce_json_strings(v); } }
        Value::Array(arr) => { for v in arr { coerce_json_strings(v); } }
        Value::String(s) => {
            if let Ok(n) = s.parse::<i64>() { *value = Value::Number(n.into()); }
            else if let Ok(n) = s.parse::<f64>() { *value = json!(n); }
            else if s == "true" { *value = Value::Bool(true); }
            else if s == "false" { *value = Value::Bool(false); }
            else if s == "null" { *value = Value::Null; }
        }
        _ => {}
    }
}
```

## PHASE 3 — 3 HIGH + MEDIUM (batched)

### HIGH-7: for_each + decompose validation
**File**: `nika-core/src/ast/analyzer/analyze.rs`
**Fix**: Add validation that warns when both `for_each` and `decompose` are present,
since decompose silently swallows for_each's concurrency/fail_fast settings.

### HIGH-8: is_connection_error add TransportClosed
**File**: `nika-mcp/src/client.rs:827-835`
**Fix**: Add `"transport closed"` to the substring check list.

### HIGH-9: Structured output Layer 3/4 cost
**File**: `nika-engine/src/runtime/structured_output.rs:437,591`
**Fix**: Calculate actual cost from provider/model info instead of hardcoded 0.0.

### MEDIUM fixes (batch):
- schema: without format: json → warn in analyzer
- ForEachCompleted: separate failed vs skipped counts
- is_error dead variable cleanup

## COMMIT PLAN (7 commits)

| # | Message | Files |
|---|---------|-------|
| 1 | `fix(provider): unify chat_continue with full AgentBuilder config` | chat.rs |
| 2 | `fix(mcp): clear caches on reconnect` | client.rs |
| 3 | `fix(runtime): use latest output in structured output retry` | structured_output.rs |
| 4 | `fix(provider): wire calculate_cost_with_cache + chat_continue estimates` | providers.rs, chat.rs, cost.rs |
| 5 | `fix(runtime): MCP param type coercion after template resolve` | verbs.rs |
| 6 | `fix(ast): warn for_each+decompose + schema without format:json` | analyze.rs |
| 7 | `fix(mcp): add TransportClosed to connection error detection` | client.rs |

## HOW TO WORK

1. Read memory `project_v041_wave5_findings.md`
2. Phase 1: 2 parallel agents (CRITICAL, independent files)
3. Phase 2: 4 agents for HIGH fixes
4. Phase 3: remaining HIGH + MEDIUM
5. `cargo check --workspace && cargo test --workspace --lib` after each phase
6. Commit per logical group, push after each batch
7. Full autonomy, ultrathink
