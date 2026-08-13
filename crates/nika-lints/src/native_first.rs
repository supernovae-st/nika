// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `native-first` rule set — spec `03-dag.md` §Native-first
//! (rules 001-005 · « normative for lints »): an `exec:` whose literal
//! command a stdlib builtin (or an MCP tool) covers.
//!
//! The CLASSIFICATION is `check::native_first::classify` — one truth
//! shared with the check-report hint (kind `native-first`), so the
//! linter ruleset and the audit surface can never disagree on what
//! fires. This module only reshapes the verdict into [`Lint`] records
//! (rule id · task · span · message · suggestion).

use nika_check::native_first::classify;
use nika_schema::raw::{RawAction, RawWorkflow};

use super::preference_rules::Lint;

/// Run the `native-first/001..005` preference rules over a parsed
/// workflow. Output is deterministic — task order, main action before
/// its `on_finally` cleanups, at most one lint per action.
#[must_use]
pub fn native_first(wf: &RawWorkflow) -> Vec<Lint> {
    let mut lints = Vec::new();
    for task in &wf.tasks {
        let t = &task.value;
        push(&t.action, t.id.value.as_str(), t.id.span, &mut lints);
    }
    lints
}

fn push(action: &RawAction, id: &str, span: nika_schema::source::Span, lints: &mut Vec<Lint>) {
    let RawAction::Exec(exec) = action else {
        return;
    };
    if let Some((rule, advice)) = classify(&exec.command) {
        lints.push(Lint::new(
            rule,
            id.to_owned(),
            span,
            "`exec:` with a probable native path (spec 03 §Native-first)".to_owned(),
            advice,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn lints_of(yaml: &str) -> Vec<(String, String)> {
        native_first(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
            .into_iter()
            .map(|l| (l.rule.to_owned(), l.task_id))
            .collect()
    }

    /// The three spec conformance fixtures, inline (the corpus itself is
    /// exercised by `tests/lints_one_obvious_way.rs` against ../spec).
    #[test]
    fn mirrors_the_spec_lints_fixtures() {
        let fired = lints_of(
            "nika: curl-crawl\ntasks:\n  crawl:\n    exec: { command: [\"curl\", \"-s\", \"https://example.com\", \"-o\", \"out/site.html\"] }\n",
        );
        assert_eq!(
            fired,
            vec![("native-first/001".to_owned(), "crawl".to_owned())]
        );

        let helper = lints_of(
            "nika: helper\ntasks:\n  upload:\n    exec:\n      command: [\"node\", \"workflows/site/bin/helper.mjs\", \"upload\", \"--file\", \"out/bg.png\"]\n",
        );
        assert_eq!(
            helper,
            vec![("native-first/005".to_owned(), "upload".to_owned())]
        );

        let silent = lints_of(
            "nika: build\ntasks:\n  test:\n    exec: { command: [\"cargo\", \"test\", \"--workspace\", \"--lib\"] }\n  nested:\n    after: { test: success }\n    exec: { command: [\"nika\", \"run\", \"subroutine.nika.yaml\"] }\n",
        );
        assert!(silent.is_empty(), "{silent:?}");
    }
}
