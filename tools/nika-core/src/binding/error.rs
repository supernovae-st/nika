// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binding resolver errors.
//!
//! `BindingResolveError` is the pure-core error type returned by
//! `binding/resolve.rs`. The engine provides `From<BindingResolveError> for
//! NikaError` so downstream callers (that still speak `NikaError`) keep
//! working unchanged.
//!
//! # Why this exists (S23)
//!
//! Before S23, the resolver returned `NikaError` (engine-level enum) and
//! `error_domains::BindingError`, both of which live in `nika-engine`.
//! Moving `resolve.rs` to `nika-core` required a core-level error type.
//!
//! The variants preserve the existing NIKA-XXX codes and Display format
//! **byte-for-byte** so on-the-wire error strings are unchanged — this is
//! what the engine `From` impl relies on for Display parity goldens.
//!
//! # Variants
//!
//! | Variant | NIKA code | Engine mapping |
//! |---------|-----------|---------------|
//! | `PathNotFound` | 052 | `NikaError::PathNotFound` |
//! | `NullValue` | 072 | `NikaError::NullValue` |
//! | `VaultAccess` | 075 | `NikaError::VaultAccess` |
//! | `NotFound` | 042 | `NikaError::BindingNotFound` (via `BindingError::NotFound`) |
//! | `TypeMismatch` | 043 | `NikaError::BindingTypeMismatch` (via `BindingError::TypeMismatch`) |

use thiserror::Error;

/// Errors raised by the binding resolver (`binding/resolve.rs`).
///
/// Intentionally minimal — one variant per failure mode in the resolver.
/// Engine code converts to `NikaError` at the API boundary via the
/// `From<BindingResolveError>` impl in `nika-engine::error`.
#[derive(Debug, Error, PartialEq, Eq, Clone)]
#[non_exhaustive]
pub enum BindingResolveError {
    /// NIKA-052: a binding path (e.g. `task.field.sub`) did not resolve.
    #[error("[NIKA-052] Path '{path}' not found (task may not have JSON output)")]
    PathNotFound { path: String },

    /// NIKA-072: a required binding value was `null` in strict mode
    /// (no default provided, no null-tolerant transform in the chain).
    #[error("[NIKA-072] Null value at path '{path}' (strict mode)")]
    NullValue { path: String, alias: String },

    /// NIKA-075: `$vault.service.field` lookup failed at the vault layer
    /// (I/O, decryption, or malformed entry).
    #[error("[NIKA-075] Vault access failed for $vault.{service}.{field}: {reason}")]
    VaultAccess {
        service: String,
        field: String,
        reason: String,
    },

    /// NIKA-042: alias declared in `with:` was never resolved — typically
    /// hit when a loop variable was not pre-resolved, or a source is
    /// structurally missing.
    #[error("[NIKA-042] Binding '{alias}' not found")]
    NotFound { alias: String },

    /// NIKA-043: resolved value did not match the declared `type:` in
    /// the full-form `with:` entry.
    #[error("[NIKA-043] Binding type mismatch at '{path}': expected {expected}, got {actual}")]
    TypeMismatch {
        path: String,
        expected: String,
        actual: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_format_locked() {
        // Byte-for-byte parity with engine NikaError messages. The engine's
        // From<BindingResolveError> impl relies on this.
        assert_eq!(
            BindingResolveError::PathNotFound { path: "a.b".into() }.to_string(),
            "[NIKA-052] Path 'a.b' not found (task may not have JSON output)"
        );
        assert_eq!(
            BindingResolveError::NullValue {
                path: "a.b".into(),
                alias: "x".into(),
            }
            .to_string(),
            "[NIKA-072] Null value at path 'a.b' (strict mode)"
        );
        assert_eq!(
            BindingResolveError::VaultAccess {
                service: "stripe".into(),
                field: "key".into(),
                reason: "io".into(),
            }
            .to_string(),
            "[NIKA-075] Vault access failed for $vault.stripe.key: io"
        );
        assert_eq!(
            BindingResolveError::NotFound {
                alias: "data".into(),
            }
            .to_string(),
            "[NIKA-042] Binding 'data' not found"
        );
        assert_eq!(
            BindingResolveError::TypeMismatch {
                path: "x".into(),
                expected: "string".into(),
                actual: "integer".into(),
            }
            .to_string(),
            "[NIKA-043] Binding type mismatch at 'x': expected string, got integer"
        );
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BindingResolveError>();
    }
}
