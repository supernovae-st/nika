// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! #1307/#1255: execute the shipped lessons under the real OS sandbox.
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

struct Rig {
    _root: tempfile::TempDir,
    home: PathBuf,
    work: PathBuf,
}

impl Rig {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated room");
        let home = root.path().join("home");
        let work = root.path().join("workspace with spaces");
        std::fs::create_dir(&home).expect("home");
        std::fs::create_dir(&work).expect("workspace");
        Self {
            _root: root,
            home,
            work,
        }
    }

    fn nika(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_nika"))
            .args(args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", &self.home)
            .env("TMPDIR", &self.home)
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .env("TERM", "dumb")
            .current_dir(&self.work)
            .stdin(Stdio::null())
            .output()
            .expect("real CLI executes")
    }

    fn run(&self, yaml: &str) -> Output {
        std::fs::write(self.work.join("lesson.nika.yaml"), yaml).expect("lesson");
        let checked = self.nika(&["check", "lesson.nika.yaml", "--json", "--native-strict"]);
        assert!(checked.status.success(), "{}", transcript(&checked));
        self.nika(&[
            "run",
            "lesson.nika.yaml",
            "--model",
            "mock/echo",
            "--output",
            "json",
        ])
    }

    fn verified_events(&self) -> Vec<Value> {
        let traces: Vec<_> = std::fs::read_dir(self.work.join(".nika/traces"))
            .expect("journal directory")
            .map(|e| e.expect("entry").path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "ndjson"))
            .collect();
        assert_eq!(traces.len(), 1, "one real run journal");
        let verified = self.nika(&[
            "trace",
            "verify",
            traces[0].to_str().expect("path"),
            "--json",
        ]);
        assert!(verified.status.success(), "{}", transcript(&verified));
        std::fs::read_to_string(&traces[0])
            .expect("journal")
            .lines()
            .map(|line| serde_json::from_str(line).expect("event JSON"))
            .collect()
    }
}

fn transcript(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn field<'a>(event: &'a Value, key: &str) -> Option<&'a Value> {
    event["fields"]
        .as_array()?
        .iter()
        .find(|f| f["key"] == key)
        .map(|f| &f["value"])
}

fn has_task(events: &[Value], kind: &str, task: &str) -> bool {
    events
        .iter()
        .any(|e| e["kind"] == kind && field(e, "task") == Some(&Value::from(task)))
}

fn cleanup_succeeded(events: &[Value]) -> bool {
    events.iter().any(|e| {
        e["kind"] == "permit_checked"
            && field(e, "task") == Some(&Value::from("test"))
            && field(e, "plane") == Some(&Value::from("on_finally"))
            && field(e, "decision") == Some(&Value::from("success"))
    })
}

#[test]
fn try_lessons_succeed_without_cargo_or_a_git_repository() {
    for slug in ["03-exec-pipeline", "snippets/run", "standup-digest"] {
        let rig = Rig::new();
        let out = rig.nika(&["try", slug, "--model", "mock/echo", "--no-progress"]);
        assert!(out.status.success(), "{slug}: {}", transcript(&out));
        assert!(!transcript(&out).contains("try sandbox has no"));
        assert!(!rig.work.join(".build.lock").exists());
        assert!(
            !rig.work.join("out").exists(),
            "try effects remain in its room"
        );
    }
}

fn pipeline(exit_code: i32) {
    let rig = Rig::new();
    let yaml = nika_pack::example("03-exec-pipeline")
        .expect("embedded")
        .replace("; exit 1", &format!("; exit {exit_code}"));
    let out = rig.run(&yaml);
    assert!(out.status.success(), "{}", transcript(&out));
    let values: Value = serde_json::from_slice(&out.stdout).expect("workflow outputs");
    assert_eq!(values["suite_exit_code"], exit_code);
    assert_eq!(values["task_status"], "success");
    let events = rig.verified_events();
    assert!(cleanup_succeeded(&events));
    let (deploy, notice) = if exit_code == 0 {
        ("task_completed", "task_cancelled")
    } else {
        ("task_skipped", "task_completed")
    };
    assert!(has_task(&events, deploy, "deploy"));
    assert!(has_task(&events, notice, "no_ship_notice"));
}

#[test]
fn red_sample_suite_closes_deploy_and_runs_cleanup() {
    pipeline(1);
}

#[test]
fn green_sample_suite_opens_deploy_and_runs_cleanup() {
    pipeline(0);
}

#[test]
fn structured_capture_preserves_denial_without_exposing_file_contents() {
    let rig = Rig::new();
    std::fs::write(rig.work.join("denied.txt"), "unreadable-fixture-canary")
        .expect("denied fixture");
    let yaml = nika_pack::example("03-exec-pipeline").expect("embedded");
    let mut doc: Value = serde_yaml_bw::from_str(yaml).expect("YAML");
    doc["tasks"]["test"]["exec"]["shell"] = "cat denied.txt".into();
    // The deliberate cat probe earns a native-first hint; ordinary check
    // admits the workflow so the OS, rather than the static linter, judges it.
    std::fs::write(
        rig.work.join("lesson.nika.yaml"),
        serde_yaml_bw::to_string(&doc).expect("YAML"),
    )
    .expect("negative probe");
    let check = rig.nika(&["check", "lesson.nika.yaml", "--json"]);
    assert!(check.status.success(), "{}", transcript(&check));
    let out = rig.nika(&[
        "run",
        "lesson.nika.yaml",
        "--model",
        "mock/echo",
        "--output",
        "json",
    ]);
    let events = rig.verified_events();
    if cfg!(target_os = "macos") {
        // Seatbelt returns EPERM: an authority refusal cannot become data.
        assert_eq!(out.status.code(), Some(1), "{}", transcript(&out));
        assert!(has_task(&events, "task_failed", "test"));
        assert!(transcript(&out).contains("NIKA-SEC-001"));
    } else {
        // bwrap leaves ungranted paths unmounted. ENOENT is indistinguishable
        // from an authored missing file and lawfully remains process data.
        assert!(out.status.success(), "{}", transcript(&out));
        let values: Value = serde_json::from_slice(&out.stdout).expect("outputs");
        assert_eq!(values["suite_exit_code"], 1);
        assert!(has_task(&events, "task_completed", "test"));
    }
    assert!(!transcript(&out).contains("unreadable-fixture-canary"));
    assert!(!has_task(&events, "task_completed", "deploy"));
    assert!(cleanup_succeeded(&events));
}

#[test]
fn standup_preserves_unavailable_history_in_outputs_and_artifact() {
    let rig = Rig::new();
    let out = rig.run(nika_pack::example("standup-digest").expect("embedded"));
    assert!(out.status.success(), "{}", transcript(&out));
    let values: Value = serde_json::from_slice(&out.stdout).expect("workflow outputs");
    assert!(
        values["history"]
            .as_str()
            .expect("history")
            .starts_with("Git history unavailable:")
    );
    let note = values["note"].as_str().expect("note");
    assert!(note.contains("No commit facts were collected."));
    assert_eq!(
        std::fs::read_to_string(rig.work.join("out/standup-note.md")).expect("note artifact"),
        note
    );
    let events = rig.verified_events();
    assert!(has_task(&events, "task_recovered", "history"));
    assert!(has_task(&events, "task_completed", "save"));
}

#[test]
fn argv_starter_returns_the_real_program_output() {
    let rig = Rig::new();
    let out = rig.run(nika_pack::example("snippets/run").expect("embedded"));
    assert!(out.status.success(), "{}", transcript(&out));
    let values: Value = serde_json::from_slice(&out.stdout).expect("workflow outputs");
    assert_eq!(values["message"], "Hello from a confined program");
    assert!(has_task(&rig.verified_events(), "task_completed", "hello"));
}
