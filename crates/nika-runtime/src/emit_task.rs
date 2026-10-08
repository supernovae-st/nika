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
    // Each image already rode its own bounded `agent_image_observed` frame; the
    // count closes that sequence. Output, outcome and expression fields are unchanged.
    if !record.harness_media.is_empty() {
        let count = i64::try_from(record.harness_media.len()).unwrap_or(i64::MAX);
        fields.push(("harness_media_count", i(count)));
    }
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

// The access stamps (lane facts · the authored requirement and the call's
// selection receipt · a seat's typed refusal) are pure projections owned
// beside the plan they read (`nika_providers::stamp`, moved verbatim).
pub(crate) use nika_providers::stamp::{push_access_fields, push_access_refused_field};

/// Emit one `task_completed` frame — the base fields (`note` ·
/// `duration_ms`) + spend (`tokens` + the additive usage split) + the OBS-E `warning` diagnostic
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
    (evidence, items): (
        Option<&crate::dispatch::commit::CommitEvidence>,
        Option<&str>,
    ),
    usage: Option<&crate::usage::UsageSplit>,
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
    // the split that PRICED the call, additive beside `tokens`
    // (which keeps its historical meaning: the completion count). A
    // reader recomputes `cost_usd` from these × the pinned catalog —
    // a warm-cache frame and a price change are no longer the same
    // sight. Absent meters stay absent: `unknown` is never 0.
    crate::usage::push_usage_fields(&mut fields, usage);
    // Real spend rides next to the tokens it prices · absent = unpriced
    // (mock · local) — the render layer already treats absent as honest.
    // …and WHY it is absent (or partial), when it is — `unknown` is
    // never masked: `local_model` · `mock_provider` · `usage_rejected` ·
    // `missing_catalog_price` · `provider_did_not_report_usage`.
    crate::settle::push_spend_fields(&mut fields, cost_usd, cost_unpriced);
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
    // #1276 · #1397 · a fan-out's per-item terminals (index · item · status
    // · code · message) — failures 2..N and the recovered ones reach the
    // journal, not only the first casualty.
    if let Some(items) = items {
        crate::emit_items::push(&mut fields, id, items, stamper, sink);
    }
    // Spec 13 · trace_format: 2 — every terminal task event carries the
    // outcome (class · cause · payload per class).
    let outcome = outcome_json(record);
    fields.push(("outcome", s(&outcome)));
    // F-O1 · the additive integrity label (present only when untrusted).
    push_integrity_fields(&mut fields, record);
    emit(stamper, sink, EventKind::TaskCompleted, &fields)
}

#[cfg(test)]
mod harness_media_tests {
    use super::*;
    #[test]
    fn terminal_media_is_side_evidence_and_never_replaces_text_or_outcome() {
        let mut record = TaskRecord::unran(
            crate::record::TaskStatus::Success,
            crate::record::TerminalCause::Normal,
        );
        record.output = serde_json::json!("original text");
        record.attempts = Some(1);
        let original = outcome_json(&record);
        record.harness_media.push(serde_json::json!({"attempt":1,"image":{"schema":"nika/harness-image-observation@1","reported_saved_path":"/unverified.png","file_verified":false}}));
        let mut fields = Vec::new();
        push_integrity_fields(&mut fields, &record);
        assert!(
            fields
                .iter()
                .any(|(key, value)| *key == "harness_media_count" && *value == i(1))
        );
        assert!(
            !fields.iter().any(|(key, _)| *key == "harness_media"),
            "no aggregate rides the terminal; the per-image frames carry the rows"
        );
        assert_eq!(outcome_json(&record), original);
        assert_eq!(
            record.field("output"),
            Some(serde_json::json!("original text"))
        );
        assert_eq!(
            record.field("harness_media"),
            None,
            "the expression field set is unchanged"
        );
    }
}
