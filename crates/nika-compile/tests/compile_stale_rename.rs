// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A field answer across a source change that renames a column (R4 A6 · D6-S1). « whose
//! quantity is below 5 » over `units` asked which field `quantity` means; the file then renamed
//! `units` to `quantity`. The answer `units` came back with the fresh observation: the rule's word
//! was now a declared key, the grounding took it before looking at the answer, the answer found no
//! question and the round said « No current question owns this answer ». Now the answer the
//! conversation asked for is read first: stale, refused and asked again over the fresh keys, the
//! plan re-anchored to the observation that question showed; only a fresh explicit answer binds.
//! An answer no question asked stays unowned, as before.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::{Value, json};

const INTENT: &str = "Read ./inventory.json, keep only the rows whose quantity is below 5 and write them to ./out.json";
const KEY: &str = "const.rule_field_1";

/// The inventory as a host observes it: its keys at a revision (the bounded peek's hash).
fn inventory(columns: &[&str], revision: &str) -> Value {
    json!({"observed": [{"path": "./inventory.json", "state": "observed", "complete": false,
        "kind": "json", "columns": columns, "common_columns": columns, "peek_sha256": revision}]})
}

const BEFORE: &[&str] = &["sku", "units", "expires"];
const RENAMED: &[&str] = &["sku", "quantity", "expires"];

fn round(plan: &Value, world: Value, answer: Option<&str>) -> CompileOutcome {
    let mut request = CompileRequest::create(INTENT)
        .with_plan(plan.clone())
        .with_knowledge(world);
    if let Some(answer) = answer {
        request = request.answer(KEY, format!("\"{answer}\""));
    }
    compile(&request).unwrap()
}

fn says(out: &CompileOutcome, text: &str) -> bool {
    out.diagnostics.iter().any(|d| d.message.contains(text))
}

#[test]
fn a_renamed_column_that_now_spells_the_word_is_asked_again_never_taken_silently() {
    let first =
        compile(&CompileRequest::create(INTENT).with_knowledge(inventory(BEFORE, "r1"))).unwrap();
    assert!(first.questions.iter().any(|q| q.key == KEY), "{first:#?}");
    let recorded = first.provenance.plan.clone().unwrap();

    // The answer given for `units`, arriving after the rename: stale, asked again over the fresh
    // keys, never « no current question owns this answer ».
    let stale = round(&recorded, inventory(RENAMED, "r2"), Some("units"));
    assert_ne!(stale.status, CompileStatus::Ready, "{stale:#?}");
    assert!(stale.candidate.is_none(), "{stale:#?}");
    assert!(
        says(&stale, "changed since this question was asked"),
        "{stale:#?}"
    );
    assert!(
        !says(&stale, "No current question owns this answer"),
        "{stale:#?}"
    );
    let asked = stale
        .questions
        .iter()
        .find(|q| q.key == KEY)
        .expect("asked again");
    assert!(asked.mandatory, "{asked:#?}");
    let offered: Vec<&str> = asked.options.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(offered, RENAMED, "the fresh keys are offered: {asked:#?}");
    // The plan is re-anchored to the observation the question showed, the key asked again noted.
    let reanchored = stale.provenance.plan.clone().unwrap();
    assert_eq!(reanchored["observed_world"], inventory(RENAMED, "r2"));
    assert_eq!(reanchored["reasked"], json!([KEY]));

    // A fresh explicit answer on the re-anchored plan binds; the old spelling does not.
    let current = round(&reanchored, inventory(RENAMED, "r2"), Some("quantity"));
    assert_eq!(current.status, CompileStatus::Ready, "{current:#?}");
    let decision = current.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["grounding"][0]["field"], "quantity",
        "{decision:#}"
    );
    assert_eq!(
        decision["grounding"][0]["bound_by"], "answer",
        "{decision:#}"
    );
    let old = round(&reanchored, inventory(RENAMED, "r2"), Some("units"));
    assert_ne!(old.status, CompileStatus::Ready, "{old:#?}");
    assert!(says(&old, "Choose an observed field"), "{old:#?}");
    // The answer the human gives on the plan it was asked over still binds after the rename is
    // re-read again: another rename re-asks, never a READY on the stale mapping.
    let again = round(&reanchored, inventory(BEFORE, "r3"), Some("quantity"));
    assert_ne!(again.status, CompileStatus::Ready, "{again:#?}");
    assert!(
        says(&again, "changed since this question was asked"),
        "{again:#?}"
    );
}

#[test]
fn an_answer_no_question_asked_stays_unowned() {
    // The word is a declared key from the first round: no question was ever asked, so an answer
    // for its key is not applied, before and after this change.
    let first =
        compile(&CompileRequest::create(INTENT).with_knowledge(inventory(RENAMED, "r1"))).unwrap();
    assert_eq!(first.status, CompileStatus::Ready, "{first:#?}");
    let recorded = first.provenance.plan.clone().unwrap();
    let stray = round(&recorded, inventory(RENAMED, "r1"), Some("sku"));
    assert_ne!(stray.status, CompileStatus::Ready, "{stray:#?}");
    assert!(
        says(&stray, "No current question owns this answer"),
        "{stray:#?}"
    );
    let decision = stray.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["grounding"][0]["field"], "quantity",
        "{decision:#}"
    );
    assert!(recorded.get("reasked").is_none(), "{recorded:#}");
}
