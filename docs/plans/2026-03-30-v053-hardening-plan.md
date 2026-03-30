# v0.53 Hardening Plan — Paranoia Audit Results + Fix Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 8 real bugs found during paranoia audit, clean up 35+ stale tests, and tag v0.53.0.

**Architecture:** TDD — write failing test first, fix, verify. Bugs found via: 8 real workflow runs with 4 providers (OpenAI, xAI, Gemini, Anthropic), 3 deep code audit agents (security, async, quality), full codebase grep.

**Tech Stack:** Rust, cargo test --workspace --lib, serde_json, tokio, reqwest

---

## Context: What Was Done

### Audit Scope (2026-03-30)

- **49 commits** analyzed since v0.52.0
- **8997 tests** all green
- **8 real workflows** created and run with real API calls
- **3 background audit agents** (security, async, code quality)
- **38 documented "BUG PROVEN" tests** reviewed individually
- **4 providers tested**: OpenAI ✓, xAI ✓, Gemini (rate-limited), Anthropic (no credits)
- **6 fetch extract modes tested**: markdown ✓, article ✓, metadata ✓, links ✓, jsonpath ✓, full ✓
- **for_each concurrency**: exec parallel ✓, structured sequential ✓, infer parallel ✓
- **Structured output**: OpenAI (L0→L2→L3 retry works), xAI (L0+L2 works)

### What Works Perfectly

- Fetch verb: all 6 extract modes, redirect handling, SSRF blocking
- for_each: concurrency, fail_fast, exec/infer/structured
- Structured output: 5-layer defense works on OpenAI and xAI
- DAG execution: dependency chain, parallel scheduling
- Exec verb: env vars, shell piping, data chaining
- Builtin tools: nika:log, nika:glob (within working dir)
- Security: NIKA-053 blocklist, cwd escape detection, path validation
- ModelResolver: correctly routes models across 4 TUI views

### Previously Audited: 35 of 38 "BUG PROVEN" Tests Already Fixed

These tests document bugs that **were already fixed in production code**. They assert the OLD buggy behavior and should be cleaned up. See Phase 5.

---

## NEW Bugs Found During Paranoia Audit

### B1: CRITICAL — fetch returns SUCCESS on HTTP 5xx (last retry attempt)

**File:** `nika-engine/src/runtime/executor/fetch.rs:419`
**Root cause:** When `attempt >= effective_max_attempts`, the `is_retryable_status` check falls through to line 464 ("Success or non-retryable error status") and the 500 response body is returned as success.
**Reproduction:** Run workflow with `fetch: { url: "https://httpbin.org/status/500" }` and `retry: { max_attempts: 2 }`. After 2 attempts with 500, task shows ✓.
**Impact:** Downstream tasks receive empty/error body as valid data. NIKA-026 dependency blocking doesn't trigger.
**Fix:** After retry exhaustion, check if `last_error.is_some()` and return it instead of falling through.

### B2: HIGH — nika:write param name inconsistency

**File:** `nika-engine/src/tools/write.rs:28`
**Root cause:** `WriteParams` struct uses `file_path` but natural expectation (and other tools) use `path`.
**Reproduction:** `invoke: { tool: "nika:write", params: { path: "/tmp/test.txt", content: "hello" } }` → NIKA-201 missing field `file_path`.
**Impact:** Confusing API for workflow authors. Other tools use `path`.
**Fix:** Add `#[serde(alias = "path")]` on the `file_path` field, or rename to `path`.

### B3: MEDIUM — nika:log output contains raw ANSI escape codes

**File:** `nika-engine/src/runtime/builtin/` (log tool)
**Root cause:** Log tool output serializes with colored formatting even in non-TTY contexts.
**Reproduction:** Run workflow with `invoke: nika:log` → output contains `[0m`, `[34m` etc.
**Impact:** Downstream tasks that parse log output get garbage. Machine-readable JSON contaminated.
**Fix:** Strip ANSI codes from tool output, or disable colors in non-TTY mode.

### B4: CRITICAL — Transform parser splits on `|` inside parenthesized arguments

**File:** `nika-core/src/binding/transform.rs:156`
**Root cause:** `.split('|')` doesn't respect parentheses/quotes. `join(" | ")` becomes `join(" ` and ` ")`.
**Reproduction:** `{{with.csv | trim | split(",") | join(" | ")}}` → NIKA-074 parse error.
**Impact:** Cannot use `|` character in any transform argument. Blocks legitimate join separators.
**Fix:** Replace naive `split('|')` with a parser that tracks paren/quote depth.

### B5: MEDIUM — nika:glob/grep/write block paths outside working directory

**Files:** `nika-engine/src/tools/*.rs`
**Root cause:** Builtin file tools validate paths against the working directory. When running workflows from `/tmp/`, tools can't access files in `/tmp/` because the binary's cwd is `tools/`.
**Impact:** Workflows that operate on files outside the project directory fail silently or with cryptic errors.
**Status:** By design (security), but NIKA-204 error message should suggest `--workdir` flag or document the limitation.

### B6: LOW — NIKA-026 count reports wrong number of blocked tasks

**Root cause:** The error message "N task(s) blocked" counts ALL failed tasks (including non-dependency failures) not just dependency-blocked ones.
**Reproduction:** In workflow 05, "3 tasks blocked" but only 1 depends on the failed task.
**Impact:** Misleading error message. Low severity.

### B7: MEDIUM — OpenAI L0 tool_injection always fails for structured output

**Root cause:** OpenAI provider doesn't support tool injection for structured output (or the schema format doesn't match).
**Impact:** Every OpenAI structured output request falls back to L2 extract_validate → L3 retry, adding latency. Works but slower.
**Status:** May be by design (OpenAI uses `response_format` not tool injection). Verify if L0 `response_format` path works.

### B8: LOW — fetch_retry task-level retry interacts with verb-level retry

**Root cause:** Fetch verb has its own internal retry loop for 5xx (lines 416-461) AND the runner has task-level retry. When both are configured, the total attempts = fetch_internal_retries × task_retries.
**Impact:** Confusing behavior — user sets `retry: { max_attempts: 2 }` but actual attempts may differ.
**Status:** Document the interaction clearly.

---

## Phase 1: Fix CRITICAL Bugs

### Task 1: Fix fetch 5xx treated as success on last retry (B1)

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/fetch.rs:462-465`
- Test: Same file or `tests_e2e_workflow.rs`

**Step 1: Write failing test**

```rust
#[tokio::test]
async fn fetch_returns_error_on_exhausted_5xx_retries() {
    // After all retry attempts return 500, fetch should return Err, not Ok("")
    // Use httpbin.org/status/500 or a mock server
}
```

**Step 2: Fix production code**

After the retry loop (line 462), add:

```rust
// If we exhausted retries and the last response was a server error,
// return the accumulated error instead of falling through to "success"
if let Some(err) = last_error {
    if response.status().is_server_error() || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(err);
    }
}
```

**Step 3: Verify**

```bash
cargo test -p nika-engine --lib -- fetch
```

**Step 4: Commit**

```
fix(fetch): return error on exhausted 5xx retries instead of empty success
```

### Task 2: Fix transform parser pipe-in-parentheses (B4)

**Files:**
- Modify: `tools/nika-core/src/binding/transform.rs:155-159`
- Test: Same file, test module

**Step 1: Write failing test**

```rust
#[test]
fn join_with_pipe_separator() {
    let input = Value::Array(vec![json!("a"), json!("b"), json!("c")]);
    let result = apply_transforms(&input, "join(\" | \")").unwrap();
    assert_eq!(result, json!("a | b | c"));
}
```

**Step 2: Fix the split logic**

Replace `trimmed.split('|')` with a function that respects parentheses depth:

```rust
fn split_pipe_transforms(input: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut in_quotes = false;
    let mut start = 0;
    for (i, c) in input.char_indices() {
        match c {
            '"' if depth > 0 => in_quotes = !in_quotes,
            '(' if !in_quotes => depth += 1,
            ')' if !in_quotes => depth -= 1,
            '|' if depth == 0 && !in_quotes => {
                result.push(&input[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    result.push(&input[start..]);
    result
}
```

**Step 3: Verify**

```bash
cargo test -p nika-core --lib -- transform
```

**Step 4: Commit**

```
fix(transforms): respect parentheses when splitting pipe chain — allows | in join/split args
```

---

## Phase 2: Fix HIGH Bugs

### Task 3: Add `path` alias to nika:write params (B2)

**Files:**
- Modify: `tools/nika-engine/src/tools/write.rs:28`

**Step 1: Add serde alias**

```rust
pub struct WriteParams {
    #[serde(alias = "path")]
    pub file_path: String,
    pub content: String,
}
```

**Step 2: Test**

```rust
#[test]
fn write_params_accepts_path_alias() {
    let json = r#"{"path": "/tmp/test.txt", "content": "hello"}"#;
    let params: WriteParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.file_path, "/tmp/test.txt");
}
```

**Step 3: Commit**

```
fix(tools): accept 'path' alias for nika:write file_path param
```

---

## Phase 3: Fix MEDIUM Bugs

### Task 4: Strip ANSI from nika:log tool output (B3)

**Files:**
- Modify: `nika-engine/src/runtime/builtin/` (log tool implementation)

**Step 1: Find and fix**

The log tool should serialize JSON without ANSI color codes. Use `strip_ansi_escapes` or disable coloring in the tool output path.

**Step 2: Commit**

```
fix(tools): strip ANSI escape codes from nika:log output
```

### Task 5: Improve NIKA-204 error message (B5)

**Files:**
- Modify: Tools validation code

**Step 1: Improve error message**

Add suggestion: "Paths must be within the workflow's working directory. Use `--workdir` to change the base directory."

**Step 2: Commit**

```
fix(tools): improve NIKA-204 path error with actionable suggestion
```

---

## Phase 4: Documentation Fixes

### Task 6: Document fetch retry interaction (B8)

**Files:**
- Modify: nika rules docs

**Step 1: Add note to nika-bugs-and-patterns.md**

Fetch verb has internal retry for 5xx (exponential backoff). Task-level `retry:` wraps the entire verb. When both configured, total attempts = internal_retries × task_retries.

**Step 2: Commit**

```
docs: document fetch internal retry vs task-level retry interaction
```

### Task 7: Document OpenAI L0 tool_injection limitation (B7)

Note that OpenAI structured output uses L0 `response_format` path, not `tool_injection`. L0 tool_injection is Anthropic-only. Verify and document.

---

## Phase 5: Stale Test Cleanup (~700 LOC)

### Task 8: Remove stale "BUG PROVEN" tests in rig_agent_loop/tests.rs

7 tests that assert buggy behavior already fixed:
- MaxTurnsReached, tools consumed, token overflow, hardcoded 8192, whitespace keys, Gemini missing, Mistral hardcode

**Action:** Delete all 7 tests. Production code has correct behavior.

### Task 9: Remove stale "BUG PROVEN" tests in verbs.rs

6 tests proving `as_str()` bug that's already fixed in agent.rs.

**Action:** Delete. Keep the positive regression tests (lines 505+).

### Task 10: Clean up remaining stale tests

- `executor/tests.rs`: timeout cleanup (fixed), tab bypass (verify), cwd templates (verify)
- `dag/flow.rs`: self-ref and phantom deps (both handled correctly)
- `template.rs`: quote heuristic (fixed)
- `media/tests_integration.rs`: color_thief min (fixed), OOM (verify)
- `chat_overlay.rs`: UTF-8 comment misleading (code is correct)
- `transform.rs`: BUG 3/7/8 comments (all fixed)

### Task 11: Run full test suite and commit

```bash
cargo test --workspace --lib
```

Expected: 8997+ tests, 0 failures.

```
refactor(test): remove ~30 stale 'BUG PROVEN' tests — all bugs already fixed in production
```

---

## Phase 6: Version Bump & Tag

### Task 12: Bump to v0.53.0

- Update `VERSION`, `Cargo.toml` workspace version
- Move CHANGELOG `[Unreleased]` to `[0.53.0]`
- Final `cargo test --workspace --lib`
- `git tag v0.53.0`

---

## Summary

| Phase | Tasks | Impact | Risk | Time |
|-------|-------|--------|------|------|
| 1. CRITICAL fixes | 2 | fetch 5xx + transform parser | Low | 1-2h |
| 2. HIGH fixes | 1 | nika:write alias | Zero | 5m |
| 3. MEDIUM fixes | 2 | ANSI + error messages | Zero | 30m |
| 4. Docs | 2 | Retry docs + L0 docs | Zero | 15m |
| 5. Stale cleanup | 4 | -700 LOC tests | Zero | 1h |
| 6. Release | 1 | v0.53.0 tag | Zero | 10m |
| **Total** | **12** | **+100 LOC fixes, -700 LOC stale** | **Low** | **3-4h** |

---

## Agent Audit Results (3 agents, ~800K tokens analyzed)

### Security Audit — 3 HIGH, 5 MEDIUM, 5 LOW

| ID | Severity | Finding |
|----|----------|---------|
| SEC-EXEC-03 | **HIGH** | No auto-shell-escaping for template values in `shell: true`. User writes `command: "echo {{with.data}}"` → raw injection if data contains metacharacters. Fix: auto-apply `\|shell` transform when `shell: true`. |
| SEC-AGENT-01 | **HIGH** | Agent tool calls bypass exec/fetch security checks entirely. Agent calls `nika:exec` via MCP without blocklist/SSRF validation. Fix: security interceptor for agent tool calls. |
| SEC-SECRET-01 | **HIGH** | API keys leak into `.nika/traces/` when not matching redact regex patterns. Custom tokens (numeric-only, non-standard) appear in plaintext. Fix: track `$env`-sourced bindings, mask in all events. |
| SEC-EXEC-01 | MEDIUM | Shell blocklist bypassed by LLM-injected data in resolved output (by design, but gap). |
| SEC-FETCH-01 | MEDIUM | `llm_txt` sub-requests skip async DNS rebinding check. |
| SEC-ARTIFACT-01 | MEDIUM | Symlinks inside artifact dir not detected (documented limitation). |
| SEC-SECRET-02 | MEDIUM | `tracing::warn` logs resolved commands with secrets (not redacted). |
| SEC-SECRET-03 | MEDIUM | Error messages contain unredacted resolved values. |

**What's excellent:** SSRF protection (best-in-class: DNS pinning, fail-closed, IPv6), template injection defense (trusted-path whitelisting), Unicode bypass protection (NFKC+zero-width), CancellationToken hierarchy, `kill_on_drop` process isolation.

### Async/Concurrency Audit — 0 HIGH, 2 MEDIUM, 5 LOW

| ID | Severity | Finding |
|----|----------|---------|
| ASYNC-07 | MEDIUM | Broadcast channel (1024 cap) overflow loses TUI events during heavy for_each. Mitigated by dirty flag. |
| ASYNC-08 | MEDIUM | EventLog eviction (>10K events) causes CLI renderer to miss intermediate events. |

**Architecture verdict: FUNDAMENTALLY SOUND.** Batched wave DAG model prevents data races. DashMap for lock-free task results. Semaphore concurrency limiter correct. Per-parent fail_fast tokens correct. No nested locks. No locks held across await. CancellationToken propagation thorough.

### Code Quality Audit — 5 MEDIUM, 7 LOW

| ID | Severity | Finding |
|----|----------|---------|
| CQ-11 | **MEDIUM** | **Traced/untraced binding resolution DIVERGES on null+transform** — `resolve_with_entry_traced` and `resolve_with_entry` handle null values through transform chains differently. Production vs debug runs may produce different results! |
| CQ-05 | **MEDIUM** | `saturating_add` not applied to 3 remaining sites: `runner.rs:2749` (summary fold), `introspect_task.rs:141`, `thinking.rs:521`. |
| CQ-13 | **MEDIUM** | `thinking_budget` is u64 but cast to u32 with `as u32` — silently truncates budgets >4B. |
| CQ-07 | **MEDIUM** | `f.round() as i64` returns 0 for NaN. `round(2)` on Infinity returns null silently. |
| CQ-01 | **MEDIUM** | Global `MOCK_CALL_COUNTER` never resets across test runs. |

---

## Updated Priority Matrix

### P0: CRITICAL (fix before v0.53 tag)

| # | Bug | File | Effort |
|---|-----|------|--------|
| B1 | fetch 500 = success on last retry | `fetch.rs:419` | 30m |
| B4 | Transform parser `\|` in parentheses | `transform.rs:156` | 1h |
| CQ-11 | Traced/untraced binding divergence | `resolve.rs:737` | 30m |

### P1: HIGH (fix in v0.53)

| # | Bug | File | Effort |
|---|-----|------|--------|
| SEC-EXEC-03 | No auto-shell-escaping in `shell: true` | `exec.rs:32` | 2h |
| SEC-AGENT-01 | Agent tool calls bypass security | `rig/tool.rs:113` | 2h |
| SEC-SECRET-01 | Trace files leak secrets | `runner.rs:522` | 1h |
| CQ-05 | 3 remaining non-saturating token adds | `runner.rs:2749` + 2 | 15m |
| CQ-13 | `thinking_budget as u32` truncation | `infer.rs:657` | 15m |
| B2 | `nika:write` param name | `write.rs:28` | 5m |

### P2: MEDIUM (v0.53 nice-to-have)

| # | Bug | File | Effort |
|---|-----|------|--------|
| SEC-FETCH-01 | llm_txt DNS rebinding | `fetch.rs:605` | 30m |
| SEC-SECRET-02 | tracing::warn leaks secrets | `exec.rs:58` | 15m |
| SEC-SECRET-03 | Error messages leak secrets | `exec.rs:107` | 15m |
| CQ-07 | NaN/Inf silent conversion | `transform.rs:422` | 30m |
| B3 | ANSI codes in nika:log output | builtin tools | 15m |
| B5 | NIKA-204 error message improvement | tools validation | 10m |

### P3: LOW (defer to v0.54+)

| # | Bug | File |
|---|-----|------|
| SEC-EXEC-01 | Shell blocklist LLM data bypass | security.rs |
| SEC-EXEC-02 | Denylist gaps (awk, tee, busybox) | security.rs |
| SEC-ARTIFACT-01 | Symlinks inside artifact dir | io/security.rs |
| ASYNC-07 | Broadcast overflow loses TUI events | log.rs |
| ASYNC-08 | EventLog eviction misses renderer events | log.rs |
| CQ-01 | MOCK_CALL_COUNTER never resets | infer.rs |
| B6 | NIKA-026 wrong blocked count | runner |
| B8 | Retry interaction documentation | docs |
| MCP cache | O(n log n) eviction | client.rs |

### Phase 5: Stale Test Cleanup (~700 LOC, zero risk)

35+ "BUG PROVEN" tests that assert buggy behavior already fixed. Delete or convert to regression tests.

---

## Estimated Total Effort

| Priority | Tasks | Time | Risk |
|----------|-------|------|------|
| P0 Critical | 3 | 2h | Low |
| P1 High | 6 | ~6h | Medium |
| P2 Medium | 6 | ~2h | Low |
| Stale cleanup | 4 | 1h | Zero |
| Release | 1 | 10m | Zero |
| **Total** | **20** | **~11h** | **Low-Medium** |

## Deferred to v0.54+

| Bug | Reason |
|-----|--------|
| Denylist → allowlist migration | Breaking change, needs design |
| Broadcast channel sizing | Acceptable with dirty flag |
| EventLog eviction strategy | Only affects >10K event workflows |
| MOCK_CALL_COUNTER | Test-only, not production |
| MCP cache LRU | Acceptable at current scale |
