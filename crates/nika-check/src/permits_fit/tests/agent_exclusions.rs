// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Negative whitelist rules restrict tools; they never demand authority.

use nika_schema::parser::{ParseMode, parse};
use nika_schema::raw::RawWorkflow;
use nika_schema::source::FileId;

fn workflow(boundary: &str, tools: &[&str]) -> RawWorkflow {
    let tools = serde_json::to_string(tools).expect("serialize fixture tools");
    let yaml = format!(
        "nika: agent-exclusions\nmodel: mock/text\n{boundary}\ntasks:\n  work:\n    agent:\n      prompt: complete\n      tools: {tools}\n"
    );
    parse(&yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
}

#[test]
fn exclusions_alone_need_no_authority_under_any_boundary_form() {
    for boundary in ["", "permits: {}", "permits: { tools: [] }"] {
        for exclusion in [
            "!nika:done",
            "!nika:read",
            "!nika:*",
            "!mcp:browser/navigate",
        ] {
            let wf = workflow(boundary, &[exclusion]);
            let report = crate::check(&wf);
            assert!(report.capability_escapes.is_empty(), "{report:?}");
            assert!(crate::infer_permits(&wf).permits.tools.is_none());
            assert!(crate::permits_infer::task_permits(&wf.tasks[0].value).is_empty());
        }
    }
}

#[test]
fn exclusions_never_become_inferred_or_missing_grants() {
    for (grant, excluded) in [
        ("nika:*", "!nika:done"),
        ("mcp:browser/*", "!mcp:browser/navigate"),
    ] {
        let boundary = format!("permits: {{ tools: [\"{grant}\"] }}");
        for tools in [vec![grant, excluded], vec![excluded, grant]] {
            let wf = workflow(&boundary, &tools);
            let report = crate::check(&wf);
            assert!(report.capability_escapes.is_empty(), "{report:?}");
            assert_eq!(
                crate::infer_permits(&wf).permits.tools,
                Some(vec![grant.to_owned()])
            );
            assert_eq!(
                crate::permits_infer::task_permits(&wf.tasks[0].value),
                vec![format!("tool: {grant}")]
            );
        }
    }
}

#[test]
fn an_exclusion_does_not_grant_a_positive_tool() {
    let wf = workflow("permits: {}", &["nika:write", "!nika:done"]);
    let report = crate::check(&wf);
    assert_eq!(report.capability_escapes.len(), 1, "{report:?}");
    assert!(report.capability_escapes[0].detail.contains("nika:write"));
    assert_eq!(
        crate::infer_permits(&wf).permits.tools,
        Some(vec!["nika:write".to_owned()])
    );
}
