// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Lifecycle words only. The host selects the facts; this module grants no transition.
use std::path::Path;
/// A proposal still needs Save, then a separate Run.
#[must_use]
pub fn proposal<P: AsRef<Path>>(paths: impl Iterator<Item = P>) -> String {
    let files: Vec<_> = paths
        .map(|p| format!("`{}`", p.as_ref().display()))
        .collect();
    format!(
        "Ready for review · {} · nothing saved, nothing run",
        files.join(" · ")
    )
}
/// The last run's exit, explicitly scoped so a later preparation cannot inherit its verdict.
#[must_use]
pub fn run(exit: u8, workflow: Option<&Path>) -> String {
    let word = match exit {
        0 => "Done · the run succeeded",
        1 => "Done · the run failed",
        2 => "Not run · the check refused",
        3 => "Not run · the environment refused",
        4 => "Paused · a gate waits",
        130 => "Stopped · the run was interrupted",
        _ => "Done · an unknown code",
    };
    workflow.map_or_else(
        || format!("Last Run · {word}"),
        |w| format!("Last Run · {word} · `{}`", w.display()),
    )
}

/// A live gate, already selected by the host.
#[must_use]
pub fn gate(workflow: &Path, task: &str) -> String {
    format!(
        "Waiting for your answer · `{}` paused at `{task}`",
        workflow.display()
    )
}
