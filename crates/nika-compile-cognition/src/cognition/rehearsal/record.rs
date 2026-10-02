// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A bounded host report as data in the existing compile decision record. No serialized
//! field grants authority or is accepted as a later invocation's rehearsal.

use super::Result;
use crate::rehearse::{Attempt, FinalState, Held, RecordedCause, Rehearsal, RehearsalReport};
use nika_compile_fidelity::behavior::{Run, Usage};
use serde_json::{Value, json};
use std::time::Duration;

pub(super) fn usage(usage: &Usage) -> Value {
    json!({"fixtures": usage.fixtures, "attempts": usage.attempts,
        "copied_bytes": usage.copied_bytes, "read_back_bytes": usage.read_back_bytes,
        "elapsed_ms": usage.elapsed_ms})
}

pub(super) fn report(
    report: &RehearsalReport,
    bound: Duration,
    run: &Run,
    result: &Result,
) -> Value {
    let observation = &report.observation;
    let copies: Vec<Value> = observation.copies.iter().map(|copy| json!({
        "path": copy.path, "source": {"bytes": copy.source.bytes, "sha256": copy.source.sha256},
        "room": copy.room.as_ref().map(|digest| json!({"bytes": digest.bytes, "sha256": digest.sha256})),
    })).collect();
    let finals: Vec<Value> = observation
        .finals
        .iter()
        .map(|receipt| json!({"path": receipt.path, "state": final_state(&receipt.state)}))
        .collect();
    let (attempt, elapsed) = match report.attempt {
        Attempt::NeverAttempted => ("never_attempted", None),
        Attempt::Completed { elapsed_ms } => ("completed", Some(elapsed_ms)),
        Attempt::Stopped { elapsed_ms } => ("stopped", Some(elapsed_ms)),
    };
    let decision = match result {
        Result::Proceed => json!({"kind": "proceed"}),
        Result::Repair(diagnostic) => {
            json!({"kind": "repair", "code": diagnostic.kind, "message": diagnostic.message})
        }
        Result::Stop(reason) => json!({"kind": "stop", "reason": reason}),
    };
    json!({
        "candidate_sha256": report.candidate_sha256, "admitted_digest": report.admitted_digest,
        "outcome": outcome(&report.outcome), "decision": decision,
        "attempt": attempt, "elapsed_ms": elapsed,
        "runtime_bound_ms": u64::try_from(bound.as_millis()).unwrap_or(u64::MAX),
        "room": {"prepared": report.room.prepared, "cleaned": report.room.cleaned, "late_refused": report.room.late_refused},
        "effects": {"network": report.effects.network, "provider": report.effects.provider,
            "spawn": report.effects.spawn, "prompt": report.effects.prompt, "secret": report.effects.secret, "child": report.effects.child},
        "copies": copies, "finals": finals,
        "read_back": run.read_back.iter().map(|read| json!({"path": read.path, "text": read.text,
            "written": read.written, "truncated": read.truncated})).collect::<Vec<_>>(),
        "ledger": {"written": observation.ledger.written, "drained": observation.ledger.drained,
            "leftovers": observation.ledger.leftovers, "late_refused": observation.ledger.late_refused, "panicked": observation.ledger.panicked},
        "bounds": {"time_ms": observation.bounds.time_ms, "room_bytes": observation.bounds.room_bytes, "preview_bytes": observation.bounds.preview_bytes},
        "refusal": observation.refusal.map(crate::rehearse::Refusal::word), "usage": usage(&run.usage),
        "failure": observation.failure.as_ref().map(|failure| json!({
            "task": failure.task, "code": failure.code,
            "cause": match failure.cause { RecordedCause::Engine => "engine", RecordedCause::VerbError => "verb_error",
                RecordedCause::Timeout => "timeout", RecordedCause::RetryExhausted => "retry_exhausted" },
        })),
    })
}

fn outcome(outcome: &Rehearsal) -> Value {
    match outcome {
        Rehearsal::Passed { outputs } => json!({"kind": "passed",
            "outputs": outputs.iter().map(|output| output.path.as_str()).collect::<Vec<_>>()}),
        Rehearsal::Missing { outputs } => json!({"kind": "missing", "outputs": outputs}),
        Rehearsal::Failed {
            task,
            code,
            message,
        } => json!({"kind": "failed", "task": task, "code": code, "message": message}),
        Rehearsal::NotRun { reason } => json!({"kind": "not_run", "reason": reason}),
    }
}

fn final_state(state: &FinalState) -> Value {
    match state {
        FinalState::Absent => json!({"kind": "absent"}),
        FinalState::Directory => json!({"kind": "directory"}),
        FinalState::Unreadable => json!({"kind": "unreadable"}),
        FinalState::File { digest, held } => json!({"kind": "file", "bytes": digest.bytes,
            "sha256": digest.sha256, "text": held.text(),
            "coverage": match held { Held::Whole(_) => "complete", Held::Preview(_) => "prefix", Held::NotText => "not_text" }}),
    }
}
