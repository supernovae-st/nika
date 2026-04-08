// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E tests for `code_actions` handler.
//!
//! These tests exercise the public API from the consumer perspective,
//! feeding realistic YAML content and asserting on the returned actions.

use nika_lsp_core::handlers::code_action::{
    code_actions, code_actions_with_diagnostics, CodeActionKind, DiagnosticInfo,
};

// ---------------------------------------------------------------------------
// 1. Missing schema -> add schema action
// ---------------------------------------------------------------------------
#[test]
fn missing_schema_produces_add_schema_action() {
    let yaml = "\
workflow: generate-caption
tasks:
  - id: caption
    infer: \"describe this image\"
";

    let actions = code_actions(yaml, 0, 0);

    let schema_action = actions
        .iter()
        .find(|a| a.title.contains("schema"))
        .expect("should produce an add-schema action");

    assert_eq!(schema_action.kind, CodeActionKind::QuickFix);
    assert!(schema_action.is_preferred);

    let edit = schema_action.edit.as_ref().expect("should have an edit");
    assert_eq!(
        edit.offset, 0,
        "edit should insert at the start of the file"
    );
    assert!(
        edit.new_text.contains("schema:"),
        "inserted text must contain `schema:`"
    );
    assert!(
        edit.new_text.contains("0.12"),
        "inserted text must reference the current schema version"
    );
}

// ---------------------------------------------------------------------------
// 2. Has schema -> no schema action
// ---------------------------------------------------------------------------
#[test]
fn file_with_schema_does_not_produce_schema_action() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: generate-caption
tasks:
  - id: caption
    infer: \"describe this image\"
";

    let actions = code_actions(yaml, 0, 0);

    assert!(
        !actions.iter().any(|a| a.title.contains("schema")),
        "no schema action expected when schema: already present"
    );
}

// ---------------------------------------------------------------------------
// 3. Shorthand infer -> expand action
// ---------------------------------------------------------------------------
#[test]
fn shorthand_infer_produces_expand_action() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: test
tasks:
  - id: greet
    infer: \"Hello world\"
";

    // Point the cursor at the `infer:` line.
    let infer_offset = yaml.find("infer:").expect("yaml contains infer") as u32;
    let actions = code_actions(yaml, infer_offset, infer_offset);

    let expand = actions
        .iter()
        .find(|a| a.title.contains("Expand"))
        .expect("should produce an expand-shorthand action");

    assert_eq!(expand.kind, CodeActionKind::Refactor);
    assert!(!expand.is_preferred);

    let edit = expand.edit.as_ref().expect("should have an edit");
    assert!(
        edit.new_text.contains("prompt:"),
        "expanded text must use the block form with `prompt:`"
    );
    assert!(
        edit.new_text.contains("Hello world"),
        "expanded text must preserve the original prompt"
    );
}

// ---------------------------------------------------------------------------
// 4. Block form infer -> no expand action
// ---------------------------------------------------------------------------
#[test]
fn block_infer_does_not_produce_expand_action() {
    let yaml = "\
schema: nika/workflow@0.12
workflow: test
tasks:
  - id: greet
    infer:
      prompt: |
        Hello world
";

    // Point the cursor at the `infer:` line (block form, no inline value).
    let infer_offset = yaml.find("infer:").expect("yaml contains infer") as u32;
    let actions = code_actions(yaml, infer_offset, infer_offset);

    assert!(
        !actions.iter().any(|a| a.title.contains("Expand")),
        "block-form infer should NOT produce an expand action"
    );
}

// ---------------------------------------------------------------------------
// 5. Empty file -> schema action only
// ---------------------------------------------------------------------------
#[test]
fn empty_file_produces_only_schema_action() {
    let actions = code_actions("", 0, 0);

    assert_eq!(
        actions.len(),
        1,
        "exactly one action expected for empty file"
    );
    assert!(actions[0].title.contains("schema"));
    assert_eq!(actions[0].kind, CodeActionKind::QuickFix);
}

// ---------------------------------------------------------------------------
// Diagnostic-linked code actions
// ---------------------------------------------------------------------------

#[test]
fn unknown_task_diagnostic_suggests_fix() {
    let yaml = "\
schema: \"@0.12\"
model: test
workflow: test
tasks:
  - id: step1
    infer: \"Hello\"
  - id: step2
    with:
      data: $setp1
    infer: \"World\"
";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-140".to_string(),
        message: "Unknown task 'setp1' referenced in with block".to_string(),
        start_offset: yaml.find("$setp1").unwrap() as u32 + 1, // after $
        end_offset: yaml.find("$setp1").unwrap() as u32 + 6,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("step1"));
    assert!(fix.is_some(), "Should suggest 'step1' for typo 'setp1'");
    assert_eq!(fix.unwrap().kind, CodeActionKind::QuickFix);
    assert!(fix.unwrap().is_preferred);
}

#[test]
fn duplicate_task_diagnostic_suggests_rename() {
    let yaml = "\
schema: \"@0.12\"
model: test
workflow: test
tasks:
  - id: step1
    infer: \"Hello\"
  - id: step1
    infer: \"World\"
";
    let dup_offset = yaml.rfind("id: step1").unwrap() as u32;
    let diags = vec![DiagnosticInfo {
        code: "NIKA-141".to_string(),
        message: "Duplicate task ID 'step1'".to_string(),
        start_offset: dup_offset + 4,
        end_offset: dup_offset + 9,
    }];
    let actions = code_actions_with_diagnostics(yaml, dup_offset, dup_offset, &diags);
    let fix = actions.iter().find(|a| a.title.contains("Rename"));
    assert!(fix.is_some(), "Should offer rename for duplicate task");
}

#[test]
fn invalid_schema_diagnostic_suggests_fix() {
    let yaml = "schema: \"@0.11\"\nmodel: test\nworkflow: test\ntasks:\n  - id: s\n    infer: x\n";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-142".to_string(),
        message: "Invalid schema version".to_string(),
        start_offset: 0,
        end_offset: 15,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("@0.12"));
    assert!(fix.is_some(), "Should offer schema upgrade");
}

#[test]
fn missing_field_diagnostic_suggests_add() {
    let yaml = "schema: \"@0.12\"\nmodel: test\ntasks:\n  - infer: \"hello\"\n";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-145".to_string(),
        message: "Missing required field 'id'".to_string(),
        start_offset: yaml.find("- infer").unwrap() as u32,
        end_offset: yaml.find("- infer").unwrap() as u32 + 7,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("id"));
    assert!(fix.is_some(), "Should offer 'Add id field'");
}

#[test]
fn missing_model_diagnostic_suggests_add() {
    let yaml = "\
schema: \"@0.12\"
workflow: test
tasks:
  - id: step1
    infer: \"Hello\"
";
    let diags = vec![DiagnosticInfo {
        code: "NIKA-034".to_string(),
        message: "model: required for infer verb".to_string(),
        start_offset: yaml.find("infer:").unwrap() as u32,
        end_offset: yaml.find("infer:").unwrap() as u32 + 6,
    }];
    let actions = code_actions_with_diagnostics(yaml, 0, 0, &diags);
    let fix = actions.iter().find(|a| a.title.contains("model"));
    assert!(fix.is_some(), "Should offer 'Add model' for NIKA-034");
}
