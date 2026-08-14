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
        .map(|i| {
            Task::new(
                format!(
                    "traduire-{}",
                    [
                        "fr", "en", "de", "es", "it", "pt", "nl", "pl", "ja", "ko", "zh", "ar"
                    ][i]
                ),
                Verb::Infer,
                "◇".to_owned(),
                Vec::new(),
            )
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
    let members = vec![Task::new(
        "fantome".to_owned(),
        Verb::Infer,
        "◇".to_owned(),
        Vec::new(),
    )];
    let run = Run::new(
        "t".to_owned(),
        "recorded".to_owned(),
        String::new(),
        Vec::new(),
    );
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
    let priced = |id: &str, start: f64, cost: Option<f64>| {
        let mut s = Step::new(id.to_owned(), start, 1.0);
        s.cost = cost;
        s
    };
    let run = Run::new(
        "t".to_owned(),
        "recorded".to_owned(),
        String::new(),
        vec![
            priced("a", 0.0, Some(0.004)),
            priced("b", 1.0, Some(0.02)),
            priced("c", 2.0, None), // the unpriced one contributes nothing
        ],
    );
    assert_eq!(derive::total_cost(&run), 0.024);
}

/// `undeclared` with a REAL gap — a workflow whose steps touch what the
/// file never declared (the demo declares everything, so an empty-vector
/// mutant survived).
#[test]
fn undeclared_names_the_missing_permit() {
    let mut lit = Task::new("lit".to_owned(), Verb::Invoke, "◆".to_owned(), Vec::new());
    lit.tool = Some("nika:read".to_owned());
    lit.origin = Some(Origin::Builtin);
    lit.touches = Some(vec![Touch::FsRead, Touch::Tools]);
    let wf = Workflow::new(
        "gap.nika.yaml".to_owned(),
        "test".to_owned(),
        String::new(),
        vec!["exec: [\"gh\"]".to_owned()],
        String::new(),
        vec![lit],
    );
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
    let smaller = g.with_nodes(g.nodes[1..].to_vec());
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
        "à durée égale le PREMIER demeure le plus lent · `>=` le remplacerait"
    );
    assert_eq!(span.n, 2);

    // and a real difference still wins, so the test is not simply pinning
    // "always the first".
    let (_wf, run) = bench(&[], &[("x", 0.0, 5.0), ("y", 0.0, 9.0)]);
    let span = derive::group_span(&members, &run).expect("un span");
    assert_eq!(span.slowest, "y", "9 bat 5 · la comparaison fonctionne");
}

/// A step that never STARTED never waited, so it cannot crown a bottleneck.
///
/// Named by a Rust review, 2026-08-13. The fold stamps a cancelled or
/// skipped step at 0/0 because it has no real instants, and `idle_of` read
/// that zero as "started at the top of the wave and idled until the end".
/// So on any Ctrl-C run the dead steps accused the wave's holder of the
/// whole wave: the « 420,4 s d'attente » class the code says it closed,
/// walking back in through a different door.
#[test]
fn a_step_that_never_started_never_waited() {
    let wf: Workflow = serde_json::from_value(serde_json::json!({
        "file": "c.nika.yaml", "engine": "test", "prompt": "",
        "permits": [], "missing": "",
        "tasks": [
            { "id": "real", "verb": "infer", "glyph": "◇", "needs": [] },
            { "id": "dead", "verb": "infer", "glyph": "◇", "needs": [] },
        ],
    }))
    .expect("workflow");
    // `dead` never began: the fold gives it 0/0 and marks it never_born.
    let run: Run = serde_json::from_value(serde_json::json!({
        "trace": "t", "when": "recorded", "output": "",
        "steps": [
            { "id": "real", "start": 0.0, "dur": 10.0 },
            // `neverBorn`, not `never_born`: Step is camelCase, and a key
            // that does not match is dropped in SILENCE because the field is
            // `#[serde(default)]`. Written snake_case first, this fixture
            // deserialized to None and the test failed loudly — which is the
            // good outcome. The same silence is the P0 a review found on
            // `Node` the same hour, one struct away.
            { "id": "dead", "start": 0.0, "dur": 0.0, "neverBorn": true },
        ],
    }))
    .expect("run");
    let ws = derive::waves(&wf);

    assert_eq!(
        derive::idle_of(&ws, &run, "dead"),
        0.0,
        "un pas jamais né n a pas attendu · son 0/0 n est pas une oisiveté"
    );
    assert_eq!(
        derive::bottleneck(&ws, &run),
        None,
        "personne n a attendu `real` · il ne tient rien"
    );

    // And a step that DID run and wait is still counted, so the fix does not
    // simply silence the derivation.
    let run: Run = serde_json::from_value(serde_json::json!({
        "trace": "t", "when": "recorded", "output": "",
        "steps": [
            { "id": "real", "start": 0.0, "dur": 10.0 },
            { "id": "dead", "start": 0.0, "dur": 1.0 },
        ],
    }))
    .expect("run");
    assert_eq!(
        derive::idle_of(&ws, &run, "dead"),
        9.0,
        "celui-là a vraiment attendu"
    );
    let neck = derive::bottleneck(&ws, &run).expect("un goulot réel");
    assert_eq!(neck.id, "real");
    assert_eq!(neck.blocked, 1);
}

/// A different verb BREAKS the grouping signature, so tasks that are not
/// alike are never reported as alike.
///
/// `verb_name` has exactly one caller: `signature`, which is
/// `verb|tool|sorted-needs`. Blanking it makes every verb spell the same, so
/// an infer and an exec sharing a tool and needs collapse into one fanout.
///
/// Its two mutants survived the first sweep, and the test written for them
/// missed: it asserted `cost_by_verb`'s names, which are HARDCODED literals
/// that never reach `verb_name`. A test that pins the wrong subject passes
/// and proves nothing, which is the failure this whole file exists to close.
#[test]
fn a_different_verb_breaks_the_grouping_signature() {
    let three = |third_verb: &str| -> Workflow {
        serde_json::from_value(serde_json::json!({
            "file": "g.nika.yaml", "engine": "test", "prompt": "",
            "permits": [], "missing": "",
            "tasks": [
                { "id": "traduire-fr", "verb": "infer", "glyph": "◇", "needs": [] },
                { "id": "traduire-en", "verb": "infer", "glyph": "◇", "needs": [] },
                { "id": "traduire-de", "verb": third_verb, "glyph": "◇", "needs": [] },
            ],
        }))
        .expect("workflow")
    };

    // Three alike · one fanout. Without this arm the test would pass by
    // pinning "always Single", which is not the property.
    let groups = derive::groups_of(&three("infer"));
    assert_eq!(groups.len(), 1, "trois jumelles font UN éventail");
    let Group::Fanout { members, .. } = &groups[0] else {
        panic!("un éventail attendu, pas un singleton");
    };
    assert_eq!(members.len(), 3);

    // Change ONE verb and the third task stops being a twin. Two twins are
    // not enough for a fanout (three or more), so all three stand alone.
    let groups = derive::groups_of(&three("exec"));
    assert_eq!(
        groups.len(),
        3,
        "un verbe différent casse la signature · `verb_name` blanchi les \
         referait fusionner"
    );
    assert!(
        groups.iter().all(|g| matches!(g, Group::Single(_))),
        "aucun éventail ne survit à un verbe divergent"
    );
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

/// ⭐ THE LAW HAD ONE SEAM LEFT — a Gate-11 lens found it.
///
/// `idle_against` refuses to count a step that never started; `wave_end`
/// did NOT, and neither did `total_time` for a SKIPPED step. Both leaned on
/// an invariant only the journal fold guarantees (`ingress` zeroes the
/// timings of a timeless step) — but `wasm::derive_run` deserializes the
/// caller's run DIRECTLY, so a step marked never-born while carrying real
/// timings walks straight in.
///
/// The consequence is the exact misattribution the crate says it closed:
/// the dead step is crowned holder of its wave, and a step that finished on
/// time is reported as having idled for it.
#[test]
fn a_step_that_never_ran_cannot_extend_its_wave() {
    let wf: nika_tui_core::model::Workflow = serde_json::from_value(serde_json::json!({
        "file": "w.nika.yaml", "engine": "test", "prompt": "", "permits": [], "missing": "",
        "tasks": [
            {"id": "real",  "verb": "infer", "glyph": "◇", "needs": []},
            {"id": "ghost", "verb": "infer", "glyph": "◇", "needs": []},
        ],
    }))
    .expect("wf");
    let run: Run = serde_json::from_value(serde_json::json!({
        "trace": "t", "when": "recorded", "output": "",
        "steps": [
            {"id": "real",  "start": 0.0,   "dur": 10.0},
            // never born, yet carrying timings — only the fold zeroes those
            {"id": "ghost", "start": 500.0, "dur": 500.0, "neverBorn": true},
        ],
    }))
    .expect("run");

    let ws = derive::waves(&wf);
    assert_eq!(
        derive::wave_end(&ws, &run, 0),
        10.0,
        "a step that never started must not stretch its wave to 1000 s"
    );
    assert_eq!(
        derive::idle_of(&ws, &run, "real"),
        0.0,
        "`real` finished with the wave · it waited for nobody"
    );
    assert!(
        derive::bottleneck(&ws, &run).is_none(),
        "nobody held this wave · crowning the ghost blames `real` for 990 s"
    );

    // the sister seam · `total_time` filtered never-born but not SKIPPED
    let skipped: Run = serde_json::from_value(serde_json::json!({
        "trace": "t", "when": "recorded", "output": "",
        "steps": [
            {"id": "real",    "start": 0.0,   "dur": 10.0},
            {"id": "sauteur", "start": 900.0, "dur": 100.0, "skipped": true},
        ],
    }))
    .expect("run");
    assert_eq!(
        derive::total_time(&skipped),
        10.0,
        "a skipped step contributes no duration · the run lasted 10 s"
    );
}

/// ⭐ THE BATCH MUST AGREE WITH THE SINGULAR — the safety property of the
/// whole refactor, and the test the batch shipped WITHOUT.
///
/// A crate-wide mutation sweep run minutes after they landed put all 14 of
/// its survivors here: `wave_ends` and `idles` could be replaced by `vec![]`
/// or an empty map and every test stayed green. The parity fixtures pin the
/// SINGULAR forms; the door's gate only judges time. Two public functions on
/// the hot path, zero Rust coverage — the zero-consumer law at the scale of
/// a function, committed by the same hand that wrote the law this evening.
///
/// The property is the one that makes the batch safe to exist: it is not a
/// second implementation of the law, it is the same law asked n times, so
/// it must answer exactly what n singular questions answer. Bit-for-bit —
/// this crate's parity is measured on bits.
#[test]
fn the_batch_derivations_answer_exactly_what_the_singular_ones_do() {
    for name in [
        "demo-ok",
        "demo-fail",
        "stress",
        "sonde-single",
        "sonde-neverborn",
        "sonde-diamant",
    ] {
        let (wf, run, _d) = load(name);
        let ws = derive::waves(&wf);

        let batch_ends = derive::wave_ends(&ws, &run);
        assert_eq!(batch_ends.len(), ws.len(), "{name} · one end per wave");
        for (w, &got) in batch_ends.iter().enumerate() {
            assert_eq!(
                got,
                derive::wave_end(&ws, &run, w),
                "{name} · wave {w} · the batch and the singular disagree"
            );
        }

        let batch_idles = derive::idles(&ws, &run);
        assert_eq!(
            batch_idles.len(),
            run.steps.len(),
            "{name} · one idle per step, ghosts included"
        );
        for s in &run.steps {
            assert_eq!(
                batch_idles.get(&s.id).copied(),
                Some(derive::idle_of(&ws, &run, &s.id)),
                "{name} · step {} · the batch and the singular disagree",
                s.id
            );
        }
    }

    // A synthetic case pins what no fixture carries: a step that did NOT run
    // sharing a wave with one that did, and arithmetic no constant can fake.
    let wf: nika_tui_core::model::Workflow = serde_json::from_value(serde_json::json!({
        "file": "b.nika.yaml", "engine": "test", "prompt": "", "permits": [], "missing": "",
        "tasks": [
            {"id": "a", "verb": "infer", "glyph": "◇", "needs": []},
            {"id": "b", "verb": "infer", "glyph": "◇", "needs": []},
            {"id": "c", "verb": "infer", "glyph": "◇", "needs": ["a"]},
        ],
    }))
    .expect("wf");
    let run: Run = serde_json::from_value(serde_json::json!({
        "trace": "t", "when": "recorded", "output": "",
        "steps": [
            {"id": "a", "start": 2.0, "dur": 3.0},          // end 5 · not 2-3, not 2*3
            {"id": "b", "start": 90.0, "dur": 9.0, "neverBorn": true},
            {"id": "c", "start": 5.0, "dur": 2.5},          // end 7.5, its own wave
        ],
    }))
    .expect("run");
    let ws = derive::waves(&wf);
    let ends = derive::wave_ends(&ws, &run);
    assert_eq!(
        ends,
        vec![5.0, 7.5],
        "wave 0 ends at a's 5 s (b never ran · 99 would be its ghost), wave 1 at 7,5"
    );

    let idles = derive::idles(&ws, &run);
    assert_eq!(idles.get("a").copied(), Some(0.0), "a held its own wave");
    assert_eq!(
        idles.get("b").copied(),
        Some(0.0),
        "a step that never ran waits for nothing"
    );
    assert_eq!(idles.get("c").copied(), Some(0.0), "c alone in wave 1");
}
