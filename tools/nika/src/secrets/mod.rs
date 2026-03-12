//! Unified secrets management (v0.27 - migrated from spn-client).
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  SECRETS MODULE                                                             │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  With spn-daemon feature (RECOMMENDED):                                     │
//! │                                                                             │
//! │  Nika Process                                                               │
//! │      │                                                                      │
//! │      └── spn-client (Unix socket IPC)                                       │
//! │          │                                                                  │
//! │          └── nika daemon (~/.nika/daemon.sock)                              │
//! │              │                                                              │
//! │              └── OS Keychain (SOLE accessor, no popups)                     │
//! │                                                                             │
//! │  Without spn-daemon feature (fallback):                                     │
//! │                                                                             │
//! │  Nika Process                                                               │
//! │      │                                                                      │
//! │      └── Direct access (causes macOS popups!)                               │
//! │          ├── OS Keychain (keyring crate)                                    │
//! │          └── Environment variables                                          │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## v0.27 Migration (spn → nika fusion)
//!
//! Provider definitions moved from `spn-client::KNOWN_PROVIDERS` to `nika::core::KNOWN_PROVIDERS`.
//! The daemon IPC functionality (`spn-client::SpnClient`) remains external.
//!
//! ## v0.22 Behavior
//!
//! When `spn-daemon` feature is enabled, Nika will NEVER access the keychain directly.
//! The daemon is the SOLE keychain accessor. If the daemon doesn't have a secret,
//! it's marked as "not found" instead of falling back to direct keyring access.
//!
//! This completely eliminates macOS Keychain popup fatigue.
//!
//! ## Usage
//!
//! ```ignore
//! use nika::secrets::{load_from_daemon_or_fallback, get_secret, has_secret};
//!
//! // Load all provider secrets at startup
//! let result = load_from_daemon_or_fallback().await;
//! println!("Loaded: {}", result.summary());
//!
//! // Get a specific secret
//! if let Some(key) = get_secret("anthropic").await {
//!     // Use the key
//! }
//!
//! // Check if secret exists
//! if has_secret("openai").await {
//!     // Provider is configured
//! }
//! ```

mod result;

#[cfg(feature = "spn-daemon")]
mod daemon;

#[cfg(not(feature = "spn-daemon"))]
mod fallback;

// Re-export result type (always available)
pub use result::SecretsLoadResult;

// Re-export functions based on feature
#[cfg(feature = "spn-daemon")]
pub use daemon::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};

#[cfg(not(feature = "spn-daemon"))]
pub use fallback::{daemon_available, get_secret, has_secret, load_from_daemon_or_fallback};

// ═══════════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS (always available)
// ═══════════════════════════════════════════════════════════════════════════════

/// Get the environment variable name for a provider ID.
///
/// Uses `nika::core::KNOWN_PROVIDERS` as the source of truth.
///
/// # Example
///
/// ```
/// use nika::secrets::provider_env_var;
///
/// assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
/// assert_eq!(provider_env_var("neo4j"), "NEO4J_PASSWORD");
/// assert_eq!(provider_env_var("unknown"), "UNKNOWN_API_KEY");
/// ```
pub fn provider_env_var(provider: &str) -> &'static str {
    crate::core::provider_to_env_var(provider).unwrap_or("UNKNOWN_API_KEY")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_env_var_lookup() {
        // Test that provider_env_var returns expected values for known providers
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("native"), "NIKA_NATIVE_MODEL_PATH");
        assert_eq!(provider_env_var("neo4j"), "NEO4J_PASSWORD");
        assert_eq!(provider_env_var("github"), "GITHUB_TOKEN");
    }

    #[test]
    fn test_provider_env_var_unknown() {
        assert_eq!(provider_env_var("nonexistent"), "UNKNOWN_API_KEY");
    }

    #[test]
    fn test_daemon_available_check() {
        // Without daemon running, should return false
        #[cfg(not(feature = "spn-daemon"))]
        assert!(!daemon_available());

        #[cfg(feature = "spn-daemon")]
        {
            // With feature, checks if socket exists
            let result = daemon_available();
            // Result depends on whether daemon is actually running
            let _ = result;
        }
    }
}
