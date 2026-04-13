// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unified error wrapper: `NikaError(Box<dyn NikaErrorCode>)`.
//!
//! The blanket `impl<E: NikaErrorCode> From<E> for NikaError` gives every
//! per-crate error enum free conversion into the unified type. Downstream
//! code uses `NikaResult<T>` (= `Result<T, NikaError>`) as the standard
//! error boundary.

use crate::codes::NikaCode;
use crate::traits::NikaErrorCode;

/// Unified error type for the Nika diamond.
///
/// Wraps any `dyn NikaErrorCode` behind a `Box`. Constructed via the
/// blanket [`From`] impl or [`NikaError::new`].
///
/// # Downcasting
///
/// Use [`downcast_ref`](NikaError::downcast_ref) to recover the concrete type:
///
/// ```rust
/// use nika_error::prelude::*;
///
/// let err: NikaError = CoreError::NotFound { what: "x".into() }.into();
/// if let Some(core) = err.downcast_ref::<CoreError>() {
///     assert!(matches!(core, CoreError::NotFound { .. }));
/// }
/// ```
pub struct NikaError(Box<dyn NikaErrorCode>);

/// Convenience alias for `Result<T, NikaError>`.
pub type NikaResult<T> = Result<T, NikaError>;

impl NikaError {
    /// Wrap any [`NikaErrorCode`] implementor.
    pub fn new<E: NikaErrorCode>(e: E) -> Self {
        Self(Box::new(e))
    }

    /// The structured NIKA-XXX code.
    #[must_use]
    pub fn nika_code(&self) -> NikaCode {
        self.0.nika_code()
    }

    /// Whether retrying the same operation may succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        self.0.is_transient()
    }

    /// Stable hash for deduplication.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        self.0.fingerprint()
    }

    /// Attempt to downcast to a concrete error type.
    ///
    /// Returns `Some(&E)` if the inner error is of type `E`, `None` otherwise.
    #[must_use]
    pub fn downcast_ref<E: NikaErrorCode>(&self) -> Option<&E> {
        // Explicit deref to avoid method resolution picking
        // <Box<dyn NikaErrorCode> as AsAny>::as_any (which returns
        // TypeId of the Box itself, not the inner concrete type).
        let inner: &dyn NikaErrorCode = &*self.0;
        inner.as_any().downcast_ref::<E>()
    }
}

impl<E: NikaErrorCode> From<E> for NikaError {
    fn from(e: E) -> Self {
        Self::new(e)
    }
}

impl std::fmt::Display for NikaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.nika_code(), self.0)
    }
}

impl std::fmt::Debug for NikaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NikaError({}: {:?})", self.0.nika_code(), self.0)
    }
}

impl std::error::Error for NikaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl miette::Diagnostic for NikaError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.code()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.help()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.0.severity()
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        self.0.labels()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.0.source_code()
    }

    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        self.0.diagnostic_source()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.0.url()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn miette::Diagnostic> + 'a>> {
        self.0.related()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;
    use crate::core_error::CoreError;

    #[test]
    fn from_core_error_validation() {
        let core = CoreError::Validation {
            reason: "missing field".into(),
        };
        let nika: NikaError = core.into();
        assert_eq!(nika.nika_code(), codes::NIKA_001);
        assert!(!nika.is_transient());
    }

    #[test]
    fn from_core_error_internal() {
        let core = CoreError::Internal {
            context: "serde".into(),
            detail: "unexpected EOF".into(),
        };
        let nika: NikaError = core.into();
        assert_eq!(nika.nika_code(), codes::NIKA_999);
    }

    #[test]
    fn display_includes_code_prefix() {
        let nika: NikaError = CoreError::NotFound {
            what: "task foo".into(),
        }
        .into();
        let display = nika.to_string();
        assert!(
            display.starts_with("NIKA-002: "),
            "expected 'NIKA-002: ...' got '{display}'"
        );
        assert!(display.contains("not found: task foo"));
    }

    #[test]
    fn debug_includes_code() {
        let nika: NikaError = CoreError::Unsupported {
            feature: "vision".into(),
        }
        .into();
        let debug = format!("{nika:?}");
        assert!(
            debug.contains("NIKA-003"),
            "debug should include code, got '{debug}'"
        );
    }

    #[test]
    fn downcast_ref_succeeds_for_correct_type() {
        let nika: NikaError = CoreError::Validation {
            reason: "bad".into(),
        }
        .into();
        let core = nika.downcast_ref::<CoreError>();
        assert!(core.is_some());
        assert!(matches!(core.unwrap(), CoreError::Validation { .. }));
    }

    #[test]
    fn downcast_ref_returns_none_for_wrong_type() {
        // Create a custom error type to test cross-type downcast
        #[derive(Debug, thiserror::Error, miette::Diagnostic)]
        #[error("other error")]
        struct OtherError;

        impl NikaErrorCode for OtherError {
            fn nika_code(&self) -> NikaCode {
                codes::NIKA_999
            }
        }

        let nika: NikaError = OtherError.into();
        assert!(nika.downcast_ref::<CoreError>().is_none());
    }

    #[test]
    fn fingerprint_delegates_to_inner() {
        // Use independent instances to avoid trivial identity
        let fp_direct = CoreError::Validation {
            reason: "x".into(),
        }
        .fingerprint();
        let nika: NikaError = CoreError::Validation {
            reason: "x".into(),
        }
        .into();
        assert_eq!(nika.fingerprint(), fp_direct);

        // Different code → different fingerprint through NikaError
        let nika2: NikaError = CoreError::Internal {
            context: "a".into(),
            detail: "b".into(),
        }
        .into();
        assert_ne!(nika.fingerprint(), nika2.fingerprint());
    }

    #[test]
    fn nika_error_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        // NikaError contains Box<dyn NikaErrorCode> which requires Send + Sync
        assert_send::<NikaError>();
        assert_sync::<NikaError>();
    }

    #[test]
    fn source_delegates_to_inner() {
        use std::error::Error;
        let nika: NikaError = CoreError::Validation {
            reason: "x".into(),
        }
        .into();
        // CoreError::Validation has no source, so this should be None
        assert!(nika.source().is_none());
    }

    #[test]
    fn new_and_from_produce_same_code() {
        let e1 = NikaError::new(CoreError::Internal {
            context: "a".into(),
            detail: "b".into(),
        });
        let e2: NikaError = CoreError::Internal {
            context: "a".into(),
            detail: "b".into(),
        }
        .into();
        assert_eq!(e1.nika_code(), e2.nika_code());
    }

    #[test]
    fn is_transient_delegates_to_inner() {
        // Custom transient error to verify delegation
        #[derive(Debug, thiserror::Error, miette::Diagnostic)]
        #[error("transient")]
        struct TransientError;

        impl NikaErrorCode for TransientError {
            fn nika_code(&self) -> NikaCode {
                codes::NIKA_999
            }
            fn is_transient(&self) -> bool {
                true
            }
        }

        let nika: NikaError = TransientError.into();
        assert!(nika.is_transient(), "should delegate is_transient=true");

        // Non-transient control
        let nika2: NikaError = CoreError::Validation {
            reason: "x".into(),
        }
        .into();
        assert!(!nika2.is_transient(), "CoreError should be non-transient");
    }

    #[test]
    fn diagnostic_help_delegates_to_inner() {
        use miette::Diagnostic;
        let nika: NikaError = CoreError::Validation {
            reason: "x".into(),
        }
        .into();
        let help = nika.help().map(|h| h.to_string());
        assert!(
            help.is_some(),
            "NikaError should delegate miette help from inner"
        );
        assert!(
            help.as_deref().is_some_and(|h| !h.is_empty()),
            "help should not be empty"
        );
    }

    #[test]
    fn source_delegates_with_chained_error() {
        use std::error::Error;

        #[derive(Debug, thiserror::Error, miette::Diagnostic)]
        #[error("root cause")]
        struct RootCause;

        #[derive(Debug, thiserror::Error, miette::Diagnostic)]
        #[error("wrapper")]
        struct ChainedError {
            #[source]
            cause: RootCause,
        }

        impl NikaErrorCode for ChainedError {
            fn nika_code(&self) -> NikaCode {
                codes::NIKA_999
            }
        }

        let nika: NikaError = ChainedError { cause: RootCause }.into();
        let src = nika.source();
        assert!(src.is_some(), "source should delegate to inner's source");
        assert_eq!(src.unwrap().to_string(), "root cause");
    }
}
