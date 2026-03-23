# Deep Bug Hunt Fix — v0.41.0 → v0.41.1

> **Copy-paste everything below into a fresh Claude Code chat.**

---

# FIX: 50 bugs from Deep Bug Hunt — Nika v0.41.0 → v0.41.1

## Context

Nika is a semantic YAML workflow engine for AI tasks. Rust workspace at
`/Users/thibaut/dev/supernovae/nika/tools/` with 11 crates. Current: **v0.41.0**.

In the previous session, 10 specialized agents (rust-pro, rust-async-expert, rust-security,
rust-perf, rust-architect, code-reviewer, Explore) scanned ~220K LOC and found **6 CRITICAL + 17
HIGH + 20 MEDIUM bugs + ~3800 LOC dead code**. ALL bugs verified as still present in v0.41.0.

Memory file: `project_deep_bughunt_2026_03_23.md`

## WHAT WAS ALREADY FIXED IN PREVIOUS SESSIONS (don't redo)

- SSRF protection (initial URL check), YoloMode to Plan, signal handlers, API key zeroization
- blocklist +8 patterns, kill_on_drop, 9x expect("model required") to proper errors
- RigProvider::infer timeout, SpawnAgentTool cancellation
- MCP validation enabled, disconnect() fix, CRLF position
- Agent loop retry + guardrail loops, chat_continue tools+turns+spawn
- Template context nested path, resolve_for_shell transforms
- Telemetry v2: 55 EventKind variants, ttft_ms, cache_read_tokens, boot events
- NIKA-053 false positive on blocklist normalization (fixed in v0.41.0)
- NIKA-150 to 160, NIKA-125 to 102 error code collision fixes
- Fetch retry overall deadline wrapping

---

## PHASE 1 — 6 CRITICAL BUGS

### CRITICAL-1: Token budget TOCTOU race condition

**Files**: `nika-engine/src/runtime/executor/verbs.rs:237,535`
**Also**: `nika-engine/src/runtime/policy.rs` (PolicyEnforcer + TokenBudget)

**The bug**: `check_token_spend()` at line 237 acquires a READ lock on PolicyEnforcer, checks
`token_budget.can_spend(estimated)`, releases lock. Then the LLM call runs (seconds). Then
`record_token_spend()` at line 535 acquires a WRITE lock and calls `token_budget.spend(actual)`.

With `for_each concurrency: 8`, all 8 tasks simultaneously read `used=0`, all pass budget check,
all fire LLM calls, budget overrun by `8 x cost_per_task`.

**Current code** (verbs.rs:234-247):
```rust
let estimated_tokens = estimate_tokens(prompt.len());
let decision = policy.check_token_spend(estimated_tokens); // READ lock
```
Then later (verbs.rs:532-535):
```rust
self.policy_enforcer
    .write()
    .record_token_spend(actual_tokens); // WRITE lock, much later
```

**Fix**: Atomic `reserve_and_check` pattern.

1. In `nika-engine/src/runtime/policy.rs`, add to `PolicyEnforcer`:
```rust
pub fn reserve_tokens(&mut self, estimated: u64) -> Result<(), String> {
    if !self.token_budget.can_spend(estimated) {
        return Err(format!(
            "Token budget exceeded: {} used + {} estimated > {} limit",
            self.token_budget.used, estimated,
            self.token_budget.limit.unwrap_or(u64::MAX),
        ));
    }
    self.token_budget.spend(estimated);
    Ok(())
}
pub fn adjust_reservation(&mut self, estimated: u64, actual: u64) {
    if actual < estimated {
        self.token_budget.used = self.token_budget.used.saturating_sub(estimated - actual);
    } else if actual > estimated {
        self.token_budget.spend(actual - estimated);
    }
}
```
2. In verbs.rs, replace check+record:
- Before infer: `self.policy_enforcer.write().reserve_tokens(estimated)?;`
- After response: `self.policy_enforcer.write().adjust_reservation(estimated, actual);`
- Apply at BOTH infer sites (line ~237 and ~2138 for vision)
- Remove old `check_token_spend()` and `record_token_spend()` calls

**Test**: Unit test: `max_token_spend: 100`, two concurrent tasks each 80 tokens. Second must block.

---

### CRITICAL-2: MCP mutex deadlock on server crash

**File**: `nika-mcp/src/rmcp_adapter.rs:318`

**The bug**: `call_tool()` does `self.service.lock().await`. If another call holds this lock
AND is stuck in 60s timeout (MCP_CALL_TIMEOUT), new call blocks 60s. With concurrency 10,
all tasks queue.

**Fix**: Timeout on lock acquisition:
```rust
let guard = match tokio::time::timeout(
    std::time::Duration::from_secs(5),
    self.service.lock(),
).await {
    Ok(g) => g,
    Err(_) => return Err(McpError::McpToolError {
        tool: tool_name.to_string(),
        reason: "MCP service lock timeout -- server may be unresponsive".to_string(),
        error_code: None,
    }),
};
```
Apply to ALL `self.service.lock()` calls (lines 176, 188, 254, 318, 463).

---

### CRITICAL-3: OutputPolicy::to_structured_spec() unwrap

**File**: `nika-core/src/ast/output.rs:112`
**Bug**: `let schema = self.schema.clone().unwrap();` -- panics if None.
**Fix**: `let schema = self.schema.clone()?;` (function returns Option).
**Test**: `test_output_policy_no_schema_returns_none`.

---

### CRITICAL-4: 4x course CLI unwrap on levels::by_number()

**File**: `nika-cli/src/course.rs:209,639,822,935`
**Bug**: `.unwrap()` crashes on invalid level. Line 639 worst: `level.number + 1` at max.
**Fix**: Replace each with `.ok_or_else(|| NikaError::CourseNotFound { ... })?;`
For line 639, guard: `if level.number >= 12 { return Ok(()); }` before the call.

---

### CRITICAL-5: chat_continue missing xAI provider

**File**: `nika-engine/src/runtime/rig_agent_loop/chat.rs:99-105`
**Bug**: Match has anthropic/openai/mistral/groq/deepseek/gemini but NO `"xai"` arm.
**Fix**: Add `"xai" => self.chat_continue_xai(prompt).await,`
Copy `chat_continue_openai`, replace client with xAI client.
Also add to auto-detect chain (lines 115-127).

---

### CRITICAL-6: SSRF bypass via HTTP redirect

**Files**: `nika-engine/src/runtime/executor/mod.rs:110`, `nika-engine/src/runtime/policy.rs:18`
**Bug**: `redirect::Policy::limited(5)` follows redirects without SSRF check on targets.
**Fix**: Custom redirect policy checking each hop against SSRF_BLOCKED_HOSTS.
Make `SSRF_BLOCKED_HOSTS` `pub(crate)`. Replace `Policy::limited()` with custom policy using
`attempt.url().host_str()` check.

---

## PHASE 2 — 17 HIGH BUGS

### HIGH-7: for_each media staging overwrite
**File**: `nika-engine/src/store/run_context.rs:296-299`
**Bug**: `insert()` overwrites. for_each iterations share parent task_id.
**Fix**: Use `entry().and_modify(|v| v.extend(...)).or_insert(media);`

### HIGH-8: drain_media iter+clear non-atomic
**File**: `nika-engine/src/runtime/rig_agent_loop/mod.rs:607-614`
**Fix**: Replace iter()+clear() with `retain(|_,_| false)` collecting values.

### HIGH-9: Artifact write errors don't fail the task
**File**: `nika-engine/src/runtime/runner.rs:1000-1013`
**Fix**: Set `task_result = TaskResult::failed(...)` when errors present.

### HIGH-10: for_each/decompose binding errors swallowed
**File**: `nika-engine/src/runtime/runner.rs:1424,1352`
**Fix**: Store `TaskResult::failed()` and `continue` instead of empty bindings.

### HIGH-11: Schema file read failure skips agent validation
**File**: `nika-engine/src/runtime/executor/verbs.rs:2218-2255`
**Fix**: Return error instead of warn + skip.

### HIGH-12: chat_continue_* reports 0 tokens/$0 cost
**File**: `nika-engine/src/runtime/rig_agent_loop/chat.rs` (6 methods)
**Fix**: Use estimate_tokens() on prompt+response. Calculate cost from estimates.

### HIGH-13: Cached tokens charged at full input rate
**File**: `nika-engine/src/provider/cost.rs:99-107`
**Fix**: Add cached_tokens param. Price cached at 10% of input rate.
```rust
pub fn calculate(&self, input_tokens: u64, output_tokens: u64, cached_tokens: u64) -> f64 {
    let effective_input = input_tokens.saturating_sub(cached_tokens);
    let cached_cost = (cached_tokens as f64 / 1_000_000.0) * self.input_per_million * 0.1;
    let input_cost = (effective_input as f64 / 1_000_000.0) * self.input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_million;
    let cost = cached_cost + input_cost + output_cost;
    if cost.is_finite() { cost } else { 0.0 }
}
```

### HIGH-14 to HIGH-23 (brief):
- 14: shell denylist: add `<(`, `$((`, `${` patterns to security.rs blocklist
- 15: symlink TOCTOU: canonicalize after mkdir in artifact writer
- 16: native read lock: clone model, release lock before stream loop
- 17: fetch retry .expect() to .ok_or_else(|| NikaError::FetchError)?
- 18: rayon pool .expect() to Result propagation
- 19: reqwest client .expect() to Result propagation
- 20: RuntimeContext task_name .expect() to return Option
- 21: get_ready_tasks clone: add is_completed_successfully() to RunContext
- 22: task.clone() deep copy: wrap AnalyzedTask in Arc
- 23: McpResponse.is_error dead variable: track actual error state

---

## PHASE 3 — MEDIUM + DEAD CODE + PERF

### Medium highlights:
- `transform.rs:491`: shell escape non-strings (one line fix)
- `runner.rs:825` + `validate.rs:56`: for_each alias collision check
- `template.rs:98`: USE_RE regex misses transforms

### Dead code (~3800 LOC):
```
DELETE: nika-engine/src/new/wizard.rs                         (1538 lines orphan)
DELETE: nika-engine/src/runtime/executor/tests_vision_e2e.rs  (762 lines orphan)
DELETE: nika-engine/src/media/tests_compression_deep.rs       (922 lines orphan)
REMOVE: BootstrapConfig dead fields (editor/session/provider) (~80 lines)
REMOVE: AnalyzedRetry.delay_for_attempt() dead method         (12 lines)
REMOVE: TaskResult.is_terminal() always-true                   (5 lines)
REMOVE: Runner::with_trace_config() dead builder               (5 lines)
REMOVE: BootContext.memory always-empty field                   (30 lines)
```

### Perf quick wins:
- RunContext::is_completed_successfully() avoids TaskResult clone in get_ready_tasks
- Arc<AnalyzedTask> avoids deep clone per for_each item
- String accumulator in consume_rig_stream() avoids Vec<String> + join
- Direct Arc<Value> in for_each aggregation avoids serialize/deserialize roundtrip

---

## HOW TO WORK

1. Read this plan FIRST
2. Read memory `project_deep_bughunt_2026_03_23.md`
3. Phase 1: Launch 6 parallel agents for 6 CRITICAL fixes (independent files)
4. Phase 2: Launch 4-6 agents for HIGH fixes
5. Phase 3: Medium + dead code + perf
6. Commit per logical group, push after each batch
7. `cd /Users/thibaut/dev/supernovae/nika/tools && cargo check --workspace && cargo test --workspace --lib`
8. Full autonomy, no questions, ultrathink
9. Commits: `type(scope): desc` with both co-authors
10. Tag v0.41.1 when all green

## COMMIT PLAN (9 commits)

| # | Message | Files |
|---|---------|-------|
| 1 | `fix(security): atomic token budget + SSRF redirect policy` | policy.rs, verbs.rs, mod.rs |
| 2 | `fix(mcp): lock timeout + atomic drain_media` | rmcp_adapter.rs, mod.rs |
| 3 | `fix(runtime): unwrap to errors (output, course, fetch, context)` | output.rs, course.rs, verbs.rs, context.rs, api.rs |
| 4 | `fix(provider): xAI chat_continue + cached pricing + estimates` | chat.rs, cost.rs |
| 5 | `fix(runtime): artifact errors fail + binding errors propagated` | runner.rs |
| 6 | `fix(runtime): schema enforced + media staging atomic` | verbs.rs, run_context.rs |
| 7 | `fix(binding): shell escape + alias collision + USE_RE` | transform.rs, validate.rs, template.rs |
| 8 | `perf: Arc task + is_completed_ok + String accum` | runner.rs, run_context.rs, rig.rs |
| 9 | `refactor: delete ~3800 LOC dead code` | 6+ files |

Review after 1-2, after 3-6, after 7-9. Tag v0.41.1.
