// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// This suite executes the real `nika-cli` binary (CARGO_BIN_EXE) — its
// whole job is the binary contract, so it spawns processes (the same
// sanctioned carve-out as bin_smoke.rs / resume_e2e.rs).
#![allow(clippy::disallowed_types)]

//! The P0-12 recovery rail from a REAL failed run: a workflow whose
//! task fails is executed through the binary (exit 1 — a workflow
//! failure, never a file finding), and `explain <file>` must OPEN on
//! the rail — the failed task, the `trace show` pointer, the route
//! forward — with the naked re-run CTA stepped aside. The rail's
//! fold/render halves are unit-tested at `src/verbs/explain_file.rs`;
//! this pins the whole arc from the recorder's own journal.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika"))
}

/// Statically clean (check passes · the exec authority is declared),
/// fatally failing at run time.
const FAILING: &str = "nika: rail-witness\npermits: { exec: true }\ntasks:\n  boom:\n    exec: { shell: \"exit 7\" }\n";

#[test]
fn explain_opens_on_the_recovery_rail_after_a_real_failure() {
    let dir = tempfile::tempdir().expect("dir");
    let wf = dir.path().join("rail.nika.yaml");
    std::fs::write(&wf, FAILING).expect("fixture");

    // ACT 1 · the real run fails (exit 1 — the workflow ran, a task
    // died) and the flight recorder holds the journal under THIS dir.
    let run = bin()
        .arg("run")
        .arg(&wf)
        .arg("--no-progress")
        .current_dir(dir.path())
        .output()
        .expect("binary runs");
    assert_eq!(
        run.status.code(),
        Some(1),
        "a failed task is a workflow failure (1), never a file finding · stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let traces = dir.path().join(".nika").join("traces");
    assert!(
        std::fs::read_dir(&traces).is_ok_and(|mut d| d.next().is_some()),
        "the failed run left a journal in {}",
        traces.display()
    );

    // ACT 2 · explain of the SAME file opens on the rail — the repair
    // is the story until the failure is audited.
    let out = bin()
        .arg("explain")
        .arg(&wf)
        .current_dir(dir.path())
        .output()
        .expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(0),
        "explain stays a success · stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("utf8");
    for needle in [
        "last run failed",
        "boom",            // the failed task, named
        "nika trace show", // the pointer into the journal
    ] {
        assert!(text.contains(needle), "missing `{needle}`:\n{text}");
    }
    // The ONE route forward: a resume when the trace carries ADR-099
    // keys, a re-check when it does not (a lone failed task attested no
    // successes — the repair line is this fixture's shape).
    assert!(
        text.contains("--resume") || text.contains("repair"),
        "the rail names the route forward:\n{text}"
    );

    // The rail OPENS the render: it precedes the file anatomy, and the
    // naked re-run CTA steps aside (the resume/repair line is the only
    // run-shaped line allowed to compete).
    let rail = text.find("last run failed").expect("rail present");
    let anatomy = text.find("rail-witness\n").expect("the title line");
    assert!(rail < anatomy, "the rail opens before the anatomy:\n{text}");
    assert!(
        !text.contains("\nrun it\n"),
        "the naked re-run CTA stepped aside:\n{text}"
    );
}
