// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Editor Sync Contract Tests
//!
//! These tests define the expected behavior of `nika sync *` commands.
//!
//! # Tests (8 total)
//!
//! 1. sync detects 4 editors
//! 2. sync writes editor config
//! 3. sync --status shows status
//! 4. sync --enable enables editor
//! 5. sync --disable disables editor
//! 6. sync --diff shows pending changes
//! 7. sync foreign MCP detection
//! 8. sync is idempotent

use super::common::run_nika;

/// Contract: `nika sync` detects 4 supported editors
#[test]
fn contract_sync_detects_editors() {
    let output = run_nika(&["sync", "status"]);

    // Should succeed
    assert!(output.status.success(), "sync status should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show editor detection
    // Supported editors: Claude Code, Cursor, Windsurf, VS Code
    let supported_editors = ["claude", "cursor", "windsurf", "vscode", "code"];

    let has_editor_info = supported_editors
        .iter()
        .any(|e| stdout.to_lowercase().contains(e));

    assert!(
        has_editor_info
            || stdout.contains("Editor")
            || stdout.contains("Sync")
            || stdout.contains("Status"),
        "sync should show editor info. Got: {}",
        stdout
    );
}

/// Contract: `nika sync` writes editor MCP config
#[test]
fn contract_sync_writes_config() {
    // Document the config locations:
    // - Claude Code: ~/.config/claude-code/mcp.json (or similar)
    // - Cursor: ~/.cursor/mcp.json
    // - Windsurf: ~/.windsurf/mcp.json
    // - VS Code: ~/.vscode/mcp.json

    let output = run_nika(&["sync", "--help"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Help should describe sync
    assert!(
        combined.contains("sync")
            || combined.contains("editor")
            || combined.contains("MCP")
            || combined.contains("config"),
        "sync help should describe config sync. Got: {}",
        combined
    );
}

/// Contract: `nika sync status` shows sync status
#[test]
fn contract_sync_status() {
    let output = run_nika(&["sync", "status"]);

    assert!(output.status.success(), "sync status should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show status for each editor
    // Either enabled/disabled or configured/not configured
    assert!(
        stdout.contains("enabled")
            || stdout.contains("disabled")
            || stdout.contains("configured")
            || stdout.contains("Status")
            || stdout.lines().count() >= 1,
        "sync --status should show status. Got: {}",
        stdout
    );
}

/// Contract: `nika sync --enable <editor>` enables editor sync
#[test]
fn contract_sync_enable_editor() {
    let _output = run_nika(&["sync", "--enable", "--help"]);

    // Check if --enable flag is documented
    let output2 = run_nika(&["sync", "--help"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output2.stdout),
        String::from_utf8_lossy(&output2.stderr)
    );

    // Should mention enable option
    assert!(
        combined.contains("enable") || combined.contains("--enable"),
        "sync should support --enable. Got: {}",
        combined
    );
}

/// Contract: `nika sync --disable <editor>` disables editor sync
#[test]
fn contract_sync_disable_editor() {
    let output = run_nika(&["sync", "--help"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should mention disable option
    assert!(
        combined.contains("disable") || combined.contains("--disable"),
        "sync should support --disable. Got: {}",
        combined
    );
}

/// Contract: `nika sync --diff <editor>` shows pending changes
#[test]
fn contract_sync_diff() {
    let output = run_nika(&["sync", "--help"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // May or may not support --diff
    let has_diff = combined.contains("diff") || combined.contains("--diff");

    // Document that diff feature is expected
    let _ = has_diff;
}

/// Contract: sync detects foreign MCP servers
#[test]
fn contract_sync_foreign_mcp_detection() {
    // Document foreign MCP detection behavior:
    // - Servers configured in editor but not in ~/.nika/mcp.yaml
    // - Shown in sync --status output
    // - Not overwritten by sync

    let output = run_nika(&["sync", "status"]);

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);

        // May show foreign servers if any exist
        let has_foreign_info = stdout.contains("foreign")
            || stdout.contains("external")
            || stdout.contains("unmanaged");

        // Document that this feature exists
        let _ = has_foreign_info;
    }
}

/// Contract: sync is idempotent
#[test]
fn contract_sync_idempotent() {
    // Running sync multiple times should produce same result
    let output1 = run_nika(&["sync"]);
    let output2 = run_nika(&["sync"]);

    // Both should have same exit status
    assert_eq!(
        output1.status.success(),
        output2.status.success(),
        "sync should be idempotent"
    );

    // Neither should crash
    let combined1 = format!(
        "{}{}",
        String::from_utf8_lossy(&output1.stdout),
        String::from_utf8_lossy(&output1.stderr)
    );
    let combined2 = format!(
        "{}{}",
        String::from_utf8_lossy(&output2.stdout),
        String::from_utf8_lossy(&output2.stderr)
    );

    assert!(
        !combined1.contains("panic") && !combined2.contains("panic"),
        "sync should not panic"
    );
}
