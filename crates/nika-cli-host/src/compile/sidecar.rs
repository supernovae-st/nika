// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The recorded plan beside the working directory: `.nika/compile/<intent sha256>.plan.json`.
//!
//! One authoring conversation is several `nika compile` invocations of the same intent
//! with more `--answer` each round. Without a record, every round reads (or samples) the
//! intent again, may settle on a different plan, and pays a provider call for nothing.
//! The record keeps the plan the first round produced; an answer round replays it through
//! `CompileRequest::with_plan` (no authoring call, the same candidate; a judge the round asks
//! makes its own calls), and so does a plain
//! re-run of a round whose candidate its judge could not judge (`verify_resume`). A round whose
//! judge answered its candidate and did not accept it (`verify_held`) removes the record: no
//! later round replays those bytes to that judge. It carries the
//! plan, the engine that produced it and the intent hash it belongs to: never the
//! candidate, never a key. The directory ignores itself (the cache-directory convention),
//! so the request's verbatim excerpts cannot enter a commit by accident.
//!
//! Beside it, `.nika/compile/<intent sha256>.declined.json` keeps the verdicts that rejected
//! candidate bytes of the intent in earlier rounds (each a `semantic_verification` attempt, the
//! latest per bytes, judge, request and context), under the same engine and wire-version guard:
//! every compile of the intent carries them, `--fresh` or not (`CompileRequest::with_declined`),
//! so a round that authors those bytes again never asks the same judge on them (R6). An
//! abstention is never kept: a later round may still decide it.

use nika_onboard::compile::{
    COMPILE_WIRE_VERSION, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, Strategy,
    round,
};
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::{Path, PathBuf};

/// The directory, relative to the working directory the operator compiles from.
pub(super) const DIR: &str = ".nika/compile";

/// What this invocation did with the record; the human text names it, the machine
/// document only carries a failure (the core's `route` already says `replayed plan`).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Note {
    /// A fresh compile produced a plan and this invocation recorded it here.
    Recorded(PathBuf),
    /// This invocation replayed the plan recorded here.
    Replayed(PathBuf),
    /// A plan was produced but could not be recorded; the compile itself stands.
    Failed { path: PathBuf, error: String },
    /// The judge answered this round's candidate and did not accept it: the record here, which
    /// a later round would replay to that judge, was removed.
    Removed(PathBuf),
    /// The record of a candidate its judge did not accept could not be removed: a later answer
    /// round would replay it to that judge.
    Unremoved { path: PathBuf, error: String },
}

pub(super) fn path(sha: &str) -> PathBuf {
    path_in(Path::new(DIR), sha)
}

/// This intent's record in `dir`.
fn path_in(dir: &Path, sha: &str) -> PathBuf {
    dir.join(format!("{sha}.plan.json"))
}

/// The finding a compile leaves on a candidate its judge could not judge, its record kept.
const RESUME: &str = "verify_resume";

/// The finding a compile leaves on a candidate its judge answered and did not accept.
const HELD: &str = "verify_held";

/// Whether the judge answered `out`'s candidate and did not accept it (the core's applied
/// `verify_held` finding): those bytes are never replayed to that judge again (R6).
fn held(out: &CompileOutcome) -> bool {
    (out.diagnostics.iter()).any(|d| d.kind == DiagnosticKind::Applied && d.target == HELD)
}

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
/// records it; anything else records nothing. A candidate its judge answered and did not accept
/// (`verify_held`) is never recorded, and this intent's record goes, replayed, resumed or left
/// unread this round: no later round replays those bytes to the same judge (R6).
pub(super) fn keep(sha: Option<&str>, note: Option<Note>, out: &CompileOutcome) -> Option<Note> {
    keep_in(Path::new(DIR), sha, note, out)
}

/// [`keep`] with the records in `dir`.
fn keep_in(
    dir: &Path,
    sha: Option<&str>,
    note: Option<Note>,
    out: &CompileOutcome,
) -> Option<Note> {
    if held(out) {
        return match sha {
            Some(sha) => removed(path_in(dir, sha), note),
            None => note,
        };
    }
    let resumed = sha
        .and_then(|sha| load_record_from(sha, &path_in(dir, sha)))
        .is_some_and(|record| record["resume"] == json!(true));
    let judged = resumed && !resumable(out);
    if note.is_some() && !judged && !sha.is_some_and(|sha| reanchored(dir, sha, out)) {
        return note;
    }
    // Judged with nothing to record (a refusal, no plan): the resuming record goes, so a plain
    // re-run reads the intent again instead of resuming a judged round forever.
    if let Some(sha) = sha.filter(|_| judged && !recordable(out)) {
        let path = path_in(dir, sha);
        return match std::fs::remove_file(&path) {
            Ok(()) => note,
            Err(error) => Some(Note::Failed {
                path,
                error: error.to_string(),
            }),
        };
    }
    let sha = sha.filter(|_| recordable(out))?;
    Some(match record_in(sha, out, dir) {
        Ok(path) => Note::Recorded(path),
        Err(error) => Note::Failed {
            path: path_in(dir, sha),
            error: error.to_string(),
        },
    })
}

/// Remove the record at `path` after its candidate was held: the note says it went, or why it
/// could not; with no record there, the round's own note stands.
fn removed(path: PathBuf, note: Option<Note>) -> Option<Note> {
    match std::fs::remove_file(&path) {
        Ok(()) => Some(Note::Removed(path)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => note,
        Err(error) => Some(Note::Unremoved {
            path,
            error: error.to_string(),
        }),
    }
}

/// The record THIS engine wrote for this intent, or nothing: an absent, unreadable,
/// foreign-engine or foreign-intent record is simply not replayed, and the fresh compile that
/// follows rewrites it.
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
fn reanchored(dir: &Path, sha: &str, out: &CompileOutcome) -> bool {
    let recorded = load_record_from(sha, &path_in(dir, sha)).and_then(|record| plan_of(&record));
    let (Some(recorded), Some(plan)) = (recorded, out.provenance.plan.as_ref()) else {
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
fn record_in(sha: &str, out: &CompileOutcome, dir: &Path) -> std::io::Result<PathBuf> {
    let record = json!({
        "compile_version": COMPILE_WIRE_VERSION,
        "engine": out.provenance.compiler_version,
        "intent_sha256": sha,
        "plan": out.provenance.plan,
        "strategy": out.provenance.strategy.map(Strategy::word),
        "resume": resumable(out),
        "created_at": jiff::Timestamp::now().to_string(),
    });
    persist(dir, path_in(dir, sha), &record)
}

/// Write `record` at `target`, atomically, in the self-ignoring directory `dir`.
fn persist(dir: &Path, target: PathBuf, record: &Value) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")?;
    }
    let mut pending = tempfile::NamedTempFile::new_in(dir)?;
    pending.write_all(serde_json::to_string_pretty(record)?.as_bytes())?;
    pending.write_all(b"\n")?;
    pending.as_file().sync_all()?;
    pending.persist(&target).map_err(|error| error.error)?;
    Ok(target)
}

/// What this invocation did with the intent's kept rejections, when the human text names it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Declined {
    /// A verdict kept here rejected this round's candidate bytes: its judge was not asked again.
    Carried(PathBuf),
    /// This round's rejections could not be kept here: a later round could ask a judge again on
    /// bytes it rejected.
    Failed { path: PathBuf, error: String },
}

/// This intent's kept rejections in `dir`.
fn declined_in(dir: &Path, sha: &str) -> PathBuf {
    dir.join(format!("{sha}.declined.json"))
}

/// The request with the verdicts that rejected candidate bytes of this intent in earlier rounds,
/// on every compile of the intent, `--fresh` or not: no judge is asked again on bytes it rejected
/// (R6). None kept, or a file this engine did not write: the request as it is.
pub(super) fn carry(sha: Option<&str>, request: CompileRequest) -> CompileRequest {
    carry_from(Path::new(DIR), sha, request)
}

/// [`carry`] with the files in `dir`.
fn carry_from(dir: &Path, sha: Option<&str>, request: CompileRequest) -> CompileRequest {
    match sha.map(|sha| kept_declined(dir, sha)) {
        Some(kept) if !kept.is_empty() => request.with_declined(kept),
        _ => request,
    }
}

/// The rejections THIS engine kept for this intent, or none: an absent, unreadable,
/// foreign-engine or foreign-intent file is not read, and the next rejection rewrites it.
fn kept_declined(dir: &Path, sha: &str) -> Vec<Value> {
    load_record_from(sha, &declined_in(dir, sha))
        .and_then(|record| record.get("declined").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

/// After the compile: the verdicts that rejected candidate bytes in it (every attempt that
/// judged, never a repeat or a carried copy) kept with this intent's earlier ones, the latest per
/// bytes, judge, request and context, atomically. The note says when a kept verdict decided this
/// round's bytes, or why the file could not be written.
pub(super) fn decline(sha: Option<&str>, out: &CompileOutcome) -> Option<Declined> {
    decline_in(Path::new(DIR), sha, out)
}

/// [`decline`] with the files in `dir`.
fn decline_in(dir: &Path, sha: Option<&str>, out: &CompileOutcome) -> Option<Declined> {
    let sha = sha?;
    let path = declined_in(dir, sha);
    let mut kept = kept_declined(dir, sha);
    if round::keep_rejections(&mut kept, round::rejections(out)) {
        let record = json!({
            "compile_version": COMPILE_WIRE_VERSION,
            "engine": env!("CARGO_PKG_VERSION"),
            "intent_sha256": sha,
            "declined": kept,
        });
        if let Err(error) = persist(dir, path.clone(), &record) {
            return Some(Declined::Failed {
                path,
                error: error.to_string(),
            });
        }
    }
    let attempts = (out.provenance.decision.as_ref())
        .and_then(|decision| decision["semantic_verification"].as_array());
    (attempts.into_iter().flatten())
        .any(|attempt| attempt["carried"] == true)
        .then_some(Declined::Carried(path))
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

    /// A real outcome reshaped as the core leaves a native round: INCOMPLETE, its plan recorded
    /// under the native strategy, and an applied finding on `target` when one is named.
    fn reshaped(target: Option<&str>) -> nika_onboard::compile::CompileOutcome {
        use nika_onboard::compile::{
            CompileRequest, CompileStatus, DiagnosticKind, Strategy, compile,
        };
        let intent = "Read ./a.md and do something clever with it, then write ./b.md";
        let mut out = compile(&CompileRequest::create(intent)).expect("compiles");
        if let Some(target) = target {
            let mut marker = (out.diagnostics.first().cloned()).expect("a diagnostic to reshape");
            marker.kind = DiagnosticKind::Applied;
            target.clone_into(&mut marker.target);
            out.diagnostics.push(marker);
        }
        out.status = CompileStatus::Incomplete;
        out.provenance.strategy = Some(Strategy::Native);
        out.provenance.plan = Some(json!({"semantic_record": 1}));
        out
    }

    /// A candidate its judge answered and did not accept (`verify_held`) is never recorded, and
    /// this intent's record goes whether this round replayed it, resumed it or left it unread:
    /// no later round replays those bytes to the same judge (R6). With no record there, the
    /// round's own note stands; a record that cannot be removed is named with the error met.
    #[test]
    fn a_held_candidate_removes_its_intents_record_and_is_never_recorded() {
        use super::{Note, keep_in, path_in, record_in};
        let room = tempfile::tempdir().expect("room");
        let dir = room.path().join(super::DIR);
        let sha = "a".repeat(64);
        let path = path_in(&dir, &sha);
        // As the core holds a candidate: shown as the preview, its replayable record dropped.
        let mut held = reshaped(Some(super::HELD));
        held.candidate = Some("nika: held\n".to_owned());
        held.provenance.plan = None;
        let mut resuming = reshaped(Some(super::RESUME));
        resuming.candidate = None;
        assert!(super::resumable(&resuming));
        for (case, recorded, note, resume) in [
            (
                "replayed",
                reshaped(None),
                Some(Note::Replayed(path.clone())),
                false,
            ),
            (
                "resumed",
                resuming,
                Some(Note::Replayed(path.clone())),
                true,
            ),
            ("unread", reshaped(None), None, false),
        ] {
            assert_eq!(record_in(&sha, &recorded, &dir).expect("recorded"), path);
            let on_disk = super::load_record_from(&sha, &path).expect("a record of this engine");
            assert_eq!(on_disk["resume"], resume, "{case}");
            let kept = keep_in(&dir, Some(&sha), note, &held);
            assert_eq!(kept, Some(Note::Removed(path.clone())), "{case}");
            assert!(!path.exists(), "{case}");
        }
        // No record there: the round's own note stands, and nothing is recorded, even for a held
        // outcome that would otherwise be recordable.
        let mut planned = held.clone();
        planned.provenance.plan = Some(json!({"semantic_record": 1}));
        assert!(super::recordable(&planned));
        assert_eq!(keep_in(&dir, Some(&sha), None, &planned), None);
        let replayed = Some(Note::Replayed(path.clone()));
        assert_eq!(
            keep_in(&dir, Some(&sha), replayed, &planned),
            Some(Note::Replayed(path.clone()))
        );
        assert_eq!(keep_in(&dir, None, None, &planned), None);
        assert!(!path.exists(), "a held outcome is never recorded");
        // The same outcome without the finding is recorded as before.
        assert_eq!(
            keep_in(&dir, Some(&sha), None, &reshaped(None)),
            Some(Note::Recorded(path.clone()))
        );
        std::fs::remove_file(&path).expect("the record");
        // A record that cannot be removed (a directory stands where it goes) is named.
        std::fs::create_dir(&path).expect("a directory where the record goes");
        let error = std::fs::remove_file(&path)
            .expect_err("no file")
            .to_string();
        assert_eq!(
            keep_in(&dir, Some(&sha), None, &held),
            Some(Note::Unremoved {
                path: path.clone(),
                error
            })
        );
        assert!(path.is_dir());
    }

    /// One verification of the bytes `sha` by the judge `seat`: a rejection, or an abstention.
    fn verdict(sha: &str, seat: &str, rejected: bool) -> serde_json::Value {
        json!({"candidate_sha256": sha, "judge": {"seat": seat, "kind": "authoring_provider"},
            "declined": true, "rejected": rejected, "settled": false, "carried": false})
    }

    /// A native outcome whose verifications are `attempts`.
    fn judged(attempts: &[serde_json::Value]) -> nika_onboard::compile::CompileOutcome {
        let mut out = reshaped(None);
        out.provenance.decision = Some(json!({"semantic_verification": attempts}));
        out
    }

    /// The verdicts that rejected candidate bytes of an intent are kept beside its plan record,
    /// one per bytes and judge, appended, under this engine's guard, in the self-ignoring
    /// directory; every compile of the intent carries them, whatever its flags, and no other
    /// intent's. An abstention, or a compile of no recorded intent, keeps nothing; a verdict this
    /// round carried is named and not kept twice; a file another engine wrote is not read, and
    /// the next rejection rewrites it; a file that cannot be written is named with the error met.
    #[test]
    fn an_intents_rejections_are_kept_once_and_carried_into_every_compile_of_it() {
        use super::{Declined, carry_from, decline_in, declined_in, kept_declined};
        use nika_onboard::compile::{COMPILE_WIRE_VERSION, CompileRequest};
        let room = tempfile::tempdir().expect("room");
        let dir = room.path().join(super::DIR);
        let sha = "b".repeat(64);
        let path = declined_in(&dir, &sha);
        let (b1, b2) = (verdict("b1", "m", true), verdict("b2", "m", true));
        let none: Vec<serde_json::Value> = Vec::new();
        assert_eq!(
            decline_in(&dir, None, &judged(std::slice::from_ref(&b1))),
            None
        );
        assert_eq!(
            decline_in(&dir, Some(&sha), &judged(&[verdict("b1", "m", false)])),
            None
        );
        assert!(!path.exists(), "nothing kept");
        assert_eq!(
            decline_in(&dir, Some(&sha), &judged(std::slice::from_ref(&b1))),
            None
        );
        let both = [b1.clone(), b2.clone()];
        assert_eq!(decline_in(&dir, Some(&sha), &judged(&both)), None);
        assert_eq!(kept_declined(&dir, &sha), both);
        let text = std::fs::read_to_string(&path).expect("the kept rejections");
        let on_disk: serde_json::Value = serde_json::from_str(&text).expect("a record");
        assert_eq!(
            on_disk,
            json!({"compile_version": COMPILE_WIRE_VERSION, "engine": env!("CARGO_PKG_VERSION"),
                "intent_sha256": sha, "declined": both})
        );
        let ignore = std::fs::read_to_string(dir.join(".gitignore")).expect("self-ignoring");
        assert_eq!(ignore, "*\n");
        // Carried into a compile of this intent, never into another's.
        let request = || CompileRequest::create("Write the greeting to ./out/result.txt.");
        assert_eq!(carry_from(&dir, Some(&sha), request()).declined, both);
        let other = "c".repeat(64);
        assert_eq!(carry_from(&dir, Some(&other), request()).declined, none);
        assert_eq!(carry_from(&dir, None, request()).declined, none);
        // A verdict this round carried is named, and kept once.
        let mut carried = b1.clone();
        carried["carried"] = json!(true);
        assert_eq!(
            decline_in(&dir, Some(&sha), &judged(&[carried])),
            Some(Declined::Carried(path.clone()))
        );
        assert_eq!(kept_declined(&dir, &sha), both);
        // Another engine's file is not read; the next rejection rewrites it.
        let mut foreign = on_disk;
        foreign["engine"] = json!("0.0.0-another");
        std::fs::write(&path, foreign.to_string()).expect("a foreign file");
        assert_eq!(kept_declined(&dir, &sha), none);
        assert_eq!(carry_from(&dir, Some(&sha), request()).declined, none);
        let b3 = verdict("b3", "m", true);
        assert_eq!(
            decline_in(&dir, Some(&sha), &judged(std::slice::from_ref(&b3))),
            None
        );
        assert_eq!(kept_declined(&dir, &sha), [b3]);
        // A file that cannot be written (a directory stands where it goes) is named.
        std::fs::remove_file(&path).expect("the kept rejections");
        std::fs::create_dir(&path).expect("a directory where they go");
        let stray = dir.join("stray");
        std::fs::write(&stray, "x").expect("a file to move");
        let error = std::fs::rename(&stray, &path)
            .expect_err("no file over a directory")
            .to_string();
        assert_eq!(
            decline_in(&dir, Some(&sha), &judged(&[verdict("b4", "m", true)])),
            Some(Declined::Failed {
                path: path.clone(),
                error
            })
        );
        assert!(path.is_dir());
    }
}

#[cfg(test)]
mod replay_tests;
