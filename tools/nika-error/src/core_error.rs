// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cross-cutting error enum for errors that don't belong to any specific crate.
//!
//! [`CoreError`] handles the 4 universal failure modes (validation, not-found,
//! unsupported, internal) so that downstream crates don't each need their own
//! `InternalError` variant.

use crate::codes::{self, NikaCode};
use crate::traits::NikaErrorCode;

/// Cross-cutting errors shared across the Nika diamond.
///
/// Use these for failures that are not specific to a single crate's domain.
/// Crate-specific errors should use their own enum implementing [`NikaErrorCode`].
///
/// # Example
///
/// ```rust
/// use nika_error::prelude::*;
///
/// let err = CoreError::Validation {
///     reason: "missing field 'id'".into(),
/// };
/// assert_eq!(err.nika_code(), codes::NIKA_001);
/// ```
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
#[non_exhaustive]
pub enum CoreError {
    /// NIKA-001: Input validation failed.
    #[error("validation: {reason}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_001)))]
    Validation {
        /// Human-readable description of what failed.
        reason: String,
    },

    /// NIKA-002: Referenced item not found.
    #[error("not found: {what}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_002)))]
    NotFound {
        /// Description of what was looked up.
        what: String,
    },

    /// NIKA-003: Feature or operation not supported.
    #[error("unsupported: {feature}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_003)))]
    Unsupported {
        /// The feature or operation that isn't supported.
        feature: String,
    },

    /// NIKA-999: Internal error (catch-all for unexpected failures).
    #[error("{context}: {detail}")]
    #[diagnostic(help("{}", codes::code_help(codes::NIKA_999)))]
    Internal {
        /// Subsystem where the error occurred (e.g. "serde", "schema").
        context: String,
        /// Human-readable detail of what went wrong.
        detail: String,
    },
}

impl NikaErrorCode for CoreError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::Validation { .. } => codes::NIKA_001,
            Self::NotFound { .. } => codes::NIKA_002,
            Self::Unsupported { .. } => codes::NIKA_003,
            Self::Internal { .. } => codes::NIKA_999,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_display() {
        let e = CoreError::Validation {
            reason: "missing field 'id'".into(),
        };
        assert_eq!(e.to_string(), "validation: missing field 'id'");
    }

    #[test]
    fn not_found_display() {
        let e = CoreError::NotFound {
            what: "model gpt-5".into(),
        };
        assert_eq!(e.to_string(), "not found: model gpt-5");
    }

    #[test]
    fn unsupported_display() {
        let e = CoreError::Unsupported {
            feature: "vision on native provider".into(),
        };
        assert_eq!(e.to_string(), "unsupported: vision on native provider");
    }

    #[test]
    fn internal_display() {
        let e = CoreError::Internal {
            context: "serde".into(),
            detail: "unexpected EOF".into(),
        };
        assert_eq!(e.to_string(), "serde: unexpected EOF");
    }

    #[test]
    fn nika_code_mapping() {
        assert_eq!(
            CoreError::Validation {
                reason: String::new()
            }
            .nika_code(),
            codes::NIKA_001
        );
        assert_eq!(
            CoreError::NotFound {
                what: String::new()
            }
            .nika_code(),
            codes::NIKA_002
        );
        assert_eq!(
            CoreError::Unsupported {
                feature: String::new()
            }
            .nika_code(),
            codes::NIKA_003
        );
        assert_eq!(
            CoreError::Internal {
                context: String::new(),
                detail: String::new()
            }
            .nika_code(),
            codes::NIKA_999
        );
    }

    #[test]
    fn all_variants_not_transient() {
        let variants: Vec<CoreError> = vec![
            CoreError::Validation {
                reason: "x".into(),
            },
            CoreError::NotFound { what: "x".into() },
            CoreError::Unsupported {
                feature: "x".into(),
            },
            CoreError::Internal {
                context: "x".into(),
                detail: "y".into(),
            },
        ];
        for v in &variants {
            assert!(!v.is_transient(), "{v} should not be transient");
        }
    }

    #[test]
    fn core_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CoreError>();
    }

    #[test]
    fn miette_diagnostic_help_is_present() {
        use miette::Diagnostic;
        let e = CoreError::Validation {
            reason: "x".into(),
        };
        let help = e.help().map(|h| h.to_string());
        assert!(help.is_some(), "validation should have miette help");
        assert!(
            help.as_deref()
                .is_some_and(|h| !h.is_empty()),
            "help should not be empty"
        );
    }
}
