// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One requested record by identifier (R4 A7). Root's public counterexample (carrier 79657c7b):
//! « Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json » compiled READY, and
//! over two different records with id 42 the workflow checked, ran and wrote whichever came
//! first (AB wrote A, BA wrote B): the lowering selected `map(select(…)) | .[0]` and the admit only
//! asked for a nonempty object. The lookup law now resolves exactly one distinct record at run
//! time — the compile's observation is bounded and quotes no value, so it cannot certify a whole
//! file, and the source may change after the compile — and every effect waits for that record.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::collections::BTreeSet;

use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::{Map, Value, json};

mod common;

const LOOK_UP: &str = "Look up ticket 42 in ./tickets.json and write it to ./ticket-42.json";
const LOOK_UP_POST: &str = "Look up ticket 42 in ./tickets.json, write it to ./ticket-42.json and post it to http://127.0.0.1:18471/hook";

/// The workflow a request compiles to over the observed ticket file, its identifier field
/// answered `id` (the question the lookup asks), with an optional recorded plan replayed.
fn workflow(intent: &str, plan: Option<Value>) -> Value {
    let mut request = CompileRequest::create(intent)
        .with_knowledge(common::observed(&[(
            "./tickets.json",
            &["id", "status", "title"],
        )]))
        .answer("const.ticket_id_field", "\"id\"");
    if let Some(plan) = plan {
        request = request.with_plan(plan);
    }
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn tasks(doc: &Value) -> &Map<String, Value> {
    doc["tasks"].as_object().unwrap()
}

/// Every task a task waits for: the tasks its `with:` bindings read and its `after:` keys.
fn parents(task: &Value) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let text = task["with"].to_string();
    for part in text.split("tasks.").skip(1) {
        let name: String = part
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        found.insert(name);
    }
    if let Some(after) = task["after"].as_object() {
        found.extend(after.keys().cloned());
    }
    found
}

/// Every task `name` transitively waits for.
fn ancestors(doc: &Value, name: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![name.to_owned()];
    while let Some(next) = stack.pop() {
        for parent in parents(&tasks(doc)[&next]) {
            if seen.insert(parent.clone()) {
                stack.push(parent);
            }
        }
    }
    seen
}

#[test]
fn a_literal_lookup_selects_through_the_one_record_law() {
    let doc = workflow(LOOK_UP, None);
    let record = &tasks(&doc)["lookup_record"]["invoke"]["args"];
    let expression = record["expression"].as_str().unwrap();
    // The one-record law, and the record it selects keeps every number exact (R4 A8).
    let guarded = format!(
        "\n{} | dguard(null)",
        nika_compile::surface::SELECT_BY_FIELD
    );
    assert!(expression.ends_with(&guarded), "{expression}");
    assert!(expression.contains("all(.[]; . == $m[0])"), "{expression}");
    assert!(
        expression.contains("no single record can be chosen"),
        "{expression}"
    );
    assert!(
        !expression.contains("| .[0])"),
        "no first-match: {expression}"
    );
    assert_eq!(record["input"]["field"], "${{ const.ticket_id_field }}");
    assert_eq!(doc["const"]["ticket_id"], "42");
    // A missing record is still refused by the admit before anything else happens.
    let admit = &tasks(&doc)["lookup_admit"];
    assert_eq!(admit["invoke"]["tool"], "nika:assert");
    assert!(ancestors(&doc, "lookup_admit").contains("lookup_record"));
}

/// Every effect of a lookup workflow — the write and the webhook post alike — runs only after
/// the one record resolved and the admit passed: an ambiguity stops the run before any of them.
#[test]
fn every_effect_waits_for_the_one_record() {
    let doc = workflow(LOOK_UP_POST, None);
    let effects: Vec<(&String, &Value)> = tasks(&doc)
        .iter()
        .filter(|(_, t)| {
            let tool = t["invoke"]["tool"].as_str().unwrap_or_default();
            !matches!(
                tool,
                "nika:read" | "nika:jq" | "nika:assert" | "nika:convert"
            )
        })
        .collect();
    let tools: BTreeSet<&str> = effects
        .iter()
        .map(|(_, t)| t["invoke"]["tool"].as_str().unwrap_or("infer"))
        .collect();
    assert!(effects.len() >= 2, "a write and a post: {tools:?} {doc:#}");
    for (name, _) in effects {
        let above = ancestors(&doc, name);
        assert!(
            above.contains("lookup_record") && above.contains("lookup_admit"),
            "{name} does not wait for the one record: {above:?}"
        );
    }
}

/// Root's recorded decision-seat plan (three fresh decisions chose `lookup` for « find ticket 42 »),
/// replayed with the predeclared answer: the same READY workflow, now through the one-record law.
#[test]
fn roots_recorded_decision_replays_through_the_one_record_law() {
    let plan = json!({
        "bindings": [{"literal": "./ticket-42.json", "role": "path"},
                     {"literal": "./tickets.json", "role": "path"},
                     {"literal": "./ticket-42.json", "role": "path"}],
        "constraints": [],
        "effects": [{"evidence": "write it to ./ticket-42.json", "policy": "automatic",
                     "policy_literal": null, "target": "./ticket-42.json", "verb": "write"}],
        "obligations": [],
        "operations": [{"categories": [], "detail": "./tickets.json", "evidence": "Read ./tickets.json", "op": "read"},
                       {"categories": [], "detail": "ticket 42", "evidence": "find ticket 42", "op": "lookup"}],
        "rules": [], "slots": [], "strategy": "warm", "trigger": null, "unknowns": []
    });
    let doc = workflow(
        "Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json",
        Some(plan),
    );
    let expression = tasks(&doc)["lookup_record"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap();
    let guarded = format!(
        "\n{} | dguard(null)",
        nika_compile::surface::SELECT_BY_FIELD
    );
    assert!(expression.ends_with(&guarded), "{expression}");
    assert!(ancestors(&doc, "write_output").contains("lookup_admit"));
}
