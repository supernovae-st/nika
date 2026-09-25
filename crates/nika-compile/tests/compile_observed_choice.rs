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
    AuthoringPolicy, CompileRequest, CompileStatus, DiagnosticKind, NativeMode, QuestionType,
    compile,
};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::time::Duration;

mod common;
use common::{Rotating, keys};

const OPEN: &str = "Additionne une colonne de ventes.csv dans total.txt.";

fn world() -> Value {
    json!({"observed": [{"path": "ventes.csv", "kind": "csv", "delimiter": ",", "columns": ["montant", "autre"]}]})
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Only)
        .with_repairs(1)
}

/// The campaign's candidate with its column as a placeholder the human answers, or written.
fn candidate(column: &str) -> String {
    format!(
        r#"nika: sum-column-to-total
const:
  source_path: ventes.csv
  output_path: total.txt
  sum_column: "{column}"
permits:
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
  fs:
    read: ["ventes.csv"]
    write: ["total.txt"]
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args:
        path: "${{{{ const.source_path }}}}"
  parse_source:
    with:
      document: "${{{{ tasks.read_source.output }}}}"
    invoke:
      tool: "nika:convert"
      args:
        input: "${{{{ with.document }}}}"
        from: csv
        to: json
  compute_total:
    with:
      records: "${{{{ tasks.parse_source.output }}}}"
    invoke:
      tool: "nika:jq"
      args:
        input:
          records: "${{{{ with.records }}}}"
          column: "${{{{ const.sum_column }}}}"
        expression: '.column as $c | ([.records[][$c] | tonumber] | add // 0) as $total | ($total | tostring) + "\n"'
  write_total:
    with:
      content: "${{{{ tasks.compute_total.output }}}}"
    invoke:
      tool: "nika:write"
      args:
        path: "${{{{ const.output_path }}}}"
        content: "${{{{ with.content }}}}"
        overwrite: true
        create_dirs: true
outputs:
  total: "${{{{ tasks.compute_total.output }}}}"
"#
    )
}

fn asking() -> String {
    json!({"candidate": candidate(""), "questions": [{"key": "const.sum_column", "label": "Quelle colonne additionner ?", "answer_type": "text", "why": "La demande ne dit pas quelle colonne."}], "gaps": [], "notes": "the column is the human's"}).to_string()
}

fn writing(column: &str) -> String {
    json!({"candidate": candidate(column), "questions": [], "gaps": [], "notes": "the column is stated"}).to_string()
}

#[tokio::test]
async fn an_open_column_is_asked_among_the_observed_columns_and_only_they_are_answers() {
    let provider = Rotating::new(vec![asking()]);
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
        record["questions"][0]["answer_type"], "choice",
        "{record:#}"
    );
    // The answer round replays the record: an offered key is baked, and READY.
    let answered = compile(
        &CompileRequest::create(OPEN)
            .with_plan(record.clone())
            .answer("const.sum_column", r#""montant""#),
    )
    .unwrap();
    assert_eq!(answered.status, CompileStatus::Ready, "{answered:#?}");
    assert!(
        answered
            .candidate
            .as_deref()
            .unwrap()
            .contains("sum_column: \"montant\""),
        "{answered:#?}"
    );
    // Anything else is a finding, and the choice stays asked.
    let wrong = compile(
        &CompileRequest::create(OPEN)
            .with_plan(record)
            .answer("const.sum_column", r#""total""#),
    )
    .unwrap();
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
    // Round 0 asks for the column the request names: refused; round 1 writes it: READY.
    let provider = Rotating::new(vec![asking(), writing("montant")]);
    let request =
        CompileRequest::create("Additionne la colonne montant de ventes.csv dans total.txt.")
            .with_knowledge(world())
            .with_authoring_policy(policy());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let rounds = out.provenance.decision.as_ref().unwrap()["native"]["rounds"].clone();
    assert!(
        rounds[0]["diagnostics"]
            .to_string()
            .contains("the observed world states them"),
        "{rounds:#}"
    );
    assert!(!keys(&out).contains(&"const.sum_column"), "{out:#?}");
}
