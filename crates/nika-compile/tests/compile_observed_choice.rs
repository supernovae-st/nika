// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An observed world states names; it does not answer a choice the request leaves open. The
//! campaign's DIALOG-03 (« Additionne une colonne de ventes.csv dans total.txt. » over
//! `montant, autre`) could only end in a silent pick: the judge refused every column question
//! whenever a world was observed. A seat's question for a column the request leaves open is
//! now admitted as a closed choice among that file's observed columns, verbatim — the only
//! answers the replay bakes — while a column the request names stays stated, never asked.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
    QuestionType,
};
use nika_compile_cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_provider,
};
use serde_json::{Value, json};
use std::time::Duration;

mod common;
use common::{Judged, Rotating, held_for_its_judge, keys};

const OPEN: &str = "Additionne une colonne de ventes.csv dans total.txt.";

fn world() -> Value {
    json!({"observed": [{"path": "ventes.csv", "kind": "csv", "delimiter": ",", "columns": ["montant", "autre"]}]})
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(1)
}

/// The campaign's request as the sketch door's graph: read, parse, sum one column, write; the
/// column is the program's `column` (a placeholder the human answers, or written).
fn sketch(questions: &Value, column: &str) -> Vec<String> {
    let edge = |name: &str, from: &str| json!([{"name": name, "from": from}]);
    let graph = json!({"name": "sum-column-to-total", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "reads": ["ventes.csv"], "purpose": "the sales"},
        {"id": "parse_source", "verb": "invoke", "tool": "nika:convert", "with": edge("document", "read_source"), "purpose": "parse"},
        {"id": "compute_total", "verb": "invoke", "tool": "nika:jq", "with": edge("records", "parse_source"), "purpose": "sum the column"},
        {"id": "write_total", "verb": "invoke", "tool": "nika:write", "writes": ["total.txt"], "with": edge("content", "compute_total"), "purpose": "the total"}
    ], "outputs": [{"name": "total", "from": "compute_total"}], "questions": questions, "gaps": [],
       "notes": "the column"});
    let fills = json!({"fills": [
        {"task": "parse_source", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "compute_total", "field": "expression",
         "value": format!("([.[][\"{column}\"] | tonumber] | add // 0) as $total | ($total | tostring) + \"\\n\"")}
    ], "notes": "two holes"});
    vec![graph.to_string(), fills.to_string()]
}

/// A keyless answer round of a semantic record: the cognition entry with no seat.
async fn replayed(request: &CompileRequest) -> CompileOutcome {
    compile_with_cognition(request, Cognition::<NoProvider>::default())
        .await
        .unwrap()
}

fn asking() -> Vec<String> {
    let question = json!([{"key": "const.sum_column", "label": "Quelle colonne additionner ?", "answer_type": "text", "why": "La demande ne dit pas quelle colonne."}]);
    sketch(&question, "${{ const.sum_column }}")
}

fn writing(column: &str) -> Vec<String> {
    sketch(&json!([]), column)
}

#[tokio::test]
async fn an_open_column_is_asked_among_the_observed_columns_and_only_they_are_answers() {
    let provider = Rotating::new(asking());
    let request = CompileRequest::create(OPEN)
        .with_knowledge(world())
        .with_authoring_policy(policy());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let question = out
        .questions
        .iter()
        .find(|q| q.key == "const.sum_column")
        .expect("the column is asked");
    assert!(question.mandatory);
    assert_eq!(question.answer_type, QuestionType::Choice);
    assert_eq!(
        question
            .options
            .iter()
            .map(|o| o.key.as_str())
            .collect::<Vec<_>>(),
        ["montant", "autre"],
        "the observed columns, verbatim, never a name the label proposes"
    );
    assert!(
        question.why.ends_with("Answer one of: montant · autre."),
        "a transport that shows no options still shows the answers: {}",
        question.why
    );
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(
        record["settlement"]["questions"][0]["answer_type"], "choice",
        "{record:#}"
    );
    // The answer round replays the record: an offered key is baked, then
    // held for the round's judge (R4 A11, step 2): this keyless round permits none.
    let answered = replayed(
        &CompileRequest::create(OPEN)
            .with_knowledge(world())
            .with_authoring_policy(policy())
            .with_plan(record.clone())
            .answer("const.sum_column", r#""montant""#),
    )
    .await;
    assert!(held_for_its_judge(&answered, OPEN), "{answered:#?}");
    assert!(
        answered
            .candidate
            .as_deref()
            .unwrap()
            .contains("sum_column: \"montant\""),
        "{answered:#?}"
    );
    // Anything else is a finding, and the choice stays asked.
    let wrong = replayed(
        &CompileRequest::create(OPEN)
            .with_knowledge(world())
            .with_authoring_policy(policy())
            .with_plan(record)
            .answer("const.sum_column", r#""total""#),
    )
    .await;
    assert_ne!(wrong.status, CompileStatus::Ready, "{wrong:#?}");
    assert!(keys(&wrong).contains(&"const.sum_column"), "{wrong:#?}");
    assert!(
        wrong
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Missed
                && d.target == "const.sum_column"
                && d.message.contains("montant · autre")),
        "{wrong:#?}"
    );
}

#[tokio::test]
async fn a_column_the_request_names_is_stated_and_never_asked() {
    // Round 0 sketches a question for the column the request names: refused before the graph is
    // fixed; the sketch repair writes it; its fills follow: READY, the column never asked.
    let named = "Additionne la colonne montant de ventes.csv dans total.txt.";
    let replies = vec![
        asking()[0].clone(),
        writing("montant")[0].clone(),
        writing("montant")[1].clone(),
    ];
    let provider = Rotating::new(replies);
    let request = CompileRequest::create(named)
        .with_knowledge(world())
        .with_authoring_policy(policy());
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let rounds = out.provenance.decision.as_ref().unwrap()["native"]["rounds"].clone();
    assert!(
        rounds[0]["diagnostics"]
            .to_string()
            .contains("the observed world states them"),
        "{rounds:#}"
    );
    let phases: Vec<&str> = (rounds.as_array().unwrap().iter())
        .map(|r| r["phase"].as_str().unwrap())
        .collect();
    assert_eq!(phases, ["sketch", "sketch", "fill"], "{rounds:#}");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let roles: Vec<&str> = (receipt.context.iter())
        .map(|c| c["call"].as_str().unwrap())
        .collect();
    assert_eq!(
        roles,
        ["sketch", "sketch-repair", "fill", "judge_request"],
        "{receipt:#?}"
    );
    assert!(!keys(&out).contains(&"const.sum_column"), "{out:#?}");
    assert!(
        out.candidate.as_deref().unwrap().contains("montant"),
        "{out:#?}"
    );
    // With no repair allowed, the refused question ends the round: no fill, never asked.
    let provider = Rotating::new(asking());
    let request = CompileRequest::create(named)
        .with_knowledge(world())
        .with_authoring_policy(policy().with_repairs(0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(
        provider.calls.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "{out:#?}"
    );
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(!keys(&out).contains(&"const.sum_column"), "{out:#?}");
}

/// The semantic record stays closed: compiled under an observed world, it carries no copy of it,
/// and its answer round needs the same world from the caller. An absent or another world, or a
/// record a decoration reopened, refuses with nothing emitted and no model asked.
#[tokio::test]
async fn a_semantic_record_stays_closed_and_replays_only_under_its_own_world() {
    let provider = Rotating::new(asking());
    let request = CompileRequest::create(OPEN)
        .with_knowledge(world())
        .with_authoring_policy(policy());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    let record = out.provenance.plan.clone().unwrap();
    assert!(record.get("semantic_record").is_some(), "{record:#}");
    for key in ["observed_world", "reasked", "verified_transform"] {
        assert!(
            record.get(key).is_none(),
            "{key} decorates a closed record: {record:#}"
        );
    }
    let mut other = world();
    other["observed"][0]["columns"] = json!(["montant", "autre", "remise"]);
    let mut reopened = record.clone();
    reopened["observed_world"] = world();
    let cases = [
        ("no world", None, record.clone()),
        ("another world", Some(other), record.clone()),
        ("a reopened record", Some(world()), reopened),
    ];
    for (case, knowledge, plan) in cases {
        let mut replay = CompileRequest::create(OPEN)
            .with_authoring_policy(policy())
            .with_plan(plan)
            .answer("const.sum_column", r#""montant""#);
        if let Some(knowledge) = knowledge {
            replay = replay.with_knowledge(knowledge);
        }
        let out = replayed(&replay).await;
        assert!(out.candidate.is_none(), "{case}: {out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "recorded_plan" && d.message.contains("cannot be replayed")),
            "{case}: {out:#?}"
        );
    }
}
