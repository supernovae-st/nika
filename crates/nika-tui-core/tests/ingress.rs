// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// A timeless step is 0.0 by a LITERAL in the fold (`if timeless { 0.0 }`),
// never by arithmetic, so strict equality is the assertion that means
// something here · an epsilon would pass on a value the fold never writes.
#![allow(clippy::float_cmp)]

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

/// A journal line, minimal but shaped like the engine's.
fn line(kind: &str, ts: u64, fields: &[(&str, serde_json::Value)]) -> String {
    serde_json::json!({
        "chain": "0", "correlation": null, "run": null,
        "id": { "uuid": "019fedd4-0000-0000-0000-000000000000" },
        "kind": kind,
        "timestamp": ts,
        "fields": fields
            .iter()
            .map(|(k, v)| serde_json::json!({ "key": k, "value": v }))
            .collect::<Vec<_>>(),
    })
    .to_string()
}

/// Gate 5, 2026-08-13. When a failure carries no structured outcome the
/// code is the FIRST WORD of `detail`, taken while the char is not a
/// space. Flipping that `!=` to `==` collects the leading spaces instead,
/// which is none of them, and the code comes out empty. No fixture
/// exercised the fallback, so the mutant rode free.
#[test]
fn a_failure_without_a_structured_outcome_takes_its_code_from_the_first_word() {
    let journal = [
        line("workflow_started", 1_000_000_000, &[]),
        line(
            "task_failed",
            1_000_000_000,
            &[
                ("task", serde_json::json!("boom")),
                (
                    "detail",
                    serde_json::json!("NIKA-TEST-001 · le message qui suit"),
                ),
                ("duration_ms", serde_json::json!(0)),
            ],
        ),
    ]
    .join("\n");
    let run = run_from_journal(&journal).expect("folds");
    let step = run.steps.iter().find(|s| s.id == "boom").expect("boom");
    let failed = step.failed.as_ref().expect("boom a échoué");
    assert_eq!(
        failed.code, "NIKA-TEST-001",
        "le code est le PREMIER MOT du detail · `==` le rendrait vide"
    );
}

/// Gate 5, 2026-08-13. The whole `task_skipped` arm could be deleted and
/// nothing noticed: the fixture journal contains no skipped step at all.
/// A skip is not a failure and not a never-born, and it stays timeless
/// because it never started, whatever duration the line declares.
#[test]
fn a_skipped_step_says_so_and_stays_timeless() {
    let journal = [
        line("workflow_started", 1_000_000_000, &[]),
        line(
            "task_skipped",
            2_000_000_000,
            &[
                ("task", serde_json::json!("saute")),
                ("duration_ms", serde_json::json!(999)),
            ],
        ),
    ]
    .join("\n");
    let run = run_from_journal(&journal).expect("folds");
    let step = run.steps.iter().find(|s| s.id == "saute").expect("saute");
    assert_eq!(step.skipped, Some(true), "un pas sauté le DIT");
    assert_eq!(step.never_born, None, "sauté n est pas jamais-né");
    assert_eq!(step.failed, None, "ni un échec");
    assert_eq!(step.start, 0.0, "intemporel · il n a jamais démarré");
    assert_eq!(step.dur, 0.0, "malgré les 999 ms que la ligne déclare");
}

/// The graph mirror reads a real inspect output — verb enum, tool/model
/// slots, permits per node, typed edges. (Captured as format 2 on a 0.116
/// binary, migrated to format 3 by the engine's own law: every node of a
/// format-2 document is a task.)
#[test]
fn the_graph_mirror_reads_inspect() {
    let bytes = std::fs::read_to_string("tests/fixtures/inspect-gated.json").expect("fixture");
    let g: GraphDoc = serde_json::from_str(&bytes).expect("parses");
    assert_eq!(g.graph_format, nika_tui_core::ingress::GRAPH_FORMAT);
    assert!(
        g.nodes.iter().all(|n| n.kind == "task"),
        "a migrated node is a task"
    );
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
