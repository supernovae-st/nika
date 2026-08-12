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

use common::{fixture_dirs, skip_in_mutants_sandbox, spec_dir};
use nika_lints::{Lint, native_first, one_obvious_way};
use nika_schema::{FileId, ParseMode, parse};

fn lint(yaml: &str) -> Vec<Lint> {
    try_lint(yaml).expect("lint fixture must parse")
}

fn try_lint(yaml: &str) -> Result<Vec<Lint>, nika_schema::SchemaError> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict)?;
    let mut lints = one_obvious_way(&wf);
    lints.extend(native_first(&wf));
    Ok(lints)
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
    if skip_in_mutants_sandbox() {
        return;
    }
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

        // The R5 predicates gap CLOSED (spec #118 · the engine speaks
        // the outcome-class spellings) — the 12 success/failure-spelled
        // fixtures rejoined this normal verdict path the day the rename
        // landed, and the ledger deleted itself per the ratchet.
        let expected: ExpectedLints = serde_json::from_str(
            &std::fs::read_to_string(dir.join("expected-lints.json"))
                .expect("read expected-lints.json"),
        )
        .expect("parse expected-lints.json");

        let got = match try_lint(&yaml) {
            Ok(lints) => lints,
            Err(e) => {
                failures.push(format!("{label} · refuses to parse: {e}"));
                continue;
            }
        };

        let got: Vec<ExpectedLint> = got
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
    if skip_in_mutants_sandbox() {
        return;
    }
    // Engine-specific shape — the WARNING text itself (not spec
    // surface · the corpus pins rule/task only). W2 scenario: /010 —
    // a non-tightening `after: terminal` beside a value edge.
    let yaml = "\
nika: fshape
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    with: { v: \"${{ tasks.a.output }}\" }
    after: { a: terminal }
    exec: { command: [\"./b.sh\", \"${{ with.v }}\"] }
";
    let lints = lint(yaml);
    assert_eq!(lints.len(), 1);
    let l = &lints[0];
    assert_eq!(l.rule, "one-obvious-way/010");
    assert!(!l.message.is_empty());
    assert!(!l.suggestion.is_empty());
    assert!(
        l.suggestion.contains("success"),
        "the /010 suggestion names the respelling"
    );
}

// ── rule 008 · interpolated string command → use the array form ──────────

fn lints_008(yaml: &str) -> Vec<nika_lints::Lint> {
    lint(yaml)
        .into_iter()
        .filter(|l| l.rule == "one-obvious-way/008")
        .collect()
}

#[test]
fn rule_008_flags_interpolated_string_command_needing_no_shell() {
    let yaml = "\
nika: interp
tasks:
  produce:
    exec: { command: [\"./gen.sh\"] }
  consume:
    with: { out: \"${{ tasks.produce.output }}\" }
    exec: { shell: \"process ${{ with.out }}\" }
";
    let eight = lints_008(yaml);
    assert_eq!(eight.len(), 1, "exactly one /008");
    assert_eq!(eight[0].task_id, "consume");
    assert!(
        eight[0].suggestion.contains("array form"),
        "{}",
        eight[0].suggestion
    );
}

#[test]
fn rule_008_silent_on_a_genuine_pipeline() {
    // A `|` means the author genuinely needs `/bin/sh -c` — keep the string.
    let yaml = "\
nika: pipe
tasks:
  produce:
    exec: { command: [\"./gen.sh\"] }
  consume:
    with: { out: \"${{ tasks.produce.output }}\" }
    exec: { shell: \"cat ${{ with.out }} | wc -l\" }
";
    assert!(lints_008(yaml).is_empty(), "a real pipeline is not flagged");
}

#[test]
fn rule_008_silent_on_the_array_form() {
    let yaml = "\
nika: argv
tasks:
  produce:
    exec: { command: [\"./gen.sh\"] }
  consume:
    with: { out: \"${{ tasks.produce.output }}\" }
    exec:
      command: [\"process\", \"${{ with.out }}\"]
";
    assert!(
        lints_008(yaml).is_empty(),
        "the array form is already the safe way"
    );
}

#[test]
fn rule_008_silent_without_interpolation() {
    let yaml = "\
nika: plain
tasks:
  build:
    exec: { command: [\"cargo\", \"build\", \"--release\"] }
";
    assert!(
        lints_008(yaml).is_empty(),
        "no interpolation, nothing to steer"
    );
}

// ── rule 009 · output binding ending in a bare iterator `[]` ──────────────

fn lints_009(yaml: &str) -> Vec<nika_lints::Lint> {
    lint(yaml)
        .into_iter()
        .filter(|l| l.rule == "one-obvious-way/009")
        .collect()
}

#[test]
fn rule_009_flags_a_binding_that_ends_in_a_bare_iterator() {
    let yaml = "\
nika: stream
tasks:
  fetch:
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    extract:
      emails: \".users[]\"
";
    let nine = lints_009(yaml);
    assert_eq!(nine.len(), 1, "exactly one /009");
    assert_eq!(nine[0].task_id, "fetch");
    assert!(nine[0].message.contains("emails"), "{}", nine[0].message);
    assert!(
        nine[0].suggestion.contains("[ … ]") || nine[0].suggestion.contains("first"),
        "{}",
        nine[0].suggestion
    );
}

#[test]
fn rule_009_silent_on_a_collected_stream() {
    // `[.users[]]` collects into one array value — the obvious way.
    let yaml = "\
nika: collected
tasks:
  fetch:
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    extract:
      emails: \"[.users[].email]\"
";
    assert!(
        lints_009(yaml).is_empty(),
        "a collected `[ … ]` stream is the safe way"
    );
}

#[test]
fn rule_009_silent_on_an_indexed_take() {
    let yaml = "\
nika: indexed
tasks:
  fetch:
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    extract:
      first_user: \".users[0]\"
";
    assert!(
        lints_009(yaml).is_empty(),
        "an indexed take is single-valued"
    );
}

#[test]
fn rule_009_silent_on_an_empty_array_literal_default() {
    // `.users // []` ends in `[]` but it is an empty-array LITERAL default,
    // not an iterator — the low-false-positive contract must not flag it.
    let yaml = "\
nika: deflt
tasks:
  fetch:
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    extract:
      users: \".users // []\"
";
    assert!(
        lints_009(yaml).is_empty(),
        "an empty-array literal default is not a bare iterator"
    );
}
