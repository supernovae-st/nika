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
