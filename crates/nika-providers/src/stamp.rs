// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The access stamps a task terminal frame carries — pure projections of
//! the admitted lane (`AccessPlan`) and a seat's typed refusal onto the
//! frame's field list. Moved verbatim from the runtime's task-emission
//! module beside the plan they project (the runtime sat at its 15k
//! prod-LOC wall); the authored requirement and the per-call selection
//! evidence ride the same projection.

use nika_types::resource::Value as FieldValue;

fn s(v: &str) -> FieldValue {
    FieldValue::String(v.to_owned())
}

/// D-2026-08-04-N1 · the access facts — structured provenance for
/// infer/agent terminals (`model` = the resolved provider/name ·
/// `provider` = its prefix · `access` = HOW it was reached · `billing`
/// = the economic lane). Additive fields; the note keeps its historical
/// `infer · <model>` form, now a render, not a carrier — readers of
/// pre-access traces still parse it.
///
/// One Door · wave 1: the admitted LANE stamps the terminal when the
/// run carried a frozen plan — `access` · `billing` · `access_id` are
/// the path that actually served (the plan the prologue recorded),
/// never a provider-prefix guess. The prefix derivation stays the bare
/// embedder's fallback; the `SubscriptionQuota` arm is the planless
/// harness receipt (P3 B7 · `access: harness` · billing `unknown`
/// until an adapter's own surface attests it · never a fake $0).
///
/// A lane resolved under an authored requirement also stamps
/// `access_requirement` (the author's words) and, when the call reported
/// it, `access_selection` (what was requested, sent, read back and
/// attested — `nika/access-selection@1`). Both are absent otherwise.
pub fn push_access_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    model: Option<&str>,
    access: Option<&nika_types::access::AccessPlan>,
    cost_unpriced: Option<nika_types::cost::UnpricedReason>,
) {
    if let Some(lane) = access {
        // The verb's resolved model when it reported one (the API path
        // answers with the responder's name), else the lane's own model
        // (a seat run: the requested model IS what the plan resolved).
        fields.push(("model", s(model.unwrap_or(&lane.model))));
        fields.push(("provider", s(&lane.provider)));
        fields.push(("access", s(lane.chosen.as_str())));
        fields.push(("access_id", s(&lane.access)));
        fields.push(("billing", s(lane.billing.as_str())));
        push_selection_fields(fields, lane);
    } else if let Some(m) = model {
        fields.push(("model", s(m)));
        if let Some((provider, _)) = m.split_once('/') {
            fields.push(("provider", s(provider)));
            let access = crate::profile::access_class_for(provider);
            fields.push(("access", s(access.as_str())));
            fields.push(("billing", s(access.default_billing().as_str())));
        }
    } else if cost_unpriced == Some(nika_types::cost::UnpricedReason::SubscriptionQuota) {
        fields.push(("access", s("harness")));
        fields.push((
            "billing",
            s(nika_types::access::BillingClass::Unknown.as_str()),
        ));
    }
}

/// The authored requirement and the call's selection evidence, each as
/// ONE compact JSON text (the `outcome` precedent) — absent when the lane
/// carries none, so a file without `run.access`/`run.reasoning` keeps its
/// frames byte-identical.
fn push_selection_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    lane: &nika_types::access::AccessPlan,
) {
    if let Some(requirement) = &lane.requirement {
        fields.push(("access_requirement", s(&requirement.to_json().to_string())));
    }
    if let Some(selection) = &lane.selection {
        fields.push(("access_selection", s(&selection.to_json().to_string())));
    }
}

/// The typed refusal of a chosen seat (`access_refused` · ONE compact
/// JSON text, the `outcome` precedent): the seat that failed, its own
/// witness, the next READY path the admission recorded and the one flag
/// that pins it — so a reader of the sealed trace knows what to pin
/// without re-running. Absent when no seat refused.
pub fn push_access_refused_field(
    fields: &mut Vec<(&'static str, FieldValue)>,
    refused: Option<&nika_types::access::AccessRefused>,
) {
    if let Some(refused) = refused {
        let json = serde_json::json!({
            "seat": refused.seat,
            "witness": refused.witness,
            "next_ready": refused.next_ready,
            "pin": refused.pin,
        });
        fields.push(("access_refused", s(&json.to_string())));
    }
}

#[cfg(test)]
mod tests;
