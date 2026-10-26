// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parse refusals a newcomer meets first teach the shape they refused
//! (#1600): a scalar verb body, an untyped input and a builtin written as a
//! task key.

use crate::error::SchemaError;
use crate::parser::{ParseMode, parse};
use crate::source::FileId;

fn refusal(source: &str) -> SchemaError {
    match parse(source, FileId::new(0), ParseMode::Strict) {
        Ok(_) => panic!("the fixture must be refused:\n{source}"),
        Err(err) => err,
    }
}

#[test]
fn a_scalar_verb_body_teaches_its_skeleton() {
    for (body, skeleton) in [
        ("infer: \"say hi\"", "infer: { prompt: \"say hi\" }"),
        ("invoke: nika:jq", "invoke: { tool: nika:jq, args: {"),
        (
            "agent: research the topic",
            "agent: { prompt: \"research the topic\" }",
        ),
    ] {
        let err = refusal(&format!(
            "nika: p\nmodel: mock/echo\ntasks:\n  a:\n    {body}\n"
        ));
        assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-019", "{err}");
        let message = err.to_string();
        assert!(message.contains(skeleton), "`{body}` → {message}");
    }
    let child = refusal("nika: p\ntasks:\n  a:\n    invoke: ./child.nika\n").to_string();
    assert!(
        child.contains("invoke: { workflow: ./child.nika }"),
        "{child}"
    );
}

#[test]
fn an_untyped_input_teaches_the_typed_form_and_the_roster() {
    let err = refusal(
        "nika: p\nmodel: mock/echo\ninputs:\n  topic: {default: x}\ntasks:\n  a:\n    infer: {prompt: hi}\n",
    );
    assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-019", "{err}");
    let message = err.to_string();
    for needle in [
        "topic: { type: string, default: \"x\" }",
        "integer",
        "timestamp",
    ] {
        assert!(message.contains(needle), "`{needle}` → {message}");
    }
    let count = refusal(
        "nika: p\ninputs:\n  n: {default: 3}\ntasks:\n  a:\n    exec: { command: [\"true\"] }\n",
    )
    .to_string();
    assert!(
        count.contains("n: { type: integer, default: 3 }"),
        "{count}"
    );
}

/// The roster an untyped input is taught is the type grammar's own spelling.
#[test]
fn the_taught_type_roster_round_trips_through_the_type_grammar() {
    let roster = crate::parser::envelope_values::primitive_roster();
    assert_eq!(roster.len(), 10, "{roster:?}");
    for name in roster {
        assert!(
            nika_types::types::Primitive::from_name(name).is_some_and(|p| p.as_str() == name),
            "`{name}` is taught but the type grammar refuses it"
        );
    }
}

#[test]
fn a_builtin_written_as_a_task_key_teaches_the_invoke_call() {
    let err = refusal("nika: p\ntasks:\n  a:\n    fetch: {url: \"https://example.com\"}\n");
    assert_eq!(err.spec_code().to_string(), "NIKA-PARSE-005", "{err}");
    let message = err.to_string();
    assert!(
        message.contains("invoke: { tool: nika:fetch, args: { url: … } }"),
        "{message}"
    );
    // A key that is no builtin keeps the generic refusal.
    let other = refusal("nika: p\ntasks:\n  a:\n    zzz: 1\n").to_string();
    assert!(!other.contains("invoke: { tool: nika:"), "{other}");
}
