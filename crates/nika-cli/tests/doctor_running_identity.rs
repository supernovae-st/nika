// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This test's job is to execute the real binary under a PATH it builds, the
// carve-out class of bin_smoke.rs.
#![allow(clippy::disallowed_types)]

//! #1614 · `nika doctor` and `nika welcome` name which `nika` is running and
//! which other `nika` sits on PATH. Before, a shadowed install read `11 ok`:
//! no path, no installer, no other copy. Each case runs the built binary with
//! an empty environment and a PATH of stub scripts beside a link to itself.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// A scratch machine: a home, a link to the built binary in `self/`, and
/// stub copies in their own directories.
struct Machine {
    root: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("scratch");
        std::fs::create_dir(root.path().join("home")).expect("home");
        let me = root.path().join("self");
        std::fs::create_dir(&me).expect("self");
        std::os::unix::fs::symlink(bin(), me.join("nika")).expect("link to the built binary");
        Self { root }
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// `<root>/<name>/nika`, a `/bin/sh` script of `body`, mode `mode`.
    fn stub(&self, name: &str, body: &str, mode: u32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = self.dir(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("nika");
        std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("stub");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).expect("chmod");
        dir
    }

    fn run(&self, path: &[PathBuf], args: &[&str]) -> Output {
        Command::new(bin())
            .args(args)
            .env_clear()
            .env("HOME", self.dir("home"))
            .env("PATH", std::env::join_paths(path).expect("PATH"))
            .env("NIKA_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .current_dir(self.dir("home"))
            .output()
            .expect("the built binary runs")
    }

    fn json(&self, path: &[PathBuf], args: &[&str]) -> serde_json::Value {
        let out = self.run(path, args);
        assert!(out.status.success(), "{out:?}");
        serde_json::from_slice(&out.stdout).expect("one JSON document")
    }

    fn text(&self, path: &[PathBuf], args: &[&str]) -> String {
        let out = self.run(path, args);
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).expect("UTF-8")
    }
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

fn resolved_bin() -> String {
    std::fs::canonicalize(bin())
        .expect("resolved binary")
        .display()
        .to_string()
}

/// What the built binary says of itself: `nika <version> (<commit>)`.
fn own_version_line() -> String {
    let out = Command::new(bin())
        .arg("--version")
        .output()
        .expect("--version");
    String::from_utf8(out.stdout)
        .expect("UTF-8")
        .trim()
        .to_owned()
}

/// The doctor rows whose label cell is `label`.
fn rows<'a>(text: &'a str, label: &str) -> Vec<&'a str> {
    text.lines()
        .filter(|l| l.split_whitespace().nth(1) == Some(label))
        .collect()
}

#[test]
fn doctor_names_the_running_binary_and_the_copy_that_shadows_it() {
    let m = Machine::new();
    let old = m.stub("old", "echo 'nika 0.1.0 (0000000aa)'", 0o755);
    let path = [old.clone(), m.dir("self")];

    let v = m.json(&path, &["doctor", "--json"]);
    let running = &v["running"];
    assert_eq!(running["path"], resolved_bin().as_str(), "{running:#}");
    assert_eq!(running["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(running["on_path"], true);
    assert_eq!(running["first_on_path"], false);
    let others = v["others"].as_array().expect("others[]");
    assert_eq!(others.len(), 1, "{others:#?}");
    assert_eq!(others[0]["path"], old.join("nika").display().to_string());
    assert_eq!(others[0]["version"], "0.1.0");
    assert_eq!(others[0]["commit"], "0000000aa");
    assert_eq!(others[0]["first_on_path"], true);
    assert_eq!(others[0]["shadows_running"], true);
    assert_eq!(others[0]["same_build"], false);
    assert!(
        v["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .any(|f| f["label"] == "path" && f["level"] == "warn"),
        "{v:#}"
    );

    let text = m.text(&path, &["doctor"]);
    let first = rows(&text, "path");
    assert_eq!(first.len(), 1, "{text}");
    assert!(first[0].starts_with('⚠'), "{text}");
    assert!(
        first[0].contains(&format!(
            "typing `nika` runs {} (nika 0.1.0 (0000000aa)",
            old.join("nika").display()
        )),
        "{text}"
    );
    let running_rows = rows(&text, "running");
    assert_eq!(running_rows.len(), 1, "{text}");
    assert!(running_rows[0].contains(&resolved_bin()), "{text}");
}

#[test]
fn doctor_stays_calm_when_this_binary_runs_first_and_the_copy_is_the_same_build() {
    let m = Machine::new();
    let twin = m.stub("twin", &format!("echo '{}'", own_version_line()), 0o755);
    let path = [m.dir("self"), twin];
    let v = m.json(&path, &["doctor", "--json"]);
    assert_eq!(v["running"]["first_on_path"], true, "{:#}", v["running"]);
    assert_eq!(v["others"][0]["same_build"], true, "{:#}", v["others"]);
    assert_eq!(v["others"][0]["running_shadows_this"], true);
    let text = m.text(&path, &["doctor"]);
    let path_rows = rows(&text, "path");
    assert_eq!(path_rows.len(), 2, "{text}");
    assert!(path_rows.iter().all(|r| r.starts_with('✔')), "{text}");
    assert!(
        path_rows[0].contains("this binary is the first `nika` on PATH"),
        "{text}"
    );
}

#[test]
fn doctor_warns_about_another_version_behind_this_binary() {
    let m = Machine::new();
    let old = m.stub("old", "echo 'nika 0.1.0'", 0o755);
    let path = [m.dir("self"), old.clone()];
    let v = m.json(&path, &["doctor", "--json"]);
    assert_eq!(v["others"][0]["running_shadows_this"], true);
    assert_eq!(v["others"][0]["order"], "older");
    let text = m.text(&path, &["doctor"]);
    let behind = text
        .lines()
        .position(|l| l.contains("shadowed by this binary"))
        .expect("the other copy is listed");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[behind].starts_with('⚠'), "{text}");
    assert_eq!(
        lines.get(behind + 1).copied(),
        Some(
            format!(
                "  fix: rm {} && hash -r   # the older copy",
                old.join("nika").display()
            )
            .as_str()
        ),
        "{text}"
    );
}

#[test]
fn a_hanging_copy_is_reported_and_never_fatal() {
    let m = Machine::new();
    let hang = m.stub("hang", "exec /bin/sleep 60", 0o755);
    let path = [m.dir("self"), hang];
    let started = Instant::now();
    let v = m.json(&path, &["doctor", "--json"]);
    assert!(
        started.elapsed() < Duration::from_secs(45),
        "the version probe is bounded: {:?}",
        started.elapsed()
    );
    assert_eq!(v["others"][0]["probe"], "timed_out", "{:#}", v["others"]);
    assert_eq!(v["others"][0]["version"], serde_json::Value::Null);
}

#[test]
fn a_non_executable_copy_is_listed_and_never_run() {
    let m = Machine::new();
    let marker = m.dir("home").join("ran");
    let stray = m.stub(
        "stray",
        &format!("touch '{}'; echo 'nika 0.1.0'", marker.display()),
        0o644,
    );
    let path = [stray, m.dir("self")];
    let v = m.json(&path, &["doctor", "--json"]);
    assert_eq!(
        v["others"][0]["probe"], "not_executable",
        "{:#}",
        v["others"]
    );
    assert_eq!(v["others"][0]["first_on_path"], false);
    assert_eq!(v["running"]["first_on_path"], true);
    assert!(!marker.exists(), "a non-executable file is never run");
}

#[test]
fn welcome_says_the_same_facts_under_this_binary() {
    let m = Machine::new();
    let old = m.stub("old", "echo 'nika 0.1.0 (0000000aa)'", 0o755);
    let path = [old.clone(), m.dir("self")];
    let v = m.json(&path, &["welcome", "--json"]);
    assert_eq!(v["welcome_version"], 1);
    assert_eq!(v["running"]["path"], resolved_bin().as_str());
    assert_eq!(v["others"][0]["first_on_path"], true);

    let text = m.text(&path, &["welcome"]);
    let lines: Vec<&str> = text.lines().collect();
    let at = lines
        .iter()
        .position(|l| *l == "this binary")
        .expect("the this-binary section");
    let line = lines.get(at + 1).copied().unwrap_or_default();
    assert!(
        line.starts_with(&format!(
            "  {} · installer: ",
            own_version_line().replacen("nika ", "v", 1)
        )),
        "{text}"
    );
    assert!(line.ends_with(&resolved_bin()), "{text}");
    if is_target_build() {
        assert!(line.contains(" · installer: source-build · "), "{text}");
    }
    assert_eq!(
        lines.get(at + 2).copied(),
        Some(
            format!(
                "  ⚠ `nika` runs another build, {} (nika 0.1.0 (0000000aa)) → nika doctor",
                old.join("nika").display()
            )
            .as_str()
        ),
        "{text}"
    );
}

/// The binary under test sits in a cargo `target/{debug,release}` directory.
fn is_target_build() -> bool {
    let resolved = PathBuf::from(resolved_bin());
    matches!(
        resolved
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str()),
        Some("debug" | "release")
    ) && resolved.components().any(|c| c.as_os_str() == "target")
}
