// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as check_run_equivalence: this suite's WHOLE JOB is the real
// binary under a real ambient environment, so `Command` (and setting a variable
// ON THE CHILD rather than on the test process — `std::env::set_var` is unsafe
// in edition 2024 and this workspace forbids unsafe) is the instrument, not an
// accident.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

//! The falsification harness for D-2026-08-11-N26 — « **une expression ne voit
//! que son ENTRÉE** … C'est une SOUSTRACTION » — at the REAL binary.
//!
//! ## What it falsifies
//!
//! A canary variable is exported into the child process and a jq program asks
//! for it. Three authority shapes are put to the same question, because the
//! defect was that all three answered the same way — YES:
//!
//! | case | `permits:` | authority |
//! |------|-----------|-----------|
//! | A | absent | ZERO (NEP-0003 · F-O8) |
//! | B | present, `env: []` | an EXPLICIT refusal |
//! | C | present, `env: [CANARY]` | granted — to a CHILD PROCESS |
//!
//! Case C refuses too, and that is the decision, not an oversight: `permits.env`
//! passes an environment to a process the workflow SPAWNS (`nika-cap::env`), and
//! an in-process expression is not that process. N26 subtracts; a subtraction
//! has no dial. Flip this test the day the operator rules otherwise.
//!
//! ## Why it is a test and not a script
//!
//! Measured on the shipped 0.108.0 binary, 2026-08-15, all three cases returned
//! the operator's variable, the run settled green, and `nika check` printed
//! « the body is pure compute so nothing escapes ». The canary reached
//! `.nika/traces/*.ndjson` as a `task_completed` `output` field. A green that
//! means something has to be re-earned on every commit, so the falsification
//! lives here rather than in someone's terminal history.
//!
//! The CHECK⇔RUN parity leg is the point of `refused_identically_by_check_and_run`:
//! one program, both evaluators, compared in ONE assertion — a program the run
//! refuses that the check accepts would break the structural-parity claim
//! `nika-check::analyzer::jq_lint` makes in its own docstring.

use std::path::Path;
use std::process::{Command, Output};

/// A variable no workflow declares and nobody grants — if it comes back, it
/// came from the ambient environment.
const CANARY_NAME: &str = "NIKA_TEST_AMBIENT_CANARY";
const CANARY_VALUE: &str = "if-you-can-read-me-the-boundary-leaks";

fn write(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("write workflow");
    path
}

fn invoke(sub: &str, workflow: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
        .arg(sub)
        .arg(workflow)
        .env(CANARY_NAME, CANARY_VALUE)
        .current_dir(workflow.parent().expect("parent"))
        .output()
        .expect("binary runs")
}

/// stdout + stderr as one string — the refusal may voice on either.
fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Every byte the run left on disk under `dir` — the trace is where the leak
/// actually landed at 0.108.0, and the run's own stdout never showed it.
fn everything_written(dir: &Path) -> String {
    let mut acc = String::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&p) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(s) = std::fs::read_to_string(&path) {
                acc.push_str(&s);
            }
        }
    }
    acc
}

/// Parse every NDJSON event left below the run directory. Non-event files
/// (including the workflow itself) simply contribute no rows.
fn json_events_written(dir: &Path) -> Vec<serde_json::Value> {
    everything_written(dir)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|value: &serde_json::Value| value.get("kind").is_some())
        .collect()
}

fn workflow_with(permits: &str, program: &str) -> String {
    format!(
        "nika: ambient-canary\n\
         {permits}\
         tasks:\n  \
           probe:\n    \
             invoke:\n      \
               tool: \"nika:jq\"\n      \
               args:\n        \
                 input: {{}}\n        \
                 expression: '{program}'\n\
         \n\
         outputs:\n  \
           leak:\n    \
             value: ${{{{ tasks.probe.output }}}}\n"
    )
}

/// The three authority shapes, one question, one answer: no.
#[test]
fn no_authority_shape_lets_an_expression_read_the_environment() {
    for (case, permits) in [
        ("A-absent", ""),
        (
            "B-explicitly-empty",
            "permits:\n  tools: [\"nika:jq\"]\n  env: []\n",
        ),
        (
            "C-granted-to-a-child-process",
            &format!("permits:\n  tools: [\"nika:jq\"]\n  env: [\"{CANARY_NAME}\"]\n"),
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let wf = write(
            dir.path(),
            "probe.nika.yaml",
            &workflow_with(permits, "env.NIKA_TEST_AMBIENT_CANARY"),
        );

        let checked = invoke("check", &wf);
        assert!(
            !checked.status.success(),
            "case {case} · check ACCEPTED a body that reads the environment\n{}",
            text(&checked)
        );
        assert!(
            text(&checked).contains("ambient process environment"),
            "case {case} · the refusal must NAME the class\n{}",
            text(&checked)
        );

        let ran = invoke("run", &wf);
        assert!(
            !ran.status.success(),
            "case {case} · the run EXECUTED it\n{}",
            text(&ran)
        );

        // The leak surface at 0.108.0 was the trace, not stdout. Sweep both.
        let written = everything_written(dir.path());
        assert!(
            !text(&ran).contains(CANARY_VALUE) && !written.contains(CANARY_VALUE),
            "case {case} · the canary ESCAPED (stdout or a file under the run dir)"
        );
    }
}

/// The clock spelling is accepted but its host effect is gone.
///
/// N27 preserves `now` as user language and rebinds it to the exact run-start
/// timestamp already carried by `WorkflowStarted`. This binary-level test pins
/// both halves: the checker accepts the spelling and the running evaluator
/// returns the evidence timestamp rather than independently sampling the host.
#[test]
#[allow(clippy::cast_precision_loss, clippy::float_cmp)] // jq's clock wire is exact f64
fn the_clock_is_accepted_and_rebound_to_workflow_started() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wf = write(dir.path(), "probe.nika.yaml", &workflow_with("", "now"));
    let checked = invoke("check", &wf);
    assert!(
        checked.status.success(),
        "`now` must remain accepted user language\n{}",
        text(&checked)
    );
    let ran = invoke("run", &wf);
    assert!(
        ran.status.success(),
        "accepted `now` must run\n{}",
        text(&ran)
    );

    let events = json_events_written(dir.path());
    let started = events
        .iter()
        .find(|event| event["kind"] == "workflow_started")
        .expect("trace carries WorkflowStarted");
    let started_ns = started["timestamp"]
        .as_i64()
        .expect("opening timestamp is signed nanoseconds");
    let completed = events
        .iter()
        .find(|event| event["kind"] == "task_completed")
        .expect("trace carries the jq result");
    let output = completed["fields"]
        .as_array()
        .expect("task fields")
        .iter()
        .find(|field| field["key"] == "output")
        .and_then(|field| field["value"].as_str())
        .and_then(|json| serde_json::from_str::<f64>(json).ok())
        .expect("task output is jq's numeric clock value");
    assert_eq!(
        output,
        started_ns as f64 / 1_000_000_000.0,
        "jq and WorkflowStarted must carry the same minted instant"
    );
}

/// CHECK ⇔ RUN parity, as ONE comparison rather than two separate greens.
///
/// `nika-check::analyzer::jq_lint` claims in its docstring that the parity is
/// « structural (one jaq, one config), not a re-implementation ». Withholding a
/// native at the runtime seams alone would have falsified exactly that sentence.
#[test]
fn refused_identically_by_check_and_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wf = write(
        dir.path(),
        "probe.nika.yaml",
        &workflow_with("", "env.NIKA_TEST_AMBIENT_CANARY"),
    );
    let checked = invoke("check", &wf);
    let ran = invoke("run", &wf);

    let verdicts = (checked.status.success(), ran.status.success());
    assert_eq!(
        verdicts.0,
        verdicts.1,
        "check and run DISAGREED on the same program · check_ok={} run_ok={}\n\
         --- check ---\n{}\n--- run ---\n{}",
        verdicts.0,
        verdicts.1,
        text(&checked),
        text(&ran)
    );
    assert!(!verdicts.0, "both must refuse");

    let class = "ambient process environment";
    assert!(
        text(&checked).contains(class) && text(&ran).contains(class),
        "both must name the SAME class\n--- check ---\n{}\n--- run ---\n{}",
        text(&checked),
        text(&ran)
    );
}

/// The subtraction is scoped: an ordinary program still runs, and a function of
/// its own argument (the pure half of the date family) still compiles. Without
/// this, « refuse everything » would pass every test above.
#[test]
fn ordinary_programs_are_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wf = write(
        dir.path(),
        "probe.nika.yaml",
        &workflow_with("", ". + {y: (0 | gmtime | .[0])}"),
    );
    let checked = invoke("check", &wf);
    assert!(
        checked.status.success(),
        "an ordinary program must still pass\n{}",
        text(&checked)
    );
    let ran = invoke("run", &wf);
    assert!(ran.status.success(), "it must still run\n{}", text(&ran));
}
