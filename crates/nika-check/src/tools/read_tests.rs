// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Literal read types are decidable without evaluating dynamic bindings.

use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;
use serde_json::json;

use super::*;

fn read_findings(args: &serde_json::Value) -> Vec<UnknownArg> {
    let yaml = format!(
        "nika: read-contract\ntasks:\n  read:\n    invoke: {{tool: 'nika:read', args: {args}}}\n"
    );
    let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    scan_unknown_args(&wf)
}

#[test]
fn read_type_findings_keep_their_code_and_teaching() {
    let yaml = "nika: read-contract\npermits: {tools: [nika:read], fs: {read: ['./data/**']}}\ntasks:\n  read:\n    invoke: {tool: 'nika:read', args: {path: 42, binary: 'true'}}\n";
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = crate::check(&wf);
    assert!(!report.is_clean());
    let findings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.gate == "ARGS")
        .collect();
    assert_eq!(findings.len(), 2);
    assert!(
        findings
            .iter()
            .all(|f| f.code.as_deref() == Some("NIKA-INVOKE-002"))
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("`path:` must be a string"))
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("`binary:` must be a boolean"))
    );
    assert_eq!(
        report
            .extra_conformance_codes()
            .iter()
            .filter(|c| c.to_string() == "NIKA-INVOKE-002")
            .count(),
        2
    );
    assert!(
        crate::typed_renames(&report).is_empty(),
        "type errors never become key renames"
    );
}

#[test]
fn read_rejects_nonboolean_binary_literals() {
    for value in [
        json!("true"),
        json!("false"),
        json!(1),
        json!(null),
        json!([]),
        json!({}),
    ] {
        let findings = read_findings(&json!({"path": "file.txt", "binary": value}));
        assert_eq!(findings.len(), 1, "{value}: {findings:?}");
        assert_eq!(findings[0].arg, "binary");
        assert!(findings[0].invalid_value.is_some());
        assert!(
            findings[0].suggestion.is_none(),
            "a type error has no key rename"
        );
    }
}

#[test]
fn read_rejects_nonstring_path_literals() {
    for value in [json!(true), json!(12), json!(null), json!([]), json!({})] {
        let findings = read_findings(&json!({"path": value}));
        assert_eq!(findings.len(), 1, "{value}: {findings:?}");
        assert_eq!(findings[0].arg, "path");
    }
}

#[test]
fn read_accepts_default_boolean_and_whole_value_bindings() {
    for value in [
        json!({"path": "file.txt"}),
        json!({"path": "file.txt", "binary": false}),
        json!({"path": "file.txt", "binary": true}),
        json!({"path": "${{ with.path }}", "binary": "${{ with.binary }}"}),
        json!({"path": "dir/${{ with.name }}", "binary": "  ${{ with.binary }}  "}),
    ] {
        assert!(read_findings(&value).is_empty(), "{value}");
    }
}

#[test]
fn read_rejects_interpolation_that_cannot_produce_a_boolean() {
    for value in [
        json!("prefix ${{ with.binary }}"),
        json!("${{ with.binary }}suffix"),
        json!("${{ with.a }}${{ with.b }}"),
        json!(["${{ with.binary }}"]),
        json!({"value": "${{ with.binary }}"}),
    ] {
        let findings = read_findings(&json!({"path": "file.txt", "binary": value}));
        assert_eq!(findings.len(), 1, "{value}: {findings:?}");
        assert_eq!(findings[0].arg, "binary");
    }
}

#[test]
fn read_unknown_keys_remain_key_errors() {
    let findings = read_findings(&json!({"path": "file.txt", "encoding": "utf-8"}));
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].arg, "encoding");
    assert!(findings[0].invalid_value.is_none());
}
