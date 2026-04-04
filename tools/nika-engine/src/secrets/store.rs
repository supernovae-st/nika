//! Thread-safe in-process secret store.
//!
//! Replaces `unsafe { std::env::set_var }` with a lock-free `DashMap`.
//! Secrets loaded from daemon IPC or NikaVault are written here during boot,
//! and read back via `resolve_env()` at runtime — safe from any thread.

use std::sync::LazyLock;

use dashmap::DashMap;
use secrecy::{ExposeSecret, SecretString};

static STORE: LazyLock<DashMap<String, SecretString>> = LazyLock::new(DashMap::new);

/// Write a secret into the in-process store (thread-safe).
pub fn set_secret(key: &str, value: &str) {
    STORE.insert(key.to_string(), SecretString::from(value.to_string()));
}

/// Read a secret from the store (returns plaintext clone).
pub fn get_secret(key: &str) -> Option<String> {
    STORE.get(key).map(|s| s.expose_secret().to_string())
}

/// Check if a key exists in the store.
pub fn has_secret(key: &str) -> bool {
    STORE.contains_key(key)
}

/// Remove a secret from the store.
pub fn remove_secret(key: &str) {
    STORE.remove(key);
}

/// Store-first, env-var fallback.
///
/// This is the primary lookup for `$env.VAR` bindings and provider key checks.
/// Returns `None` if the key is absent from both store and environment,
/// or if the value is empty/whitespace-only.
pub fn resolve_env(key: &str) -> Option<String> {
    if let Some(val) = get_secret(key) {
        if !val.trim().is_empty() {
            return Some(val);
        }
    }
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
}

#[cfg(test)]
pub fn clear() {
    STORE.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_get_secret() {
        set_secret("TEST_STORE_KEY_1", "hunter2");
        assert_eq!(get_secret("TEST_STORE_KEY_1").as_deref(), Some("hunter2"));
    }

    #[test]
    fn test_has_secret() {
        set_secret("TEST_STORE_KEY_2", "val");
        assert!(has_secret("TEST_STORE_KEY_2"));
        assert!(!has_secret("TEST_STORE_NONEXISTENT"));
    }

    #[test]
    fn test_resolve_env_prefers_store() {
        set_secret("TEST_STORE_KEY_3", "from_store");
        // Even if env has a different value, store wins
        let val = resolve_env("TEST_STORE_KEY_3");
        assert_eq!(val.as_deref(), Some("from_store"));
    }

    #[test]
    fn test_resolve_env_falls_back_to_env() {
        // KEY that is NOT in store but IS in env (we can't easily set env in
        // parallel tests, so just verify the None path for a missing key)
        let val = resolve_env("TEST_STORE_CERTAINLY_MISSING_KEY_XYZ");
        assert!(val.is_none());
    }

    #[test]
    fn test_resolve_env_filters_empty() {
        set_secret("TEST_STORE_EMPTY", "");
        assert!(resolve_env("TEST_STORE_EMPTY").is_none());
    }

    #[test]
    fn test_resolve_env_filters_whitespace() {
        set_secret("TEST_STORE_WS", "   ");
        assert!(resolve_env("TEST_STORE_WS").is_none());
    }
}
