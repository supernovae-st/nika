// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E tests for code_lens handler.
use nika_lsp_core::handlers::code_lens::{code_lenses, LensCommand};

#[test]
fn validate_on_schema_line() {
    let yaml = "schema: \"@0.12\"\nworkflow: test\ntasks:\n  - id: s\n    infer: x\n";
    let lenses = code_lenses(yaml, None);
    let validate = lenses.iter().find(|l| l.command == LensCommand::Validate);
    assert!(validate.is_some());
    assert_eq!(validate.unwrap().line, 0);
}

#[test]
fn run_on_tasks_line() {
    let yaml = "schema: \"@0.12\"\ntasks:\n  - id: s\n    infer: x\n";
    let lenses = code_lenses(yaml, None);
    let run = lenses.iter().find(|l| l.command == LensCommand::Run);
    assert!(run.is_some());
}

#[test]
fn task_count_label() {
    let yaml = "tasks:\n  - id: a\n    exec: x\n  - id: b\n    exec: y\n  - id: c\n    exec: z\n";
    let lenses = code_lenses(yaml, None);
    let count = lenses
        .iter()
        .find(|l| matches!(l.command, LensCommand::TaskCount(_)));
    assert!(count.is_some());
    if let LensCommand::TaskCount(n) = count.unwrap().command {
        assert_eq!(n, 3);
    }
}

#[test]
fn no_lenses_on_empty() {
    assert!(code_lenses("", None).is_empty());
}
