// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The SUCCESS arms of the two model verbs — one verb output folded
//! into one [`Dispatched`] (value · tokens · spend · the admitted lane).
//! Split from `dispatch.rs` at the 100-line fn cap (One Door · wave 1
//! threaded the lane through both arms); the bodies moved verbatim.

use nika_types::access::AccessPlan;
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
