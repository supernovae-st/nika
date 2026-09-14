// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Execute the generated authoring setup with the real binary and an empty HOME.
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

fn isolated(program: &Path, home: &Path, cwd: &Path) -> Command {
    let binary_dir = Path::new(env!("CARGO_BIN_EXE_nika"))
        .parent()
        .expect("binary directory");
    let path = std::env::join_paths([binary_dir, Path::new("/usr/bin"), Path::new("/bin")])
        .expect("executable search path");
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .env("NIKA_KEYCHAIN", "off")
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .current_dir(cwd)
        .stdin(Stdio::null());
    command
}

fn nika(home: &Path, cwd: &Path, args: &[&str]) -> Output {
    isolated(Path::new(env!("CARGO_BIN_EXE_nika")), home, cwd)
        .args(args)
        .output()
        .expect("nika executes")
}

fn transcript(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn init_preserves_detected_scope_and_reports_an_all_refusal() {
    let home = tempfile::tempdir().expect("isolated home");
    let project = home.path().join("project");
    std::fs::create_dir(&project).expect("project");
    let detected = nika(
        home.path(),
        &project,
        &["init", "--yes", "--wire", "detected"],
    );
    assert_eq!(detected.status.code(), Some(0), "{}", transcript(&detected));
    assert!(!transcript(&detected).contains("wire all rewrites"));
    assert!(
        !home.path().join(".codex").exists(),
        "no undetected home configuration"
    );

    let refused = nika(home.path(), &project, &["init", "--yes", "--wire", "all"]);
    assert_eq!(refused.status.code(), Some(3), "{}", transcript(&refused));
    assert!(transcript(&refused).contains("wire all --yes"));
    assert!(!transcript(&refused).contains("wired ·"));
    assert!(
        project.join("AGENTS.md").is_file(),
        "completed scaffold is retained"
    );
    assert!(!home.path().join(".codex").exists());
}

fn hook(
    home: &Path,
    project: &Path,
    cwd: &Path,
    command: &str,
    payload: &serde_json::Value,
) -> Output {
    let mut child = isolated(Path::new("/bin/sh"), home, cwd)
        .args(["-c", command])
        .env("CLAUDE_PROJECT_DIR", project)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("generated command starts");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("payload");
    child.wait_with_output().expect("hook completes")
}

#[test]
fn claude_hooks_execute_from_a_nested_directory_and_preserve_user_settings() {
    let home = tempfile::tempdir().expect("isolated home");
    let project = home.path().join("project with spaces");
    let nested = project.join("nested");
    std::fs::create_dir_all(&nested).expect("project");
    let init = nika(home.path(), &project, &["init", "--yes"]);
    assert!(init.status.success(), "{}", transcript(&init));
    let settings_path = project.join(".claude/settings.json");
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings_path).expect("settings"))
            .expect("valid settings");
    let command = |event: &str| {
        settings["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .expect("generated command")
    };
    let session = hook(
        home.path(),
        &project,
        &nested,
        command("SessionStart"),
        &serde_json::json!({"hook_event_name": "SessionStart", "cwd": project}),
    );
    assert!(session.status.success(), "{}", transcript(&session));
    let context: serde_json::Value =
        serde_json::from_slice(&session.stdout).expect("Claude envelope");
    assert_eq!(
        context["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
    assert!(
        context["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("context")
            .contains("nika")
    );

    let workflow = project.join("dirty.nika.yaml");
    std::fs::write(&workflow, "nika: dirty\npermits: {}\ntasks:\n  read:\n    invoke:\n      tool: nika:read\n      args: {path: secret.txt}\n").expect("negative workflow");
    let edit = hook(
        home.path(),
        &project,
        &nested,
        command("PostToolUse"),
        &serde_json::json!({"hook_event_name": "PostToolUse", "tool_input": {"file_path": workflow}, "cwd": project}),
    );
    assert_eq!(edit.status.code(), Some(2), "{}", transcript(&edit));
    assert!(transcript(&edit).contains("NIKA-SEC-"));
    let guard = hook(
        home.path(),
        &project,
        &nested,
        command("PreToolUse"),
        &serde_json::json!({"hook_event_name": "PreToolUse", "tool_input": {"command": "nika run dirty.nika.yaml"}, "cwd": project}),
    );
    assert!(
        guard.status.success(),
        "adapter carries the JSON verdict: {}",
        transcript(&guard)
    );
    let verdict: serde_json::Value = serde_json::from_slice(&guard.stdout).expect("guard envelope");
    assert_eq!(verdict["hookSpecificOutput"]["permissionDecision"], "deny");

    let own_settings = b"{\"env\":{\"TEAM_SETTING\":\"preserved\"}}\n";
    std::fs::write(&settings_path, own_settings).expect("user configuration");
    let again = nika(home.path(), &project, &["init", "--yes"]);
    assert!(again.status.success(), "{}", transcript(&again));
    assert_eq!(
        std::fs::read(&settings_path).expect("settings retained"),
        own_settings
    );
}

#[test]
fn human_guide_first_workflow_runs_offline_and_project_receipt_precedes_next() {
    let home = tempfile::tempdir().expect("isolated home");
    let project = home.path().join("project");
    std::fs::create_dir(&project).expect("project");
    std::fs::write(project.join("README.md"), "existing project readme\n").expect("readme");
    let init = nika(home.path(), &project, &["init", "--yes", "--project-file"]);
    assert!(init.status.success(), "{}", transcript(&init));
    let text = transcript(&init);
    // #1283 · the project file is laid by default: its receipt rides the
    // report with its purpose, before the `next ·` hand-off.
    let receipt = text
        .lines()
        .position(|line| line.starts_with("✔ created ") && line.contains("nika.yaml — "))
        .expect("the project-file receipt rides the report");
    let next = text
        .lines()
        .position(|line| line.starts_with("next ·"))
        .expect("the hand-off block");
    assert!(receipt < next, "{text}");
    assert_eq!(
        std::fs::read_to_string(project.join("README.md")).expect("readme retained"),
        "existing project readme\n"
    );
    let guide = std::fs::read_to_string(project.join("NIKA.md")).expect("human guide");
    let commands = guide
        .split("```sh\n")
        .nth(1)
        .expect("first commands")
        .split("```")
        .next()
        .expect("commands end");
    for command in commands.lines() {
        let args: Vec<_> = command.split_whitespace().skip(1).collect();
        let result = nika(home.path(), &project, &args);
        assert!(
            result.status.success(),
            "guide command {command}: {}",
            transcript(&result)
        );
    }
}
