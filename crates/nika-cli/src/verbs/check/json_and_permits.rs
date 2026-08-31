// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Night-gauntlet pins that do not live in `tests/verdict_profiles.rs`.
//! Overlapping JSON / native-strict / identity tests moved with 0.116.0.

use super::*;

#[test]
fn infer_permits_on_a_red_file_is_not_exit_0() {
    // B15: a file with findings must not look paste-ready-and-green.
    let dir = std::env::temp_dir().join(format!("nika-b15-red-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("red.nika.yaml");
    std::fs::write(
        &path,
        "nika: red\npermits: { exec: [\"echo\"] }\ntasks:\n  t:\n    after: { ghost: success }\n    exec: { command: [\"echo\", \"hi\"] }\n",
    )
    .expect("fixture");
    let out = run_infer_permits(path.to_str().expect("utf8"), false);
    assert_ne!(out.code, 0, "B15 red file rc≠0: {}", out.text);
    assert!(
        out.text.contains("exec:") && !out.text.contains("exec: true"),
        "B15 exec infers the binary, not true: {}",
        out.text
    );
    assert!(
        out.text.contains("echo"),
        "B15 names the argv program: {}",
        out.text
    );
}

#[test]
fn infer_permits_shell_form_names_sh_not_true() {
    let dir = std::env::temp_dir().join(format!("nika-b15-sh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("shell.nika.yaml");
    std::fs::write(
        &path,
        "nika: sh\npermits: { exec: true }\ntasks:\n  t:\n    exec: { shell: \"echo hi\" }\n",
    )
    .expect("fixture");
    let out = run_infer_permits(path.to_str().expect("utf8"), false);
    assert!(
        !out.text.contains("exec: true"),
        "B15/#1279 never paste exec: true: {}",
        out.text
    );
    assert!(
        out.text.contains("exec: [\"sh\"]") || out.text.contains("exec: [\"echo\"]"),
        "B15 infers the binary: {}",
        out.text
    );
}

/// Persona 03: TTY used to stamp `NIKA-BUILTIN-001` on a ghost
/// `mcp:spotify/search` while `--json` carried `NIKA-INVOKE-001`.
#[test]
fn ghost_mcp_tool_uses_the_same_code_on_tty_and_json() {
    let dir = std::env::temp_dir().join(format!("nika-mcp-ghost-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("spotify.nika.yaml");
    std::fs::write(
        &path,
        "nika: spotify-search\npermits:\n  tools: [\"mcp:spotify/search\"]\ntasks:\n  s:\n    invoke:\n      tool: mcp:spotify/search\n      args: { q: x }\n",
    )
    .expect("fixture");
    let file = path.to_str().expect("utf8");
    let theme = Theme::new(false, true, false);
    let human = run(file, false, false, None, theme);
    let json = run(file, true, false, None, theme);
    assert!(
        human.text.contains("NIKA-INVOKE-001"),
        "TTY must print JSON's invoke code: {}",
        human.text
    );
    assert!(
        !human.text.contains("NIKA-BUILTIN-001"),
        "a ghost MCP server is not a builtin miss: {}",
        human.text
    );
    assert!(
        json.text.contains("NIKA-INVOKE-001"),
        "JSON must keep INVOKE-001: {}",
        json.text
    );
}
