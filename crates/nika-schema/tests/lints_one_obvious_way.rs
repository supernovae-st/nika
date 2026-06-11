// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]
#![allow(clippy::disallowed_methods)]

//! `one-obvious-way` lint conformance · spec `03-dag.md` §One obvious
//! way — « normative for linters ».
//!
//! The fixture corpus lives in the SPEC repo
//! (`nika-spec/conformance/tests/lints/**` · `input.yaml` +
//! `expected-lints.json`) so any engine's linter can claim the same
//! precision contract — fires + does-not-fire pairs per rule, exact
//! ordered `(rule, task)` equality. The corpus moved out of this file
//! 2026-06-11 (the shared-test-corpus pattern · same as the
//! conformance tiers); only the engine-specific shape assertions stay
//! inline below.

mod common;

use common::{fixture_dirs, spec_dir};
use nika_schema::lints::{Lint, one_obvious_way};
use nika_schema::{FileId, ParseMode, parse};

fn lint(yaml: &str) -> Vec<Lint> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("lint fixture must parse");
    one_obvious_way(&wf)
}

#[derive(Debug, serde::Deserialize, PartialEq)]
struct ExpectedLint {
    rule: String,
    task: String,
}

#[derive(Debug, serde::Deserialize)]
struct ExpectedLints {
    lints: Vec<ExpectedLint>,
}

#[test]
fn lint_fixture_corpus() {
    let root = spec_dir().join("conformance/tests/lints");
    assert!(root.is_dir(), "missing {} — clone ../spec", root.display());

    let mut failures: Vec<String> = Vec::new();
    let mut total = 0_usize;

    for dir in fixture_dirs(&root) {
        total += 1;
        let label = dir
            .strip_prefix(&root)
            .unwrap_or(&dir)
            .display()
            .to_string();
        let yaml = std::fs::read_to_string(dir.join("input.yaml")).expect("read input.yaml");
        let expected: ExpectedLints = serde_json::from_str(
            &std::fs::read_to_string(dir.join("expected-lints.json"))
                .expect("read expected-lints.json"),
        )
        .expect("parse expected-lints.json");

        let got: Vec<ExpectedLint> = lint(&yaml)
            .into_iter()
            .map(|l| ExpectedLint {
                rule: l.rule.to_string(),
                task: l.task_id,
            })
            .collect();

        // EXACT ordered equality — pins precision (no false positives),
        // determinism and task ordering in one comparison.
        if got != expected.lints {
            failures.push(format!(
                "{label} · expected {:?} · got {:?}",
                expected.lints, got
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {total} lint fixtures FAILED ·\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(total >= 24, "only {total} lint fixtures — corpus drift?");
}

#[test]
fn lint_carries_message_and_suggestion() {
    // Engine-specific shape — the WARNING text itself (not spec
    // surface · the corpus pins rule/task only).
    let yaml = "\
nika: v1
workflow: fshape
tasks:
  - id: a
    exec: { command: \"./a.sh\" }
  - id: b
    depends_on: [a]
    when: \"${{ tasks.a.status == 'success' }}\"
    exec: { command: \"./b.sh\" }
";
    let lints = lint(yaml);
    assert_eq!(lints.len(), 1);
    let l = &lints[0];
    assert_eq!(l.rule, "one-obvious-way/001");
    assert!(!l.message.is_empty());
    assert!(!l.suggestion.is_empty());
    assert!(l.suggestion.contains("depends_on"), "{}", l.suggestion);
}
