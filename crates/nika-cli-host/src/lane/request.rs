// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Local Run launch configuration; negotiation is not an approval.
use nika_display::check_render::RepairTarget;
use std::path::Path;

#[derive(Debug)]
#[non_exhaustive]
pub struct RunHostOptions {
    pub repair_target: Option<RepairTarget>,
    pub cost_review_stdio: bool,
}
impl From<RepairTarget> for RunHostOptions {
    fn from(target: RepairTarget) -> Self {
        Some(target).into()
    }
}
impl From<Option<RepairTarget>> for RunHostOptions {
    fn from(repair_target: Option<RepairTarget>) -> Self {
        Self {
            repair_target,
            cost_review_stdio: false,
        }
    }
}
impl RunHostOptions {
    #[must_use]
    pub fn with_cost_review_stdio(mut self, enabled: bool) -> Self {
        self.cost_review_stdio = enabled;
        self
    }
}
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
