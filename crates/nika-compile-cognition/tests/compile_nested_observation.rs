// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a host observed below a file's records reaches the seat, and the question gate claims no
//! more than that observation holds. The world is built as the CLI host builds it (the records'
//! sample, its kinds and its nested structure beside the rows, keyed by path) from synthetic
//! files; a capturing seat keeps every outgoing `InferRequest`. A nested collection's key paths
//! and kinds are in the request, never a value; a field question under a collection the
//! observation covers completely is refused with those exact names, and one under a collection
//! it does not cover is never refused as « already stated ». The flat-name refusal is unchanged.
//! Scripted doubles only: hermetic mechanics, never model capability.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Mutex;
use std::time::Duration;

use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, NativeMode};
use nika_compile_cognition::compile_with_provider;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Map, Value, json};

const INTENT: &str =
    "Read ./supplies.json, keep the stock parts to reorder and write them to ./out/stock.json.";
const SOURCE: &str = "./supplies.json";

/// A seat that keeps the text of every request and answers its script in order (a judge's
/// closed choice is approved).
struct Capture {
    answers: Mutex<Vec<String>>,
    seen: Mutex<Vec<String>>,
}

impl Capture {
    fn new(answers: &[Value]) -> Self {
        Self {
            answers: Mutex::new(answers.iter().map(Value::to_string).collect()),
            seen: Mutex::new(Vec::new()),
        }
    }
    fn first(&self) -> String {
        self.seen.lock().unwrap()[0].clone()
    }
}

impl ProviderInferDyn for Capture {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let said: Vec<String> = (request.messages.iter())
            .flat_map(|message| &message.content)
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect();
        self.seen.lock().unwrap().push(said.join("\n"));
        let choice = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema["properties"]["choice"]["enum"].clone(),
            _ => Value::Null,
        };
        let text = if let Some(keys) = choice.as_array() {
            let approve = ["faithful", "carried"]
                .into_iter()
                .find(|key| keys.iter().any(|value| value == *key))
                .unwrap_or("none");
            json!({"choice": approve}).to_string()
        } else {
            let mut queued = self.answers.lock().unwrap();
            if queued.is_empty() {
                String::new()
            } else {
                queued.remove(0)
            }
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

/// The world a host observes over whole JSON files, as the CLI host builds it.
fn world(files: &[(&str, Value)]) -> Value {
    let mut observed = Vec::new();
    let mut kinds = Map::new();
    for (path, document) in files {
        let rows = match document {
            Value::Array(items) => items.clone(),
            other => vec![other.clone()],
        };
        let sample = nika_compile::observation::records(&rows);
        observed.push(
            json!({"path": path, "state": "observed", "complete": true, "kind": "json",
            "columns": sample.columns, "common_columns": sample.common}),
        );
        let mut entry = sample.kinds.clone();
        if !sample.nested.is_null() {
            entry["nested"] = sample.nested.clone();
        }
        kinds.insert((*path).to_owned(), entry);
    }
    json!({"observed": observed, "kinds": kinds})
}

/// Synthetic supplies: stock records of two shapes, movements, values never sent.
fn supplies() -> Value {
    json!({"stock": [
            {"part": "P-ALPHA", "on_hand": 3, "reorder_level": 5, "manufacturer": "ACME-SENTINEL"},
            {"part": "P-BETA", "on_hand": 9, "reorder_level": 2, "on_order": 1}],
        "movements": [
            {"seq": 1, "part": "P-ALPHA", "kind": "MOVE-SENTINEL", "qty": 2},
            {"seq": 2, "part": "P-BETA", "kind": "MOVE-SENTINEL", "qty": 4}]})
}

/// A graph for [`INTENT`] that asks one structural question `key`.
fn asking(key: &str) -> Value {
    json!({"name": "reorder", "tasks": [
        {"id": "read_supplies", "verb": "invoke", "tool": "nika:read", "reads": [SOURCE], "purpose": "the supplies"},
        {"id": "keep", "verb": "invoke", "tool": "nika:jq", "with": [{"name": "document", "from": "read_supplies"}], "purpose": "keep the parts to reorder"},
        {"id": "write_stock", "verb": "invoke", "tool": "nika:write", "writes": ["./out/stock.json"], "with": [{"name": "text", "from": "keep"}], "purpose": "write them"}],
        "questions": [{"key": key, "label": "Which field?", "answer_type": "text", "why": "the request does not say"}],
        "gaps": [], "notes": "graph"})
}

async fn compiled(knowledge: Value, seat: &Capture) -> CompileOutcome {
    let request = CompileRequest::create(INTENT)
        .with_knowledge(knowledge)
        .with_authoring_policy(
            AuthoringPolicy::new("mock/nested", 4096, Duration::from_secs(2))
                .with_native(NativeMode::Sketch)
                .with_repairs(0),
        );
    compile_with_provider(&request, seat).await.unwrap()
}

/// The first round's diagnostics, as the journal keeps them.
fn refusals(out: &CompileOutcome) -> String {
    out.provenance.decision.as_ref().unwrap()["native"]["rounds"][0]["diagnostics"].to_string()
}

#[tokio::test]
async fn the_outgoing_request_states_the_nested_names_and_kinds_never_a_value() {
    let seat = Capture::new(&[asking("const.movement_part_field")]);
    let _ = compiled(world(&[(SOURCE, supplies())]), &seat).await;
    let request_text = seat.first();
    for name in [
        "movements[]",
        "stock[]",
        "qty",
        "kind",
        "on_hand",
        "reorder_level",
        "on_order",
    ] {
        assert!(
            request_text.contains(name),
            "{name} reaches the seat: {request_text}"
        );
    }
    for value in ["P-ALPHA", "ACME-SENTINEL", "MOVE-SENTINEL"] {
        assert!(!request_text.contains(value), "no value is sent: {value}");
    }
}

#[tokio::test]
async fn a_field_question_under_a_covered_collection_is_refused_with_its_exact_names() {
    let seat = Capture::new(&[asking("const.movement_part_field")]);
    let out = compiled(world(&[(SOURCE, supplies())]), &seat).await;
    let said = refusals(&out);
    assert!(said.contains("const.movement_part_field"), "{said}");
    assert!(said.contains("movements[]: kind, part, qty, seq"), "{said}");
    assert!(
        said.contains("stock[]:") && said.contains("reorder_level"),
        "{said}"
    );
    assert!(!said.contains("MOVE-SENTINEL"), "{said}");
}

#[tokio::test]
async fn a_collection_the_observation_does_not_cover_is_never_stated() {
    // Partial: a bound cut the collection, so its names are not all observed.
    let mut partial = world(&[(SOURCE, supplies())]);
    partial["kinds"][SOURCE]["nested"]["complete"] = json!(false);
    // Absent: a host that kept only the records' own keys (the structure unobserved).
    let mut flat = world(&[(SOURCE, supplies())]);
    flat["kinds"][SOURCE]
        .as_object_mut()
        .unwrap()
        .remove("nested");
    for (case, knowledge) in [("partial", partial), ("unobserved", flat)] {
        let seat = Capture::new(&[asking("const.movement_part_field")]);
        let out = compiled(knowledge, &seat).await;
        let said = refusals(&out);
        assert_eq!(
            said, "[]",
            "{case}: the question is admitted, never « already stated »"
        );
    }
}

#[tokio::test]
async fn a_flat_known_name_is_still_never_asked() {
    let tickets = json!([{"id": 1, "status": "open", "topic": "a"}, {"id": 2, "status": "closed", "topic": "b"}]);
    let seat = Capture::new(&[asking("const.status_field")]);
    let out = compiled(
        world(&[("./tickets.json", tickets), (SOURCE, supplies())]),
        &seat,
    )
    .await;
    let said = refusals(&out);
    assert!(said.contains("the observed world states them"), "{said}");
    assert!(said.contains("./tickets.json: id, status, topic"), "{said}");
}

#[tokio::test]
async fn several_files_keep_each_files_own_nested_names() {
    let orders = json!({"lines": [{"sku": "S1", "units": 2}], "header": {"region": "R-SENTINEL"}});
    let seat = Capture::new(&[asking("const.line_units_field")]);
    let out = compiled(
        world(&[(SOURCE, supplies()), ("./orders.json", orders)]),
        &seat,
    )
    .await;
    let request_text = seat.first();
    assert!(
        request_text.contains("lines[]")
            && request_text.contains("units")
            && request_text.contains("header"),
        "{request_text}"
    );
    assert!(!request_text.contains("R-SENTINEL"), "{request_text}");
    let said = refusals(&out);
    assert!(
        said.contains("./orders.json:") && said.contains("lines[]: sku, units"),
        "{said}"
    );
    assert!(said.contains("movements[]: kind, part, qty, seq"), "{said}");
}

#[tokio::test]
async fn temporal_formats_and_runtime_parsing_guidance_reach_the_author_without_values() {
    let mut document = supplies();
    document["punches"] = json!([
        {"at": "2042-11-28T07:36", "note": "PRIVATE-TIME-NOTE"},
        {"at": "2042-11-29T07:36:42Z"}, {"at": "2042-11-30T07:36:42+04:00"},
        {"at": null}, {}]);
    let seat = Capture::new(&[asking("const.timezone")]);
    let _ = compiled(world(&[(SOURCE, document)]), &seat).await;
    let request_text = seat.first();
    for fact in [
        "punches[].at",
        "temporal",
        "matched",
        "9999-99-99T99:99",
        "9999-99-99T99:99:99Z",
        "9999-99-99T99:99:99+99:99",
        "fromdateiso8601",
        "strptime",
        "mktime",
        "no timezone is inferred",
    ] {
        assert!(
            request_text.contains(fact),
            "missing observed context: {fact}"
        );
    }
    for private in [
        "2042-11-28",
        "2042-11-29",
        "2042-11-30",
        "+04:00",
        "PRIVATE-TIME-NOTE",
    ] {
        assert!(
            !request_text.contains(private),
            "a raw temporal value leaked: {private}"
        );
    }
}
