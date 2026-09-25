// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A search's query comes from something that exists when the workflow runs: the text the
//! request's side answers, the invocation's item when the request supplies no material of
//! its own, or the payload an event delivers. A search over the request's own material never
//! declares an input the run could not supply; without its text it asks before the candidate
//! claims it can run. Recorded plans replay through the assembler with zero calls.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::{Value, json};

mod common;
use common::keys;

const FIND: &str = "Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json";

/// The request's own file is read, and the lookup was read as a search whose text the
/// request never states as search text (only as the object of « find »).
fn search_over_the_requests_file() -> Value {
    json!({"bindings": [{"literal": "./ticket-42.json", "role": "path"}, {"literal": "./tickets.json", "role": "path"}],
           "constraints": [],
           "effects": [{"evidence": "write it to ./ticket-42.json", "policy": "automatic", "policy_literal": null, "target": "./ticket-42.json", "verb": "write"}],
           "obligations": [],
           "operations": [{"categories": [], "detail": "./tickets.json", "evidence": "./tickets.json", "op": "read"},
                          {"categories": [], "detail": "ticket 42", "evidence": "ticket 42", "op": "search"}],
           "rules": [], "slots": [], "strategy": "cold", "trigger": null, "unknowns": []})
}

fn find(answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(FIND).with_plan(search_over_the_requests_file());
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile(&request).unwrap()
}

fn document(out: &CompileOutcome) -> Value {
    let source = out.candidate.as_deref().expect("a candidate is assembled");
    serde_yaml_bw::from_str(source).unwrap()
}

#[test]
fn a_search_over_the_requests_own_file_asks_its_text_before_it_can_run() {
    let out = find(&[("const.search_root", r#"".""#)]);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).contains(&"const.search_term"), "{out:#?}");
    let label = &out
        .questions
        .iter()
        .find(|q| q.key == "const.search_term")
        .unwrap()
        .label;
    assert!(
        label.contains("ticket 42"),
        "the question quotes the request: {label}"
    );
    if let Some(source) = out.candidate.as_deref() {
        assert!(
            !source.contains("inputs.item"),
            "no invented runtime input: {source}"
        );
    }
}

#[test]
fn an_answered_search_text_is_baked_in_and_declares_no_runtime_input() {
    let out = find(&[
        ("const.search_root", r#"".""#),
        ("const.search_term", r#""ticket 42""#),
    ]);
    assert!(!keys(&out).contains(&"const.search_term"), "{out:#?}");
    let doc = document(&out);
    assert!(doc["inputs"].get("item").is_none(), "{doc:#}");
    assert_eq!(doc["const"]["search_term"], "ticket 42", "{doc:#}");
    assert_eq!(
        doc["tasks"]["search_hits"]["invoke"]["args"]["pattern"], "${{ const.search_term }}",
        "{doc:#}"
    );
}

#[test]
fn an_empty_search_text_is_refused_and_stays_asked() {
    let out = find(&[
        ("const.search_root", r#"".""#),
        ("const.search_term", r#""  ""#),
    ]);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).contains(&"const.search_term"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "const.search_term" && d.message.contains("nonempty")),
        "{out:#?}"
    );
}

/// A search with no material of its own: the request is a reusable search whose query and
/// material are the invocation's item.
#[test]
fn a_search_without_material_of_its_own_keeps_the_invocation_item() {
    let intent =
        "Retrouve les passages pertinents du guide interne et écris-les dans ./out/passages.md";
    let plan = json!({"bindings": [{"literal": "./out/passages.md", "role": "path"}],
        "constraints": [],
        "effects": [{"evidence": "écris-les dans ./out/passages.md", "policy": "automatic", "policy_literal": null, "target": "./out/passages.md", "verb": "write"}],
        "obligations": [],
        "operations": [{"categories": [], "detail": "les passages pertinents du guide interne", "evidence": "Retrouve les passages pertinents du guide interne", "op": "search"}],
        "rules": [], "slots": [], "strategy": "cold", "trigger": null, "unknowns": []});
    let out = compile(
        &CompileRequest::create(intent)
            .with_plan(plan)
            .answer("const.search_root", r#""./guide""#),
    )
    .unwrap();
    assert!(!keys(&out).contains(&"const.search_term"), "{out:#?}");
    let doc = document(&out);
    assert_eq!(doc["inputs"]["item"]["required"], true, "{doc:#}");
    assert_eq!(
        doc["tasks"]["search_hits"]["invoke"]["args"]["pattern"], "${{ inputs.item }}",
        "{doc:#}"
    );
}

fn triggered_search(trigger: &str) -> (String, Value) {
    let intent = format!(
        "{trigger}, read ./tickets.json, search it for that ticket and write the hits to ./hits.json"
    );
    let plan = json!({"bindings": [{"literal": "./tickets.json", "role": "path"}, {"literal": "./hits.json", "role": "path"}],
        "constraints": [],
        "effects": [{"evidence": "write the hits to ./hits.json", "policy": "automatic", "policy_literal": null, "target": "./hits.json", "verb": "write"}],
        "obligations": [],
        "operations": [{"categories": [], "detail": "./tickets.json", "evidence": "read ./tickets.json", "op": "read"},
                       {"categories": [], "detail": "that ticket", "evidence": "search it for that ticket", "op": "search"}],
        "rules": [], "slots": [], "strategy": "cold", "trigger": trigger, "unknowns": []});
    (intent, plan)
}

#[test]
fn an_event_delivers_the_query_of_a_search_over_the_requests_own_file() {
    let (intent, plan) = triggered_search("When a ticket number arrives");
    let out = compile(
        &CompileRequest::create(&intent)
            .with_plan(plan)
            .answer("const.search_root", r#"".""#),
    )
    .unwrap();
    assert!(!keys(&out).contains(&"const.search_term"), "{out:#?}");
    let doc = document(&out);
    assert_eq!(doc["inputs"]["item"]["required"], true, "{doc:#}");
    assert_eq!(
        doc["tasks"]["search_hits"]["invoke"]["args"]["pattern"], "${{ inputs.item }}",
        "{doc:#}"
    );
}

#[test]
fn a_schedule_delivers_no_query_so_the_search_text_is_asked() {
    let (intent, plan) = triggered_search("Every Monday at 9");
    let out = compile(
        &CompileRequest::create(&intent)
            .with_plan(plan)
            .answer("const.search_root", r#"".""#),
    )
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(keys(&out).contains(&"const.search_term"), "{out:#?}");
}
