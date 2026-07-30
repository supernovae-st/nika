// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `VerbAgentError` — the `agent` verb error surface (NIKA-460..468).
//!
//! Spec mapping (`docs/crate-specs/nika-verb-agent.md` §4): budgets are
//! FAILURES (460 turns · 461 tokens · spec `NIKA-AGENT-001/002`), the
//! whitelist violation is the immediate security stop (462 · spec
//! `NIKA-SEC-002` · not model-negotiable), provider failures chain
//! (463 · wire `NIKA-INFER-001` · the same class the `infer` verb
//! speaks · one voice · #468), the final-message schema gate (464 · wire
//! `NIKA-INFER-002`), parameter validation (465), the
//! tool-definition seam failure (466 · wraps the kernel NIKA-234), the
//! stall verdict (467 · ADR-096), and the security-boundary refusal
//! (468 · a whitelisted tool denied by `permits:`/the SSRF floor mid-loop
//! — the wire speaks the boundary's own `NIKA-SEC-004`/`NIKA-SEC-005`,
//! never fed back to the model).

use nika_error::codes::{
    NIKA_460, NIKA_461, NIKA_462, NIKA_463, NIKA_464, NIKA_465, NIKA_466, NIKA_467, NIKA_468,
    NIKA_469, NikaCode,
};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::ProviderError;
use nika_kernel::ai::tool_defs::ToolDefsError;
use nika_types::blame::BlamePolarity;
use nika_types::cost::SpendOnFailure;

/// The `agent` verb error surface.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum VerbAgentError {
    /// The loop hit `max_turns` without completing (NIKA-460). F-P22
    /// (NEP-0017): the blame is APPENDED to the verdict, never a rewrite
    /// — a task-written `max_turns:` is imputed « by the caller » (F-A5);
    /// the engine-applied default is imputed « by the contract » that
    /// declares it, and the journal's `task_failed` detail (the Display)
    /// names that faulty contract.
    #[error("agent hit max_turns ({turns}) without completing · blame: {blame} ({blame_source})")]
    #[diagnostic(code(nika::verb::agent_max_turns))]
    MaxTurns {
        /// How many turns ran.
        turns: u32,
        /// The last assistant text (the spec's `partial_output`).
        partial_output: String,
        /// Who the exhausted budget is imputed to (F-P22 · the third
        /// polarity names the contract, not the caller, when the DEFAULT
        /// tripped).
        blame: BlamePolarity,
        /// The faulty contract the blame names (e.g. `spec 02-verbs.md
        /// §agent · max_turns default 10`) — the receipt's declarer.
        blame_source: &'static str,
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

    /// The backend omitted the usage block on a PRICED model (NIKA-469 ·
    /// wire `NIKA-AGENT-005`) — every budget and ledger reads that turn as
    /// free, so the loop fails CLOSED instead of continuing invisibly
    /// (the 2026-07-29 audit, run 3 · R3-F1: an omitting backend
    /// completed a 2-turn loop green under `max_tokens_total: 1` over
    /// 1,800 billed tokens — reproduced both directions on the oracle).
    /// The mock/local zero is a TRUE zero and stays the documented
    /// unmetered carve-out; a number is never invented.
    #[error(
        "the provider reported no token usage for priced model `{model}` — the budget cannot meter this call (fail-closed)"
    )]
    #[diagnostic(code(nika::verb::agent_usage_unmetered))]
    UsageUnmetered {
        /// The priced model whose spend is now invisible.
        model: String,
        /// The spend the loop had already incurred (billed turns are real).
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

    /// A whitelisted tool was REFUSED by the security boundary mid-loop
    /// (NIKA-468): the declared `permits:` capability boundary
    /// (`NIKA-SEC-004`) or the SSRF floor (`NIKA-SEC-005`). Same class as
    /// the whitelist violation — security boundaries are not
    /// model-negotiable, so the refusal is NEVER fed back to the model
    /// (spec §permits · `security_error` · the invariant
    /// `nika-cap/src/permits.rs`, the runtime's `security_err` and
    /// `nika-types` state): the loop fails immediately. The boundary's
    /// own spec code rides the wire (one voice with a bare `invoke`
    /// refusal) — the internal registry numeral never surfaces.
    #[error("agent tool `{tool}` refused by the security boundary ({code})")]
    #[diagnostic(code(nika::verb::agent_security_boundary))]
    SecurityBoundary {
        /// The denied tool id.
        tool: String,
        /// The boundary's own spec code (`NIKA-SEC-004` · `NIKA-SEC-005`),
        /// passed through as the wire code.
        code: String,
        /// The spend the loop had already incurred (a mid-loop refusal
        /// arrives after billed turns).
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
            | Self::UsageUnmetered { spend, .. }
            | Self::WhitelistViolation { spend, .. }
            | Self::Inference { spend, .. }
            | Self::SchemaValidation { spend, .. }
            | Self::Stalled { spend, .. }
            | Self::SecurityBoundary { spend, .. } => *spend = incurred,
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
            | Self::UsageUnmetered { spend, .. }
            | Self::WhitelistViolation { spend, .. }
            | Self::Inference { spend, .. }
            | Self::SchemaValidation { spend, .. }
            | Self::Stalled { spend, .. }
            | Self::SecurityBoundary { spend, .. } => spend.has_signal().then_some(spend),
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
            Self::SecurityBoundary { .. } => NIKA_468,
            Self::UsageUnmetered { .. } => NIKA_469,
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
    /// failed `schema:`). The boundary refusal passes the boundary's OWN
    /// code through (`NIKA-SEC-004` · `NIKA-SEC-005`) — the refusal is
    /// the same verdict whether it stopped a bare `invoke` task or an
    /// agent loop, so the wire says the same thing (one voice with the
    /// runtime's `security_err`). The remaining variants (`InvalidParam`
    /// · `ToolDefs` · `Stalled`) have no spec namespace row — they keep
    /// their numeric wire form via the trait default.
    fn spec_code(&self) -> String {
        match self {
            Self::MaxTurns { .. } => "NIKA-AGENT-001".to_owned(),
            Self::MaxTokens { .. } => "NIKA-AGENT-002".to_owned(),
            Self::UsageUnmetered { .. } => "NIKA-AGENT-005".to_owned(),
            Self::WhitelistViolation { .. } => "NIKA-SEC-002".to_owned(),
            Self::Inference { .. } => "NIKA-INFER-001".to_owned(),
            Self::SchemaValidation { .. } => "NIKA-INFER-002".to_owned(),
            Self::SecurityBoundary { code, .. } => code.clone(),
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
            | Self::Stalled { .. }
            | Self::SecurityBoundary { .. }
            | Self::UsageUnmetered { .. } => false,
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
                    blame: BlamePolarity::ByTheContract,
                    blame_source: "spec 02-verbs.md §agent · `max_turns` default 10",
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
            (
                VerbAgentError::SecurityBoundary {
                    tool: "nika:read".to_owned(),
                    code: "NIKA-SEC-004".to_owned(),
                    spend: Box::default(),
                },
                468,
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
            blame: BlamePolarity::ByTheCaller,
            blame_source: "the task's own `max_turns:`",
            spend: Box::default(),
        };
        if let VerbAgentError::MaxTurns { partial_output, .. } = &err {
            assert_eq!(partial_output, "draft so far");
        }
        assert!(err.to_string().contains('3'));
    }

    #[test]
    fn security_boundary_speaks_the_boundary_code_on_the_wire() {
        // One-voice: a `permits:` refusal that stops an agent loop carries
        // the SAME wire code a bare `invoke` refusal carries (what
        // `on_codes:` filters and `tasks.X.error.code` records) — the
        // internal registry numeral NIKA-468 never surfaces (it is outside
        // the spec grammar, exactly like 463/464 → NIKA-INFER-00x).
        for code in ["NIKA-SEC-004", "NIKA-SEC-005"] {
            let err = VerbAgentError::SecurityBoundary {
                tool: "nika:read".to_owned(),
                code: code.to_owned(),
                spend: Box::default(),
            };
            assert_eq!(err.nika_code().num, 468, "{code}");
            assert_eq!(err.spec_code(), code, "the boundary's own code rides");
            assert!(!err.is_transient(), "a boundary refusal is a verdict");
        }
        // The refusal detail reaches the operator message, not the model —
        // and the variant decorates at the return seam like every mid-loop
        // security stop (the NIKA-462 precedent).
        let err = VerbAgentError::SecurityBoundary {
            tool: "nika:read".to_owned(),
            code: "NIKA-SEC-004".to_owned(),
            spend: Box::default(),
        };
        assert!(
            err.spend().is_none(),
            "a zero-signal spend reads as None pre-decoration"
        );
        let mut billed = nika_kernel::ai::provider::TokenUsage::default();
        billed.input_tokens = 15;
        let decorated = err.with_spend(SpendOnFailure::new(
            billed,
            None,
            Some("mock/agent".to_owned()),
        ));
        assert!(
            decorated.spend().is_some(),
            "billed turns before the refusal are real money"
        );
    }
}
