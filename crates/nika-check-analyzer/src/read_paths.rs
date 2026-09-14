// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The statically-known read targets whose absence FAILS the run — the
//! pure half of the missing-input lint (V-arc F1 2026-07-09): this side
//! names the (task · path) pairs, the CALLER decides what existence means
//! on its side of the I/O boundary (the CLI checks the local filesystem;
//! a server might check an artifact store).
//!
//! Descended from `nika-check::walk` at the parent's 15k wall (2026-09-13
//! · the `for_each.max_items` estimator was the line that crossed it),
//! beside the static-value resolver it reads through.

use nika_schema::raw::{RawAction, RawWorkflow};

use crate::static_ref::static_literal_of;

/// Every `nika:read` / `nika:grep` whose `path` arg resolves STATICALLY
/// and whose absence would fail the run. A grep `path:` is the file or
/// tree the run opens (#1576 · a file is a one-file search since 0.120),
/// so its absence is the same wave failure. A task carrying `on_error:`
/// OWNS that failure (`recover:` · `skip:`) and is left out: the hint
/// predicts « the run would fail at that wave », and a prediction the
/// file's own semantics contradict spends the trust it exists to earn
/// (#1269 — measured on 0.115.0: the hint named a missing `./input.txt`
/// and the run recovered it, exit 0).
#[must_use]
pub fn static_read_paths(wf: &RawWorkflow) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for task in &wf.tasks {
        let RawAction::Invoke(invoke) = &task.value.action else {
            continue;
        };
        if task.value.on_error.is_some()
            || !matches!(
                invoke.tool().map(|t| t.value.as_str()),
                Some("nika:read" | "nika:grep")
            )
        {
            continue;
        }
        let Some(args) = &invoke.args else { continue };
        let Some(path_val) = args.value.get("path") else {
            continue;
        };
        if let Some(path) = static_string_arg(wf, path_val) {
            out.push((task.value.id.value.clone(), path));
        }
    }
    out
}

/// Resolve an invoke arg to a STATIC string when it is a plain literal
/// or a bare authority ref [`static_literal_of`] resolves to a string
/// literal — the shapes a scaffold ships. Anything dynamic (task refs ·
/// concatenations) resolves to `None`: analysis never guesses.
fn static_string_arg(wf: &RawWorkflow, value: &serde_json::Value) -> Option<String> {
    let s = value.as_str()?;
    let trimmed = s.trim();
    if !trimmed.contains("${{") {
        return Some(trimmed.to_owned());
    }
    static_literal_of(wf, trimmed)?.as_str().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn paths_of(yaml: &str) -> Vec<(String, String)> {
        static_read_paths(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// #1576 — a grep `path:` is a read the run opens, named beside
    /// `nika:read`; a const-backed path resolves; a templated task ref
    /// stays the run's (the lint never guesses).
    #[test]
    fn names_reads_and_greps_literal_or_const_backed_never_dynamic() {
        assert_eq!(
            paths_of(
                "nika: reads\nconst: { src: './in.md' }\ntasks:\n  r:\n    invoke: { tool: 'nika:read', args: { path: '${{ const.src }}' } }\n  g:\n    invoke: { tool: 'nika:grep', args: { path: './notes/brief.md', pattern: 'x' } }\n  d:\n    invoke: { tool: 'nika:grep', args: { path: '${{ tasks.r.output }}', pattern: 'x' } }\n",
            ),
            vec![
                ("r".to_owned(), "./in.md".to_owned()),
                ("g".to_owned(), "./notes/brief.md".to_owned()),
            ]
        );
    }

    /// #1269 — a task that owns its failure (`recover:` or `skip:`) is not
    /// a wave failure to predict; the sibling without `on_error:` still is.
    #[test]
    fn an_owned_failure_is_not_predicted() {
        assert_eq!(
            paths_of(
                "nika: owned\ntasks:\n  recovers:\n    on_error: { recover: null }\n    invoke: { tool: 'nika:read', args: { path: './missing.txt' } }\n  skips:\n    on_error: { skip: true }\n    invoke: { tool: 'nika:read', args: { path: './missing.txt' } }\n  fails:\n    invoke: { tool: 'nika:read', args: { path: './missing.txt' } }\n",
            ),
            vec![("fails".to_owned(), "./missing.txt".to_owned())]
        );
    }
}
