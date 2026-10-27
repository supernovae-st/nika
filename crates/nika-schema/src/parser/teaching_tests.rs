// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parse refusals a newcomer meets first teach the shape they refused
//! (#1600, #1608): a scalar verb body, an untyped input, a builtin written as
//! a task key, and a YAML syntax error that names its site and its usual cause.

use crate::error::SchemaError;
use crate::parser::{ParseMode, parse};
use crate::source::FileId;

fn refusal(source: &str) -> SchemaError {
    match parse(source, FileId::new(0), ParseMode::Strict) {
        Ok(_) => panic!("the fixture must be refused:\n{source}"),
        Err(err) => err,
    }
}

/// The 1-based line a refusal's span starts on.
fn span_line(source: &str, err: &SchemaError) -> usize {
    let span = err.span().unwrap_or_else(|| panic!("a span: {err:?}"));
    let start = usize::try_from(span.start.0).unwrap_or(usize::MAX);
    source[..start.min(source.len())].matches('\n').count() + 1
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

#[test]
fn a_colon_in_a_plain_value_names_its_site_and_the_quote_repair() {
    let source = "nika: p\nmodel: mock/echo\ntasks:\n  a:\n    infer:\n      prompt: Note: this has a colon: here\n";
    let err = refusal(source);
    assert!(
        matches!(err, SchemaError::YamlSyntax { span: Some(_), .. }),
        "{err:?}"
    );
    assert_eq!(span_line(source, &err), 6, "{err:?}");
    let message = err.to_string();
    assert!(
        message.contains("mapping values are not allowed") && message.contains("quote"),
        "{message}"
    );
}

#[test]
fn a_bare_expression_in_a_flow_mapping_teaches_quoting_it() {
    let source = "nika: p\nmodel: mock/echo\ninputs:\n  t: {type: string, default: x}\ntasks:\n  a:\n    infer: {prompt: ${{ inputs.t }}}\n";
    let err = refusal(source);
    assert!(
        matches!(err, SchemaError::YamlSyntax { span: Some(_), .. }),
        "{err:?}"
    );
    assert_eq!(span_line(source, &err), 7, "{err:?}");
    let message = err.to_string();
    assert!(
        message.contains("quote") && message.contains("\"${{ … }}\""),
        "{message}"
    );
}

/// A taught skeleton must be one the parser accepts as written: a bare builtin
/// is reflected under its `nika:` prefix, an `mcp:` reference as written, and
/// any other bare word gets the placeholder rather than a refused `tool:`.
#[test]
fn a_scalar_invoke_teaches_a_tool_reference_the_parser_accepts() {
    for (target, skeleton) in [
        ("fetch", "invoke: { tool: nika:fetch, args: {"),
        ("nika:jq", "invoke: { tool: nika:jq, args: {"),
        (
            "mcp:docs/search",
            "invoke: { tool: mcp:docs/search, args: {",
        ),
        ("zzz", "invoke: { tool: nika:<name>, args: {"),
    ] {
        let message =
            refusal(&format!("nika: p\ntasks:\n  a:\n    invoke: {target}\n")).to_string();
        assert!(message.contains(skeleton), "`invoke: {target}` → {message}");
    }
    // Followed as written, the taught builtin call parses.
    parse(
        "nika: p\ntasks:\n  a:\n    invoke: { tool: nika:fetch, args: { url: \"https://example.com\" } }\n",
        FileId::new(0),
        ParseMode::Strict,
    )
    .unwrap_or_else(|err| panic!("the taught form must parse: {err}"));
}

/// A list or mapping default names its container form, never a primitive it is not, and keeps
/// the default; a `null` default names no type it cannot know.
#[test]
fn an_untyped_container_input_teaches_its_container_form() {
    for (entry, skeleton) in [
        (
            "tags: {default: [a, b]}",
            "tags: { type: { array: string }, default: [\"a\",\"b\"] }",
        ),
        (
            "mixed: {default: [1, a]}",
            "mixed: { type: { array: <T> }, default: [1,\"a\"] }",
        ),
        (
            "conf: {default: {k: v}}",
            "conf: { type: { object: { field: <T> } }, default: {\"k\":\"v\"} }",
        ),
        (
            "none: {default: null}",
            "none: { type: <T>, default: null }",
        ),
    ] {
        let message = refusal(&format!(
            "nika: p\ninputs:\n  {entry}\ntasks:\n  a:\n    exec: {{ command: [\"true\"] }}\n"
        ))
        .to_string();
        assert!(message.contains(skeleton), "`{entry}` → {message}");
    }
}

/// The roster an untyped input is taught is the one the type grammar's own refusal prints, so
/// a primitive added to the grammar cannot be missing from this teaching.
#[test]
fn the_taught_roster_is_the_type_grammars_own() {
    let roster = crate::parser::envelope_values::primitive_roster().join(" · ");
    let refusal = nika_types::types::parse_type(
        &serde_json::json!("zzzzzz"),
        &std::collections::BTreeSet::new(),
        "t",
    )
    .map(|_| ())
    .err()
    .map(|err| err.detail)
    .unwrap_or_default();
    assert!(
        refusal.contains(&format!("the primitives are {roster};")),
        "taught `{roster}` but the grammar says: {refusal}"
    );
}

/// A builtin task key is recognized with its `nika:` prefix too.
#[test]
fn a_prefixed_builtin_task_key_teaches_the_invoke_call() {
    let message =
        refusal("nika: p\ntasks:\n  a:\n    nika:fetch: {url: \"https://example.com\"}\n")
            .to_string();
    assert!(
        message.contains("invoke: { tool: nika:fetch, args: { url: … } }"),
        "{message}"
    );
}

/// The colon cause names both readings of `outer: inner: rest`: a value to quote, and a key
/// that meant to nest. A flow mapping whose expression is already quoted is not told to quote
/// it.
#[test]
fn a_yaml_cause_names_what_the_line_shows_and_nothing_else() {
    let nested =
        refusal("nika: p\nmodel: mock/echo\ntasks:\n  a:\n    infer: prompt: hi\n").to_string();
    assert!(
        nested.contains("quote the whole value")
            && nested.contains(
                "if `prompt:` is a key of its own, put it on its own line, indented under `infer:`"
            ),
        "{nested}"
    );
    let sentence = refusal(
        "nika: p\nmodel: mock/echo\ntasks:\n  a:\n    infer:\n      prompt: a long note here: with a colon\n",
    )
    .to_string();
    assert!(
        sentence.contains("quote the whole value") && !sentence.contains("is a key of its own"),
        "{sentence}"
    );
    let quoted = refusal(
        "nika: p\nmodel: mock/echo\ninputs:\n  t: {type: string, default: x}\ntasks:\n  a:\n    infer: {prompt: \"${{ inputs.t }}\", system: a: b}\n",
    )
    .to_string();
    assert!(!quoted.contains("an expression must be quoted"), "{quoted}");
    let in_text = refusal(
        "nika: p\nmodel: mock/echo\ninputs:\n  t: {type: string, default: x}\ntasks:\n  a:\n    infer: {prompt: \"say ${{ inputs.t }}\", system: a: b}\n",
    )
    .to_string();
    assert!(
        !in_text.contains("an expression must be quoted"),
        "{in_text}"
    );
    let single = refusal(
        "nika: p\nmodel: mock/echo\ninputs:\n  t: {type: string, default: x}\ntasks:\n  a:\n    infer: {prompt: 'it''s ${{ inputs.t }}', system: a: b}\n",
    )
    .to_string();
    assert!(!single.contains("an expression must be quoted"), "{single}");
    // Unquoted, the same expression is told to be quoted.
    let bare = refusal(
        "nika: p\nmodel: mock/echo\ninputs:\n  t: {type: string, default: x}\ntasks:\n  a:\n    infer: {system: 'it''s', prompt: ${{ inputs.t }}}\n",
    )
    .to_string();
    assert!(bare.contains("an expression must be quoted"), "{bare}");
}

/// A scanner that ran out of input names the stream end; the frame names where the content
/// ended (the place the missing closer belongs), never a blank line past it, and the cause
/// says the file ended there.
#[test]
fn a_refusal_at_the_end_of_the_file_frames_the_last_content() {
    for source in [
        "nika: p\nmodel: mock/echo\ntasks:\n  a:\n    infer: {prompt: \"hi\"\n\n\n",
        "nika: p\nmodel: mock/echo\ntasks:\n  a:\n    infer: {prompt: \"hi\", max_tokens: 10",
    ] {
        let err = refusal(source);
        assert_eq!(span_line(source, &err), 5, "{source:?} → {err:?}");
        let start = err.span().map_or(usize::MAX, |span| {
            usize::try_from(span.start.0).unwrap_or(usize::MAX)
        });
        assert_eq!(start, source.trim_end().len(), "{source:?} → {err:?}");
        let message = err.to_string();
        assert!(
            message.contains("the file ends inside an open `{ … }`: close it with `}`"),
            "{message}"
        );
    }
}
