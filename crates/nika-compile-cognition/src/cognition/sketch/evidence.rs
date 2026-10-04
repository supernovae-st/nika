// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The evidence a sketch candidate faces before its whole-request judgment (slice E): this
//! compile's rehearsal room, then the behavioural judge over the request's own contract (never
//! one the candidate states). A demonstrated defect — a run that failed or missed an output, a
//! result the request's contract refuses — is what a repair of the graph or its fills starts
//! from. What the evidence cannot settle (no supported obligation, a coherent refusal before any
//! attempt, an incomplete observation) stays UNKNOWN: recorded, never a pass and never a repair.
//! A harness the room cannot vouch for stops the candidate without an author repair. Nothing
//! here sends a model request: the sketch door owns the budget and decides whether to repair.

use std::collections::BTreeMap;

use nika_compile_fidelity::behavior::{Outcome, Report, contract_of_request};
use serde_json::{Value, json};

use super::super::rehearsal::{Rehearsals, Result as Rehearsed};
use crate::fidelity::Diagnostic;
use crate::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind};

/// What the evidence shows of one candidate.
pub(super) enum Evidence {
    /// No rehearsal host was offered: the door judges as before, claiming nothing more.
    Unoffered,
    /// Every supported obligation was shown on a completed run.
    Holds,
    /// The evidence does not settle the request: recorded with its reason, never a pass.
    Unknown,
    /// A demonstrated defect: the repair's starting point.
    Defect(Diagnostic),
    /// No verdict on the program (an invalid harness, an engine failure, the rehearsal budget
    /// spent): the candidate is not READY and no author repair follows.
    Stop(String),
}

/// The evidence of `out`'s candidate: one rehearsal of its exact bytes over the request's own
/// paths, admitted first against `attempts` rehearsals in all, then the behavioural judgment of
/// that run. Returns the evidence and the record the journal keeps of it.
pub(super) async fn examined(
    rehearsals: &mut Rehearsals<'_>,
    request: &CompileRequest,
    intent: &str,
    out: &CompileOutcome,
    attempts: u32,
) -> (Evidence, Value) {
    if !rehearsals.offered() {
        return (Evidence::Unoffered, Value::Null);
    }
    let candidate = super::super::knowledge::sha256(out.candidate.as_deref().unwrap_or_default());
    let noted = |outcome: &str, behaviour: Value| {
        json!({"phase": "evidence", "candidate_sha256": candidate, "outcome": outcome,
            "behaviour": behaviour})
    };
    if !rehearsals.admits(attempts) {
        let why = format!(
            "The rehearsal budget of this compile ({attempts} run(s)) is spent; no further run was asked."
        );
        return (Evidence::Stop(why), noted("stop", Value::Null));
    }
    // The candidate as the compile will return it: its semantic record bound to the caller,
    // its paths read through the request the final barrier reads.
    let mut view = out.clone();
    let mut read = request.clone();
    if let Some(serves) = rehearsals.serves() {
        super::bind_caller(serves.caller.clone(), &serves.raw, &mut view);
        read = serves.reading.clone();
    }
    match rehearsals.inspect(&read, &view).await.result {
        Rehearsed::Repair(diagnostic) => {
            return (Evidence::Defect(diagnostic), noted("defect", Value::Null));
        }
        Rehearsed::Stop(reason) => return (Evidence::Stop(reason), noted("stop", Value::Null)),
        Rehearsed::Proceed => {}
    }
    let answers: &BTreeMap<String, String> = &request.answers;
    let contract = contract_of_request(intent, answers);
    let Some(report) = (!contract.obligations.is_empty())
        .then(|| rehearsals.judged(&contract, attempts))
        .flatten()
    else {
        let why = "The request states no obligation the behavioural judge supports, or no run of this candidate was observed: its business result is UNKNOWN.";
        return (Evidence::Unknown, noted("unknown", json!({"why": why})));
    };
    let behaviour = summary(&contract, &report);
    let failed: Vec<Value> = (report.judged.iter())
        .filter(|judged| judged.tally.failed > 0)
        .map(|judged| {
            let observed: Vec<&str> = (judged.findings.iter())
                .filter(|finding| finding.outcome == Outcome::Failed)
                .map(|finding| finding.observed.as_str())
                .collect();
            json!({"requested": judged.requested, "observed": observed})
        })
        .collect();
    if !failed.is_empty() {
        let message = json!({"kind": "behaviour_failed", "obligations": failed}).to_string();
        let defect = Diagnostic {
            kind: "behaviour",
            message,
        };
        return (Evidence::Defect(defect), noted("defect", behaviour));
    }
    let shown = (report.judged.iter()).all(|judged| {
        let tally = &judged.tally;
        tally.passed > 0
            && tally.failed + tally.incomplete + tally.not_run + tally.invalid_harness == 0
    });
    if shown {
        return (Evidence::Holds, noted("holds", behaviour));
    }
    let mut behaviour = behaviour;
    behaviour["why"] = json!(
        "The completed run does not show every supported obligation: its business result is UNKNOWN."
    );
    (Evidence::Unknown, noted("unknown", behaviour))
}

/// The judgment's identity in the journal: the contract it read and how each obligation ended.
fn summary(contract: &nika_compile_fidelity::behavior::Contract, report: &Report) -> Value {
    let sum = |pick: fn(&nika_compile_fidelity::behavior::Tally) -> usize| -> usize {
        report.judged.iter().map(|judged| pick(&judged.tally)).sum()
    };
    json!({
        "contract_sha256": super::super::knowledge::sha256(&format!("{contract:?}")),
        "obligations": contract.obligations.len(),
        "passed": sum(|t| t.passed),
        "failed": sum(|t| t.failed),
        "unknown": sum(|t| t.incomplete + t.not_run + t.invalid_harness),
    })
}

/// A candidate the evidence or the budget leaves not READY: withdrawn with its preview, its
/// questions, its requested boundary and its replayable record, the reason stated.
pub(super) fn refuse(out: &mut CompileOutcome, reason: String) {
    out.status = CompileStatus::Incomplete;
    out.candidate = None;
    out.check_preview = None;
    out.requested_boundary = None;
    out.questions.clear();
    out.provenance.plan = None;
    crate::finding(out, DiagnosticKind::Unknown, "rehearsal", reason);
}
