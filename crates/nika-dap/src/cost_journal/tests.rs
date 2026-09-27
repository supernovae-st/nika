// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The journal fold: settled rows clear, an uncertain settlement blocks, a
//! leased row whose writer is gone becomes ONE durable unknown, a row no lease
//! covers is never judged, and a row cut mid-write is named by its digest.
//! Rows are appended, never rewritten. The lease: held refuses, released is
//! taken again. (Moved with the code from nika-cli-host's `run_cost`.)
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
use super::*;
use serde_json::json;

const HOST: &str = "fixture-host";

fn observation(state: &str, unknown_calls: u64) -> serde_json::Value {
    json!({"attempts": [], "billed_nano_usd": null, "known_subtotal_nano_usd": "0",
        "limit_nano_usd": null, "overridden_defaults": [null, null], "refusal": null,
        "schema": "nika/inference-cost-observation@1", "state": state,
        "unknown_attempts": [], "unknown_calls": unknown_calls, "unknown_cost": null})
}

fn row(invocation: &str, phase: &str, state: &str, lease: Option<(u64, &str)>) -> String {
    let mut row = json!({"schema": "nika/run-cost-observation@1", "invocation": invocation,
        "phase": phase, "observation": observation(state, 0)});
    if let Some((pid, host)) = lease {
        row["lease"] = json!({"pid": pid, "host": host});
    }
    row.to_string()
}

fn project(lines: &[String]) -> (tempfile::TempDir, OwnedDir) {
    let root = tempfile::tempdir().unwrap();
    let nika = OwnedDir::open(root.path())
        .unwrap()
        .create_below(&[".nika"])
        .unwrap();
    for line in lines {
        nika.append_line(JOURNAL, line).unwrap();
    }
    (root, nika)
}

fn journal(root: &tempfile::TempDir) -> String {
    std::fs::read_to_string(root.path().join(".nika").join(JOURNAL)).unwrap_or_default()
}

/// The Runs a fold names, on a journal no writer cut mid-write.
fn runs(nika: &OwnedDir, host: &str, observer: &str) -> Vec<Blocker> {
    let exposures = fold(nika, host, observer).unwrap();
    assert!(exposures.torn.is_empty(), "{exposures:?}");
    exposures.runs
}

#[test]
fn an_absent_journal_blocks_nothing_and_writes_nothing() {
    let (root, nika) = project(&[]);
    assert!(fold(&nika, HOST, "observer").unwrap().is_clear());
    assert!(!root.path().join(".nika").join(JOURNAL).exists());
}

#[test]
fn settled_rows_clear_and_an_uncertain_settlement_blocks_without_a_new_row() {
    let lines = [
        row("run-a", "prepared", "Open", Some((7, HOST))),
        row("run-a", "settled", "Closed", Some((7, HOST))),
        row("run-b", "prepared", "Open", Some((8, HOST))),
        row("run-b", "settled", "Uncertain", Some((8, HOST))),
    ];
    let (root, nika) = project(&lines);
    let before = journal(&root);
    assert_eq!(
        runs(&nika, HOST, "observer"),
        vec![Blocker {
            invocation: "run-b".into(),
            exposure: Exposure::Uncertain
        }]
    );
    assert_eq!(journal(&root), before, "a settled Run is never re-derived");
}

/// P3 · the restart derives the killed Run's UNKNOWN once, from the exact
/// `prepared` row its leased writer left, and appends it; a second fold
/// reads the recorded UNKNOWN and appends nothing.
#[test]
fn a_leased_prepared_row_becomes_one_durable_unknown_and_stays_unknown() {
    let prepared = row("run-killed", "prepared", "Open", Some((4242, HOST)));
    let (root, nika) = project(std::slice::from_ref(&prepared));
    let before = journal(&root);
    let unknown = Blocker {
        invocation: "run-killed".into(),
        exposure: Exposure::Unknown { pid: Some(4242) },
    };
    assert_eq!(runs(&nika, HOST, "run-next"), vec![unknown.clone()]);
    let after = journal(&root);
    assert!(
        after.starts_with(&before),
        "the rows read stay byte-identical"
    );
    let lines: Vec<&str> = after.lines().collect();
    assert_eq!(lines.len(), 2, "exactly one derived row");
    let derived: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(
        derived["invocation"], "run-killed",
        "the same Run's identity"
    );
    assert_eq!(derived["phase"], "unknown");
    let original: serde_json::Value = serde_json::from_str(&prepared).unwrap();
    assert_eq!(
        derived["observation"], original["observation"],
        "the account's last words, verbatim"
    );
    assert_eq!(derived["unsettled"]["cause"], "interrupted");
    assert_eq!(
        derived["unsettled"]["writer"],
        json!({"pid": 4242, "host": HOST})
    );
    assert_eq!(
        derived["unsettled"]["prior_sha256"],
        nika_event::source_id::sha256_hex(prepared.as_bytes())
    );
    assert_eq!(derived["unsettled"]["observed_by"], "run-next");
    assert_eq!(
        runs(&nika, HOST, "run-later"),
        vec![unknown],
        "the recorded unknown still blocks: no automatic retry"
    );
    assert_eq!(journal(&root), after, "an unknown is recorded once");
}

#[test]
fn rows_without_a_lease_or_from_another_host_are_never_judged() {
    let lines = [
        row("run-legacy", "prepared", "Open", None),
        row(
            "run-elsewhere",
            "prepared",
            "Open",
            Some((9, "another-host")),
        ),
    ];
    let (root, nika) = project(&lines);
    let before = journal(&root);
    let unjudged = |id: &str| Blocker {
        invocation: id.into(),
        exposure: Exposure::Unjudged,
    };
    assert_eq!(
        runs(&nika, HOST, "observer"),
        vec![unjudged("run-elsewhere"), unjudged("run-legacy")]
    );
    // A host that cannot name itself judges nothing either.
    let (root_nameless, nika_nameless) =
        project(&[row("run-x", "prepared", "Open", Some((1, "")))]);
    assert_eq!(
        runs(&nika_nameless, "", "observer"),
        vec![unjudged("run-x")]
    );
    assert_eq!(journal(&root), before);
    assert_eq!(journal(&root_nameless).lines().count(), 1);
}

#[test]
fn an_unrecognized_row_fails_closed() {
    let foreign_phase = row("run-a", "reconciled", "Closed", None);
    let bare_unknown = row("run-a", "unknown", "Open", None);
    let foreign_schema = r#"{"schema":"other@1","invocation":"run-a"}"#.to_owned();
    for text in [foreign_phase, bare_unknown, foreign_schema] {
        let (_root, nika) = project(std::slice::from_ref(&text));
        assert!(fold(&nika, HOST, "observer").is_err(), "{text}");
    }
}

/// E · C2 · a writer killed between a row and its newline leaves a torn tail.
/// The next append first ends that line, so no row fuses with it; the fold
/// names the cut bytes by their digest and keeps blocking — never a parse
/// error that bricks every later review, never a silent skip.
#[test]
fn a_row_cut_mid_write_is_named_by_digest_and_never_fuses() {
    let prepared = row("run-cut", "prepared", "Open", Some((5, HOST)));
    let cut = &prepared[..prepared.len() / 2];
    let (root, nika) = project(&[]);
    std::fs::write(root.path().join(".nika").join(JOURNAL), cut).unwrap();
    let settled = row("run-next", "settled", "Closed", Some((6, HOST)));
    append_row(&nika, &settled).unwrap();
    let text = journal(&root);
    assert_eq!(
        text,
        format!("{cut}\n{settled}\n"),
        "the cut bytes stay, alone"
    );
    let exposures = fold(&nika, HOST, "observer").unwrap();
    assert_eq!(
        exposures.torn,
        vec![nika_event::source_id::sha256_hex(cut.as_bytes())]
    );
    assert!(exposures.runs.is_empty(), "run-next settled cleanly");
    assert!(!exposures.is_clear(), "the cut row still blocks");
    assert!(refusal(&exposures).contains("a row was cut mid-write (sha256 "));
    // A row whose newline alone was lost is whole: it is read, then ended.
    let (root_whole, nika_whole) = project(&[]);
    std::fs::write(root_whole.path().join(".nika").join(JOURNAL), &prepared).unwrap();
    assert_eq!(
        runs(&nika_whole, HOST, "observer"),
        vec![Blocker {
            invocation: "run-cut".into(),
            exposure: Exposure::Unknown { pid: Some(5) },
        }]
    );
    let lines: Vec<String> = journal(&root_whole).lines().map(str::to_owned).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], prepared);
}

#[test]
fn the_refusal_names_each_run_and_its_evidence() {
    let text = refusal(&Exposures {
        runs: vec![
            Blocker {
                invocation: "run-a".into(),
                exposure: Exposure::Unknown { pid: Some(12) },
            },
            Blocker {
                invocation: "run-b".into(),
                exposure: Exposure::Uncertain,
            },
            Blocker {
                invocation: "run-c".into(),
                exposure: Exposure::Unjudged,
            },
        ],
        torn: Vec::new(),
    });
    assert!(text.contains("Run run-a ended without a settlement and its process 12 is gone"));
    assert!(text.contains("Run run-b settled with a sent request whose charge is unknown"));
    assert!(text.contains("Run run-c was admitted and never settled"));
    assert!(text.contains("no automatic retry"));
    assert!(text.contains(".nika/inference-cost-observations.ndjson"));
}

/// A held lease refuses a second holder and names it; a dropped lease is taken
/// again (the kernel's release is the whole liveness proof).
#[test]
fn a_held_lease_is_busy_and_a_released_one_is_taken_again() {
    let (root, nika) = project(&[]);
    let writer = Writer::this_process();
    let Taken::Held(lease) = take(&nika, &writer).unwrap() else {
        panic!("a free lease is taken");
    };
    match take(&nika, &writer).unwrap() {
        Taken::Busy { pid } => assert_eq!(pid, Some(u64::from(writer.pid))),
        Taken::Held(_) => panic!("a held lease is never shared"),
    }
    drop(lease);
    assert!(matches!(take(&nika, &writer).unwrap(), Taken::Held(_)));
    let record = std::fs::read_to_string(root.path().join(".nika").join(LEASE)).unwrap();
    let record: serde_json::Value = serde_json::from_str(&record).unwrap();
    assert_eq!(record, writer.json());
}
