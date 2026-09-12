// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;

use super::{
    Finish, RawTask, SettleAs, TaskRecord, Value, bind_outputs, declassify_evidence,
    filter_leaky_resume,
};

/// Assemble the `Finish` of a RAN task (the output bindings spec 04 ·
/// the resume filter · the F-O1 declassify evidence · the F-P4 approval
/// attestation) — split out of `run_task_pipeline` for the 100-line fn
/// ratchet · semantics unchanged.
// REASON: the ran assembly threads the task + its computed parts — 10
// params, each one a distinct pipeline product (same trade as the caller).
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble_ran_finish(
    task: &RawTask,
    id: String,
    mut settle: SettleAs,
    resume: Option<crate::resume::ResumeStamp>,
    resume_ctx: &crate::resume::ResumeContext,
    inputs: &BTreeMap<String, Value>,
    records: &BTreeMap<String, TaskRecord>,
    integrity: nika_cap::Integrity,
    approval: Option<crate::approval::ApprovalAttestation>,
    jq_clock: nika_cap::JqClock,
) -> Finish {
    // `output:` named bindings (spec 04 §Output binding) — evaluated
    // over the task's FINAL raw output, BEFORE settle emits the
    // terminal frame, so a binding error (NIKA-VAR-002/004) turns a
    // success into a failure (the cascade) rather than landing after
    // a `TaskCompleted`. The map carries one entry per declared
    // binding (the value on success · `Null` on a non-success ·
    // defined-null reads).
    let named = bind_outputs(task, &mut settle, jq_clock);
    let resume = filter_leaky_resume(resume, &settle, resume_ctx);
    // F-O1 PR-3 · the task RAN — the door was used: the receipt
    // carries one `declassify` event per declared entry (the settle
    // spine emits them after `task_started`).
    let mut declassified = declassify_evidence(task, inputs, records);
    if let SettleAs::Ran(ran) = &mut settle {
        // Finish already preserves these receipts through deferred recovery.
        declassified.append(&mut ran.cleanup_declassified);
    }
    Finish {
        id,
        settle,
        named,
        resume,
        integrity,
        declassified,
        approval,
    }
}
