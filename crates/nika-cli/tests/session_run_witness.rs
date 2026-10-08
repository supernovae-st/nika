// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The native run door of the machine and HTTP hosts (`LaneRunDoor`), over the real binary: its
//! child runs only the bytes the Session checked. Another valid workflow written in place after
//! the check runs nothing, a request that recorded no checked bytes runs nothing, and the
//! checked bytes run.
#![allow(clippy::expect_used, clippy::panic, clippy::disallowed_types)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use nika_session::{RunRequest, Witness};
use nika_session_host::run::{LaneRunDoor, RunDoor as _, RunStep};

const CHECKED: &str = "nika: lane-witness\nmodel: mock/echo\ntasks:\n  echo:\n    infer: { prompt: checked-bytes-ran, max_tokens: 20 }\noutputs:\n  result: ${{ tasks.echo.output }}\n";

/// A request for `workflow`, bound to `checked` (none: a request that recorded no bytes).
fn request(workflow: &str, checked: Option<&str>) -> RunRequest {
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: Vec::new(),
        max_cost_usd: 0.1,
        access_pin: None,
        bytes: checked.map(|source| Box::new(Witness::of(source.as_bytes()))),
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

#[test]
fn the_native_child_runs_only_the_bytes_the_session_checked() {
    let project = tempfile::tempdir().expect("project");
    let root = project.path();
    let workflow = root.join("own.nika");
    // Another valid workflow lands at the checked path after the check: refused before any task.
    std::fs::write(
        &workflow,
        CHECKED.replace("checked-bytes-ran", "replaced-after-check"),
    )
    .expect("replaced");
    let (step, said) = run(root, &request("own.nika", Some(CHECKED)));
    let RunStep::Observed { exit, trace } = step else {
        panic!("the child answered: {step:?}\n{said}");
    };
    assert_ne!(exit, 0, "{said}");
    assert!(trace.is_none(), "nothing ran, so no trace: {said}");
    assert!(said.contains("not the bytes its check judged"), "{said}");
    assert_eq!(traces(root), 0, "{said}");
    // A request that recorded no checked bytes runs nothing either.
    let (step, said) = run(root, &request("own.nika", None));
    assert!(
        matches!(step, RunStep::Observed { exit, .. } if exit != 0),
        "{step:?}\n{said}"
    );
    assert_eq!(traces(root), 0, "{said}");
    // The checked bytes back in place: they run, and only they.
    std::fs::write(&workflow, CHECKED).expect("checked bytes");
    let (step, said) = run(root, &request("own.nika", Some(CHECKED)));
    let RunStep::Observed { exit, trace } = step else {
        panic!("the child answered: {step:?}\n{said}");
    };
    assert_eq!(exit, 0, "{said}");
    // The settlement names the trace under the project it ran in.
    let evidence = std::fs::read_to_string(root.join(trace.expect("its trace"))).expect("trace");
    assert!(evidence.contains("checked-bytes-ran"), "{evidence}");
}
