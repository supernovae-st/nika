// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::compile::{CompileRequest, compile};

fn read(intent: &str) -> Reading {
    Reading::of(compile(&CompileRequest::create(intent)).expect("compiles"))
}

#[test]
fn the_reading_is_the_compilers_typed_fields() {
    assert!(matches!(
        read("hello there, how are you today?"),
        Reading::NotWork(_)
    ));
    assert!(matches!(
        read("Read ./notes/brief.md and write it to ./out/copy.md"),
        Reading::Ready(_)
    ));
    match read(
        "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md",
    ) {
        Reading::Questions(out) => {
            assert_eq!(out.questions[0].key, "model");
            assert!(
                out.provenance.plan.is_some(),
                "the HOT plan is recorded for replay"
            );
        }
        other => panic!("a draft needs its model, asked: {other:?}"),
    }
    assert!(matches!(
        read("Read ./a.md and do something clever with it, then write ./b.md"),
        Reading::Unsettled(_)
    ));
    match read("chain") {
        Reading::Questions(out) => assert_eq!(out.questions[0].key, "tasks.think.infer.prompt"),
        other => panic!("a skeleton with holes asks: {other:?}"),
    }
}

/// The compiler's own questions of each shape: a skeleton's prompt
/// hole is text, a skeleton's constant hole is a literal.
fn question_of(skeleton: &str, shape: QuestionType) -> CompileQuestion {
    let out = compile(&CompileRequest::create(skeleton)).expect("compiles");
    let question = out.questions.into_iter().next().expect("one hole");
    assert_eq!(question.answer_type, shape, "{skeleton}");
    question
}

#[test]
fn a_line_is_typed_to_the_questions_shape() {
    let text = question_of("chain", QuestionType::Text);
    let literal = question_of("bounded-batch", QuestionType::Literal);
    assert_eq!(literal_for(&text, "mock/echo"), "\"mock/echo\"");
    assert_eq!(literal_for(&text, " 5 "), "\"5\"");
    assert_eq!(literal_for(&literal, "5"), "5");
    assert_eq!(literal_for(&literal, "true"), "true");
    assert_eq!(literal_for(&literal, "./notes"), "\"./notes\"");
    assert_eq!(literal_for(&literal, "[\"a\"]"), "[\"a\"]");
}

#[test]
fn the_reasons_are_the_compilers_own_findings() {
    let unsettled = compile(&CompileRequest::create(
        "Read ./a.md and do something clever with it, then write ./b.md",
    ))
    .expect("compiles");
    let said = reasons(&unsettled);
    let findings: Vec<String> = unsettled
        .diagnostics
        .iter()
        .filter(|d| {
            matches!(
                d.kind,
                DiagnosticKind::Unknown | DiagnosticKind::Missed | DiagnosticKind::Refused
            )
        })
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(
        said, findings,
        "exactly the unknown · missed · refused ones"
    );
    let ready = compile(&CompileRequest::create(
        "Read ./notes/brief.md and write it to ./out/copy.md",
    ))
    .expect("compiles");
    assert!(
        reasons(&ready).is_empty(),
        "a Ready candidate states no reason"
    );
}

/// C10 · D-R · an answered `intent.clarification` replaces the request only with words: a
/// string that is not blank, never another literal.
#[test]
fn a_clarification_gives_words_only_when_it_is_a_string_that_is_not_blank() {
    let one = |literal: &str| {
        std::collections::BTreeMap::from([(CLARIFICATION_KEY.to_owned(), literal.to_owned())])
    };
    assert_eq!(
        clarified(&one("\"the whole request\"")).as_deref(),
        Some("the whole request")
    );
    for literal in ["\"  \"", "42", "not json", "null"] {
        assert_eq!(clarified(&one(literal)), None, "{literal}");
    }
    assert_eq!(clarified(&std::collections::BTreeMap::new()), None);
}

/// C10 · D-H · the compiler's fidelity grammar in a human's words, beside the reading it
/// refines: machine sentences dropped, duplicates folded, each closed form said plainly, a cut
/// answer named as an internal limit, anything else as the compiler said it.
#[test]
fn the_compilers_reasons_read_in_a_humans_words() {
    let said = human_reasons(
        [
            "the semantic plan has no step",
            "unmapped part: .",
            "Candidate 2 is not feasible: dropped the recognized operation `sum` (the total of amount)",
            "the path `./out.csv` is no longer carried by the candidate",
            "the literal `42` is not in the request",
            "the answer was cut: raise --authoring-max-tokens above 16384",
            "a reason as the compiler said it.",
            "a reason as the compiler said it",
        ]
        .map(str::to_owned)
        .to_vec(),
    );
    assert_eq!(
        said,
        [
            "the draft lost « the total of amount » (the sum step)",
            "the draft dropped « ./out.csv »: nothing reads or writes it any more",
            "the draft invented a value (« 42 ») your request never gave",
            "the model's answer was cut at its 16384-token output limit before it was complete — an internal limit of this attempt, not a problem with your request",
            "a reason as the compiler said it",
        ]
    );
}

/// C10 · D-H · a question's own grammar: the clause it quotes, whether it asks for code, and how
/// many clauses the outcome's ledger holds.
#[test]
fn a_question_quotes_its_clause_and_says_when_it_asks_for_code() {
    assert_eq!(
        clause_of("How should `the total of amount` be computed?").as_deref(),
        Some("the total of amount")
    );
    assert_eq!(clause_of("no quoted clause"), None);
    assert_eq!(clause_of("an empty `  ` clause"), None);
    let mut out = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    let mut question = out.questions[0].clone();
    assert!(!asks_for_syntax(&question), "{}", question.label);
    question.key = "const.rule_expression".to_owned();
    assert!(asks_for_syntax(&question), "a const expression is code");
    question.key = "value".to_owned();
    question.label = "Which jq expression keeps the rows?".to_owned();
    assert!(asks_for_syntax(&question), "a jq expression is code");
    out.provenance.decision = Some(serde_json::json!({"ledger": [{}, {}]}));
    assert_eq!(clauses_understood(&out), Some(2));
    out.provenance.decision = Some(serde_json::json!({"ledger": []}));
    assert_eq!(
        clauses_understood(&out),
        None,
        "an empty ledger says nothing"
    );
}
