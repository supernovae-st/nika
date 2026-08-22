// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The settle spine — one task's settlement in wave order (frames ·
//! records · spend fields), the single site `recover`'s park/drain
//! passes settle through.
//!
//! Split out of `lib.rs` at the C2 wall (the 1500-LOC file ratchet —
//! settlement is ONE coherent unit: `settle` owns the pens, every
//! terminal shape rides through it). Every door keeps its signature.

use std::collections::BTreeMap;

use nika_event::EventKind;
use nika_types::resource::Value as FieldValue;
use serde_json::Value;

use crate::record::{self, TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause};
use crate::stamp::{EventSink, Stamper};
use crate::task::{self, Finish, SettleAs};
use crate::{agent_events, child, emit, emit_task, i, resume, s};

/// The `SkippedGate` arm of the settle (skipped/gate · the gate's own
/// CEL text rides verbatim) — split for the 100-line fn ratchet.
fn settle_skipped_gate(
    id: &str,
    note: &str,
    expr: Option<&str>,
    named: std::collections::BTreeMap<String, Value>,
    records: &mut BTreeMap<String, TaskRecord>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let record = with_named(
        TaskRecord::unran(TaskStatus::Skipped, TerminalCause::Gate),
        named,
    );
    let mut fields = vec![("task", s(id)), ("note", s(note))];
    if let Some(cel) = &expr {
        fields.push(("when", s(cel)));
    }
    fields.push(("outcome", s(&record::outcome_json(&record))));
    emit(stamper, sink, EventKind::TaskSkipped, &fields);
    records.insert(id.to_owned(), record);
}

/// The Cancelled arm of the settle (spec 13 · cancelled/upstream) — the
/// WHY rides along: which upstream kept the gate closed.
fn settle_cancelled(
    id: &str,
    note: &str,
    blocked_by: Option<&str>,
    named: std::collections::BTreeMap<String, Value>,
    records: &mut BTreeMap<String, TaskRecord>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let record = with_named(
        TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Upstream),
        named,
    );
    let mut fields = vec![("task", s(id)), ("note", s(note))];
    if let Some(culprit) = blocked_by {
        fields.push(("blocked_by", s(culprit)));
    }
    fields.push(("outcome", s(&record::outcome_json(&record))));
    emit(stamper, sink, EventKind::TaskCancelled, &fields);
    records.insert(id.to_owned(), record);
}

/// Settle a pre-dispatch failure (gate eval · `with:` render ·
/// `for_each` collection · an F-P4 approval refusal) — the task never
/// started: no `TaskStarted` · no `on_finally` (spec 03) · the failure
/// cascades. The one boundary evaluation IS the settling attempt (spec
/// 13 · `failure/verb_error` · attempts = 1). Split out of [`settle`]
/// for the 100-line fn ratchet · semantics unchanged.
#[allow(clippy::too_many_arguments)] // the failure parts + the pens
fn settle_failed_before_start(
    id: &str,
    stage: &'static str,
    error: TaskErrorRecord,
    named: BTreeMap<String, Value>,
    approval: Option<&crate::approval::ApprovalAttestation>,
    records: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let mut record = TaskRecord::unran(TaskStatus::Failure, TerminalCause::VerbError);
    record.attempts = Some(1);
    let detail = format!("{} · {}", error.code, error.message);
    record.error = Some(error);
    let record = with_named(record, named);
    // F-P4 — an approval REFUSAL attests BEFORE its failure frame (the
    // deny the chain binds · NEP-0013 law 4).
    emit_approval(approval, stamper, sink);
    emit(
        stamper,
        sink,
        EventKind::TaskFailed,
        &[
            ("task", s(id)),
            ("note", s(stage)),
            ("detail", s(&detail)),
            ("outcome", s(&record::outcome_json(&record))),
        ],
    );
    records.insert(id.to_owned(), record);
    *ok = false;
}

/// Emit one `approval_decided` frame for a decided ticket (NEP-0013 law
/// 4) — the one emission site, shared by the Ran path (between
/// `task_started` and the terminal frame) and the refusal path (before
/// `task_failed`).
fn emit_approval(
    approval: Option<&crate::approval::ApprovalAttestation>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    if let Some(attestation) = approval {
        emit(
            stamper,
            sink,
            EventKind::ApprovalDecided,
            &attestation.fields(),
        );
    }
}

/// Settle one task in wave order · owns the pens (stamper + sink) ·
/// inserts the result record. `pub(crate)`: the recover-await spine
/// (`recover::settle_or_park` + the drain passes) settles through THIS
/// one site — parked stories keep the same frames as live ones.
pub(crate) fn settle(
    finish: Finish,
    records: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    cache_hits: &mut Vec<String>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let id = finish.id;
    // `output:` named bindings (spec 04) — the same map rides every
    // outcome: the evaluated values on success · all-`Null` on a
    // non-success (defined-null reads · empty when no `output:`).
    let named = finish.named;
    let resume = finish.resume;
    // F-O1 — the coarse integrity label the pipeline computed (trusted
    // for a never-started task); stamped on the record + rides the
    // terminal frame when untrusted. Additive — no gate reads it (PR-2).
    let integrity = finish.integrity;
    // F-O1 PR-3 — the `declassify:` receipt evidence (NEP-0004 law 5);
    // emitted once per entry on the Ran path (the door was used).
    let declassified = finish.declassified;
    // F-P4 — the approval attestation (NEP-0013 law 4): `Some` when a
    // prompt's ticket decided (or was refused) THIS run.
    let approval = finish.approval;
    match finish.settle {
        SettleAs::Cancelled { note, blocked_by } => {
            settle_cancelled(
                &id,
                note,
                blocked_by.as_deref(),
                named,
                records,
                stamper,
                sink,
            );
        }
        SettleAs::SkippedGate { note, expr } => {
            settle_skipped_gate(&id, note, expr.as_deref(), named, records, stamper, sink);
        }
        SettleAs::FailedBeforeStart { stage, error } => {
            settle_failed_before_start(
                &id,
                stage,
                error,
                named,
                approval.as_ref(),
                records,
                ok,
                stamper,
                sink,
            );
        }
        SettleAs::CacheHit { output } => {
            let record = settle_cache_hit(
                &id,
                output,
                named,
                resume.as_ref(),
                &integrity,
                stamper,
                sink,
            );
            records.insert(id.clone(), record);
            cache_hits.push(id);
        }
        SettleAs::Ran(run) => {
            let mut record = settle_ran(
                &id,
                *run,
                resume.as_ref(),
                &integrity,
                &declassified,
                approval.as_ref(),
                ok,
                stamper,
                sink,
            );
            record.named = named;
            records.insert(id, record);
        }
    }
}

/// Emit one `declassify` frame per declared entry (NEP-0004 law 5 · the
/// only door through the re-gate): the receipt commits to WHAT was
/// lifted (`from` · the taint path's binding), WHY (`because`), and the
/// digest of the value the door admitted (`value_digest`, when the
/// binding resolved at dispatch). Emitted BETWEEN `task_started` and
/// the terminal frame — the hash chain binds the lift to the task that
/// consumed it.
fn emit_declassified(
    id: &str,
    evidence: &[task::DeclassifyEvidence],
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    for entry in evidence {
        let mut fields = vec![
            ("task", s(id)),
            ("from", s(&entry.from)),
            ("because", s(&entry.because)),
        ];
        if let Some(digest) = &entry.value_digest {
            fields.push(("value_digest", s(digest)));
        }
        emit(stamper, sink, EventKind::Declassify, &fields);
    }
}

/// ADR-099 §2 — the skip is VISIBLE: one `task_cache_hit` frame carrying
/// the matched identity + the rehydrated output (so a resumed run's own
/// trace stays resumable). No `TaskStarted` · no duration — the task
/// never ran here. Downstream observes a plain success (spec
/// vocabulary), so the outcome reads `success/normal`; the rehydration
/// is the settling attempt (attempts = 1).
fn settle_cache_hit(
    id: &str,
    output: Value,
    named: BTreeMap<String, Value>,
    resume: Option<&resume::ResumeStamp>,
    integrity: &nika_cap::Integrity,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> TaskRecord {
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
    record.attempts = Some(1);
    record.output = output;
    record.named = named;
    // The rehydrated output carries the task's provenance (F-O1).
    record.integrity = integrity.clone();
    let mut fields = vec![("task", s(id)), ("note", s("cache hit"))];
    let output_text = serde_json::to_string(&record.output).unwrap_or_else(|_| "null".to_owned());
    if let Some(stamp) = resume {
        fields.push((resume::fields::DEF_HASH, s(&stamp.def_hash)));
        fields.push((resume::fields::INPUT_HASH, s(&stamp.input_hash)));
        fields.push((resume::fields::OUTPUT, s(&output_text)));
    }
    fields.push(("outcome", s(&record::outcome_json(&record))));
    emit_task::push_integrity_fields(&mut fields, &record);
    let ended = emit(stamper, sink, EventKind::TaskCacheHit, &fields);
    record.ended_at = Some(ended);
    record
}

/// Attach the `output:` named bindings to a record (spec 04 · the bindings
/// ride every outcome · null on a non-success).
fn with_named(mut record: TaskRecord, named: BTreeMap<String, Value>) -> TaskRecord {
    record.named = named;
    record
}

/// Settle a task that RAN — the started frame · the retry history ·
/// the terminal frame · the result record (spec §3.9). A SUCCESS with a
/// resume stamp carries the ADR-099 identity + output on its
/// `task_completed` frame (additive trace fields · the checkpoint).
/// The attempt history, one `TaskRetrying` frame per retry — split from
/// [`settle_ran`] at the 100-line cap (the block is self-contained: it
/// reads only the retry ledger and touches no record state).
fn emit_retry_history(
    id: &str,
    retries: &[task::RetryStamp],
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    for r in retries {
        emit(
            stamper,
            sink,
            EventKind::TaskRetrying,
            &[
                ("task", s(id)),
                ("attempt", i(i64::from(r.attempt))),
                ("max_attempts", i(i64::from(r.max_attempts))),
                ("delay_ms", i(i64::try_from(r.delay_ms).unwrap_or(i64::MAX))),
            ],
        );
    }
}

/// The Ran preamble (NEP-0004 law 5 · the declassify door BEFORE the
/// attempt history · then the NEP-0007 permit witness · retries · the
/// agent loop's decisions ADR-096) — split for the 100-line fn ratchet.
fn emit_ran_preamble(
    id: &str,
    declassified: &[task::DeclassifyEvidence],
    run: &task::RanTask,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    emit_declassified(id, declassified, stamper, sink);
    emit_permit_checked(id, &run.decisions, stamper, sink);
    emit_retry_history(id, &run.retries, stamper, sink);
    agent_events::emit_agent_events(id, &run.agent_events, stamper, sink);
}

/// Emit one `permit_checked` frame per dispatch-boundary decision
/// (NEP-0007 law 2 · spec 17 §the permit witness): granted and refused
/// alike, between `task_started` and the terminal frame — the hash
/// chain binds the exercised authority to the task that exercised it.
fn emit_permit_checked(
    id: &str,
    decisions: &[crate::witness::PermitDecision],
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    for d in decisions {
        emit(
            stamper,
            sink,
            EventKind::PermitChecked,
            &[
                ("task", s(id)),
                ("plane", s(d.plane)),
                ("gate", s(&d.gate)),
                ("decision", s(d.decision)),
                ("why", s(&d.why)),
            ],
        );
    }
}

#[allow(clippy::too_many_arguments)] // the ran settle parts + the pens
pub(crate) fn settle_ran(
    id: &str,
    run: task::RanTask,
    resume: Option<&resume::ResumeStamp>,
    integrity: &nika_cap::Integrity,
    declassified: &[task::DeclassifyEvidence],
    approval: Option<&crate::approval::ApprovalAttestation>,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> TaskRecord {
    let started_at = emit_ran_prologue(id, &run, declassified, approval, stamper, sink);
    let duration = i64::try_from(run.duration_ms).unwrap_or(i64::MAX);
    // Every attempt including the settling one (spec 13 §payload).
    let attempts = run.attempts();
    // F-P6 · the settling dispatch's binding evidence (lifted before
    // `run.result` moves — EVERY terminal shape can carry it).
    let evidence = run.evidence;
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
    record.started_at = Some(started_at);
    record.duration_ms = Some(run.duration_ms);
    // F-O1 — the pipeline's computed label, set BEFORE any terminal frame
    // so the frame's additive fields read the settled truth.
    record.integrity = integrity.clone();
    match run.result {
        task::RunResult::Success {
            value,
            tokens,
            recovered_from,
            warning,
            child,
            cost_usd,
            cost_unpriced,
            model,
            access_receipt,
        } => settle_success_terminal(
            id,
            &run.note,
            duration,
            (value, tokens, recovered_from, warning),
            child.as_deref(),
            (cost_usd, cost_unpriced, model, access_receipt),
            attempts,
            resume,
            evidence.as_ref(),
            &mut record,
            stamper,
            sink,
        ),
        task::RunResult::SkippedWithError {
            error,
            cost_usd,
            cost_unpriced,
            access_receipt,
        } => settle_skip_with_error(
            id,
            error,
            (cost_usd, cost_unpriced, access_receipt.as_ref()),
            evidence.as_ref(),
            &mut record,
            stamper,
            sink,
        ),
        task::RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
            access_receipt,
        } => settle_failed_terminal(
            id,
            &run.note,
            duration,
            error,
            (cost_usd, cost_unpriced, access_receipt.as_ref()),
            attempts,
            evidence.as_ref(),
            &mut record,
            ok,
            stamper,
            sink,
        ),
        // Backstop: a pending recovery parks BEFORE settle (the
        // `recover::settle_or_park` spine) — one reaching this site
        // settles its classification-time failure (total · no panic).
        task::RunResult::PendingRecovery(pending) => settle_pending_backstop(
            id,
            &run.note,
            duration,
            *pending,
            attempts,
            evidence.as_ref(),
            &mut record,
            ok,
            stamper,
            sink,
        ),
    }
    record
}

/// The Ran prologue — `task_started` · the preamble (declassify ·
/// permit witness · retries · agent decisions) · the F-P4 approval
/// decision BETWEEN `task_started` and the terminal frame (the
/// declassify/permit-witness idiom · NEP-0013 law 4: the chain binds the
/// answered hash to the task it armed) — split for the 100-line fn
/// ratchet · semantics unchanged. Returns the `task_started` timestamp.
fn emit_ran_prologue(
    id: &str,
    run: &task::RanTask,
    declassified: &[task::DeclassifyEvidence],
    approval: Option<&crate::approval::ApprovalAttestation>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> nika_types::timestamp::Timestamp {
    let started_at = emit(
        stamper,
        sink,
        EventKind::TaskStarted,
        &[("task", s(id)), ("note", s(&run.note))],
    );
    emit_ran_preamble(id, declassified, run, stamper, sink);
    emit_approval(approval, stamper, sink);
    started_at
}

/// The pending-recovery backstop arm of [`settle_ran`] (a park only ever
/// happens BEFORE settle — one reaching here settles its
/// classification-time failure · total, no panic) — split for the
/// 100-line fn ratchet · semantics unchanged.
// REASON: the terminal frame's field surface + the settle pens.
#[allow(clippy::too_many_arguments)]
fn settle_pending_backstop(
    id: &str,
    note: &str,
    duration: i64,
    pending: crate::recover::PendingRecovery,
    attempts: u32,
    evidence: Option<&crate::dispatch::commit::CommitEvidence>,
    record: &mut TaskRecord,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let crate::recover::PendingRecovery {
        failed,
        render_error,
        ..
    } = pending;
    settle_failed_terminal(
        id,
        note,
        duration,
        render_error,
        (
            failed.cost_usd,
            failed.cost_unpriced,
            failed.access_receipt.as_deref(),
        ),
        attempts,
        evidence,
        record,
        ok,
        stamper,
        sink,
    );
}

/// The failure terminal — the ONE site for a settled failure's frame +
/// record (the `Failed` arm and the pending-recovery backstop share it).
/// Billed-then-failed spend rides the frame — the dollars a dying task
/// burned must never vanish with it (already ledger-debited per attempt;
/// this is the frame's per-task truth).
/// The success terminal — `task_recovered` (when the success was
/// repaired · D-2026-07-08-N4 sequence) then `task_completed`, with the
/// spec-13 outcome derived from the settled record: `success/normal` or
/// `success/recovered` (+ the ORIGINAL error as `recovered_from`).
// REASON: the terminal frame's field surface + the settle pens — the
// same shape as `settle_ran` itself.
#[allow(clippy::too_many_arguments)]
fn settle_success_terminal(
    id: &str,
    note: &str,
    duration: i64,
    (value, tokens, recovered_from, warning): (
        Value,
        Option<i64>,
        Option<TaskErrorRecord>,
        Option<String>,
    ),
    child: Option<&child::ChildRunSummary>,
    (cost_usd, cost_unpriced, model, access_receipt): (
        Option<f64>,
        Option<nika_types::cost::UnpricedReason>,
        Option<String>,
        Option<crate::dispatch::AccessReceipt>,
    ),
    attempts: u32,
    resume: Option<&resume::ResumeStamp>,
    evidence: Option<&crate::dispatch::commit::CommitEvidence>,
    record: &mut TaskRecord,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    if let Some(original) = &recovered_from {
        emit_task::emit_recovered(id, &original.code, stamper, sink);
    }
    record.cause = if recovered_from.is_some() {
        TerminalCause::Recovered
    } else {
        TerminalCause::Normal
    };
    record.attempts = Some(attempts);
    record.recovered_from = recovered_from;
    record.output = value;
    let ended = emit_task::emit_completed(
        id,
        note,
        duration,
        tokens,
        cost_usd,
        cost_unpriced,
        model.as_deref(),
        access_receipt.as_ref(),
        warning.as_deref(),
        child,
        resume,
        evidence,
        record,
        stamper,
        sink,
    );
    record.ended_at = Some(ended);
    record.access_receipt = access_receipt;
}

// REASON: the terminal frame's field surface + the settle pens — the
// same shape as `settle_ran` itself.
#[allow(clippy::too_many_arguments)]
fn settle_failed_terminal(
    id: &str,
    note: &str,
    duration: i64,
    error: TaskErrorRecord,
    spend: (
        Option<f64>,
        Option<nika_types::cost::UnpricedReason>,
        Option<&crate::dispatch::AccessReceipt>,
    ),
    attempts: u32,
    evidence: Option<&crate::dispatch::commit::CommitEvidence>,
    record: &mut TaskRecord,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    // The failure-cause triage (spec 13): timeout budget · retries
    // exhausted · plain verb error — assigned at the ONE failure site.
    record.status = TaskStatus::Failure;
    record.cause = record::failure_cause(&error, attempts);
    record.attempts = Some(attempts);
    let detail = format!("{} · {}", error.code, error.message);
    record.error = Some(error);
    record.access_receipt = spend.2.cloned();
    let mut fields = vec![
        ("task", s(id)),
        ("note", s(note)),
        ("detail", s(&detail)),
        ("duration_ms", i(duration)),
    ];
    push_spend_fields(&mut fields, spend.0, spend.1);
    if let Some(receipt) = spend.2 {
        emit_task::push_access_fields(&mut fields, receipt);
    }
    // F-P6 · a divergence refusal carries its finding HERE (never a warn);
    // a post-gate verb failure attests the fired ≡ judged digests.
    push_commit_fields(&mut fields, evidence);
    fields.push(("outcome", s(&record::outcome_json(record))));
    emit_task::push_integrity_fields(&mut fields, record);
    let ended = emit(stamper, sink, EventKind::TaskFailed, &fields);
    record.ended_at = Some(ended);
    *ok = false;
}

/// `on_error: skip` — the ONE state where status is skipped AND the
/// error stays readable (spec 05). The billed spend of the skipped
/// attempts rides the frame (skip ≠ refund). Outcome: `skipped/error_skip`
/// with the PRESERVED error (spec 13 · the skipped payload law carries
/// the error only — attempts stay off the record, per the closed table).
fn settle_skip_with_error(
    id: &str,
    error: TaskErrorRecord,
    spend: (
        Option<f64>,
        Option<nika_types::cost::UnpricedReason>,
        Option<&crate::dispatch::AccessReceipt>,
    ),
    evidence: Option<&crate::dispatch::commit::CommitEvidence>,
    record: &mut TaskRecord,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    record.status = TaskStatus::Skipped;
    record.cause = TerminalCause::ErrorSkip;
    let detail = format!("{} · {}", error.code, error.message);
    record.error = Some(error);
    record.access_receipt = spend.2.cloned();
    let mut fields = vec![
        ("task", s(id)),
        ("note", s("on_error · skip")),
        ("detail", s(&detail)),
    ];
    push_spend_fields(&mut fields, spend.0, spend.1);
    if let Some(receipt) = spend.2 {
        emit_task::push_access_fields(&mut fields, receipt);
    }
    // F-P6 · a skipped divergence refusal keeps its finding (never a warn).
    push_commit_fields(&mut fields, evidence);
    fields.push(("outcome", s(&record::outcome_json(record))));
    emit_task::push_integrity_fields(&mut fields, record);
    let ended = emit(stamper, sink, EventKind::TaskSkipped, &fields);
    record.ended_at = Some(ended);
}

/// Push the spend pair onto a frame's fields — absent stays absent
/// (never a fake zero), the WHY rides when named.
fn push_spend_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    cost_usd: Option<f64>,
    cost_unpriced: Option<nika_types::cost::UnpricedReason>,
) {
    if let Some(c) = cost_usd {
        fields.push(("cost_usd", FieldValue::Float(c)));
    }
    if let Some(reason) = cost_unpriced {
        fields.push(("cost_unpriced", s(reason.as_str())));
    }
}

/// F-P6 · the binding evidence — the fired step's digests (`preview_digest`
/// · `commit_digest`) or the refusal's `divergence: {preview, commit}` finding
/// (compact JSON text · the `child` row precedent). `pub(crate)` for `emit_task`.
pub(crate) fn push_commit_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    evidence: Option<&crate::dispatch::commit::CommitEvidence>,
) {
    match evidence {
        Some(crate::dispatch::commit::CommitEvidence::Fired(attestation)) => {
            fields.push(("preview_digest", s(&attestation.preview_digest)));
            fields.push(("commit_digest", s(&attestation.commit_digest)));
        }
        Some(crate::dispatch::commit::CommitEvidence::Refused(d)) => {
            fields.push(("divergence", s(&d.frame_json())));
        }
        None => {}
    }
}
