// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbInferError` — the `infer` verb error surface (NIKA-430..435).
//!
//! Constants are registry-owned in `nika_error::codes` (same pattern as
//! the M2 computer-use ranges); the spec-level rows are
//! `NIKA-INFER-001..004` in `nika-spec spec/05-errors.md` — 430 maps to
//! 001 (provider call), 431 maps to 002 (schema validation), 434 maps to
//! 003 (usage unmetered), 435 maps to 004 (empty answer · #651).

use nika_error::codes::{self, NikaCode};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::ProviderError;
use nika_types::cost::SpendOnFailure;

/// Errors from the `infer` verb executor.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VerbInferError {
    /// A subscription seat could not prove or execute the infer-grade
    /// one-shot contract. No provider spend is attached: the seat consumes
    /// subscription quota, never an invented token or price meter.
    #[error("harness access failed during `infer`: {detail}")]
    #[diagnostic(code(nika::verb::infer_provider_call))]
    HarnessAccess {
        /// Attestation refusal or adapter execution witness.
        detail: String,
    },

    /// The ACP session of a declared `run.access.protocol: acp` one-shot
    /// failed the task, typed by its own class (NIKA-1803 unavailable ·
    /// NIKA-1804 the session died, the one transient row · NIKA-1805
    /// refused: an unoffered model or effort, a read-back that does not
    /// hold, a tool beat). The `agent:` twin under the same declaration
    /// speaks the same codes; never erased into a provider failure.
    #[error("infer harness: {source}")]
    #[diagnostic(code(nika::access::infer_harness))]
    Harness {
        /// The harness's typed failure.
        #[source]
        source: nika_kernel::ai::harness::HarnessError,
    },

    /// The provider call failed (HTTP error, refusal, rate limit, …).
    #[error("provider call failed during `infer`: {source}")]
    #[diagnostic(code(nika::verb::infer_provider_call))]
    ProviderCall {
        /// The underlying provider error.
        #[source]
        source: ProviderError,
        /// The spend of the round-trips that DID run before this call
        /// failed (a schema-repair loop bills every round-trip; the
        /// failing call itself reports no usage — providers do not
        /// bill errored requests). Boxed: the split would otherwise
        /// dominate the whole error's size (`result_large_err`).
        spend: Box<SpendOnFailure>,
    },

    /// The seat rejected a request that carried the task `schema:`
    /// natively — HTTP 400 or 422 at the door with `response_format` set
    /// (NIKA-430 · wire `NIKA-INFER-001`). Nothing was sampled; a retry
    /// re-asks the identical refusal. The fix is the schema or the seat,
    /// and the message names both.
    #[error(
        "`{model}` rejected the structured request while the task `schema:` travelled as native {wire}: {source} — either the schema (simplify it: drop formats, patterns, minLength; or seat a model the catalog lists with json_mode: schema, `nika catalog --json`) or the account (a provider answers an exhausted credit balance or quota with this same status: check its billing page); the reply was never sampled"
    )]
    #[diagnostic(code(nika::verb::infer_provider_call))]
    SchemaRefused {
        /// The seat that refused (`provider/name`).
        model: String,
        /// How the schema travelled: `json_schema` or `json_object`.
        wire: &'static str,
        /// The refusal, as the provider named it.
        #[source]
        source: Box<ProviderError>,
        /// The spend of the round-trips that DID answer before this one.
        spend: Box<SpendOnFailure>,
    },

    /// The backend omitted the usage block on a PRICED model (NIKA-434 ·
    /// wire `NIKA-INFER-003`) — the ledger would bill this task $0 while
    /// the provider charges real money (the 2026-07-29 audit, run 3 ·
    /// R3-F1 · the agent loop's `NIKA-AGENT-005` sibling). Fail-closed; a
    /// mock/local zero is a TRUE zero (the documented unmetered
    /// carve-out), never an invented number.
    #[error(
        "the provider reported no token usage for priced model `{model}` — the ledger cannot bill this call honestly (fail-closed)"
    )]
    #[diagnostic(code(nika::verb::infer_usage_unmetered))]
    UsageUnmetered {
        /// The priced model whose spend is now invisible.
        model: String,
        /// The spend of the round-trips that DID run before this call.
        spend: Box<SpendOnFailure>,
    },

    /// The provider spent tokens yet the VISIBLE answer is empty
    /// (NIKA-435 · wire `NIKA-INFER-004` · #651 — the OBS-E warn,
    /// promoted): a thinking model under a tight `max_tokens` can spend
    /// the whole budget on its reasoning trace and conclude BLANK — the
    /// run used to settle green over an empty `output`. Fail-closed with
    /// the `max_tokens` / thinking-budget teaching on `detail`; a blank
    /// answer with ZERO tokens of any kind is a plain empty completion
    /// and never reaches this variant.
    #[error("infer produced an empty answer on `{model}` — {detail}")]
    #[diagnostic(code(nika::verb::infer_empty_answer))]
    EmptyAnswer {
        /// The model that answered blank (`provider/name`).
        model: String,
        /// The spend signal that fired + the `max_tokens` /
        /// thinking-budget teaching (the warn's text, carried over
        /// verbatim).
        detail: String,
        /// The spend of the round-trip that answered blank — the tokens
        /// ARE billed whether or not anything visible came back.
        spend: Box<SpendOnFailure>,
    },

    /// The output never satisfied the task `schema:` within the retry budget.
    #[error("structured output failed schema validation after {attempts} attempt(s): {detail}")]
    #[diagnostic(code(nika::verb::infer_schema_validation))]
    SchemaValidation {
        /// Total provider round-trips spent (initial call + retries).
        attempts: u32,
        /// The last validation failure, human-readable.
        detail: String,
        /// The spend of those billed round-trips — real money whether
        /// or not the shape converged (the ledger + budget see it).
        spend: Box<SpendOnFailure>,
    },

    /// An `infer` parameter is invalid (empty prompt · temperature out of
    /// 0-2 · missing `vision:` file).
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

impl VerbInferError {
    /// The spend the failed execution had already incurred, when the
    /// variant carries one AND it could price to anything (a
    /// zero-signal spend — e.g. the FIRST call failing — reads as
    /// `None`: nothing was billed, nothing to meter).
    #[must_use]
    pub fn spend(&self) -> Option<&SpendOnFailure> {
        match self {
            Self::ProviderCall { spend, .. }
            | Self::SchemaRefused { spend, .. }
            | Self::UsageUnmetered { spend, .. }
            | Self::EmptyAnswer { spend, .. }
            | Self::SchemaValidation { spend, .. } => spend.has_signal().then_some(spend),
            Self::HarnessAccess { .. }
            | Self::Harness { .. }
            | Self::InvalidParam { .. }
            | Self::ModelResolution { .. } => None,
        }
    }
}

impl NikaErrorCode for VerbInferError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::ProviderCall { .. } | Self::SchemaRefused { .. } | Self::HarnessAccess { .. } => {
                codes::NIKA_430
            }
            Self::UsageUnmetered { .. } => codes::NIKA_434,
            Self::EmptyAnswer { .. } => codes::NIKA_435,
            Self::SchemaValidation { .. } => codes::NIKA_431,
            Self::InvalidParam { .. } => codes::NIKA_432,
            Self::ModelResolution { .. } => codes::NIKA_433,
            Self::Harness { source } => source.nika_code(),
        }
    }

    /// The user-facing SPEC code (`spec/05-errors.md` · what `on_codes:`
    /// filters on). `NIKA-430` → `NIKA-INFER-001` (provider call) · `NIKA-431`
    /// → `NIKA-INFER-002` (schema validation) · `NIKA-433` (model resolution)
    /// is the `NIKA-INFER-001` family (a provider-resolution failure) ·
    /// `NIKA-434` → `NIKA-INFER-003` (usage unmetered) · `NIKA-435` →
    /// `NIKA-INFER-004` (empty answer · #651). `NIKA-432` (`InvalidParam`)
    /// has NO spec row (an upstream-reject guard) — it keeps its numeric
    /// wire form via the trait default.
    fn spec_code(&self) -> String {
        match self {
            Self::ProviderCall { .. }
            | Self::SchemaRefused { .. }
            | Self::HarnessAccess { .. }
            | Self::ModelResolution { .. } => "NIKA-INFER-001".to_owned(),
            Self::UsageUnmetered { .. } => "NIKA-INFER-003".to_owned(),
            Self::EmptyAnswer { .. } => "NIKA-INFER-004".to_owned(),
            Self::SchemaValidation { .. } => "NIKA-INFER-002".to_owned(),
            Self::InvalidParam { .. } | Self::Harness { .. } => self.nika_code().to_string(),
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            // Inherit the provider's own retry classification (rate limits
            // and 5xx are transient; auth and model-not-found are not): the
            // transport never re-sends, so the author's `retry:` decides.
            Self::ProviderCall { source, .. } => source.is_transient(),
            // Only a session death heals on retry — of the SAME route.
            Self::Harness { source } => source.is_transient(),
            // An empty answer at the SAME budget re-asks for the identical
            // failure — the remedy is `max_tokens`, never a retry (#651); a
            // seat that refused the schema at the door refuses it again.
            Self::HarnessAccess { .. }
            | Self::SchemaRefused { .. }
            | Self::EmptyAnswer { .. }
            | Self::SchemaValidation { .. }
            | Self::UsageUnmetered { .. }
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

    fn rate_limited() -> ProviderError {
        ProviderError::HttpResponse {
            details: nika_kernel::ai::provider::ProviderHttpError::new(
                429,
                Some("rate_limit_exceeded"),
                None,
                Some("2"),
            ),
        }
    }

    fn bad_request() -> ProviderError {
        ProviderError::HttpResponse {
            details: nika_kernel::ai::provider::ProviderHttpError::new(
                400,
                None,
                Some("invalid_request_error"),
                None,
            ),
        }
    }

    /// The readings of a failed call: both speak `NIKA-INFER-001`; a
    /// rate-limited call stays transient (the transport never re-sends, so
    /// the author's `retry:` decides) and names the delay the seat asked
    /// for; the schema refusal names the next safe action.
    #[test]
    fn a_rate_limited_call_stays_transient_and_the_schema_refusal_names_the_seat() {
        let limited = VerbInferError::ProviderCall {
            source: rate_limited(),
            spend: Box::default(),
        };
        assert_eq!(limited.spec_code(), "NIKA-INFER-001");
        assert!(limited.is_transient(), "the author's retry: decides");
        let text = limited.to_string();
        assert!(text.contains("rate limited (HTTP 429)"), "{text}");
        assert!(text.contains("Retry-After=2"), "{text}");
        assert!(limited.spend().is_none(), "a refused answer bills nothing");

        let refused = VerbInferError::SchemaRefused {
            model: "openai/gpt-4o-mini".to_owned(),
            wire: "json_schema",
            source: Box::new(bad_request()),
            spend: Box::default(),
        };
        assert_eq!(refused.spec_code(), "NIKA-INFER-001");
        assert!(
            !refused.is_transient(),
            "the seat refuses the same schema again"
        );
        let text = refused.to_string();
        assert!(text.contains("`openai/gpt-4o-mini` rejected"), "{text}");
        assert!(text.contains("native json_schema"), "{text}");
        assert!(text.contains("HTTP 400"), "{text}");
        assert!(text.contains("either the schema"), "{text}");
        assert!(text.contains("json_mode: schema"), "{text}");
        assert!(text.contains("never sampled"), "{text}");
    }

    #[test]
    fn codes_match_the_registry() {
        let cases: Vec<(VerbInferError, NikaCode)> = vec![
            (
                VerbInferError::ProviderCall {
                    source: provider_err(),
                    spend: Box::default(),
                },
                codes::NIKA_430,
            ),
            (
                VerbInferError::HarnessAccess {
                    detail: "unattested seat".to_owned(),
                },
                codes::NIKA_430,
            ),
            (
                VerbInferError::SchemaValidation {
                    attempts: 3,
                    detail: "missing field".to_owned(),
                    spend: Box::default(),
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
            (
                VerbInferError::EmptyAnswer {
                    model: "ollama/qwen3.5:4b".to_owned(),
                    detail: "reasoning consumed 84 tokens".to_owned(),
                    spend: Box::default(),
                },
                codes::NIKA_435,
            ),
            (
                VerbInferError::SchemaRefused {
                    model: "openai/gpt-4o-mini".to_owned(),
                    wire: "json_schema",
                    source: Box::new(bad_request()),
                    spend: Box::default(),
                },
                codes::NIKA_430,
            ),
        ];
        for (err, expected) in cases {
            assert_eq!(err.nika_code(), expected, "{err}");
            // Wire parity: the registry resolves the Display form back.
            assert_eq!(codes::lookup(&expected.to_string()), Some(expected));
        }
    }

    #[test]
    fn spec_codes_match_the_wire_table() {
        // The infer half of the one-voice contract (#468): the provider
        // class is `NIKA-INFER-001`, the schema gate `NIKA-INFER-002` —
        // the SAME codes the agent loop's chained failures carry.
        let provider = VerbInferError::ProviderCall {
            source: provider_err(),
            spend: Box::default(),
        };
        assert_eq!(provider.spec_code(), "NIKA-INFER-001");
        let harness = VerbInferError::HarnessAccess {
            detail: "unattested seat".to_owned(),
        };
        assert_eq!(harness.spec_code(), "NIKA-INFER-001");
        assert!(!harness.is_transient());
        let resolution = VerbInferError::ModelResolution {
            model: "ghost/model".to_owned(),
            source: provider_err(),
        };
        assert_eq!(resolution.spec_code(), "NIKA-INFER-001");
        let schema = VerbInferError::SchemaValidation {
            attempts: 3,
            detail: "missing field".to_owned(),
            spend: Box::default(),
        };
        assert_eq!(schema.spec_code(), "NIKA-INFER-002");
        // The promoted empty-answer footgun (#651) speaks its own row.
        let empty = VerbInferError::EmptyAnswer {
            model: "ollama/qwen3.5:4b".to_owned(),
            detail: "reasoning consumed 84 tokens".to_owned(),
            spend: Box::default(),
        };
        assert_eq!(empty.spec_code(), "NIKA-INFER-004");
        assert!(
            !empty.is_transient(),
            "the remedy is max_tokens, never a retry"
        );
        // The upstream-reject guard has no spec row — numeric wire form.
        let param = VerbInferError::InvalidParam {
            param: "temperature",
            detail: "3.5 out of 0-2".to_owned(),
        };
        assert_eq!(param.spec_code(), "NIKA-432");
    }

    #[test]
    fn transience_classification() {
        // Verb-local failures are never transient.
        assert!(
            !VerbInferError::SchemaValidation {
                attempts: 1,
                detail: String::new(),
                spend: Box::default(),
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
        // Provider transience is inherited, not overridden — both branches.
        let transient = VerbInferError::ProviderCall {
            source: provider_err(),
            spend: Box::default(),
        };
        assert!(provider_err().is_transient(), "500 is retry-eligible");
        assert!(transient.is_transient());
        let auth = || ProviderError::AuthFailed {
            reason: "bad key".to_owned(),
        };
        assert!(!auth().is_transient(), "auth failure is terminal");
        assert!(
            !VerbInferError::ProviderCall {
                source: auth(),
                spend: Box::default(),
            }
            .is_transient()
        );
    }

    /// The ACP one-shot keeps the harness's own access class (the `agent:`
    /// twin's codes): its wire form is that code, it carries no spend, and
    /// only a session death is transient.
    #[test]
    fn a_harness_failure_keeps_its_access_class() {
        use nika_kernel::ai::harness::HarnessError;
        let cases = [
            (
                HarnessError::Unavailable {
                    reason: "absent".to_owned(),
                },
                codes::NIKA_1803,
                false,
            ),
            (
                HarnessError::Session {
                    reason: "pipe closed".to_owned(),
                },
                codes::NIKA_1804,
                true,
            ),
            (
                HarnessError::Refused {
                    reason: "a tool beat".to_owned(),
                },
                codes::NIKA_1805,
                false,
            ),
            (
                HarnessError::Selection {
                    reason: "an unoffered effort".to_owned(),
                },
                codes::NIKA_1805,
                false,
            ),
        ];
        for (source, code, transient) in cases {
            let err = VerbInferError::Harness { source };
            assert_eq!(err.nika_code(), code, "{err}");
            assert_eq!(err.spec_code(), code.to_string(), "{err}");
            assert_eq!(err.is_transient(), transient, "{err}");
            assert!(err.spend().is_none(), "{err}");
        }
    }
}
