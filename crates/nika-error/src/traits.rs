// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core trait that all Nika error enums implement.

use crate::codes::NikaCode;

/// Helper for trait-object downcasting.
///
/// Blanket-implemented for all `'static` types. Used as a supertrait of
/// [`NikaErrorCode`] so [`NikaError::downcast_ref`](crate::NikaError::downcast_ref)
/// works without per-impl boilerplate.
pub trait AsAny: 'static {
    /// Obtain a `&dyn Any` for downcasting.
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<T: 'static> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Contract for all error types in the Nika diamond.
///
/// Each crate defines its own error enum and implements this trait.
/// The unified [`NikaError`](crate::NikaError) wraps `Box<dyn NikaErrorCode>`.
///
/// # Method naming
///
/// The method is called `nika_code()` (not `code()`) to avoid conflict
/// with [`miette::Diagnostic::code()`] which returns `Option<Box<dyn DiagnosticCode>>`.
///
/// # Example
///
/// ```rust
/// use nika_error::prelude::*;
///
/// let err = CoreError::NotFound { what: "model gpt-5".into() };
/// assert_eq!(err.nika_code(), codes::NIKA_002);
/// assert!(!err.is_transient());
/// ```
pub trait NikaErrorCode:
    std::error::Error + miette::Diagnostic + AsAny + Send + Sync + 'static
{
    /// The structured NIKA-XXX code for this error.
    fn nika_code(&self) -> NikaCode;

    /// The USER-FACING **spec** code — the `NIKA-<NS>-<NNN>` form an author
    /// writes in `retry.on_codes` / `on_error.on_codes` and reads at
    /// `tasks.X.error.code` (spec `05-errors.md`).
    ///
    /// Defaults to the engine registry's numeric wire form
    /// ([`nika_code`](Self::nika_code)`.to_string()` · `NIKA-440`). A crate
    /// whose registry code has a DISTINCT spec namespace row (e.g. the verb
    /// errors · `NIKA-440` → `NIKA-EXEC-001`) overrides this so the
    /// `on_codes` matcher compares the same identifier the author is forced
    /// (by `nika check`) to write. The two stay reconcilable: both forms
    /// resolve through `nika explain`.
    ///
    /// **Why a method, not a field on [`NikaCode`]:** the mapping is
    /// per-error-VARIANT, not per-numeric-code (a future variant could share
    /// a numeric range yet carry a different spec row), and it keeps the
    /// numeric registry a pure value type.
    fn spec_code(&self) -> String {
        self.nika_code().to_string()
    }

    /// Whether retrying the same operation may succeed.
    ///
    /// Defaults to `false`. Override for transient errors (network timeout,
    /// rate limit, etc.) to enable retry logic in upper layers via `backon`.
    fn is_transient(&self) -> bool {
        false
    }

    /// Hash for in-process deduplication and grouping.
    ///
    /// Default hashes the numeric code. Override for finer grouping
    /// (e.g., include the URL for HTTP errors).
    ///
    /// **Not stable across Rust versions or process restarts.**
    /// Uses `DefaultHasher` which has no cross-version guarantees.
    /// Do not persist or compare across binaries.
    fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.nika_code().num.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;

    // Minimal test impl to verify trait defaults
    #[derive(Debug, thiserror::Error, miette::Diagnostic)]
    #[error("test error")]
    struct TestError;

    impl NikaErrorCode for TestError {
        fn nika_code(&self) -> NikaCode {
            codes::NIKA_999
        }
    }

    #[test]
    fn default_is_transient_returns_false() {
        let e = TestError;
        assert!(!e.is_transient());
    }

    #[test]
    fn default_fingerprint_is_deterministic() {
        let e = TestError;
        let fp1 = e.fingerprint();
        let fp2 = e.fingerprint();
        assert_eq!(fp1, fp2);
        assert_ne!(fp1, 0);
    }

    #[test]
    fn as_any_enables_downcast() {
        let e = TestError;
        let any_ref = e.as_any();
        assert!(any_ref.downcast_ref::<TestError>().is_some());
    }

    #[test]
    fn fingerprint_differs_for_different_codes() {
        #[derive(Debug, thiserror::Error, miette::Diagnostic)]
        #[error("other")]
        struct OtherError;

        impl NikaErrorCode for OtherError {
            fn nika_code(&self) -> NikaCode {
                codes::NIKA_001
            }
        }

        let e1 = TestError; // NIKA_999
        let e2 = OtherError; // NIKA_001
        assert_ne!(
            e1.fingerprint(),
            e2.fingerprint(),
            "different codes should produce different fingerprints"
        );
    }

    #[test]
    fn fingerprint_is_not_trivially_one() {
        let e = TestError;
        // Default fingerprint hashes the code num, should not be 1
        assert_ne!(e.fingerprint(), 1, "fingerprint should not be trivially 1");
    }
}
