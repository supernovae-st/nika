// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Every span names the exact source text, after any block scalar. The
//! documents are prompts as authors write them: accented prose, a middle
//! dot, CJK, a four-byte emoji, inside `|` and `>` blocks. A key span
//! covers its key; a value or diagnostic span may be a point at its start.

use marked_yaml::Marker;

use super::CharToByte;
use crate::error::SchemaError;
use crate::parser::{ParseMode, parse};
use crate::raw::{RawAction, RawWorkflow};
use crate::source::{FileId, Span};

fn parsed(src: &str) -> RawWorkflow {
    parse(src, FileId::new(0), ParseMode::Strict).expect("fixture parses")
}

fn slice(src: &str, span: Span) -> &str {
    src.get(span.start.as_usize()..span.end.as_usize())
        .expect("a span lies on char boundaries inside the source")
}

/// Whether `span` names `text`: exactly when it has a length, by its start
/// when it is a point.
fn names(src: &str, span: Span, text: &str) -> bool {
    if span.is_empty() {
        src.get(span.start.as_usize()..)
            .is_some_and(|rest| rest.starts_with(text))
    } else {
        slice(src, span) == text
    }
}

fn task_ids(src: &str) -> Vec<&str> {
    parsed(src)
        .tasks
        .iter()
        .map(|task| slice(src, task.value.id.span))
        .collect()
}

fn prompt_span(wf: &RawWorkflow, task: usize) -> Span {
    match &wf.tasks[task].value.action {
        RawAction::Infer(infer) => infer.prompt.span,
        other => panic!("fixture task {task} is an infer, got {other:?}"),
    }
}

const LITERAL: &str = "nika: t\ntasks:\n  a:\n    infer:\n      prompt: |\n        Résumé · naïve — \
                       ünïcödé\n        encore é\n  b:\n    infer:\n      prompt: x\n";

#[test]
fn a_non_ascii_literal_block_keeps_every_later_span_exact() {
    assert_eq!(task_ids(LITERAL), ["a", "b"]);
    let span = prompt_span(&parsed(LITERAL), 1);
    assert!(names(LITERAL, span, "x"), "{span:?}");
}

#[test]
fn an_unknown_key_after_a_non_ascii_block_is_spanned_on_itself() {
    let src = LITERAL.replace("prompt: x\n", "prompt: x\n      bogus: 1\n");
    let err = parse(&src, FileId::new(0), ParseMode::Strict).expect_err("unknown field");
    let SchemaError::UnknownField {
        span: Some(span), ..
    } = err
    else {
        panic!("expected a spanned unknown field, got {err:?}");
    };
    assert!(names(&src, span, "bogus"), "{span:?}");
}

#[test]
fn a_folded_block_under_crlf_breaks_keeps_later_spans_exact() {
    let src = LITERAL
        .replace("prompt: |", "prompt: >")
        .replace('\n', "\r\n");
    assert_eq!(task_ids(&src), ["a", "b"]);
    let span = prompt_span(&parsed(&src), 1);
    assert!(names(&src, span, "x"), "{span:?}");
}

#[test]
fn several_non_ascii_blocks_never_accumulate_drift() {
    let src = "nika: t\ntasks:\n  a:\n    infer:\n      prompt: |+\n        日本語のテキスト\n\n  \
               b:\n    infer:\n      prompt: >-\n        emoji 🦋 four bytes\n  c:\n    \
               infer:\n      prompt: plain é\n  d:\n    infer:\n      prompt: x\n";
    assert_eq!(task_ids(src), ["a", "b", "c", "d"]);
    let wf = parsed(src);
    assert!(names(src, prompt_span(&wf, 2), "plain é"));
    assert!(names(src, prompt_span(&wf, 3), "x"));
}

#[test]
fn non_ascii_outside_any_block_scalar_stays_exact() {
    let src = "nika: t\ntasks:\n  a:\n    infer:\n      prompt: \"Résumé · ü\"\n  b:\n    \
               infer:\n      prompt: x\n";
    assert_eq!(task_ids(src), ["a", "b"]);
    assert!(names(src, prompt_span(&parsed(src), 0), "\"Résumé · ü\""));
}

#[test]
fn an_ascii_document_keeps_its_spans() {
    let src = LITERAL
        .replace("Résumé · naïve — ünïcödé", "Resume - naive")
        .replace("encore é", "again");
    assert_eq!(task_ids(&src), ["a", "b"]);
    assert!(names(&src, prompt_span(&parsed(&src), 1), "x"));
}

#[test]
fn a_non_ascii_block_closing_the_file_stays_in_bounds() {
    let src = "nika: t\ntasks:\n  a:\n    infer:\n      prompt: |\n        fin · é";
    let span = prompt_span(&parsed(src), 0);
    assert!(span.end.as_usize() <= src.len(), "{span:?}");
    let key_end = src.find("prompt:").map(|at| at + "prompt:".len());
    let content = src.find("fin");
    assert!(
        key_end <= Some(span.start.as_usize()) && Some(span.start.as_usize()) <= content,
        "the block starts between its key and its content: {span:?}"
    );
    assert!(slice(src, span).len() <= src.len() - span.start.as_usize());
}

/// The table itself, with deliberately wrong indices: only the line and
/// the column decide, at each of yaml-rust2's three breaks.
#[test]
fn a_marker_is_read_by_line_and_column_never_by_its_index() {
    // bytes: é 0..2 · CR 2 · LF 3 · b 4 · CR 5 · c 6 · LF 7 · d 8 · end 9
    let table = CharToByte::new("é\r\nb\rc\nd").expect("table");
    assert_eq!(table.marker(&Marker::new(0, 99, 2, 1)), 4, "b after a CRLF");
    assert_eq!(
        table.marker(&Marker::new(0, 99, 3, 1)),
        6,
        "c after a lone CR"
    );
    assert_eq!(table.marker(&Marker::new(0, 99, 4, 1)), 8, "d after a LF");
    assert_eq!(
        table.marker(&Marker::new(0, 0, 1, 2)),
        2,
        "line 1 ends at its break"
    );
    assert_eq!(
        table.marker(&Marker::new(0, 0, 1, 40)),
        2,
        "a long column clamps"
    );
    assert_eq!(
        table.marker(&Marker::new(0, 0, 9, 1)),
        9,
        "past the last line is EOF"
    );
}

#[test]
fn an_ascii_source_keeps_the_exact_index() {
    let table = CharToByte::new("a\nbc").expect("table");
    assert_eq!(table.marker(&Marker::new(0, 3, 2, 2)), 3);
}
