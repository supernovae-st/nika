// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Properties a provider judge returns beside its choice: asked only by a question that declares
//! them, read from the same answer by name, recorded beside the choice; every other closed choice
//! keeps its exact bytes.

use super::{
    AlsoProperty, ChoiceAnswer, ChoiceOption, ChoiceQuestion, closed_choice, decoded, decoded_also,
    reason, record,
};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, Message, StopReason, TokenUsage};
use serde_json::{Map, Value, json};

fn question() -> ChoiceQuestion {
    ChoiceQuestion::new(
        "verify-request",
        "Compare the request with the bytes.",
        json!({"request": "r"}),
        vec![ChoiceOption::new("faithful", "it carries the request")],
    )
}

fn parts() -> AlsoProperty {
    AlsoProperty::new(
        "model_parts",
        json!({"type": "array", "items": {"type": "string"}}),
        "Also list the parts a model must write.",
    )
}

fn answer(text: &str) -> InferResponse {
    let content = vec![ContentBlock::Text {
        text: text.to_owned(),
    }];
    InferResponse::new(content, TokenUsage::new(1, 1), StopReason::EndTurn)
}

/// The text of the system message.
fn system(messages: &[Message]) -> String {
    match messages.first().and_then(|message| message.content.first()) {
        Some(ContentBlock::Text { text }) => text.clone(),
        _ => String::new(),
    }
}

/// The map of `value`, an object.
fn map(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object,
        _ => Map::new(),
    }
}

/// A closed choice that declares no property is asked and recorded exactly as before, and an
/// answer's extra key is never read for it.
#[test]
fn a_question_without_a_property_keeps_its_bytes() {
    let (messages, schema) = closed_choice(&question());
    let told = system(&messages);
    assert!(
        told.ends_with("Return only a JSON object {\"choice\": <key>}."),
        "{told}"
    );
    let today = json!({"type": "object", "additionalProperties": false, "required": ["choice"],
        "properties": {"choice": {"type": "string", "enum": ["faithful", "none"]}}});
    assert_eq!(schema, today);
    let reply = answer(r#"{"choice": "faithful", "model_parts": ["a summary"]}"#);
    assert!(decoded_also(&question(), &reply).is_empty());
    let recorded = record(&question(), Ok(&ChoiceAnswer::new("faithful", "m")));
    assert!(recorded.get("also").is_none(), "{recorded}");
}

/// A question that declares a property asks it beside the choice, in the text and in the schema,
/// and reads it from the same answer; an answer that does not hold it carries none.
#[test]
fn a_declared_property_is_asked_read_and_recorded_beside_the_choice() {
    let asked = question().with_also(parts());
    let (messages, schema) = closed_choice(&asked);
    let told = system(&messages);
    let returned = "Return only a JSON object {\"choice\": <key>, \"model_parts\": …}. Also list the parts a model must write.";
    assert!(told.ends_with(returned), "{told}");
    assert_eq!(schema["required"], json!(["choice", "model_parts"]));
    assert_eq!(schema["properties"]["model_parts"], parts().schema);
    let reply = answer(r#"{"choice": "faithful", "model_parts": ["a summary"]}"#);
    assert_eq!(decoded(&asked, &reply), Ok("faithful".to_owned()));
    let held = map(json!({"model_parts": ["a summary"]}));
    assert_eq!(decoded_also(&asked, &reply), held);
    let silent = answer(r#"{"choice": "faithful"}"#);
    assert!(decoded_also(&asked, &silent).is_empty());
    let mut answered = ChoiceAnswer::new("faithful", "m");
    answered.also = held;
    let recorded = record(&asked, Ok(&answered));
    assert_eq!(recorded["also"], json!({"model_parts": ["a summary"]}));
}

/// Several properties are asked in the order they were declared and read back by name: a
/// statement and the judge's reason, in the one answer.
#[test]
fn several_properties_are_asked_in_order_and_read_by_name() {
    let asked = question().with_also(parts()).with_also(reason());
    let (messages, schema) = closed_choice(&asked);
    let told = system(&messages);
    let keys = "{\"choice\": <key>, \"model_parts\": …, \"reason\": …}. Also list the parts";
    assert!(told.contains(keys), "{told}");
    assert!(told.ends_with(&reason().instructions), "{told}");
    assert_eq!(
        schema["required"],
        json!(["choice", "model_parts", "reason"])
    );
    assert_eq!(schema["properties"]["reason"], json!({"type": "string"}));
    let reply =
        answer(r#"{"choice": "faithful", "model_parts": [], "reason": "the write keeps the ids"}"#);
    let held = map(json!({"model_parts": [], "reason": "the write keeps the ids"}));
    assert_eq!(decoded_also(&asked, &reply), held);
}
