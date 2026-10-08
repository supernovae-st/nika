// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `nika run` line a door hands its child, built from already selected public Run data and
//! never serializing authority. Descended from the CLI host's lane at its size cap (the nika-cli
//! unit, D-2026-07-09-N1); the lane re-exports it at its own path.

use std::path::Path;

/// The hidden `nika run` flag that binds a child run to the witness of the bytes its request was
/// checked on: the child refuses any other source before anything runs.
pub const EXPECT_SOURCE: &str = "--expect-source";

/// Build argv from already selected public Run data; never serialize authority.
#[must_use]
pub fn run_args(root: &Path, workflow: &Path, ceiling: f64, vars: &[String]) -> Vec<String> {
    let mut args = vec![
        "run".into(),
        root.join(workflow).display().to_string(),
        "--json".into(),
        "--max-cost-usd".into(),
        ceiling.to_string(),
    ];
    for var in vars {
        args.extend(["--var".into(), var.clone()]);
    }
    args
}

/// Build a Run with the human's explicit access unchanged, also for monetary review.
#[must_use]
pub fn run_args_with_access(
    root: &Path,
    workflow: &Path,
    ceiling: f64,
    vars: &[String],
    pin: Option<&str>,
) -> Vec<String> {
    let mut args = run_args(root, workflow, ceiling, vars);
    if let Some(pin) = pin {
        args.extend(["--access".into(), pin.into()]);
    }
    args
}

/// Resume a paused Run of `workflow` at `trace` with the human's `answer`.
#[must_use]
pub fn resume_args(root: &Path, workflow: &Path, trace: &Path, answer: &str) -> Vec<String> {
    vec![
        "run".into(),
        root.join(workflow).display().to_string(),
        "--json".into(),
        "--resume".into(),
        trace.display().to_string(),
        "--answer".into(),
        answer.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_carries_exactly_the_explicit_pin_without_changing_inputs_or_budget() {
        let root = Path::new("/project");
        let workflow = Path::new("one.nika");
        let vars = vec!["name=value with spaces".into()];
        let plain = run_args(root, workflow, 0.25, &vars);
        assert_eq!(
            run_args_with_access(root, workflow, 0.25, &vars, None),
            plain
        );
        for pin in [
            "codex",
            "claude-code",
            "mock",
            "an-invalid-pin-is-not-erased",
        ] {
            let args = run_args_with_access(root, workflow, 0.25, &vars, Some(pin));
            assert_eq!(&args[..plain.len()], &plain);
            assert_eq!(&args[plain.len()..], &["--access", pin]);
            assert_eq!(args.iter().filter(|s| *s == "--access").count(), 1);
        }
    }
}
