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

/// R4 — the per-seat fix used to teach `install: @zed-industries/
/// claude-agent-acp@0.23.1` on a machine WITHOUT Claude Code: the
/// operator installed the ACP WRAPPER (never the app), tried it as
/// `--access claude-agent-acp`, and ate NIKA-1802 « retired ». Every
/// seat fix now names the LIVE pin and the gesture; the wrapper id is
/// disambiguated where the package itself must be named (the adapter
/// install), and absent where it was the whole lie (app not installed).
#[cfg(feature = "access-harness")]
#[test]
fn the_seat_fix_names_the_live_pin_never_teaches_the_wrapper_as_a_pin() {
    let package =
        "@zed-industries/claude-agent-acp@0.23.1 (npm i -g · wraps the claude CLI's own auth)";
    // Not installed: the fix teaches installing the APP, and names the
    // pin — the wrapper string does not appear at all.
    let absent = super::harness_finding_from_parts("claude-code", None, None, package, false);
    let fix = absent.fix.as_deref().expect("a fix");
    assert!(fix.contains("--access claude-code"), "{fix}");
    assert!(
        !fix.contains("claude-agent-acp"),
        "a machine without the app was told to install the wrapper: {fix}"
    );
    // App installed, adapter missing: the package IS the adapter, so it
    // is named — beside the explicit « never the pin » clause and the
    // live token.
    let no_adapter = super::harness_finding_from_parts("claude-code", None, None, package, true);
    let fix = no_adapter.fix.as_deref().expect("a fix");
    assert!(fix.contains("--access claude-code"), "{fix}");
    assert!(fix.contains("never the pin"), "{fix}");
    // Not signed in: the gesture, then the pin.
    let unsigned =
        super::harness_finding_from_parts("claude-code", Some((0, 23)), Some(false), package, true);
    let fix = unsigned.fix.as_deref().expect("a fix");
    assert!(fix.contains("sign in to Claude Code itself"), "{fix}");
    assert!(fix.contains("--access claude-code"), "{fix}");
    // The authenticated seat teaches nothing.
    let ready =
        super::harness_finding_from_parts("claude-code", Some((0, 23)), Some(true), package, true);
    assert_eq!(ready.fix, None);
}
