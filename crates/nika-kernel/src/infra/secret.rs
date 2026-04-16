// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Secret resolution trait for API key management.
//!
//! Kernel never sees plaintext at construction. `SecretRef` is the public
//! reference (safe to log); `Secret` carries the resolved value with
//! zeroize-on-drop and a redacted Debug impl.

use std::fmt;

use nika_error::NikaError;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Reference to a secret (not the value itself).
///
/// Typically `"$env.ANTHROPIC_API_KEY"` or a vault path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SecretRef {
    /// The reference string (safe to log).
    pub reference: String,
}

impl SecretRef {
    /// Create a new secret reference.
    #[must_use]
    pub fn new(reference: impl Into<String>) -> Self {
        Self {
            reference: reference.into(),
        }
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Intentionally show only the ref, never the resolved value.
        write!(f, "SecretRef({})", self.reference)
    }
}

/// A resolved secret value. Zeroized on drop.
///
/// The `Debug` impl is redacted to prevent accidental logging.
pub struct Secret(Zeroizing<String>);

impl Secret {
    /// Create a secret from a string value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// Access the secret value.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

/// Resolve secret references to actual values.
///
/// Production: env vars, vault, SSM, etc.
/// Tests: `NullSecretResolver` returns the reference as-is.
#[trait_variant::make(SecretResolverDyn: Send)]
pub trait SecretResolver: Send + Sync + crate::sealed::Sealed {
    /// Resolve a secret reference to its value.
    ///
    /// CANCEL SAFETY: cancel-safe — resolution is idempotent and read-only.
    /// A dropped future never cached nor exposed the plaintext value, and
    /// the underlying secret store is unaffected. Retry is always safe.
    async fn resolve(&self, secret: &SecretRef) -> Result<Secret, NikaError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_ref_display_does_not_leak() {
        let r = SecretRef::new("$env.API_KEY");
        let display = r.to_string();
        assert!(display.contains("$env.API_KEY"));
        assert!(!display.contains("sk-"));
    }

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new("sk-super-secret-key");
        let debug = format!("{s:?}");
        assert_eq!(debug, "Secret(***)");
        assert!(!debug.contains("sk-super-secret"));
    }

    #[test]
    fn secret_expose() {
        let s = Secret::new("my-key");
        assert_eq!(s.expose(), "my-key");
    }

    #[test]
    fn secret_clone() {
        let s = Secret::new("key");
        let c = s.clone();
        assert_eq!(c.expose(), "key");
    }

    fn _assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn secret_types_are_send_sync() {
        _assert_send_sync::<SecretRef>();
        _assert_send_sync::<Secret>();
    }
}
