// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use serde_json::json;

use super::*;

fn source() -> Value {
    json!({"id": "src:engine", "kind": "source_artifact", "title": "Nika engine examples",
           "licence": "AGPL-3.0-or-later", "ownership": "project", "upstream": ""})
}

fn block() -> Value {
    json!({
        "id": "block:total", "kind": "block", "title": "Total a column",
        "purpose": "Sum one numeric column of a CSV.", "trust": "machine-verified",
        "status": "QUALIFIED", "proof_level": "CHECKED", "confidence": 0.85,
        "pin": {"binary": "nika 0.122.0 (4bdc06aee)", "spec_sha": "41eab151"},
        "provenance": {"sources": ["src:engine", "src:gone"], "activity": "extracted"},
        "file": "blocks/total.nika",
    })
}

#[test]
fn a_block_is_presented_whole_as_a_component_with_its_provenance() {
    let row = block();
    let source = source();
    let body = "nika: total\ntasks:\n  sum:\n    exec: echo 1\n";
    let text = entry_text(
        &row,
        &[("src:engine", Some(&source)), ("src:gone", None)],
        Some(("blocks/total.nika", body)),
    );
    assert!(
        text.starts_with("block block:total — Total a column\n"),
        "{text}"
    );
    assert!(
        text.contains("role: component — an executable checked block, reused by instantiating it"),
        "{text}"
    );
    assert!(
        text.contains(
            "src:engine (Nika engine examples; licence AGPL-3.0-or-later; project; upstream none)"
        ),
        "{text}"
    );
    assert!(text.contains("src:gone (not in this release)"), "{text}");
    assert!(text.contains("activity: extracted"), "{text}");
    assert!(
        text.contains("trust machine-verified · status QUALIFIED · proof CHECKED"),
        "{text}"
    );
    // The whole contract, exactly as admitted (RFC 8785 numbers), and the whole body.
    assert!(
        text.contains(&format!("```json\n{}\n```", jcs_json(&row))),
        "{text}"
    );
    assert!(text.contains(r#""confidence":0.85"#), "{text}");
    assert!(text.ends_with(&format!(
        "blocks/total.nika:\n```yaml\n{}\n```",
        body.trim_end()
    )));
}

#[test]
fn a_counterexample_is_a_boundary_never_a_component() {
    let row = json!({"id": "counterexample:total:R2F:1", "kind": "counterexample",
                     "title": "Total without validation", "difference": "nika:validate absent"});
    let text = entry_text(&row, &[], None);
    assert_eq!(role(&row), Some("boundary"));
    assert!(
        text.contains(
            "role: boundary — what fails and why, to be avoided; never a component, never reused"
        ),
        "{text}"
    );
    assert_eq!(role(&block()), Some("component"));
    assert_eq!(role(&json!({"kind": "skill"})), Some("method"));
    assert_eq!(role(&json!({"kind": "callable"})), Some("contract"));
    assert_eq!(role(&json!({"kind": "unknown"})), None);
}

#[test]
fn a_body_holding_fences_is_fenced_longer() {
    let row = json!({"id": "skill:aggregate", "kind": "skill", "title": "Aggregate"});
    let body = "Use this:\n```yaml\nnika: x\n```\n";
    let text = entry_text(&row, &[], Some(("skills/aggregate.md", body)));
    assert!(
        text.ends_with(&format!(
            "skills/aggregate.md:\n````markdown\n{}\n````",
            body.trim_end()
        )),
        "{text}"
    );
    assert_eq!(
        sources_line(&block()).as_deref(),
        Some("sources: src:engine, src:gone")
    );
    assert_eq!(sources_line(&row), None);
}
