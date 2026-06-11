// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbInferError` — the `infer` verb error surface (NIKA-430..433).
//!
//! Constants are registry-owned in `nika_error::codes` (same pattern as
//! the M2 computer-use ranges); the spec-level rows are
//! `NIKA-INFER-001/002` in `nika-spec spec/05-errors.md` — 430 maps to
//! 001 (provider call), 431 maps to 002 (schema validation).

use nika_error::codes::{self, NikaCode};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::ProviderError;

/// Errors from the `infer` verb executor.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VerbInferError {
    /// The provider call failed (HTTP error, refusal, rate limit, …).
    #[error("provider call failed during `infer`: {source}")]
    #[diagnostic(code(nika::verb::infer_provider_call))]
    ProviderCall {
        /// The underlying provider error.
        #[source]
        source: ProviderError,
    },

    /// The output never satisfied the task `schema:` within the retry budget.
    #[error("structured output failed schema validation after {attempts} attempt(s): {detail}")]
    #[diagnostic(code(nika::verb::infer_schema_validation))]
    SchemaValidation {
        /// Total provider round-trips spent (initial call + retries).
        attempts: u8,
        /// The last validation failure, human-readable.
        detail: String,
    },

    /// An `infer` parameter is invalid (empty prompt · temperature out of 0-2).
    #[error("invalid `infer` parameter `{param}`: {detail}")]
    #[diagnostic(code(nika::verb::infer_invalid_param))]
    InvalidParam {
        /// Which parameter failed validation.
        param: &'static str,
        /// Why it failed.
        detail: String,
    },

    /// The `model:` string did not resolve to a provider profile.
    #[error("model `{model}` failed to resolve: {source}")]
    #[diagnostic(code(nika::verb::infer_model_resolution))]
    ModelResolution {
        /// The model string as received.
        model: String,
        /// The registry resolution error.
        #[source]
        source: ProviderError,
    },
}

impl NikaErrorCode for VerbInferError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::ProviderCall { .. } => codes::NIKA_430,
            Self::SchemaValidation { .. } => codes::NIKA_431,
            Self::InvalidParam { .. } => codes::NIKA_432,
            Self::ModelResolution { .. } => codes::NIKA_433,
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            // Inherit the provider's own retry classification (rate limits
            // and 5xx are transient; auth and model-not-found are not).
            Self::ProviderCall { source } => source.is_transient(),
            Self::SchemaValidation { .. }
            | Self::InvalidParam { .. }
            | Self::ModelResolution { .. } => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_err() -> ProviderError {
        ProviderError::Api {
            status: 500,
            message: "boom".to_owned(),
        }
    }

    #[test]
    fn codes_match_the_registry() {
        let cases: Vec<(VerbInferError, NikaCode)> = vec![
            (
                VerbInferError::ProviderCall {
                    source: provider_err(),
                },
                codes::NIKA_430,
            ),
            (
                VerbInferError::SchemaValidation {
                    attempts: 3,
                    detail: "missing field".to_owned(),
                },
                codes::NIKA_431,
            ),
            (
                VerbInferError::InvalidParam {
                    param: "temperature",
                    detail: "3.5 out of 0-2".to_owned(),
                },
                codes::NIKA_432,
            ),
            (
                VerbInferError::ModelResolution {
                    model: "ghost/model".to_owned(),
                    source: provider_err(),
                },
                codes::NIKA_433,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.nika_code(), expected, "{err}");
            // Wire parity: the registry resolves the Display form back.
            assert_eq!(codes::lookup(&expected.to_string()), Some(expected));
        }
    }

    #[test]
    fn transience_classification() {
        // Verb-local failures are never transient.
        assert!(
            !VerbInferError::SchemaValidation {
                attempts: 1,
                detail: String::new(),
            }
            .is_transient()
        );
        assert!(
            !VerbInferError::InvalidParam {
                param: "prompt",
                detail: String::new(),
            }
            .is_transient()
        );
        assert!(
            !VerbInferError::ModelResolution {
                model: "x/y".to_owned(),
                source: provider_err(),
            }
            .is_transient()
        );
        // Provider transience is inherited, not overridden.
        let wrapped = VerbInferError::ProviderCall {
            source: provider_err(),
        };
        assert_eq!(wrapped.is_transient(), provider_err().is_transient());
    }
}
