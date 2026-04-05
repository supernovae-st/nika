# Nika Crawler Hardening — Final Handoff

> For autonomous agent execution | ~6h TDD work
> Nika v0.63.0-dev | Commit: 1c9df31a9 | ~9600 tests passing

---

## Context

Over the last 48h, two parallel sessions implemented a massive crawler upgrade:
- **Session A** (this session): 8 engine commits (metadata URL resolution, url_normalize, slice, max_stdout, redirect chain, llms-full.txt, Cargo deps)
- **Session B** (parallel): 25+ commits implementing null handling, data tools, transforms, response:slim, robots.txt, rate limiting, RAG builtins

A comprehensive audit (20+ research/review agents) confirmed **85% is done**. This handoff covers the **remaining 15%** plus thorough verification.

---

## What's Already Done (DO NOT RE-IMPLEMENT)

These are IMPLEMENTED and TESTED. Read the code to understand them, but do NOT rewrite:

| Feature | Commit | File |
|---------|--------|------|
| BUG-034: for_each null → [] | `ea4ba4a93`+ | runner.rs:172 `value_to_array()` |
| BUG-035: template `| default()` | `56f59d96b` | template.rs:489-503 |
| BUG-036: for_each null via 034 | via BUG-034 | runner.rs |
| BUG-037: json_query [] not null | `f48255b27` | data_tools.rs:356-364 |
| BUG-038: binding lenient resolve | `431092709` | resolve.rs:958-973 |
| IMP-028: response:slim | `ae062a6d9` | extract.rs + fetch.rs |
| IMP-030: sitemap consistent keys | `aca12500f` | extract.rs:638-649 |
| nika:map, filter, group_by | `2800cb8ed` | data_tools.rs |
| nika:aggregate | `b9bcaacd2` | aggregate.rs |
| nika:chunk, token_count | `4d9956b86` | data_tools.rs:669-860 |
| starts_with/ends_with/contains | `96b2aeef5` | transform.rs |
| content_hash, unique_urls | `96b2aeef5` | transform.rs |
| robots.rs (RobotsCache) | `a9541f363` | runtime/robots.rs |
| rate_limit.rs (DomainRateLimiter) | `a9541f363` | runtime/rate_limit.rs |
| FetchCache (ETag store) | `67be16891` | runtime/fetch_cache.rs |
| cookie_store deps | `67be16891` | Cargo.toml |

---

## TASK 1: Wire robots.txt + rate limiting into fetch.rs (~2h)

### 1.1 — Read the infrastructure first

**Read these files completely before writing ANY code:**
- `nika-engine/src/runtime/robots.rs` — understand `RobotsCache::is_allowed()` API
- `nika-engine/src/runtime/rate_limit.rs` — understand `DomainRateLimiter::acquire()` API
- `nika-engine/src/runtime/executor/fetch.rs` — understand the full fetch flow, especially:
  - Where SSRF check happens (before HTTP request)
  - Where the HTTP request is sent (around line 500+)
  - Where custom clients are built (for SSRF pinning, redirect tracking)
- `nika-engine/src/runtime/executor/mod.rs` — understand `TaskExecutor` struct fields
- `nika-engine/src/runtime/boot.rs` — understand `PolicyConfig` and how it flows to executor

### 1.2 — Add RobotsCache and DomainRateLimiter to TaskExecutor

**File**: `nika-engine/src/runtime/executor/mod.rs`

The `TaskExecutor` struct needs new fields:
```rust
/// robots.txt cache — shared across all fetch tasks
robots_cache: Option<Arc<RobotsCache>>,
/// Per-domain rate limiter
domain_rate_limiter: Option<Arc<DomainRateLimiter>>,
```

Initialize in `TaskExecutor::new()` / `TaskExecutor::with_policy()`:
- Create `RobotsCache` if policy has `respect_robots_txt: true`
- Create `DomainRateLimiter` with `rate_limit_per_domain` from policy (default: 10 req/s)

**Read PolicyConfig** to see if these fields already exist. If not, add them:
```rust
// In PolicyConfig or a new FetchConfig:
pub respect_robots_txt: bool,      // default: true
pub rate_limit_per_domain: u32,    // default: 10
pub user_agent: String,            // default: "nika/{version}"
```

Check if `nika.toml` [fetch] section parsing already exists (commit `fd95d3601` threaded policy to executor).

### 1.3 — Integrate into fetch execution flow

**File**: `nika-engine/src/runtime/executor/fetch.rs`

Add two checks AFTER SSRF validation, BEFORE the HTTP request:

```rust
// 1. robots.txt check (after URL validation, before request)
if let Some(ref robots) = self.robots_cache {
    let parsed_url = url::Url::parse(url.as_ref()).ok();
    if let Some(ref u) = parsed_url {
        if !robots.is_allowed(u, &http_client).await {
            self.event_log.emit(EventKind::FetchBlocked {
                task_id: Arc::clone(task_id),
                url: url.to_string(),
                reason: "robots.txt disallows this URL".into(),
            });
            return Err(NikaError::PolicyViolation {
                reason: format!("robots.txt disallows: {}", url),
            });
        }
    }
}

// 2. Per-domain rate limiting (after robots check, before request)
if let Some(ref limiter) = self.domain_rate_limiter {
    if let Ok(parsed) = url::Url::parse(url.as_ref()) {
        if let Some(domain) = parsed.host_str() {
            limiter.acquire(domain).await;
        }
    }
}
```

**IMPORTANT**: Check if a `FetchBlocked` event variant exists in `nika-event/src/log.rs`. If not, add it or reuse an existing variant like `PolicyViolation`.

### 1.4 — Tests

**Wiremock tests** in `tests_wiremock.rs`:

```rust
#[tokio::test]
async fn wiremock_fetch_blocked_by_robots_txt() {
    let server = MockServer::start().await;
    // Serve robots.txt that blocks /admin/
    Mock::given(method("GET")).and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_string("User-agent: *\nDisallow: /admin/"))
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/admin/secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string("secret"))
        .mount(&server).await;

    // Create executor with robots enabled
    let executor = /* build with respect_robots_txt: true */;
    let result = executor.execute(/* fetch /admin/secret */).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("robots.txt"));
}

#[tokio::test]
async fn wiremock_fetch_allowed_by_robots_txt() {
    // Same setup but fetch /public/ path → should succeed
}

#[tokio::test]
async fn wiremock_fetch_no_robots_txt_allows_all() {
    // robots.txt returns 404 → all URLs allowed
}

#[tokio::test]
async fn wiremock_fetch_rate_limited_per_domain() {
    // Verify rate limiter is called (timing-based or mock)
}
```

**Commit**: `feat(fetch): integrate robots.txt + rate limiting into fetch execution`

---

## TASK 2: Wire cookies + ETag cache into fetch.rs (~3h)

### 2.1 — Read infrastructure first

**Read these files completely:**
- `nika-engine/src/runtime/fetch_cache.rs` — understand `FetchCache` API (store, get, conditional_headers)
- Check `cookie_store` and `reqwest_cookie_store` APIs — how `CookieStoreRwLock` works
- `nika-core/src/ast/raw/action.rs` — understand `RawFetchAction` struct (where to add `session` + `cache`)
- `nika-engine/src/ast/action.rs` — understand `FetchParams` (runtime struct)
- `nika-core/src/ast/raw/parser.rs` — understand `parse_fetch_action()` (YAML parsing)
- `nika-core/src/ast/analyzer/analyze.rs` — understand `analyze_fetch()` or equivalent
- `nika-engine/src/ast/lower.rs` — understand `lower_fetch()` (AST lowering)

### 2.2 — Add `session` and `cache` fields to AST pipeline

Same pattern as `max_stdout` was added to exec (commit `045c97c99`). Follow the EXACT same pipeline:

1. **RawFetchAction** (`nika-core/src/ast/raw/action.rs`): Add `pub session: Option<Spanned<bool>>` and `pub cache: Option<Spanned<bool>>`
2. **AnalyzedFetchAction** (`nika-core/src/ast/analyzed/task.rs`): Add `pub session: bool` and `pub cache: bool`
3. **FetchParams** (`nika-engine/src/ast/action.rs`): Add `pub session: Option<bool>` and `pub cache: Option<bool>`
4. **Parser** (`nika-core/src/ast/raw/parser.rs`): In `parse_fetch_action()`, add `session: get_bool_field(file, m, "session")?` and `cache: get_bool_field(file, m, "cache")?`
5. **Analyzer** (`nika-core/src/ast/analyzer/analyze.rs`): Pass through `session` and `cache`
6. **Lowering** (`nika-engine/src/ast/lower.rs`): In `lower_fetch()`, pass through fields
7. **All struct literals** everywhere that construct FetchParams — add `session: None, cache: None`

**WARNING**: This will touch MANY files (same as max_stdout). Use `cargo check --workspace` frequently. There will be ~20-30 struct literal sites that need `session: None, cache: None` added. Use a script if needed (like the max_stdout bulk fix pattern).

### 2.3 — Add cookie jar to TaskExecutor

**File**: `nika-engine/src/runtime/executor/mod.rs`

```rust
use cookie_store::CookieStore;
use reqwest_cookie_store::CookieStoreRwLock;

// In TaskExecutor struct:
cookie_jar: Option<Arc<CookieStoreRwLock>>,
```

Initialize in constructor:
```rust
let cookie_jar = Some(Arc::new(CookieStoreRwLock::new(CookieStore::default())));
```

### 2.4 — Wire cookie jar into EVERY Client::builder() in fetch.rs

**CRITICAL**: Search for ALL `reqwest::Client::builder()` calls in fetch.rs. There are at least 3:
1. The shared `http_client` field (built once in TaskExecutor constructor)
2. Custom client for SSRF pinning (built per-request)
3. Custom client for redirect tracking (response:full)

ALL must include:
```rust
.cookie_provider(Arc::clone(&self.cookie_jar.as_ref().unwrap()))
```

BUT only when `session: true` is set on the task. If `session` is false/None, do NOT attach the cookie jar (backward compatible).

### 2.5 — Wire FetchCache into fetch flow

When `cache: true`:
1. Before request: check `self.fetch_cache.conditional_headers(&url)` → add If-None-Match / If-Modified-Since
2. After response: if 304 → return cached body via `self.fetch_cache.get(&url)`
3. After response: if 200 → store ETag + Last-Modified + body via `self.fetch_cache.store(&url, ...)`

### 2.6 — Tests

```rust
#[tokio::test]
async fn wiremock_fetch_session_cookies_persist() {
    // Task 1: POST /login → Set-Cookie: session=abc
    // Task 2: GET /profile (same workflow) → Cookie: session=abc sent
}

#[tokio::test]
async fn wiremock_fetch_session_disabled_no_cookies() {
    // Same setup but session: false → no Cookie header sent
}

#[tokio::test]
async fn wiremock_fetch_cache_304_returns_cached() {
    // First request: 200 + ETag
    // Second request: cache: true → sends If-None-Match → 304 → returns cached body
}

#[tokio::test]
async fn wiremock_fetch_cache_200_updates_cache() {
    // Request with cache: true → stores response
}

#[tokio::test]
async fn wiremock_fetch_cache_disabled_no_conditional_headers() {
    // cache: false → no If-None-Match header sent
}
```

**Commits** (3 separate):
- `feat(ast): add session and cache fields to fetch verb`
- `feat(fetch): cookie jar integration with session: true`
- `feat(fetch): ETag/304 conditional request caching with cache: true`

---

## TASK 3: BUG-040 PartialSuccess artifact test (~30min)

### 3.1 — Write test

**File**: `nika-engine/src/runtime/runner.rs` (test section)

```rust
#[tokio::test]
async fn for_each_partial_success_writes_artifact() {
    // Setup: for_each with 3 items, fail_fast: false
    // Item 1: succeeds
    // Item 2: fails (exec returns error)
    // Item 3: succeeds
    // Expected: PartialSuccess status, artifact written with [result1, null, result3]
}
```

Search for existing for_each + artifact tests to understand the test pattern. Look for `fail_fast` in runner.rs tests.

**Commit**: `test(runtime): verify PartialSuccess for_each writes artifact (BUG-040)`

---

## TASK 4: Comprehensive E2E Verification (~1.5h)

### 4.1 — Full test suite

```bash
cargo test --workspace --lib --exclude nika-py
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

ALL must pass with ZERO failures, ZERO warnings.

### 4.2 — Verify no silent bugs in new features

Write targeted tests for edge cases that could be silent failures:

**Null handling edge cases:**
```rust
// Template: missing key with non-default transform should STILL error
#[test]
fn template_missing_key_with_trim_errors() {
    // {{with.obj.missing | trim}} → should be NIKA-052, NOT NIKA-153
    // This verifies BUG-035 fix doesn't silently pass null to trim
}

// Binding: missing root alias should STILL error (not null)
#[test]
fn binding_completely_unknown_alias_errors() {
    // $nonexistent_task → NIKA-052, not silent null
}

// for_each: non-null non-array should STILL error (not 0 iterations)
#[test]
fn for_each_string_value_errors() {
    // for_each: "not an array" → error, not silent empty
}
```

**Data tools edge cases:**
```rust
// nika:map with non-object elements
#[test]
fn map_on_scalar_array_with_selector() {
    // [1, 2, 3] with selector "name" → [null, null, null]
}

// nika:filter with missing field
#[test]
fn filter_missing_field_excludes_element() {
    // [{name: "a"}, {age: 5}] filter field:"name" op:"eq" value:"a"
    // → [{name: "a"}] (second element has no "name", excluded)
}

// nika:group_by with nested key
#[test]
fn group_by_nested_key() {
    // key: "address.city" → groups correctly? Or only top-level?
}

// nika:aggregate with non-numeric field
#[test]
fn aggregate_sum_non_numeric_returns_zero() {
    // [{val: "text"}] sum on "val" → 0 or error?
}
```

**Transform edge cases:**
```rust
// content_hash determinism across runs
#[test]
fn content_hash_same_input_always_same_output() {
    let h1 = TransformOp::ContentHash.apply(&json!("hello")).unwrap();
    let h2 = TransformOp::ContentHash.apply(&json!("hello")).unwrap();
    assert_eq!(h1, h2);
}

// unique_urls with non-URL strings
#[test]
fn unique_urls_non_url_strings_kept() {
    // ["not-a-url", "also-not-url"] → kept as-is, not crashed
}

// starts_with on non-string
#[test]
fn starts_with_on_number_errors() {
    assert!(TransformOp::StartsWith("x".into()).apply(&json!(42)).is_err());
}
```

**response:slim edge cases:**
```rust
// slim + extract:metadata combo
#[tokio::test]
async fn wiremock_response_slim_with_metadata_no_body() {
    // Verify body is NOT in output but extracted metadata IS
}

// slim without extract
#[tokio::test]
async fn wiremock_response_slim_without_extract() {
    // Verify minimal JSON: status, url, elapsed_ms, redirects, redirect_count
}
```

**json_query edge cases:**
```rust
// json_query returns [] not null (verify BUG-037 fix)
#[tokio::test]
async fn json_query_tool_empty_result_is_array_not_null() {
    let tool = JsonQueryTool;
    let result = tool.call(r#"{"data": {"a": 1}, "query": "$.missing"}"#.into()).await.unwrap();
    let parsed: Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array(), "Expected [], got: {}", parsed);
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}
```

### 4.3 — Verify architectural consistency

Run these checks:

```bash
# No dead imports
cargo clippy --workspace --all-targets -- -D warnings -W unused-imports

# No dead code in new files
grep -rn "dead_code\|#\[allow(dead" nika-engine/src/runtime/robots.rs nika-engine/src/runtime/rate_limit.rs nika-engine/src/runtime/fetch_cache.rs

# No TODO/FIXME left in new code
grep -rn "TODO\|FIXME\|HACK\|XXX" nika-engine/src/runtime/robots.rs nika-engine/src/runtime/rate_limit.rs nika-engine/src/runtime/fetch_cache.rs nika-engine/src/runtime/builtin/aggregate.rs

# Verify all new tools are in the builtin tool list (nika:* tools should show up in nika tools list)
grep -c "tools.insert" nika-engine/src/runtime/builtin/router.rs
```

### 4.4 — Verify schema files are updated

Check if `nika-engine/schemas/nika-workflow.schema.json` and `nika/schemas/nika-workflow.schema.json` include:
- `"slim"` in response mode enum
- `"session"` and `"cache"` in fetch properties (after TASK 2)
- `"sitemap"` in extract mode enum
- New transforms in documentation

---

## Commit format (every commit)

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Commands

```bash
cd ~/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py    # ALWAYS --lib, ALWAYS --exclude nika-py
cargo test -p nika-engine --lib -- test_name      # Targeted
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## TDD (mandatory)

```
1. Read source code FIRST (never code blind)
2. Write test FIRST (RED) — verify it FAILS
3. Implement minimal fix (GREEN)
4. cargo fmt && cargo clippy — 0 warnings
5. Commit
```

---

## Summary: What to deliver

| Task | Effort | Deliverables |
|------|--------|-------------|
| **TASK 1**: Wire robots + rate limit into fetch.rs | 2h | 4+ wiremock tests, integration code |
| **TASK 2**: AST session/cache fields + cookie/ETag wiring | 3h | AST pipeline, 5+ wiremock tests, fetch.rs integration |
| **TASK 3**: BUG-040 PartialSuccess artifact test | 30min | 1 test (verify or fix) |
| **TASK 4**: E2E verification + edge case tests | 1.5h | 15+ edge case tests, full suite green |

**Expected**: ~7h total, 25+ new tests, 5-8 commits, full green suite.
