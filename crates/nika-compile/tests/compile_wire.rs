// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Generation 1 of the Compile machine document, judged on the shared parity fixture.
//!
//! The same `fixtures/compile_parity_v1.json` drives the CLI door
//! (`nika-cli/tests/compile_cli.rs`) and the Serve door (`nika-serve` compile tests):
//! every door must print THIS document for THIS request. Hermetic: no file, credential,
//! model or workflow is touched.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use nika_compile::{
    COMPILE_WIRE_VERSION, CompileRequest, CompileStatus, DiagnosticKind, compile, outcome_document,
};
use serde_json::Value;

const FIXTURE: &str = include_str!("fixtures/compile_parity_v1.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("parity fixture is JSON")
}

fn source<'a>(fixture: &'a Value, case: &Value) -> &'a str {
    let name = case["native"]["source_ref"].as_str().expect("source_ref");
    fixture["sources"][name].as_str().expect("named source")
}

/// The native recipe, built with the public builders only (no transport code).
fn native_request(fixture: &Value, case: &Value) -> CompileRequest {
    let native = &case["native"];
    let text = |key: &str| native[key].as_str().expect("native text field");
    let mut request = match text("mode") {
        "create" => CompileRequest::create(text("intent")),
        "edit" => CompileRequest::edit(source(fixture, case), text("change_text")),
        "set_constant" => {
            CompileRequest::set_constant(source(fixture, case), text("name"), text("literal"))
        }
        other => panic!("unknown native mode {other}"),
    };
    if let Some(id) = native["workflow_id"].as_str() {
        request = request.with_workflow_id(id);
    }
    for pair in native["answers"].as_array().into_iter().flatten() {
        request = request.answer(
            pair[0].as_str().expect("answer key"),
            pair[1].as_str().expect("answer literal text"),
        );
    }
    request
}

fn documents() -> BTreeMap<String, (Value, Value)> {
    let fixture = fixture();
    fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| {
            let name = case["name"].as_str().expect("case name").to_owned();
            let outcome = compile(&native_request(&fixture, case)).expect("compile machinery");
            (name, (case.clone(), outcome_document(&outcome)))
        })
        .collect()
}

#[test]
fn the_fixture_names_every_door_and_each_case_once() {
    let fixture = fixture();
    assert_eq!(fixture["compile_version"], COMPILE_WIRE_VERSION);
    let cases = fixture["cases"].as_array().expect("cases");
    let names: std::collections::BTreeSet<_> =
        cases.iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), cases.len(), "case names are unique");
    for case in cases {
        let doors = case["doors"].as_array().unwrap();
        for door in ["core", "serve"] {
            assert!(doors.iter().any(|named| named == door), "{case}");
        }
    }
}

#[test]
fn every_document_has_exactly_the_generation_one_shape() {
    for (name, (_, document)) in documents() {
        let object = document.as_object().expect("document object");
        let keys: Vec<_> = object.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "candidate",
                "check_preview",
                "compile_version",
                "diagnostics",
                "provenance",
                "questions",
                "requested_boundary",
                "status"
            ],
            "{name}: the core document carries no transport fact such as `written`"
        );
        assert_eq!(document["compile_version"], COMPILE_WIRE_VERSION, "{name}");
        let status = document["status"].as_str().expect("status word");
        assert!(
            matches!(status, "ready" | "incomplete" | "refused"),
            "{name}: {status}"
        );
        assert!(
            document["candidate"].is_string() || document["candidate"].is_null(),
            "{name}"
        );
        for question in document["questions"].as_array().expect("questions") {
            let question = question.as_object().expect("question object");
            let keys: Vec<_> = question.keys().map(String::as_str).collect();
            assert_eq!(keys, ["key", "label", "mandatory", "type", "why"], "{name}");
            assert!(
                matches!(question["type"].as_str(), Some("text" | "literal")),
                "{name}"
            );
            assert!(question["mandatory"].is_boolean(), "{name}");
        }
        for diagnostic in document["diagnostics"].as_array().expect("diagnostics") {
            let diagnostic = diagnostic.as_object().expect("diagnostic object");
            let keys: Vec<_> = diagnostic.keys().map(String::as_str).collect();
            assert_eq!(keys, ["kind", "message", "target"], "{name}");
            assert!(
                matches!(
                    diagnostic["kind"].as_str(),
                    Some("applied" | "missed" | "unknown" | "requiresHuman" | "refused")
                ),
                "{name}: {diagnostic:?}"
            );
        }
        let provenance = document["provenance"].as_object().expect("provenance");
        // `strategy` is the additive observational fact of the internal resolution
        // (skeleton · support · hot · warm · cold); `plan`/`decision` appear only on
        // the general path. Their absence or presence never changes the core shape.
        let keys: Vec<_> = provenance
            .keys()
            .map(String::as_str)
            .filter(|k| !matches!(*k, "strategy" | "plan" | "decision"))
            .collect();
        assert_eq!(
            keys,
            ["cognition", "compiler_version", "skeleton", "spec_pin"],
            "{name}"
        );
        if let Some(strategy) = provenance.get("strategy") {
            assert!(
                matches!(
                    strategy.as_str(),
                    Some("skeleton" | "support" | "hot" | "warm" | "cold")
                ),
                "{name}: {strategy}"
            );
        }
        assert_eq!(provenance["cognition"], "deterministicOnly", "{name}");
        assert_eq!(
            provenance["compiler_version"],
            env!("CARGO_PKG_VERSION"),
            "{name}"
        );
        // A preview and a requested boundary exist together or not at all, and the
        // preview never claims more than the source it judged.
        assert_eq!(
            document["check_preview"].is_null(),
            document["requested_boundary"].is_null(),
            "{name}"
        );
        if let Some(preview) = document["check_preview"].as_object() {
            assert_eq!(preview["scope"], "sourceOnly", "{name}");
            assert!(preview["report"].is_object(), "{name}");
        }
    }
}

#[test]
fn the_fixture_expectations_hold_on_the_core() {
    let fixture = fixture();
    let documents = documents();
    for (name, (case, document)) in &documents {
        let expect = &case["expect"];
        if let Some(status) = expect["status"].as_str() {
            assert_eq!(document["status"], status, "{name}");
        }
        let candidate = document["candidate"].as_str();
        match expect["candidate"].as_str() {
            Some("present") => assert!(candidate.is_some(), "{name}"),
            Some("absent") => assert!(candidate.is_none(), "{name}"),
            Some("source") => assert_eq!(candidate, Some(source(&fixture, case)), "{name}"),
            Some("changed") => {
                assert!(
                    candidate.is_some_and(|c| c != source(&fixture, case)),
                    "{name}"
                );
            }
            Some(other) => panic!("{name}: unknown candidate expectation {other}"),
            None => {}
        }
        if let Some(skeleton) = expect.get("skeleton") {
            assert_eq!(&document["provenance"]["skeleton"], skeleton, "{name}");
        }
        if let Some(keys) = expect["question_keys"].as_array() {
            let asked: Vec<_> = document["questions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|q| q["key"].clone())
                .collect();
            assert_eq!(&asked, keys, "{name}");
        }
        for wanted in expect["diagnostics"].as_array().into_iter().flatten() {
            assert!(
                document["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| { d["kind"] == wanted[0] && d["target"] == wanted[1] }),
                "{name}: missing {wanted} in {}",
                document["diagnostics"]
            );
        }
        match expect["preview"].as_str() {
            Some("present") => assert!(document["check_preview"].is_object(), "{name}"),
            Some("absent") => assert!(document["check_preview"].is_null(), "{name}"),
            _ => {}
        }
        for word in expect["boundary_mentions"].as_array().into_iter().flatten() {
            let boundary = document["requested_boundary"].to_string();
            assert!(
                boundary.contains(word.as_str().unwrap()),
                "{name}: {boundary}"
            );
        }
        if let Some(other) = expect["same_candidate_as"].as_str() {
            assert_eq!(
                document["candidate"], documents[other].1["candidate"],
                "{name}"
            );
        }
    }
}

#[test]
fn status_and_disposition_words_are_the_documents_words() {
    assert_eq!(CompileStatus::Ready.word(), "ready");
    assert_eq!(CompileStatus::Incomplete.word(), "incomplete");
    assert_eq!(CompileStatus::Refused.word(), "refused");
    for (kind, word) in [
        (DiagnosticKind::Applied, "applied"),
        (DiagnosticKind::Missed, "missed"),
        (DiagnosticKind::Unknown, "unknown"),
        (DiagnosticKind::RequiresHuman, "requiresHuman"),
        (DiagnosticKind::Refused, "refused"),
    ] {
        assert_eq!(kind.word(), word);
    }
}

#[test]
fn rendering_is_byte_stable_with_sorted_keys() {
    // The CLI printed this document before the projection moved here; its bytes stay
    // identical only while maps serialize sorted (`serde_json/preserve_order` off).
    let outcome = compile(&CompileRequest::create("hello")).expect("compile");
    let first = outcome_document(&outcome).to_string();
    let second = outcome_document(&outcome).to_string();
    assert_eq!(first, second);
    let head: String = first.chars().take(60).collect();
    assert!(first.starts_with(r#"{"candidate":"#), "{head}");
    assert!(first.contains(r#""compile_version":1"#));
}
