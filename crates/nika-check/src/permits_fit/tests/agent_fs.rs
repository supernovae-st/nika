// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;

fn escapes(tool: &str, fs: &str, granted: bool) -> Vec<CapabilityEscape> {
    let grants = if granted {
        serde_json::json!([tool])
    } else {
        serde_json::json!([])
    };
    let tools = serde_json::json!([tool]);
    let yaml = format!(
        "nika: agent-fs\npermits:\n  tools: {grants}\n{fs}\ntasks:\n  work:\n    agent:\n      prompt: work\n      tools: {tools}\n"
    );
    scan_escapes(&parse(&yaml, FileId::new(0), ParseMode::Strict).expect("fixture"))
}

#[test]
fn granted_agent_fs_tools_cannot_use_an_empty_direction() {
    for (tool, missing, allowed) in [
        ("nika:read", "fs.read", "  fs: {read: [data/**]}"),
        ("nika:grep", "fs.read", "  fs: {read: [data/**]}"),
        ("nika:glob", "fs.read", "  fs: {read: [data/**]}"),
        ("nika:write", "fs.write", "  fs: {write: [out/**]}"),
    ] {
        for fs in ["", "  fs: {}", "  fs: {read: [], write: []}"] {
            let findings = escapes(tool, fs, true);
            assert_eq!(findings.len(), 1, "{tool} / {fs}: {findings:?}");
            assert!(findings[0].detail.contains(missing));
            let fix = findings[0].fix.as_deref().expect("two repair choices");
            assert!(fix.contains("grant") && fix.contains("remove"), "{fix}");
        }
        assert!(
            escapes(tool, allowed, true).is_empty(),
            "specific paths remain runtime judgment"
        );
    }
}

#[test]
fn agent_fs_directions_are_independent_and_other_tool_rules_stay_owned() {
    assert_eq!(escapes("nika:edit", "", true).len(), 2);
    assert_eq!(
        escapes("nika:edit", "  fs: {read: [data/**]}", true).len(),
        1
    );
    assert_eq!(
        escapes("nika:edit", "  fs: {write: [data/**]}", true).len(),
        1
    );
    assert!(
        escapes(
            "nika:edit",
            "  fs: {read: [data/**], write: [data/**]}",
            true
        )
        .is_empty()
    );
    let ungranted = escapes("nika:read", "", false);
    assert_eq!(ungranted.len(), 1);
    assert_eq!(
        ungranted[0].category, "tools",
        "do not double-count an ungranted tool"
    );
    for tool in ["nika:jq", "nika:done", "nika:decide", "mcp:files/read"] {
        assert!(
            escapes(tool, "", true).is_empty(),
            "{tool}: no unconditional local fs effect"
        );
    }
}

#[test]
fn agent_media_tools_require_only_their_unconditional_fs_directions() {
    assert_eq!(
        escapes("nika:image_fx", "  fs: {write: [out/**]}", true).len(),
        1
    );
    assert_eq!(
        escapes("nika:image_fx", "  fs: {read: [in/**]}", true).len(),
        1
    );
    assert!(
        escapes(
            "nika:image_fx",
            "  fs: {read: [in/**], write: [out/**]}",
            true
        )
        .is_empty()
    );
    for tool in ["nika:chart", "nika:image_generate", "nika:tts_generate"] {
        assert_eq!(escapes(tool, "", true).len(), 1);
        assert!(escapes(tool, "  fs: {write: [out/**]}", true).is_empty());
    }
}
