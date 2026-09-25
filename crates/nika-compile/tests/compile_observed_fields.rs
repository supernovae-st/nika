// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, QuestionType, compile};
use serde_json::json;

const INTENT: &str = "Lis ./orders.json, garde seulement les commandes dont le statut est delivered et écris le résultat dans ./delivered.json";

#[test]
fn hot_status_mismatch_asks_and_replay_accepts_only_an_observed_key() {
    let request = CompileRequest::create(INTENT).with_knowledge(json!({"observed":[{
        "path":"./orders.json", "kind":"json", "state":"observed",
        "columns":["id", "status"], "common_columns":["id", "status"], "complete":false
    }]}));
    let out = compile(&request).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let question = out
        .questions
        .iter()
        .find(|q| q.key == "const.rule_field_1")
        .expect("field question");
    assert_eq!(question.answer_type, QuestionType::Choice);
    assert_eq!(
        question
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["id", "status"]
    );
    let record = out.provenance.plan.unwrap();
    let wrong = compile(
        &CompileRequest::create(INTENT)
            .with_plan(record.clone())
            .answer("const.rule_field_1", "\"statut\""),
    )
    .unwrap();
    assert_ne!(wrong.status, CompileStatus::Ready, "{wrong:#?}");
    let answered = compile(
        &CompileRequest::create(INTENT)
            .with_plan(record)
            .answer("const.rule_field_1", "\"status\""),
    )
    .unwrap();
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
    assert!(answered.candidate.unwrap().contains("status"));
}
