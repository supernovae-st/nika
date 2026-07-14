// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task terminal-frame emission — `task_completed` (+ its `task_recovered`
//! prefix frame) extracted from the settle path (lib.rs sat at the
//! 1500-line file cap; this trio is the cohesive cut).

use crate::record::{TaskRecord, outcome_json};
use crate::{EventKind, EventSink, FieldValue, Stamper, emit, i, resume, s};

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
    warning: Option<&str>,
    resume: Option<&resume::ResumeStamp>,
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
    // OBS-E · a non-fatal diagnostic rides the success frame as a
    // `warning` field (the reasoning-model blank-answer footgun) · the
    // task still completes.
    if let Some(msg) = warning {
        fields.push(("warning", s(msg)));
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
    // Spec 13 · trace_format: 2 — every terminal task event carries the
    // outcome (class · cause · payload per class).
    let outcome = outcome_json(record);
    fields.push(("outcome", s(&outcome)));
    emit(stamper, sink, EventKind::TaskCompleted, &fields)
}
