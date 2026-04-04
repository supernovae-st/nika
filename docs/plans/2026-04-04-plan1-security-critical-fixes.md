# Plan 1: Security Critical Fixes

**Date**: 2026-04-04 | **Version**: v0.68.0 (feature freeze)
**Priority**: IMMEDIATE — All items are security-relevant
**Source**: 7-agent mega audit (Rust Security, Async Expert, Code Explorer)

---

## Overview

| ID | Severity | Finding | File | Effort |
|----|----------|---------|------|--------|
| C-1 | CRITICAL | `unsafe set_var()` UB in multi-threaded serve | `secrets/mod.rs:34` | 2h |
| H-1 | HIGH | No YAML size limit → OOM via anchor bombs | `ast/loader.rs:212` | 30m |
| H-2 | HIGH | Agent FetchTool SSRF bypass on redirects | `builtin/fetch_tool.rs:37` | 1h |
| H-3 | HIGH | Symlink escape in artifact path validation | `io/security.rs:124` | 1h |
| H-4 | HIGH | Vault KDF too weak (6 iter, 64KB) | `nika-vault/src/lib.rs:571` | 45m |
| M-5 | MEDIUM | PATH not blocked in exec env | `runtime/security.rs:579` | 15m |
| M-6 | MEDIUM | Daemon HasSecret unauthenticated | `nika-daemon/src/server.rs:463` | 30m |
| M-7 | MEDIUM | Vault plaintext not zeroized | `nika-vault/src/lib.rs:519` | 30m |

**Total estimated**: ~6.5 hours

---

## C-1: Replace `inject_secret_to_env` with In-Process Secret Store

### Problem

`secrets/mod.rs:34` calls `unsafe { std::env::set_var() }`. This is UB when called from
multiple Tokio tasks in `nika serve --embedded` mode. Two concurrent `POST /v1/run` requests
both trigger `load_from_daemon_or_fallback()` which races on the process environment.

Since Rust 2024 edition, `std::env::set_var` is `unsafe` precisely because of this.

### Current Code

```rust
// tools/nika-engine/src/secrets/mod.rs:34
pub fn inject_secret_to_env(env_var: &str, value: &str) {
    // SAFETY: callers guarantee single-threaded context (see doc above)
    unsafe { std::env::set_var(env_var, value) };
}
```

### Solution: `SecretStore` with `DashMap`

Create a thread-safe in-process secret store that replaces `std::env::set_var`. The
binding resolver reads from this store instead of `std::env::var` for provider keys.

### Step-by-step

#### Step 1: Create `SecretStore` type

**File**: `tools/nika-engine/src/secrets/store.rs` (NEW)

```rust
//! Thread-safe in-process secret store.
//!
//! Replaces `std::env::set_var` for provider API keys, eliminating UB
//! in multi-threaded contexts (nika serve embedded mode).

use dashmap::DashMap;
use once_cell::sync::Lazy;
use secrecy::{ExposeSecret, SecretString};

/// Global secret store — thread-safe, lock-free reads via DashMap.
static STORE: Lazy<DashMap<String, SecretString>> = Lazy::new(DashMap::new);

/// Store a secret (replaces `std::env::set_var` for API keys).
pub fn set_secret(key: &str, value: &str) {
    STORE.insert(key.to_string(), SecretString::from(value.to_string()));
}

/// Read a secret (replaces `std::env::var` for API keys).
pub fn get_secret(key: &str) -> Option<String> {
    STORE.get(key).map(|s| s.expose_secret().to_string())
}

/// Check if a secret exists.
pub fn has_secret(key: &str) -> bool {
    STORE.contains_key(key)
}

/// Clear all secrets (for testing).
#[cfg(test)]
pub fn clear() {
    STORE.clear();
}

/// Resolve a value from the secret store first, then fall back to env var.
/// This is the primary lookup function used by the binding resolver.
pub fn resolve_env(key: &str) -> Option<String> {
    // Secret store takes priority (loaded from daemon/vault)
    if let Some(val) = get_secret(key) {
        return Some(val);
    }
    // Fall back to actual env var
    std::env::var(key).ok().filter(|v| !v.is_empty())
}
```

#### Step 2: Update `inject_secret_to_env` to use the store

**File**: `tools/nika-engine/src/secrets/mod.rs`

Replace:
```rust
pub fn inject_secret_to_env(env_var: &str, value: &str) {
    unsafe { std::env::set_var(env_var, value) };
}
```

With:
```rust
pub fn inject_secret_to_env(env_var: &str, value: &str) {
    store::set_secret(env_var, value);
}
```

#### Step 3: Update `$env.VAR` binding resolution

**File**: `tools/nika-engine/src/binding/resolve.rs`

Find where `$env.VAR` is resolved (likely `std::env::var(name)`).
Replace with `crate::secrets::store::resolve_env(name)`.

Search pattern: `std::env::var` in resolve.rs — replace the env-binding path.

#### Step 4: Update provider `has_env_key()` check

**File**: `tools/nika-engine/src/provider/rig/mod.rs`

In `from_name()` (line ~175), `provider.has_env_key()` likely calls `std::env::var`.
Update to use `store::resolve_env(provider.env_var)`.

#### Step 5: Update `RigProvider` constructors

The rig-core clients (`anthropic::Client::from_env()`) read env vars directly.
For these, we need to use `from_name_with_key()` instead of `from_env()`:

```rust
// Instead of: Self::claude() which calls anthropic::Client::from_env()
// Use: Self::claude_with_key(key) where key comes from store::resolve_env()
let key = store::resolve_env(provider.env_var)
    .ok_or(ProviderError::MissingApiKey { provider: name.into() })?;
// Then pass key explicitly to the client constructor
```

#### Step 6: Remove all `unsafe` blocks

Remove `unsafe { std::env::set_var() }` from:
- `secrets/mod.rs:36`
- All test code that uses `std::env::set_var` → use `serial_test` + store

#### Step 7: Tests

```rust
#[test]
fn secret_store_set_and_get() {
    store::set_secret("TEST_KEY", "test_value");
    assert_eq!(store::get_secret("TEST_KEY"), Some("test_value".to_string()));
}

#[test]
fn resolve_env_prefers_store_over_env() {
    store::set_secret("MIXED_KEY", "from_store");
    std::env::set_var("MIXED_KEY", "from_env");
    assert_eq!(store::resolve_env("MIXED_KEY"), Some("from_store".to_string()));
    // cleanup
    unsafe { std::env::remove_var("MIXED_KEY") };
    store::clear();
}

#[test]
fn resolve_env_falls_back_to_env() {
    std::env::set_var("ONLY_ENV_KEY", "env_val");
    assert_eq!(store::resolve_env("ONLY_ENV_KEY"), Some("env_val".to_string()));
    unsafe { std::env::remove_var("ONLY_ENV_KEY") };
}
```

### Verification

```bash
cargo test --workspace --lib -p nika-engine -- secrets::store
cargo test --workspace --lib -p nika-engine -- secrets::tests
# Ensure no remaining unsafe set_var in non-test code:
rg "unsafe.*set_var" tools/nika-engine/src/ --glob '!*test*'
```

### Risk Assessment

- **Breaking change**: No — the external API is unchanged. Secrets still flow
  from daemon/vault into the engine. The difference is internal storage.
- **Performance**: DashMap reads are lock-free (atomic compare-and-swap).
  Negligible overhead vs `std::env::var`.
- **Backward compat**: `$env.VAR` bindings still work — `resolve_env()` falls
  back to `std::env::var` for non-secret env vars (PATH, HOME, etc.).

---

## H-1: YAML Size Limit for Skill/Agent Files

### Problem

`serde_yaml::from_str` (serde-saphyr) does NOT reject YAML anchors. A malicious
`.agent.yaml` or `.skill.yaml` file with billion-laughs anchor expansion causes OOM.

Workflow YAML is parsed by `marked_yaml` which rejects anchors — SAFE.
But skill/agent auxiliary files use `serde_yaml::from_str` — VULNERABLE.

### Step-by-step

#### Step 1: Add size check before parsing

**File**: `tools/nika-engine/src/ast/loader.rs`

Find the function that loads skill/agent YAML (around line 212).
Add a size check before `serde_yaml::from_str`:

```rust
const MAX_AUXILIARY_YAML_SIZE: usize = 1_048_576; // 1 MB

fn load_auxiliary_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, NikaError> {
    let content = std::fs::read_to_string(path)?;
    if content.len() > MAX_AUXILIARY_YAML_SIZE {
        return Err(NikaError::SchemaError {
            message: format!(
                "YAML file '{}' is {} bytes, exceeding 1MB limit",
                path.display(), content.len()
            ),
        });
    }
    serde_yaml::from_str(&content).map_err(|e| ...)
}
```

#### Step 2: Apply to all `serde_yaml::from_str` call sites on external input

Search: `rg "serde_yaml::from_str" tools/nika-engine/src/`
For each call site loading from a file (not from internal strings), add the size check.

#### Step 3: Test

```rust
#[test]
fn rejects_oversized_yaml() {
    let huge = "a: ".to_string() + &"x".repeat(2_000_000);
    let result = load_auxiliary_yaml::<serde_json::Value>(&huge);
    assert!(result.is_err());
}
```

### Verification

```bash
# No serde_yaml::from_str on unchecked file content:
rg "serde_yaml::from_str" tools/nika-engine/src/ | grep -v test
```

---

## H-2: SSRF-Safe Client for Agent FetchTool

### Problem

`builtin/fetch_tool.rs:37-39` creates its own reqwest client with `Policy::limited(10)` —
a simple redirect limit with NO SSRF checking on redirect targets.

The main `TaskExecutor.http_client` has a custom SSRF-aware redirect policy that checks
every hop against private IP ranges. But agents use `FetchTool` which bypasses this.

### Current Code

```rust
// tools/nika-engine/src/runtime/builtin/fetch_tool.rs:37
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .redirect(reqwest::redirect::Policy::limited(10))  // ← NO SSRF CHECK
    .user_agent("nika-agent/1.0")
    .build()
    .expect("reqwest client");
```

### Solution: Extract shared SSRF-safe client builder

#### Step 1: Create shared client builder

**File**: `tools/nika-engine/src/runtime/executor/http_client.rs` (NEW or extend existing)

```rust
/// Build a reqwest client with SSRF-safe redirect policy.
/// Used by both TaskExecutor (fetch verb) and FetchTool (agent builtin).
pub fn build_ssrf_safe_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(ssrf_redirect_policy())
        .connect_timeout(Duration::from_secs(10))
        .user_agent("nika/1.0")
        .build()
        .expect("reqwest client")
}

fn ssrf_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else if is_ssrf_target(attempt.url()) {
            attempt.error("SSRF: redirect to private IP blocked")
        } else {
            attempt.follow()
        }
    })
}
```

#### Step 2: Update FetchTool to use shared builder

**File**: `tools/nika-engine/src/runtime/builtin/fetch_tool.rs`

Replace the client construction in `FetchTool::new()`:
```rust
let client = super::super::executor::http_client::build_ssrf_safe_client(
    Duration::from_secs(30),
);
```

#### Step 3: Update TaskExecutor to use same builder

Ensure `TaskExecutor` also uses `build_ssrf_safe_client` instead of duplicating the
redirect policy. This makes the SSRF policy a single source of truth.

#### Step 4: Test

```rust
#[tokio::test]
async fn agent_fetch_blocks_ssrf_redirect() {
    // Start a local server that 302s to 169.254.169.254
    // Verify FetchTool returns an SSRF error, not the metadata
}
```

### Verification

```bash
rg "redirect::Policy" tools/nika-engine/src/ --glob '!*test*'
# Should show only 1 location (the shared builder), not 2+
```

---

## H-3: Symlink Escape in Artifact Path Validation

### Problem

`io/security.rs:124` returns `Ok(full_path)` — the non-canonicalized path.
`validate_artifact_path` uses logical normalization, not `canonicalize()`, so a
symlink inside the artifact directory that points outside the boundary passes validation.

### Solution: Post-validation canonicalize check

#### Step 1: Add symlink check after validation

**File**: `tools/nika-engine/src/io/security.rs`

After `validate_artifact_path` returns `Ok(full_path)`, before writing:

```rust
/// Validate artifact path with symlink protection.
pub fn validate_artifact_path_safe(
    artifact_dir: &Path,
    output_path: &Path,
) -> Result<PathBuf, NikaError> {
    let path = validate_artifact_path(artifact_dir, output_path)?;

    // Symlink check: if path exists, canonicalize and re-check boundary
    if path.exists() {
        let canonical = path.canonicalize().map_err(|e| NikaError::ArtifactPathError {
            path: path.display().to_string(),
            reason: format!("Failed to canonicalize: {e}"),
        })?;
        let canonical_base = artifact_dir.canonicalize().unwrap_or_else(|_| artifact_dir.to_path_buf());
        if !canonical.starts_with(&canonical_base) {
            return Err(NikaError::ArtifactPathError {
                path: output_path.display().to_string(),
                reason: "Symlink escape detected: resolved path is outside artifact directory".into(),
            });
        }
    } else {
        // For new files: validate each existing ancestor
        let mut check = path.clone();
        while let Some(parent) = check.parent() {
            if parent.exists() {
                let canonical_parent = parent.canonicalize().map_err(|e| NikaError::ArtifactPathError {
                    path: path.display().to_string(),
                    reason: format!("Failed to canonicalize parent: {e}"),
                })?;
                let canonical_base = artifact_dir.canonicalize()
                    .unwrap_or_else(|_| artifact_dir.to_path_buf());
                if !canonical_parent.starts_with(&canonical_base) {
                    return Err(NikaError::ArtifactPathError {
                        path: output_path.display().to_string(),
                        reason: "Symlink escape in parent directory".into(),
                    });
                }
                break;
            }
            check = parent.to_path_buf();
        }
    }
    Ok(path)
}
```

#### Step 2: Update call sites

Replace all calls to `validate_artifact_path` with `validate_artifact_path_safe`
in `artifact_processor.rs`.

#### Step 3: Test

```rust
#[test]
fn rejects_symlink_escape() {
    let dir = tempfile::tempdir().unwrap();
    let artifacts = dir.path().join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    // Create symlink: artifacts/escape -> /tmp
    std::os::unix::fs::symlink("/tmp", artifacts.join("escape")).unwrap();
    let result = validate_artifact_path_safe(&artifacts, Path::new("escape/evil.txt"));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("symlink"));
}
```

---

## H-4: Strengthen Vault KDF Parameters

### Problem

`nika-vault/src/lib.rs:571`:
```rust
let derived = kdf::derive_key(&password, &kdf_salt, 6, 1 << 16, 32)
```

Parameters: `iterations=6, memory=64KB`. OWASP recommends Argon2id with 19MB+.

### Solution

#### Step 1: Increase KDF parameters

**File**: `tools/nika-vault/src/lib.rs:571`

```rust
// Before:
let derived = kdf::derive_key(&password, &kdf_salt, 6, 1 << 16, 32)

// After: 3 iterations, 64MB memory (OWASP recommendation for interactive)
let derived = kdf::derive_key(&password, &kdf_salt, 3, 1 << 26, 32)
```

#### Step 2: Handle vault migration

Existing vaults were encrypted with the old KDF params. We need to:

1. Try decryption with new params first
2. On failure, try with old params (migration read)
3. If old params succeed, re-encrypt with new params

```rust
fn derive_key(&self) -> Result<orion::aead::SecretKey, VaultError> {
    let salt = self.load_or_create_salt()?;
    let fingerprint = machine_fingerprint()?;
    let password = kdf::Password::from_slice(fingerprint.as_bytes())?;
    let kdf_salt = kdf::Salt::from_slice(&salt)?;

    // Try current params (v2: 3 iter, 64MB)
    if let Ok(key) = kdf::derive_key(&password, &kdf_salt, 3, 1 << 26, 32) {
        return orion::aead::SecretKey::from_slice(key.unprotected_as_bytes());
    }

    // Fallback: legacy params (v1: 6 iter, 64KB) — for migration
    let key = kdf::derive_key(&password, &kdf_salt, 6, 1 << 16, 32)
        .map_err(|e| VaultError::Crypto(format!("KDF derive: {e}")))?;
    orion::aead::SecretKey::from_slice(key.unprotected_as_bytes())
        .map_err(|e| VaultError::Crypto(format!("AEAD key: {e}")))
}
```

**Note**: The actual migration (re-encrypt with new params) happens on the next `set()` call.

#### Step 3: Add vault version marker

Add a `version` byte at the start of `vault.enc`:
- `0x01` = legacy KDF (6 iter, 64KB)
- `0x02` = new KDF (3 iter, 64MB)

This avoids the try/fallback pattern and makes the format self-describing.

#### Step 4: Test

```rust
#[test]
fn vault_kdf_migration() {
    // Create vault with old params
    // Read with new code (should auto-migrate)
    // Verify read still works
}
```

### Risk Assessment

- **Performance**: 64MB Argon2i adds ~200ms to vault operations. Acceptable since
  vault is read once at boot, not per-request.
- **Migration**: Transparent — existing vaults auto-migrate on next write.

---

## M-5: Block PATH in Exec Env

### One-line fix

**File**: `tools/nika-engine/src/runtime/security.rs:579`

```rust
const BLOCKED_ENV_VARS: &[&str] = &[
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    "DYLD_FRAMEWORK_PATH",
    "LD_AUDIT",
    "LD_PROFILE",
    "PATH",  // ← ADD THIS
];
```

### Test

```rust
#[test]
fn rejects_path_env_var() {
    let env = vec![("PATH".into(), "/evil/bin".into())];
    let result = validate_env_vars(&env);
    assert!(result.is_err());
}
```

---

## M-6: Require Auth for `ListSecrets`

### Problem

`HasSecret` and `ListSecrets` don't require auth tokens. `ListSecrets` reveals
which providers have stored keys.

### Solution

**File**: `tools/nika-daemon/src/server.rs:463`

Add auth check to `ListSecrets` (keep `HasSecret` unauthenticated for health checks):

```rust
DaemonRequest::ListSecrets { auth_token } => {
    self.validate_auth_token(&auth_token)?;
    // ... existing logic
}
```

Update the `DaemonRequest::ListSecrets` variant to include `auth_token: String`.

---

## M-7: Zeroize Vault Plaintext

### Solution

**File**: `tools/nika-vault/src/lib.rs`

Add `zeroize` dependency:
```toml
[dependencies]
zeroize = { version = "1", features = ["derive"] }
```

Derive `Zeroize` on `VaultPayload`:
```rust
#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct VaultPayload {
    entries: BTreeMap<String, VaultEntry>,
}

#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct VaultEntry {
    value: String,
    updated_at: String,
}
```

This ensures the plaintext is zeroed when `VaultPayload` is dropped.

---

## Execution Order

```
1. C-1  SecretStore (CRITICAL, eliminates UB)          → 2h
2. H-2  SSRF-safe FetchTool (easy, high impact)        → 1h
3. M-5  Block PATH env (one-line)                       → 15m
4. H-1  YAML size limit (straightforward)               → 30m
5. H-3  Symlink artifact check (moderate)               → 1h
6. H-4  Vault KDF upgrade + migration (careful)         → 45m
7. M-6  ListSecrets auth (small daemon change)          → 30m
8. M-7  Zeroize vault payload (add derive)              → 30m
```

## Verification Checklist

- [ ] `cargo test --workspace --lib` — all 9800+ tests pass
- [ ] `rg "unsafe.*set_var" tools/ --glob '!*test*'` — zero results in prod code
- [ ] `rg "redirect::Policy::limited" tools/` — zero results (all SSRF-safe)
- [ ] `rg "serde_yaml::from_str" tools/nika-engine/` — all guarded by size check
- [ ] Security test: symlink in artifacts → rejected
- [ ] Security test: agent fetch redirect to 169.254.x.x → blocked
- [ ] Vault migration test: old vault reads with new code
