# Crawler Hardening — Autonomous Execution Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Wire robots.txt, rate limiting, cookie jar, and ETag caching into the fetch executor — completing the final 15% of the crawler upgrade.

**Architecture:** Three infrastructure modules (robots.rs, rate_limit.rs, fetch_cache.rs) already exist with full test coverage. This plan wires them into the fetch execution pipeline via new TaskExecutor fields and two new AST fields (`session`, `cache`) on the fetch verb. All work is TDD: write test RED, implement GREEN, commit.

**Tech Stack:** Rust 1.86, tokio, reqwest 0.13 (cookies feature), wiremock, governor 0.10, texting_robots 0.2, dashmap

**Baseline:** 9695 tests passing, commit `1c9df31a9`, Nika v0.63.0-dev

---

## Pre-Flight Checklist

```bash
cd ~/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py  # Must be 9695+ pass, 0 fail
cargo fmt --all --check                          # Must be clean
cargo clippy --workspace --all-targets -- -D warnings  # Must be clean
```

---

## TASK 1: Wire robots.txt + rate limiting into fetch executor (~2h)

### Task 1.1: Add RobotsCache + DomainRateLimiter fields to TaskExecutor

**Files:**
- Modify: `nika-engine/src/runtime/executor/mod.rs:58-107` (struct fields)
- Modify: `nika-engine/src/runtime/executor/mod.rs:123-249` (constructor)

**Step 1: Add fields to TaskExecutor struct**

In `nika-engine/src/runtime/executor/mod.rs`, add two fields after `resolved_agents` (line 106):

```rust
// After line 106 (resolved_agents field), add:
    /// robots.txt compliance cache — shared across all fetch tasks in a workflow.
    /// `None` when respect_robots_txt is false (opt-out).
    robots_cache: Option<Arc<crate::runtime::robots::RobotsCache>>,
    /// Per-domain rate limiter for polite crawling.
    /// `None` when rate limiting is disabled (rate_limit_rps = 0).
    domain_rate_limiter: Option<Arc<crate::runtime::rate_limit::DomainRateLimiter>>,
```

**Step 2: Initialize fields in `with_policy()` constructor**

In `nika-engine/src/runtime/executor/mod.rs`, inside `with_policy()`, before the `Ok(Self {` block (around line 226), add:

```rust
        // Crawl intelligence: robots.txt + per-domain rate limiting
        // Both are always-on by default. Future: make configurable via [fetch] in nika.toml.
        let robots_cache = Some(Arc::new(crate::runtime::robots::RobotsCache::new(
            &format!("nika/{}", env!("CARGO_PKG_VERSION")),
        )));
        let domain_rate_limiter = Some(Arc::new(
            crate::runtime::rate_limit::DomainRateLimiter::new(10), // 10 req/s per domain
        ));
```

Then add the fields to the struct literal inside `Ok(Self { ... })`:

```rust
            robots_cache,
            domain_rate_limiter,
```

**Step 3: Run `cargo check -p nika-engine`**

Expected: PASS (fields added but not yet used — no compile errors).

**Step 4: Commit**

```bash
git add nika-engine/src/runtime/executor/mod.rs
git commit -m "feat(fetch): add robots_cache + domain_rate_limiter fields to TaskExecutor

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 1.2: Write failing wiremock test — robots.txt blocks /admin/

**Files:**
- Modify: `nika-engine/src/runtime/executor/tests_wiremock.rs`

**Step 1: Write the test**

Add at the end of `tests_wiremock.rs`:

```rust
// ═══════════════════════════════════════════════════════════════
// Robots.txt + Rate Limiting Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_blocked_by_robots_txt() {
    let server = MockServer::start().await;

    // Serve robots.txt that blocks /admin/
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("User-agent: *\nDisallow: /admin/"),
        )
        .mount(&server)
        .await;

    // Serve the actual page (should never be reached)
    Mock::given(method("GET"))
        .and(path("/admin/secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string("secret"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, event_log) = setup();
    let task_id: Arc<str> = Arc::from("robots_blocked");
    let params = fetch_params(&format!("{}/admin/secret", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await;

    assert!(result.is_err(), "Should be blocked by robots.txt");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("robots.txt"),
        "Error should mention robots.txt, got: {err}"
    );

    // Verify PolicyBlocked event was emitted
    let events = event_log.events();
    let blocked = events.iter().find(|e| {
        matches!(
            &e.kind,
            EventKind::PolicyBlocked { policy_type, .. } if policy_type == "robots_txt"
        )
    });
    assert!(blocked.is_some(), "Should emit PolicyBlocked event for robots.txt");
}
```

**Step 2: Run test to verify it FAILS**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_blocked_by_robots_txt --exact
```

Expected: FAIL — robots.txt check not yet wired into fetch.rs, so the request succeeds and returns "secret".

---

### Task 1.3: Write failing test — robots.txt allows /public/

**Files:**
- Modify: `nika-engine/src/runtime/executor/tests_wiremock.rs`

**Step 1: Write the test**

```rust
#[tokio::test]
async fn wiremock_fetch_allowed_by_robots_txt() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("User-agent: *\nDisallow: /admin/"),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/public/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string("public content"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("robots_allowed");
    let params = fetch_params(&format!("{}/public/page", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    assert_eq!(result, "public content");
}
```

**Step 2: Run test — should PASS (no block on /public/)**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_allowed_by_robots_txt --exact
```

Expected: PASS even before integration — robots check not wired yet, fetch just goes through.

---

### Task 1.4: Write failing test — no robots.txt (404) allows all

**Files:**
- Modify: `nika-engine/src/runtime/executor/tests_wiremock.rs`

```rust
#[tokio::test]
async fn wiremock_fetch_no_robots_txt_allows_all() {
    let server = MockServer::start().await;

    // No /robots.txt mock → will return 404
    Mock::given(method("GET"))
        .and(path("/anything"))
        .respond_with(ResponseTemplate::new(200).set_body_string("allowed"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();
    let task_id: Arc<str> = Arc::from("no_robots");
    let params = fetch_params(&format!("{}/anything", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    assert_eq!(result, "allowed");
}
```

**Step 3: Commit tests**

```bash
git add nika-engine/src/runtime/executor/tests_wiremock.rs
git commit -m "test(fetch): add robots.txt wiremock tests (RED for blocked case)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 1.5: Implement robots.txt + rate limiting integration in fetch.rs

**Files:**
- Modify: `nika-engine/src/runtime/executor/fetch.rs:168-185` (after SSRF policy check, before TemplateResolved event)

**Step 1: Add robots.txt + rate limiting checks**

In `fetch.rs`, AFTER the SSRF policy block (line 184: `return Err(NikaError::PolicyViolation { reason });`), and BEFORE the TemplateResolved event emission (line 187), insert:

```rust
        // ── robots.txt compliance ──────────────────────────────────────────
        // Check AFTER SSRF validation (we don't want to fetch robots.txt for
        // blocked hosts) but BEFORE the actual request.
        if let Some(ref robots) = self.robots_cache {
            if let Ok(parsed) = url::Url::parse(&url) {
                if !robots.is_allowed(&parsed, &self.http_client).await {
                    self.event_log.emit(EventKind::PolicyBlocked {
                        task_id: Arc::clone(task_id),
                        verb: "fetch".to_string(),
                        policy_type: "robots_txt".to_string(),
                        reason: format!("robots.txt disallows: {}", url),
                    });
                    tracing::info!(
                        task_id = %task_id,
                        url = %url,
                        "fetch: blocked by robots.txt"
                    );
                    return Err(NikaError::PolicyViolation {
                        reason: format!("robots.txt disallows: {}", url),
                    });
                }
            }
        }

        // ── Per-domain rate limiting ───────────────────────────────────────
        // Polite crawling: wait until the rate limiter permits a request to
        // this domain. Different domains have independent quotas.
        if let Some(ref limiter) = self.domain_rate_limiter {
            if let Ok(parsed) = url::Url::parse(&url) {
                if let Some(domain) = parsed.host_str() {
                    limiter.acquire(domain).await;
                }
            }
        }
```

**Step 2: Run the blocked test to verify it PASSES**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_blocked_by_robots_txt --exact
```

Expected: PASS — robots.txt now blocks /admin/secret.

**Step 3: Run all three robots tests**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_blocked_by_robots --exact
cargo test -p nika-engine --lib -- wiremock_fetch_allowed_by_robots --exact
cargo test -p nika-engine --lib -- wiremock_fetch_no_robots --exact
```

Expected: ALL PASS.

**Step 4: Run full test suite**

```bash
cargo test --workspace --lib --exclude nika-py
```

Expected: 9695+ pass, 0 fail.

**Step 5: Commit**

```bash
git add nika-engine/src/runtime/executor/fetch.rs
git commit -m "feat(fetch): integrate robots.txt + rate limiting into fetch execution

Robots check after SSRF validation, before HTTP request.
Rate limiting per-domain via governor token bucket.
Both are always-on (10 req/s default).

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## TASK 2: AST pipeline for session + cache fields on fetch verb (~3h)

### Task 2.1: Add `session` and `cache` to RawFetchAction

**Files:**
- Modify: `nika-core/src/ast/raw/action.rs:114-144`

**Step 1: Add fields to RawFetchAction struct**

After the `selector` field (last field in the struct), add:

```rust
    /// Enable cookie jar for session persistence across fetch tasks.
    /// When true, cookies from Set-Cookie headers are stored and sent on subsequent requests.
    pub session: Option<Spanned<bool>>,

    /// Enable HTTP response caching with ETag / If-Modified-Since.
    /// When true, repeat fetches to the same URL use conditional requests.
    pub cache: Option<Spanned<bool>>,
```

**Step 2: Run `cargo check -p nika-core`**

Expected: FAIL — struct literals in parser.rs and tests don't include new fields (since `RawFetchAction` does NOT derive `Default` on all construction sites).

Actually: RawFetchAction does `#[derive(Debug, Clone, Default)]`, so sites using `..Default::default()` will work. But explicit struct literals in `parse_fetch_action()` will fail.

**Step 3: Fix `parse_fetch_action()` in parser.rs**

In `nika-core/src/ast/raw/parser.rs`, inside `parse_fetch_action()` (line ~840), add to the `Ok(RawFetchAction { ... })` struct literal:

```rust
        session: get_bool_field(file, m, "session")?,
        cache: get_bool_field(file, m, "cache")?,
```

**Step 4: Run `cargo check -p nika-core`**

Expected: PASS (Default handles the remaining test sites).

**Step 5: Commit**

```bash
git add nika-core/src/ast/raw/action.rs nika-core/src/ast/raw/parser.rs
git commit -m "feat(ast): add session and cache fields to RawFetchAction

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.2: Add `session` and `cache` to AnalyzedFetchAction

**Files:**
- Modify: `nika-core/src/ast/analyzed/task.rs:202-237`

**Step 1: Add fields to AnalyzedFetchAction struct**

After the `span` field (last field), add:

```rust
    /// Enable cookie jar for session persistence
    pub session: bool,

    /// Enable HTTP response caching
    pub cache: bool,
```

**Step 2: Fix `analyze_fetch()` in analyzer**

In `nika-core/src/ast/analyzer/analyze.rs`, inside the `AnalyzedFetchAction { ... }` struct literal returned by `analyze_fetch()`, add:

```rust
        session: raw.session.as_ref().map(|s| s.value).unwrap_or(false),
        cache: raw.cache.as_ref().map(|s| s.value).unwrap_or(false),
```

**Step 3: Run `cargo check -p nika-core`**

Expected: FAIL — other struct literal sites for AnalyzedFetchAction need the new fields.

**Step 4: Fix ALL struct literal sites**

Search for all `AnalyzedFetchAction {` across the workspace and add `session: false, cache: false,` to each. Key locations:
- `nika-engine/src/ast/lower.rs` — `unlower()` function (roundtrip test helper) — multiple sites
- Any test files constructing AnalyzedFetchAction directly

Use this search command to find them all:
```bash
cargo check -p nika-core 2>&1 | grep "missing.*session"
cargo check -p nika-engine 2>&1 | grep "missing.*session"
```

For each error site, add `session: false, cache: false,` to the struct literal.

**Step 5: Run `cargo check --workspace`**

Expected: PASS.

**Step 6: Commit**

```bash
git add -A  # All modified files
git commit -m "feat(ast): add session and cache to AnalyzedFetchAction + analyzer

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.3: Add `session` and `cache` to FetchParams (runtime)

**Files:**
- Modify: `nika-engine/src/ast/action.rs:370-400`
- Modify: `nika-engine/src/ast/lower.rs:260-275`

**Step 1: Add fields to FetchParams struct**

After the `selector` field in `FetchParams`, add:

```rust
    /// Enable cookie jar for session persistence across fetch tasks
    #[serde(default)]
    pub session: Option<bool>,

    /// Enable HTTP response caching with ETag / If-Modified-Since
    #[serde(default)]
    pub cache: Option<bool>,
```

**Step 2: Fix `lower_fetch()` in lower.rs**

In `nika-engine/src/ast/lower.rs`, inside `lower_fetch()`, add to the `FetchParams { ... }` struct literal:

```rust
        session: if fetch.session { Some(true) } else { None },
        cache: if fetch.cache { Some(true) } else { None },
```

**Step 3: Fix ALL FetchParams struct literals**

Search for compilation errors:
```bash
cargo check -p nika-engine 2>&1 | grep "missing.*session"
```

Key locations to fix (add `session: None, cache: None,` to each):
- `nika-engine/src/runtime/executor/tests_wiremock.rs:44-58` — `fetch_params()` helper
- `nika-engine/src/ast/lower.rs` — roundtrip test helpers
- `nika-engine/src/ast/action.rs` — test functions
- `nika-cli/src/verbs.rs` — CLI direct fetch verb handler (if exists in cli crate)

**Step 4: Run `cargo check --workspace`**

Expected: PASS.

**Step 5: Run full test suite**

```bash
cargo test --workspace --lib --exclude nika-py
```

Expected: 9695+ pass, 0 fail (all existing tests still pass with `session: None, cache: None` defaults).

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(ast): add session and cache to FetchParams + lowering

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.4: Write failing test — cookie jar persists across tasks

**Files:**
- Modify: `nika-engine/src/runtime/executor/tests_wiremock.rs`

**Step 1: Write the test**

```rust
// ═══════════════════════════════════════════════════════════════
// Cookie / Session Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_session_cookies_persist() {
    let server = MockServer::start().await;

    // Login endpoint sets a session cookie
    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("logged in")
                .append_header("Set-Cookie", "session=abc123; Path=/"),
        )
        .mount(&server)
        .await;

    // Profile endpoint requires the cookie
    Mock::given(method("GET"))
        .and(path("/profile"))
        .and(header("Cookie", "session=abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_string("user data"))
        .mount(&server)
        .await;

    // Profile WITHOUT cookie returns 401
    Mock::given(method("GET"))
        .and(path("/profile"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();

    // Task 1: POST /login with session: true → gets Set-Cookie
    let task_id: Arc<str> = Arc::from("login");
    let mut login_params = fetch_params(&format!("{}/login", server.uri()), "POST");
    login_params.session = Some(true);
    let action = TaskAction::Fetch { fetch: login_params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "logged in");

    // Task 2: GET /profile with session: true → sends Cookie header
    let task_id2: Arc<str> = Arc::from("profile");
    let mut profile_params = fetch_params(&format!("{}/profile", server.uri()), "GET");
    profile_params.session = Some(true);
    let action2 = TaskAction::Fetch { fetch: profile_params };
    let result2 = executor
        .execute(&task_id2, &action2, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result2, "user data", "Cookie should be sent on second request");
}

#[tokio::test]
async fn wiremock_fetch_session_disabled_no_cookies() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("logged in")
                .append_header("Set-Cookie", "session=abc123; Path=/"),
        )
        .mount(&server)
        .await;

    // Profile with cookie matcher — will only match if Cookie is sent
    Mock::given(method("GET"))
        .and(path("/profile"))
        .and(header("Cookie", "session=abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_string("user data"))
        .named("with-cookie")
        .mount(&server)
        .await;

    // Profile without cookie
    Mock::given(method("GET"))
        .and(path("/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_string("anonymous"))
        .named("without-cookie")
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();

    // Task 1: POST /login WITHOUT session: true
    let task_id: Arc<str> = Arc::from("login_nosess");
    let login_params = fetch_params(&format!("{}/login", server.uri()), "POST");
    // session defaults to None (disabled)
    let action = TaskAction::Fetch { fetch: login_params };
    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // Task 2: GET /profile — should NOT send Cookie (session disabled)
    let task_id2: Arc<str> = Arc::from("profile_nosess");
    let profile_params = fetch_params(&format!("{}/profile", server.uri()), "GET");
    let action2 = TaskAction::Fetch { fetch: profile_params };
    let result2 = executor
        .execute(&task_id2, &action2, &bindings, &datastore, None)
        .await
        .unwrap();
    // Without session, the generic mock (without cookie matcher) should respond
    assert_eq!(result2, "anonymous", "Should NOT send cookies when session is disabled");
}
```

**Step 2: Run tests to verify they FAIL**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_session --exact
```

Expected: FAIL — cookie jar not yet integrated into fetch.rs.

**Step 3: Commit failing tests**

```bash
git add nika-engine/src/runtime/executor/tests_wiremock.rs
git commit -m "test(fetch): add cookie session wiremock tests (RED)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.5: Implement cookie jar in TaskExecutor

**Files:**
- Modify: `nika-engine/src/runtime/executor/mod.rs` (struct + constructor)
- Modify: `nika-engine/src/runtime/executor/fetch.rs` (client builders)

**Step 1: Add cookie jar field to TaskExecutor**

In `nika-engine/src/runtime/executor/mod.rs`, add to the struct:

```rust
    /// Shared cookie jar for session persistence (fetch tasks with session: true).
    cookie_jar: Arc<reqwest_cookie_store::CookieStoreRwLock>,
```

Add import at top of file:

```rust
use reqwest_cookie_store::CookieStoreRwLock;
```

Initialize in `with_policy()` constructor:

```rust
        let cookie_jar = Arc::new(CookieStoreRwLock::new(
            cookie_store::CookieStore::default(),
        ));
```

And add to struct literal:

```rust
            cookie_jar,
```

**Step 2: Wire cookie jar into fetch.rs client builders**

In `nika-engine/src/runtime/executor/fetch.rs`, the key integration point is the client builder section (lines 202-271).

The shared `self.http_client` (line 270: `Cow::Borrowed(&self.http_client)`) does NOT get the cookie jar — it's the default path for non-session requests.

For `session: true`, we need to build a custom client WITH the cookie jar. Modify the `needs_custom_client` condition (line 200-201):

```rust
        let session_enabled = fetch.session == Some(true);
        let needs_custom_client =
            fetch.follow_redirects == Some(false) || !pinned_addrs.is_empty() || is_response_full || session_enabled;
```

Then inside the `if needs_custom_client` block, after the builder is created but before `.build()`, add:

```rust
            // Wire cookie jar for session persistence
            if session_enabled {
                builder = builder.cookie_provider(Arc::clone(&self.cookie_jar));
            }
```

**Step 3: Run cookie tests**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_session --exact
```

Expected: BOTH PASS.

**Step 4: Run full suite**

```bash
cargo test --workspace --lib --exclude nika-py
```

Expected: 9695+ pass, 0 fail.

**Step 5: Commit**

```bash
git add nika-engine/src/runtime/executor/mod.rs nika-engine/src/runtime/executor/fetch.rs
git commit -m "feat(fetch): cookie jar integration with session: true

Shared CookieStoreRwLock in TaskExecutor, wired into reqwest
client builder when session: true. Cookies persist across
fetch tasks within the same workflow run.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.6: Write failing test — ETag cache returns 304

**Files:**
- Modify: `nika-engine/src/runtime/executor/tests_wiremock.rs`

**Step 1: Write the tests**

```rust
// ═══════════════════════════════════════════════════════════════
// ETag / Cache Tests
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn wiremock_fetch_cache_304_returns_cached() {
    let server = MockServer::start().await;

    // First request: 200 with ETag
    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("original content")
                .append_header("ETag", "\"v1\""),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    // Second request with If-None-Match: return 304
    Mock::given(method("GET"))
        .and(path("/data"))
        .and(header("If-None-Match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();

    // First fetch: cache: true → stores response + ETag
    let task_id: Arc<str> = Arc::from("cache_first");
    let mut params = fetch_params(&format!("{}/data", server.uri()), "GET");
    params.cache = Some(true);
    let action = TaskAction::Fetch { fetch: params };
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "original content");

    // Second fetch: cache: true → sends If-None-Match → gets 304 → returns cached
    let task_id2: Arc<str> = Arc::from("cache_second");
    let mut params2 = fetch_params(&format!("{}/data", server.uri()), "GET");
    params2.cache = Some(true);
    let action2 = TaskAction::Fetch { fetch: params2 };
    let result2 = executor
        .execute(&task_id2, &action2, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result2, "original content", "Should return cached body on 304");
}

#[tokio::test]
async fn wiremock_fetch_cache_disabled_no_conditional_headers() {
    let server = MockServer::start().await;

    // Serve with ETag
    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("content")
                .append_header("ETag", "\"v1\""),
        )
        .expect(2) // Both requests should hit the server
        .mount(&server)
        .await;

    let (executor, bindings, datastore, _) = setup();

    // First fetch without cache
    let task_id: Arc<str> = Arc::from("nocache_1");
    let params = fetch_params(&format!("{}/data", server.uri()), "GET");
    let action = TaskAction::Fetch { fetch: params };
    executor
        .execute(&task_id, &action, &bindings, &datastore, None)
        .await
        .unwrap();

    // Second fetch without cache — should NOT send If-None-Match
    let task_id2: Arc<str> = Arc::from("nocache_2");
    let params2 = fetch_params(&format!("{}/data", server.uri()), "GET");
    let action2 = TaskAction::Fetch { fetch: params2 };
    let result = executor
        .execute(&task_id2, &action2, &bindings, &datastore, None)
        .await
        .unwrap();
    assert_eq!(result, "content");
    // The expect(2) on the mock ensures both requests hit the server (no cache)
}
```

**Step 2: Run tests to verify they FAIL**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_cache --exact
```

Expected: FAIL — cache not wired into fetch.rs.

**Step 3: Commit**

```bash
git add nika-engine/src/runtime/executor/tests_wiremock.rs
git commit -m "test(fetch): add ETag cache wiremock tests (RED)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2.7: Implement ETag cache in fetch executor

**Files:**
- Modify: `nika-engine/src/runtime/executor/mod.rs` (add FetchCache field)
- Modify: `nika-engine/src/runtime/executor/fetch.rs` (conditional headers + 304 handling)

**Step 1: Add FetchCache to TaskExecutor**

In `mod.rs`, add field:

```rust
    /// HTTP response cache for ETag / If-Modified-Since conditional requests.
    fetch_cache: Arc<crate::runtime::fetch_cache::FetchCache>,
```

Initialize in constructor:

```rust
        let fetch_cache = Arc::new(crate::runtime::fetch_cache::FetchCache::new());
```

Add to struct literal:

```rust
            fetch_cache,
```

**Step 2: Add conditional headers to request (fetch.rs)**

In `fetch.rs`, AFTER headers are added (around line 316) and BEFORE the JSON/body section (line 321), add:

```rust
        // ── ETag / conditional request headers ─────────────────────────────
        // When cache: true, add If-None-Match / If-Modified-Since from cached
        // response (if we've seen this URL before in this workflow).
        if fetch.cache == Some(true) {
            for (name, value) in self.fetch_cache.conditional_headers(&url) {
                request = request.header(&name, &value);
            }
        }
```

**Step 3: Handle 304 response**

In `fetch.rs`, inside the response handling section, AFTER the retryable status check (around line 500-520) and BEFORE the response mode dispatch (line ~596), add a 304 handler:

```rust
                    // ── 304 Not Modified: return cached body ───────────────
                    if status_code == 304 && fetch.cache == Some(true) {
                        if let Some(cached) = self.fetch_cache.get(&url) {
                            tracing::debug!(
                                task_id = %task_id,
                                url = %url,
                                "fetch: 304 Not Modified, returning cached body"
                            );
                            return Ok(cached.body);
                        }
                    }
```

**Step 4: Store response in cache**

After a successful response is read (in the default text response path, around line 839-949), when `cache: true`, store the response:

The exact location depends on where the body text is available. Look for the variable that holds the final response text before it's returned. In the default (text) response mode path, after extraction is complete and the body is ready to return, add:

```rust
                    // Store in cache if caching enabled
                    if fetch.cache == Some(true) {
                        let etag = response.headers()
                            .get("etag")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string());
                        let last_modified = response.headers()
                            .get("last-modified")
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.to_string());
                        // body_text is the final response — store before returning
                        self.fetch_cache.store(
                            &url,
                            body_text.clone(),
                            status_code,
                            etag,
                            last_modified,
                        );
                    }
```

**IMPORTANT**: The exact insertion point depends on where `response.headers()` is still accessible (before the body is consumed). You may need to capture ETag and Last-Modified headers BEFORE reading the body, then store AFTER. Pattern:

```rust
// Before body consumption:
let cache_etag = if fetch.cache == Some(true) {
    response.headers().get("etag").and_then(|v| v.to_str().ok()).map(|s| s.to_string())
} else { None };
let cache_last_modified = if fetch.cache == Some(true) {
    response.headers().get("last-modified").and_then(|v| v.to_str().ok()).map(|s| s.to_string())
} else { None };

// ... body consumption ...

// After body is read, before return:
if fetch.cache == Some(true) {
    self.fetch_cache.store(&url, body_text.clone(), status_code, cache_etag, cache_last_modified);
}
```

**Step 5: Run cache tests**

```bash
cargo test -p nika-engine --lib -- wiremock_fetch_cache --exact
```

Expected: BOTH PASS.

**Step 6: Run full suite**

```bash
cargo test --workspace --lib --exclude nika-py
```

Expected: 9695+ pass, 0 fail.

**Step 7: Commit**

```bash
git add nika-engine/src/runtime/executor/mod.rs nika-engine/src/runtime/executor/fetch.rs
git commit -m "feat(fetch): ETag/304 conditional request caching with cache: true

FetchCache stores response body + ETag + Last-Modified per URL.
On repeat fetch with cache: true, sends If-None-Match header.
304 responses return cached body without re-downloading.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## TASK 3: BUG-040 — PartialSuccess for_each writes artifact (~30min)

### Task 3.1: Write the test

**Files:**
- Modify: `nika-engine/src/runtime/runner.rs` (test section at bottom)

**Step 1: Read existing for_each test patterns**

Use the `create_for_each_workflow()` helper at runner.rs:3593. Understand how:
- `fail_fast: false` enables partial success
- `TaskOutcome::PartialSuccess` is set when some items fail
- Artifacts are written via the artifact pipeline

**Step 2: Write the test**

Add to the test section of runner.rs:

```rust
#[tokio::test]
async fn for_each_partial_success_collects_results_with_nulls() {
    // for_each with 3 items: item 1 succeeds, item 2 fails (bad command), item 3 succeeds
    // fail_fast: false → PartialSuccess with [result1, null, result3]
    let workflow = create_for_each_workflow(
        "partial",
        r#"["echo_ok", "bad_command_that_fails", "echo_ok2"]"#,
        "item",
        // Use a conditional: if item starts with "echo" run echo, else run invalid command
        // Simpler: just echo the item, but one item is a command that will fail
        "echo {{with.item}}",
        None,   // sequential
        false,  // fail_fast = false → continue on failure
        false,  // no shell
    );

    let event_log = EventLog::new();
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;

    // Should complete (not error) because fail_fast is false
    assert!(result.is_ok(), "Workflow should complete with PartialSuccess");

    // Check task outcome
    let task_result = runner.datastore.get("partial");
    assert!(task_result.is_some(), "Should have task result");
    let tr = task_result.unwrap();
    assert!(
        tr.is_usable(),
        "PartialSuccess should be usable by downstream tasks"
    );

    // The output should be a JSON array with results for succeeded items
    // and null for failed items
    let output: serde_json::Value = serde_json::from_str(&tr.output).unwrap();
    assert!(output.is_array(), "for_each output should be an array");
    let arr = output.as_array().unwrap();
    assert_eq!(arr.len(), 3, "Should have 3 elements (one per for_each item)");

    // Items 0 and 2 succeeded (echo worked), item 1 may have failed
    // The exact behavior depends on whether echo handles bad_command_that_fails
    // Since we're using echo (not shell), echo will succeed for all items
    // Let's use a different approach: exec a nonexistent command for failure
}
```

**IMPORTANT**: The test needs careful design. `echo` always succeeds. To make item 2 fail, use a workflow where one item triggers a command that doesn't exist. The `create_for_each_workflow` helper creates exec tasks. With `shell: false`, a command like `nonexistent_binary` will fail.

Better approach — create a custom workflow directly:

```rust
#[tokio::test]
async fn for_each_partial_success_preserves_order() {
    use nika_core::ast::{SchemaVersion, Span, TaskTable};
    use crate::ast::analyzed::*;

    // Build workflow with for_each that has fail_fast: false
    // Use exec verb: items are shell commands, some will fail
    let mut tasks = TaskTable::new();
    tasks.insert(
        "partial".to_string(),
        AnalyzedTask {
            id: "partial".to_string(),
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "{{with.cmd}}".to_string(),
                shell: true,
                cwd: None,
                timeout_ms: Some(5000),
                env: Default::default(),
                max_stdout: None,
                span: Span::dummy(),
            }),
            for_each: Some(AnalyzedForEach {
                items: r#"["echo ok1", "/bin/false", "echo ok3"]"#.to_string(),
                as_var: "cmd".to_string(),
                concurrency: None,
                fail_fast: false,
                span: Span::dummy(),
            }),
            with_bindings: Default::default(),
            depends_on: vec![],
            condition: None,
            retry: None,
            provider: None,
            model: None,
            output: None,
            artifact: None,
            structured: None,
            span: Span::dummy(),
        },
    );

    let workflow = AnalyzedWorkflow {
        schema: SchemaVersion::V0_12,
        name: "partial_test".to_string(),
        description: None,
        provider: Some("mock".to_string()),
        model: None,
        inputs: Default::default(),
        context: Default::default(),
        skills: Default::default(),
        agents: Default::default(),
        artifacts: None,
        tasks,
        includes: vec![],
    };

    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;
    assert!(result.is_ok(), "Workflow should not hard-fail with fail_fast: false");

    let task_result = runner.datastore.get("partial");
    assert!(task_result.is_some());
    let tr = task_result.unwrap();
    assert!(tr.is_usable(), "PartialSuccess should be usable");

    let output: serde_json::Value = serde_json::from_str(&tr.output).unwrap();
    let arr = output.as_array().expect("Should be an array");
    assert_eq!(arr.len(), 3);
    // Item 0: "echo ok1" succeeded → contains "ok1"
    assert!(arr[0].as_str().unwrap_or("").contains("ok1"), "Item 0 should succeed");
    // Item 1: "/bin/false" failed → null
    assert!(arr[1].is_null(), "Item 1 should be null (failed)");
    // Item 2: "echo ok3" succeeded → contains "ok3"
    assert!(arr[2].as_str().unwrap_or("").contains("ok3"), "Item 2 should succeed");
}
```

**Step 3: Run test**

```bash
cargo test -p nika-engine --lib -- for_each_partial_success_preserves_order --exact
```

Expected: Should PASS if the existing for_each implementation correctly handles fail_fast: false with null placeholders. If it FAILS, investigate and fix.

**Step 4: Commit**

```bash
git add nika-engine/src/runtime/runner.rs
git commit -m "test(runtime): verify PartialSuccess for_each preserves order with nulls (BUG-040)

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## TASK 4: Comprehensive E2E Verification + Edge Case Tests (~1.5h)

### Task 4.1: Transform edge case tests

**Files:**
- Modify: `nika-core/src/binding/transform.rs` (test section)

**Step 1: Write edge case tests**

Add to the test module in transform.rs:

```rust
#[test]
fn content_hash_deterministic() {
    let h1 = apply_transform(TransformOp::ContentHash, &json!("hello world"));
    let h2 = apply_transform(TransformOp::ContentHash, &json!("hello world"));
    assert_eq!(h1.unwrap(), h2.unwrap(), "Same input must produce same hash");
}

#[test]
fn content_hash_different_inputs() {
    let h1 = apply_transform(TransformOp::ContentHash, &json!("hello"));
    let h2 = apply_transform(TransformOp::ContentHash, &json!("world"));
    assert_ne!(h1.unwrap(), h2.unwrap());
}

#[test]
fn unique_urls_non_url_strings_kept() {
    let input = json!(["not-a-url", "also-not-url", "not-a-url"]);
    let result = apply_transform(TransformOp::UniqueUrls, &input).unwrap();
    // Non-URLs should be kept (not crash) but duplicates removed
    let arr = result.as_array().unwrap();
    assert!(arr.len() <= 2, "Duplicates should be removed");
}

#[test]
fn starts_with_on_number_errors() {
    let result = apply_transform(
        TransformOp::StartsWith("x".into()),
        &json!(42),
    );
    assert!(result.is_err(), "starts_with on number should error");
}

#[test]
fn ends_with_on_null_errors() {
    let result = apply_transform(
        TransformOp::EndsWith("x".into()),
        &serde_json::Value::Null,
    );
    assert!(result.is_err(), "ends_with on null should error");
}

#[test]
fn contains_on_bool_errors() {
    let result = apply_transform(
        TransformOp::Contains("x".into()),
        &json!(true),
    );
    assert!(result.is_err(), "contains on bool should error");
}
```

**Note**: Use the correct `apply_transform` function name — check existing tests in transform.rs for the exact helper function or method call pattern (likely `TransformOp::ContentHash.apply(&json!(...))` or similar).

**Step 2: Run tests**

```bash
cargo test -p nika-core --lib -- transform --exact
```

Expected: ALL PASS (these are verification tests for existing behavior).

---

### Task 4.2: Data tools edge case tests

**Files:**
- Modify: `nika-engine/src/runtime/builtin/aggregate.rs` (test section)
- Modify: `nika-engine/src/runtime/builtin/json_transform.rs` (test section)

**Step 1: Aggregate edge cases**

```rust
#[tokio::test]
async fn aggregate_sum_non_numeric_returns_zero() {
    let tool = AggregateTool;
    let result = tool
        .call(json!({"array": [{"val": "text"}, {"val": "more text"}], "ops": ["sum"], "field": "val"}).to_string())
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["sum"], json!(0), "Sum of non-numeric values should be 0");
}

#[tokio::test]
async fn aggregate_count_includes_non_numeric() {
    let tool = AggregateTool;
    let result = tool
        .call(json!({"array": [{"val": "text"}, {"val": 5}], "ops": ["count"], "field": "val"}).to_string())
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["count"], json!(2), "Count should include all elements");
}
```

**Step 2: json_query edge case**

```rust
#[tokio::test]
async fn json_query_empty_result_is_array_not_null() {
    let tool = JsonQueryTool;
    let input = json!({"data": {"a": 1}, "query": "$.missing"}).to_string();
    let result = tool.call(input).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed.is_array(), "Empty result should be [], got: {parsed}");
    assert_eq!(parsed.as_array().unwrap().len(), 0);
}
```

**Step 3: Run tests**

```bash
cargo test -p nika-engine --lib -- aggregate --exact
cargo test -p nika-engine --lib -- json_query --exact
```

Expected: ALL PASS.

**Step 4: Commit all edge case tests**

```bash
git add -A
git commit -m "test(builtin): edge case tests for transforms, aggregate, json_query

Verifies: content_hash determinism, unique_urls with non-URLs,
starts_with/ends_with/contains type errors, aggregate non-numeric
handling, json_query BUG-037 empty result.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 4.3: Architectural consistency checks

**Step 1: Run lint + format**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: ZERO warnings, ZERO errors.

**Step 2: Check for dead code in new files**

```bash
grep -rn "dead_code\|#\[allow(dead" nika-engine/src/runtime/robots.rs nika-engine/src/runtime/rate_limit.rs nika-engine/src/runtime/fetch_cache.rs
```

Expected: No dead_code allows (clean code).

**Step 3: Check no TODO/FIXME left**

```bash
grep -rn "TODO\|FIXME\|HACK\|XXX" nika-engine/src/runtime/robots.rs nika-engine/src/runtime/rate_limit.rs nika-engine/src/runtime/fetch_cache.rs nika-engine/src/runtime/builtin/aggregate.rs
```

Expected: Zero matches (all resolved).

**Step 4: Verify tool count in router**

```bash
grep -c "tools.insert\|register(" nika-engine/src/runtime/builtin/router.rs
```

Document the count for the record.

---

### Task 4.4: Schema file update

**Files:**
- Modify: `nika-engine/schemas/nika-workflow.schema.json`

**Step 1: Add `"slim"` to response mode enum**

Find the `response` field definition and update:

```json
"response": {
  "type": "string",
  "enum": ["full", "binary", "slim"],
  "description": "Response mode: full (JSON with status/headers/body), binary (CAS store), or slim (metadata only)"
}
```

**Step 2: Add `session` and `cache` to fetch properties**

```json
"session": {
  "type": "boolean",
  "description": "Enable cookie jar for session persistence across fetch tasks"
},
"cache": {
  "type": "boolean",
  "description": "Enable HTTP response caching with ETag / If-Modified-Since conditional requests"
}
```

**Step 3: Commit**

```bash
git add nika-engine/schemas/nika-workflow.schema.json
git commit -m "docs(schema): add slim response mode, session and cache fetch fields

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 4.5: Final full suite verification

**Step 1: Run EVERYTHING**

```bash
cargo test --workspace --lib --exclude nika-py
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

**Step 2: Count tests**

```bash
cargo test --workspace --lib --exclude nika-py 2>&1 | grep "^test result:" | awk '{sum += $4} END {print "Total tests:", sum}'
```

Expected: 9720+ (25+ new tests added).

**Step 3: Git log review**

```bash
git log --oneline -15
```

Verify: 8-10 clean commits, all with co-author lines, conventional commit format.

---

## Summary

| Task | Tests Added | Commits | Key Integration |
|------|------------|---------|-----------------|
| 1.1-1.5 | 3 wiremock | 3 | robots_cache + domain_rate_limiter in TaskExecutor + fetch.rs |
| 2.1-2.3 | 0 (AST plumbing) | 3 | session + cache fields through 6-stage AST pipeline |
| 2.4-2.5 | 2 wiremock | 2 | cookie_jar in TaskExecutor, wired when session: true |
| 2.6-2.7 | 2 wiremock | 2 | fetch_cache conditional headers + 304 handling |
| 3.1 | 1 runner | 1 | PartialSuccess for_each verification |
| 4.1-4.2 | 8+ unit | 1 | Edge case verification for transforms + tools |
| 4.3-4.4 | 0 | 1 | Schema update + lint verification |
| **Total** | **~16+** | **~13** | **Full green suite** |
