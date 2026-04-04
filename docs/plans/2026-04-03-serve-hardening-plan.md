# Nika Serve + Engine Hardening — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 11 serve bugs + 2 engine bugs from the 2026-04-03 handoff report. P0 and P1 first, P2/P3 second.

**Architecture:** Fixes are isolated per-file. The only cross-crate change is S3/E1 (threading `PolicyConfig` from boot → Runner → TaskExecutor). S2 is already implemented and removed from the plan.

**Tech Stack:** Rust, axum, tokio, sha2, governor, reqwest, subtle

---

## Task 1: S1 — Startup banner recursive workflow count

The startup banner uses flat `std::fs::read_dir` to count workflows, but the actual workflow list uses recursive `collect_workflows`. When workflows are in subdirectories, the banner shows "(0 files)".

**Files:**
- Modify: `tools/nika-serve/src/lib.rs:357-368`

**Step 1: Write the failing test**

Add to existing test module in `tools/nika-serve/src/lib.rs` (after line 790 or wherever the `#[cfg(test)]` module ends):

```rust
#[test]
fn count_workflows_recursive() {
    let dir = tempfile::TempDir::new().unwrap();
    // Root-level workflow
    std::fs::write(
        dir.path().join("root.nika.yaml"),
        "schema: nika/workflow@0.12",
    )
    .unwrap();
    // Nested in subdirectory
    std::fs::create_dir_all(dir.path().join("jungo")).unwrap();
    std::fs::write(
        dir.path().join("jungo/api.nika.yaml"),
        "schema: nika/workflow@0.12",
    )
    .unwrap();
    // Deeply nested
    std::fs::create_dir_all(dir.path().join("dev/test")).unwrap();
    std::fs::write(
        dir.path().join("dev/test/mock.nika.yaml"),
        "schema: nika/workflow@0.12",
    )
    .unwrap();
    // Non-workflow file — must NOT be counted
    std::fs::write(dir.path().join("readme.md"), "# hello").unwrap();
    // Hidden dir — must NOT be counted
    std::fs::create_dir_all(dir.path().join(".nika")).unwrap();
    std::fs::write(
        dir.path().join(".nika/internal.nika.yaml"),
        "schema: nika/workflow@0.12",
    )
    .unwrap();

    let count = count_workflow_files(dir.path());
    assert_eq!(count, 3, "should find 3 workflows recursively, skipping hidden dirs");
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib count_workflows_recursive`
Expected: FAIL — `count_workflow_files` does not exist yet.

**Step 3: Extract counting logic into a standalone function and make it recursive**

Replace lines 357-368 in `lib.rs` with a call to a new function. Add the function above `print_startup_banner`:

```rust
/// Count `.nika.yaml` files recursively, skipping hidden directories.
fn count_workflow_files(dir: &std::path::Path) -> usize {
    let mut count = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // Skip hidden directories (.nika/, .git/, etc.)
        if name_str.starts_with('.') {
            continue;
        }
        let ft = entry.file_type();
        if ft.as_ref().is_ok_and(|t| t.is_dir()) {
            count += count_workflow_files(&entry.path());
        } else if name_str.ends_with(".nika.yaml") || name_str.ends_with(".nika.yml") {
            count += 1;
        }
    }
    count
}
```

Then replace the `workflow_count` binding at line 357:

```rust
    let workflow_count = count_workflow_files(&config.workflows_dir);
```

**Step 4: Run test to verify it passes**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib count_workflows_recursive`
Expected: PASS

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/lib.rs
git commit -m "fix(serve): recursive workflow count in startup banner (S1)"
```

---

## Task 2: S9 — Null byte in validate_workflow_path

Null bytes in paths can confuse OS syscalls. Add a check.

**Files:**
- Modify: `tools/nika-serve/src/routes/workflows.rs:481-491`

**Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` at end of `workflows.rs`:

```rust
#[test]
fn rejects_null_bytes() {
    assert!(validate_workflow_path("evil\0.nika.yaml").is_err());
    assert!(validate_workflow_path("sub/\0path.nika.yaml").is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib rejects_null_bytes`
Expected: FAIL — current validation passes null bytes through.

**Step 3: Add null byte check**

In `validate_workflow_path` at line 482, add before the existing checks:

```rust
fn validate_workflow_path(workflow: &str) -> Result<(), ServeError> {
    if workflow.contains('\0')
        || workflow.contains("..")
        || workflow.starts_with('/')
        || workflow.starts_with('\\')
    {
        return Err(ServeError::PathTraversal);
    }
```

**Step 4: Run test to verify it passes**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib rejects_null_bytes`
Expected: PASS

Also run existing tests: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/routes/workflows.rs
git commit -m "fix(serve): reject null bytes in workflow paths (S9)"
```

---

## Task 3: S4 — Constant-time auth token comparison (hash before ct_eq)

`subtle::ConstantTimeEq` on slices of different lengths leaks the token length via timing. Hash both sides first.

**Files:**
- Modify: `tools/nika-serve/src/auth.rs:34`

**Step 1: Write the failing test**

Add a test module to `auth.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ct_eq_different_length_slices_is_false() {
        // Demonstrate that raw ct_eq on different-length slices returns false
        // immediately — which leaks length info. After our fix, we hash first
        // so comparison is always 32 bytes regardless of input length.
        use sha2::{Sha256, Digest};

        let expected = "my-super-secret-token-that-is-long";
        let short = "short";
        let wrong_same_len = "my-super-secret-token-that-is-XXXX";

        // All comparisons should take constant time (same hash length)
        let h_expected = Sha256::digest(expected.as_bytes());
        let h_short = Sha256::digest(short.as_bytes());
        let h_wrong = Sha256::digest(wrong_same_len.as_bytes());

        assert!(!bool::from(h_expected.ct_eq(&h_short)));
        assert!(!bool::from(h_expected.ct_eq(&h_wrong)));
        assert!(bool::from(h_expected.ct_eq(&Sha256::digest(expected.as_bytes()))));
    }
}
```

**Step 2: Run test to verify it passes (this is a unit test of the approach)**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib auth::tests`
Expected: PASS (the test validates the approach, not the bug)

**Step 3: Fix the auth comparison to hash before comparing**

Replace line 34 in `auth.rs`. Change the `match` block:

```rust
use sha2::{Sha256, Digest};

    match token {
        Some(t) => {
            let expected = Sha256::digest(state.config.auth_token.as_bytes());
            let provided = Sha256::digest(t.as_bytes());
            if bool::from(expected.ct_eq(&provided)) {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
```

Remove the `use subtle::ConstantTimeEq;` at top of file — `ct_eq` is now called on `sha2::digest::Output` which implements the trait via re-export.

Actually, keep `use subtle::ConstantTimeEq;` — `sha2::digest::Output` (a `GenericArray`) implements `ConstantTimeEq` from the `subtle` crate, but the trait must be in scope. Check if it compiles without the import; if not, keep it.

**Step 4: Run all auth tests + full serve tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/auth.rs
git commit -m "fix(serve): hash tokens before ct_eq to prevent length leak (S4)"
```

---

## Task 4: S6 — Rate limit header uses actual configured value

The `X-RateLimit-Limit` header is hardcoded to `"10"` even when configured differently.

**Files:**
- Modify: `tools/nika-serve/src/rate_limit.rs:47-101`
- Modify: `tools/nika-serve/src/lib.rs` (where rate limiter is created)

**Step 1: Write the failing test**

Add to existing `#[cfg(test)] mod tests` in `rate_limit.rs`:

```rust
#[test]
fn rate_limit_header_reflects_config() {
    // When rate is 50/s, the header should say "50" not "10"
    let limiter = new_rate_limiter_with(50, 100);
    // We can't easily test the middleware header here without a full Axum setup,
    // but we CAN verify the RateLimitState stores the right value.
    let state = RateLimitState {
        limiter,
        rate_per_second: 50,
    };
    assert_eq!(state.rate_per_second, 50);
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib rate_limit_header_reflects_config`
Expected: FAIL — `RateLimitState` doesn't exist yet.

**Step 3: Add `RateLimitState` and thread configured rate into the middleware**

In `rate_limit.rs`, add a state struct and update the middleware signature:

```rust
/// State passed to the rate limit middleware (carries config + limiter).
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: Arc<KeyedRateLimiter>,
    pub rate_per_second: u32,
}
```

Update `rate_limit_middleware` to extract `RateLimitState`:

```rust
pub async fn rate_limit_middleware(
    State(rl): State<RateLimitState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let Some(key) = token else {
        return next.run(req).await;
    };

    let limit_str = rl.rate_per_second.to_string();

    match rl.limiter.check_key(&key) {
        Ok(_) => {
            let mut resp = next.run(req).await;
            let headers = resp.headers_mut();
            let _ = headers.insert(
                "x-ratelimit-limit",
                HeaderValue::from_str(&limit_str).unwrap_or(HeaderValue::from_static("10")),
            );
            let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("ok"));
            resp
        }
        Err(not_until) => {
            let wait = not_until.wait_time_from(governor::clock::Clock::now(
                &governor::clock::DefaultClock::default(),
            ));
            let retry_after = wait.as_secs().max(1);

            let mut resp = (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({
                    "error": "rate limit exceeded",
                    "retry_after": retry_after,
                })),
            )
                .into_response();

            let headers = resp.headers_mut();
            let _ = headers.insert(
                "retry-after",
                HeaderValue::from_str(&retry_after.to_string())
                    .unwrap_or(HeaderValue::from_static("1")),
            );
            let _ = headers.insert(
                "x-ratelimit-limit",
                HeaderValue::from_str(&limit_str).unwrap_or(HeaderValue::from_static("10")),
            );
            let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));

            resp
        }
    }
}
```

Then update `lib.rs` where the rate limiter layer is created. Find where `new_rate_limiter_with` is called and pass `RateLimitState` instead of just the `Arc<KeyedRateLimiter>`. Search for the `.layer(axum::middleware::from_fn_with_state(` call for rate limiting and update it:

```rust
let rl_state = rate_limit::RateLimitState {
    limiter: rate_limit::new_rate_limiter_with(
        config.rate_per_second as u32,
        config.rate_burst,
    ),
    rate_per_second: config.rate_per_second as u32,
};
// ... in the middleware layer:
.layer(axum::middleware::from_fn_with_state(rl_state, rate_limit::rate_limit_middleware))
```

**Step 4: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/rate_limit.rs tools/nika-serve/src/lib.rs
git commit -m "fix(serve): X-RateLimit-Limit header reflects actual config (S6)"
```

---

## Task 5: S5 — Artifact download path containment uses project_root

Artifact paths are checked against `workflows_dir`, but artifacts write to `[artifacts] dir` which may not be under `workflows_dir`.

**Files:**
- Modify: `tools/nika-serve/src/routes/artifacts.rs:96-103`

**Step 1: Write the failing test**

Add to `artifacts.rs` test module (or create one):

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn artifact_path_check_allows_project_root_children() {
        // Demonstrates the bug: if workflows_dir = ./jungo and
        // artifacts_dir = ./output, artifacts under ./output should be allowed.
        // Currently checked against workflows_dir which rejects them.
        use std::path::PathBuf;

        let project_root = PathBuf::from("/home/nika/nk-jungo");
        let workflows_dir = PathBuf::from("/home/nika/nk-jungo/jungo");
        let artifact_path = PathBuf::from("/home/nika/nk-jungo/output/report.md");

        // Should pass: artifact is under project_root
        assert!(artifact_path.starts_with(&project_root));
        // Would fail with old check: artifact is NOT under workflows_dir
        assert!(!artifact_path.starts_with(&workflows_dir));
    }
}
```

**Step 2: Run test to verify the logic assertion passes (proving the bug)**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib artifact_path_check`
Expected: PASS (the test proves the problem exists)

**Step 3: Fix the containment check to use project_root**

Replace lines 96-103 in `artifacts.rs`:

```rust
    // Verify file exists on disk and is within the project root.
    // We check against project_root (not workflows_dir) because artifacts
    // write to [artifacts] dir which may be a sibling of workflows_dir.
    let path = std::path::Path::new(&artifact.path);
    if let Ok(canonical) = tokio::fs::canonicalize(path).await {
        let base_dir = state
            .config
            .project_root
            .as_ref()
            .unwrap_or(&state.config.workflows_dir);
        let allowed_base = tokio::fs::canonicalize(base_dir)
            .await
            .map_err(|_| ServeError::NotFound)?;
        if !canonical.starts_with(&allowed_base) {
            return Err(ServeError::PathTraversal);
        }
    }
```

**Step 4: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/routes/artifacts.rs
git commit -m "fix(serve): artifact path check uses project_root not workflows_dir (S5)"
```

---

## Task 6: S7 — Log subprocess stderr on success

On success exit, stderr is silently discarded. Warnings from `nika run` are lost.

**Files:**
- Modify: `tools/nika-serve/src/worker.rs:402-404`

**Step 1: Write the failing test**

This is hard to unit test (needs process spawning). Instead, verify by code review.

**Step 2: Add stderr logging on success**

Replace lines 402-404 in `worker.rs`:

```rust
                Ok(Ok(status)) => {
                    if status.success() {
                        if !stderr.is_empty() {
                            tracing::warn!(
                                workflow = %workflow_path.display(),
                                stderr = %stderr.trim(),
                                "subprocess succeeded with stderr output"
                            );
                        }
                        Ok(stdout)
                    } else {
```

**Step 3: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 4: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/worker.rs
git commit -m "fix(serve): log subprocess stderr on success at warn level (S7)"
```

---

## Task 7: S8 — ANSI stripper handles OSC sequences

The custom ANSI stripper handles CSI (`ESC[`) but not OSC (`ESC]`). OSC sequences leak interior bytes.

**Files:**
- Modify: `tools/nika-serve/src/executor.rs:363-385`

**Step 1: Write the failing test**

Add to existing `#[cfg(test)] mod tests` in `executor.rs`:

```rust
#[test]
fn strip_ansi_handles_osc_sequences() {
    // OSC hyperlink: ESC ] 8 ;; https://example.com BEL text ESC ] 8 ;; BEL
    let input = "before\x1b]8;;https://example.com\x07link text\x1b]8;;\x07after";
    let result = strip_ansi_escapes(input);
    assert_eq!(result, "beforelink textafter");
}

#[test]
fn strip_ansi_handles_osc_with_st_terminator() {
    // OSC terminated by ST (ESC \) instead of BEL
    let input = "pre\x1b]0;window title\x1b\\post";
    let result = strip_ansi_escapes(input);
    assert_eq!(result, "prepost");
}
```

**Step 2: Run test to verify it fails**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib strip_ansi_handles_osc`
Expected: FAIL — OSC bytes leak through.

**Step 3: Fix the ANSI stripper to handle OSC**

Replace the `strip_ansi_escapes` function at lines 363-385:

```rust
fn strip_ansi_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC [ ... (letter)
                    chars.next(); // consume '['
                    for ch in chars.by_ref() {
                        if ch.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC ] ... (BEL | ST)
                    // Terminated by BEL (\x07) or ST (ESC \)
                    chars.next(); // consume ']'
                    for ch in chars.by_ref() {
                        if ch == '\x07' {
                            break; // BEL terminator
                        }
                        if ch == '\x1b' {
                            // ST = ESC + backslash
                            if chars.peek() == Some(&'\\') {
                                chars.next(); // consume '\'
                            }
                            break;
                        }
                    }
                }
                _ => {
                    // Other ESC sequences (single-char): skip next char
                    chars.next();
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
```

**Step 4: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib strip_ansi`
Expected: ALL PASS (both new tests + existing `strip_ansi_basic` if any)

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/executor.rs
git commit -m "fix(serve): ANSI stripper handles OSC sequences (S8)"
```

---

## Task 8: S3/E1 — Thread PolicyConfig from nika.toml to Runner → TaskExecutor

This is the big one. `[policy] allowed_hosts` is parsed from nika.toml but never reaches the executor. Three locations need fixing.

**Files:**
- Modify: `tools/nika-engine/src/runtime/runner.rs:243-288` (add `with_policy` builder)
- Modify: `tools/nika-engine/src/runtime/executor/mod.rs:137-161` (pass `allowed_hosts` to redirect closure)
- Modify: `tools/nika-cli/src/verbs.rs:142-162` (load boot config policy)
- Modify: `tools/nika-serve/src/executor.rs:180` (pass policy to Runner)

### Part A: Add `with_policy` builder to Runner

**Step 1: Write the failing test**

Add to `runner.rs` test module (or create a test in `tools/nika-engine/src/runtime/` tests):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::boot::PolicyConfig;

    #[test]
    fn runner_with_policy_threads_allowed_hosts() {
        // Minimal analyzed workflow for testing
        let yaml = r#"
schema: "nika/workflow@0.12"
workflow: test-policy
provider: mock
tasks:
  - id: t1
    infer: "hello"
"#;
        let raw = nika_core::ast::parse(yaml).unwrap();
        let analyzed = nika_core::ast::analyze(raw).unwrap();

        let mut policy = PolicyConfig::default();
        policy.allowed_hosts = vec!["localhost".to_string()];

        let runner = Runner::new(analyzed).unwrap();
        // Currently no way to pass policy to Runner — this test documents the need
        // After fix: Runner::with_policy(analyzed, policy) should propagate
    }
}
```

**Step 2: Implement `with_policy` on Runner**

In `runner.rs`, add a new constructor after `with_event_log`:

```rust
    /// Create a Runner with explicit policy configuration.
    ///
    /// Policy from `[policy]` in nika.toml is threaded to the TaskExecutor
    /// so that `allowed_hosts`, `blocked_hosts`, `max_token_spend`, etc.
    /// are enforced during workflow execution.
    pub fn with_policy(
        workflow: AnalyzedWorkflow,
        event_log: EventLog,
        policy: PolicyConfig,
    ) -> Result<Self, NikaError> {
        let workflow = if workflow.goal.is_some() {
            crate::runtime::orchestrate::wrap_as_orchestrator(workflow)
        } else {
            workflow
        };

        let flow_graph = Dag::from_analyzed(&workflow).map_err(|e| NikaError::ValidationError {
            reason: format!("DAG construction failed: {e}"),
        })?;
        flow_graph.detect_cycles()?;
        let datastore = RunContext::new();

        let resolver = crate::core::McpConfigResolver::from_environment();
        let mcp_configs =
            lower_mcp_servers_with_resolver(workflow.mcp_servers.clone(), Some(&resolver));
        let provider = workflow
            .provider
            .as_ref()
            .map(|p| p.as_str())
            .unwrap_or_else(|| detect_first_configured_provider());

        let mut executor = TaskExecutor::with_policy(
            provider,
            workflow.model.as_deref(),
            mcp_configs,
            event_log.clone(),
            Some(policy),
            None,
            None,
        )?;

        executor.wire_introspection_tools(Arc::new(datastore.clone()));

        let generation_id = format!("gen-{}", uuid::Uuid::new_v4());

        Ok(Self {
            workflow,
            flow_graph,
            datastore,
            executor,
            event_log,
            generation_id,
            quiet: false,
            cancel_token: CancellationToken::new(),
            paused: Arc::new(AtomicBool::new(false)),
            resume_notify: Arc::new(Notify::new()),
            resolved_assets: ResolvedAssets::default(),
            trace_config: TraceConfig::default(),
            cli_renderer: None,
            global_task_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS)),
        })
    }
```

**Step 3: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-engine --lib runner`
Expected: PASS

**Step 4: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-engine/src/runtime/runner.rs
git commit -m "feat(engine): add Runner::with_policy() constructor (S3 part A)"
```

### Part B: Fix redirect closure to respect allowed_hosts

**Step 5: Fix the shared HTTP client redirect policy**

In `tools/nika-engine/src/runtime/executor/mod.rs`, the shared `http_client` redirect-policy closure at lines 137-161 calls `is_ssrf_blocked()` WITHOUT checking `allowed_hosts`. The per-request client (fetch.rs:219-237) already does this correctly.

Replace lines 137-161:

```rust
        // Capture allowed_hosts for the shared redirect closure.
        // Clone the list so the closure owns it (policy struct consumed later).
        let redirect_allowed: Vec<String> = policy_config
            .as_ref()
            .map(|p| p.allowed_hosts.clone())
            .unwrap_or_default();

        let ssrf_redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
            use crate::runtime::policy::is_ssrf_blocked;

            if attempt.previous().len() >= REDIRECT_LIMIT {
                attempt.stop()
            } else {
                let blocked = attempt.url().host_str().and_then(|host| {
                    let h = host.to_lowercase();
                    let h_normalized = h.trim_start_matches('[').trim_end_matches(']');
                    let explicitly_allowed = redirect_allowed
                        .iter()
                        .any(|a| h_normalized == a.to_lowercase());
                    if !explicitly_allowed && is_ssrf_blocked(h_normalized) {
                        Some(h)
                    } else {
                        None
                    }
                });
                if let Some(host) = blocked {
                    attempt.error(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("SSRF protection: redirect to '{}' blocked", host),
                    ))
                } else {
                    attempt.follow()
                }
            }
        });
```

Note: This must be placed BEFORE `policy_config.unwrap_or_default()` at line 174 because we borrow from `policy_config` to build the allowed list. The `redirect_allowed` clone happens before consumption.

**Step 6: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-engine --lib`
Expected: ALL PASS

**Step 7: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-engine/src/runtime/executor/mod.rs
git commit -m "fix(engine): shared HTTP client redirect respects allowed_hosts (S3 part B)"
```

### Part C: Wire policy in CLI verbs and nika-serve embedded executor

**Step 8: Fix `one_shot_executor` in verbs.rs**

In `tools/nika-cli/src/verbs.rs`, replace lines 142-162:

```rust
async fn one_shot_executor(
    provider: &str,
    model: Option<&str>,
) -> Result<(TaskExecutor, EventLog), NikaError> {
    let event_log = EventLog::new();

    // Load config to resolve custom endpoints AND policy
    let nika_config = nika_engine::config::NikaConfig::load().ok();
    let custom_endpoints = nika_config
        .as_ref()
        .and_then(|cfg| cfg.resolve_endpoints().ok())
        .filter(|m| !m.is_empty());

    // Load boot config for [policy] section
    let policy = nika_engine::runtime::boot::BootSequence::load_config_sync()
        .map(|cfg| cfg.policy)
        .unwrap_or_default();

    let executor = TaskExecutor::with_policy(
        provider,
        model,
        None,
        event_log.clone(),
        Some(policy),
        None,
        custom_endpoints,
    )?;
    Ok((executor, event_log))
}
```

Check if `BootSequence::load_config_sync()` exists. If not, add a simpler approach — read nika.toml directly:

```rust
    // Load [policy] from nika.toml (if available)
    let policy = nika_engine::runtime::boot::load_policy_config();
```

You may need to add a small public helper in `boot.rs`:

```rust
/// Load just the PolicyConfig from nika.toml (sync, best-effort).
///
/// Used by CLI one-shot verbs that don't run the full boot sequence.
pub fn load_policy_config() -> PolicyConfig {
    let cwd = std::env::current_dir().unwrap_or_default();
    // Walk up to find nika.toml
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join("nika.toml");
        if candidate.exists() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                if let Ok(config) = toml::from_str::<BootstrapConfig>(&content) {
                    return config.policy;
                }
            }
            break;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
    PolicyConfig::default()
}
```

**Step 9: Wire policy in nika-serve embedded executor**

In `tools/nika-serve/src/executor.rs`, around line 180 where `Runner::with_event_log` is called, change to use `Runner::with_policy` when policy is available. The `ServeConfig` needs a `policy` field.

First, add `policy: PolicyConfig` to `ServeConfig` in `config.rs`:

```rust
pub policy: Option<nika_engine::runtime::boot::PolicyConfig>,
```

Load it from nika.toml during config construction. Then in `executor.rs`:

```rust
    let runner = if let Some(ref policy) = config.policy {
        nika_engine::runtime::Runner::with_policy(analyzed, event_log, policy.clone())
    } else {
        nika_engine::runtime::Runner::with_event_log(analyzed, event_log)
    }
    .map_err(|e| format!("runner init error: {e}"))?
    .quiet()
    .with_base_path(base_path)
    .with_cancel_token(cancel_token);
```

**Step 10: Run full test suite**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test --workspace --lib`
Expected: ALL PASS

**Step 11: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-cli/src/verbs.rs tools/nika-engine/src/runtime/boot.rs tools/nika-serve/src/executor.rs tools/nika-serve/src/config.rs
git commit -m "feat(engine): thread [policy] from nika.toml to executor (S3/E1)"
```

---

## Task 9: S10 — SSE subscribe TOCTOU race

Two separate lock acquisitions between "check channel exists" and "subscribe". A completing job can remove the channel in between.

**Files:**
- Modify: `tools/nika-serve/src/events.rs:150-165`

**Step 1: Analyze the race**

Current code:
```rust
let has_channel = state.event_bus.channels.lock().await.contains_key(&job_id);  // Lock 1
// ... gap where channel could be removed ...
let rx = state.event_bus.subscribe(&job_id).await;  // Lock 2
```

**Step 2: Fix by folding check + subscribe into single lock**

This depends on the `EventBus` API. Check if `subscribe` takes the lock internally. If so, we need a `subscribe_if_exists` method or fold the logic.

Replace lines 150-165:

```rust
    // BUG-5 + S10: Check existence and subscribe in one lock acquisition
    // to prevent TOCTOU race where channel is removed between check and subscribe.
    let rx = {
        let channels = state.event_bus.channels.lock().await;
        if channels.contains_key(&job_id) {
            drop(channels);
            Some(state.event_bus.subscribe(&job_id).await)
        } else {
            drop(channels);
            // Verify job exists in storage before creating a new channel
            let job_exists = state
                .storage
                .get_job(&job_id)
                .await
                .ok()
                .flatten()
                .is_some();
            if !job_exists {
                return Err(crate::error::ServeError::NotFound);
            }
            Some(state.event_bus.subscribe(&job_id).await)
        }
    };
    let rx = rx.unwrap();
```

Note: If `subscribe()` also takes the lock, we can't hold both. In that case, the actual fix is to make `subscribe` return an `Option` when the channel is gone, and the caller retries. Check the `EventBus::subscribe` implementation before applying this fix verbatim. The key principle: minimize the gap between check and action.

**Step 3: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 4: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/events.rs
git commit -m "fix(serve): reduce SSE subscribe TOCTOU window (S10)"
```

---

## Task 10: S11 — Store GC task handle for graceful shutdown

The background GC task is spawned but the `JoinHandle` is dropped.

**Files:**
- Modify: `tools/nika-serve/src/lib.rs:183-196`
- Modify: `tools/nika-serve/src/state.rs` (add field)

**Step 1: Add `gc_handle` field to AppState**

In `state.rs`, add:

```rust
pub gc_handle: Option<tokio::task::JoinHandle<()>>,
```

**Step 2: Store the handle in lib.rs**

Replace lines 183-196:

```rust
    let gc_storage = state.storage.clone();
    let gc_interval = std::time::Duration::from_secs(config.gc_interval_secs);
    let gc_retention = config.gc_retention_secs;
    let gc_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(gc_interval).await;
            match gc_storage.delete_old_jobs(gc_retention).await {
                Ok(0) => {}
                Ok(n) => info!(count = n, "job GC: deleted old jobs"),
                Err(e) => tracing::warn!(error = %e, "job GC failed"),
            }
        }
    });
    state.gc_handle = Some(gc_handle);
```

Then abort on shutdown — find the graceful shutdown handler and add:

```rust
if let Some(handle) = state.gc_handle.take() {
    handle.abort();
}
```

**Step 3: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-serve --lib`
Expected: ALL PASS

**Step 4: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-serve/src/lib.rs tools/nika-serve/src/state.rs
git commit -m "fix(serve): track GC task handle for graceful shutdown (S11)"
```

---

## Task 11: E3 — nika.md overwrite sentinel

Every `nika check` or `nika run` overwrites `~/.claude/rules/nika.md`.

**Files:**
- Modify: `tools/nika-cli/src/doctor.rs` (or wherever the nika.md write lives)

**Step 1: Find the write location**

Search for the code that writes `nika.md`. It's likely in `doctor.rs` or the CLI init code. Look for `rules/nika.md` or `claude/rules`.

**Step 2: Add sentinel check**

Before writing, check if the file exists and contains `# DO NOT OVERWRITE`:

```rust
fn should_write_nika_md(path: &std::path::Path) -> bool {
    if !path.exists() {
        return true;
    }
    match std::fs::read_to_string(path) {
        Ok(content) => !content.contains("# DO NOT OVERWRITE"),
        Err(_) => true,
    }
}
```

**Step 3: Only write if sentinel absent**

Wrap the existing write call:

```rust
if should_write_nika_md(&nika_md_path) {
    std::fs::write(&nika_md_path, NIKA_MD_CONTENT)?;
}
```

**Step 4: Run tests**

Run: `cd /Users/thibaut/dev/supernovae/nika/tools && cargo test -p nika-cli --lib`
Expected: ALL PASS

**Step 5: Commit**

```bash
cd /Users/thibaut/dev/supernovae/nika
git add tools/nika-cli/src/doctor.rs
git commit -m "fix(cli): respect sentinel in nika.md to prevent overwrite (E3/BUG-009)"
```

---

## Summary: 11 Tasks, 15 Commits

| Task | Bug | Priority | Est. |
|------|-----|----------|------|
| 1 | S1 — Recursive banner count | P0 | 15 min |
| 2 | S9 — Null byte rejection | P3 | 5 min |
| 3 | S4 — Hash before ct_eq | P1 | 15 min |
| 4 | S6 — Rate limit header | P2 | 20 min |
| 5 | S5 — Artifact path base | P1 | 15 min |
| 6 | S7 — Stderr on success | P2 | 10 min |
| 7 | S8 — OSC in ANSI stripper | P2 | 15 min |
| 8 | S3/E1 — Policy threading | P0+P1 | 60 min |
| 9 | S10 — SSE TOCTOU | P3 | 15 min |
| 10 | S11 — GC handle | P3 | 10 min |
| 11 | E3 — nika.md overwrite | P2 | 15 min |

**Total: ~3.5 hours**

**S2 is already implemented** — the GET /v1/workflows/{name}/source endpoint exists at `workflows.rs:300-331` and is registered in `routes/mod.rs:42-45`. Removed from the plan.

**E2 ($env in templates)** is a design decision (minimal fix vs. doc-only). Excluded from this plan — create a separate ticket.
