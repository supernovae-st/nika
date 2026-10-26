// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! #1614 · the running binary and the other `nika` on PATH, observed over a
//! PATH made of stub scripts: a shadowing copy, the same build, another
//! version behind this binary, a hanging copy, a failing one, a file that is
//! not executable and a relative entry that must never run.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{
    Answer, BinaryIdentity, Build, Inputs, Installer, Places, collect_from, doctor_findings,
    insert_json, parse_version_output, version_order, welcome_lines,
};
use crate::doctor::Level;

/// The running binary's build in every scenario.
fn this_build() -> Build {
    Build {
        version: "0.124.0".to_owned(),
        commit: Some("aaaaaaaaa".to_owned()),
    }
}

/// A scratch machine: directories that hold one `nika` each, and the
/// running binary in its own directory.
struct Machine {
    root: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("scratch"),
        }
    }

    /// `<root>/<dir>/nika` running `body` under `/bin/sh`, mode `mode`.
    fn stub(&self, dir: &str, body: &str, mode: u32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = self.root.path().join(dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let file = dir.join("nika");
        std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("write stub");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).expect("chmod");
        dir
    }

    /// A copy that answers `--version` with `line`.
    fn answering(&self, dir: &str, line: &str) -> PathBuf {
        self.stub(dir, &format!("echo '{line}'"), 0o755)
    }

    /// The running binary's directory (its file is never executed here).
    fn running_dir(&self) -> PathBuf {
        self.stub("self", "echo 'never asked' && exit 9", 0o755)
    }

    fn running_path(&self) -> PathBuf {
        std::fs::canonicalize(self.root.path().join("self/nika")).expect("running path")
    }

    fn observe(&self, path: &[&PathBuf], deadline: Duration) -> BinaryIdentity {
        collect_from(Inputs {
            path_var: Some(std::env::join_paths(path).expect("PATH")),
            running: Ok(self.running_path()),
            build: this_build(),
            places: Places::default(),
            deadline,
        })
    }
}

const DEADLINE: Duration = Duration::from_secs(10);

fn json(id: &BinaryIdentity) -> serde_json::Value {
    let mut v = serde_json::json!({});
    insert_json(&mut v, id);
    v
}

fn path_rows(id: &BinaryIdentity) -> Vec<crate::doctor::Finding> {
    doctor_findings(id)
        .into_iter()
        .filter(|f| f.label == "path")
        .collect()
}

#[test]
fn a_shadowing_copy_first_on_path_warns_and_names_what_typing_nika_runs() {
    let m = Machine::new();
    let old = m.answering("old", "nika 0.122.0 (bbbbbbbbb)");
    let me = m.running_dir();
    let id = m.observe(&[&old, &me], DEADLINE);

    let rows = doctor_findings(&id);
    assert_eq!(rows[0].label, "running");
    assert_eq!(rows[0].level, Level::Ok);
    assert!(
        rows[0]
            .detail
            .starts_with(&m.running_path().display().to_string()),
        "{rows:#?}"
    );
    assert!(
        rows[0].detail.contains("nika 0.124.0 (aaaaaaaaa)"),
        "{rows:#?}"
    );
    let first = &rows[1];
    assert_eq!((first.label.as_str(), first.level), ("path", Level::Warn));
    assert_eq!(
        first.detail,
        format!(
            "typing `nika` runs {} (nika 0.122.0 (bbbbbbbbb) · installer: unknown), not this binary",
            old.join("nika").display()
        )
    );
    // This binary is newer: the older copy goes.
    assert_eq!(
        first.fix.as_deref(),
        Some(
            format!(
                "rm {} && hash -r   # the older copy runs first on PATH",
                old.join("nika").display()
            )
            .as_str()
        )
    );
    // The shadowing copy is the summary row, never listed twice.
    assert_eq!(rows.len(), 2, "{rows:#?}");

    let v = json(&id);
    assert_eq!(v["running"]["on_path"], true);
    assert_eq!(v["running"]["first_on_path"], false);
    assert_eq!(v["running"]["version"], "0.124.0");
    assert_eq!(v["running"]["commit"], "aaaaaaaaa");
    let other = &v["others"][0];
    assert_eq!(v["others"].as_array().map(Vec::len), Some(1));
    assert_eq!(other["first_on_path"], true);
    assert_eq!(other["shadows_running"], true);
    assert_eq!(other["running_shadows_this"], false);
    assert_eq!(other["probe"], "reported");
    assert_eq!(other["version"], "0.122.0");
    assert_eq!(other["commit"], "bbbbbbbbb");
    assert_eq!(other["same_build"], false);
    assert_eq!(other["order"], "older");
}

#[test]
fn the_same_build_through_another_file_stays_calm() {
    let m = Machine::new();
    let launcher = m.answering("launcher", "nika 0.124.0 (aaaaaaaaa)");
    let me = m.running_dir();
    let id = m.observe(&[&launcher, &me], DEADLINE);
    let rows = path_rows(&id);
    assert_eq!(rows.len(), 1, "{rows:#?}");
    assert_eq!(rows[0].level, Level::Ok);
    assert!(
        rows[0]
            .detail
            .contains("another file that reports this same build"),
        "{rows:#?}"
    );
    assert_eq!(json(&id)["others"][0]["same_build"], true);
    let (_, notice) = welcome_lines(&id, &|p: &Path| p.display().to_string());
    assert_eq!(notice, None, "the same build is no notice");
}

#[test]
fn another_version_behind_this_binary_warns_with_its_removal() {
    let m = Machine::new();
    let me = m.running_dir();
    let old = m.answering("old", "nika 0.123.2 (ccccccccc)");
    let newer = m.answering("newer", "nika 0.125.0 (ddddddddd)");
    let id = m.observe(&[&me, &old, &newer], DEADLINE);
    let rows = path_rows(&id);
    assert_eq!(rows[0].level, Level::Ok);
    assert_eq!(
        rows[0].detail,
        "this binary is the first `nika` on PATH · 2 other `nika` files on PATH, listed below"
    );
    let older = &rows[1];
    assert_eq!(older.level, Level::Warn);
    assert!(
        older
            .detail
            .ends_with("nika 0.123.2 (ccccccccc) · installer: unknown · shadowed by this binary · older than this binary"),
        "{older:#?}"
    );
    assert_eq!(
        older.fix.as_deref(),
        Some(
            format!(
                "rm {} && hash -r   # the older copy",
                old.join("nika").display()
            )
            .as_str()
        )
    );
    let newer_row = &rows[2];
    assert_eq!(newer_row.level, Level::Warn);
    assert!(
        newer_row.detail.ends_with("newer than this binary"),
        "{newer_row:#?}"
    );
    assert_eq!(
        newer_row.fix.as_deref(),
        Some(
            format!(
                "export PATH={}:\"$PATH\" && hash -r   # the newer copy then runs first",
                newer.display()
            )
            .as_str()
        )
    );
    let v = json(&id);
    assert_eq!(v["running"]["first_on_path"], true);
    assert_eq!(v["others"][0]["running_shadows_this"], true);
    assert_eq!(v["others"][0]["shadows_running"], false);
    assert_eq!(v["others"][1]["order"], "newer");
    let (_, notice) = welcome_lines(&id, &|p: &Path| p.display().to_string());
    assert_eq!(
        notice.as_deref(),
        Some("2 other copies of `nika` on PATH differ from this one → nika doctor")
    );
}

#[test]
fn a_hanging_copy_is_reported_within_the_deadline_never_fatal() {
    let m = Machine::new();
    let me = m.running_dir();
    let hang = m.stub("hang", "exec /bin/sleep 30", 0o755);
    let started = Instant::now();
    let id = m.observe(&[&me, &hang], Duration::from_millis(400));
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "the probe is bounded: {:?}",
        started.elapsed()
    );
    assert_eq!(id.entries[1].answer, Answer::TimedOut);
    let rows = path_rows(&id);
    assert_eq!(rows[1].level, Level::Unknown, "{rows:#?}");
    assert!(
        rows[1].detail.contains("no `--version` answer within"),
        "{rows:#?}"
    );
    assert_eq!(json(&id)["others"][0]["probe"], "timed_out");
}

#[test]
fn a_failing_first_copy_warns_without_a_version() {
    let m = Machine::new();
    let broken = m.stub("broken", "echo 'Segmentation fault' >&2; exit 3", 0o755);
    let me = m.running_dir();
    let id = m.observe(&[&broken, &me], DEADLINE);
    assert!(
        matches!(&id.entries[0].answer, Answer::Failed(why) if why.contains('3')),
        "{id:#?}"
    );
    let rows = path_rows(&id);
    assert_eq!(rows[0].level, Level::Warn);
    assert!(
        rows[0].detail.starts_with(&format!(
            "typing `nika` runs {}, not this binary · `--version` failed:",
            broken.join("nika").display()
        )),
        "{rows:#?}"
    );
    assert_eq!(json(&id)["others"][0]["probe"], "failed");
}

#[test]
fn a_non_executable_entry_is_listed_and_never_wins() {
    let m = Machine::new();
    let stray = m.stub("stray", "echo 'nika 0.1.0'", 0o644);
    let me = m.running_dir();
    let id = m.observe(&[&stray, &me], DEADLINE);
    assert_eq!(
        id.winner(),
        Some(1),
        "the shell skips a non-executable file"
    );
    let rows = path_rows(&id);
    assert_eq!(
        rows[0].detail,
        "this binary is the first `nika` on PATH · 1 other `nika` file on PATH, listed below"
    );
    assert_eq!(rows[1].level, Level::Ok);
    assert!(
        rows[1]
            .detail
            .contains("not executable, the shell skips it"),
        "{rows:#?}"
    );
    let v = json(&id);
    assert_eq!(v["others"][0]["probe"], "not_executable");
    assert_eq!(v["others"][0]["executable"], false);
    assert_eq!(v["others"][0]["first_on_path"], false);
    assert_eq!(v["running"]["first_on_path"], true);
}

#[test]
fn a_relative_path_entry_is_listed_and_never_executed() {
    let m = Machine::new();
    let marker = m.root.path().join("executed");
    let project = m.stub(
        "project",
        &format!("touch '{}'; echo 'nika 0.1.0'", marker.display()),
        0o755,
    );
    let me = m.running_dir();
    // The same directory, spelled relative to this process's working
    // directory (up to `/`, then down): a PATH entry a shell resolves.
    let cwd = std::fs::canonicalize(std::env::current_dir().expect("cwd")).expect("cwd");
    let up: PathBuf = cwd.components().skip(1).map(|_| "..").collect();
    let relative = up.join(project.strip_prefix("/").expect("absolute scratch"));
    let id = collect_from(Inputs {
        path_var: Some(std::env::join_paths([&relative, &me]).expect("PATH")),
        running: Ok(m.running_path()),
        build: this_build(),
        places: Places::default(),
        deadline: DEADLINE,
    });
    assert!(!marker.exists(), "a relative entry never runs");
    assert_eq!(id.entries.len(), 2, "{id:#?}");
    assert_eq!(id.entries[0].answer, Answer::Relative);
    let rows = path_rows(&id);
    assert_eq!(rows[0].level, Level::Warn, "{rows:#?}");
    assert!(
        rows[0]
            .detail
            .contains(", from a relative PATH entry, not this binary"),
        "{rows:#?}"
    );
    assert_eq!(json(&id)["others"][0]["probe"], "not_probed");
}

#[test]
fn duplicate_path_entries_are_one_copy() {
    let m = Machine::new();
    let me = m.running_dir();
    let old = m.answering("old", "nika 0.1.0");
    let id = m.observe(&[&me, &old, &me, &old], DEADLINE);
    assert_eq!(id.entries.len(), 2, "{id:#?}");
}

#[test]
fn no_nika_on_path_is_said_calmly() {
    let m = Machine::new();
    let empty = m.root.path().join("empty");
    std::fs::create_dir(&empty).expect("mkdir");
    m.running_dir();
    let id = m.observe(&[&empty], DEADLINE);
    let rows = path_rows(&id);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (rows[0].level, rows[0].detail.as_str()),
        (
            Level::Ok,
            "no `nika` on PATH · this binary was started by its path"
        )
    );
    let v = json(&id);
    assert_eq!(v["running"]["on_path"], false);
    assert_eq!(v["others"], serde_json::json!([]));
}

#[test]
fn the_welcome_line_names_build_installer_and_path_and_the_shadow() {
    let m = Machine::new();
    let old = m.answering("old", "nika 0.122.0 (bbbbbbbbb)");
    let me = m.running_dir();
    let id = m.observe(&[&old, &me], DEADLINE);
    let (line, notice) = welcome_lines(&id, &|p: &Path| p.display().to_string());
    assert_eq!(
        line,
        format!(
            "v0.124.0 (aaaaaaaaa) · installer: unknown · {}",
            m.running_path().display()
        )
    );
    assert_eq!(
        notice,
        Some(format!(
            "`nika` runs another build, {} (nika 0.122.0 (bbbbbbbbb)) → nika doctor",
            old.join("nika").display()
        ))
    );
}

#[test]
fn an_unreadable_running_path_is_unknown_never_a_guess() {
    let id = collect_from(Inputs {
        path_var: None,
        running: Err("gone".to_owned()),
        build: this_build(),
        places: Places::default(),
        deadline: DEADLINE,
    });
    let rows = doctor_findings(&id);
    assert_eq!(rows[0].level, Level::Unknown);
    assert_eq!(id.installer, Installer::Unknown);
    let v = json(&id);
    assert_eq!(v["running"]["path"], serde_json::Value::Null);
    assert_eq!(v["running"]["path_error"], "gone");
}

#[test]
fn version_output_is_read_from_its_first_line() {
    let read = |s: &str| parse_version_output(s);
    assert_eq!(
        read("nika 0.123.2 (5591478a7)\n"),
        Some(Build {
            version: "0.123.2".to_owned(),
            commit: Some("5591478a7".to_owned())
        })
    );
    assert_eq!(
        read("\n  nika 0.120.0-dev\n"),
        Some(Build {
            version: "0.120.0-dev".to_owned(),
            commit: None
        })
    );
    assert_eq!(read("nika-cli 0.1.0"), None);
    assert_eq!(read("nika version"), None);
    assert_eq!(read(""), None);
    assert_eq!(read("usage: nika"), None);
}

#[test]
fn version_order_reads_release_versions_only() {
    assert_eq!(version_order("0.124.0", "0.123.2"), Some(Ordering::Greater));
    assert_eq!(version_order("0.9.0", "0.10.0"), Some(Ordering::Less));
    assert_eq!(
        version_order("0.120.0-dev", "0.120.0"),
        Some(Ordering::Less)
    );
    assert_eq!(version_order("0.120.0", "0.120.0"), Some(Ordering::Equal));
    assert_eq!(version_order("0.120.0-a", "0.120.0-b"), None);
    assert_eq!(version_order("latest", "0.1.0"), None);
}

#[test]
fn a_build_without_a_commit_is_judged_by_its_version() {
    let tagged = this_build();
    let bare = Build {
        version: "0.124.0".to_owned(),
        commit: None,
    };
    let other = Build {
        version: "0.124.0".to_owned(),
        commit: Some("eeeeeeeee".to_owned()),
    };
    assert!(tagged.same_build(&bare));
    assert!(!tagged.same_build(&other));
}

#[test]
fn the_installer_is_read_from_the_path_alone() {
    let home = Path::new("/home/ada");
    let places = Places::new(Some(home), None);
    let of = |found: &str, resolved: &str| places.installer(Path::new(found), Path::new(resolved));
    assert_eq!(
        of(
            "/opt/homebrew/bin/nika",
            "/opt/homebrew/Cellar/nika/0.124.0/bin/nika"
        ),
        Installer::Homebrew
    );
    assert_eq!(
        of(
            "/usr/local/bin/nika",
            "/usr/local/lib/node_modules/@supernovae-st/nika-linux-x64/bin/nika"
        ),
        Installer::NpmGlobal
    );
    assert_eq!(
        of(
            "/work/app/node_modules/.bin/nika",
            "/work/app/node_modules/@supernovae-st/nika-darwin-arm64/bin/nika"
        ),
        Installer::NpmPayload
    );
    assert_eq!(
        of("/home/ada/.cargo/bin/nika", "/home/ada/.cargo/bin/nika"),
        Installer::Cargo
    );
    assert_eq!(
        of("/home/ada/.nika/bin/nika", "/home/ada/.nika/bin/nika"),
        Installer::InstallSh
    );
    assert_eq!(
        of(
            "/home/ada/.nika/bin/nika",
            "/home/ada/.nika/releases/v0.124.0/nika"
        ),
        Installer::ReleaseDir
    );
    assert_eq!(
        of(
            "/home/ada/.nix-profile/bin/nika",
            "/nix/store/abc-nika-0.124.0/bin/nika"
        ),
        Installer::Nix
    );
    assert_eq!(
        of("/src/nika/target/debug/nika", "/src/nika/target/debug/nika"),
        Installer::SourceBuild
    );
    assert_eq!(of("/usr/bin/nika", "/usr/bin/nika"), Installer::Unknown);
    let cargo_home = Places::new(Some(home), Some(Path::new("/opt/cargo")));
    assert_eq!(
        cargo_home.installer(
            Path::new("/opt/cargo/bin/nika"),
            Path::new("/opt/cargo/bin/nika")
        ),
        Installer::Cargo
    );
}

#[test]
fn shell_words_quote_only_what_the_shell_would_split() {
    let m = Machine::new();
    let spaced = m.answering("with space", "nika 0.1.0");
    let me = m.running_dir();
    let id = m.observe(&[&spaced, &me], DEADLINE);
    let rows = path_rows(&id);
    assert_eq!(
        rows[0].fix.as_deref(),
        Some(
            format!(
                "rm '{}' && hash -r   # the older copy runs first on PATH",
                spaced.join("nika").display()
            )
            .as_str()
        )
    );
}
