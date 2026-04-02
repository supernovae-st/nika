# v0.59 Mega Hardening Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all CRITICAL/HIGH/MEDIUM findings from the 10-agent audit, harden nika serve for production, and clean up architecture debt before launch.

**Architecture:** Bottom-up fixes starting with security-critical webhook issues, then serve correctness (metrics, races), then vault hardening, then code quality. Each task is independent and committable.

**Tech Stack:** Rust, Axum, orion (crypto), metrics/metrics-exporter-prometheus, nika-storage (SQLite), tokio

**Findings source:** 10-agent parallel audit (2026-04-01) — 2 CRITICAL, 4 HIGH, 22 MEDIUM

---

## Batch 1: CRITICAL Security (2 tasks)

### Task 1: Webhook — refuse delivery without secret

**Files:**
- Modify: `tools/nika-serve/src/webhook.rs:26-39`
- Test: `tools/nika-serve/src/webhook.rs` (test module)

**Step 1: Write the failing test**

```rust
#[test]
fn webhook_config_none_when_secret_missing() {
    std::env::set_var("NIKA_WEBHOOK_URL", "https://hooks.example.com/test");
    std::env::remove_var("NIKA_WEBHOOK_SECRET");
    let config = WebhookConfig::from_env();
    assert!(config.is_none(), "must refuse webhook without secret");
    std::env::remove_var("NIKA_WEBHOOK_URL");
}
```

**Step 2: Run test to verify it fails**

Run: `cd tools && cargo test -p nika-serve webhook_config_none_when_secret_missing --lib -- --exact 2>&1 | tail -5`
Expected: FAIL (currently returns Some with empty secret)

**Step 3: Write minimal implementation**

In `webhook.rs`, replace lines 35-38:

```rust
// OLD:
let secret = std::env::var("NIKA_WEBHOOK_SECRET").unwrap_or_default();
if secret.is_empty() {
    warn!("NIKA_WEBHOOK_URL is set but NIKA_WEBHOOK_SECRET is empty — signatures will be weak");
}

// NEW:
let secret = match std::env::var("NIKA_WEBHOOK_SECRET") {
    Ok(s) if !s.is_empty() => s,
    _ => {
        warn!(
            "NIKA_WEBHOOK_URL is set but NIKA_WEBHOOK_SECRET is missing or empty — \
             webhook delivery disabled. Set NIKA_WEBHOOK_SECRET to enable."
        );
        return None;
    }
};
```

**Step 4: Run test to verify it passes**

Run: `cd tools && cargo test -p nika-serve webhook --lib 2>&1 | tail -10`
Expected: ALL webhook tests PASS

**Step 5: Commit**

```bash
git add tools/nika-serve/src/webhook.rs
git commit -m "fix(serve): refuse webhook delivery without NIKA_WEBHOOK_SECRET"
```

---

### Task 2: Webhook — add replay protection (Stripe pattern)

**Files:**
- Modify: `tools/nika-serve/src/webhook.rs:44-97`
- Test: `tools/nika-serve/src/webhook.rs` (test module)

**Step 1: Write the failing tests**

```rust
#[test]
fn sign_v2_includes_timestamp() {
    let sig = sign_v2("my-secret", b"hello world", 1711929600);
    assert!(sig.starts_with("t=1711929600,v1="), "must include timestamp prefix");
}

#[test]
fn sign_v2_deterministic_with_same_timestamp() {
    let a = sign_v2("key", b"data", 1000);
    let b = sign_v2("key", b"data", 1000);
    assert_eq!(a, b);
}

#[test]
fn sign_v2_different_timestamps_produce_different_sigs() {
    let a = sign_v2("key", b"data", 1000);
    let b = sign_v2("key", b"data", 2000);
    assert_ne!(a, b);
}

#[test]
fn verify_valid_signature() {
    let ts = 1711929600u64;
    let body = b"hello world";
    let sig = sign_v2("secret", body, ts);
    assert!(verify("secret", body, &sig, ts + 10, 300));
}

#[test]
fn verify_rejects_expired_timestamp() {
    let ts = 1711929600u64;
    let body = b"hello world";
    let sig = sign_v2("secret", body, ts);
    // 600 seconds later — outside 300s tolerance
    assert!(!verify("secret", body, &sig, ts + 600, 300));
}

#[test]
fn verify_rejects_wrong_signature() {
    assert!(!verify("secret", b"body", "t=1000,v1=deadbeef", 1005, 300));
}
```

**Step 2: Run tests to verify they fail**

Run: `cd tools && cargo test -p nika-serve sign_v2 --lib 2>&1 | tail -5`
Expected: FAIL (function not defined)

**Step 3: Write minimal implementation**

Replace the `sign()` function and add `sign_v2()` + `verify()`:

```rust
/// Compute HMAC-SHA256 signature with timestamp (Stripe-style replay protection).
///
/// Format: `t=TIMESTAMP,v1=HEXDIGEST`
/// Signed payload: `"{timestamp}.{body}"` (dot-separated, prevents length-extension).
pub fn sign_v2(secret: &str, body: &[u8], timestamp: u64) -> String {
    let mut signed_payload = format!("{timestamp}.").into_bytes();
    signed_payload.extend_from_slice(body);

    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(&signed_payload);
    let result = mac.finalize();
    let hex: String = result.into_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("t={timestamp},v1={hex}")
}

/// Verify a webhook signature, rejecting replays older than `tolerance_secs`.
pub fn verify(secret: &str, body: &[u8], signature: &str, now: u64, tolerance_secs: u64) -> bool {
    // Parse "t=TIMESTAMP,v1=DIGEST"
    let parts: Vec<&str> = signature.splitn(2, ',').collect();
    let (ts_part, sig_part) = match parts.as_slice() {
        [t, s] => (*t, *s),
        _ => return false,
    };
    let ts: u64 = match ts_part.strip_prefix("t=").and_then(|s| s.parse().ok()) {
        Some(t) => t,
        None => return false,
    };

    // Replay check
    if now.abs_diff(ts) > tolerance_secs {
        return false;
    }

    // Recompute and compare (constant-time via HMAC verify)
    let expected = sign_v2(secret, body, ts);
    // Use subtle for constant-time comparison
    use subtle::ConstantTimeEq;
    expected.as_bytes().ct_eq(signature.as_bytes()).into()
}
```

Update `notify()` to use `sign_v2()` with current timestamp:

```rust
pub fn notify(config: &WebhookConfig, job_id: &str, status: &str, output: Option<&str>) {
    let url = config.url.clone();
    let secret = config.secret.clone();
    let body = serde_json::json!({
        "job_id": job_id,
        "status": status,
        "output": output,
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

    tokio::spawn(async move {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let signature = sign_v2(&secret, &body_bytes, timestamp);

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        match client
            .post(&url)
            .header("content-type", "application/json")
            .header("x-nika-signature", &signature)
            .body(body_bytes)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                debug!(url = %url, status = resp.status().as_u16(), "webhook delivered");
            }
            Err(e) => {
                warn!(url = %url, error = %e, "webhook delivery failed");
            }
        }
    });
}
```

Add `subtle` to nika-serve Cargo.toml (already a workspace dep).

Delete the old `sign()` function entirely — zero users, zero backward compat.

**Step 4: Run tests to verify they pass**

Run: `cd tools && cargo test -p nika-serve webhook --lib 2>&1 | tail -15`
Expected: ALL webhook tests PASS

**Step 5: Commit**

```bash
git add tools/nika-serve/src/webhook.rs tools/nika-serve/Cargo.toml
git commit -m "feat(serve): webhook replay protection — Stripe-style t=TS,v1=SIG"
```

---

## Batch 2: HIGH — Serve Correctness (3 tasks)

### Task 3: Wire dead Prometheus metrics

**Files:**
- Modify: `tools/nika-serve/src/metrics.rs` (add middleware fn)
- Modify: `tools/nika-serve/src/lib.rs:~130` (add middleware layer)
- Modify: `tools/nika-serve/src/routes/workflows.rs:~115` (call record_active_jobs)
- Modify: `tools/nika-serve/src/worker.rs:~116,~164` (call record_active_jobs on decrement)
- Test: `tools/nika-serve/src/metrics.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn http_metrics_middleware_is_defined() {
    // Ensure the function exists and has the right signature
    let _: fn(
        axum::extract::Request,
        axum::middleware::Next,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = axum::response::Response> + Send>>
    = |req, next| Box::pin(http_metrics_middleware(req, next));
    // Type check is the test
}
```

Actually, better approach — add the middleware directly and test the wiring:

**Step 1: Add http_metrics_middleware to metrics.rs**

After `record_http_request()`, add:

```rust
/// Axum middleware that records HTTP request metrics (method, path, status).
pub async fn http_metrics_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    record_http_request(&method, &path, response.status().as_u16());
    response
}
```

**Step 2: Wire record_active_jobs in worker.rs**

After each `fetch_sub(1, ...)` call, add:
```rust
crate::metrics::record_active_jobs(self.active_jobs.load(Ordering::Relaxed));
```

Two locations:
- `WorkerGuard::drop()` (~line 116, after the fetch_sub)
- Shutdown path (~line 164, after the fetch_sub)

**Step 3: Wire record_active_jobs in workflows.rs**

After `try_acquire_job_slot()` succeeds (~line 115), add:
```rust
crate::metrics::record_active_jobs(state.active_jobs.load(std::sync::atomic::Ordering::Relaxed));
```

**Step 4: Wire http_metrics_middleware in lib.rs**

Add the middleware layer to `api_routes` and `sse_routes` — add INSIDE the layer chain, before auth (so it measures all responses including 401s):

```rust
.layer(middleware::from_fn(crate::metrics::http_metrics_middleware))
```

Add it right after the `request_id_middleware` layer (outermost = runs first).

**Step 5: Run tests**

Run: `cd tools && cargo test -p nika-serve --lib 2>&1 | tail -10`
Expected: PASS

**Step 6: Commit**

```bash
git add tools/nika-serve/src/metrics.rs tools/nika-serve/src/lib.rs tools/nika-serve/src/worker.rs tools/nika-serve/src/routes/workflows.rs
git commit -m "fix(serve): wire dead Prometheus metrics — active_jobs gauge + HTTP request counter"
```

---

### Task 4: Fix cancel/complete race condition

**Files:**
- Modify: `tools/nika-storage/src/lib.rs` (SQL WHERE clause)
- Test: `tools/nika-storage/src/lib.rs` (test module)

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn complete_job_does_not_overwrite_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db");
    let storage = Storage::open(&db).unwrap();

    let id = storage.create_job("test.nika.yaml", None).await.unwrap();
    storage.update_state(&id, JobState::Running, None, None).await.unwrap();

    // Cancel the job first
    storage.update_state(&id, JobState::Cancelled, None, Some("cancelled by API".into())).await.unwrap();

    // Now try to complete it (simulates the race)
    storage.complete_job(&id, "late result").await.unwrap();

    // Should still be Cancelled
    let job = storage.get_job(&id).await.unwrap().unwrap();
    assert_eq!(job.state, JobState::Cancelled, "complete must not overwrite cancelled");
    assert_eq!(job.output.as_deref(), Some("cancelled by API"), "output must not change");
}
```

**Step 2: Run test to verify it fails**

Run: `cd tools && cargo test -p nika-storage complete_job_does_not_overwrite --lib 2>&1 | tail -10`
Expected: FAIL (currently overwrites Cancelled with Completed)

**Step 3: Write minimal implementation**

In `tools/nika-storage/src/lib.rs`, find the `do_update_state` function. In the `JobState::Completed | JobState::Failed | JobState::Cancelled` match arm, change the SQL:

```rust
// OLD:
"UPDATE jobs SET state = ?1, completed_at = ?2, exit_code = ?3, output = ?4 WHERE id = ?5"

// NEW:
"UPDATE jobs SET state = ?1, completed_at = ?2, exit_code = ?3, output = ?4 WHERE id = ?5 AND state IN ('pending', 'running')"
```

This ensures only valid state transitions happen. A job that's already completed/failed/cancelled won't be overwritten.

**Step 4: Run tests**

Run: `cd tools && cargo test -p nika-storage --lib 2>&1 | tail -10`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add tools/nika-storage/src/lib.rs
git commit -m "fix(serve): prevent cancel/complete race — SQL state guard on transitions"
```

---

### Task 5: Remove blanket #[allow(dead_code)] on NativeRuntime

**Files:**
- Modify: `tools/nika-engine/src/provider/native/runtime.rs:~101`

**Step 1: Read the struct and identify truly dead fields**

Read `runtime.rs` and identify which fields are used outside `#[cfg(feature = "native-inference")]` blocks.

**Step 2: Gate dead fields behind the feature**

```rust
// BEFORE:
#[allow(dead_code)]
pub struct NativeRuntime {
    model_info: ModelInfo,
    model_path: PathBuf,
    config: NativeConfig,
    is_vision: bool,
    #[cfg(feature = "native-inference")]
    model: Arc<...>,
}

// AFTER:
pub struct NativeRuntime {
    #[cfg(feature = "native-inference")]
    model_info: ModelInfo,
    #[cfg(feature = "native-inference")]
    model_path: PathBuf,
    config: NativeConfig,  // keep if used in non-feature path
    #[cfg(feature = "native-inference")]
    is_vision: bool,
    #[cfg(feature = "native-inference")]
    model: Arc<...>,
}
```

If fields are ONLY used in `#[cfg(feature = "native-inference")]` methods, gate them. If a field is used in a non-feature method, keep it ungated.

**Step 3: Run tests**

Run: `cd tools && cargo test -p nika-engine --lib 2>&1 | tail -10`
Expected: PASS (no warnings about dead code)

Also verify without the feature:
Run: `cd tools && cargo check -p nika-engine --no-default-features 2>&1 | grep -i "warning.*dead_code" | head -5`
Expected: No dead_code warnings

**Step 4: Commit**

```bash
git add tools/nika-engine/src/provider/native/runtime.rs
git commit -m "fix(engine): remove blanket #[allow(dead_code)] on NativeRuntime — gate fields behind feature"
```

---

## Batch 3: MEDIUM — Crypto/Vault Hardening (4 tasks)

### Task 6: Bump Argon2i iterations from 3 to 6

**Files:**
- Modify: `tools/nika-core/src/vault.rs:455`
- Test: `tools/nika-core/src/vault.rs` (existing tests)

**Step 1: Change the iteration count**

```rust
// OLD (line 455):
let derived = kdf::derive_key(&password, &kdf_salt, 3, 1 << 16, 32)

// NEW:
let derived = kdf::derive_key(&password, &kdf_salt, 6, 1 << 16, 32)
```

**Step 2: Run vault tests**

Run: `cd tools && cargo test -p nika-core vault --lib 2>&1 | tail -10`
Expected: PASS (new vaults created with iter=6; old vaults fail to decrypt — acceptable since we have zero users)

**Step 3: Commit**

```bash
git add tools/nika-core/src/vault.rs
git commit -m "fix(vault): bump Argon2i iterations from 3 to 6 — stronger KDF"
```

---

### Task 7: Set secrets directory to 0o700

**Files:**
- Modify: `tools/nika-core/src/vault.rs:422-424` and `:470-471`

**Step 1: Write the failing test**

```rust
#[cfg(unix)]
#[test]
fn vault_secrets_dir_is_700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let secrets_dir = dir.path().join("secrets");
    let vault_path = secrets_dir.join("vault.enc");
    let salt_path = secrets_dir.join("vault.salt");

    let vault = NikaVault::new(vault_path, salt_path);
    vault.set("test", "secret123").unwrap();

    let mode = std::fs::metadata(&secrets_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "secrets dir must be 0o700, got {mode:o}");
}
```

**Step 2: Run test to verify it fails**

Expected: FAIL (currently 0o755 from default umask)

**Step 3: Implement fix**

After each `std::fs::create_dir_all(parent)?;` in `write_payload()` and `load_or_create_salt()`, add:

```rust
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
}
```

**Step 4: Run tests**

Run: `cd tools && cargo test -p nika-core vault --lib 2>&1 | tail -10`
Expected: PASS

**Step 5: Commit**

```bash
git add tools/nika-core/src/vault.rs
git commit -m "fix(vault): set ~/.nika/secrets/ to 0o700 — prevent directory listing by other users"
```

---

### Task 8: Add file locking on vault read-modify-write

**Files:**
- Modify: `tools/nika-core/src/vault.rs` (set/delete methods)
- Modify: `tools/nika-core/Cargo.toml` (add fs2 dep)

**Step 1: Add fs2 to workspace deps**

In `tools/Cargo.toml` (workspace), add:
```toml
fs2 = "0.4"
```

In `tools/nika-core/Cargo.toml`, add:
```toml
fs2 = { workspace = true }
```

**Step 2: Write failing test**

```rust
#[test]
fn vault_concurrent_writes_dont_lose_data() {
    let dir = tempfile::tempdir().unwrap();
    let vault_path = dir.path().join("vault.enc");
    let salt_path = dir.path().join("vault.salt");

    let vault = NikaVault::new(vault_path.clone(), salt_path.clone());

    // Write 10 providers concurrently via threads
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let v = NikaVault::new(vault_path.clone(), salt_path.clone());
            std::thread::spawn(move || {
                v.set(&format!("provider_{i}"), &format!("key_{i}")).unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }

    // All 10 should be present
    let all = vault.list().unwrap();
    assert_eq!(all.len(), 10, "all concurrent writes must be preserved");
}
```

**Step 3: Implement file locking**

In `set()` and `delete()` methods, wrap the read-modify-write in a file lock:

```rust
pub fn set(&self, provider: &str, secret: &str) -> Result<(), VaultError> {
    use fs2::FileExt;

    // Ensure parent dir exists
    if let Some(parent) = self.vault_path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }

    // Lock file for exclusive access
    let lock_path = self.vault_path.with_extension("lock");
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&lock_path)?;
    lock_file.lock_exclusive()?;

    let result = (|| {
        let mut payload = self.read_payload()?.unwrap_or_default();
        payload.version = 2;
        payload.secrets.insert(provider.to_string(), VaultEntry::Key(secret.to_string()));
        self.write_payload(&payload)
    })();

    lock_file.unlock()?;
    result
}
```

Apply same pattern to `delete()`, `set_credential()`, and `delete_field()`.

**Step 4: Run tests**

Run: `cd tools && cargo test -p nika-core vault --lib 2>&1 | tail -10`
Expected: PASS (including concurrent write test)

**Step 5: Commit**

```bash
git add tools/Cargo.toml tools/nika-core/Cargo.toml tools/nika-core/src/vault.rs
git commit -m "fix(vault): add file locking on read-modify-write — prevent concurrent data loss"
```

---

### Task 9: Warn on weak passphrase

**Files:**
- Modify: `tools/nika-core/src/vault.rs:489-498`

**Step 1: Add warning for short passphrase**

In `machine_fingerprint()`, after the non-empty check:

```rust
fn machine_fingerprint() -> Result<String, VaultError> {
    if let Ok(pass) = std::env::var("NIKA_VAULT_PASSPHRASE") {
        if !pass.is_empty() {
            if pass.len() < 12 {
                tracing::warn!(
                    "NIKA_VAULT_PASSPHRASE is short ({} chars) — recommend 12+ chars for security",
                    pass.len()
                );
            }
            return Ok(format!("nika-vault-v1:passphrase:{pass}"));
        }
    }
    let machine_id = get_machine_id()?;
    let username = whoami::username();
    Ok(format!("nika-vault-v1:{machine_id}:{username}"))
}
```

**Step 2: Run tests**

Run: `cd tools && cargo test -p nika-core vault --lib 2>&1 | tail -5`
Expected: PASS

**Step 3: Commit**

```bash
git add tools/nika-core/src/vault.rs
git commit -m "fix(vault): warn on short NIKA_VAULT_PASSPHRASE (< 12 chars)"
```

---

## Batch 4: MEDIUM — Serve Hardening (5 tasks)

### Task 10: Token minimum 16 → 32 chars

**Files:**
- Modify: `tools/nika-serve/src/config.rs:115-118`

**Step 1: Change minimum**

```rust
// OLD:
if auth_token.len() < 16 {
    return Err(ServeError::Config(
        "NIKA_SERVE_TOKEN must be at least 16 characters".into(),
    ));
}

// NEW:
if auth_token.len() < 32 {
    return Err(ServeError::Config(
        "NIKA_SERVE_TOKEN must be at least 32 characters. \
         Generate one with: openssl rand -hex 32".into(),
    ));
}
```

**Step 2: Run tests** (existing test may need update)

Run: `cd tools && cargo test -p nika-serve config --lib 2>&1 | tail -10`

**Step 3: Commit**

```bash
git add tools/nika-serve/src/config.rs
git commit -m "fix(serve): raise NIKA_SERVE_TOKEN minimum from 16 to 32 chars"
```

---

### Task 11: Cap X-Request-Id length to 128 chars

**Files:**
- Modify: `tools/nika-serve/src/request_id.rs:14-31`
- Test: `tools/nika-serve/src/request_id.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn long_request_id_is_truncated() {
    let long_id = "x".repeat(256);
    let truncated = sanitize_request_id(&long_id);
    assert!(truncated.len() <= 128, "must truncate to 128 chars max");
}
```

**Step 2: Implement**

```rust
const MAX_REQUEST_ID_LEN: usize = 128;

fn sanitize_request_id(id: &str) -> &str {
    if id.len() <= MAX_REQUEST_ID_LEN {
        id
    } else {
        &id[..MAX_REQUEST_ID_LEN]
    }
}

pub async fn request_id_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| sanitize_request_id(s).to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());

    let mut resp = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(&id) {
        resp.headers_mut().insert(REQUEST_ID_HEADER, val);
    }
    resp
}
```

**Step 3: Run tests and commit**

```bash
git add tools/nika-serve/src/request_id.rs
git commit -m "fix(serve): cap X-Request-Id to 128 chars — prevent log poisoning"
```

---

### Task 12: Input value size limit (64 KB)

**Files:**
- Modify: `tools/nika-serve/src/routes/workflows.rs` (input validation)

**Step 1: Add per-value size check**

In the `run_workflow()` handler, after key validation, add:

```rust
const MAX_INPUT_VALUE_BYTES: usize = 64 * 1024; // 64 KB

for (key, value) in &inputs {
    if let Some(s) = value.as_str() {
        if s.len() > MAX_INPUT_VALUE_BYTES {
            return Err(ServeError::Validation(format!(
                "input '{key}' exceeds {MAX_INPUT_VALUE_BYTES} byte limit ({} bytes)",
                s.len()
            )));
        }
    }
}
```

**Step 2: Test + Commit**

```bash
git commit -m "fix(serve): cap input values at 64 KB — prevent memory abuse"
```

---

### Task 13: Fix .expect() on CORS origin

**Files:**
- Modify: `tools/nika-serve/src/lib.rs:~145`

**Step 1: Find and fix**

Replace the `.expect("invalid CORS origin")` with proper error handling:

```rust
// OLD:
let cors = CorsLayer::new()
    .allow_origin(origin.parse::<HeaderValue>().expect("invalid CORS origin"));

// NEW:
let origin_header = origin.parse::<HeaderValue>()
    .map_err(|e| ServeError::Config(format!("invalid CORS origin '{origin}': {e}")))?;
let cors = CorsLayer::new().allow_origin(origin_header);
```

**Step 2: Test + Commit**

```bash
git commit -m "fix(serve): handle invalid CORS origin gracefully — no panic on bad config"
```

---

### Task 14: Make rate limit + GC configurable via env vars

**Files:**
- Modify: `tools/nika-serve/src/rate_limit.rs:26-27`
- Modify: `tools/nika-serve/src/lib.rs:162-175` (GC constants)
- Modify: `tools/nika-serve/src/config.rs` (add fields)

**Step 1: Add fields to ServeConfig**

```rust
pub struct ServeConfig {
    // ... existing fields ...
    /// Rate limit: requests per second (default: 10)
    pub rate_per_second: u64,
    /// Rate limit: burst size (default: 30)
    pub rate_burst: u32,
    /// Job GC: retention in seconds (default: 7 days)
    pub gc_retention_secs: u64,
    /// Job GC: check interval in seconds (default: 3600)
    pub gc_interval_secs: u64,
}
```

Parse from env in `from_env()`:

```rust
let rate_per_second = std::env::var("NIKA_SERVE_RATE_LIMIT")
    .ok().and_then(|s| s.parse().ok()).unwrap_or(10);
let rate_burst = std::env::var("NIKA_SERVE_RATE_BURST")
    .ok().and_then(|s| s.parse().ok()).unwrap_or(30);
let gc_retention_secs = std::env::var("NIKA_SERVE_GC_RETENTION")
    .ok().and_then(|s| s.parse().ok()).unwrap_or(7 * 24 * 3600);
let gc_interval_secs = std::env::var("NIKA_SERVE_GC_INTERVAL")
    .ok().and_then(|s| s.parse().ok()).unwrap_or(3600);
```

Update rate_limit.rs `new_rate_limiter()` to accept config params, and lib.rs GC loop to use config values.

**Step 2: Test + Commit**

```bash
git commit -m "feat(serve): make rate limit + job GC configurable via env vars"
```

---

## Batch 5: MEDIUM — Provider/vLLM (3 tasks)

### Task 15: Extract token usage from raw_openai_compat_infer

**Files:**
- Modify: `tools/nika-engine/src/provider/rig/mod.rs:505-513`

**Step 1: Extract usage from JSON response**

In `raw_openai_compat_infer()`, after extracting content, also extract usage:

```rust
let prompt_tokens = json.pointer("/usage/prompt_tokens")
    .and_then(|v| v.as_u64())
    .unwrap_or(0);
let completion_tokens = json.pointer("/usage/completion_tokens")
    .and_then(|v| v.as_u64())
    .unwrap_or(0);
```

Return these in the CompletionResponse or update the RunStats directly.

**Step 2: Test + Commit**

```bash
git commit -m "fix(provider): extract token usage from raw OpenAiCompat response"
```

---

### Task 16: Fix infer_stream_with_options timeout for OpenAiCompat

**Files:**
- Modify: `tools/nika-engine/src/provider/rig/mod.rs:~1584`

**Step 1: Fix timeout**

Replace the hardcoded `STREAM_TOTAL_TIMEOUT` with endpoint-specific timeout for OpenAiCompat:

```rust
// OLD:
let total_timeout = Duration::from_secs(STREAM_TOTAL_TIMEOUT);

// NEW (matching the infer_stream pattern at line 1307-1312):
let total_timeout = match self {
    RigProvider::OpenAiCompat { timeout_secs, .. } => {
        Duration::from_secs((*timeout_secs).max(60) * 2)
    }
    _ => Duration::from_secs(STREAM_TOTAL_TIMEOUT),
};
```

**Step 2: Test + Commit**

```bash
git commit -m "fix(provider): use endpoint timeout_secs for OpenAiCompat streaming"
```

---

### Task 17: Reset MOCK_CALL_COUNTER per workflow run

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs:319`

**Step 1: Make counter per-Runner instead of global**

Move `MOCK_CALL_COUNTER` from `static` to a field on `TaskExecutor` or pass it via the mock provider context. Simplest fix: reset the counter at the start of each `Runner::run()`:

```rust
// In runner.rs, at the start of run():
#[cfg(test)]
{
    use std::sync::atomic::Ordering;
    crate::runtime::executor::infer::MOCK_CALL_COUNTER.store(0, Ordering::SeqCst);
}
```

But for serve embedded mode, reset it per workflow invocation (not just tests). Better: make it an `Arc<AtomicU32>` field on `RunContext`.

**Step 2: Test + Commit**

```bash
git commit -m "fix(engine): reset MOCK_CALL_COUNTER per workflow run — fix serve embedded mode"
```

---

## Batch 6: MEDIUM — Daemon (2 tasks)

### Task 18: Spawn cache reaper task

**Files:**
- Modify: `tools/nika-daemon/src/server.rs` (in `run()` method)

**Step 1: Add periodic reaper**

In `DaemonServer::run()`, after cron scheduler spawn, add:

```rust
// Periodic cache cleanup (every 5 minutes)
let cache_for_reaper = cache_service.clone();
tokio::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
    interval.tick().await; // skip first tick
    loop {
        interval.tick().await;
        let removed = cache_for_reaper.cleanup_expired();
        if removed > 0 {
            tracing::debug!(removed, "cache reaper cleaned expired entries");
        }
    }
});
```

**Step 2: Test + Commit**

```bash
git commit -m "fix(daemon): spawn cache reaper task — cleanup expired entries every 5 min"
```

---

### Task 19: Wire vault audit log into production code

**Files:**
- Modify: `tools/nika-core/src/vault.rs` (NikaVault struct)
- Modify: `tools/nika-engine/src/secrets/fallback.rs`

**Step 1: Add audit field to NikaVault**

```rust
pub struct NikaVault {
    vault_path: PathBuf,
    salt_path: PathBuf,
    audit: VaultAuditLog,
}
```

**Step 2: Log in get/set/delete**

```rust
pub fn set(&self, provider: &str, secret: &str) -> Result<(), VaultError> {
    // ... existing logic ...
    self.audit.log("set", provider, None, "cli")?;
    Ok(())
}

pub fn get(&self, provider: &str) -> Result<Option<String>, VaultError> {
    let result = self.read_payload()?.and_then(|p| /* ... */);
    if result.is_some() {
        let _ = self.audit.log("get", provider, None, "runtime");
    }
    Ok(result)
}
```

**Step 3: Test + Commit**

```bash
git commit -m "feat(vault): wire audit log into get/set/delete — track credential access"
```

---

## Batch 7: MEDIUM — Structured Output + Code Quality (4 tasks)

### Task 20: Pass cached_example to StructuredOutputEngine (eliminate triple read)

**Files:**
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs:~954`

**Step 1: Set cached_example on engine before validate()**

Find where `StructuredOutputEngine` is created for the main validation path. Set:

```rust
engine.cached_example = cached_example.clone();
```

This eliminates Read #3 (the engine re-reading from_example file).

**Step 2: Test + Commit**

```bash
git commit -m "perf(structured): pass cached_example to engine — eliminate triple file read"
```

---

### Task 21: Fix L4 repair event model name

**Files:**
- Modify: `tools/nika-engine/src/runtime/structured_output.rs:~785-796`

**Step 1: Add repair_model_name field**

```rust
pub struct StructuredOutputEngine {
    // ... existing fields ...
    repair_model_name: Option<String>,
}
```

Use it in L4 event emission instead of `self.model_name`.

**Step 2: Test + Commit**

```bash
git commit -m "fix(structured): log repair_model name in L4 events — accurate telemetry"
```

---

### Task 22: Delete dead enable_extractor (L1) code

**Files:**
- Modify: `tools/nika-core/src/ast/structured.rs` (remove field)
- Modify: `tools/nika-engine/src/runtime/structured_output.rs:394-402` (remove warning block)

**Step 1: Remove the field from StructuredOutputSpec**

Delete `enable_extractor` field and all references. Zero users, zero backward compat.

**Step 2: Run tests**

Run: `cd tools && cargo test --workspace --lib 2>&1 | tail -10`

**Step 3: Commit**

```bash
git commit -m "refactor(structured): remove dead enable_extractor field (L1 was never implemented)"
```

---

### Task 23: Deduplicate is_retryable_provider_error

**Files:**
- Modify: `tools/nika-engine/src/runtime/runner.rs:925-963`
- Modify: `tools/nika-engine/src/runtime/executor/infer.rs:38-61`

**Step 1: Consolidate into a single function**

Move `is_retryable_provider_error()` to `error.rs` as a method on `NikaError`:

```rust
impl NikaError {
    pub fn is_retryable(&self) -> bool {
        match self {
            NikaError::ProviderApiError { .. } => {
                // Check variant fields directly instead of string matching
                true // all provider errors are retryable except auth
            }
            NikaError::ExecError { .. } => /* ... */,
            NikaError::FetchError { .. } => true,
            NikaError::Timeout { .. } => true,
            _ => false,
        }
    }
}
```

Delete both string-matching functions. Update call sites.

**Step 2: Add tests for the consolidated function**

**Step 3: Commit**

```bash
git commit -m "refactor(engine): consolidate is_retryable into NikaError method — no string matching"
```

---

## Summary

| Batch | Tasks | Severity | Est. LOC changed |
|-------|-------|----------|-----------------|
| 1 | T1-T2 | CRITICAL | ~100 |
| 2 | T3-T5 | HIGH | ~80 |
| 3 | T6-T9 | MEDIUM (crypto) | ~60 |
| 4 | T10-T14 | MEDIUM (serve) | ~100 |
| 5 | T15-T17 | MEDIUM (provider) | ~40 |
| 6 | T18-T19 | MEDIUM (daemon) | ~30 |
| 7 | T20-T23 | MEDIUM (quality) | ~80 |
| **Total** | **23 tasks** | | **~490 LOC** |

**Execution order:** Batch 1 first (security), then 2 (correctness), then 3-7 in any order.

**Test command after all batches:** `cd tools && cargo test --workspace --lib`
