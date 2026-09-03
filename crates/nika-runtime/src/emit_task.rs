// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task terminal-frame emission — `task_completed` (+ its `task_recovered`
//! prefix frame) extracted from the settle path (lib.rs sat at the
//! 1500-line file cap; this trio is the cohesive cut).

use crate::record::{TaskRecord, outcome_json};
use crate::{EventKind, EventSink, FieldValue, Stamper, emit, i, resume, s};

/// The F-O1 additive integrity fields — pushed onto a terminal task frame
/// ONLY when the settled record is untrusted (`integrity` ·
/// `integrity_source`, the born-origin witness). Absent = trusted: old
/// journals stay readable and the field is never required — and no gate
/// consumes the label yet (PR-2 is the re-gate).
pub(crate) fn push_integrity_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    record: &TaskRecord,
) {
    if let nika_cap::Integrity::Untrusted { source } = &record.integrity {
        fields.push(("integrity", s(record.integrity.as_str())));
        fields.push(("integrity_source", s(source)));
    }
}

/// `task_recovered` — the ONE emission site (INV#24 · engine#301 · the
/// D-2026-07-08-N4 sequence lock): INSERTS before the terminal, so
/// `task_completed` stays the one success terminal and audit surfaces read
/// the repair from the kind stream. `code` = what was recovered FROM.
pub(crate) fn emit_recovered(
    id: &str,
    code: &str,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    emit(
        stamper,
        sink,
        EventKind::TaskRecovered,
        &[("task", s(id)), ("code", s(code))],
    );
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
pub(crate) fn push_access_fields(
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
    } else if let Some(m) = model {
        fields.push(("model", s(m)));
        if let Some((provider, _)) = m.split_once('/') {
            fields.push(("provider", s(provider)));
            let access = nika_providers::profile::access_class_for(provider);
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

/// Emit one `task_completed` frame — the base fields (`note` ·
/// `duration_ms`) + spend (`tokens`) + the OBS-E `warning` diagnostic
/// when present + the ADR-099 checkpoint trio (`def_hash` · `input_hash`
/// · `output` as ONE compact JSON text) when the task carries a resume
/// stamp + the spec-13 `outcome` (class · cause · payload, derived from
/// the settled RECORD — one truth for the trace and the `tasks.*`
/// namespace). Returns the terminal timestamp.
// The payload knobs mirror the frame's field surface — a builder
// struct would just restate them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_completed(
    id: &str,
    note: &str,
    duration: i64,
    tokens: Option<i64>,
    cost_usd: Option<f64>,
    cost_unpriced: Option<nika_types::cost::UnpricedReason>,
    (model, access): (Option<&str>, Option<&nika_types::access::AccessPlan>),
    warning: Option<&str>,
    child: Option<&crate::child::ChildRunSummary>,
    resume: Option<&resume::ResumeStamp>,
    evidence: Option<&crate::dispatch::commit::CommitEvidence>,
    record: &TaskRecord,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> nika_types::timestamp::Timestamp {
    let mut fields = vec![
        ("task", s(id)),
        ("note", s(note)),
        ("duration_ms", i(duration)),
    ];
    if let Some(n) = tokens {
        fields.push(("tokens", i(n)));
    }
    // Real spend rides next to the tokens it prices · absent = unpriced
    // (mock · local) — the render layer already treats absent as honest.
    if let Some(c) = cost_usd {
        fields.push(("cost_usd", FieldValue::Float(c)));
    }
    // …and WHY it is absent (or partial), when it is — `unknown` is
    // never masked: `local_model` · `mock_provider` ·
    // `missing_catalog_price` · `provider_did_not_report_usage`.
    if let Some(reason) = cost_unpriced {
        fields.push(("cost_unpriced", s(reason.as_str())));
    }
    push_access_fields(&mut fields, model, access, cost_unpriced);
    // OBS-E · a non-fatal diagnostic rides the success frame as a
    // `warning` field (the reasoning-model blank-answer footgun) · the
    // task still completes.
    if let Some(msg) = warning {
        fields.push(("warning", s(msg)));
    }
    // spec 14 law 8 (trace forest) — the child-run row `{target,
    // trace_id, chain_head, def_hash, outcome}` rides the terminal
    // frame; law 9 (receipts) — this frame is itself hash-chained, so
    // the parent's chain COMMITS to the child's head (Merkle).
    let child_json = child.map(crate::child::ChildRunSummary::json);
    if let Some(row) = &child_json {
        fields.push(("child", s(&row.to_string())));
    }
    // ADR-099 · the checkpoint fields — only a stamped success carries
    // them (additive trace fields).
    let output_text =
        resume.map(|_| serde_json::to_string(&record.output).unwrap_or_else(|_| "null".to_owned()));
    if let (Some(stamp), Some(text)) = (resume, output_text.as_deref()) {
        fields.push((resume::fields::DEF_HASH, s(&stamp.def_hash)));
        fields.push((resume::fields::INPUT_HASH, s(&stamp.input_hash)));
        fields.push((resume::fields::OUTPUT, s(text)));
    }
    // F-P6 · the fired step's binding evidence (preview ≡ commit) — and
    // the finding when a RECOVERED divergence preceded (never a warn).
    crate::settle::push_commit_fields(&mut fields, evidence);
    // Spec 13 · trace_format: 2 — every terminal task event carries the
    // outcome (class · cause · payload per class).
    let outcome = outcome_json(record);
    fields.push(("outcome", s(&outcome)));
    // F-O1 · the additive integrity label (present only when untrusted).
    push_integrity_fields(&mut fields, record);
    emit(stamper, sink, EventKind::TaskCompleted, &fields)
}
