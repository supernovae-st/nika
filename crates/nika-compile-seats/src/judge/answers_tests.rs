// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Answer text is a host storage format, not the structure the judging seat should read.

use super::{Construction, part_question, state};
use crate::decide::system_one;
use nika_compile::CompileRequest;
use serde_json::{Value, json};

/// Two selected checkboxes may bind the same literal while asking for different report
/// sections. Their scoped labels and human citations stay readable throughout the wire.
#[test]
fn selected_meanings_are_structured_in_whole_part_and_wire_questions() {
    let intent = "Prepare a report. Ask me now which sections to include; do not save or run.";
    let selected = |option: &str, label: &str| {
        json!({"key": option, "label": label, "values": [{"role": "value", "value": "true"}],
            "provenance": {"kind": "offered", "message": "u3", "question": "sections", "option": option}})
    };
    let binding = |key: &str, option: &str, value: &str, message: &str| {
        json!({"key": key, "role": "value", "value": value,
            "provenance": {"kind": "offered", "message": message, "question": key, "option": option}})
    };
    let detail = json!({"bound_values": "detailed",
        "bindings": [binding("detail", "detailed", "detailed", "u2")],
        "choice_context": {"question_key": "detail", "question": "Brief or detailed?",
            "human_citation": "u2", "selected_options": [{"key": "detailed", "label": "Detailed",
                "values": [{"role": "value", "value": "detailed"}],
                "provenance": {"kind": "offered", "message": "u2",
                    "question": "detail", "option": "detailed"}}]}});
    let sections = json!({"bound_values": ["true", "true"],
        "bindings": [binding("sections", "revenue", "true", "u3"), binding("sections", "customers", "true", "u3")],
        "choice_context": {"question_key": "sections", "question": "Revenue, customer ranking or status?",
            "human_citation": "u3", "selected_options": [selected("revenue", "Revenue"),
                selected("customers", "Customer ranking")]}});
    let request = CompileRequest::create(intent)
        .answer("detail", detail.to_string())
        .answer("sections", sections.to_string());
    let original = request.answers.clone();
    let shown = state(intent, &request, "nika: report\n");
    let expected = json!({"detail": detail, "sections": sections});
    assert_eq!(shown["answers"], expected);
    assert_eq!(
        request.answers, original,
        "the public storage contract stays intact"
    );
    assert_eq!(shown["request"], intent);
    assert_eq!(shown["candidate_nika"], "nika: report\n");
    assert_eq!(Construction::shown(&shown, "judge").0["answers"], expected);

    let question = part_question(
        0,
        2,
        "Ask me now which sections to include",
        false,
        &shown,
        "reference",
    );
    assert_eq!(question.state["answers"], expected);
    let wire = system_one::request("jev-1.13.0", &[&question]);
    let received = system_one::read_state(&wire, &question.id).unwrap();
    assert_eq!(received["answers"], expected);
    assert_eq!(
        received["answers"]["sections"]["choice_context"]["selected_options"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        received["answers"]["sections"]["choice_context"]["selected_options"][1]["label"],
        "Customer ranking"
    );
}

/// Decoding is not another interpretation: a quoted JSON-looking answer stays a string,
/// primitive types stay distinct, and unavailable literal structure stays verbatim.
#[test]
fn answer_projection_decodes_once_and_keeps_unreadable_text_exact() {
    let quoted_json = r#"{"action":"save & run"}"#;
    let request = CompileRequest::create("Keep these answers as data")
        .answer("text", json!(quoted_json).to_string())
        .answer("integer", "42")
        .answer("digits", "\"42\"")
        .answer("boolean", "false")
        .answer("null", "null")
        .answer("array", "[1,true,null]")
        .answer("invalid", "  not JSON  ")
        .answer("inexact", "[9223372036854775808]");
    assert_eq!(
        state("Keep these answers as data", &request, "")["answers"],
        json!({
            "text": quoted_json, "integer": 42, "digits": "42", "boolean": false,
            "null": null, "array": [1, true, null], "invalid": "  not JSON  ",
            "inexact": "[9223372036854775808]",
        })
    );
    assert_eq!(request.answers["integer"], "42");
    assert_eq!(request.answers["inexact"], "[9223372036854775808]");
}

/// A label-only choice is a meaningful selection without a fabricated business literal.
#[test]
fn selected_options_without_values_stay_evidence_without_fabricated_bindings() {
    let choice = json!({"bound_values": [], "bindings": [], "choice_context": {
        "question_key": "format", "question": "Which report format?", "human_citation": "u2",
        "selected_options": [{"key": "table", "label": "Table", "values": [],
            "provenance": {"kind": "offered", "message": "u2", "question": "format", "option": "table"}}]
    }});
    let request =
        CompileRequest::create("Prepare, do not save or run.").answer("format", choice.to_string());
    let shown = state("Prepare, do not save or run.", &request, "nika: report\n");
    assert_eq!(shown["answers"]["format"], choice);
    assert_eq!(shown["answers"]["format"]["bound_values"], json!([]));
    assert_eq!(shown["answers"]["format"]["bindings"], json!([]));
    assert_eq!(shown.get("authority"), None::<&Value>);
}
