// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native run door of the machine and HTTP hosts (`LaneRunDoor`), over the real binary: its
//! child runs only the bytes and the world the Session checked. Another valid workflow written in
//! place after the check runs nothing, nor does a valid child rewritten after it while the parent
//! is unchanged; a request that recorded no check runs nothing; the checked world runs.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use nika_session::RunRequest;
use nika_session::change::check_on_disk;
use nika_session_host::run::{LaneRunDoor, RunDoor as _, RunStep};

const CHECKED: &str = "nika: lane-witness\nmodel: mock/echo\ntasks:\n  echo:\n    infer: { prompt: checked-bytes-ran, max_tokens: 20 }\noutputs:\n  result: ${{ tasks.echo.output }}\n";
const PARENT: &str =
    "nika: lane-parent\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika\" }\n";
const CHILD: &str = "nika: lane-child\nmodel: mock/echo\ntasks:\n  echo:\n    infer: { prompt: checked-child-ran, max_tokens: 20 }\noutputs:\n  said: ${{ tasks.echo.output }}\n";

/// A request for `workflow`, bound to the bytes and the world the Session's own check judges
/// under `root` as they stand now.
fn checked(root: &Path, workflow: &str) -> RunRequest {
    let audit = check_on_disk(root, Path::new(workflow));
    assert!(audit.clean, "{:?}", audit.findings);
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: Vec::new(),
        max_cost_usd: 0.1,
        access_pin: None,
        bytes: audit.bytes.map(Box::new),
        closure: audit.closure.map(Box::new),
    }
}

/// The door's step for `run` under `root`, and every line its child said.
fn run(root: &Path, run: &RunRequest) -> (RunStep, String) {
    let mut door = LaneRunDoor::new(PathBuf::from(env!("CARGO_BIN_EXE_nika")));
    let (sink, said) = channel::<String>();
    let step = door.run(root, run, &sink);
    drop(sink);
    (step, said.iter().collect::<Vec<_>>().join("\n"))
}

/// The traces a run left under `root`.
fn traces(root: &Path) -> usize {
    std::fs::read_dir(root.join(".nika/traces")).map_or(0, Iterator::count)
}

/// The child refused `run` before any task: a nonzero exit, no trace, the refusal said.
fn refused(root: &Path, run_request: &RunRequest) {
    let (step, said) = run(root, run_request);
    let RunStep::Observed { exit, trace, .. } = step else {
        panic!("the child answered: {step:?}\n{said}");
    };
    assert_ne!(exit, 0, "{said}");
    assert!(trace.is_none(), "nothing ran, so no trace: {said}");
    assert!(said.contains("not the bytes its check judged"), "{said}");
    assert_eq!(traces(root), 0, "{said}");
}

/// The child ran `run`, and its trace under `root` holds `evidence`.
fn ran(root: &Path, run_request: &RunRequest, evidence: &str) {
    let (step, said) = run(root, run_request);
    let RunStep::Observed { exit, trace, .. } = step else {
        panic!("the child answered: {step:?}\n{said}");
    };
    assert_eq!(exit, 0, "{said}");
    // The settlement names the trace under the project it ran in.
    let journal = std::fs::read_to_string(root.join(trace.expect("its trace"))).expect("trace");
    assert!(journal.contains(evidence), "{journal}");
}

#[test]
fn the_native_child_runs_only_the_bytes_the_session_checked() {
    let project = tempfile::tempdir().expect("project");
    let root = project.path();
    let workflow = root.join("own.nika");
    std::fs::write(&workflow, CHECKED).expect("checked bytes");
    let request = checked(root, "own.nika");
    // Another valid workflow lands at the checked path after the check: refused before any task.
    let replaced = CHECKED.replace("checked-bytes-ran", "replaced-after-check");
    std::fs::write(&workflow, replaced).expect("replaced");
    refused(root, &request);
    // A request that recorded no check runs nothing either.
    let unbound = RunRequest {
        bytes: None,
        closure: None,
        ..request.clone()
    };
    let (step, said) = run(root, &unbound);
    assert!(
        matches!(step, RunStep::Observed { exit, .. } if exit != 0),
        "{step:?}\n{said}"
    );
    assert_eq!(traces(root), 0, "{said}");
    // The checked bytes back in place: they run, and only they.
    std::fs::write(&workflow, CHECKED).expect("checked bytes");
    ran(root, &request, "checked-bytes-ran");
}

#[test]
fn the_native_child_refuses_a_world_whose_child_changed_after_the_check() {
    let project = tempfile::tempdir().expect("project");
    let root = project.path();
    std::fs::write(root.join("parent.nika"), PARENT).expect("parent");
    std::fs::write(root.join("child.nika"), CHILD).expect("child");
    let request = checked(root, "parent.nika");
    // A valid child lands in place after the check; the parent's own bytes are unchanged.
    let rewritten = CHILD.replace("checked-child-ran", "rewritten-after-check");
    std::fs::write(root.join("child.nika"), rewritten).expect("child rewritten");
    refused(root, &request);
    // The checked child back in place: the checked world runs.
    std::fs::write(root.join("child.nika"), CHILD).expect("checked child");
    ran(root, &request, "lane-parent");
}
