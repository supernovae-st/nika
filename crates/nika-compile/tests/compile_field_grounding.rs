// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Every key a typed rule reads is grounded before it reaches lowering (R4 S1): the source
//! declares it, a bounded observation shows it, or a human asserts it; a key nothing supports is
//! asked, never guessed, and the decision records the evidence it was grounded by.
//! - A request word the observed keys do not hold (« id » over `sku`, « quantity » over `units`,
//!   « expiration » and « expiry » over `expires`, « reduction » over `value`) is a precise
//!   question offering the observed keys; no synonym, spelling or similarity maps it.
//! - The answer grounds the key only in the context it was asked: the same answer after the
//!   source changed is stale, refused and asked again.
//! - A key some sampled records lack is grounded, but what the rule does with those records is
//!   not stated: the obligation stays open, never defaulted.
//! - Keys observed in the request's own language are named by it and stay READY.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, QuestionType, compile};
use serde_json::{Value, json};

const INVENTORY: &[&str] = &["sku", "units", "expires", "value"];

/// The inventory as the CLI observes it, at a revision (its bounded peek's hash).
fn inventory(path: &str, revision: &str) -> Value {
    let csv = csv(path);
    let mut row = json!({"path": path, "state": "observed", "complete": false,
        "kind": if csv { "csv" } else { "json" }, "columns": INVENTORY, "peek_sha256": revision});
    if !csv {
        row["common_columns"] = json!(INVENTORY);
    }
    json!({ "observed": [row] })
}

fn csv(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"))
}

fn grounding(out: &CompileOutcome) -> Vec<Value> {
    let decision = out.provenance.decision.as_ref().unwrap();
    decision["grounding"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Each request word the inventory does not spell, and the key a human maps it to.
const WORDS: &[(&str, &str, &str)] = &[
    ("whose id is A12", "id", "sku"),
    ("whose quantity is below 5", "quantity", "units"),
    ("whose expiration is 2026-10-01", "expiration", "expires"),
    ("whose expiry is 2026-10-01", "expiry", "expires"),
    ("whose reduction is above 10", "reduction", "value"),
];

#[test]
fn a_word_the_observed_keys_do_not_hold_is_asked_and_an_answer_grounds_it() {
    for path in ["./inventory.json", "./inventory.csv"] {
        for (clause, word, key) in WORDS {
            let intent =
                format!("Read {path}, keep only the rows {clause} and write them to ./out.json");
            let first =
                compile(&CompileRequest::create(&intent).with_knowledge(inventory(path, "r1")))
                    .unwrap();
            assert_ne!(first.status, CompileStatus::Ready, "{intent}: {first:#?}");
            assert!(first.candidate.is_none(), "{intent}");
            let question = first
                .questions
                .iter()
                .find(|q| q.key == "const.rule_field_1");
            assert!(question.is_some(), "{intent}: {first:#?}");
            let question = question.unwrap();
            assert_eq!(question.answer_type, QuestionType::Choice, "{intent}");
            let offered: Vec<&str> = question.options.iter().map(|o| o.key.as_str()).collect();
            assert_eq!(
                offered, INVENTORY,
                "{intent}: every observed key, none invented"
            );
            assert!(
                question.label.contains(&format!("`{word}`")),
                "{intent}: {}",
                question.label
            );
            let entry = &grounding(&first)[0];
            assert_eq!(entry["field"], json!(word), "{intent}");
            assert_eq!(entry["grade"], "inferred", "{intent}");
            assert_eq!(entry["admissible"], false, "{intent}");
            let record = first.provenance.plan.clone().unwrap();
            let answered = compile(
                &CompileRequest::create(&intent)
                    .with_plan(record)
                    .with_knowledge(inventory(path, "r1"))
                    .answer("const.rule_field_1", format!("\"{key}\"")),
            )
            .unwrap();
            assert_eq!(
                answered.status,
                CompileStatus::Ready,
                "{intent}: {answered:#?}"
            );
            let candidate = answered.candidate.as_deref().unwrap();
            assert!(
                candidate.contains(&format!(".{key}")),
                "{intent}: {candidate}"
            );
            assert!(
                !candidate.contains(&format!(".{word}")),
                "{intent}: {candidate}"
            );
            let entry = &grounding(&answered)[0];
            assert_eq!(entry["field"], json!(key), "{intent}");
            assert_eq!(entry["bound_by"], "answer", "{intent}");
            assert_eq!(entry["admissible"], true, "{intent}");
            let grade = if csv(path) {
                "declared"
            } else {
                "observed_partial"
            };
            assert_eq!(entry["grade"], grade, "{intent}");
            assert_eq!(entry["revision"], "r1", "{intent}");
        }
    }
}

/// An answer grounds its key in the revision it was asked against; another revision of the
/// source makes it stale: refused, asked again, never READY on the old mapping.
#[test]
fn an_answer_for_another_revision_of_the_source_is_stale() {
    let intent = "Read ./inventory.json, keep only the rows whose quantity is below 5 and write them to ./out.json";
    let first = compile(
        &CompileRequest::create(intent).with_knowledge(inventory("./inventory.json", "r1")),
    )
    .unwrap();
    let record = first.provenance.plan.clone().unwrap();
    let answer = |revision: &str| {
        compile(
            &CompileRequest::create(intent)
                .with_plan(record.clone())
                .with_knowledge(inventory("./inventory.json", revision))
                .answer("const.rule_field_1", "\"units\""),
        )
        .unwrap()
    };
    let stale = answer("r2");
    assert_ne!(stale.status, CompileStatus::Ready, "{stale:#?}");
    assert!(stale.candidate.is_none(), "{stale:#?}");
    assert!(
        stale
            .diagnostics
            .iter()
            .any(|d| d.target == "const.rule_field_1"
                && d.message.contains("changed since this question was asked")),
        "{stale:#?}"
    );
    assert!(
        stale
            .questions
            .iter()
            .any(|q| q.key == "const.rule_field_1"),
        "{stale:#?}"
    );
    let current = answer("r1");
    assert_eq!(current.status, CompileStatus::Ready, "{current:#?}");
}

/// A key some sampled records lack is grounded (it exists), but the rule over records lacking it
/// is an operator law the request does not state: not READY, the obligation named and recorded.
#[test]
fn a_key_some_records_lack_keeps_its_obligation_open() {
    let world = json!({"observed": [{"path": "./orders.json", "state": "observed",
        "complete": false, "kind": "json", "columns": ["id", "status", "discount"],
        "common_columns": ["id", "status"], "peek_sha256": "r1"}]});
    for clause in ["whose discount is above 3", "whose discount is not 0"] {
        let intent =
            format!("Read ./orders.json, keep only the rows {clause} and write them to ./out.json");
        let out = compile(&CompileRequest::create(&intent).with_knowledge(world.clone())).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(out.candidate.is_none(), "{intent}: {out:#?}");
        assert!(
            out.diagnostics.iter().any(|d| d.target == "grounding"
                && d.message
                    .contains("`discount` is in only some sampled records")),
            "{intent}: {out:#?}"
        );
        let entry = &grounding(&out)[0];
        assert_eq!(
            entry["admissible"], true,
            "{intent}: the key itself is grounded"
        );
        assert_eq!(entry["in_every_sampled_record"], false, "{intent}");
        assert_eq!(entry["open"], "records lacking the key", "{intent}");
    }
    // A word no observed key spells is asked over every observed key, the partial one included.
    let intent = "Read ./orders.json, keep only the rows whose rebate is above 3 and write them to ./out.json";
    let out = compile(&CompileRequest::create(intent).with_knowledge(world.clone())).unwrap();
    let question = out
        .questions
        .iter()
        .find(|q| q.key == "const.rule_field_1")
        .unwrap();
    let offered: Vec<&str> = question.options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(offered, ["id", "status", "discount"], "{out:#?}");
    // A key in every sampled record carries no such obligation.
    let intent =
        "Read ./orders.json, keep only the rows whose status is open and write them to ./out.json";
    let out = compile(&CompileRequest::create(intent).with_knowledge(world)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

/// No record observed (an empty file, rows that are not objects, a file not there yet): a key the
/// request only names is asked as the exact key, never lowered; an answer grounds it.
#[test]
fn a_source_that_shows_no_record_grounds_no_key() {
    let intent =
        "Read ./orders.json, keep only the rows whose status is open and write them to ./out.json";
    for state in ["empty", "unknown", "absent"] {
        let world =
            json!({"observed": [{"path": "./orders.json", "state": state, "complete": false}]});
        let out = compile(&CompileRequest::create(intent).with_knowledge(world.clone())).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{state}: {out:#?}");
        let question = out.questions.iter().find(|q| q.key == "const.rule_field_1");
        assert!(
            question.is_some_and(|q| q.answer_type == QuestionType::Text),
            "{state}: {out:#?}"
        );
        assert_eq!(grounding(&out)[0]["revision"], json!(state), "{state}");
        let record = out.provenance.plan.clone().unwrap();
        let answered = compile(
            &CompileRequest::create(intent)
                .with_plan(record)
                .with_knowledge(world)
                .answer("const.rule_field_1", "\"status\""),
        )
        .unwrap();
        assert_eq!(
            answered.status,
            CompileStatus::Ready,
            "{state}: {answered:#?}"
        );
        assert_eq!(grounding(&answered)[0]["grade"], "user_asserted", "{state}");
    }
}

/// Keys observed in the request's own language are the request's words: READY, no question.
#[test]
fn keys_observed_in_the_requests_language_are_named_by_it() {
    for (intent, path, keys) in [
        (
            "Lis ./commandes.json, garde seulement les commandes dont le statut est livré et écris le résultat dans ./livrees.json",
            "./commandes.json",
            vec!["id", "statut"],
        ),
        (
            "Lee ./pedidos.csv, conserva solo las filas cuyo importe supera 200 y escríbelas en ./grandes.csv",
            "./pedidos.csv",
            vec!["cliente", "importe"],
        ),
    ] {
        let csv = csv(path);
        let mut row = json!({"path": path, "state": "observed", "complete": false,
            "kind": if csv { "csv" } else { "json" }, "columns": keys, "peek_sha256": "r1"});
        if !csv {
            row["common_columns"] = json!(keys);
        }
        let out =
            compile(&CompileRequest::create(intent).with_knowledge(json!({"observed": [row]})))
                .unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert!(out.questions.is_empty(), "{intent}");
        let entry = &grounding(&out)[0];
        assert_eq!(entry["bound_by"], "request", "{intent}");
        assert_eq!(entry["admissible"], true, "{intent}");
    }
}

/// The same grounding feeds creation and replay: an answer round records the very evidence the
/// first round decided with.
#[test]
fn replay_records_the_same_grounding() {
    let intent = "Every weekday at 8, read ./inventory.json, keep only the rows whose units is below 5 and write them to ./out.json";
    let first = compile(
        &CompileRequest::create(intent).with_knowledge(inventory("./inventory.json", "r1")),
    )
    .unwrap();
    assert_eq!(first.status, CompileStatus::Ready, "{first:#?}");
    let record = first.provenance.plan.clone().unwrap();
    let again = compile(
        &CompileRequest::create(intent)
            .with_plan(record)
            .answer("trigger.timezone", "\"Europe/Paris\""),
    )
    .unwrap();
    assert_eq!(again.status, CompileStatus::Ready, "{again:#?}");
    assert_eq!(grounding(&again), grounding(&first));
    assert_eq!(again.candidate, first.candidate);
}
