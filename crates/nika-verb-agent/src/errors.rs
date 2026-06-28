// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbAgentError` — the `agent` verb error surface (NIKA-460..467).
//!
//! Spec mapping (`docs/crate-specs/nika-verb-agent.md` §4): budgets are
//! FAILURES (460 turns · 461 tokens · spec `NIKA-AGENT-001/002`), the
//! whitelist violation is the immediate security stop (462 · not
//! model-negotiable), provider failures chain (463), the final-message
//! schema gate (464), parameter validation (465), and the
//! tool-definition seam failure (466 · wraps the kernel NIKA-234).

use nika_error::codes::{
    NIKA_460, NIKA_461, NIKA_462, NIKA_463, NIKA_464, NIKA_465, NIKA_466, NIKA_467, NikaCode,
};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::ProviderError;
use nika_kernel::ai::tool_defs::ToolDefsError;

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
    },

    /// The loop exhausted `max_tokens_total` (NIKA-461).
    #[error("agent exhausted max_tokens_total ({total_tokens} spent)")]
    #[diagnostic(code(nika::verb::agent_max_tokens))]
    MaxTokens {
        /// Cumulative tokens at the stop.
        total_tokens: u64,
        /// The last assistant text (the spec's `partial_output`).
        partial_output: String,
    },

    /// The model requested a tool outside the whitelist (NIKA-462).
    /// Security boundaries are not model-negotiable: immediate failure.
    #[error("agent requested non-whitelisted tool `{tool}`")]
    #[diagnostic(code(nika::verb::agent_whitelist_violation))]
    WhitelistViolation {
        /// The denied tool id.
        tool: String,
    },

    /// A provider call failed mid-loop (NIKA-463).
    #[error("agent inference failed: {source}")]
    #[diagnostic(code(nika::verb::agent_inference))]
    Inference {
        /// The underlying provider error.
        #[source]
        source: ProviderError,
    },

    /// The final message failed `schema:` validation (NIKA-464).
    #[error("agent final message failed schema validation: {detail}")]
    #[diagnostic(code(nika::verb::agent_schema_validation))]
    SchemaValidation {
        /// Why validation failed (parse or schema detail).
        detail: String,
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
    },
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
    /// agent tool call outside the whitelist). The remaining variants
    /// (`Inference` · `SchemaValidation` · `InvalidParam` · `ToolDefs` ·
    /// `Stalled`) have no dedicated spec namespace row — they keep their
    /// numeric wire form via the trait default.
    fn spec_code(&self) -> String {
        match self {
            Self::MaxTurns { .. } => "NIKA-AGENT-001".to_owned(),
            Self::MaxTokens { .. } => "NIKA-AGENT-002".to_owned(),
            Self::WhitelistViolation { .. } => "NIKA-SEC-002".to_owned(),
            Self::Inference { .. }
            | Self::SchemaValidation { .. }
            | Self::InvalidParam { .. }
            | Self::ToolDefs { .. }
            | Self::Stalled { .. } => self.nika_code().to_string(),
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
            Self::Inference { source } => source.is_transient(),
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
                },
                460,
            ),
            (
                VerbAgentError::MaxTokens {
                    total_tokens: 1,
                    partial_output: String::new(),
                },
                461,
            ),
            (
                VerbAgentError::WhitelistViolation {
                    tool: "nika:rm".to_owned(),
                },
                462,
            ),
            (
                VerbAgentError::SchemaValidation {
                    detail: "x".to_owned(),
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
        };
        assert!(!terminal.is_transient(), "a 400 is a verdict");
    }

    #[test]
    fn budget_errors_carry_the_partial_output() {
        let err = VerbAgentError::MaxTurns {
            turns: 3,
            partial_output: "draft so far".to_owned(),
        };
        if let VerbAgentError::MaxTurns { partial_output, .. } = &err {
            assert_eq!(partial_output, "draft so far");
        }
        assert!(err.to_string().contains('3'));
    }
}
