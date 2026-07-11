// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbAgentError` — the `agent` verb error surface (NIKA-460..467).
//!
//! Spec mapping (`docs/crate-specs/nika-verb-agent.md` §4): budgets are
//! FAILURES (460 turns · 461 tokens · spec `NIKA-AGENT-001/002`), the
//! whitelist violation is the immediate security stop (462 · spec
//! `NIKA-SEC-002` · not model-negotiable), provider failures chain
//! (463 · wire `NIKA-INFER-001` · the same class the `infer` verb
//! speaks · #468), the final-message schema gate (464 · wire
//! `NIKA-INFER-002`), parameter validation (465), and the
//! tool-definition seam failure (466 · wraps the kernel NIKA-234).

use nika_error::codes::{
    NIKA_460, NIKA_461, NIKA_462, NIKA_463, NIKA_464, NIKA_465, NIKA_466, NIKA_467, NikaCode,
};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::ProviderError;
use nika_kernel::ai::tool_defs::ToolDefsError;
use nika_types::cost::SpendOnFailure;

/// The `agent` verb error surface.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VerbAgentError {
    /// The loop hit `max_turns` without completing (NIKA-460).
    #[error("agent hit max_turns ({turns}) without completing")]
    #[diagnostic(code(nika::verb::agent_max_turns))]
    MaxTurns {
        /// How many turns ran.
        turns: u32,
        /// The last assistant text (the spec's `partial_output`).
        partial_output: String,
        /// The spend the loop had already incurred (billed turns are
        /// real money — decorated at the verb's return seam).
        spend: Box<SpendOnFailure>,
    },

    /// The loop exhausted `max_tokens_total` (NIKA-461).
    #[error("agent exhausted max_tokens_total ({total_tokens} spent)")]
    #[diagnostic(code(nika::verb::agent_max_tokens))]
    MaxTokens {
        /// Cumulative tokens at the stop.
        total_tokens: u64,
        /// The last assistant text (the spec's `partial_output`).
        partial_output: String,
        /// The spend the loop had already incurred.
        spend: Box<SpendOnFailure>,
    },

    /// The model requested a tool outside the whitelist (NIKA-462).
    /// Security boundaries are not model-negotiable: immediate failure.
    #[error("agent requested non-whitelisted tool `{tool}`")]
    #[diagnostic(code(nika::verb::agent_whitelist_violation))]
    WhitelistViolation {
        /// The denied tool id.
        tool: String,
        /// The spend the loop had already incurred (a mid-loop denial
        /// arrives after billed turns).
        spend: Box<SpendOnFailure>,
    },

    /// A provider call failed mid-loop (NIKA-463 · wire `NIKA-INFER-001`).
    #[error("agent inference failed: {source}")]
    #[diagnostic(code(nika::verb::agent_inference))]
    Inference {
        /// The underlying provider error.
        #[source]
        source: ProviderError,
        /// The spend of the turns that DID run before this call failed
        /// (the failing call itself reports no usage — providers do not
        /// bill errored requests).
        spend: Box<SpendOnFailure>,
    },

    /// The final message failed `schema:` validation (NIKA-464 · wire
    /// `NIKA-INFER-002`).
    #[error("agent final message failed schema validation: {detail}")]
    #[diagnostic(code(nika::verb::agent_schema_validation))]
    SchemaValidation {
        /// Why validation failed (parse or schema detail).
        detail: String,
        /// The spend of the billed turns + re-ask round-trips.
        spend: Box<SpendOnFailure>,
    },

    /// An `agent` parameter is invalid (NIKA-465).
    #[error("invalid `agent` parameter `{param}`: {detail}")]
    #[diagnostic(code(nika::verb::agent_invalid_param))]
    InvalidParam {
        /// Which parameter failed validation.
        param: &'static str,
        /// Why it failed.
        detail: String,
    },

    /// The tool-definition source failed (NIKA-466 · wraps kernel 234).
    #[error("agent tool definitions unavailable: {source}")]
    #[diagnostic(code(nika::verb::agent_tool_defs))]
    ToolDefs {
        /// The underlying kernel seam error.
        #[source]
        source: ToolDefsError,
    },

    /// The loop stalled: an identical action+observation cycle repeated
    /// past the stall threshold after the corrective reflection was
    /// already spent (NIKA-467 · ADR-096). Further turns would burn
    /// budget on a proven no-progress loop.
    #[error(
        "agent stalled: a {period}-turn action cycle repeated {repeats}× with \
         identical observations (no progress)"
    )]
    #[diagnostic(code(nika::verb::agent_stalled))]
    Stalled {
        /// Detected cycle length in turns.
        period: u32,
        /// How many times the cycle repeated.
        repeats: u32,
        /// The last assistant text (the spec's `partial_output`).
        partial_output: String,
        /// The spend the stalled loop had already incurred.
        spend: Box<SpendOnFailure>,
    },
}

impl VerbAgentError {
    /// Attach the loop's already-incurred spend — decorated ONCE at the
    /// verb's return seam (billed turns are real money whether or not
    /// the task succeeds; the dispatch layer prices this, the run
    /// ledger debits it, the `--max-cost-usd` gate sees it). Pre-loop
    /// variants (`InvalidParam` · `ToolDefs`) carry none by
    /// construction and pass through unchanged.
    #[must_use]
    pub fn with_spend(mut self, incurred: SpendOnFailure) -> Self {
        let incurred = Box::new(incurred);
        match &mut self {
            Self::MaxTurns { spend, .. }
            | Self::MaxTokens { spend, .. }
            | Self::WhitelistViolation { spend, .. }
            | Self::Inference { spend, .. }
            | Self::SchemaValidation { spend, .. }
            | Self::Stalled { spend, .. } => *spend = incurred,
            Self::InvalidParam { .. } | Self::ToolDefs { .. } => {}
        }
        self
    }

    /// The spend the failed loop had already incurred, when the variant
    /// carries one AND it could price to anything (a zero-signal spend
    /// reads as `None` — nothing to meter, nothing to decorate).
    #[must_use]
    pub fn spend(&self) -> Option<&SpendOnFailure> {
        match self {
            Self::MaxTurns { spend, .. }
            | Self::MaxTokens { spend, .. }
            | Self::WhitelistViolation { spend, .. }
            | Self::Inference { spend, .. }
            | Self::SchemaValidation { spend, .. }
            | Self::Stalled { spend, .. } => spend.has_signal().then_some(spend),
            Self::InvalidParam { .. } | Self::ToolDefs { .. } => None,
        }
    }
}

impl NikaErrorCode for VerbAgentError {
    fn nika_code(&self) -> NikaCode {
        match self {
            Self::MaxTurns { .. } => NIKA_460,
            Self::MaxTokens { .. } => NIKA_461,
            Self::WhitelistViolation { .. } => NIKA_462,
            Self::Inference { .. } => NIKA_463,
            Self::SchemaValidation { .. } => NIKA_464,
            Self::InvalidParam { .. } => NIKA_465,
            Self::ToolDefs { .. } => NIKA_466,
            Self::Stalled { .. } => NIKA_467,
        }
    }

    /// The user-facing SPEC code (`spec/05-errors.md` · what `on_codes:`
    /// filters on). `NIKA-460` → `NIKA-AGENT-001` (`max_turns`) · `NIKA-461`
    /// → `NIKA-AGENT-002` (`max_tokens`) · `NIKA-462` (non-whitelisted tool)
    /// is the security-stop `NIKA-SEC-002` (spec table `NIKA-SEC-002` ·
    /// agent tool call outside the whitelist). The loop's CHAINED
    /// model-call failures speak the spec's shared classes — the
    /// namespace follows the failure class, not the hosting verb (the
    /// spec's own `NIKA-SEC-002` row is the precedent): a mid-loop
    /// provider failure is `NIKA-INFER-001` (provider call failed · the
    /// SAME code the `infer` verb emits · one voice · #468) and the
    /// final-message schema gate is `NIKA-INFER-002` (structured output
    /// failed `schema:`). The remaining variants (`InvalidParam` ·
    /// `ToolDefs` · `Stalled`) have no spec namespace row — they keep
    /// their numeric wire form via the trait default.
    fn spec_code(&self) -> String {
        match self {
            Self::MaxTurns { .. } => "NIKA-AGENT-001".to_owned(),
            Self::MaxTokens { .. } => "NIKA-AGENT-002".to_owned(),
            Self::WhitelistViolation { .. } => "NIKA-SEC-002".to_owned(),
            Self::Inference { .. } => "NIKA-INFER-001".to_owned(),
            Self::SchemaValidation { .. } => "NIKA-INFER-002".to_owned(),
            Self::InvalidParam { .. } | Self::ToolDefs { .. } | Self::Stalled { .. } => {
                self.nika_code().to_string()
            }
        }
    }

    fn is_transient(&self) -> bool {
        match self {
            // Budgets, security and validation are verdicts — rerunning
            // the same loop is workflow policy (`retry:`), not transience.
            // A stall is the same class: the loop PROVED no progress; a
            // blind rerun replays the proof.
            Self::MaxTurns { .. }
            | Self::MaxTokens { .. }
            | Self::WhitelistViolation { .. }
            | Self::SchemaValidation { .. }
            | Self::InvalidParam { .. }
            | Self::Stalled { .. } => false,
            // Mid-loop provider failures inherit the provider's verdict
            // (rate limits ARE transient).
            Self::Inference { source, .. } => source.is_transient(),
            // The kernel seam is terminal-by-default (its own contract).
            Self::ToolDefs { source } => source.is_transient(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_registry() {
        let cases: Vec<(VerbAgentError, u16)> = vec![
            (
                VerbAgentError::MaxTurns {
                    turns: 10,
                    partial_output: String::new(),
                    spend: Box::default(),
                },
                460,
            ),
            (
                VerbAgentError::MaxTokens {
                    total_tokens: 1,
                    partial_output: String::new(),
                    spend: Box::default(),
                },
                461,
            ),
            (
                VerbAgentError::WhitelistViolation {
                    tool: "nika:rm".to_owned(),
                    spend: Box::default(),
                },
                462,
            ),
            (
                VerbAgentError::SchemaValidation {
                    detail: "x".to_owned(),
                    spend: Box::default(),
                },
                464,
            ),
            (
                VerbAgentError::InvalidParam {
                    param: "prompt",
                    detail: "empty".to_owned(),
                },
                465,
            ),
            (
                VerbAgentError::ToolDefs {
                    source: ToolDefsError::Unavailable {
                        reason: "down".to_owned(),
                    },
                },
                466,
            ),
            (
                VerbAgentError::Stalled {
                    period: 2,
                    repeats: 5,
                    partial_output: String::new(),
                    spend: Box::default(),
                },
                467,
            ),
        ];
        for (err, num) in cases {
            assert_eq!(err.nika_code().num, num, "{err}");
            assert!(!err.is_transient(), "{err} is a verdict, not transient");
        }
    }

    #[test]
    fn inference_maps_to_463_and_inherits_provider_transience() {
        use nika_kernel::ai::provider::ProviderError;
        // A rate-limit IS transient — the agent must propagate that so a
        // `retry:` policy can act (hardcoding `false` here would be a bug).
        let transient = VerbAgentError::Inference {
            source: ProviderError::RateLimited {
                retry_after_ms: Some(2000),
            },
            spend: Box::default(),
        };
        assert_eq!(transient.nika_code().num, 463);
        assert!(
            transient.is_transient(),
            "rate limit propagates as transient"
        );

        // A 4xx API error is terminal — same delegation, opposite verdict.
        let terminal = VerbAgentError::Inference {
            source: ProviderError::Api {
                status: 400,
                message: "bad request".to_owned(),
            },
            spend: Box::default(),
        };
        assert!(!terminal.is_transient(), "a 400 is a verdict");
    }

    #[test]
    fn chained_failures_speak_the_shared_spec_classes_on_the_wire() {
        // #468 · one-voice: the wire code (`spec_code()` · what
        // `on_codes:` matches and `tasks.X.error.code` carries) is the
        // spec's shared class, never the internal registry numeral —
        // `NIKA-463`/`NIKA-464` are outside the spec grammar and
        // `nika check` rejects them in `on_codes:`.
        use nika_kernel::ai::provider::ProviderError;
        let inference = VerbAgentError::Inference {
            source: ProviderError::Api {
                status: 408,
                message: "HTTP request timed out after 300000ms".to_owned(),
            },
            spend: Box::default(),
        };
        assert_eq!(inference.spec_code(), "NIKA-INFER-001");
        let schema = VerbAgentError::SchemaValidation {
            detail: "missing field".to_owned(),
            spend: Box::default(),
        };
        assert_eq!(schema.spec_code(), "NIKA-INFER-002");
        // Classes WITHOUT a spec row keep the numeric wire form.
        let param = VerbAgentError::InvalidParam {
            param: "prompt",
            detail: "empty".to_owned(),
        };
        assert_eq!(param.spec_code(), "NIKA-465");
    }

    #[test]
    fn budget_errors_carry_the_partial_output() {
        let err = VerbAgentError::MaxTurns {
            turns: 3,
            partial_output: "draft so far".to_owned(),
            spend: Box::default(),
        };
        if let VerbAgentError::MaxTurns { partial_output, .. } = &err {
            assert_eq!(partial_output, "draft so far");
        }
        assert!(err.to_string().contains('3'));
    }
}
