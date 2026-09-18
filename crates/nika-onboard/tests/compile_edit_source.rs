// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source fidelity at the public Compile door; no files or providers are used.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use nika_onboard::compile::{CompileRequest, CompileStatus, DiagnosticKind, compile};
use serde_json::{Value, json};

const HEADER: &str = "# Licence stays here\n# café 🦋\nnika: edit-source\n";
const TAIL: &str = "\n# permits stay here\npermits: {tools: ['nika:jq']}\ntasks:\n  echo:\n    invoke:\n      tool: nika:jq\n      args: {input: '${{ const.payload }}', expression: '.'}\n# end\n";

fn assert_edit(prefix: &str, literal: &str, suffix: &str, value: &Value) {
    let source = format!("{prefix}{literal}{suffix}");
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
    }
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
        let out = compile(&CompileRequest::set_constant(&source, "payload", "7")).unwrap();
        assert_eq!(out.status, CompileStatus::Refused, "{out:?}");
        assert_eq!(out.candidate.as_deref(), Some(source.as_str()));
        assert!(
            !out.diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::Applied)
        );
    }
}
