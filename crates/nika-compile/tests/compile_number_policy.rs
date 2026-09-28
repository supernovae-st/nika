// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a rule does with a value that is not a number (R4 A5 · C3/C4). A numeric filter over
//! observed null, true, « n-a » and [2] was READY and failed at run on jq's `tonumber`; a top 2
//! over observed nulls ranked them lowest, a policy nobody stated. The host now counts the raw
//! kind of every sampled value (`world.kinds`, beside the rows): where a field a rule reads as a
//! number shows anything else, what such a record does is asked before READY (skip or fail),
//! bound to the source's revision; where it shows numbers only, the law reads it and an unseen
//! non-number stops the run with its value named. A plan recorded before replays unchanged and
//! is grounded again: it never runs lenient jq and never reaches READY past an observed
//! non-number without a stated policy.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, compile};
use serde_json::{Value, json};
use std::hash::{Hash, Hasher};

mod common;

const FILTER: &str = "Read ./tickets.json, keep only the rows whose amount is above 100 and write them to ./big.json";
const TOP: &str =
    "Read ./players.json, keep the 2 rows with the highest points and write them to ./top.json";
const TOTAL: &str = "Read ./tickets.json, compute the total of the amount column and write the total to ./total.txt";
const SORT: &str = "Read ./tickets.json, sort the rows by amount and write them to ./sorted.json";

/// The world a host observes for one JSON source: its row, and its kinds beside it.
fn world(path: &str, rows: &[Value]) -> Value {
    let sample = nika_compile::observation::records(rows);
    let text = serde_json::to_string(rows).unwrap();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    let mut row = json!({
        "path": path, "state": "observed", "complete": false, "kind": "json",
        "columns": sample.columns, "bytes": text.len(),
        "peek_sha256": format!("{:016x}", hasher.finish()), "common_columns": sample.common,
    });
    if !sample.values.is_empty() {
        row["values"] = Value::Object(sample.values.into_iter().collect());
    }
    json!({"observed": [row], "kinds": {path: sample.kinds}})
}

fn mixed() -> Vec<Value> {
    vec![
        json!({"id": 1, "amount": 120}),
        json!({"id": 2, "amount": "150"}),
        json!({"id": 3, "amount": null}),
        json!({"id": 4, "amount": true}),
        json!({"id": 5, "amount": "n-a"}),
        json!({"id": 6, "amount": [2]}),
        json!({"id": 7, "amount": 90}),
    ]
}

fn numbers() -> Vec<Value> {
    vec![
        json!({"id": 1, "amount": 120}),
        json!({"id": 2, "amount": "150"}),
        json!({"id": 3, "amount": 0}),
        json!({"id": 4, "amount": 90, "note": "call back"}),
    ]
}

fn run(intent: &str, world: &Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent).with_knowledge(world.clone());
    for (key, value) in answers {
        request = request.answer(*key, *value);
    }
    compile(&request).unwrap()
}

fn question<'a>(out: &'a CompileOutcome, key: &str) -> Option<&'a nika_compile::CompileQuestion> {
    out.questions.iter().find(|q| q.key == key)
}

fn numbers_record(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().expect("decision")["numbers"].clone()
}

#[test]
fn observed_non_numbers_are_asked_before_ready_and_never_run_on_lenient_jq() {
    let out = run(FILTER, &world("./tickets.json", &mixed()), &[]);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.candidate.is_none(),
        "nothing is emitted before the answer"
    );
    let asked = question(&out, "const.rule_number_1").expect("the policy is asked");
    assert!(asked.mandatory);
    assert_eq!(
        asked
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["skip", "fail"]
    );
    assert!(
        asked
            .label
            .contains("not a number in 4 of 7 sampled records")
            && asked
                .label
                .contains("null 1 · true/false 1 · text 1 · list 1"),
        "{}",
        asked.label
    );
    assert!(
        asked.why.contains("never a whole-file proof"),
        "{}",
        asked.why
    );
    assert_eq!(numbers_record(&out)[0]["bound_by"], "pending");
    assert!(
        !out.questions
            .iter()
            .any(|q| q.key == "intent.clarification"),
        "one question, never a second one about the same records: {out:#?}"
    );
}

#[test]
fn skip_and_fail_answers_bind_the_law_and_a_wrong_answer_is_asked_again() {
    let world = world("./tickets.json", &mixed());
    let skip = run(FILTER, &world, &[("const.rule_number_1", r#""skip""#)]);
    assert_eq!(skip.status, CompileStatus::Ready, "{skip:#?}");
    let source = skip.candidate.as_deref().unwrap();
    assert_eq!(
        common::compute(source),
        "[.records[] | select(((.amount | isnum) and (.amount | num) > 100))]"
    );
    assert!(
        !source.contains("has(\"amount\")"),
        "SKIP is not pre-empted by the guard"
    );
    assert_eq!(numbers_record(&skip)[0]["policy"], "skip");
    assert_eq!(numbers_record(&skip)[0]["bound_by"], "answer");
    let fail = run(FILTER, &world, &[("const.rule_number_1", r#""fail""#)]);
    assert_eq!(fail.status, CompileStatus::Ready, "{fail:#?}");
    assert_eq!(
        common::compute(fail.candidate.as_deref().unwrap()),
        "[.records[] | select((.amount | num) > 100)]"
    );
    let wrong = run(FILTER, &world, &[("const.rule_number_1", r#""maybe""#)]);
    assert_ne!(wrong.status, CompileStatus::Ready);
    assert!(question(&wrong, "const.rule_number_1").is_some());
    assert!(
        wrong
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Missed
                && d.target == "const.rule_number_1"
                && d.message == "Answer skip or fail."),
        "{wrong:#?}"
    );
}

#[test]
fn a_top_n_over_observed_nulls_is_asked_and_never_ranks_them_by_total_order() {
    let players = vec![
        json!({"name": "a", "points": 12}),
        json!({"name": "b", "points": null}),
        json!({"name": "c", "points": 30}),
        json!({"name": "d", "points": 7}),
        json!({"name": "e", "points": null}),
    ];
    let world = world("./players.json", &players);
    let out = run(TOP, &world, &[]);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked = question(&out, "const.rule_number_1").expect("asked");
    assert!(asked.label.contains("(null 2)"), "{}", asked.label);
    let skip = run(TOP, &world, &[("const.rule_number_1", r#""skip""#)]);
    assert_eq!(skip.status, CompileStatus::Ready, "{skip:#?}");
    assert_eq!(
        common::compute(skip.candidate.as_deref().unwrap()),
        ".records | (length as $all | map(select((.points | isnum))) | if length == 0 and $all > 0 then error(\"no `points` is a number: no row can be ranked\") else . end) | sort_by((.points | num)) | reverse | .[:2]"
    );
}

#[test]
fn numbers_only_ask_nothing_and_an_unobserved_source_reads_the_law() {
    // A number, a decimal text and a zero are numbers; an unrelated text column asks nothing.
    let out = run(FILTER, &world("./tickets.json", &numbers()), &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(question(&out, "const.rule_number_1").is_none());
    assert_eq!(
        common::compute(out.candidate.as_deref().unwrap()),
        "[.records[] | select((.amount | num) > 100)]"
    );
    let record = numbers_record(&out);
    assert_eq!(record[0]["bound_by"], "observed numbers");
    assert_eq!(record[0]["kinds"], json!({"number": 3, "number_text": 1}));
    assert_eq!(record[0]["sampled"], 4);
    // Nothing observed: once the key is named (S1), the law reads it, FAIL for a non-number.
    let bare = compile(&CompileRequest::create(FILTER).answer("const.rule_field_1", r#""amount""#))
        .unwrap();
    assert_eq!(bare.status, CompileStatus::Ready, "{bare:#?}");
    assert_eq!(
        common::compute(bare.candidate.as_deref().unwrap()),
        "[.records[] | select((.amount | num) > 100)]"
    );
    assert_eq!(numbers_record(&bare)[0]["bound_by"], "unobserved");
}

#[test]
fn zero_false_and_missing_stay_three_facts() {
    let rows = vec![
        json!({"id": 1, "amount": 0}),
        json!({"id": 2, "amount": false}),
        json!({"id": 3}),
        json!({"id": 4, "amount": 150}),
    ];
    let out = run(FILTER, &world("./tickets.json", &rows), &[]);
    let asked = question(&out, "const.rule_number_1").expect("asked");
    assert!(
        asked
            .label
            .contains("not a number in 2 of 4 sampled records (missing 1 · true/false 1)"),
        "{}",
        asked.label
    );
    // The missing key is one of the kinds the question names: never a second question for it.
    assert!(
        !out.questions
            .iter()
            .any(|q| q.key == "intent.clarification"),
        "{out:#?}"
    );
}

#[test]
fn a_total_and_a_plain_sort_read_the_law_where_the_sample_says_numbers() {
    let world_mixed = world("./tickets.json", &mixed());
    let total = run(TOTAL, &world_mixed, &[("const.rule_number_1", r#""skip""#)]);
    assert_eq!(total.status, CompileStatus::Ready, "{total:#?}");
    assert!(
        common::compute(total.candidate.as_deref().unwrap())
            .contains("error(\"no `amount` is a number: its total cannot be stated\")"),
        "{}",
        common::compute(total.candidate.as_deref().unwrap())
    );
    let sorted = run(SORT, &world("./tickets.json", &numbers()), &[]);
    assert_eq!(sorted.status, CompileStatus::Ready, "{sorted:#?}");
    assert!(
        common::compute(sorted.candidate.as_deref().unwrap()).contains("sort_by((.amount | num))"),
        "{}",
        common::compute(sorted.candidate.as_deref().unwrap())
    );
    // A plain sort over a key the sample shows as text keeps its legacy reading.
    let names = vec![json!({"amount": "low"}), json!({"amount": "high"})];
    let text = run(SORT, &world("./tickets.json", &names), &[]);
    assert!(
        common::compute(text.candidate.as_deref().unwrap_or_default())
            .contains("sort_by(.amount | tonumber? // .)"),
        "{text:#?}"
    );
}

#[test]
fn an_answer_for_another_revision_is_asked_again_even_when_only_the_kinds_moved() {
    let first = world("./tickets.json", &mixed());
    let asked = run(FILTER, &first, &[]);
    let plan = asked.provenance.plan.clone().expect("a recorded plan");
    // The same row identity (a forged unchanged peek), but a type-only change moved the kinds:
    // one more text where a number was.
    let mut shifted = mixed();
    shifted[0] = json!({"id": 1, "amount": "n-b"});
    let mut moved = world("./tickets.json", &shifted);
    moved["observed"][0] = first["observed"][0].clone();
    let request = CompileRequest::create(FILTER)
        .with_plan(plan.clone())
        .with_knowledge(moved)
        .answer("const.rule_number_1", r#""skip""#);
    let out = compile(&request).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "const.rule_number_1" && d.message.contains("changed since")),
        "{out:#?}"
    );
    // The same revision keeps the answer.
    let kept = CompileRequest::create(FILTER)
        .with_plan(plan)
        .with_knowledge(first)
        .answer("const.rule_number_1", r#""skip""#);
    assert_eq!(compile(&kept).unwrap().status, CompileStatus::Ready);
}

#[test]
fn a_legacy_numeric_plan_replays_canonical_and_is_grounded_again() {
    // A plan recorded with no observation: its rule record is the reader's own reading.
    let legacy = compile(&CompileRequest::create(FILTER)).unwrap();
    let plan = legacy.provenance.plan.clone().expect("a recorded plan");
    let rule_jq = plan["rules"][0]["jq"].as_str().unwrap();
    assert_eq!(rule_jq, "[.records[] | select((.amount | tonumber) > 100)]");
    // Replayed over numbers: canonical, READY, and the candidate reads the law.
    let clean = compile(
        &CompileRequest::create(FILTER)
            .with_plan(plan.clone())
            .with_knowledge(world("./tickets.json", &numbers())),
    )
    .unwrap();
    assert_eq!(clean.status, CompileStatus::Ready, "{clean:#?}");
    assert_eq!(
        common::compute(clean.candidate.as_deref().unwrap()),
        "[.records[] | select((.amount | num) > 100)]"
    );
    // Replayed over observed non-numbers: never READY without a stated policy.
    let mixed = compile(
        &CompileRequest::create(FILTER)
            .with_plan(plan.clone())
            .with_knowledge(world("./tickets.json", &mixed())),
    )
    .unwrap();
    assert_ne!(mixed.status, CompileStatus::Ready, "{mixed:#?}");
    assert!(
        question(&mixed, "const.rule_number_1").is_some(),
        "{mixed:#?}"
    );
    // A record carrying a policy or spellings is refused, never read without them.
    for (path, value) in [
        ("numbers", json!({"amount": "skip"})),
        ("numbers", json!({"amount": 7})),
    ] {
        let mut forged = plan.clone();
        forged["rules"][0][path] = value;
        let out = compile(&CompileRequest::create(FILTER).with_plan(forged)).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "recorded_plan" && d.message.contains("cannot be replayed")),
            "{out:#?}"
        );
    }
    let mut forged = plan;
    forged["rules"][0]["clauses"][0]["spellings"] = json!(["100"]);
    let out = compile(&CompileRequest::create(FILTER).with_plan(forged)).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
}
