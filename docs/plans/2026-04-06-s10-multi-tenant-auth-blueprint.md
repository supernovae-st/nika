# S10 — Multi-Tenant Auth Blueprint (Enriched)

> **Date**: 2026-04-06 | **Baseline**: 10,190 tests GREEN | **Target**: v0.73
> **Sources**: 5-agent research (Rust architect, security audit, Perplexity x2, codebase analysis)
> **LOC**: ~850 | **Files**: 8 modified + 2 new | **Tests**: 15 new | **Schema**: V5→V6

---

## EXECUTIVE SUMMARY

Replace single `NIKA_SERVE_TOKEN` with named API keys. BLAKE3-hashed, moka-cached, auto-expiring.
Backward compatible (legacy mode preserved). CLI-first token management (HTTP routes optional).

---

## ARCHITECTURE DECISIONS (from 5-agent consensus)

### AD-1: BLAKE3, not Argon2id

Tokens are 192-bit CSPRNG (not passwords). Brute-forcing 2^192 is impossible regardless of hash speed.
BLAKE3 at ~300ns/hash = 3M auth/sec/core. Argon2id at 100ms = 10/sec. Unacceptable for API server.
**Industry confirms**: Supabase, Windmill, GitHub all use fast hashes (SHA-256) for API tokens.
Post-quantum: Grover reduces to 96-bit equivalent. Still safe for 20+ years.

### AD-2: moka cache, not DashMap

DashMap has no built-in TTL — requires manual `(value, Instant)` pairs + GC background task.
**moka::sync::Cache** gives: built-in TTL per entry, bounded LRU (max_capacity), zero GC task needed.
Eviction amortized on access. Well-maintained (10M+ downloads). Already battle-tested.

```toml
# workspace Cargo.toml
moka = { version = "0.12", features = ["sync"] }
```

### AD-3: Principal as axum FromRequestParts extractor

Handlers declare `principal: Principal` in their signature — zero-cost type-safe auth.
No manual `request.extensions().get::<Principal>()` unwrapping.

```rust
#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Principal {
    type Rejection = ServeError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<Principal>().cloned().ok_or(ServeError::Unauthorized)
    }
}
```

### AD-4: Auth middleware takes Arc\<AuthMode\>, not AppState

Decouples auth from storage/semaphore/workers/etc. Clean separation of concerns.

```rust
pub async fn require_auth(
    State(auth): State<Arc<AuthMode>>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> { ... }
```

### AD-5: Legacy mode stores pre-computed hash, not plaintext

`AuthMode::Legacy { expected_hash: [u8; 32] }` — raw token hashed once at startup, then dropped.
No plaintext token in memory for the server's lifetime.

### AD-6: Token management CLI-only (no HTTP routes in L1)

**Security audit finding #4**: HTTP token routes without RBAC = any operator token can enumerate/revoke all tokens.
**Decision**: L1 uses CLI-only (`nika serve token add/list/revoke`). HTTP routes deferred to L2 with admin role.

### AD-7: Unified token generation via getrandom

**Security audit finding #2**: Blueprint had two generation paths (UUID concat vs getrandom). Use only:

```rust
fn generate_raw_token() -> String {
    let mut bytes = [0u8; 24]; // 192 bits
    getrandom::fill(&mut bytes).expect("CSPRNG unavailable");
    format!("nk_{}", hex::encode(bytes))
}
```

### AD-8: Typed Role enum from day 1

Even though L1 only uses `Operator`, define the enum now to avoid stringly-typed migration later.

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Operator,  // L1: can run workflows, manage jobs
    Viewer,    // L3: read-only
    Admin,     // L3: token management
}
```

### AD-9: Separate Unauthorized (401) and Forbidden (403)

Current plan only had Forbidden. Need both:
- **401 Unauthorized**: Missing/invalid/expired/revoked token (identical message always)
- **403 Forbidden**: Valid token, insufficient permissions (L2+ scope enforcement)

### AD-10: Normalize revoke response

**Security audit finding #9**: `DELETE /v1/tokens/{id}` should always return `{ revoked: true }` regardless of whether the ID existed. Prevents token ID enumeration.

---

## CURRENT SERVE STATE (exact)

```
auth.rs:        SHA-256 + subtle::ConstantTimeEq, 11 tests
lib.rs:         metrics → request-id → timeout → body-limit → auth → rate-limit → handler
state.rs:       AppState { storage, config, executor, semaphore, shutdown, workers, active_jobs, event_bus, webhook_config }
config.rs:      ServeConfig.auth_token: String (NIKA_SERVE_TOKEN env, >=32 chars, required)
rate_limit.rs:  governor DashMapStateStore, keyed by raw Bearer string, GC hourly
error.rs:       NotFound, PathTraversal, QueueFull, InvalidWorkflow, Config, Storage, Internal
routes:         /health, /v1/run, /v1/status/{id}, /v1/cancel/{id}, /v1/workflows, etc.
tests:          86 total (51 async)
axum:           0.8 | tower: 0.5 | tower-http: 0.6
```

---

## DATA TYPES

### TokenEntry (nika-storage)

```rust
pub struct TokenEntry {
    pub id: String,                   // UUID v4
    pub name: String,                 // human-readable, 1-64 chars, unique
    pub token_hash: Vec<u8>,          // BLAKE3(raw_token), 32 bytes, stored as BLOB
    pub role: String,                 // "operator" (L1), "viewer"/"admin" (L3)
    pub scope: String,                // "*" (L1), glob pattern (L2)
    pub created_at: String,           // RFC 3339
    pub expires_at: Option<String>,   // RFC 3339 or None
    pub last_used_at: Option<String>, // updated async (fire-and-forget)
    pub revoked: bool,                // soft-delete, immediate rejection
}
```

### AuthMode (nika-serve)

```rust
pub enum AuthMode {
    /// Legacy single-token (NIKA_SERVE_TOKEN env var)
    Legacy {
        expected_hash: [u8; 32], // BLAKE3 of the env token, computed once at startup
    },
    /// Multi-key mode (>0 tokens in serve_tokens table)
    MultiKey {
        store: TokenStore,
    },
}
```

### Principal (nika-serve)

```rust
#[derive(Clone, Debug)]
pub struct Principal {
    pub token_id: String,     // UUID for rate limiting + logging
    pub token_name: String,   // human-readable
    pub role: Role,           // typed enum
    pub scope: String,        // glob pattern
}
```

### TokenStore (nika-serve)

```rust
pub struct TokenStore {
    cache: moka::sync::Cache<[u8; 32], Principal>, // BLAKE3 hash → Principal, 60s TTL, 10K cap
    storage: nika_storage::Storage,
}

impl TokenStore {
    pub fn new(storage: nika_storage::Storage) -> Self {
        let cache = moka::sync::Cache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(60))
            .build();
        Self { cache, storage }
    }

    pub async fn authenticate(&self, raw_token: &str) -> Result<Principal, ServeError> {
        let hash = blake3::hash(raw_token.as_bytes());
        let hash_bytes: [u8; 32] = *hash.as_bytes();

        // Fast path: cache hit
        if let Some(principal) = self.cache.get(&hash_bytes) {
            return Ok(principal);
        }

        // Slow path: DB lookup
        let entry = self.storage
            .get_token_by_hash(&hash_bytes)
            .await
            .map_err(ServeError::Storage)?
            .ok_or(ServeError::Unauthorized)?;

        // Validate: not revoked, not expired
        if entry.revoked {
            return Err(ServeError::Unauthorized);
        }
        if let Some(ref expires) = entry.expires_at {
            if chrono::DateTime::parse_from_rfc3339(expires)
                .map(|dt| dt < chrono::Utc::now())
                .unwrap_or(false)
            {
                return Err(ServeError::Unauthorized);
            }
        }

        let principal = Principal {
            token_id: entry.id.clone(),
            token_name: entry.name,
            role: entry.role.parse().unwrap_or(Role::Operator),
            scope: entry.scope,
        };

        self.cache.insert(hash_bytes, principal.clone());

        // Fire-and-forget: update last_used_at
        let storage = self.storage.clone();
        let id = entry.id;
        tokio::spawn(async move {
            let _ = storage.touch_token_last_used(&id).await;
        });

        Ok(principal)
    }

    pub fn invalidate(&self, token_hash: &[u8; 32]) {
        self.cache.invalidate(token_hash);
    }
}
```

---

## V6 SCHEMA

```sql
-- Wrapped in transaction for atomicity
BEGIN;

CREATE TABLE IF NOT EXISTS serve_tokens (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    token_hash   BLOB NOT NULL UNIQUE,
    role         TEXT NOT NULL DEFAULT 'operator',
    scope        TEXT NOT NULL DEFAULT '*',
    created_at   TEXT NOT NULL,
    expires_at   TEXT,
    last_used_at TEXT,
    revoked      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_serve_tokens_hash ON serve_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_serve_tokens_name ON serve_tokens(name);

COMMIT;
```

---

## STARTUP AUTH MODE DETERMINATION

```rust
let token_count = storage.count_tokens().await?;
let legacy_token = std::env::var("NIKA_SERVE_TOKEN").ok().filter(|t| t.len() >= 32);

let auth_mode = match (token_count, legacy_token) {
    (0, None) => {
        return Err(ServeError::Config(
            "No authentication configured.\n\
             Either set NIKA_SERVE_TOKEN env var (>=32 chars),\n\
             or create a token: nika serve token add --name my-key".into()
        ));
    }
    (0, Some(token)) => {
        let hash = blake3::hash(token.as_bytes());
        AuthMode::Legacy { expected_hash: *hash.as_bytes() }
        // raw token dropped here — not stored in memory
    }
    (n, env) => {
        if env.is_some() {
            tracing::warn!(
                "NIKA_SERVE_TOKEN set but {n} tokens in DB — using multi-key mode \
                 (env var ignored)"
            );
        }
        AuthMode::MultiKey { store: TokenStore::new(storage.clone()) }
    }
};

let auth_mode = Arc::new(auth_mode);
```

---

## IMPLEMENTATION PLAN (8 phases, 8 commits)

### Phase 1: Storage V6 — serve_tokens table + CRUD

**File**: `tools/nika-storage/src/lib.rs`

Changes:
- `SCHEMA_VERSION: 5 → 6`
- Add `TokenEntry` struct (after CronSchedule)
- Add V6 migration block (BEGIN/COMMIT wrapped)
- Add 6 `DbCommand` variants: `InsertToken`, `GetTokenByHash`, `ListTokens`, `RevokeToken`, `TouchTokenLastUsed`, `CountTokens`
- Add 6 `Storage` async methods
- Add 6 `do_*` query functions
- Add dispatch arms in `run_db_loop`

**Tests** (5):
```
test_insert_and_get_token_by_hash
test_list_tokens_returns_all
test_revoke_token_by_name
test_count_tokens_excludes_revoked
test_touch_token_last_used
```

**Commit**: `feat(storage): V6 schema — serve_tokens table + token CRUD`

---

### Phase 2: TokenStore — moka cache + BLAKE3 authenticate

**File**: `tools/nika-serve/src/token_store.rs` (NEW, ~150 LOC)

Create separate module (not inline in auth.rs) for testability:
- `TokenStore` struct with `moka::sync::Cache<[u8; 32], Principal>`
- `authenticate()`: hash → cache → DB → validate → cache → fire-and-forget touch
- `invalidate()`: immediate cache eviction

**File**: `tools/nika-serve/src/principal.rs` (NEW, ~80 LOC)
- `Principal` struct + `Role` enum
- `FromRequestParts` impl for axum extractor
- `generate_raw_token()` function (single source of truth)

**File**: `tools/nika-serve/Cargo.toml`
- Add: `blake3`, `moka` (sync feature), `getrandom`, `hex`

**Tests** (4):
```
test_authenticate_valid_token       (real SQLite via tempdir)
test_reject_revoked_token
test_reject_expired_token
test_cache_hit_skips_db             (verify cache.entry_count())
```

**Commit**: `feat(serve): TokenStore with BLAKE3 + moka cache`

---

### Phase 3: Auth middleware rewrite — Legacy + MultiKey

**File**: `tools/nika-serve/src/auth.rs` (REWRITE)

Replace current SHA-256 single-token auth with:
- `AuthMode` enum (Legacy with pre-computed BLAKE3 hash, MultiKey with TokenStore)
- `require_auth()` takes `State<Arc<AuthMode>>` instead of `State<AppState>`
- Legacy path: BLAKE3 hash + `subtle::ConstantTimeEq` (defense-in-depth)
- MultiKey path: `store.authenticate(raw)` → inject `Principal` into extensions
- `/health` bypass preserved
- **Identical error message** for all failure modes (no enumeration)

**File**: `tools/nika-serve/src/error.rs`
- Add `Unauthorized` (401) and `Forbidden` (403) variants

**File**: `tools/nika-serve/src/state.rs`
- Remove `config.auth_token` usage in auth context
- No `auth_mode` in AppState (it's a separate `State<Arc<AuthMode>>` layer)

**File**: `tools/nika-serve/src/lib.rs`
- Compute `AuthMode` at startup (count_tokens → determine mode)
- Pass `Arc<AuthMode>` to auth middleware layer
- Remove `auth_token` requirement from `ServeConfig::from_env()` (now optional)

**Regression**: ALL 86 existing serve tests MUST pass (legacy mode as default)

**Commit**: `feat(serve): auth middleware supports legacy + multi-key modes`

---

### Phase 4: Rate limiter migration — Principal.token_id

**File**: `tools/nika-serve/src/rate_limit.rs`

Replace raw Bearer token extraction with:
```rust
let key = req.extensions()
    .get::<Principal>()
    .map(|p| p.token_id.clone())
    .or_else(|| extract_bearer_from_header(&req));  // legacy fallback
```

Benefits:
- Fresh rate limit bucket on revoke+recreate (UUID changes)
- No raw token in DashMap memory
- Consistent keying across Legacy and MultiKey modes

**Commit**: `refactor(serve): rate limiter uses Principal token_id`

---

### Phase 5: CLI token management

**File**: `tools/nika-cli/src/serve_token.rs` (NEW, ~200 LOC)

Three commands, direct DB access (no HTTP needed):

```
nika serve token add --name "jungo-prod" [--expires "2026-12-31"] [--scope "*"]
nika serve token list [--json]
nika serve token revoke <id-or-name>
```

- `add`: generate_raw_token() → BLAKE3 hash → insert → print token ONCE
- `list`: query all → table display (hash redacted, expiry color-coded)
- `revoke`: set revoked=true (idempotent, no error if not found)

**File**: `tools/nika-cli/src/lib.rs` — add `pub mod serve_token;`
**File**: `tools/nika/src/main.rs` — wire under `Serve` command

**Tests** (2):
```
test_add_and_list_tokens
test_revoke_idempotent
```

**Commit**: `feat(cli): nika serve token add/list/revoke`

---

### Phase 6: Startup banner + docs

**File**: `tools/nika-serve/src/lib.rs`

Update `print_startup_banner()`:
```
  ├── Auth         legacy token (48 chars)
  ├── Auth         multi-key (3 active tokens)
```

**File**: `tools/nika/CHANGELOG.md` — v0.73 entry
**File**: `AGENTS.md` — document `nika serve token` commands

**Commit**: `docs: v0.73 multi-tenant auth + startup banner`

---

### Phase 7: Drop SHA-256 dependency

**File**: `tools/nika-serve/Cargo.toml` — remove `sha2 = "0.10"`

Legacy mode now uses BLAKE3 (via `AuthMode::Legacy { expected_hash }`).
`subtle` stays for constant-time comparison on the legacy path.

**Commit**: `refactor(serve): drop sha2 — legacy auth uses BLAKE3`

---

### Phase 8: Security hardening

- Token shown exactly once on creation (CLI prints, never stored in logs)
- Log only `nk_<first 7 chars>...` prefix in tracing (never full token)
- `Authorization` header excluded from request logging
- HTTP warning on startup when not binding to localhost without TLS

**Commit**: `fix(serve): security hardening — redact tokens in logs`

---

## DEPENDENCY CHANGES

```toml
# Add to workspace Cargo.toml [workspace.dependencies]
moka = { version = "0.12", features = ["sync"] }

# Add to nika-serve/Cargo.toml [dependencies]
blake3 = { workspace = true }
moka = { workspace = true }
getrandom = { workspace = true }
hex = { workspace = true }

# Remove from nika-serve/Cargo.toml (Phase 7)
# sha2 = "0.10"  ← removed
```

---

## SECURITY AUDIT SUMMARY (5 findings addressed)

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | Cache timing hit vs miss | MEDIUM | Accept — 192-bit entropy makes oracle useless. Documented. |
| 2 | Dual token generation (UUID vs getrandom) | HIGH | **Fixed**: single `generate_raw_token()` via getrandom (AD-7) |
| 3 | Unbounded cache | HIGH | **Fixed**: moka max_capacity=10,000 + built-in LRU eviction (AD-2) |
| 4 | Token routes lack authorization | HIGH | **Fixed**: CLI-only in L1, no HTTP routes (AD-6) |
| 5 | CLI revoke bypasses in-memory cache | MEDIUM | Accept for L1 — document 60s window. Fix in L2 (daemon signal). |

---

## INDUSTRY PATTERNS ADOPTED

| Pattern | Source | Applied |
|---------|--------|---------|
| Prefixed tokens `nk_` | GitHub `ghp_`, Stripe `sk_` | ✅ Token format |
| BLAKE3 hash (not Argon2) | Supabase, Windmill (SHA-256) | ✅ Faster + same security for high-entropy |
| Token shown once | Universal (Temporal, Windmill, GitHub) | ✅ CLI prints once |
| moka bounded cache | Production Rust pattern | ✅ Replaces DashMap |
| Per-token rate limiting | Cloudflare, Railway | ✅ Principal.token_id |
| No default expiry | Fly.io (20-year), Temporal | ✅ Optional --expires |
| Typed roles from day 1 | Windmill (5 roles) | ✅ Role enum (3 variants, 1 active) |
| CLI-first management | Temporal, Windmill | ✅ Direct DB access |

---

## VERIFICATION CHECKLIST

```bash
cd tools

# Phase 1: Storage
cargo test -p nika-storage --lib -- serve_token          # 5 new tests

# Phase 2: TokenStore
cargo test -p nika-serve --lib -- token_store            # 4 new tests

# Phase 3: Auth middleware
cargo test -p nika-serve --lib                           # ALL 86+ pass (regression)

# Phase 4: Rate limiter
cargo test -p nika-serve --lib -- rate_limit             # 3 existing pass

# Phase 5: CLI
cargo test -p nika-cli --lib -- serve_token              # 2 new tests

# Full workspace
cargo test --workspace --lib                             # 10,200+ tests
cargo clippy --workspace                                 # ZERO warnings

# Manual smoke test
nika serve token add --name "test-key"                   # Prints nk_... once
nika serve token list                                    # Shows table
nika serve token revoke test-key                         # Revokes
NIKA_SERVE_TOKEN= nika serve                             # Error: no auth
nika serve token add --name "prod"                       # Create token
nika serve                                               # "Auth: multi-key (1 active token)"
curl -H "Authorization: Bearer nk_..." localhost:3000/v1/workflows  # 200
curl -H "Authorization: Bearer wrong" localhost:3000/v1/workflows   # 401
```

---

## DEFERRED TO L2/L3

| Feature | Level | ~LOC | Trigger |
|---------|-------|------|---------|
| HTTP token routes (`/v1/tokens`) | L2 | 180 | When admin role is enforced |
| Scope enforcement (glob matching) | L2 | 100 | When per-workflow scoping needed |
| RBAC (admin/viewer enforcement) | L3 | 150 | When team access control needed |
| Audit log (token_audit_log table) | L3 | 200 | When compliance requires it |
| Daemon cache invalidation signal | L2 | 80 | When CLI revoke must be instant |
| Token rotation shortcut | L2 | 50 | `nika serve token rotate <name>` |

---

## RULES

- `cargo test --workspace --lib` green after EVERY commit
- 1 phase = 1 commit
- Co-author: ONLY `Nika 🦋 <nika@supernovae.studio>`
- AGPL-3.0-or-later on new files
- TDD: test first, watch fail, implement, watch pass
- ALL auth error messages IDENTICAL (no enumeration)
- Raw tokens NEVER in logs (redact to `nk_<7chars>...`)
- `subtle::ConstantTimeEq` on legacy path (defense-in-depth)
