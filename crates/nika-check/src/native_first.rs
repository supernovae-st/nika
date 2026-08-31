// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native-first hints — historical `nika_check::native_first` path.
//!
//! Classification lives in `nika-check-analyzer` (the 15k prod-LOC
//! descent): this module is the Hint wrap plus the two public functions
//! embedders and `nika-lints` already call.

use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};

use super::hints::Hint;

/// The hint kind (agents route on it · `hints.rs` kind registry).
const KIND: &str = "native-first";

/// The rule ladder — most specific first inside each command segment.
/// Returns every stable rule id (`native-first/00N`) + advice body in
/// source order. ONE truth: the check hint AND the reference linter
/// ruleset (`lints::native_first`) both classify in the analyzer.
#[must_use]
pub fn classify_all(command: &RawCommand) -> Vec<(&'static str, String)> {
    nika_check_analyzer::native_first::classify_all(command)
}

/// Classify the first segment with a probable native path.
///
/// Kept as the compatibility surface for embedders that consumed the
/// original single-verdict API. New consumers should use
/// [`classify_all`] so later shell segments are never hidden.
#[must_use]
pub fn classify(command: &RawCommand) -> Option<(&'static str, String)> {
    nika_check_analyzer::native_first::classify(command)
}

/// Scan every `exec:` task for a native path — every matching shell
/// segment keeps its own site, wrapped as a check [`Hint`].
pub(super) fn scan(wf: &RawWorkflow) -> Vec<Hint> {
    let mut hints = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        let RawAction::Exec(exec) = &task.value.action else {
            continue;
        };
        for (rule, advice) in classify_all(&exec.command) {
            hints.push(Hint {
                kind: KIND,
                code: Some(rule),
                task: id.to_owned(),
                advice: format!("{rule} · {advice}"),
            });
        }
    }
    hints
}
