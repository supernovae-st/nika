// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The ingress tests — the journal fold pinned against a REAL recorded
//! trace (the demo's broken lane, mock content), and the graph mirror
//! against a real `nika inspect --format json` output.

use nika_tui_core::ingress::{GraphDoc, run_from_journal};

/// The fold, pinned event class by event class on the casse journal.
#[test]
fn the_journal_folds_into_the_run() {
    let bytes = std::fs::read_to_string("tests/fixtures/journal-casse.ndjson").expect("fixture");
    let run = run_from_journal(&bytes).expect("folds");

    assert_eq!(run.steps.len(), 7, "every terminal event is a step");

    let by_id = |id: &str| run.steps.iter().find(|s| s.id == id).expect(id);

    // the gate + the two wave-1 producers completed
    assert_eq!(by_id("accorde").failed, None);
    assert!((by_id("fetch").dur - 0.008).abs() < 1e-9, "dur in seconds");

    // the failure carries its real code
    let lire = by_id("lire");
    let failed = lire.failed.as_ref().expect("lire failed");
    assert_eq!(failed.code, "NIKA-BUILTIN-READ-001");

    // the never-borns carry the CULPABLE, not needs[0]
    let resume = by_id("resume");
    assert_eq!(resume.never_born, Some(true));
    assert_eq!(resume.blocked_by.as_deref(), Some("lire"));
    let ecris = by_id("ecris");
    assert_eq!(ecris.blocked_by.as_deref(), Some("resume"));

    // starts are relative to the run's first event — compte starts after
    // the failure (0.01 s), never at epoch scale.
    let compte = by_id("compte");
    assert!(
        compte.start < 1.0,
        "relative seconds, not epoch: {}",
        compte.start
    );
    assert!((compte.dur - 0.412).abs() < 1e-9);

    // the trace id is the run's uuid
    assert!(!run.trace.is_empty(), "the fold names the run");
}

/// A torn line refuses the whole fold — half a journal is no journal.
#[test]
fn a_torn_journal_refuses_with_its_line() {
    let bytes = "{\"kind\":\"workflow_started\"}\n{not json";
    let err = run_from_journal(bytes).expect_err("refuses");
    assert_eq!(
        err.to_string(),
        "line 2: not JSON — not a journal this engine wrote"
    );
}

/// The graph mirror reads a real inspect output — verb enum, tool/model
/// slots, permits per node, typed edges.
#[test]
fn the_graph_mirror_reads_inspect() {
    let bytes = std::fs::read_to_string("tests/fixtures/inspect-gated.json").expect("fixture");
    let g: GraphDoc = serde_json::from_str(&bytes).expect("parses");
    assert_eq!(g.graph_format, 2);
    assert_eq!(g.workflow, "triage-tickets");
    assert!(!g.nodes.is_empty());
    // the gate node is an invoke on nika:prompt, and says so
    let gate = g
        .nodes
        .iter()
        .find(|n| n.tool.as_deref() == Some("nika:prompt"))
        .expect("the gate node");
    assert_eq!(gate.verb, nika_tui_core::model::Verb::Invoke);
    assert!(gate.permits.iter().any(|p| p == "tool: nika:prompt"));
}
