// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source fidelity at the public Compile door; no files or providers are used.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nika_onboard::compile::{CompileRequest, CompileStatus, DiagnosticKind, compile};
use serde_json::{Value, json};

const HEADER: &str = "# Licence stays here\n# café 🦋\nnika: edit-source\n";
const TAIL: &str = "\n# permits stay here\npermits: {tools: ['nika:jq']}\ntasks:\n  echo:\n    invoke:\n      tool: nika:jq\n      args: {input: '${{ const.payload }}', expression: '.'}\n# end\n";

/// Both request doors must agree; the shared candidate is returned so a caller
/// can pin the exact emitted bytes instead of trusting a reparse alone.
fn assert_edit(prefix: &str, literal: &str, suffix: &str, value: &Value) -> String {
    let source = format!("{prefix}{literal}{suffix}");
    let mut candidates = Vec::new();
    for request in [
        CompileRequest::edit(&source, format!("Set const.payload to {value}")),
        CompileRequest::set_constant(&source, "payload", value.to_string()),
    ] {
        let out = compile(&request).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{source}\n{out:?}");
        let candidate = out.candidate.unwrap();
        assert!(candidate.starts_with(prefix), "{candidate}");
        assert!(candidate.ends_with(suffix), "{candidate}");
        let mut expected: Value = serde_yaml_bw::from_str(&source).unwrap();
        let slot = &mut expected["const"]["payload"];
        if slot.get("type").is_some() && slot.get("value").is_some() {
            slot["value"] = value.clone();
        } else {
            *slot = value.clone();
        }
        assert_eq!(
            serde_yaml_bw::from_str::<Value>(&candidate).unwrap(),
            expected
        );
        candidates.push(candidate);
    }
    assert_eq!(candidates[0], candidates[1]);
    candidates.remove(0)
}

fn assert_refused_unchanged(source: &str, name: &str, literal_json: &str) {
    let out = compile(&CompileRequest::set_constant(source, name, literal_json)).unwrap();
    assert_eq!(out.status, CompileStatus::Refused, "{source}\n{out:?}");
    assert_eq!(out.candidate.as_deref(), Some(source));
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Applied)
    );
}

#[test]
fn scalar_edits_preserve_all_surrounding_bytes_and_types() {
    let prefix = format!("{HEADER}const:\n  untouched: '100 # not a comment'\n  payload: ");
    let suffix = format!(" # retain this\n  last: true{TAIL}");
    for (old, new) in [
        ("100", json!(250)),
        ("'it''s old'", json!("new # value: [literal]")),
        (r#""old \"quoted\" text""#, json!("line one\nline two")),
        ("true", json!(false)),
        ("null", json!(7)),
        ("1.5", json!(2.25)),
        ("'café 🦋'", json!("東京")),
        ("'100'", json!("250")),
        ("before", json!("false")),
        ("hello, world [plain]", json!("after")),
        ("https://example.invalid/a#anchor", json!("after")),
        ("0", json!({"nested": [1, true, "#"]})),
    ] {
        assert_edit(&prefix, old, &suffix, &new);
    }
}

#[test]
fn flow_and_typed_values_keep_their_own_surroundings() {
    assert_edit(
        &format!("{HEADER}const: {{untouched: 100, payload: "),
        "100",
        &format!(", other: 'same'}} # after map{TAIL}"),
        &json!(250),
    );
    assert_edit(
        &format!("{HEADER}const:\n  payload:\n    type: integer # declaration\n    value: "),
        "100",
        &format!(" # value comment\n  other: true{TAIL}"),
        &json!(250),
    );
    assert_edit(
        &format!("{HEADER}const: {{payload: "),
        "{type: invoice, id: 42}",
        &format!(", other: 'same'}}{TAIL}"),
        &json!({"id": 43}),
    );
    for old in [
        "[1, 'two', true]",
        "[\n    1, # inside the replaced value\n    2\n  ]",
    ] {
        assert_edit(
            &format!("{HEADER}const:\n  payload: "),
            old,
            &format!(" # outside the replaced value\n  other: 'same'{TAIL}"),
            &json!([3, "four"]),
        );
    }
}

#[test]
fn crlf_and_noop_edits_do_not_normalize_source() {
    let prefix = format!("{HEADER}const:\n  payload: ").replace('\n', "\r\n");
    let suffix = format!(" # inline{TAIL}").replace('\n', "\r\n");
    assert_edit(&prefix, "100", &suffix, &json!(250));
    for old in ["100", "'100'", "null", "{x: [1, 2]}"] {
        let source = format!("{prefix}{old}{suffix}");
        let doc: Value = serde_yaml_bw::from_str(&source).unwrap();
        let out = compile(&CompileRequest::set_constant(
            &source,
            "payload",
            doc["const"]["payload"].to_string(),
        ))
        .unwrap();
        assert_eq!(out.status, CompileStatus::Ready);
        assert_eq!(out.candidate.as_deref(), Some(source.as_str()));
    }
}

/// Accept the value, pin the emitted token, then prove closure: an unrelated
/// second EDIT still works and restoring the literal restores the exact source.
/// `old` is both the YAML literal in the base and the JSON answer restoring it.
fn assert_editable_again(prefix: &str, old: &str, suffix: &str, value: &Value, token: &str) {
    let first = assert_edit(prefix, old, suffix, value);
    assert_eq!(first, format!("{prefix}{token}{suffix}"));
    let second = compile(&CompileRequest::set_constant(&first, "other", "2")).unwrap();
    assert_eq!(second.status, CompileStatus::Ready, "{second:?}");
    let second = second.candidate.unwrap();
    assert_eq!(second, first.replace("other: 1", "other: 2"));
    let doc: Value = serde_yaml_bw::from_str(&second).unwrap();
    let payload = &doc["const"]["payload"];
    let typed = payload.get("type").is_some() && payload.get("value").is_some();
    assert_eq!(if typed { &payload["value"] } else { payload }, value);
    assert_eq!(doc["const"]["other"], 2);
    let restored = compile(&CompileRequest::set_constant(&first, "payload", old)).unwrap();
    assert_eq!(restored.status, CompileStatus::Ready, "{restored:?}");
    assert_eq!(restored.candidate, Some(format!("{prefix}{old}{suffix}")));
}

#[test]
fn emitted_unicode_retains_its_value_and_can_be_edited_again() {
    let prefix = format!("{HEADER}const:\n  payload: ");
    let suffix = format!(" # inline\n  other: 1{TAIL}");
    // DEL, every C1 control (NEL included) and both BMP noncharacters: raw,
    // the secondary decoder rejects or folds them, so each is escaped.
    let hostile = ('\u{7f}'..='\u{9f}').chain(['\u{fffe}', '\u{ffff}']);
    let mut every = (String::new(), String::new());
    for c in hostile {
        let escape = format!("\\u{:04x}", u32::from(c));
        every.0.push(c);
        every.1.push_str(&escape);
        assert_editable_again(
            &prefix,
            "100",
            &suffix,
            &json!(format!("a{c}b")),
            &format!("\"a{escape}b\""),
        );
        assert_editable_again(
            &prefix,
            "100",
            &suffix,
            &json!([1, {"deep": [format!("{c}"), true]}]),
            &format!("[1,{{\"deep\":[\"{escape}\",true]}}]"),
        );
        assert_editable_again(
            &prefix,
            "100",
            &suffix,
            &json!({format!("k{c}"): format!("{c}v")}),
            &format!("{{\"k{escape}\":\"{escape}v\"}}"),
        );
    }
    // A flow parent and a typed declaration take the same token.
    let (raw, escaped) = every;
    assert_editable_again(
        &format!("{HEADER}const: {{payload: "),
        "100",
        &format!(", other: 1}} # after map{TAIL}"),
        &json!({"k": raw.as_str()}),
        &format!("{{\"k\":\"{escaped}\"}}"),
    );
    assert_editable_again(
        &format!("{HEADER}const:\n  payload:\n    type: string # declaration\n    value: "),
        r#""old""#,
        &format!(" # value comment\n  other: 1{TAIL}"),
        &json!(raw.as_str()),
        &format!("\"{escaped}\""),
    );
}

#[test]
fn escaping_stays_inside_the_hostile_set_and_existing_escapes() {
    let prefix = format!("{HEADER}const:\n  payload: ");
    let suffix = format!(" # inline\n  other: 1{TAIL}");
    // Neighbours of each escaped range, a line separator and non-BMP text stay raw.
    let raw = "~\u{a0}\u{2028}\u{fffd}\u{feff}🦋";
    assert_editable_again(&prefix, "100", &suffix, &json!(raw), &format!("\"{raw}\""));
    // An escaped backslash before a hostile point must not swallow the new escape.
    assert_editable_again(
        &prefix,
        "100",
        &suffix,
        &json!("\\\u{7f}\"\n"),
        r#""\\\u007f\"\n""#,
    );
}

#[test]
fn omitted_null_is_refused_while_written_null_stays_editable() {
    // The parser marks an omitted value at the NEXT token; that range is never
    // the target, whatever the next token looks like.
    for consts in [
        "const:\n  payload:\n  other: true",
        "const:\n  payload: # only a comment\n  other: true",
        "const:\n  payload:\n  \"other\": true",
        "const:\n  payload:\n  ~: true\n  other: true",
        "const:\n  other: true\n  payload:",
        "const: {payload: , other: true}",
        "const: {other: true, payload: }",
    ] {
        let source = format!("{HEADER}{consts}{TAIL}");
        assert_refused_unchanged(&source, "payload", "7");
        // The neighbour keeps working: only the omitted marker is out of scope.
        let out = compile(&CompileRequest::set_constant(&source, "other", "false")).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{source}\n{out:?}");
    }
    let prefix = format!("{HEADER}const:\n  payload: ");
    let suffix = format!(" # inline\n  other: true{TAIL}");
    for written in ["~", "null"] {
        let candidate = assert_edit(&prefix, written, &suffix, &json!(7));
        assert_eq!(candidate, format!("{prefix}7{suffix}"));
    }
}

#[test]
fn unsupported_block_presentations_are_refused_without_losing_comments() {
    for old in [
        "|\n    old text",
        ">\n    old text",
        "first\n    second",
        "'first\n    second'",
        "\n    x: 1",
    ] {
        let source = format!("{HEADER}const:\n  payload: {old}\n  other: 'keep'{TAIL}");
        assert_refused_unchanged(&source, "payload", "7");
    }
}

/// Scope limitation, pinned on purpose: CREATE's assembler emits block forms
/// for multi-line and collection answers, and the bounded EDIT refuses those
/// presentations instead of re-emitting the whole document.
#[test]
fn create_block_output_is_outside_the_bounded_edit() {
    let create = |answer: &str| {
        let request = CompileRequest::create("classify-and-route").answer("const.request", answer);
        let out = compile(&request).unwrap();
        assert_eq!(out.status, CompileStatus::Ready, "{out:?}");
        out.candidate.unwrap()
    };
    let one_line = create(r#""One line.""#);
    let edited = compile(&CompileRequest::set_constant(
        &one_line,
        "request",
        r#""Another line.""#,
    ))
    .unwrap();
    assert_eq!(edited.status, CompileStatus::Ready, "{edited:?}");
    assert_eq!(
        edited.candidate,
        Some(one_line.replace("request: One line.", r#"request: "Another line.""#))
    );
    for (answer, block_form) in [
        (r#""line one\nline two""#, "request: |-\n"),
        (r#"["a", "b"]"#, "request:\n  - a\n"),
    ] {
        let source = create(answer);
        assert!(source.contains(block_form), "{source}");
        assert_refused_unchanged(&source, "request", r#""Another line.""#);
    }
}
