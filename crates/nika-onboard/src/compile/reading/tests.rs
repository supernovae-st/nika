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
