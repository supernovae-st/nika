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
