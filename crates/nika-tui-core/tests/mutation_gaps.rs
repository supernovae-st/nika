// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable
)]
#![allow(clippy::float_cmp)] // the parity proof compares f64 BITS — strict equality is its whole point

//! The mutation-gap proofs — each test here exists because a mutant
//! survived the first sweep (155 tested · 35 missed). Every one names
//! the mutant class it kills.

use nika_tui_core::derive;
use nika_tui_core::ingress::GraphDoc;
use nika_tui_core::model::{Group, Origin, Run, Step, Task, Touch, Verb, Workflow};

fn load(name: &str) -> (Workflow, Run, serde_json::Value) {
    let raw = std::fs::read_to_string(format!("tests/fixtures/{name}.json")).expect("fixture");
    let v: serde_json::Value = serde_json::from_str(&raw).expect("parses");
    (
        serde_json::from_value(v["workflow"].clone()).expect("workflow"),
        serde_json::from_value(v["run"].clone()).expect("run"),
        v["derived"].clone(),
    )
}

/// `group_span` — every operator pinned (the stress bench's twelve-locale
/// fan-out: min · max · total cost · the SLOWEST by strict `>`).
#[test]
fn the_group_span_reports_its_members() {
    let (_wf, run, _d) = load("stress");
    let members: Vec<Task> = (0..12)
        .map(|i| Task {
            id: format!(
                "traduire-{}",
                [
                    "fr", "en", "de", "es", "it", "pt", "nl", "pl", "ja", "ko", "zh", "ar"
                ][i]
            ),
            verb: Verb::Infer,
            glyph: "◇".to_owned(),
            needs: Vec::new(),
            tool: None,
            origin: None,
            family: None,
            touches: None,
        })
        .collect();
    let span = derive::group_span(&members, &run).expect("twelve recorded members");
    assert_eq!(span.n, 12);
    // the bench's own numbers: the quickest locale and the slowest
    let durs: Vec<f64> = run
        .steps
        .iter()
        .filter(|s| s.id.starts_with("traduire-"))
        .map(|s| s.dur)
        .collect();
    let want_min = durs.iter().copied().fold(f64::INFINITY, f64::min);
    let want_max = durs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert_eq!(span.min, want_min);
    assert_eq!(span.max, want_max);
    let slowest = run
        .steps
        .iter()
        .filter(|s| s.id.starts_with("traduire-"))
        .reduce(|a, s| if s.dur > a.dur { s } else { a })
        .expect("a slowest");
    assert_eq!(
        span.slowest, slowest.id,
        "strict > keeps the FIRST slowest of ties"
    );
    let want_cost: f64 = run
        .steps
        .iter()
        .filter(|s| s.id.starts_with("traduire-"))
        .map(|s| s.cost.unwrap_or(0.0))
        .sum();
    assert_eq!(span.cost, want_cost);
}

/// `group_span` on zero recorded members is None, not a zero-row lie.
#[test]
fn a_span_without_steps_is_absent() {
    let (_wf, _run, _d) = load("stress");
    let members = vec![Task {
        id: "fantome".to_owned(),
        verb: Verb::Infer,
        glyph: "◇".to_owned(),
        needs: Vec::new(),
        tool: None,
        origin: None,
        family: None,
        touches: None,
    }];
    let run = Run {
        trace: "t".to_owned(),
        when: "recorded".to_owned(),
        output: String::new(),
        steps: Vec::new(),
    };
    assert_eq!(derive::group_span(&members, &run), None);
}

/// `tasks_touching` returns the touching steps, in declared order.
#[test]
fn tasks_touching_filters_by_class() {
    let (wf, _run, _d) = load("demo-ok");
    let writers: Vec<&str> = derive::tasks_touching(&wf, Touch::FsWrite)
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(writers, vec!["ecris"]);
    let readers: Vec<&str> = derive::tasks_touching(&wf, Touch::FsRead)
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(readers, vec!["lire"]);
    let execs: Vec<&str> = derive::tasks_touching(&wf, Touch::Exec)
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(execs, vec!["fetch"], "the gh call is the exec touch");
    assert!(
        derive::tasks_touching(&wf, Touch::Net).is_empty(),
        "the demo declares no net touch"
    );
}

/// `foreign_tasks` — the mcp/registry origins, and only those.
#[test]
fn foreign_tasks_are_the_ones_you_did_not_write() {
    let (wf, _run, _d) = load("demo-ok");
    let foreign: Vec<&str> = derive::foreign_tasks(&wf)
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(
        foreign,
        vec!["verifie"],
        "the mcp validator is the one foreign step"
    );
    assert!(wf.tasks.iter().any(|t| t.origin == Some(Origin::Model)));
}

/// `is_fanout` — both variants, named.
#[test]
fn is_fanout_tells_single_from_group() {
    let (wf, _run, _d) = load("stress");
    let groups = derive::groups_of(&wf);
    let fanouts = groups.iter().filter(|g| g.is_fanout()).count();
    assert_eq!(fanouts, 1, "the twelve locales fold into ONE group");
    let singles = groups.iter().filter(|g| !g.is_fanout()).count();
    assert!(singles > 0);
    match groups.iter().find(|g| g.is_fanout()).expect("a fanout") {
        Group::Fanout { name, members } => {
            assert_eq!(name, "traduire");
            assert_eq!(members.len(), 12);
        }
        Group::Single(_) => unreachable!("filtered above"),
    }
}

/// The journal fold pins the START of every step, exactly — the
/// arithmetic (completed − duration, relative to the run's first event)
/// is what a mutation tries to flip.
#[test]
fn the_fold_pins_relative_starts_exactly() {
    let bytes = std::fs::read_to_string("tests/fixtures/journal-casse.ndjson").expect("fixture");
    let run = nika_tui_core::ingress::run_from_journal(&bytes).expect("folds");
    let by_id = |id: &str| run.steps.iter().find(|s| s.id == id).expect(id);
    // recomputed from the journal's ns timestamps, by hand
    assert!((by_id("accorde").start - 0.0).abs() < 1e-9);
    assert!((by_id("fetch").start - 0.001).abs() < 1e-9);
    assert!((by_id("lire").start - 0.009).abs() < 1e-9);
    assert!((by_id("compte").start - 0.01).abs() < 1e-9);
    assert!((by_id("resume").start - 0.422).abs() < 1e-9);
}

/// `total_cost` on PRICED steps — the demo runs are all unpriced, so a
/// `-> 0.0` mutant survived the fixtures. This one costs real money.
#[test]
fn total_cost_sums_the_priced_steps() {
    let run = Run {
        trace: "t".to_owned(),
        when: "recorded".to_owned(),
        output: String::new(),
        steps: vec![
            Step {
                id: "a".to_owned(),
                start: 0.0,
                dur: 1.0,
                cost: Some(0.004),
                tokens: None,
                failed: None,
                never_born: None,
                blocked_by: None,
            },
            Step {
                id: "b".to_owned(),
                start: 1.0,
                dur: 1.0,
                cost: Some(0.02),
                tokens: None,
                failed: None,
                never_born: None,
                blocked_by: None,
            },
            Step {
                id: "c".to_owned(),
                start: 2.0,
                dur: 1.0,
                cost: None, // the unpriced one contributes nothing
                tokens: None,
                failed: None,
                never_born: None,
                blocked_by: None,
            },
        ],
    };
    assert_eq!(derive::total_cost(&run), 0.024);
}

/// `undeclared` with a REAL gap — a workflow whose steps touch what the
/// file never declared (the demo declares everything, so an empty-vector
/// mutant survived).
#[test]
fn undeclared_names_the_missing_permit() {
    let wf = Workflow {
        file: "gap.nika.yaml".to_owned(),
        engine: "test".to_owned(),
        prompt: String::new(),
        permits: vec!["exec: [\"gh\"]".to_owned()],
        missing: String::new(),
        tasks: vec![Task {
            id: "lit".to_owned(),
            verb: Verb::Invoke,
            glyph: "◆".to_owned(),
            needs: Vec::new(),
            tool: Some("nika:read".to_owned()),
            origin: Some(Origin::Builtin),
            family: None,
            touches: Some(vec![Touch::FsRead, Touch::Tools]),
        }],
    };
    let gaps = derive::undeclared(&wf);
    assert!(
        gaps.contains(&Touch::FsRead),
        "fs.read is touched, never declared"
    );
    assert!(gaps.contains(&Touch::Tools));
    assert!(!gaps.contains(&Touch::Exec), "exec IS declared");
}

/// The wasm doors, called natively — a door that answered empty would
/// pass none of these (the Default/empty-string mutants).
#[test]
fn the_wasm_doors_answer_with_content() {
    let (wf, run, derived) = load("demo-ok");
    let out = nika_tui_core::wasm::derive_run(
        &serde_json::to_string(&wf).expect("wf"),
        &serde_json::to_string(&run).expect("run"),
    );
    let got: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(
        got["total_time"].as_f64().expect("total_time"),
        derived["total_time"].as_f64().expect("want"),
        "the door returns the fixture's own number"
    );
    assert_eq!(got["bottleneck"]["id"], derived["bottleneck"]["id"]);

    let journal = std::fs::read_to_string("tests/fixtures/journal-casse.ndjson").expect("journal");
    let folded = nika_tui_core::wasm::fold_journal(&journal);
    let run2: serde_json::Value = serde_json::from_str(&folded).expect("json");
    assert_eq!(run2["steps"].as_array().expect("steps").len(), 7);

    let graph = std::fs::read_to_string("tests/fixtures/inspect-gated.json").expect("graph");
    let b1 = nika_tui_core::wasm::seat_first(&graph);
    let b1v: serde_json::Value = serde_json::from_str(&b1).expect("json");
    assert_eq!(b1v["rev"].as_u64(), Some(1));
    let g: GraphDoc = serde_json::from_str(&graph).expect("graphdoc");
    let smaller = GraphDoc {
        nodes: g.nodes[1..].to_vec(),
        ..g
    };
    let b2 = nika_tui_core::wasm::seat_next(&b1, &serde_json::to_string(&smaller).expect("g2"));
    let b2v: serde_json::Value = serde_json::from_str(&b2).expect("json");
    assert_eq!(
        b2v["marks"][0].as_str(),
        Some("−"),
        "the hole keeps its glyph"
    );

    // and a refusal is a NAMED json value, never a panic
    let out = nika_tui_core::wasm::derive_run("{not json", "{}");
    let v: serde_json::Value = serde_json::from_str(&out).expect("even the refusal is json");
    assert!(
        v["error"]
            .as_str()
            .is_some_and(|m| m.contains("derive_run")),
        "the refusal names its door: {out}"
    );
}
