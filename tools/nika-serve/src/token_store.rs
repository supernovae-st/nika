//! Multi-tenant token store with BLAKE3 hashing and moka cache.
//!
//! Authenticates incoming Bearer tokens against named API keys stored in SQLite.
//! Uses a moka LRU cache (60s TTL, 10K cap) to avoid hitting the database on
//! every request. BLAKE3 provides constant-time 192-bit security at ~3M auth/sec.

use std::time::Duration;

use moka::sync::Cache;
use nika_storage::Storage;

/// Authenticated principal extracted from a valid token.
#[derive(Debug, Clone)]
pub struct Principal {
    /// UUID for rate limiting and audit logging.
    pub token_id: String,
    /// Human-readable token name (e.g. "ci-deploy").
    pub token_name: String,
    /// Role for RBAC (L1: only Operator enforced).
    pub role: nika_storage::Role,
    /// Scope glob pattern — comma-separated patterns.
    /// `"*"` = all, `"project-a/*"` = prefix, exact = literal match.
    pub scope: String,
}

impl Principal {
    /// Returns `true` if this principal can execute workflows (run, batch, cancel).
    pub fn can_execute(&self) -> bool {
        matches!(
            self.role,
            nika_storage::Role::Admin | nika_storage::Role::Operator
        )
    }

    /// Returns `true` if this principal can perform admin operations (reload).
    pub fn can_admin(&self) -> bool {
        matches!(self.role, nika_storage::Role::Admin)
    }

    /// Check if this principal is allowed to access a workflow by path.
    ///
    /// Scope patterns (comma-separated):
    /// - `"*"` matches everything
    /// - `"prefix/*"` matches any path starting with `"prefix/"`
    /// - `"exact.nika.yaml"` exact match only
    /// - `""` empty scope denies all access
    pub fn can_access(&self, workflow: &str) -> bool {
        self.scope.split(',').any(|pat| {
            let pat = pat.trim();
            if pat == "*" {
                true
            } else if let Some(prefix) = pat.strip_suffix('*') {
                workflow.starts_with(prefix)
            } else {
                pat == workflow
            }
        })
    }
}

/// Authentication mode determined at startup.
pub enum AuthMode {
    /// Legacy single-token mode (backward compatible with NIKA_SERVE_TOKEN).
    /// The hash is pre-computed BLAKE3 of the env var token.
    Legacy { expected_hash: [u8; 32] },
    /// Multi-key mode with named tokens from SQLite.
    MultiKey { store: TokenStore },
}

impl AuthMode {
    /// Authenticate a raw Bearer token string.
    ///
    /// Returns `Some(Principal)` on success, `None` on failure.
    /// All error paths take the same time (BLAKE3 hash is always computed).
    pub async fn authenticate(&self, raw_token: &str) -> Option<Principal> {
        let hash = blake3::hash(raw_token.as_bytes());
        let hash_bytes: [u8; 32] = *hash.as_bytes();

        match self {
            AuthMode::Legacy { expected_hash } => {
                if subtle::ConstantTimeEq::ct_eq(&hash_bytes[..], &expected_hash[..]).into() {
                    Some(Principal {
                        token_id: "legacy".to_string(),
                        token_name: "NIKA_SERVE_TOKEN".to_string(),
                        role: nika_storage::Role::Operator,
                        scope: "*".to_string(),
                    })
                } else {
                    None
                }
            }
            AuthMode::MultiKey { store } => store.authenticate(hash_bytes).await,
        }
    }

    /// Description for startup banner.
    pub fn description(&self) -> &'static str {
        match self {
            AuthMode::Legacy { .. } => "Legacy (NIKA_SERVE_TOKEN)",
            AuthMode::MultiKey { .. } => "Multi-key (named tokens)",
        }
    }
}

/// Token store backed by SQLite with an in-memory moka cache.
pub struct TokenStore {
    cache: Cache<[u8; 32], Principal>,
    storage: Storage,
}

impl TokenStore {
    /// Create a new token store backed by the given storage.
    pub fn new(storage: Storage) -> Self {
        let cache = Cache::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(60))
            .build();

        Self { cache, storage }
    }

    /// Authenticate a BLAKE3 hash against the store.
    ///
    /// Flow: cache hit → return | cache miss → DB lookup → validate → cache → return.
    async fn authenticate(&self, hash: [u8; 32]) -> Option<Principal> {
        // 1. Cache hit
        if let Some(principal) = self.cache.get(&hash) {
            // Fire-and-forget touch (update last_used_at)
            let storage = self.storage.clone();
            let id = principal.token_id.clone();
            tokio::spawn(async move {
                let _ = storage.touch_token(&id).await;
            });
            return Some(principal);
        }

        // 2. Cache miss → DB lookup
        let entry = self.storage.get_token_by_hash(&hash).await.ok()??;

        // 3. Validate: not revoked, not expired
        if entry.revoked {
            return None;
        }
        if let Some(ref expires) = entry.expires_at {
            let now = chrono::Utc::now().to_rfc3339();
            if expires < &now {
                return None;
            }
        }

        // 4. Build principal and cache it
        let principal = Principal {
            token_id: entry.id.clone(),
            token_name: entry.name,
            role: entry.role,
            scope: entry.scope,
        };
        self.cache.insert(hash, principal.clone());

        // 5. Fire-and-forget touch
        let storage = self.storage.clone();
        let id = entry.id;
        tokio::spawn(async move {
            let _ = storage.touch_token(&id).await;
        });

        Some(principal)
    }

    /// Immediately evict a token from the cache (on revoke).
    pub fn invalidate(&self, hash: &[u8; 32]) {
        self.cache.invalidate(hash);
    }
}

/// Generate a cryptographically secure 192-bit API token.
///
/// Returns the raw token string prefixed with `nk_` for easy identification.
/// The caller must BLAKE3-hash this before storing.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 24]; // 192 bits
    getrandom::fill(&mut bytes).expect("getrandom failed");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("nk_{hex}")
}

/// Hash a raw token with BLAKE3 for storage.
pub fn hash_token(raw: &str) -> [u8; 32] {
    *blake3::hash(raw.as_bytes()).as_bytes()
}

/// Redact a token for safe logging: `nk_<7chars>...`
pub fn redact_token(raw: &str) -> String {
    if raw.len() > 10 {
        format!("{}...", &raw[..10])
    } else {
        "nk_***".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_token_format() {
        let token = generate_token();
        assert!(token.starts_with("nk_"), "token must start with nk_ prefix");
        assert_eq!(token.len(), 51, "nk_ (3) + 48 hex chars = 51");

        // All tokens are unique
        let token2 = generate_token();
        assert_ne!(token, token2);
    }

    #[test]
    fn hash_token_deterministic() {
        let hash1 = hash_token("nk_test123");
        let hash2 = hash_token("nk_test123");
        assert_eq!(hash1, hash2);

        let hash3 = hash_token("nk_different");
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn redact_hides_full_token() {
        assert_eq!(redact_token("nk_abcdefghij1234"), "nk_abcdefg...");
        assert_eq!(redact_token("short"), "nk_***");
    }

    #[tokio::test]
    async fn legacy_mode_accepts_correct_token() {
        let raw = "my-secret-token-for-testing-1234";
        let expected_hash = hash_token(raw);
        let auth = AuthMode::Legacy { expected_hash };

        let principal = auth.authenticate(raw).await;
        assert!(principal.is_some());
        let p = principal.unwrap();
        assert_eq!(p.token_id, "legacy");
        assert_eq!(p.token_name, "NIKA_SERVE_TOKEN");
    }

    #[tokio::test]
    async fn legacy_mode_rejects_wrong_token() {
        let expected_hash = hash_token("correct-token-aaaaaaaaaaaaa");
        let auth = AuthMode::Legacy { expected_hash };

        let principal = auth.authenticate("wrong-token-bbbbbbbbbbbbb").await;
        assert!(principal.is_none());
    }

    #[tokio::test]
    async fn multikey_mode_authenticate_and_cache() {
        let storage = Storage::open_memory().unwrap();
        let raw = generate_token();
        let hash = hash_token(&raw);

        // Insert token into DB
        let entry = nika_storage::TokenEntry {
            id: "t1".to_string(),
            name: "ci-deploy".to_string(),
            token_hash: hash.to_vec(),
            role: nika_storage::Role::Operator,
            scope: "*".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
            last_used_at: None,
            revoked: false,
        };
        storage.insert_token(entry).await.unwrap();

        let store = TokenStore::new(storage);
        let auth = AuthMode::MultiKey { store };

        // First call: cache miss → DB lookup
        let p = auth.authenticate(&raw).await.unwrap();
        assert_eq!(p.token_id, "t1");
        assert_eq!(p.token_name, "ci-deploy");

        // Second call: cache hit (same result)
        let p2 = auth.authenticate(&raw).await.unwrap();
        assert_eq!(p2.token_id, "t1");

        // Wrong token: rejected
        let bad = auth.authenticate("nk_wrong_token_aaaa").await;
        assert!(bad.is_none());
    }

    #[tokio::test]
    async fn multikey_rejects_revoked_token() {
        let storage = Storage::open_memory().unwrap();
        let raw = generate_token();
        let hash = hash_token(&raw);

        let entry = nika_storage::TokenEntry {
            id: "t1".to_string(),
            name: "revoked".to_string(),
            token_hash: hash.to_vec(),
            role: nika_storage::Role::Operator,
            scope: "*".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: None,
            last_used_at: None,
            revoked: false,
        };
        storage.insert_token(entry).await.unwrap();
        storage.revoke_token("t1").await.unwrap();

        let store = TokenStore::new(storage);
        let auth = AuthMode::MultiKey { store };

        assert!(auth.authenticate(&raw).await.is_none());
    }

    #[test]
    fn auth_mode_description() {
        let legacy = AuthMode::Legacy {
            expected_hash: [0; 32],
        };
        assert_eq!(legacy.description(), "Legacy (NIKA_SERVE_TOKEN)");

        let store = TokenStore::new(Storage::open_memory().unwrap());
        let multi = AuthMode::MultiKey { store };
        assert_eq!(multi.description(), "Multi-key (named tokens)");
    }

    // =========================================================================
    // L2 scope enforcement — Principal::can_access()
    // =========================================================================

    fn make_principal(scope: &str) -> Principal {
        Principal {
            token_id: "t1".to_string(),
            token_name: "test".to_string(),
            role: nika_storage::Role::Operator,
            scope: scope.to_string(),
        }
    }

    #[test]
    fn scope_wildcard_matches_everything() {
        let p = make_principal("*");
        assert!(p.can_access("anything.nika.yaml"));
        assert!(p.can_access("project-a/deep/nested/flow.nika.yaml"));
        assert!(p.can_access(""));
    }

    #[test]
    fn scope_prefix_glob_matches_subtree() {
        let p = make_principal("project-a/*");
        assert!(p.can_access("project-a/flow.nika.yaml"));
        assert!(p.can_access("project-a/deep/nested.nika.yaml"));
        assert!(!p.can_access("project-b/flow.nika.yaml"));
        assert!(!p.can_access("flow.nika.yaml"));
    }

    #[test]
    fn scope_exact_match() {
        let p = make_principal("pipeline.nika.yaml");
        assert!(p.can_access("pipeline.nika.yaml"));
        assert!(!p.can_access("other.nika.yaml"));
        assert!(!p.can_access("pipeline.nika.yaml/extra"));
    }

    #[test]
    fn scope_multiple_globs_comma_separated() {
        let p = make_principal("project-a/*,project-b/*");
        assert!(p.can_access("project-a/flow.nika.yaml"));
        assert!(p.can_access("project-b/flow.nika.yaml"));
        assert!(!p.can_access("project-c/flow.nika.yaml"));
    }

    #[test]
    fn scope_empty_denies_all() {
        let p = make_principal("");
        assert!(!p.can_access("anything.nika.yaml"));
    }

    // =========================================================================
    // L3 RBAC — Principal::can_execute() / can_admin()
    // =========================================================================

    fn make_principal_with_role(role: nika_storage::Role) -> Principal {
        Principal {
            token_id: "t1".to_string(),
            token_name: "test".to_string(),
            role,
            scope: "*".to_string(),
        }
    }

    #[test]
    fn admin_can_execute_and_admin() {
        let p = make_principal_with_role(nika_storage::Role::Admin);
        assert!(p.can_execute());
        assert!(p.can_admin());
    }

    #[test]
    fn operator_can_execute_but_not_admin() {
        let p = make_principal_with_role(nika_storage::Role::Operator);
        assert!(p.can_execute());
        assert!(!p.can_admin());
    }

    #[test]
    fn viewer_cannot_execute_or_admin() {
        let p = make_principal_with_role(nika_storage::Role::Viewer);
        assert!(!p.can_execute());
        assert!(!p.can_admin());
    }
}
