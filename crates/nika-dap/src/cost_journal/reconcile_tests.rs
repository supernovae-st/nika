// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The P4 door through its owner API: an inspection reads the journal as data
//! (saying which UNKNOWN it derived), a submission appends exactly one event or
//! refuses with every journal byte untouched, and a request is bound to the
//! inspected directory.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::reconcile::*;
use super::{JOURNAL, Taken, Writer, take, unknown_row};
use nika_fs::OwnedDir;
use serde_json::json;
use std::path::Path;

const HOST: &str = "fixture-host";

fn holder() -> Writer {
    Writer {
        pid: 7,
        host: HOST.into(),
        boot: None,
    }
}

fn account(state: &str, calls: u64, attempts: &serde_json::Value) -> serde_json::Value {
    json!({"attempts": [], "billed_nano_usd": null, "known_subtotal_nano_usd": "0",
        "limit_nano_usd": null, "overridden_defaults": [null, null], "refusal": null,
        "schema": "nika/inference-cost-observation@1", "state": state,
        "unknown_attempts": attempts, "unknown_calls": calls,
        "unknown_cost": {"provider": "deepseek", "model": "deepseek-chat",
            "endpoint": "https://127.0.0.1:18443/v1", "max_requests": 1}})
}

fn run_row(invocation: &str, phase: &str, pid: u64, observation: &serde_json::Value) -> String {
    json!({"schema": "nika/run-cost-observation@1", "invocation": invocation, "phase": phase,
        "observation": observation, "lease": {"pid": pid, "host": HOST}})
    .to_string()
}

/// A Run prepared and settled Uncertain by `pid`: its two lines.
fn uncertain(invocation: &str, pid: u64) -> [String; 2] {
    let sent = json!([{"sent": true, "estimated_nano_usd": null, "request_id": "req-9"}]);
    [
        run_row(invocation, "prepared", pid, &account("Open", 0, &json!([]))),
        run_row(invocation, "settled", pid, &account("Uncertain", 1, &sent)),
    ]
}

/// A project directory holding `.nika/` with `lines` as its journal.
fn project_with(lines: &[String]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let nika = OwnedDir::open(dir.path())
        .unwrap()
        .create_below(&[".nika"])
        .unwrap();
    for line in lines {
        nika.append_line(JOURNAL, line).unwrap();
    }
    dir
}

fn bytes(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join(".nika").join(JOURNAL)).unwrap_or_default()
}

fn sha(line: &str) -> String {
    nika_event::source_id::sha256_hex(line.as_bytes())
}

fn request(project: &str, invocation: &str, prior: &str, resolution: Resolution) -> Request {
    Request::new(
        project.into(),
        invocation.into(),
        prior.into(),
        resolution,
        Evidence::operator_attestation("invoice INV-9 line 1"),
        Principal::new(501, Some("operator".into())),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_790_570_000),
    )
}

#[test]
fn inspect_then_submit_resolves_and_keeps_the_whole_journal_as_a_prefix() {
    let [prepared, settled] = uncertain("run-a", 31);
    let dir = project_with(&[prepared, settled.clone()]);
    let seen = inspect(dir.path(), &holder(), "exe-review").unwrap();
    assert!(seen.derived.is_empty(), "nothing to derive");
    let [run] = seen.runs.as_slice() else {
        panic!("{seen:?}")
    };
    assert_eq!(run.state, RunState::Uncertain);
    assert_eq!(run.head_sha256, sha(&settled));
    assert_eq!(run.why_not, None);
    assert_eq!(run.facts["provider_request_ids"], json!(["req-9"]));
    assert_eq!(run.writer, Some(json!({"pid": 31, "host": HOST})));
    assert!(!seen.is_clear());
    assert_eq!(seen.to_value()["runs"][0]["reconcilable"], true);
    let before = bytes(dir.path());
    let submitted = request(&seen.project, "run-a", &run.head_sha256, Resolution::Billed);
    let receipt = submit(dir.path(), &holder(), &submitted).unwrap();
    let after = bytes(dir.path());
    assert!(after.starts_with(&before) && receipt.prefix_preserved);
    assert_eq!(receipt.before.0, u64::try_from(before.len()).unwrap());
    assert_eq!(receipt.after.0, u64::try_from(after.len()).unwrap());
    assert!(receipt.clear && !receipt.still_blocks);
    let appended = after[before.len()..].strip_suffix(b"\n").unwrap();
    assert_eq!(
        receipt.event_sha256,
        nika_event::source_id::sha256_hex(appended)
    );
    let claim = &receipt.event["reconciliation"];
    assert_eq!(claim["resolution"], "billed");
    assert_eq!(claim["prior_sha256"], sha(&settled));
    assert_eq!(
        claim["evidence"],
        json!({"class": "operator_attestation", "verified": false,
            "reference": "invoice INV-9 line 1"})
    );
    assert_eq!(
        claim["principal"],
        json!({"kind": "local_account", "uid": 501, "name": "operator"})
    );
    assert_eq!(claim["project"]["binding"], seen.project);
    assert_eq!(claim["observed_at"], "2026-09-28T04:33:20Z");
    assert_eq!(receipt.event["lease"], holder().json());
    assert_eq!(receipt.to_value()["journal"]["prefix_preserved"], true);
    let again = inspect(dir.path(), &holder(), "exe-later").unwrap();
    assert!(again.is_clear());
    let [done] = again.runs.as_slice() else {
        panic!("{again:?}")
    };
    assert_eq!(done.state, RunState::Reconciled(Resolution::Billed));
    assert_eq!(done.why_not, Some("it is already reconciled"));
    let phases: Vec<&str> = done.history.iter().map(|r| r.phase.as_str()).collect();
    assert_eq!(phases, ["prepared", "settled", "reconciled"]);
}

/// An inspection records a killed Run's unknown (and says so); a submission
/// never derives: before the inspection it refuses, bytes untouched.
#[test]
fn inspection_derives_explicitly_and_a_submission_never_does() {
    let prepared = run_row("run-k", "prepared", 4242, &account("Open", 0, &json!([])));
    let dir = project_with(std::slice::from_ref(&prepared));
    let nika = OwnedDir::open(dir.path())
        .unwrap()
        .open_below(&[".nika"])
        .unwrap();
    let binding = project(&nika).unwrap();
    let untouched = bytes(dir.path());
    let early = request(&binding, "run-k", &sha(&prepared), Resolution::NotBilled);
    let refused = submit(dir.path(), &holder(), &early);
    assert!(matches!(refused, Err(Refusal::InspectFirst)), "{refused:?}");
    assert_eq!(bytes(dir.path()), untouched);
    let seen = inspect(dir.path(), &holder(), "exe-review").unwrap();
    let derived_line = unknown_row(
        "run-k",
        &serde_json::from_str(&prepared).unwrap(),
        prepared.as_bytes(),
        "exe-review",
    )
    .to_string();
    assert_eq!(seen.derived, vec![("run-k".to_owned(), sha(&derived_line))]);
    assert_eq!(seen.to_value()["derived"][0]["invocation"], "run-k");
    assert_eq!(
        bytes(dir.path()),
        format!("{prepared}\n{derived_line}\n").into_bytes()
    );
    assert_eq!(seen.bytes_before, u64::try_from(untouched.len()).unwrap());
    let [run] = seen.runs.as_slice() else {
        panic!("{seen:?}")
    };
    assert_eq!(run.state, RunState::Unknown);
    assert_eq!(run.head_sha256, sha(&derived_line));
    let late = request(&binding, "run-k", &run.head_sha256, Resolution::NotBilled);
    let receipt = submit(dir.path(), &holder(), &late).unwrap();
    assert!(receipt.clear && receipt.prefix_preserved);
}

/// Every refusal leaves every journal byte as it was: another project, an
/// unknown Run, a stale head, an invalid reference, a clean Run, a busy lease,
/// no journal, and a second (double) resolution.
#[test]
fn every_refusal_leaves_the_journal_bytes_untouched() {
    let [prepared, settled] = uncertain("run-r", 41);
    let clean = [
        run_row("run-c", "prepared", 42, &account("Open", 0, &json!([]))),
        run_row("run-c", "settled", 42, &account("Closed", 0, &json!([]))),
    ];
    let lines = [
        prepared.clone(),
        settled.clone(),
        clean[0].clone(),
        clean[1].clone(),
    ];
    let dir = project_with(&lines);
    let seen = inspect(dir.path(), &holder(), "exe-review").unwrap();
    let here = seen.project.clone();
    let head = sha(&settled);
    let before = bytes(dir.path());
    let mut invalid = request(&here, "run-r", &head, Resolution::Billed);
    invalid.evidence = Evidence::operator_attestation("");
    let cases = [
        (
            request(&"0".repeat(64), "run-r", &head, Resolution::Billed),
            "other_project",
        ),
        (
            request(&here, "run-none", &head, Resolution::Billed),
            "unknown_run",
        ),
        (
            request(&here, "run-r", &sha(&prepared), Resolution::Billed),
            "stale",
        ),
        (invalid, "invalid"),
        (
            request(&here, "run-c", &sha(&clean[1]), Resolution::Billed),
            "not_reconcilable",
        ),
    ];
    for (req, kind) in &cases {
        let refused = submit(dir.path(), &holder(), req).unwrap_err();
        assert_eq!(refused.kind(), *kind, "{refused}");
        assert!(!refused.is_environment(), "{refused}");
        assert_eq!(bytes(dir.path()), before, "{kind}: bytes untouched");
    }
    let stale = submit(dir.path(), &holder(), &cases[2].0).unwrap_err();
    assert_eq!(stale.to_value()["head_sha256"], head);
    // A live holder of the lease (a Run in flight, a review waiting).
    let nika = OwnedDir::open(dir.path())
        .unwrap()
        .open_below(&[".nika"])
        .unwrap();
    let other = Writer {
        pid: 99,
        host: HOST.into(),
        boot: None,
    };
    let Taken::Held(lease) = take(&nika, &other).unwrap() else {
        panic!("a free lease is taken");
    };
    let fresh = request(&here, "run-r", &head, Resolution::Billed);
    let busy = submit(dir.path(), &holder(), &fresh).unwrap_err();
    assert!(matches!(busy, Refusal::Busy { pid: Some(99) }), "{busy:?}");
    assert!(busy.is_environment());
    let blocked = inspect(dir.path(), &holder(), "exe-review");
    assert!(matches!(blocked, Err(Refusal::Busy { .. })));
    assert_eq!(bytes(dir.path()), before, "busy: bytes untouched");
    drop(lease);
    // No journal at all: nothing is created.
    let empty = tempfile::tempdir().unwrap();
    let none = submit(empty.path(), &holder(), &fresh).unwrap_err();
    assert_eq!(none.kind(), "no_journal");
    assert!(
        !empty.path().join(".nika").exists(),
        "a refusal creates nothing"
    );
    // The first resolution lands; the second (double) resolution is refused.
    let first = request(&here, "run-r", &head, Resolution::NotBilled);
    submit(dir.path(), &holder(), &first).unwrap();
    let resolved = bytes(dir.path());
    let double = submit(dir.path(), &holder(), &fresh).unwrap_err();
    assert!(
        matches!(double, Refusal::NotReconcilable("it is already reconciled")),
        "{double:?}"
    );
    assert_eq!(bytes(dir.path()), resolved, "double: bytes untouched");
}

/// The binding follows the held `.nika` directory: a rename on the same
/// filesystem keeps it; a separate copy and a replaced `.nika` refuse, with
/// their journal bytes untouched.
#[test]
fn the_binding_follows_the_held_directory() {
    let parent = tempfile::tempdir().unwrap();
    let [prepared, settled] = uncertain("run-b", 43);
    let lines = format!("{prepared}\n{settled}\n");
    let make = |name: &str| {
        let root = parent.path().join(name);
        std::fs::create_dir_all(root.join(".nika")).unwrap();
        std::fs::write(root.join(".nika").join(JOURNAL), &lines).unwrap();
        root
    };
    let head = sha(&settled);
    let first = make("first");
    let seen = inspect(&first, &holder(), "exe-review").unwrap();
    // A separate copy of `.nika` with the same bytes: another directory.
    let copy = make("copy");
    let copied = submit(
        &copy,
        &holder(),
        &request(&seen.project, "run-b", &head, Resolution::Billed),
    );
    assert_eq!(copied.unwrap_err().kind(), "other_project");
    assert_eq!(bytes(&copy), lines.as_bytes());
    // `.nika` replaced between the inspection and the submission.
    let replaced = make("replaced");
    let there = inspect(&replaced, &holder(), "exe-review").unwrap();
    std::fs::rename(replaced.join(".nika"), replaced.join("old-nika")).unwrap();
    std::fs::create_dir(replaced.join(".nika")).unwrap();
    std::fs::write(replaced.join(".nika").join(JOURNAL), &lines).unwrap();
    let swapped = request(&there.project, "run-b", &head, Resolution::Billed);
    let refused = submit(&replaced, &holder(), &swapped).unwrap_err();
    assert_eq!(refused.kind(), "other_project");
    assert_eq!(bytes(&replaced), lines.as_bytes());
    // The project renamed on the same filesystem: the same held directory.
    let moved = parent.path().join("moved");
    std::fs::rename(&first, &moved).unwrap();
    let kept = request(&seen.project, "run-b", &head, Resolution::Billed);
    let receipt = submit(&moved, &holder(), &kept).unwrap();
    assert!(receipt.clear && receipt.prefix_preserved);
}

/// `still_unknown` through the door: it keeps blocking and becomes the Run's
/// latest row, so the old head is stale and the new one resolves it.
#[test]
fn still_unknown_through_the_door_moves_the_head() {
    let [prepared, settled] = uncertain("run-s", 44);
    let dir = project_with(&[prepared, settled.clone()]);
    let seen = inspect(dir.path(), &holder(), "exe-review").unwrap();
    let hold = request(
        &seen.project,
        "run-s",
        &sha(&settled),
        Resolution::StillUnknown,
    );
    let held_receipt = submit(dir.path(), &holder(), &hold).unwrap();
    assert!(held_receipt.still_blocks && !held_receipt.clear);
    let now = inspect(dir.path(), &holder(), "exe-review").unwrap();
    let [run] = now.runs.as_slice() else {
        panic!("{now:?}")
    };
    assert_eq!(run.state, RunState::StillUnknown);
    assert_eq!(run.head_sha256, held_receipt.event_sha256);
    let old = request(&now.project, "run-s", &sha(&settled), Resolution::Billed);
    let stale = submit(dir.path(), &holder(), &old).unwrap_err();
    assert!(matches!(stale, Refusal::Stale { ref head } if *head == held_receipt.event_sha256));
    let named = request(
        &now.project,
        "run-s",
        &held_receipt.event_sha256,
        Resolution::Billed,
    );
    assert!(submit(dir.path(), &holder(), &named).unwrap().clear);
}

/// Old, unknowable evidence is represented, never migrated: a Run whose writer
/// this host cannot judge is shown and refused, a refused row and a torn row
/// are listed and keep blocking; the door clears none of them.
#[test]
fn unjudged_refused_and_torn_rows_are_shown_and_never_cleared() {
    let far = json!({"schema": "nika/run-cost-observation@1", "invocation": "run-far",
        "phase": "prepared", "observation": account("Open", 0, &json!([])),
        "lease": {"pid": 5, "host": "far-host"}})
    .to_string();
    let legacy = json!({"schema": "nika/run-cost-observation@1", "invocation": "run-old",
        "phase": "prepared", "observation": account("Open", 0, &json!([]))})
    .to_string();
    let orphan = run_row(
        "run-orphan",
        "settled",
        6,
        &account("Closed", 0, &json!([])),
    );
    let dir = project_with(&[legacy, far.clone(), orphan.clone()]);
    let path = dir.path().join(".nika").join(JOURNAL);
    let mut journal = std::fs::read(&path).unwrap();
    journal.extend_from_slice(b"{\"cut");
    std::fs::write(&path, &journal).unwrap();
    let seen = inspect(dir.path(), &holder(), "exe-review").unwrap();
    let states: Vec<(&str, RunState)> = seen
        .runs
        .iter()
        .map(|r| (r.invocation.as_str(), r.state))
        .collect();
    assert_eq!(
        states,
        [
            ("run-far", RunState::Unjudged),
            ("run-old", RunState::Unjudged)
        ]
    );
    assert!(seen.runs.iter().all(|r| {
        r.why_not
            .is_some_and(|why| why.contains("cannot be judged"))
    }));
    assert_eq!(seen.conflicts.len(), 1);
    assert_eq!(
        seen.torn,
        vec![nika_event::source_id::sha256_hex(b"{\"cut")]
    );
    assert_eq!(seen.to_value()["conflicts"][0]["reconcilable"], false);
    let before = bytes(dir.path());
    let on_far = request(&seen.project, "run-far", &sha(&far), Resolution::NotBilled);
    let refused = submit(dir.path(), &holder(), &on_far).unwrap_err();
    assert_eq!(refused.kind(), "not_reconcilable");
    let on_orphan = request(
        &seen.project,
        "run-orphan",
        &sha(&orphan),
        Resolution::NotBilled,
    );
    let refused = submit(dir.path(), &holder(), &on_orphan).unwrap_err();
    assert_eq!(refused.kind(), "unknown_run");
    assert_eq!(bytes(dir.path()), before);
}
