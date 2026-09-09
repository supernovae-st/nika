// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The SUCCESS arms of the two model verbs — one verb output folded
//! into one [`Dispatched`] (value · tokens · spend · the admitted lane).
//! Split from `dispatch.rs` at the 100-line fn cap (One Door · wave 1
//! threaded the lane through both arms); the bodies moved verbatim.

use nika_error::traits::NikaErrorCode;
use nika_types::access::{AccessPlan, AccessRefused};
use nika_types::cost::UnpricedReason;
use nika_verb_agent::{AgentOutput, AgentValue};
#[cfg(feature = "access-harness")]
use nika_verb_infer::HarnessInferOutput;
use nika_verb_infer::{InferOutput, InferValue};
use serde_json::Value;

use super::{Dispatched, spend_for_model};
use crate::usage::UsageSplit;

/// A one-shot `infer:` that answered — the resolved model prices the
/// usage through the SAME resolver as the check-time floor.
pub(super) fn infer_success(out: InferOutput, access: Option<AccessPlan>) -> Dispatched {
    let note = format!("infer · {}", out.model_resolved);
    let value = match out.output {
        InferValue::Text(text) => Value::String(text),
        // Structured output IS a JSON value (spec 04 typed dataflow —
        // downstream templates render it canonically · for_each can
        // fan over arrays).
        InferValue::Structured(value) => value,
        // #[non_exhaustive] · a future value form fails loudly.
        other => {
            return Dispatched::unwired(
                &note,
                format!("infer value form not wired yet: {other:?}"),
            );
        }
    };
    let tokens = Some(i64::try_from(out.usage.output_tokens).unwrap_or(i64::MAX));
    // #651 · the empty-answer footgun (OBS-E) now settles FAILED at the
    // verb (NIKA-INFER-004) — a blank answer with token spend never
    // reaches this success arm, so the non-fatal warning lane is
    // retired here.
    let warning = None;
    // Real spend: catalog pricing × the provider's FULL usage split
    // (cache subsets at their own rates) · the SAME resolver as the
    // check-time floor (they can never disagree on which row prices a
    // model) · unpriced models emit nothing PLUS the honest WHY (local ·
    // mock · uncataloged · provider silent).
    let (cost_usd, cost_unpriced) = spend_for_model(&out.model_resolved, &out.usage);
    let cost_source = Some(out.model_resolved.clone());
    // the split that PRICED the call rides the frame beside the
    // number, with the responder's own identity (`gen_ai.response.model`
    // / `.id` — captured at the wire since ADR-112's precondition was
    // met, dropped at this seam until now).
    let split = UsageSplit::of(&out.usage)
        .served_by(
            out.response.gen_ai.response_model.clone(),
            out.response.gen_ai.response_id.clone(),
        )
        .carried();
    Dispatched::ok_metered(
        note,
        value,
        tokens,
        warning,
        cost_usd,
        cost_source,
        cost_unpriced,
    )
    .with_usage(split)
    .with_access(access)
}

/// A chosen seat that failed at the call — the typed refusal the
/// terminal frame carries (`access_refused`): the seat, its OWN witness
/// (the error's words · the seat's refusal text rides them), the next
/// READY path the admission recorded and the one flag that pins it.
/// The run fails as it always did; nothing falls through onto a metered
/// path the author did not choose — the frame says what to pin.
pub(super) fn seat_refused(
    seat_id: &str,
    err: &dyn NikaErrorCode,
    access: Option<&AccessPlan>,
) -> AccessRefused {
    AccessRefused::from_plan(seat_id, err.to_string(), access)
}

/// `access_refused` only when the error is a proven seat/access
/// refusal (NIKA-1800..1805, an infer harness-access miss, or a
/// harness error wrapped as agent inference). A tool, max-turns, or
/// schema failure after a seat already ran is not a pin: teaching
/// `--access api` there would lie.
pub(super) fn proven_seat_refusal(
    seat_id: &str,
    err: &dyn NikaErrorCode,
    access: Option<&AccessPlan>,
) -> Option<AccessRefused> {
    is_proven_access_refusal(err).then(|| seat_refused(seat_id, err, access))
}

fn is_proven_access_refusal(err: &dyn NikaErrorCode) -> bool {
    let code = err.nika_code();
    if code.category == nika_error::codes::Category::Access {
        return (1800..=1805).contains(&code.num);
    }
    if let Some(infer) = err
        .as_any()
        .downcast_ref::<nika_verb_infer::VerbInferError>()
    {
        return matches!(infer, nika_verb_infer::VerbInferError::HarnessAccess { .. });
    }
    // The agent bridge currently erases HarnessError into ProviderError::Other.
    // A provider's text can imitate that spelling; it is not typed evidence.
    false
}

/// A one-shot `infer:` served by the operator's subscription seat: the
/// subscription absorbs the spend — named (`SubscriptionQuota`), never
/// a fabricated $0.
#[cfg(feature = "access-harness")]
pub(super) fn harness_infer_success(
    seat_id: &str,
    out: HarnessInferOutput,
    access: Option<AccessPlan>,
) -> Dispatched {
    Dispatched::ok_metered(
        format!("infer · seat {seat_id} · requested {}", out.requested_model),
        out.output,
        None,
        None,
        None,
        None,
        Some(UnpricedReason::SubscriptionQuota),
    )
    .with_access(access)
}

/// An `agent:` loop that settled — BOTH spend channels ride: the loop's
/// TOOL spend (exact · tool-reported — an agent-driven $0.02 render must
/// never show as $0.00) PLUS the LLM turns priced from the loop's
/// absorbed usage split via the same resolver `infer` uses (the seam
/// closed 2026-07-08). Either alone still rides; an unpriced LLM leg
/// names its reason next to whatever tool spend DID meter.
pub(super) fn agent_success(out: AgentOutput, access: Option<AccessPlan>) -> Dispatched {
    let note = format!("agent · {} turns", out.turns);
    let value = match out.output {
        AgentValue::Text(text) => Value::String(text),
        AgentValue::Structured(value) => value,
        // #[non_exhaustive] · a future value form fails loudly.
        other => {
            return Dispatched::unwired(
                &note,
                format!("agent value form not wired yet: {other:?}"),
            );
        }
    };
    let tokens = Some(i64::try_from(out.total_tokens).unwrap_or(i64::MAX));
    let (llm_cost, llm_unpriced) = match out.model_resolved.as_deref() {
        Some(model) => spend_for_model(model, &out.usage),
        // Harness-built (B7): the subscription absorbs it — named,
        // NEVER a fabricated $0 (the ledger law).
        None => (None, Some(UnpricedReason::SubscriptionQuota)),
    };
    let cost_usd = match (llm_cost, out.tools_cost_usd) {
        (None, None) => None,
        (llm, tools) => Some(llm.unwrap_or(0.0) + tools.unwrap_or(0.0)),
    };
    // the loop's ABSORBED split (every turn summed, like the
    // `tokens` it rides beside). No response id: a loop has many.
    let split = UsageSplit::of(&out.usage).carried();
    Dispatched::ok_metered(
        note,
        value,
        tokens,
        None,
        cost_usd,
        out.model_resolved.clone(),
        llm_unpriced,
    )
    .with_usage(split)
    .with_access(access)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod proven_refusal_tests {
    use nika_kernel::ai::harness::HarnessError;
    use nika_kernel::ai::provider::ProviderError;
    use nika_types::blame::BlamePolarity;
    use nika_types::cost::SpendOnFailure;
    use nika_verb_agent::VerbAgentError;
    use nika_verb_infer::VerbInferError;

    use super::{is_proven_access_refusal, proven_seat_refusal};

    #[test]
    fn infer_harness_access_is_a_typed_refusal() {
        let err = VerbInferError::HarnessAccess {
            detail: "not infer-grade".to_owned(),
        };
        let refused = proven_seat_refusal("gemini-cli", &err, None).expect("seat refusal");
        assert_eq!(refused.seat, "gemini-cli");
        assert!(refused.witness.contains("not infer-grade"), "{refused:?}");
    }

    #[test]
    fn infer_schema_after_the_seat_is_not_a_pin() {
        let err = VerbInferError::SchemaValidation {
            attempts: 1,
            detail: "missing field".to_owned(),
            spend: Box::default(),
        };
        assert!(proven_seat_refusal("gemini-cli", &err, None).is_none());
        assert!(!is_proven_access_refusal(&err));
    }

    #[test]
    fn agent_max_turns_after_a_seat_is_not_a_pin() {
        let err = VerbAgentError::MaxTurns {
            turns: 1,
            partial_output: "hi".to_owned(),
            blame: BlamePolarity::ByTheCaller,
            blame_source: "the task's own `max_turns:`",
            spend: Box::default(),
        };
        assert!(proven_seat_refusal("gemini-cli", &err, None).is_none());
    }

    #[test]
    fn agent_schema_after_a_seat_is_not_a_pin() {
        let err = VerbAgentError::SchemaValidation {
            detail: "missing field".to_owned(),
            spend: Box::default(),
        };
        assert!(proven_seat_refusal("gemini-cli", &err, None).is_none());
    }

    #[test]
    fn agent_tool_after_a_seat_is_not_a_pin() {
        let err = VerbAgentError::WhitelistViolation {
            tool: "nika:rm".to_owned(),
            spend: Box::default(),
        };
        assert!(proven_seat_refusal("gemini-cli", &err, None).is_none());
    }

    #[test]
    fn agent_provider_failure_after_a_seat_is_not_a_pin() {
        let err = VerbAgentError::Inference {
            source: ProviderError::Other {
                reason: "MockProvider: response queue exhausted".into(),
            },
            spend: Box::default(),
        };
        assert!(proven_seat_refusal("gemini-cli", &err, None).is_none());
    }

    #[test]
    fn provider_text_cannot_impersonate_a_typed_harness_refusal() {
        for reason in [
            "harness unavailable: missing",
            "harness session failed: denied",
            "harness refused: denied",
        ] {
            let err = VerbAgentError::Inference {
                source: ProviderError::Other {
                    reason: reason.to_owned(),
                },
                spend: Box::default(),
            };
            assert!(
                proven_seat_refusal("codex", &err, None).is_none(),
                "{reason}"
            );
        }
    }

    #[test]
    fn access_family_codes_are_a_typed_refusal_except_the_human_gate() {
        let unavailable = HarnessError::Unavailable {
            reason: "no binary".to_owned(),
        };
        assert!(is_proven_access_refusal(&unavailable));
        let session = HarnessError::Session {
            reason: "wire died".to_owned(),
        };
        assert!(is_proven_access_refusal(&session));
        let refused = HarnessError::Refused {
            reason: "not signed in".to_owned(),
        };
        assert!(is_proven_access_refusal(&refused));
        let gate = VerbAgentError::HarnessGate {
            question: "allow net?".to_owned(),
            detail: "permits.net missing".to_owned(),
            spend: Box::new(SpendOnFailure::default()),
        };
        assert!(
            !is_proven_access_refusal(&gate),
            "NIKA-1806 is a pause, not a pin"
        );
    }
}
