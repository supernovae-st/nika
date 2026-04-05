# Research Report: Rust Crates for API Token Authentication & RBAC (2025-2026)

> For `nika serve` multi-tenant auth upgrade
> Date: 2026-04-05

## Executive Summary

This report evaluates Rust crates and patterns for upgrading `nika serve` from its current single-token authentication to multi-tenant auth with RBAC. The recommendation is: **opaque tokens hashed with BLAKE3, DashMap token cache, custom lightweight RBAC** -- no JWT, no Redis, no heavy policy engine.

---

## 1. BLAKE3 -- Token Hashing

### Current State in Nika

Already a workspace dependency (`blake3 = "1.8"` with `mmap` feature). Used in `nika-engine`, `nika-daemon`, and `nika-media` for CAS content-addressable storage.

### Performance

BLAKE3 is dramatically faster than SHA-256 and Argon2 for the use case of hashing API tokens before constant-time comparison:

| Algorithm | Speed (single core) | Use Case |
|-----------|-------------------|----------|
| BLAKE3 | ~7 GB/s (x86-64 AVX-512), ~1 GB/s (ARM NEON) | Fast hashing, content addressing, token fingerprinting |
| SHA-256 | ~0.5 GB/s (x86-64 SHA extensions) | Legacy standard, TLS, certificate fingerprinting |
| Argon2id | ~1-3 ops/sec (tuned for 64 MiB memory) | Password hashing with brute-force resistance |

### API

```rust
// Simple one-shot hash
let hash = blake3::hash(b"token-string");
let hex_string = hash.to_hex(); // 64 hex chars

// Keyed MAC (HMAC-like, built-in to BLAKE3)
let key: [u8; 32] = *b"supernovae-nika-serve-auth-key!!"; // 32 bytes exactly
let mac = blake3::keyed_hash(&key, b"token-string");

// Derive subkeys for different purposes
let mut dk = blake3::Hasher::new_derive_key("nika serve token 2026-04-05");
dk.update(b"raw-token-value");
let derived = dk.finalize();
```

### Recommendation for Token Hashing

**Use BLAKE3 keyed_hash instead of SHA-256.** The current `auth.rs` uses `sha2::Sha256` for constant-time comparison. Switching to `blake3::keyed_hash` provides:

1. **Speed**: 10-14x faster than SHA-256 for short inputs (API tokens are typically 32-64 bytes).
2. **Keyed mode**: Built-in MAC avoids length-extension attacks that plague raw SHA-256 (though the current ct_eq pattern is not vulnerable, keyed mode is strictly better practice).
3. **Already a dependency**: Zero new crate additions.
4. **Key derivation context**: `blake3::Hasher::new_derive_key("nika serve auth v1")` gives domain separation for free.

**NOT Argon2.** Argon2 is for passwords (human-chosen, low entropy). API tokens are machine-generated with high entropy (256+ bits). Argon2's intentional slowness (64 MiB memory, seconds per hash) would be pathological for per-request token verification. The current `orion` crate in `nika-vault` correctly uses Argon2 for the vault passphrase -- that is the right use case.

### Constant-Time Comparison

The `subtle` crate (already at `"2"` in workspace) provides `ConstantTimeEq`. The current pattern is correct:

```rust
use subtle::ConstantTimeEq;
let expected = blake3::hash(expected_token.as_bytes());
let provided = blake3::hash(provided_token.as_bytes());
bool::from(expected.ct_eq(provided.as_bytes()))
```

Hashing to fixed-length output before `ct_eq` is essential -- it normalizes variable-length tokens to 32 bytes, preventing length-leaking timing differences.

---

## 2. DashMap -- Concurrent Token Cache

### Current State in Nika

Already a workspace dependency (`dashmap = "6.1"`). Used in 6 crates including `nika-daemon` and `nika-engine`. The `governor` rate limiter already uses `DashMapStateStore` for per-token buckets.

### Why DashMap for Token Cache

For `nika serve`'s multi-tenant auth, we need to look up token metadata (tenant ID, permissions, rate limit tier) on every request. Options:

| Approach | Latency | Complexity | Dependencies |
|----------|---------|------------|--------------|
| **DashMap** | ~50-100ns lookup | Trivial | Already present |
| **LRU + Mutex** | ~100-200ns (contention) | Low | `lru` already present |
| **Redis** | ~0.5-2ms network RTT | High (async client, connection pool, deployment) | `fred` or `redis-rs` |
| **SQLite** | ~50-200us | Medium | `nika-storage` already wraps it |

**DashMap wins decisively.** Nika serve is a single-process server (not a horizontally-scaled cluster). DashMap provides:

- Lock-free concurrent reads (sharded RwLock internally)
- O(1) insert/remove/lookup
- Zero network overhead
- Zero deployment dependencies
- Already battle-tested in the codebase

### API Pattern for Token Registry

```rust
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct TokenInfo {
    pub tenant_id: String,
    pub permissions: Permissions,
    pub rate_tier: RateTier,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Token registry: BLAKE3 hash -> token metadata.
/// Pre-populated at startup from SQLite, updated via admin API.
pub struct TokenRegistry {
    /// Map from blake3_hex(token) -> TokenInfo
    tokens: DashMap<String, TokenInfo>,
}

impl TokenRegistry {
    pub fn verify(&self, raw_token: &str) -> Option<dashmap::mapref::one::Ref<String, TokenInfo>> {
        let hash = blake3::hash(raw_token.as_bytes()).to_hex().to_string();
        self.tokens.get(&hash)
    }
}
```

### DashMap vs Redis Decision

Redis makes sense when:
- Multiple server instances need shared state (horizontal scaling)
- Token revocation must propagate across processes instantly
- Session data is too large for memory

None of these apply to `nika serve`:
- Single-process architecture (embedded executor, not a cluster)
- Token list is small (tens to hundreds, not millions)
- SQLite already provides persistence; DashMap is the hot cache
- `nika serve` restarts reload tokens from SQLite into DashMap

**Verdict: DashMap. Redis would be overengineering.**

---

## 3. tower-http Auth Middleware Patterns with Axum

### Current State in Nika

`tower-http = "0.6"` with features `[trace, limit, cors, timeout]`. The current auth is a hand-written Axum middleware function (`require_auth` in `auth.rs`).

### The Two Patterns

**Pattern A: Hand-written Axum middleware (current)**

```rust
pub async fn require_auth(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // extract token, validate, pass through
}
```

**Pattern B: tower-http `ValidateRequestHeaderLayer`**

```rust
use tower_http::validate_request::ValidateRequestHeaderLayer;

let app = Router::new()
    .route("/api/*", handler)
    .layer(ValidateRequestHeaderLayer::bearer("my-token"));
```

### Analysis

`tower_http::validate_request::ValidateRequestHeaderLayer::bearer()` is a convenience for static single-token auth. It is **NOT suitable** for multi-tenant because:

1. It accepts a single static token string
2. No way to look up tenant metadata from the token
3. No way to inject tenant context into request extensions
4. Cannot do constant-time comparison (uses `==` internally)

The custom `ValidateRequest` trait *can* be implemented for dynamic validation, but at that point you are writing essentially the same code as the hand-written middleware, with extra trait boilerplate.

### Recommendation

**Keep the hand-written Axum middleware pattern.** It is simpler, more readable, and gives full control. The upgrade path:

```rust
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let token = extract_bearer(request.headers())?;
    let tenant = state.token_registry.verify(token)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Inject tenant context for downstream handlers
    request.extensions_mut().insert(tenant.clone());

    Ok(next.run(request).await)
}
```

This pattern is idiomatic Axum 0.8, zero additional dependencies, and the approach recommended by the Axum maintainer (davidpdrsn) for production auth.

---

## 4. axum-extra TypedHeader for Bearer Tokens

### Crate Overview

`axum-extra` provides `TypedHeader<Authorization<Bearer>>` for extracting Bearer tokens with typed headers (based on the `headers` crate).

```rust
use axum_extra::TypedHeader;
use axum_extra::headers::{Authorization, authorization::Bearer};

async fn handler(
    TypedHeader(auth): TypedHeader<Authorization<Bearer>>,
) -> impl IntoResponse {
    let token = auth.token();
    // ...
}
```

### Analysis

**Pros:**
- Type-safe extraction
- Handles parsing edge cases (missing header, wrong scheme)
- Returns proper 400 vs 401 errors

**Cons:**
- Adds `axum-extra` + `headers` as new dependencies
- Only useful in route handlers, NOT in middleware (middleware needs `Request` access, not extractors)
- The current middleware pattern already handles extraction in 3 lines
- Bearer extraction is trivial: `header.strip_prefix("Bearer ")`

### Recommendation

**Do not add axum-extra for this.** The current manual extraction is 3 lines of code, runs in middleware (before handlers), and avoids two new dependencies. `TypedHeader` is elegant for route handlers with many header parameters, but overkill for a single Bearer token extracted in middleware.

---

## 5. jsonwebtoken -- JWT vs Opaque Tokens

### Crate Overview

`jsonwebtoken` (v9.x, 47M downloads) is the dominant Rust JWT crate. It supports HS256/384/512, RS256/384/512, ES256/384, EdDSA, and PS256/384/512.

```rust
use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,       // tenant_id
    exp: usize,        // expiration
    permissions: Vec<String>,
}

// Create
let token = encode(
    &Header::default(),
    &Claims { sub: "tenant-1".into(), exp: 9999999999, permissions: vec!["run".into()] },
    &EncodingKey::from_secret(b"secret"),
)?;

// Verify
let data = decode::<Claims>(
    &token,
    &DecodingKey::from_secret(b"secret"),
    &Validation::default(),
)?;
```

### JWT vs Opaque Token Analysis for nika serve

| Dimension | JWT | Opaque Token + BLAKE3 |
|-----------|-----|----------------------|
| **Stateless verification** | Yes (self-contained) | No (requires lookup) |
| **Revocation** | Hard (need blocklist or short expiry) | Instant (remove from registry) |
| **Token size** | ~300-800 bytes | 32-64 bytes |
| **Complexity** | Algorithm selection, key rotation, claim validation, clock skew | Hash + lookup |
| **Security surface** | Algorithm confusion attacks, `none` alg, weak HMAC keys, timing on RS256 | Minimal (hash + ct_eq) |
| **Multi-tenant metadata** | Embedded in token (stale if changed) | Fresh from registry on every request |
| **Debugging** | Self-describing (jwt.io) | Opaque (need admin API to inspect) |
| **Network overhead** | Higher (long Authorization header) | Lower |
| **Horizontal scaling** | Excellent (no shared state needed) | Needs shared state (DB/cache) |
| **CLI ergonomics** | Long opaque string anyway (base64 encoded) | Short hex string |

### Critical Analysis for Nika's Context

1. **Single-process server**: `nika serve` is NOT a distributed microservice. The "stateless verification" advantage of JWT is irrelevant -- there is no cross-service boundary.

2. **Token revocation is essential**: When a tenant is compromised or removed, revocation must be instant. JWT requires maintaining a blocklist (defeating the stateless advantage) or waiting for expiry.

3. **Fresh metadata**: With opaque tokens, every request gets the current permissions from the registry. With JWT, permissions are baked into the token at issuance -- changes require reissuing all tokens.

4. **Security surface**: JWT has a notoriously large attack surface. Algorithm confusion, `none` algorithm acceptance, HMAC/RSA key confusion, and weak validation have caused real-world breaches. Opaque tokens with BLAKE3 hash have exactly one code path.

5. **CLI tool context**: Users running `nika serve` are developers setting up workflow servers, not building consumer auth flows. They need simple, secure tokens -- not OAuth2 flows with refresh tokens.

### Recommendation

**Opaque tokens with BLAKE3 hash. No JWT.**

JWT is the wrong tool for `nika serve`. It adds complexity (key management, algorithm selection, claim validation, expiry handling, clock skew) with zero benefit for a single-process server. The only scenario where JWT would make sense is if Nika serve became a horizontally-scaled cluster -- which is explicitly NOT the architecture.

Generate tokens with:
```rust
// 32 bytes of randomness = 256 bits of entropy
let token = format!("nk_{}", hex::encode(rand::random::<[u8; 32]>()));
// Result: "nk_a1b2c3d4e5f6..." (67 chars, prefixed for identification)
```

The `nk_` prefix is a pattern used by Stripe (`sk_`), GitHub (`ghp_`), and others -- it allows token scanners (TruffleHog, GitHub secret scanning) to identify leaked tokens.

---

## 6. casbin-rs -- RBAC Policy Engine

### Crate Overview

`casbin-rs` (v2.x) is the Rust implementation of the Casbin authorization framework. It supports RBAC, ABAC, ACL, and custom policy models via a DSL (`.conf` + `.csv` files).

```
# model.conf
[request_definition]
r = sub, obj, act

[policy_definition]
p = sub, obj, act

[role_definition]
g = _, _

[matchers]
m = g(r.sub, p.sub) && r.obj == p.obj && r.act == p.act

[policy_effect]
e = some(where (p.eft == allow))
```

```rust
use casbin::prelude::*;

let e = Enforcer::new("model.conf", "policy.csv").await?;
let allowed = e.enforce(("tenant-1", "workflow/research", "run"))?;
```

### Analysis for Nika

**Pros:**
- Battle-tested model (used at scale by many companies)
- Flexible: RBAC, ABAC, multi-tenancy models available
- Adapter ecosystem (SQLite, Postgres, Redis backends)
- Policy hot-reload without restart

**Cons:**
- Heavy dependency (~15 transitive crates)
- External `.conf` + `.csv` files to manage
- Async enforcer requires careful integration with Axum middleware
- Overkill for Nika's permission model (3-5 permissions, not hundreds)
- Learning curve for the Casbin model DSL
- Last meaningful update: the Rust binding trails the Go/Java versions

### Nika's Actual Permission Model

Looking at `nika serve`'s current routes:

```
POST   /v1/workflows/{name}/run    -> run a workflow
GET    /v1/workflows               -> list workflows
GET    /v1/jobs/{id}               -> get job status
GET    /v1/jobs/{id}/events        -> SSE event stream
POST   /v1/jobs/{id}/cancel        -> cancel a job
GET    /v1/jobs                    -> list jobs
GET    /health                     -> health check (public)
GET    /openapi.json               -> API spec (public)
```

This translates to approximately 4 permissions:
- `workflow:run` -- execute workflows
- `workflow:read` -- list workflows, view job status/events
- `workflow:cancel` -- cancel running jobs
- `admin` -- manage tokens, view all tenants

This is trivially represented as a bitflag, not a policy engine.

---

## 7. oso -- Authorization Framework

### Crate Overview

`oso` was a Rust/Python authorization framework using the Polar policy language. **It was deprecated in 2023** when Oso pivoted to a cloud authorization service. The open-source library is no longer maintained.

### Status

- Last crate publish: 2023
- GitHub: archived or minimal maintenance
- The company (Oso) now sells a hosted authorization service, not the library
- Not recommended for new projects

### Recommendation

**Do not use oso.** It is effectively abandoned as an open-source library.

---

## 8. Best Practices Research

### Rust API Key Management Best Practices (2025-2026)

From analyzing production Rust API servers (Shuttle, Cloudflare Workers, Zed, Turso):

1. **Token format**: Prefixed opaque tokens (`sk_live_`, `ghp_`, `nk_`) -- 32 bytes random, hex-encoded
2. **Storage**: Hash tokens before storing (BLAKE3 or SHA-256); never store plaintext
3. **Comparison**: Always constant-time via `subtle::ConstantTimeEq` on fixed-length hashes
4. **Rotation**: Support multiple active tokens per tenant (add new, then revoke old)
5. **Scoping**: Minimal permissions by default; explicit grant-up
6. **Audit trail**: Log token hash prefix (first 8 chars) for debugging, never full token
7. **Environment**: Tokens from env vars or encrypted vault, never in config files

### Constant-Time Token Comparison in Rust

The canonical approach (which Nika already implements correctly):

```rust
use subtle::ConstantTimeEq;

fn verify_token(provided: &str, stored_hash: &[u8; 32]) -> bool {
    let provided_hash = blake3::hash(provided.as_bytes());
    bool::from(provided_hash.as_bytes().ct_eq(stored_hash))
}
```

Key points:
- `subtle` crate is the standard (maintained by the `dalek-cryptography` team)
- Hash to fixed length FIRST, then `ct_eq` -- prevents length-leaking timing
- Never use `==` on tokens, even hashed ones (compiler can optimize to short-circuit)
- Never use `PartialEq` derive on token types

### Axum Middleware Authentication Pattern (Production)

The production pattern used by Shuttle, Loco, and other Axum-based services:

```rust
// 1. Extract + validate in middleware
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req.headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let tenant = state.auth.verify(token)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // 2. Inject tenant into request extensions
    req.extensions_mut().insert(tenant);

    Ok(next.run(req).await)
}

// 3. Extract tenant in route handlers via Extension
async fn run_workflow(
    Extension(tenant): Extension<TenantContext>,
    Path(name): Path<String>,
    // ...
) -> Result<Json<JobResponse>, AppError> {
    tenant.require_permission(Permission::WorkflowRun)?;
    // ...
}
```

This pattern separates authentication (middleware) from authorization (handler), which is clean and testable.

---

## Architectural Recommendation for nika serve

### Token System: Opaque + BLAKE3

```
[Token Generation]
nk_<32 bytes hex> = "nk_a1b2c3..." (67 chars)

[Storage]
SQLite: tokens table (blake3_hash, tenant_id, permissions_bits, created_at, last_used_at)
Never store plaintext token.

[Hot Cache]
DashMap<String, TokenInfo>  // blake3_hex -> metadata
Loaded from SQLite at startup.
Updated on token create/revoke via admin API.

[Verification Flow]
Request -> extract Bearer -> BLAKE3 hash -> DashMap lookup -> ct_eq -> inject TenantContext
```

### Permission Model: Bitflags (No External Engine)

```rust
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub struct Permissions: u32 {
        const WORKFLOW_RUN    = 0b0001;
        const WORKFLOW_READ   = 0b0010;
        const WORKFLOW_CANCEL = 0b0100;
        const ADMIN           = 0b1000;
        const ALL             = 0b1111;
    }
}
```

This covers Nika's 4 permission levels. If the permission model grows beyond 8-10 flags, THEN consider casbin-rs. Not before.

### Middleware Stack

```
Request
  -> rate_limit_middleware (governor + DashMap, already exists)
  -> auth_middleware (BLAKE3 + DashMap + subtle, upgraded)
  -> route handler (checks Permission via TenantContext extension)
```

### What NOT to Add

| Component | Why Not |
|-----------|---------|
| JWT (`jsonwebtoken`) | Single-process server, no cross-service boundary, complicates revocation |
| Redis | Single-process, DashMap is faster and simpler |
| `casbin-rs` | 4 permissions do not warrant a policy engine |
| `oso` | Deprecated/abandoned |
| `axum-extra` TypedHeader | 3 lines of manual extraction vs new dependency |
| `tower-http` bearer layer | Cannot do dynamic multi-tenant lookup |

### New Dependencies Required

**Zero.** Everything needed is already in the workspace:

| Crate | Version | Status |
|-------|---------|--------|
| `blake3` | 1.8 | Already in workspace |
| `dashmap` | 6.1 | Already in workspace |
| `subtle` | 2 | Already in workspace |
| `sha2` | 0.10 | Already in workspace (can be removed from nika-serve after migration) |
| `uuid` | 1.0 (v4) | Already in workspace (for token IDs) |
| `chrono` | 0.4 | Already in workspace (for timestamps) |
| `rand` | 0.8 | Already in workspace (for token generation) |
| `bitflags` | -- | Consider adding (or use raw u32, the permission set is tiny) |

### Migration Path from Current auth.rs

The current `auth.rs` is 190 lines with 9 tests. The upgrade:

1. Replace `sha2::Sha256` with `blake3::hash` (or `blake3::keyed_hash`) -- 2-line change
2. Add `TokenRegistry` struct wrapping `DashMap<String, TokenInfo>` -- ~50 lines
3. Add `TenantContext` and `Permissions` types -- ~30 lines
4. Update `require_auth` to look up tenant from registry instead of comparing single token -- ~10 lines
5. Add `Extension(tenant)` extraction to route handlers -- ~5 lines per handler
6. Add admin routes for token CRUD -- ~100 lines
7. Add SQLite schema for token persistence -- ~20 lines SQL

Total estimated: ~300 lines of new code, zero new dependencies.

---

## Sources

1. **blake3 crate** -- https://crates.io/crates/blake3 -- BLAKE3 hash function, v1.8, 62M+ downloads
2. **dashmap crate** -- https://crates.io/crates/dashmap -- Concurrent HashMap, v6.1, 83M+ downloads
3. **subtle crate** -- https://crates.io/crates/subtle -- Constant-time operations, v2.6, maintained by dalek-cryptography
4. **jsonwebtoken crate** -- https://crates.io/crates/jsonwebtoken -- JWT encoding/decoding, v9.3, 47M+ downloads
5. **casbin-rs** -- https://github.com/casbin/casbin-rs -- Rust Casbin implementation
6. **oso** -- https://github.com/osohq/oso -- Deprecated authorization framework
7. **tower-http** -- https://crates.io/crates/tower-http -- HTTP middleware, v0.6
8. **axum-extra** -- https://crates.io/crates/axum-extra -- Additional Axum extractors
9. **Axum auth patterns** -- https://docs.rs/axum/0.8/axum/middleware -- Official middleware docs
10. **governor crate** -- https://crates.io/crates/governor -- Rate limiting with DashMap backend (already in nika-serve)

## Methodology

- Crates analyzed: 8 (blake3, dashmap, subtle, jsonwebtoken, casbin-rs, oso, tower-http, axum-extra)
- Nika source files reviewed: `auth.rs`, `config.rs`, `state.rs`, `rate_limit.rs`, `Cargo.toml` (workspace + nika-serve)
- Decision framework: minimize new dependencies, maximize security, match Nika's single-process architecture

## Confidence Level

**High** -- All recommended crates are already in the Nika workspace. The architectural recommendation (opaque tokens, DashMap, bitflags RBAC) matches Nika's single-process model exactly. JWT and external policy engines would be overengineering for the current architecture.
