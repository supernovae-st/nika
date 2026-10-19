// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite's WHOLE JOB is to execute the real `nika-cli` binary
// (CARGO_BIN_EXE) — the bin_smoke carve-out class.
#![allow(clippy::disallowed_types)]

//! A kept run joined to a component of the saved bytes it ran, over REAL runs of the REAL
//! binary: the execution's own start names the hash of the exact bytes, and the component's tasks
//! are settled by that execution's own frames. An exit of zero, a run of other bytes, a skipped
//! or failed task and a child run's task of the same name never join.

use std::io::Write as _;
use std::path::Path;
use std::process::Command;

use nika_trace::run_view::{ComponentRun, KeptRun, RunFacts};

const COMPONENT: [&str; 2] = ["log_line", "write_line"];

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

fn write_fixture(dir: &Path, name: &str, yaml: &str) {
    let mut f = std::fs::File::create(dir.join(name)).expect("fixture file");
    f.write_all(yaml.as_bytes()).expect("fixture body");
}

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target")
        .join("tmp");
    let dir = base.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("tmp dir");
    dir
}

/// Run the binary in `dir` and return (exit code, stdout+stderr).
fn run_in(dir: &Path, args: &[&str]) -> (i32, String) {
    let out = bin()
        .current_dir(dir)
        .args(args)
        .output()
        .expect("binary runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

/// The one journal whose start names `digest`, and the run a host keeps of it: the execution its
/// frames carry.
fn journal_of(dir: &Path, digest: &str) -> (String, KeptRun) {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir.join(".nika").join("traces")).expect("trace dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).expect("journal reads");
        let facts = RunFacts::of(&path, &raw).expect("frames");
        if facts.starts() == [Some(digest.to_owned())] {
            let [execution] = facts.executions() else {
                panic!("one execution per journal: {:?}", facts.executions());
            };
            let mut kept = KeptRun::new();
            kept.execution = Some(execution.uuid.to_string());
            found.push((raw, kept));
        }
    }
    assert_eq!(found.len(), 1, "one journal starts these bytes");
    found.remove(0)
}

fn settled(run: &ComponentRun) -> Vec<(String, String)> {
    match run {
        ComponentRun::Joined(tasks) => (tasks.iter())
            .map(|t| (t.id().to_owned(), t.settled().to_owned()))
            .collect(),
        other => panic!("not joined: {other:?}"),
    }
}

/// A root workflow whose last two tasks are the component's; `gate` follows the write's `after:`.
fn root(name: &str, gate: &str) -> String {
    format!(
        "nika: {name}
permits:
  tools: [\"nika:log\", \"nika:write\"]
  fs: {{ write: [\"./out/**\"] }}
tasks:
  prepare:
    invoke: {{ tool: \"nika:log\", args: {{ message: \"prepare\" }} }}
  log_line:
    after: {{ prepare: success }}
    invoke: {{ tool: \"nika:log\", args: {{ message: \"component\" }} }}
  write_line:
    after: {{ log_line: success }}{gate}
    invoke: {{ tool: \"nika:write\", args: {{ path: \"./out/line.txt\", content: \"component\", create_dirs: true }} }}
"
    )
}

/// The saved bytes run as a root: the join names the component's tasks completed by that
/// execution, whose start names the hash of the exact bytes; the write is on disk.
#[test]
fn a_root_run_of_the_saved_bytes_joins_its_component() {
    let dir = tmp_dir("join-root");
    let saved = root("join-root", "");
    write_fixture(&dir, "root.nika", &saved);
    let (code, text) = run_in(&dir, &["run", "root.nika"]);
    assert_eq!(code, 0, "{text}");
    let digest = sha256_hex(saved.as_bytes());
    let (journal, kept) = journal_of(&dir, &digest);
    let run = kept.component_run(&journal, &digest, &COMPONENT);
    assert_eq!(
        settled(&run),
        [
            ("log_line".to_owned(), "completed".to_owned()),
            ("write_line".to_owned(), "completed".to_owned())
        ]
    );
    assert!(run.ran());
    assert!(dir.join("out/line.txt").exists());
}

/// An exit of zero is not the component's run: the write its `when:` skipped leaves the run
/// succeeded and the component unjoined.
#[test]
fn a_run_that_exits_zero_without_the_component_write_does_not_join() {
    let dir = tmp_dir("join-skipped");
    let saved = root("join-skipped", "\n    when: false");
    write_fixture(&dir, "root.nika", &saved);
    let (code, text) = run_in(&dir, &["run", "root.nika"]);
    assert_eq!(code, 0, "{text}");
    let digest = sha256_hex(saved.as_bytes());
    let (journal, kept) = journal_of(&dir, &digest);
    let facts = RunFacts::of(Path::new("root"), &journal).expect("frames");
    assert_eq!(facts.terminal(), Some("succeeded"));
    let run = kept.component_run(&journal, &digest, &COMPONENT);
    assert_eq!(
        settled(&run),
        [
            ("log_line".to_owned(), "completed".to_owned()),
            ("write_line".to_owned(), "skipped".to_owned())
        ]
    );
    assert!(!run.ran());
    assert!(!dir.join("out/line.txt").exists());
}

/// A component task that failed is said failed, and the component unjoined.
#[test]
fn a_failed_component_task_does_not_join() {
    let dir = tmp_dir("join-failed");
    let saved = "nika: join-failed
permits:
  tools: [\"nika:log\", \"nika:read\"]
  fs: { read: [\"./absent.txt\"] }
tasks:
  log_line:
    invoke: { tool: \"nika:log\", args: { message: \"component\" } }
  read_line:
    after: { log_line: success }
    invoke: { tool: \"nika:read\", args: { path: \"./absent.txt\" } }
";
    write_fixture(&dir, "root.nika", saved);
    let (code, text) = run_in(&dir, &["run", "root.nika"]);
    assert_eq!(code, 1, "the run fails on its read: {text}");
    let digest = sha256_hex(saved.as_bytes());
    let (journal, kept) = journal_of(&dir, &digest);
    let run = kept.component_run(&journal, &digest, &["log_line", "read_line"]);
    assert_eq!(
        settled(&run),
        [
            ("log_line".to_owned(), "completed".to_owned()),
            ("read_line".to_owned(), "failed".to_owned())
        ]
    );
    assert!(!run.ran());
}

/// A candidate saved after the run is other bytes: the execution's start names the bytes it ran,
/// and proves nothing of the saved ones.
#[test]
fn a_run_of_other_bytes_does_not_join_the_saved_candidate() {
    let dir = tmp_dir("join-other");
    let ran = root("join-other", "");
    write_fixture(&dir, "root.nika", &ran);
    let (code, text) = run_in(&dir, &["run", "root.nika"]);
    assert_eq!(code, 0, "{text}");
    let (journal, kept) = journal_of(&dir, &sha256_hex(ran.as_bytes()));
    let saved = format!("{ran}# saved after the run\n");
    assert_eq!(
        kept.component_run(&journal, &sha256_hex(saved.as_bytes()), &COMPONENT),
        ComponentRun::OtherSource(sha256_hex(ran.as_bytes()))
    );
}

const CHILD: &str = "nika: join-child
permits:
  tools: [\"nika:write\"]
  fs: { write: [\"./out/**\"] }
tasks:
  write_line:
    invoke: { tool: \"nika:write\", args: { path: \"./out/child.txt\", content: \"child\", create_dirs: true } }
";

const PARENT: &str = "nika: join-parent
permits:
  tools: [\"nika:log\", \"nika:write\"]
  fs: { write: [\"./out/**\"] }
tasks:
  call:
    invoke: { workflow: \"./child.nika\" }
  log_line:
    after: { call: success }
    invoke: { tool: \"nika:log\", args: { message: \"component\" } }
  write_line:
    after: { log_line: success }
    when: false
    invoke: { tool: \"nika:write\", args: { path: \"./out/line.txt\", content: \"component\", create_dirs: true } }
";

/// A child run's task of the same name is never the root's: the child's write ran in the child's
/// own execution and journal, the root's write was skipped, and the root's execution is in no
/// frame of the child's journal.
#[test]
fn a_child_task_of_the_same_name_is_never_the_roots() {
    let dir = tmp_dir("join-child");
    write_fixture(&dir, "child.nika", CHILD);
    write_fixture(&dir, "parent.nika", PARENT);
    let (code, text) = run_in(&dir, &["run", "parent.nika"]);
    assert_eq!(code, 0, "{text}");
    let parent = sha256_hex(PARENT.as_bytes());
    let child = sha256_hex(CHILD.as_bytes());
    let (root_journal, root_run) = journal_of(&dir, &parent);
    let (child_journal, child_run) = journal_of(&dir, &child);
    assert!(
        (child_run.component_run(&child_journal, &child, &["write_line"])).ran(),
        "the child's own write ran in the child's execution"
    );
    let run = root_run.component_run(&root_journal, &parent, &COMPONENT);
    assert_eq!(
        settled(&run),
        [
            ("log_line".to_owned(), "completed".to_owned()),
            ("write_line".to_owned(), "skipped".to_owned())
        ]
    );
    assert!(!run.ran());
    assert!(matches!(
        root_run.component_run(&child_journal, &parent, &COMPONENT),
        ComponentRun::Unknown(_)
    ));
    assert_eq!(
        child_run.component_run(&child_journal, &parent, &COMPONENT),
        ComponentRun::OtherSource(child)
    );
}
