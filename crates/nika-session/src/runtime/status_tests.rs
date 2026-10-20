// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `/status` names the saved workflow first (`SessionRuntime::workflow_status`).

use std::path::PathBuf;

use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::reasoner::NoReasoner;
use crate::runtime::SessionRuntime;

/// `/status` answers first what a person asks of a saved workflow: is a schedule declared,
/// is it running, how did its last run end. Before any workflow, nothing is claimed.
#[test]
fn the_status_names_the_saved_workflow_its_schedule_and_its_last_run() {
    let root = tempfile::tempdir().expect("root");
    let none = ResolvedSessionIntelligence::new(
        IntelligenceKind::None,
        None,
        DataLocus::None,
        false,
        None,
    );
    let mut s = SessionRuntime::open(root.path(), none, Box::new(NoReasoner));
    assert!(!s.status().contains("workflow:"), "{}", s.status());
    s.last_workflow = Some(PathBuf::from("recap-hebdo.nika"));
    s.last_run = Some((0, "run observed · exit 0".to_owned()));
    let status = s.status();
    assert!(
        status.starts_with(
            "session\n  workflow: `recap-hebdo.nika`\n    schedule: no schedule declared"
        ),
        "{status}"
    );
    assert!(status.contains("\n    last run: "), "{status}");
    assert!(status.contains("\n  root: "), "{status}");
}

/// Another workflow's declared schedule stays in `/status` when the last one saved has none
/// (live gauntlet r08, 2026-10-16: after a cleanup workflow ran, « no schedule declared » hid
/// the journal's evening schedule).
#[test]
fn the_status_keeps_the_schedules_other_workflows_declared() {
    let root = tempfile::tempdir().expect("root");
    std::fs::write(
        root.path().join("nika.yaml"),
        "nika: journal\narm:\n  - workflow: journal.nika\n    cadence: \"TZ=Indian/Mauritius 0 20 * * *\"\n    plafond: 0.10\n    manqué: rattraper-une-fois\n",
    )
    .expect("project");
    let none = ResolvedSessionIntelligence::new(
        IntelligenceKind::None,
        None,
        DataLocus::None,
        false,
        None,
    );
    let mut s = SessionRuntime::open(root.path(), none, Box::new(NoReasoner));
    s.last_workflow = Some(PathBuf::from("nettoyage.nika"));
    let status = s.status();
    assert!(
        status.contains("schedule: no schedule declared")
            && status
                .contains("nika.yaml declares `journal.nika` (TZ=Indian/Mauritius 0 20 * * *)"),
        "{status}"
    );
}

/// A run the person's « no » stopped at its gate exits 0 as a success does: `/status` says the
/// refusal its trace records, never « succeeded » (live gauntlet wave 8, 2026-10-19, r02 and r04:
/// « last run: succeeded » beside « arrêté à votre « non » »).
#[test]
fn the_status_says_a_run_the_person_refused_stopped_at_their_no() {
    let root = tempfile::tempdir().expect("root");
    let traces = root.path().join(".nika/traces");
    std::fs::create_dir_all(&traces).expect("traces");
    let frame = |kind: &str, fields: serde_json::Value| {
        let fields: Vec<serde_json::Value> = (fields.as_object().into_iter().flatten())
            .map(|(key, value)| serde_json::json!({"key": key, "value": value}))
            .collect();
        serde_json::json!({
            "correlation": null,
            "execution": {"uuid": "01a1454f-aaab-7077-bc12-87282886951d"},
            "fields": fields,
            "id": {"uuid": "01a0ef11-03a7-74fb-bba0-000000000002"},
            "kind": kind,
            "run": null,
            "timestamp": 1
        })
        .to_string()
    };
    let journal = [
        frame(
            "workflow_started",
            serde_json::json!({"workflow": "digest"}),
        ),
        frame(
            "approval_decided",
            serde_json::json!({"task": "approve", "decision": "deny", "source": "human"}),
        ),
        frame(
            "workflow_completed",
            serde_json::json!({"status": "succeeded"}),
        ),
    ];
    let trace = traces.join("2026-10-19T19-21-19Z-951d.ndjson");
    std::fs::write(&trace, journal.join("\n")).expect("journal");
    let none = ResolvedSessionIntelligence::new(
        IntelligenceKind::None,
        None,
        DataLocus::None,
        false,
        None,
    );
    let mut s = SessionRuntime::open(root.path(), none, Box::new(NoReasoner));
    s.last_workflow = Some(PathBuf::from("digest.nika"));
    s.last_run = Some((0, "run observed · exit 0".to_owned()));
    s.last_trace = Some(PathBuf::from(
        ".nika/traces/2026-10-19T19-21-19Z-951d.ndjson",
    ));
    let status = s.status();
    assert!(
        status.contains("\n    last run: stopped at your « no » at its gate\n"),
        "{status}"
    );
    s.last_trace = None;
    assert!(
        s.status().contains("\n    last run: succeeded\n"),
        "{}",
        s.status()
    );
}
