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
        _ => unreachable!("filtered above"),
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
    assert_eq!(
        by_id("resume").start,
        0.0,
        "a never-born has no start · the cancellation stamps at teardown"
    );
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
                skipped: None,
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
                skipped: None,
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
                skipped: None,
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
    let b1 = nika_tui_core::wasm::board_first(&graph);
    let b1v: serde_json::Value = serde_json::from_str(&b1).expect("json");
    assert_eq!(b1v["rev"].as_u64(), Some(1));
    let g: GraphDoc = serde_json::from_str(&graph).expect("graphdoc");
    let smaller = GraphDoc {
        nodes: g.nodes[1..].to_vec(),
        ..g
    };
    let b2 = nika_tui_core::wasm::board_next(&b1, &serde_json::to_string(&smaller).expect("g2"));
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

// ── the 2026-08-13 sweep · the survivors of the long run, named ────

/// Build a workflow and a run from bare ids, needs and (start, dur).
fn bench(tasks: &[(&str, Vec<&str>)], steps: &[(&str, f64, f64)]) -> (Workflow, Run) {
    let wf = serde_json::json!({
        "file": "b.nika.yaml", "engine": "test", "prompt": "", "permits": [], "missing": "",
        "tasks": tasks.iter().map(|(id, needs)| serde_json::json!({
            "id": id, "verb": "infer", "glyph": "◇", "needs": needs,
        })).collect::<Vec<_>>(),
    });
    let run = serde_json::json!({
        "trace": "t", "when": "recorded", "output": "",
        "steps": steps.iter().map(|(id, start, dur)| serde_json::json!({
            "id": id, "start": start, "dur": dur,
        })).collect::<Vec<_>>(),
    });
    (
        serde_json::from_value(wf).expect("workflow"),
        serde_json::from_value(run).expect("run"),
    )
}

/// WHICH neck gets crowned when two waves both have one.
///
/// `idle_total > b.idle_total` carried THREE survivors (`>` to `<`, to
/// `==`, to `>=`). The crown is the whole point of the function: a surface
/// reading the wrong neck chases the wrong task and never diverges
/// visibly. Two cases pin all three mutants.
#[test]
fn the_worst_wave_wins_the_crown_and_a_tie_keeps_the_first() {
    // Wave 0 · a1 holds to 10, a2 idles 9. Wave 1 · b1 holds to 30, b2
    // idles 19. The strictly worse wave must win.
    let (wf, run) = bench(
        &[
            ("a1", vec![]),
            ("a2", vec![]),
            ("b1", vec!["a1"]),
            ("b2", vec!["a1"]),
        ],
        &[
            ("a1", 0.0, 10.0),
            ("a2", 0.0, 1.0),
            ("b1", 10.0, 20.0),
            ("b2", 10.0, 1.0),
        ],
    );
    let ws = derive::waves(&wf);
    let neck = derive::bottleneck(&ws, &run).expect("un goulot");
    assert_eq!(
        neck.id, "b1",
        "la vague la PIRE est couronnée · `<` et `==` la ratent"
    );
    assert_eq!(neck.idle_total, 19.0);

    // A TIE · both waves idle 9. Strict `>` keeps the FIRST; `>=` lets the
    // later one steal a crown it did not earn.
    let (wf, run) = bench(
        &[
            ("a1", vec![]),
            ("a2", vec![]),
            ("b1", vec!["a1"]),
            ("b2", vec!["a1"]),
        ],
        &[
            ("a1", 0.0, 10.0),
            ("a2", 0.0, 1.0),
            ("b1", 10.0, 10.0),
            ("b2", 10.0, 1.0),
        ],
    );
    let ws = derive::waves(&wf);
    let neck = derive::bottleneck(&ws, &run).expect("un goulot");
    assert_eq!(
        neck.id, "a1",
        "à égalité le premier garde la couronne · `>=` la vole"
    );
}

/// The blocked threshold is STRICT: an idle of exactly 0.05 does not count.
///
/// `idle_of(..) > 0.05` survived a flip to `>=`. Reached exactly by ending
/// the wave at 0.05 and the idler at 0.0, so the subtraction is
/// bit-identical to the literal rather than a float near it.
#[test]
fn an_idle_exactly_at_the_threshold_is_not_blocked() {
    let (wf, run) = bench(
        &[("a1", vec![]), ("a2", vec![])],
        &[("a1", 0.0, 0.05), ("a2", 0.0, 0.0)],
    );
    let ws = derive::waves(&wf);
    assert_eq!(
        derive::bottleneck(&ws, &run),
        None,
        "0.05 pile n est pas bloqué · `>=` inventerait un goulot"
    );
}

/// `Waves::contains` and `Waves::is_empty` had no direct caller at all, so
/// every mutant on them rode free. The zero-consumer law, at method scale.
#[test]
fn the_wave_index_answers_membership_and_emptiness() {
    let (wf, _run) = bench(&[("a", vec![]), ("b", vec!["a"])], &[]);
    let ws = derive::waves(&wf);
    assert!(ws.contains("a"), "une tâche déclarée est connue");
    assert!(ws.contains("b"));
    assert!(
        !ws.contains("fantome"),
        "un pas fantôme d un vieux journal n est PAS une tâche"
    );
    assert!(!ws.is_empty(), "deux vagues, donc non vide");
    assert_eq!(ws.len(), 2);

    let (empty, _run) = bench(&[], &[]);
    let ws = derive::waves(&empty);
    assert!(ws.is_empty(), "zéro tâche, zéro vague");
    assert_eq!(ws.len(), 0);
}

/// `group_span` picks its slowest by STRICT `>`, so a tie keeps the FIRST
/// member. `>=` would let a later equal member steal the title, and a
/// report that reshuffles on equal durations is a report nobody can pin.
#[test]
fn the_slowest_of_a_tie_is_the_first_member() {
    let members: Vec<Task> = serde_json::from_value(serde_json::json!([
        { "id": "x", "verb": "infer", "glyph": "◇", "needs": [] },
        { "id": "y", "verb": "infer", "glyph": "◇", "needs": [] },
    ]))
    .expect("members");
    let (_wf, run) = bench(&[], &[("x", 0.0, 5.0), ("y", 0.0, 5.0)]);
    let span = derive::group_span(&members, &run).expect("un span");
    assert_eq!(
        span.slowest, "x",
        "à durée égale le PREMIER reste le plus lent · `>=` le remplacerait"
    );
    assert_eq!(span.n, 2);

    // and a real difference still wins, so the test is not simply pinning
    // "always the first".
    let (_wf, run) = bench(&[], &[("x", 0.0, 5.0), ("y", 0.0, 9.0)]);
    let span = derive::group_span(&members, &run).expect("un span");
    assert_eq!(span.slowest, "y", "9 bat 5 · la comparaison fonctionne");
}

/// The four verb spellings are the WIRE voice, and a consumer pins them.
/// Both the enum speller and the fixture speller survived being blanked.
#[test]
fn the_four_verbs_spell_themselves() {
    assert_eq!(Verb::Infer.as_str(), "infer");
    assert_eq!(Verb::Exec.as_str(), "exec");
    assert_eq!(Verb::Invoke.as_str(), "invoke");
    assert_eq!(Verb::Agent.as_str(), "agent");

    // and through the derivation, where the fixture speller lives
    let (wf, run) = bench(&[("a", vec![])], &[("a", 0.0, 1.0)]);
    let costs = derive::cost_by_verb(&wf, &run);
    let names: Vec<&str> = costs.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, vec!["infer", "exec", "invoke", "agent"]);
}
