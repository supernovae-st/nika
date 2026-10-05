// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The recorded plan beside the working directory: `.nika/compile/<intent sha256>.plan.json`.
//!
//! One authoring conversation is several `nika compile` invocations of the same intent
//! with more `--answer` each round. Without a record, every round reads (or samples) the
//! intent again, may settle on a different plan, and pays a provider call for nothing.
//! The record keeps the plan the first round produced; an answer round replays it through
//! `CompileRequest::with_plan` (zero provider calls, the same candidate), and so does a plain
//! re-run of a round whose candidate its judge could not judge (`verify_resume`). It carries the
//! plan, the engine that produced it and the intent hash it belongs to: never the
//! candidate, never a key. The directory ignores itself (the cache-directory convention),
//! so the request's verbatim excerpts cannot enter a commit by accident.

use nika_onboard::compile::{
    COMPILE_WIRE_VERSION, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, Strategy,
};
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The directory, relative to the working directory the operator compiles from.
pub(super) const DIR: &str = ".nika/compile";

/// What this invocation did with the record; the human text names it, the machine
/// document only carries a failure (the core's `route` already says `replayed plan`).
pub(super) enum Note {
    /// A fresh compile produced a plan and this invocation recorded it here.
    Recorded(PathBuf),
    /// This invocation replayed the plan recorded here.
    Replayed(PathBuf),
    /// A plan was produced but could not be recorded; the compile itself stands.
    Failed { path: PathBuf, error: String },
}

pub(super) fn path(sha: &str) -> PathBuf {
    Path::new(DIR).join(format!("{sha}.plan.json"))
}

/// The finding a compile leaves on a candidate its judge could not judge, its record kept.
const RESUME: &str = "verify_resume";

/// Whether `out` is the core's resumable round: INCOMPLETE, no candidate offered, its record
/// kept, and the `verify_resume` finding applied. The marker alone, or on any other outcome, is
/// no such round.
fn resumable(out: &CompileOutcome) -> bool {
    out.status == CompileStatus::Incomplete
        && out.candidate.is_none()
        && out.provenance.plan.is_some()
        && (out.diagnostics.iter()).any(|d| d.kind == DiagnosticKind::Applied && d.target == RESUME)
}

/// Whether this invocation attaches `record`: an answer round (one or more `--answer`), or a
/// plain re-run of a round the judge could not judge; never `--fresh`.
fn attaches(record: &Value, args: &super::CompileArgs) -> bool {
    !args.fresh && (!args.answers.is_empty() || record["resume"] == json!(true))
}

/// The record for this intent, attached to the request when [`attaches`] says so (the judge is
/// then asked again on the same bytes, with no new authoring call); any other round leaves the
/// request as it is.
pub(super) fn replay(
    sha: Option<&str>,
    args: &super::CompileArgs,
    request: CompileRequest,
) -> (CompileRequest, Option<Note>) {
    if let Some(sha) = sha
        && let Some(record) = load_record(sha).filter(|record| attaches(record, args))
        && let Some(plan) = plan_of(&record)
    {
        return (request.with_plan(plan), Some(Note::Replayed(path(sha))));
    }
    (request, None)
}

/// After the compile: a replay keeps its note and rewrites nothing, unless the compiler
/// re-anchored its plan to a changed source (R4 A6), or a resumed round was judged this time:
/// the record is then replaced, atomically, so the next answer binds against the observation the
/// question showed and a judged round no longer resumes; a fresh compile that settled a plan
/// records it; anything else records nothing.
pub(super) fn keep(sha: Option<&str>, note: Option<Note>, out: &CompileOutcome) -> Option<Note> {
    let resumed = sha
        .and_then(load_record)
        .is_some_and(|record| record["resume"] == json!(true));
    let judged = resumed && !resumable(out);
    if note.is_some() && !judged && !sha.is_some_and(|sha| reanchored(sha, out)) {
        return note;
    }
    // Judged with nothing to record (a refusal, no plan): the resuming record goes, so a plain
    // re-run reads the intent again instead of resuming a judged round forever.
    if let Some(sha) = sha.filter(|_| judged && !recordable(out)) {
        return match std::fs::remove_file(path(sha)) {
            Ok(()) => note,
            Err(error) => Some(Note::Failed {
                path: path(sha),
                error: error.to_string(),
            }),
        };
    }
    let sha = sha.filter(|_| recordable(out))?;
    Some(match record(sha, out) {
        Ok(path) => Note::Recorded(path),
        Err(error) => Note::Failed {
            path: path(sha),
            error: error.to_string(),
        },
    })
}

/// The plan recorded for this intent by THIS engine, with its strategy word riding
/// inside, or nothing: an absent, unreadable, foreign-engine or foreign-intent record is
/// simply not replayed, and the fresh compile that follows rewrites it.
pub(super) fn load(sha: &str) -> Option<Value> {
    plan_of(&load_record(sha)?)
}

/// The record THIS engine wrote for this intent, or nothing.
fn load_record(sha: &str) -> Option<Value> {
    load_record_from(sha, &path(sha))
}

fn load_record_from(sha: &str, path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let record: Value = serde_json::from_str(&text).ok()?;
    (record.get("compile_version") == Some(&json!(COMPILE_WIRE_VERSION))
        && record.get("engine").and_then(Value::as_str) == Some(env!("CARGO_PKG_VERSION"))
        && record.get("intent_sha256").and_then(Value::as_str) == Some(sha))
    .then_some(record)
}

/// A record's plan: legacy plans carry their strategy inside; semantic records are closed.
/// Keep the latter unchanged so the compiler rebuilds and judges their graph and fills.
fn plan_of(record: &Value) -> Option<Value> {
    let mut plan = record.get("plan").filter(|plan| plan.is_object())?.clone();
    if plan.get("semantic_record").is_none()
        && plan.get("strategy").is_none()
        && let Some(strategy) = record.get("strategy").and_then(Value::as_str)
    {
        plan["strategy"] = json!(strategy);
    }
    Some(plan)
}

/// Whether the compiler re-anchored the recorded plan: its observation or the keys it asked
/// again moved, and neither plan carries an approval (a verified or pending transform stays
/// bound to the plan that authored it, never carried to another source).
fn reanchored(sha: &str, out: &CompileOutcome) -> bool {
    let (Some(recorded), Some(plan)) = (load(sha), out.provenance.plan.as_ref()) else {
        return false;
    };
    let moved = ["observed_world", "reasked"]
        .iter()
        .any(|k| recorded.get(*k) != plan.get(*k));
    let approval = ["verified_transform", "pending_transform"]
        .iter()
        .any(|k| recorded.get(*k).is_some() || plan.get(*k).is_some());
    moved && !approval
}

/// Only a settled general-path plan is worth replaying: skeletons and the support
/// grammar have none, and a plan the compiler refused to assemble must not be assembled
/// on the next round either.
pub(super) fn recordable(out: &CompileOutcome) -> bool {
    out.provenance.plan.is_some()
        && matches!(
            out.provenance.strategy,
            Some(Strategy::Hot | Strategy::Warm | Strategy::Cold | Strategy::Native)
        )
}

/// Record the outcome's plan for this intent, atomically, in a self-ignoring directory.
pub(super) fn record(sha: &str, out: &CompileOutcome) -> std::io::Result<PathBuf> {
    record_in(sha, out, Path::new(DIR))
}

fn record_in(sha: &str, out: &CompileOutcome, dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")?;
    }
    let record = json!({
        "compile_version": COMPILE_WIRE_VERSION,
        "engine": out.provenance.compiler_version,
        "intent_sha256": sha,
        "plan": out.provenance.plan,
        "strategy": out.provenance.strategy.map(Strategy::word),
        "resume": resumable(out),
        "created_at": jiff::Timestamp::now().to_string(),
    });
    let target = dir.join(format!("{sha}.plan.json"));
    let mut pending = tempfile::NamedTempFile::new_in(dir)?;
    pending.write_all(serde_json::to_string_pretty(&record)?.as_bytes())?;
    pending.write_all(b"\n")?;
    pending.as_file().sync_all()?;
    pending.persist(&target).map_err(|error| error.error)?;
    Ok(target)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use clap::Parser as _;
    use serde_json::json;

    #[derive(clap::Parser)]
    struct Door {
        #[command(flatten)]
        args: super::super::CompileArgs,
    }

    fn args(argv: &[&str]) -> super::super::CompileArgs {
        let base = ["compile", "Read ./a.md and write ./b.md"];
        let parsed = Door::try_parse_from(base.iter().chain(argv).copied());
        parsed.expect("parses").args
    }

    #[test]
    fn a_round_the_judge_could_not_judge_resumes_on_a_plain_rerun_never_on_fresh() {
        let (resume, settled) = (json!({"resume": true}), json!({"resume": false}));
        // A plain re-run attaches only a record that resumes; an older record without the
        // field never does.
        assert!(super::attaches(&resume, &args(&[])));
        assert!(!super::attaches(&settled, &args(&[])));
        assert!(!super::attaches(&json!({}), &args(&[])));
        // An answer round attaches either; `--fresh` attaches neither.
        let answered = args(&["--answer", "const.x=\"y\""]);
        assert!(super::attaches(&settled, &answered));
        assert!(super::attaches(&resume, &answered));
        assert!(!super::attaches(&resume, &args(&["--fresh"])));
    }

    #[test]
    fn only_the_cores_whole_resume_contract_is_resumable() {
        use nika_onboard::compile::{CompileRequest, CompileStatus, DiagnosticKind, compile};
        // A real outcome reshaped field by field (the diagnostic type has no public builder).
        let intent = "Read ./a.md and do something clever with it, then write ./b.md";
        let mut out = compile(&CompileRequest::create(intent)).expect("compiles");
        let mut marker = (out.diagnostics.first().cloned()).expect("a diagnostic to reshape");
        marker.kind = DiagnosticKind::Applied;
        marker.target = super::RESUME.to_owned();
        out.diagnostics.push(marker);
        out.status = CompileStatus::Incomplete;
        out.candidate = None;
        out.provenance.plan = Some(json!({"semantic_record": 1}));
        assert!(super::resumable(&out));
        // Every other shape is no resumable round: each part of the contract is required.
        let mut ready = out.clone();
        ready.status = CompileStatus::Ready;
        let mut offered = out.clone();
        offered.candidate = Some("nika: x".to_owned());
        let mut unrecorded = out.clone();
        unrecorded.provenance.plan = None;
        let mut unknown = out.clone();
        let mut elsewhere = out.clone();
        let (last_unknown, last_elsewhere) = (
            unknown.diagnostics.last_mut().expect("the marker"),
            elsewhere.diagnostics.last_mut().expect("the marker"),
        );
        last_unknown.kind = DiagnosticKind::Unknown;
        last_elsewhere.target = "semantic_verification".to_owned();
        for (case, shape) in [
            ("ready", ready),
            ("offered", offered),
            ("unrecorded", unrecorded),
            ("unknown kind", unknown),
            ("other target", elsewhere),
        ] {
            assert!(!super::resumable(&shape), "{case}");
        }
    }
}

#[cfg(test)]
mod replay_tests;
