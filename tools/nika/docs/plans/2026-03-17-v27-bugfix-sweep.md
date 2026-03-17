# v0.27.0 Bugfix Sweep — Ultra-Detailed Implementation Plan

> **Method:** TDD (Red-Green-Refactor) | Subagent-driven | Nuclear v0 philosophy
> **Execution:** Parallel batches by severity | Context7 for Rust patterns

---

## Triage — Research Results

### REAL BUGS (10 items to fix)

| # | Severity | Bug | File | Fix |
|---|----------|-----|------|-----|
| 1 | CRITICAL | Agent response drops non-string JSON | `verbs.rs:1167-1172` | Replace `.as_str()` with `match` on Value type |
| 2 | CRITICAL | Security blocklist whitespace bypass | `security.rs:134-136` | Add whitespace normalization to `normalize_for_blocklist()` |
| 3 | HIGH | Token counter u64→u32 downcast overflow | `providers.rs:458-459` | Change metadata fields to u64 or use saturating_cast |
| 7 | HIGH | Tools consumed after first agent run | `providers.rs:110,187,372` | Replace `mem::take` with `.clone()` |
| 5 | MEDIUM | JSON context false positive (known limitation) | `template.rs:1196-1225` | Already mitigated by starts_with check — document limitation |
| 8 | MEDIUM | MCP cache key non-canonical JSON | `client.rs:232-240` | Sort JSON keys before hashing |
| 9 | MEDIUM | Home dir panic (.expect) | `paths.rs:103` | Return Result instead of panicking |
| 14 | LOW | MaxTurnsReached dead variant | `types.rs:31` | Nuclear delete + related dead variants |
| 16 | LOW | Cache eviction O(n log n) | `client.rs:295-309` | Replace with BinaryHeap or LRU tracking |
| — | LOW | Outdated test comments (#4, #6, #10) | `tests.rs` | Fix test assertions that document already-fixed bugs |

### NOT BUGS (6 items — no action needed)

| # | Status | Reason |
|---|--------|--------|
| 4 | ALREADY FIXED | `resolve_for_shell()` has Pass 3 for inputs |
| 6 | ALREADY FIXED | Code uses `effective_max_tokens().unwrap_or(8192)` |
| 10 | ALREADY FIXED | `chat_continue_gemini()` exists at chat.rs:495 |
| 12 | NOT A BUG | Intentional legacy `goal:` backward compat |
| 13 | NOT A BUG | Intentional legacy `include:` backward compat |
| 15 | NOT A BUG | `dag/flow.rs` still actively used and re-exported |

---

## Batch 1: CRITICAL Fixes

### Fix #1: Agent Response Extraction Data Loss

**Location:** `src/runtime/executor/verbs.rs:1167-1172`

**Root cause:** `.as_str()` returns `None` for non-string JSON values.

**Current code:**
```rust
let response = result
    .final_output
    .get("response")
    .and_then(|v| v.as_str())
    .unwrap_or("");
Ok(response.to_string())
```

**TDD Steps:**
1. RED: Write test `test_agent_response_preserves_json_object` that sends JSON object through agent and asserts it's preserved
2. RED: Write test `test_agent_response_preserves_json_array` for arrays
3. RED: Write test `test_agent_response_preserves_string` (regression — must still work)
4. GREEN: Replace with:
```rust
let response = match result.final_output.get("response") {
    Some(serde_json::Value::String(s)) => s.clone(),
    Some(v) => v.to_string(),
    None => String::new(),
};
Ok(response)
```
5. REFACTOR: Add tracing for non-string agent responses
6. VERIFY: `cargo test --lib executor::tests`

**Telemetry:**
```rust
tracing::debug!(
    task_id = %task_id,
    response_type = ?result.final_output.get("response").map(|v| v.type_name()),
    "Agent response extracted"
);
```

### Fix #2: Security Blocklist Whitespace Bypass

**Location:** `src/runtime/security.rs:134-136`

**Root cause:** `normalize_for_blocklist()` strips zero-width chars and normalizes Unicode but does NOT normalize whitespace.

**Current code:**
```rust
fn normalize_for_blocklist(s: &str) -> String {
    s.nfkc().filter(|c| !ZERO_WIDTH_CHARS.contains(c)).collect()
}
```

**TDD Steps:**
1. RED: Write test `test_blocklist_catches_double_spaces` — `"rm  -rf  /"` MUST be blocked
2. RED: Write test `test_blocklist_catches_tabs` — `"rm\t-rf\t/"` MUST be blocked
3. RED: Write test `test_blocklist_catches_mixed_whitespace` — `"rm \t -rf / "` MUST be blocked
4. GREEN: Add whitespace normalization:
```rust
fn normalize_for_blocklist(s: &str) -> String {
    s.nfkc()
        .filter(|c| !ZERO_WIDTH_CHARS.contains(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
```
5. REFACTOR: Update existing bypass tests to assert blocked (change from panic!("GAP CONFIRMED") to assert!(result.is_err()))
6. VERIFY: `cargo test --lib security`

**Telemetry:**
```rust
tracing::warn!(
    command = %cmd,
    normalized = %normalized,
    pattern = %pattern,
    "NIKA-053: Blocked dangerous command"
);
```

---

## Batch 2: HIGH Fixes

### Fix #3: Token Counter u64→u32 Downcast

**Location:** `src/runtime/rig_agent_loop/providers.rs:458-459`

**Root cause:** `total_input_tokens as u32` truncates u64 values > u32::MAX.

**Current code:**
```rust
input_tokens: total_input_tokens as u32,
output_tokens: total_output_tokens as u32,
```

**TDD Steps:**
1. RED: Verify metadata struct field types
2. GREEN: Change metadata emission to use `u64::min(total, u32::MAX as u64) as u32` or change struct fields to u64
3. VERIFY: `cargo test --lib rig_agent_loop`

### Fix #7: Tools Consumed After First Agent Run

**Location:** `src/runtime/rig_agent_loop/providers.rs:110,187,372`

**Root cause:** `std::mem::take(&mut self.tools)` empties the tools vec.

**Current code:**
```rust
let tools = std::mem::take(&mut self.tools);
```

**TDD Steps:**
1. RED: Write test `test_tools_survive_multiple_runs` that calls run_claude twice
2. GREEN: Replace `mem::take` with `.clone()`:
```rust
let tools = self.tools.clone();
```
3. VERIFY: `cargo test --lib rig_agent_loop`

---

## Batch 3: MEDIUM Fixes

### Fix #8: MCP Cache Key Canonicalization

**Location:** `src/mcp/client.rs:232-240`

**TDD Steps:**
1. RED: Write test proving `{"a":1,"b":2}` and `{"b":2,"a":1}` get SAME cache key
2. GREEN: Sort JSON keys recursively before hashing
3. VERIFY: `cargo test --lib mcp`

### Fix #9: Home Dir Panic → Result

**Location:** `src/core/paths.rs:103`

**TDD Steps:**
1. RED: Verify current behavior panics without HOME
2. GREEN: Return `Result<PathBuf, NikaError>` and propagate
3. Update all callers to handle Result
4. VERIFY: `cargo test --lib core`

---

## Batch 4: LOW / Cleanup

### Fix #14: Nuclear Delete Dead RigAgentStatus Variants

**Location:** `src/runtime/rig_agent_loop/types.rs`

**Dead variants:** `MaxTurnsReached`, `TokenBudgetExceeded`, `CostLimitReached`, `DurationLimitReached`, `PartialCompletion`

**TDD Steps:**
1. Search for any code producing these variants
2. Delete from enum definition
3. Delete any match arms
4. VERIFY: `cargo check`

### Clean Outdated Tests (#4, #6, #10)

Fix test comments and assertions that document bugs which no longer exist:
- `wave2_chat_continue_missing_gemini_dispatch` — Gemini IS dispatched now
- `audit_resolve_for_shell_missing_inputs_support` — inputs ARE resolved now
- `max_tokens hardcoded` comment — effective_max_tokens() IS used now

---

## Verification Protocol (Ralph Wiggum)

```
1. cargo test --lib                    # All 5,143+ unit tests
2. cargo test --tests                  # Integration tests
3. cargo clippy -- -D warnings         # Zero warnings
4. cargo fmt --check                   # Format clean
5. Manual: run real workflow with agent verb
6. Manual: run workflow with exec "rm  -rf  /" (must block)
```

---

## Commit Strategy

One commit per fix, push after each batch:

```
fix(runtime): preserve non-string JSON in agent response extraction
fix(security): normalize whitespace in blocklist to prevent bypass
fix(runtime): prevent token counter overflow on u64→u32 downcast
fix(runtime): clone tools instead of mem::take for agent reuse
fix(mcp): canonicalize JSON keys in cache key computation
fix(core): return Result from home_dir() instead of panicking
refactor(runtime): nuclear delete dead RigAgentStatus variants
test: fix outdated test assertions for already-resolved bugs
```
