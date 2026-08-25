// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[test]
fn doctor_teaches_access_classes_and_live_seat_pins() {
    let detail = &super::access_class_finding().detail;
    assert!(detail.contains("--access classes:"), "{detail}");
    assert!(
        detail.contains("harness seats: claude-code · codex"),
        "{detail}"
    );
    assert!(detail.contains("ACP wrapper ids are not pins"), "{detail}");
}

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

#[cfg(feature = "access-harness")]
#[test]
fn codex_without_acp_still_names_the_direct_infer_path() {
    let finding = super::harness_finding_from_parts("codex", None, None, "codex-acp package", true);

    assert!(
        finding.detail.contains("infer-grade direct path detected"),
        "{}",
        finding.detail
    );
    assert!(
        finding.detail.contains("login judged at run"),
        "{}",
        finding.detail
    );
    assert!(
        finding.detail.contains("agent ACP speaker missing"),
        "{}",
        finding.detail
    );
    assert!(
        finding
            .fix
            .as_deref()
            .is_some_and(|fix| fix.contains("only required for agent:")),
        "{:?}",
        finding.fix
    );
}
