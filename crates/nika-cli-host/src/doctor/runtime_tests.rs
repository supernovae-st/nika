// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[cfg(feature = "access-harness")]
#[test]
fn doctor_lists_every_agentic_cli_runtime() {
    let findings = super::harness_findings();
    assert_eq!(findings.len(), 5, "{findings:?}");
    let text: String = findings
        .iter()
        .map(|f| format!("{} {}", f.label, f.detail))
        .collect::<Vec<_>>()
        .join("\n");
    for token in [
        "claude-code",
        "codex",
        "gemini-cli",
        "kimi-code",
        "qwen-code",
    ] {
        assert!(text.contains(token), "missing {token} in:\n{text}");
    }
    assert!(
        findings.iter().all(|f| f.label == "runtime"),
        "ACP runtimes must not reuse the MCP-wire `agent` label: {text}"
    );
    assert!(
        !text.contains("Nika MCP oracle"),
        "runtime rows must not market MCP wire: {text}"
    );
}
