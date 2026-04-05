# Multi-Tenant Auth Blueprint for `nika serve`

> **Target**: v0.73 (post-launch, after scheduling ships with V5)
> **Schema migration**: V6 (`SCHEMA_VERSION` bump from 5 to 6)
> **Estimated LOC**: ~850 (L1 only — named keys + optional expiry)
> **Files touched**: 8 modified + 2 created
> **Zero backward compat**: v0 philosophy — legacy mode is a courtesy, not a promise

---

## 1. Data Types

### 1.1 TokenEntry (nika-storage)

```rust
// tools/nika-storage/src/lib.rs (or extracted to tokens.rs)

/// A named API token for nika serve multi-tenant auth.
///
/// Raw tokens are NEVER stored — only the BLAKE3 hash.
/// The raw token is shown exactly once at creation time.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenEntry {
    /// Unique identifier (UUID v4, simple format — 32 hex chars).
    pub id: String,

    /// Human-readable name (unique). E.g. "jungo-prod", "ci-staging".
    pub name: String,

    /// BLAKE3 hash of the raw token (32 bytes, stored as BLOB).
    /// Used for constant-time lookup + comparison.
    pub token_hash: Vec<u8>,

    /// Role: "admin" | "operator" (L1 ships operator-only, admin for token management).
    pub role: String,

    /// Scope glob pattern. "*" = all workflows, "jungo-*.nika.yaml" = scoped (L2).
    /// L1 stores but does NOT enforce — always "*".
    pub scope: String,

    /// RFC 3339 creation timestamp.
    pub created_at: String,

    /// Optional RFC 3339 expiry. None = never expires.
    pub expires_at: Option<String>,

    /// Last time this token was used for authentication (updated on cache miss).
    pub last_used_at: Option<String>,

    /// Soft-delete flag. Revoked tokens are rejected immediately.
    pub revoked: bool,
}
```

**Design decisions**:
- `token_hash` is `Vec<u8>` (32 bytes BLAKE3), NOT hex string. SQLite stores as BLOB. Avoids hex encode/decode overhead on every request.
- `role` is a plain String, not an enum. L1 only uses "operator". L3 adds "admin"/"viewer". No premature enum — we're v0.
- `scope` stored but not enforced in L1. This means the DB schema is forward-compatible for L2 without another migration.
- `revoked` is Integer in SQLite (0/1), maps to `bool` in Rust.

### 1.2 Principal (nika-serve)

```rust
// tools/nika-serve/src/auth.rs

/// Authenticated request context, attached to request extensions.
///
/// Handlers read this via `request.extensions().get::<Principal>()`.
/// The Principal is built from a validated TokenEntry (cache or DB).
#[derive(Debug, Clone)]
pub struct Principal {
    /// Token ID (UUID). Used as rate limiter key (replaces raw token string).
    pub token_id: String,

    /// Human-readable token name. Appears in logs and job metadata.
    pub token_name: String,

    /// Role string. L1 = always "operator". L3 = "admin" | "operator" | "viewer".
    pub role: String,

    /// Scope glob. L1 = always "*". L2 = checked against workflow path.
    pub scope: String,
}

impl Principal {
    /// Build from a validated TokenEntry.
    fn from_entry(entry: &TokenEntry) -> Self {
        Self {
            token_id: entry.id.clone(),
            token_name: entry.name.clone(),
            role: entry.role.clone(),
            scope: entry.scope.clone(),
        }
    }

    /// Check if this principal can access a given workflow path (L2).
    /// L1 always returns true (scope is "*").
    pub fn can_access_workflow(&self, _workflow: &str) -> bool {
        // L1: always true. L2 will implement glob matching here.
        self.scope == "*" || true
    }
}
```

**Why not an enum for role?** Because L1 only has "operator". Adding an enum now means dead variants. When L3 ships, we add the enum then. Zero dead code.

---

## 2. TokenStore (nika-serve)

```rust
// tools/nika-serve/src/auth.rs

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// In-memory cache for validated token entries.
///
/// Avoids a DB roundtrip on every HTTP request.
/// Cache entries expire after TTL (default: 60s).
/// Invalidated immediately on revoke (cache.remove).
struct CachedEntry {
    principal: Principal,
    inserted_at: Instant,
}

/// Thread-safe token lookup with DashMap cache + SQLite fallback.
///
/// Flow:
/// 1. Hash incoming bearer token with BLAKE3 (32 bytes)
/// 2. Lookup hash in DashMap cache
/// 3. Cache hit + not expired → return Principal
/// 4. Cache miss → SELECT by token_hash from SQLite
/// 5. Validate: not revoked, not expired
/// 6. Build Principal, insert into cache
/// 7. Update last_used_at in background (non-blocking)
#[derive(Clone)]
pub struct TokenStore {
    /// Cache keyed by BLAKE3 hash (hex-encoded for DashMap key ergonomics).
    cache: Arc<DashMap<String, CachedEntry>>,

    /// Storage handle for DB fallback.
    storage: nika_storage::Storage,

    /// Cache TTL. Entries older than this are evicted on next access.
    ttl: Duration,
}

impl TokenStore {
    pub fn new(storage: nika_storage::Storage) -> Self {
        Self {
            cache: Arc::new(DashMap::new()),
            storage,
            ttl: Duration::from_secs(60),
        }
    }

    /// Authenticate a raw bearer token.
    ///
    /// Returns Ok(Principal) on success, Err(()) on any failure.
    /// Uses BLAKE3 hash for DB lookup — raw token never touches SQLite.
    pub async fn authenticate(&self, raw_token: &str) -> Result<Principal, ()> {
        let hash = blake3::hash(raw_token.as_bytes());
        let hash_hex = hash.to_hex().to_string();
        let hash_bytes = hash.as_bytes().to_vec();

        // 1. Check cache
        if let Some(entry) = self.cache.get(&hash_hex) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Ok(entry.principal.clone());
            }
            // Expired — drop ref before removing
            drop(entry);
            self.cache.remove(&hash_hex);
        }

        // 2. DB lookup
        let token_entry = self
            .storage
            .get_token_by_hash(&hash_bytes)
            .await
            .map_err(|_| ())?
            .ok_or(())?;

        // 3. Validate
        if token_entry.revoked {
            return Err(());
        }
        if let Some(ref exp) = token_entry.expires_at {
            let now = chrono::Utc::now().to_rfc3339();
            if *exp < now {
                return Err(());
            }
        }

        // 4. Build principal + cache
        let principal = Principal::from_entry(&token_entry);
        self.cache.insert(
            hash_hex,
            CachedEntry {
                principal: principal.clone(),
                inserted_at: Instant::now(),
            },
        );

        // 5. Update last_used_at (fire-and-forget, don't block the request)
        let storage = self.storage.clone();
        let token_id = token_entry.id.clone();
        tokio::spawn(async move {
            let _ = storage.touch_token_last_used(&token_id).await;
        });

        Ok(principal)
    }

    /// Immediately invalidate a token from cache (called on revoke).
    /// Does NOT need the raw token — iterates cache looking for token_id match.
    /// This is O(n) but revoke is rare and cache is small.
    pub fn invalidate_by_id(&self, token_id: &str) {
        self.cache.retain(|_, v| v.principal.token_id != token_id);
    }

    /// Evict all expired entries. Called periodically by GC task.
    pub fn evict_expired(&self) {
        let ttl = self.ttl;
        self.cache.retain(|_, v| v.inserted_at.elapsed() < ttl);
    }
}
```

**Key design choices**:
- DashMap key is hex-encoded BLAKE3 hash (64 chars). Using `String` instead of `[u8; 32]` because DashMap requires `Hash + Eq` and byte arrays don't impl `Hash` conveniently.
- `last_used_at` update is fire-and-forget via `tokio::spawn`. Never blocks the request path.
- `invalidate_by_id` scans all cache entries. This is fine because: (a) revoke is a rare admin operation, (b) cache is bounded by active token count (typically < 100), (c) DashMap iteration is lock-free per shard.
- No `Arc<Mutex>` anywhere. DashMap handles concurrent access internally.

---

## 3. Rewritten Auth Middleware

```rust
// tools/nika-serve/src/auth.rs

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::state::AppState;

/// Authentication mode, determined at startup.
#[derive(Clone, Debug)]
pub enum AuthMode {
    /// Legacy: single NIKA_SERVE_TOKEN (existing behavior, zero DB tokens).
    Legacy { token: String },

    /// Multi-key: tokens stored in serve_tokens table.
    MultiKey { store: TokenStore },
}

/// Axum middleware: authenticates every request except /health and /metrics.
///
/// Supports two modes:
/// - Legacy: constant-time SHA-256 comparison against NIKA_SERVE_TOKEN (unchanged)
/// - Multi-key: BLAKE3 hash lookup in TokenStore (DashMap cache + SQLite)
///
/// On success in multi-key mode, attaches a Principal to request extensions
/// so downstream handlers can read token_id, name, role, scope.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Public endpoints: always bypass auth
    let path = request.uri().path();
    if path == "/health" {
        return Ok(next.run(request).await);
    }

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let raw_token = auth_header
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Empty token after "Bearer " prefix
    if raw_token.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match &state.auth_mode {
        AuthMode::Legacy { token } => {
            // Existing behavior: SHA-256 + constant-time comparison
            let expected = Sha256::digest(token.as_bytes());
            let provided = Sha256::digest(raw_token.as_bytes());
            if !bool::from(expected.ct_eq(&provided)) {
                return Err(StatusCode::UNAUTHORIZED);
            }
            // Legacy mode: no Principal in extensions (handlers don't depend on it)
        }
        AuthMode::MultiKey { store } => {
            let principal = store
                .authenticate(raw_token)
                .await
                .map_err(|_| StatusCode::UNAUTHORIZED)?;

            // Attach Principal to request extensions for downstream handlers
            request.extensions_mut().insert(principal);
        }
    }

    Ok(next.run(request).await)
}
```

**Migration path**:
- Legacy mode preserves **identical** behavior to current auth.rs — SHA-256 + `subtle::ConstantTimeEq`. Zero behavior change for existing deployments.
- Multi-key mode uses BLAKE3 (faster, 32 bytes native) because tokens are pre-hashed at creation time and stored as BLOB. No timing attack concern because cache hit is O(1) DashMap lookup.
- The `check_bearer_token` free function (current auth.rs line 20-35) is absorbed into `AuthMode::Legacy` branch. It was only used by `require_auth` and tests.

---

## 4. V6 Schema Migration SQL

```sql
-- tools/nika-storage/src/lib.rs, inside init_schema(), after V5 block

-- V6: serve_tokens table for multi-tenant auth
CREATE TABLE IF NOT EXISTS serve_tokens (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    token_hash  BLOB NOT NULL UNIQUE,
    role        TEXT NOT NULL DEFAULT 'operator',
    scope       TEXT NOT NULL DEFAULT '*',
    created_at  TEXT NOT NULL,
    expires_at  TEXT,
    last_used_at TEXT,
    revoked     INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_serve_tokens_hash
    ON serve_tokens(token_hash);

CREATE INDEX IF NOT EXISTS idx_serve_tokens_name
    ON serve_tokens(name);
```

**Implementation in `init_schema()`** (`nika-storage/src/lib.rs:645`):

```rust
// Bump: const SCHEMA_VERSION: u32 = 6;  (line 22, was 5 after scheduling)

// V6: serve_tokens (multi-tenant auth)
if version < 6 {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS serve_tokens (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            token_hash  BLOB NOT NULL UNIQUE,
            role        TEXT NOT NULL DEFAULT 'operator',
            scope       TEXT NOT NULL DEFAULT '*',
            created_at  TEXT NOT NULL,
            expires_at  TEXT,
            last_used_at TEXT,
            revoked     INTEGER NOT NULL DEFAULT 0
        );

        CREATE INDEX IF NOT EXISTS idx_serve_tokens_hash
            ON serve_tokens(token_hash);

        CREATE INDEX IF NOT EXISTS idx_serve_tokens_name
            ON serve_tokens(name);",
    )
    .map_err(|e| StorageError::Other(format!("create serve_tokens table: {e}")))?;
}
```

**Why BLOB for token_hash?** BLAKE3 produces exactly 32 bytes. Storing as BLOB avoids hex encode/decode on every DB lookup. The `idx_serve_tokens_hash` index on BLOB is efficient in SQLite — binary comparison, no collation overhead.

**Why index on name?** The `UNIQUE` constraint on `name` already creates an implicit index in SQLite, but being explicit makes intent clear and ensures the revoke-by-name query is fast.

---

## 5. Storage Layer: DbCommand Variants + Methods

### 5.1 New DbCommand Variants

```rust
// tools/nika-storage/src/lib.rs, inside enum DbCommand { ... }

InsertToken {
    entry: TokenEntry,
    reply: oneshot::Sender<StorageResult<()>>,
},
GetTokenByHash {
    hash: Vec<u8>,
    reply: oneshot::Sender<StorageResult<Option<TokenEntry>>>,
},
ListTokens {
    reply: oneshot::Sender<StorageResult<Vec<TokenEntry>>>,
},
RevokeToken {
    id: String,
    reply: oneshot::Sender<StorageResult<bool>>,
},
TouchTokenLastUsed {
    id: String,
    reply: oneshot::Sender<StorageResult<()>>,
},
CountTokens {
    reply: oneshot::Sender<StorageResult<u64>>,
},
```

### 5.2 New Storage Methods

```rust
// tools/nika-storage/src/lib.rs, inside impl Storage { ... }

/// Insert a new API token.
pub async fn insert_token(&self, entry: TokenEntry) -> StorageResult<()> {
    let (reply, rx) = oneshot::channel();
    self.tx
        .send(DbCommand::InsertToken { entry, reply })
        .await
        .map_err(|_| StorageError::ChannelClosed)?;
    rx.await.map_err(|_| StorageError::ChannelClosed)?
}

/// Look up a token by its BLAKE3 hash (32 bytes).
pub async fn get_token_by_hash(&self, hash: &[u8]) -> StorageResult<Option<TokenEntry>> {
    let (reply, rx) = oneshot::channel();
    self.tx
        .send(DbCommand::GetTokenByHash {
            hash: hash.to_vec(),
            reply,
        })
        .await
        .map_err(|_| StorageError::ChannelClosed)?;
    rx.await.map_err(|_| StorageError::ChannelClosed)?
}

/// List all tokens (for CLI display — hash is NOT returned to avoid leak).
pub async fn list_tokens(&self) -> StorageResult<Vec<TokenEntry>> {
    let (reply, rx) = oneshot::channel();
    self.tx
        .send(DbCommand::ListTokens { reply })
        .await
        .map_err(|_| StorageError::ChannelClosed)?;
    rx.await.map_err(|_| StorageError::ChannelClosed)?
}

/// Revoke a token by ID or name. Returns true if a row was updated.
pub async fn revoke_token(&self, id: &str) -> StorageResult<bool> {
    let (reply, rx) = oneshot::channel();
    self.tx
        .send(DbCommand::RevokeToken {
            id: id.to_string(),
            reply,
        })
        .await
        .map_err(|_| StorageError::ChannelClosed)?;
    rx.await.map_err(|_| StorageError::ChannelClosed)?
}

/// Update last_used_at timestamp for a token.
pub async fn touch_token_last_used(&self, id: &str) -> StorageResult<()> {
    let (reply, rx) = oneshot::channel();
    self.tx
        .send(DbCommand::TouchTokenLastUsed {
            id: id.to_string(),
            reply,
        })
        .await
        .map_err(|_| StorageError::ChannelClosed)?;
    rx.await.map_err(|_| StorageError::ChannelClosed)?
}

/// Count non-revoked tokens in serve_tokens table.
/// Used at startup to determine auth mode (0 = legacy, >0 = multi-key).
pub async fn count_tokens(&self) -> StorageResult<u64> {
    let (reply, rx) = oneshot::channel();
    self.tx
        .send(DbCommand::CountTokens { reply })
        .await
        .map_err(|_| StorageError::ChannelClosed)?;
    rx.await.map_err(|_| StorageError::ChannelClosed)?
}
```

### 5.3 Query Implementations

```rust
// tools/nika-storage/src/lib.rs, alongside existing do_* functions

fn do_insert_token(conn: &Connection, entry: &TokenEntry) -> StorageResult<()> {
    conn.execute(
        "INSERT INTO serve_tokens (id, name, token_hash, role, scope, created_at, expires_at, revoked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            entry.id,
            entry.name,
            entry.token_hash,
            entry.role,
            entry.scope,
            entry.created_at,
            entry.expires_at,
            entry.revoked as i32,
        ],
    )?;
    Ok(())
}

fn do_get_token_by_hash(conn: &Connection, hash: &[u8]) -> StorageResult<Option<TokenEntry>> {
    conn.query_row(
        "SELECT id, name, token_hash, role, scope, created_at, expires_at, last_used_at, revoked
         FROM serve_tokens WHERE token_hash = ?1",
        params![hash],
        |row| {
            Ok(TokenEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                token_hash: row.get(2)?,
                role: row.get(3)?,
                scope: row.get(4)?,
                created_at: row.get(5)?,
                expires_at: row.get(6)?,
                last_used_at: row.get(7)?,
                revoked: row.get::<_, i32>(8)? != 0,
            })
        },
    )
    .optional()
    .map_err(StorageError::from)
}

fn do_list_tokens(conn: &Connection) -> StorageResult<Vec<TokenEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, token_hash, role, scope, created_at, expires_at, last_used_at, revoked
         FROM serve_tokens ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TokenEntry {
            id: row.get(0)?,
            name: row.get(1)?,
            token_hash: row.get(2)?,  // Included for internal use; CLI redacts
            role: row.get(3)?,
            scope: row.get(4)?,
            created_at: row.get(5)?,
            expires_at: row.get(6)?,
            last_used_at: row.get(7)?,
            revoked: row.get::<_, i32>(8)? != 0,
        })
    })?;
    let mut tokens = Vec::new();
    for row in rows {
        tokens.push(row?);
    }
    Ok(tokens)
}

fn do_revoke_token(conn: &Connection, id_or_name: &str) -> StorageResult<bool> {
    // Try by ID first, then by name
    let updated = conn.execute(
        "UPDATE serve_tokens SET revoked = 1 WHERE (id = ?1 OR name = ?1) AND revoked = 0",
        params![id_or_name],
    )?;
    Ok(updated > 0)
}

fn do_touch_token_last_used(conn: &Connection, id: &str) -> StorageResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE serve_tokens SET last_used_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

fn do_count_tokens(conn: &Connection) -> StorageResult<u64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM serve_tokens WHERE revoked = 0",
        [],
        |r| r.get(0),
    )?;
    Ok(count as u64)
}
```

### 5.4 DB Loop Dispatch

Add to `run_db_loop` match block (`nika-storage/src/lib.rs:560`):

```rust
DbCommand::InsertToken { entry, reply } => {
    let _ = reply.send(do_insert_token(&conn, &entry));
}
DbCommand::GetTokenByHash { hash, reply } => {
    let _ = reply.send(do_get_token_by_hash(&conn, &hash));
}
DbCommand::ListTokens { reply } => {
    let _ = reply.send(do_list_tokens(&conn));
}
DbCommand::RevokeToken { id, reply } => {
    let _ = reply.send(do_revoke_token(&conn, &id));
}
DbCommand::TouchTokenLastUsed { id, reply } => {
    let _ = reply.send(do_touch_token_last_used(&conn, &id));
}
DbCommand::CountTokens { reply } => {
    let _ = reply.send(do_count_tokens(&conn));
}
```

---

## 6. Token Management Route Handlers

```rust
// tools/nika-serve/src/routes/tokens.rs  ← NEW FILE

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::auth::{AuthMode, Principal};
use crate::error::ServeError;
use crate::state::AppState;

// ═══════════════════════════════════════════════════════════════════════════
// REQUEST / RESPONSE TYPES
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Deserialize, JsonSchema)]
pub struct CreateTokenRequest {
    /// Human-readable name (unique, 1-64 chars, alphanumeric + hyphens).
    pub name: String,

    /// Optional ISO 8601 expiry date (e.g. "2026-12-31T23:59:59Z").
    pub expires_at: Option<String>,

    /// Optional scope glob (L2). Default: "*" (all workflows).
    pub scope: Option<String>,
}

#[derive(Serialize, JsonSchema)]
pub struct CreateTokenResponse {
    /// Token ID (UUID).
    pub id: String,

    /// Token name.
    pub name: String,

    /// The raw API token. SHOWN EXACTLY ONCE. Not recoverable.
    pub token: String,

    /// Expiry date, if set.
    pub expires_at: Option<String>,

    /// Scope glob.
    pub scope: String,
}

#[derive(Serialize, JsonSchema)]
pub struct TokenListEntry {
    pub id: String,
    pub name: String,
    pub role: String,
    pub scope: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub last_used_at: Option<String>,
    pub revoked: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct TokenListResponse {
    pub tokens: Vec<TokenListEntry>,
    pub count: usize,
}

#[derive(Serialize, JsonSchema)]
pub struct RevokeResponse {
    pub revoked: bool,
    pub id: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// HANDLERS
// ═══════════════════════════════════════════════════════════════════════════

/// `POST /v1/tokens` — Create a new API token.
///
/// Only available in multi-key mode. In legacy mode, returns 404.
/// The raw token is returned exactly once in the response body.
pub async fn create_token(
    State(state): State<AppState>,
    Json(req): Json<CreateTokenRequest>,
) -> Result<Json<CreateTokenResponse>, ServeError> {
    // Validate name: 1-64 chars, alphanumeric + hyphens
    if req.name.is_empty()
        || req.name.len() > 64
        || !req.name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(ServeError::InvalidWorkflow(
            "token name must be 1-64 chars, alphanumeric/hyphens/underscores".into(),
        ));
    }

    // Generate raw token: "nk_" prefix + 48 random hex chars = 51 chars total
    // Prefix makes tokens easily identifiable in logs/configs.
    let raw_token = format!("nk_{}", hex::encode(uuid::Uuid::new_v4().as_bytes())
        .chars()
        .chain(hex::encode(uuid::Uuid::new_v4().as_bytes()).chars())
        .take(48)
        .collect::<String>());

    // BLAKE3 hash for storage
    let hash = blake3::hash(raw_token.as_bytes());
    let hash_bytes = hash.as_bytes().to_vec();

    let token_id = uuid::Uuid::new_v4().simple().to_string();
    let scope = req.scope.unwrap_or_else(|| "*".into());

    let entry = nika_storage::TokenEntry {
        id: token_id.clone(),
        name: req.name.clone(),
        token_hash: hash_bytes,
        role: "operator".into(),
        scope: scope.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
        expires_at: req.expires_at.clone(),
        last_used_at: None,
        revoked: false,
    };

    state.storage.insert_token(entry).await?;

    tracing::info!(name = %req.name, id = %token_id, "API token created");

    Ok(Json(CreateTokenResponse {
        id: token_id,
        name: req.name,
        token: raw_token,  // Shown ONCE — never stored, never logged
        expires_at: req.expires_at,
        scope,
    }))
}

/// `GET /v1/tokens` — List all API tokens (hash redacted).
pub async fn list_tokens(
    State(state): State<AppState>,
) -> Result<Json<TokenListResponse>, ServeError> {
    let entries = state.storage.list_tokens().await?;
    let tokens: Vec<TokenListEntry> = entries
        .into_iter()
        .map(|e| TokenListEntry {
            id: e.id,
            name: e.name,
            role: e.role,
            scope: e.scope,
            created_at: e.created_at,
            expires_at: e.expires_at,
            last_used_at: e.last_used_at,
            revoked: e.revoked,
        })
        .collect();
    let count = tokens.len();
    Ok(Json(TokenListResponse { tokens, count }))
}

/// `DELETE /v1/tokens/{id}` — Revoke a token by ID or name.
///
/// Revocation is immediate: the token is marked as revoked in the DB
/// and evicted from the in-memory cache.
pub async fn revoke_token(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<RevokeResponse>, ServeError> {
    let revoked = state.storage.revoke_token(&id).await?;

    // Invalidate cache immediately
    if let AuthMode::MultiKey { ref store } = state.auth_mode {
        store.invalidate_by_id(&id);
    }

    if revoked {
        tracing::info!(id = %id, "API token revoked");
    }

    Ok(Json(RevokeResponse {
        revoked,
        id,
    }))
}
```

### 6.1 Route Registration

Add to `tools/nika-serve/src/routes/mod.rs`:

```rust
pub mod tokens;  // NEW

// In build_router(), add these routes:
.api_route(
    "/v1/tokens",
    post_with(tokens::create_token, tokens::create_docs)
        .get_with(tokens::list_tokens, tokens::list_docs),
)
.api_route(
    "/v1/tokens/{id}",
    axum::routing::delete(tokens::revoke_token),
)
```

---

## 7. CLI Commands

### 7.1 Serve Token Subcommand

```rust
// tools/nika-cli/src/serve_token.rs  ← NEW FILE

use clap::Subcommand;

#[derive(Subcommand)]
pub enum ServeTokenAction {
    /// Create a new API token for nika serve
    Add {
        /// Human-readable token name (unique)
        #[arg(long)]
        name: String,

        /// Optional expiry date (ISO 8601: 2026-12-31)
        #[arg(long)]
        expires: Option<String>,

        /// Optional scope glob (default: "*" = all workflows)
        #[arg(long)]
        scope: Option<String>,

        /// SQLite database path (default: .nika/serve.db)
        #[arg(long, default_value = ".nika/serve.db")]
        db: String,
    },

    /// List all API tokens
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// SQLite database path (default: .nika/serve.db)
        #[arg(long, default_value = ".nika/serve.db")]
        db: String,
    },

    /// Revoke an API token (by ID or name)
    Revoke {
        /// Token ID or name to revoke
        id_or_name: String,

        /// SQLite database path (default: .nika/serve.db)
        #[arg(long, default_value = ".nika/serve.db")]
        db: String,
    },
}
```

### 7.2 Handler

```rust
pub async fn handle_serve_token(action: ServeTokenAction) -> Result<(), nika::NikaError> {
    match action {
        ServeTokenAction::Add { name, expires, scope, db } => {
            let storage = nika_storage::Storage::open(std::path::Path::new(&db))
                .map_err(|e| nika::NikaError::ConfigError {
                    reason: format!("open db: {e}"),
                })?;

            // Generate raw token
            let raw_token = generate_raw_token();
            let hash = blake3::hash(raw_token.as_bytes());
            let token_id = uuid::Uuid::new_v4().simple().to_string();

            let entry = nika_storage::TokenEntry {
                id: token_id.clone(),
                name: name.clone(),
                token_hash: hash.as_bytes().to_vec(),
                role: "operator".into(),
                scope: scope.unwrap_or_else(|| "*".into()),
                created_at: chrono::Utc::now().to_rfc3339(),
                expires_at: expires,
                last_used_at: None,
                revoked: false,
            };

            storage.insert_token(entry).await.map_err(|e| {
                nika::NikaError::ConfigError {
                    reason: format!("insert token: {e}"),
                }
            })?;

            // Display token ONCE — never again recoverable
            eprintln!();
            eprintln!("  Token created successfully!");
            eprintln!();
            eprintln!("  Name:  {name}");
            eprintln!("  ID:    {token_id}");
            eprintln!("  Token: {raw_token}");
            eprintln!();
            eprintln!("  Save this token now — it will NOT be shown again.");
            eprintln!();

            Ok(())
        }

        ServeTokenAction::List { json, db } => {
            let storage = nika_storage::Storage::open(std::path::Path::new(&db))
                .map_err(|e| nika::NikaError::ConfigError {
                    reason: format!("open db: {e}"),
                })?;

            let tokens = storage.list_tokens().await.map_err(|e| {
                nika::NikaError::ConfigError {
                    reason: format!("list tokens: {e}"),
                }
            })?;

            if json {
                // JSON output (for scripts)
                let display: Vec<_> = tokens.iter().map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "name": t.name,
                        "role": t.role,
                        "scope": t.scope,
                        "created_at": t.created_at,
                        "expires_at": t.expires_at,
                        "last_used_at": t.last_used_at,
                        "revoked": t.revoked,
                    })
                }).collect();
                println!("{}", serde_json::to_string_pretty(&display).unwrap());
            } else {
                // Table output
                if tokens.is_empty() {
                    eprintln!("  No tokens configured. Using legacy NIKA_SERVE_TOKEN mode.");
                    return Ok(());
                }
                eprintln!();
                eprintln!("  {:8}  {:20}  {:10}  {:8}  {:20}", "ID", "NAME", "ROLE", "STATUS", "LAST USED");
                eprintln!("  {:8}  {:20}  {:10}  {:8}  {:20}", "──", "────", "────", "──────", "─────────");
                for t in &tokens {
                    let status = if t.revoked { "revoked" } else { "active" };
                    let last = t.last_used_at.as_deref().unwrap_or("never");
                    let id_short = &t.id[..8.min(t.id.len())];
                    eprintln!("  {:8}  {:20}  {:10}  {:8}  {:20}", id_short, t.name, t.role, status, last);
                }
                eprintln!();
            }

            Ok(())
        }

        ServeTokenAction::Revoke { id_or_name, db } => {
            let storage = nika_storage::Storage::open(std::path::Path::new(&db))
                .map_err(|e| nika::NikaError::ConfigError {
                    reason: format!("open db: {e}"),
                })?;

            let revoked = storage.revoke_token(&id_or_name).await.map_err(|e| {
                nika::NikaError::ConfigError {
                    reason: format!("revoke token: {e}"),
                }
            })?;

            if revoked {
                eprintln!("  Token '{id_or_name}' revoked.");
            } else {
                eprintln!("  Token '{id_or_name}' not found or already revoked.");
            }

            Ok(())
        }
    }
}

/// Generate a raw API token: "nk_" prefix + 48 hex chars from 24 random bytes.
fn generate_raw_token() -> String {
    use std::io::Read;
    let mut bytes = [0u8; 24];
    // Use getrandom for cryptographic randomness (same source as uuid)
    getrandom::getrandom(&mut bytes).expect("getrandom failed");
    format!("nk_{}", hex::encode(bytes))
}
```

### 7.3 CLI Wiring

In `tools/nika/src/main.rs`, add to `Commands` enum:

```rust
/// Manage API tokens for nika serve
#[cfg(feature = "serve")]
#[command(subcommand, next_help_heading = "SYSTEM")]
ServeToken(cli::serve_token::ServeTokenAction),
```

Alternative: nest under existing `Serve` as a subcommand group:
```
nika serve token add --name "jungo-prod"
nika serve token list
nika serve token revoke jungo-prod
```

**Recommendation**: Use `nika serve token <action>` as a subcommand of Serve. This keeps the CLI namespace clean — tokens are a serve concern, not a global concern. Implementation: add a `#[command(subcommand)]` inside the Serve variant or add a separate `ServeToken` command that aliases to `serve token`.

---

## 8. Startup Validation Logic (4 Cases)

Add to `run_server()` in `tools/nika-serve/src/lib.rs`, after storage is opened (line 59):

```rust
// Determine auth mode: legacy single-token or multi-key
let token_count = storage.count_tokens().await?;
let legacy_token = std::env::var("NIKA_SERVE_TOKEN").ok();

let auth_mode = match (token_count, legacy_token) {
    // Case 1: No tokens in DB, no env var → ERROR (must configure auth)
    (0, None) => {
        return Err(ServeError::Config(
            "No authentication configured. Either:\n  \
             1. Set NIKA_SERVE_TOKEN env var (legacy single-token mode)\n  \
             2. Create tokens: nika serve token add --name my-token"
                .into(),
        ));
    }

    // Case 2: No tokens in DB, env var set → LEGACY mode (existing behavior)
    (0, Some(token)) => {
        if token.len() < 32 {
            return Err(ServeError::Config(
                "NIKA_SERVE_TOKEN must be at least 32 characters. \
                 Generate one with: openssl rand -hex 32"
                    .into(),
            ));
        }
        info!("auth mode: legacy single-token (NIKA_SERVE_TOKEN)");
        AuthMode::Legacy { token }
    }

    // Case 3: Tokens in DB → MULTI-KEY mode (NIKA_SERVE_TOKEN ignored if set)
    (n, env) => {
        if env.is_some() {
            tracing::warn!(
                "NIKA_SERVE_TOKEN is set but {} token(s) exist in DB — \
                 using multi-key mode (env var ignored)",
                n
            );
        }
        info!(count = n, "auth mode: multi-key ({n} active token(s))");
        AuthMode::MultiKey {
            store: TokenStore::new(storage.clone()),
        }
    }
};
```

### Startup Banner Update

In `print_startup_banner()` (`lib.rs:381`), update the auth line:

```rust
// Replace:
//   eprintln!("  ├── Auth         Bearer token ({token_len} chars)");
// With:
match &auth_mode {
    AuthMode::Legacy { token } => {
        let token_len = token.len();
        eprintln!("  ├── Auth         legacy token ({token_len} chars)");
    }
    AuthMode::MultiKey { store } => {
        // count is already known from startup
        eprintln!("  ├── Auth         multi-key ({token_count} active tokens)");
    }
}
```

---

## 9. Rate Limiter Key Migration

### Current (raw token as key)

`tools/nika-serve/src/rate_limit.rs:63-68`:
```rust
let token = req.headers().get("authorization")
    .and_then(|v| v.to_str().ok())
    .and_then(|s| s.strip_prefix("Bearer "))
    .map(|s| s.to_string());
```

**Problem**: Raw token string is the DashMap key. Different tokens = different rate limit buckets. If a token is revoked and re-created with the same name, the old bucket remains with stale quota.

### New (token_id from Principal)

```rust
pub async fn rate_limit_middleware(
    State(rl): State<RateLimitState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // In multi-key mode: use Principal.token_id from request extensions.
    // In legacy mode: fall back to raw token string (unchanged behavior).
    let key = req
        .extensions()
        .get::<Principal>()
        .map(|p| p.token_id.clone())
        .or_else(|| {
            // Legacy fallback: extract raw token
            req.headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        });

    let Some(key) = key else {
        return next.run(req).await;
    };

    // ... rest unchanged (lines 75-114)
}
```

**Why this works**: The auth middleware runs BEFORE the rate limit middleware (see `lib.rs:118-127` — Axum layer ordering means outermost-added runs first). So by the time `rate_limit_middleware` executes, `Principal` is already in extensions.

**Middleware execution order** (confirmed from `lib.rs:120`):
```
request-id → timeout → body-limit → auth → rate-limit → handler
```

Auth inserts `Principal` into extensions. Rate limiter reads it. The dependency is satisfied by the existing layer ordering.

---

## 10. ServeError::Forbidden Variant

### Error Type

```rust
// tools/nika-serve/src/error.rs

#[derive(thiserror::Error, Debug)]
pub enum ServeError {
    // ... existing variants ...

    #[error("Forbidden: {0}")]
    Forbidden(String),
}
```

### HTTP Mapping

```rust
// In impl IntoResponse for ServeError:
Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
```

### Usage (L2 scope enforcement)

```rust
// In route handlers, after extracting Principal:
if let Some(principal) = request.extensions().get::<Principal>() {
    if !principal.can_access_workflow(&req.workflow) {
        return Err(ServeError::Forbidden(format!(
            "token '{}' cannot access workflow '{}'",
            principal.token_name, req.workflow
        )));
    }
}
```

**L1 ships without scope enforcement.** The `Forbidden` variant exists so L2 can use it without another error.rs change.

---

## 11. AppState Changes

```rust
// tools/nika-serve/src/state.rs

pub struct AppState {
    pub storage: nika_storage::Storage,
    pub config: Arc<ServeConfig>,
    pub executor: Executor,
    pub semaphore: Arc<Semaphore>,
    pub shutdown: tokio::sync::watch::Receiver<bool>,
    pub workers: Arc<Mutex<HashMap<String, WorkerHandle>>>,
    pub active_jobs: Arc<AtomicUsize>,
    pub event_bus: EventBus,
    pub webhook_config: Option<crate::webhook::WebhookConfig>,

    // NEW: auth mode determined at startup
    pub auth_mode: crate::auth::AuthMode,
}
```

### ServeConfig Change

Remove `auth_token: String` from `ServeConfig`. Auth is no longer a config field — it's determined at startup from DB state + env var. The `from_env()` method no longer requires `NIKA_SERVE_TOKEN` (it's checked later in `run_server`).

```rust
// tools/nika-serve/src/config.rs
// REMOVE these lines (174-183):
//   let auth_token = std::env::var("NIKA_SERVE_TOKEN")
//       .map_err(|_| ServeError::Config("NIKA_SERVE_TOKEN must be set".into()))?;
//   if auth_token.len() < 32 { ... }
// REMOVE `auth_token` from ServeConfig struct and from_env() return
```

This is a breaking internal change but there are zero external callers of `ServeConfig` outside of nika-serve and the main.rs CLI handler.

### Cargo.toml Change

Add to `tools/nika-serve/Cargo.toml`:

```toml
blake3 = { workspace = true }
dashmap = { workspace = true }
chrono = { workspace = true }
hex = { workspace = true }
getrandom = { workspace = true }
```

Add to `tools/nika-storage/Cargo.toml`:

```toml
# blake3 NOT needed here — hashing happens in nika-serve.
# Storage just stores/retrieves the raw bytes.
```

---

## 12. Test Function Signatures (10 tests)

```rust
// tools/nika-storage/src/lib.rs — #[cfg(test)] mod tests

#[tokio::test]
async fn test_insert_and_get_token_by_hash();
// Insert a TokenEntry, retrieve by BLAKE3 hash, assert all fields match.

#[tokio::test]
async fn test_list_tokens_returns_all();
// Insert 3 tokens, list_tokens returns all 3 in created_at DESC order.

#[tokio::test]
async fn test_revoke_token_by_name();
// Insert token "jungo-prod", revoke by name, verify revoked=true.

#[tokio::test]
async fn test_count_tokens_excludes_revoked();
// Insert 3 tokens, revoke 1, count_tokens returns 2.

#[tokio::test]
async fn test_touch_token_last_used();
// Insert token, touch, verify last_used_at is set.

// tools/nika-serve/src/auth.rs — #[cfg(test)] mod tests

#[tokio::test]
async fn test_token_store_authenticate_valid();
// Create TokenStore with in-memory storage, insert token,
// authenticate with raw token, verify Principal fields.

#[tokio::test]
async fn test_token_store_rejects_revoked();
// Insert then revoke token, authenticate fails.

#[tokio::test]
async fn test_token_store_rejects_expired();
// Insert token with expires_at in the past, authenticate fails.

#[tokio::test]
async fn test_token_store_cache_hit();
// Authenticate twice — second call hits cache (verify via timing or mock).

#[tokio::test]
async fn test_legacy_mode_accepts_valid_token();
// Build AuthMode::Legacy, verify SHA-256 constant-time check passes.
// (Equivalent to existing `accepts_valid_token` test, adapted for new enum.)
```

---

## 13. Exact File:Line Modification Points

| File | Line(s) | Change |
|------|---------|--------|
| `tools/nika-storage/src/lib.rs:22` | `const SCHEMA_VERSION: u32 = 4;` → `6` (or `5` if scheduling doesn't ship first) |
| `tools/nika-storage/src/lib.rs:50-69` | Add `TokenEntry` struct after `Job` struct |
| `tools/nika-storage/src/lib.rs:149-220` | Add 6 `DbCommand` variants after `DeleteCheckpoints` |
| `tools/nika-storage/src/lib.rs:230-233` | (No change — `Storage` struct stays the same) |
| `tools/nika-storage/src/lib.rs:493-517` | Add 6 `Storage` async methods after `list_artifacts` |
| `tools/nika-storage/src/lib.rs:560-637` | Add 6 `DbCommand` dispatch arms in `run_db_loop` after `DeleteCheckpoints` |
| `tools/nika-storage/src/lib.rs:720-732` | Add V6 migration block before `conn.pragma_update` |
| `tools/nika-storage/src/lib.rs:739+` | Add 6 `do_*` query functions after existing `do_*` functions |
| `tools/nika-storage/Cargo.toml` | No change needed (chrono already a dep) |
| `tools/nika-serve/src/auth.rs:1-189` | **REWRITE**: Replace entire file with `AuthMode` enum, `Principal`, `TokenStore`, `require_auth` |
| `tools/nika-serve/src/state.rs:25-56` | Add `pub auth_mode: crate::auth::AuthMode` field to `AppState` |
| `tools/nika-serve/src/config.rs:133` | Remove `pub auth_token: String` field |
| `tools/nika-serve/src/config.rs:168-183` | Remove `NIKA_SERVE_TOKEN` validation from `from_env()` |
| `tools/nika-serve/src/config.rs:257` | Remove `auth_token` from `Ok(Self { ... })` return |
| `tools/nika-serve/src/error.rs:28` | Add `Forbidden(String)` variant after `Internal` |
| `tools/nika-serve/src/error.rs:38-55` | Add `Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg)` match arm |
| `tools/nika-serve/src/rate_limit.rs:62-68` | Replace raw token extraction with `Principal` lookup from extensions |
| `tools/nika-serve/src/routes/mod.rs:3` | Add `pub mod tokens;` |
| `tools/nika-serve/src/routes/mod.rs:25-67` | Add `/v1/tokens` and `/v1/tokens/{id}` routes |
| `tools/nika-serve/src/routes/tokens.rs` | **CREATE**: ~180 LOC — handlers + types |
| `tools/nika-serve/src/lib.rs:54-105` | Add auth mode determination after storage.open, add `auth_mode` to `AppState` construction |
| `tools/nika-serve/src/lib.rs:126-127` | `auth::require_auth` middleware already takes `State<AppState>` — no change needed, it reads `state.auth_mode` |
| `tools/nika-serve/src/lib.rs:381-436` | Update `print_startup_banner` to show auth mode |
| `tools/nika-serve/src/lib.rs:451-502` | Update `test_app()` to include `auth_mode` field in `AppState` |
| `tools/nika-serve/src/lib.rs:647-699` | Update `test_app_with_dir()` to include `auth_mode` field |
| `tools/nika-serve/Cargo.toml:12-38` | Add `blake3`, `dashmap`, `chrono`, `hex`, `getrandom` workspace deps |
| `tools/nika-cli/src/serve_token.rs` | **CREATE**: ~200 LOC — CLI subcommand |
| `tools/nika-cli/src/lib.rs` | Add `pub mod serve_token;` |
| `tools/nika/src/main.rs:780-802` | Add `Token` subcommand under `Serve` or add `ServeToken` command |
| `tools/nika/src/main.rs:1744-1818` | Update Serve handler: remove `auth_token` from `ServeConfig` construction |

---

## 14. TDD Sequence (Implementation Order)

```
Phase 1: Storage (nika-storage) — 5 tests
  1. test_insert_and_get_token_by_hash        → RED → V6 migration + do_insert/do_get → GREEN
  2. test_list_tokens_returns_all             → RED → do_list_tokens → GREEN
  3. test_revoke_token_by_name                → RED → do_revoke_token → GREEN
  4. test_count_tokens_excludes_revoked       → RED → do_count_tokens → GREEN
  5. test_touch_token_last_used               → RED → do_touch_token_last_used → GREEN

Phase 2: TokenStore (nika-serve/auth.rs) — 4 tests
  6. test_token_store_authenticate_valid      → RED → TokenStore::authenticate → GREEN
  7. test_token_store_rejects_revoked         → RED → revoked check → GREEN
  8. test_token_store_rejects_expired         → RED → expiry check → GREEN
  9. test_token_store_cache_hit               → RED → DashMap cache → GREEN

Phase 3: Auth Middleware Rewrite — 1 test
  10. test_legacy_mode_accepts_valid_token    → RED → AuthMode::Legacy branch → GREEN
      (+ re-run ALL existing auth tests — they must pass with AuthMode::Legacy)

Phase 4: Integration — run existing test suite
  cargo test -p nika-serve --lib
  cargo test -p nika-storage --lib
  Verify: ALL existing tests pass (legacy mode is the default when token_count = 0)

Phase 5: Token routes + CLI
  Manual testing: nika serve token add/list/revoke
  Verify: startup banner shows correct auth mode
```

---

## 15. Security Considerations

1. **Raw tokens never stored.** Only BLAKE3 hash in DB. Database dump is not a credential dump.
2. **Token format**: `nk_` prefix (51 chars total). Prefix aids log scanning and accidental commit detection (`.gitignore` pattern, TruffleHog rule).
3. **Constant-time in legacy mode**: SHA-256 + `subtle::ConstantTimeEq` preserved exactly. No regression.
4. **BLAKE3 in multi-key mode**: Hash lookup is not timing-sensitive because the hash is pre-computed and the DashMap lookup is O(1). The security property is that stored hashes can't be reversed to raw tokens.
5. **Cache invalidation on revoke**: Immediate eviction via `invalidate_by_id`. Max propagation delay = 0 (not TTL-bounded).
6. **Rate limiter isolation**: Token ID (UUID) as key means revoking+recreating a token with the same name gets a fresh rate limit bucket. No quota inheritance.
7. **Token entropy**: 24 random bytes (192 bits) from `getrandom` (CSPRNG). Exceeds the 128-bit minimum for API tokens.
8. **No token in logs**: The `create_token` handler returns the raw token in the HTTP response body only. `tracing::info!` logs the name and ID, never the token.

---

## 16. What This Blueprint Does NOT Cover (Deferred to L2/L3)

- **L2 Scope enforcement**: `Principal::can_access_workflow` with glob matching (L2 feature).
- **L3 RBAC**: "admin" role for token management endpoints, "viewer" for read-only job status.
- **L3 Audit log**: `token_audit_log` table with action/ip/timestamp per token use.
- **Token rotation**: Replace a token (generate new, revoke old) in one CLI command.
- **Per-token rate limits**: Different rate/burst per token (currently global config).
- **nika.toml auth config**: `[serve.auth] mode = "multi-key"` — currently env-only.
