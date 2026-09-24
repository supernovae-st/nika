// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Writes whose targets name no file each keep one stable path question: a single such write
//! is asked as `const.output_path`; several are numbered in plan order and keep their keys
//! when another one is answered, one answer never binds two outputs, and a file another
//! output already receives is refused. Recorded plans replay through the assembler with zero
//! calls.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::{Value, json};

mod common;
use common::keys;

const TWO: &str = "Read ./data/orders.csv, harmonise the totals per country and write a short note. Save the totals as JSON and save the note as Markdown.";
const THREE: &str = "Read ./data/orders.csv, harmonise the totals per country and write a short note. Save the totals as JSON and save the note as Markdown. Keep a copy of the note in ./out/archive.md.";
const ONE: &str =
    "Read ./data/orders.csv, harmonise the totals per country. Save the totals as JSON.";
const REPEATED: &str = "Read ./data/orders.csv and write a short note. Save a copy of the note for the team and save a copy of the note for the archive.";

const MODEL: (&str, &str) = ("model", r#""mock/echo""#);
const RULE: (&str, &str) = ("const.rule_expression", r#"".records""#);

fn write(target: &str, evidence: &str) -> Value {
    json!({"evidence": evidence, "policy": "automatic", "policy_literal": null, "target": target, "verb": "write"})
}

fn plan(operations: Value, effects: Vec<Value>) -> Value {
    let mut record = json!({"bindings": [{"literal": "./data/orders.csv", "role": "path"}], "constraints": [],
           "obligations": [], "rules": [], "slots": [],
           "strategy": "cold", "trigger": null, "unknowns": []});
    record["operations"] = operations;
    record["effects"] = Value::Array(effects);
    record
}

fn read_compute_draft() -> Value {
    json!([
        {"categories": [], "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv", "op": "read"},
        {"categories": [], "detail": "the totals per country", "evidence": "harmonise the totals per country", "op": "compute"},
        {"categories": [], "detail": "a short note", "evidence": "write a short note", "op": "draft"}
    ])
}

fn two_outputs() -> Value {
    plan(
        read_compute_draft(),
        vec![
            write("the totals as JSON", "Save the totals as JSON"),
            write("the note as Markdown", "save the note as Markdown"),
        ],
    )
}

fn run(intent: &str, plan: Value, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent).with_plan(plan);
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile(&request).unwrap()
}

fn label<'a>(out: &'a CompileOutcome, key: &str) -> &'a str {
    &out.questions.iter().find(|q| q.key == key).unwrap().label
}

fn document(out: &CompileOutcome) -> Value {
    let source = out.candidate.as_deref().expect("a candidate is assembled");
    serde_yaml_bw::from_str(source).unwrap()
}

#[test]
fn a_single_unnamed_output_keeps_the_output_path_key() {
    let single = plan(
        json!([
            {"categories": [], "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv", "op": "read"},
            {"categories": [], "detail": "the totals per country", "evidence": "harmonise the totals per country", "op": "compute"}
        ]),
        vec![write("the totals as JSON", "Save the totals as JSON")],
    );
    let asked = run(ONE, single.clone(), &[RULE]);
    assert!(keys(&asked).contains(&"const.output_path"), "{asked:#?}");
    assert!(!keys(&asked).contains(&"const.output_1_path"), "{asked:#?}");
    let out = run(
        ONE,
        single,
        &[RULE, ("const.output_path", r#""./out/totals.json""#)],
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(document(&out)["const"]["output_path"], "./out/totals.json");
}

#[test]
fn two_unnamed_outputs_ask_two_distinct_keys_in_the_first_round() {
    let asked = run(TWO, two_outputs(), &[MODEL, RULE]);
    let asked_keys = keys(&asked);
    assert!(asked_keys.contains(&"const.output_1_path"), "{asked:#?}");
    assert!(asked_keys.contains(&"const.output_2_path"), "{asked:#?}");
    assert!(!asked_keys.contains(&"const.output_path"), "{asked:#?}");
    assert_eq!(
        asked_keys
            .iter()
            .filter(|key| key.ends_with("_path") && key.starts_with("const.output"))
            .count(),
        2,
        "one question per output: {asked:#?}"
    );
    assert!(label(&asked, "const.output_1_path").contains("the totals as JSON"));
    assert!(label(&asked, "const.output_2_path").contains("the note as Markdown"));
    assert!(asked.candidate.is_none(), "{asked:#?}");
}

#[test]
fn answering_one_output_keeps_the_other_questions_identity() {
    let out = run(
        TWO,
        two_outputs(),
        &[MODEL, RULE, ("const.output_2_path", r#""./out/note.md""#)],
    );
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked_keys = keys(&out);
    assert!(asked_keys.contains(&"const.output_1_path"), "{out:#?}");
    assert!(!asked_keys.contains(&"const.output_2_path"), "{out:#?}");
    assert!(!asked_keys.contains(&"const.output_path"), "{out:#?}");
    assert!(label(&out, "const.output_1_path").contains("the totals as JSON"));
}

#[test]
fn two_different_answers_stay_attached_to_their_own_outputs() {
    let out = run(
        TWO,
        two_outputs(),
        &[
            MODEL,
            RULE,
            ("const.output_1_path", r#""./out/totals.json""#),
            ("const.output_2_path", r#""./out/note.md""#),
        ],
    );
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = document(&out);
    assert_eq!(
        doc["permits"]["fs"]["write"],
        json!(["./out/totals.json", "./out/note.md"]),
        "{doc:#}"
    );
    assert_eq!(doc["const"]["output_path"], "./out/totals.json", "{doc:#}");
    assert_eq!(doc["const"]["note_path"], "./out/note.md", "{doc:#}");
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"], "${{ tasks.compute.output }}",
        "{doc:#}"
    );
    assert_eq!(
        doc["tasks"]["write_note"]["with"]["content"], "${{ tasks.draft.output.body }}",
        "{doc:#}"
    );
}

#[test]
fn one_path_answered_for_two_outputs_is_refused_for_the_second() {
    let out = run(
        TWO,
        two_outputs(),
        &[
            MODEL,
            RULE,
            ("const.output_1_path", r#""./out/both.md""#),
            ("const.output_2_path", r#""./out/both.md""#),
        ],
    );
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).contains(&"const.output_2_path"), "{out:#?}");
    assert!(!keys(&out).contains(&"const.output_1_path"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "const.output_2_path"
                && d.message.contains("already receives another output")),
        "{out:#?}"
    );
}

#[test]
fn a_named_output_keeps_its_file_and_the_unnamed_ones_their_own_keys() {
    let three = plan(
        read_compute_draft(),
        vec![
            write("the totals as JSON", "Save the totals as JSON"),
            write("the note as Markdown", "save the note as Markdown"),
            write(
                "./out/archive.md",
                "Keep a copy of the note in ./out/archive.md",
            ),
        ],
    );
    let asked = run(THREE, three.clone(), &[MODEL, RULE]);
    let asked_keys = keys(&asked);
    assert!(asked_keys.contains(&"const.output_1_path"), "{asked:#?}");
    assert!(asked_keys.contains(&"const.output_2_path"), "{asked:#?}");
    assert!(!asked_keys.contains(&"const.output_path"), "{asked:#?}");
    // The named file is another output's: answering it for an unnamed one is refused.
    let refused = run(
        THREE,
        three,
        &[
            MODEL,
            RULE,
            ("const.output_2_path", r#""./out/archive.md""#),
        ],
    );
    assert!(
        keys(&refused).contains(&"const.output_2_path"),
        "{refused:#?}"
    );
    assert!(
        refused
            .diagnostics
            .iter()
            .any(|d| d.target == "const.output_2_path"
                && d.message.contains("already receives another output")),
        "{refused:#?}"
    );
}

#[test]
fn repeated_noun_phrases_keep_distinct_questions() {
    let repeated = plan(
        json!([
            {"categories": [], "detail": "./data/orders.csv", "evidence": "Read ./data/orders.csv", "op": "read"},
            {"categories": [], "detail": "a short note", "evidence": "write a short note", "op": "draft"}
        ]),
        vec![
            write("a copy of the note", "Save a copy of the note for the team"),
            write(
                "a copy of the note",
                "save a copy of the note for the archive",
            ),
        ],
    );
    let asked = run(REPEATED, repeated.clone(), &[MODEL]);
    assert!(keys(&asked).contains(&"const.output_1_path"), "{asked:#?}");
    assert!(keys(&asked).contains(&"const.output_2_path"), "{asked:#?}");
    let one = run(
        REPEATED,
        repeated,
        &[MODEL, ("const.output_1_path", r#""./out/team.md""#)],
    );
    assert!(keys(&one).contains(&"const.output_2_path"), "{one:#?}");
    assert!(!keys(&one).contains(&"const.output_1_path"), "{one:#?}");
}

#[test]
fn dot_components_and_repeated_separators_do_not_create_distinct_destinations() {
    for alias in ["out/both.md", "./out/./both.md", "./out//both.md"] {
        let answer = serde_json::to_string(alias).unwrap();
        let out = run(
            TWO,
            two_outputs(),
            &[
                MODEL,
                RULE,
                ("const.output_1_path", r#""./out/both.md""#),
                ("const.output_2_path", &answer),
            ],
        );
        assert_ne!(out.status, CompileStatus::Ready, "{alias}: {out:#?}");
        assert!(
            keys(&out).contains(&"const.output_2_path"),
            "{alias}: {out:#?}"
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.message.contains("already receives another output")),
            "{alias}: {out:#?}"
        );
    }
}
