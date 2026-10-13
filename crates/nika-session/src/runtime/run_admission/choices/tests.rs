// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::runtime::{TurnOutcome, tests::ready_with};
use serde_json::json;

fn source(ty: &Value) -> String {
    r#"nika: input-choice
inputs:
  value: { type: TYPE, required: true }
permits: { fs: { write: ["./result.txt"] }, tools: ["nika:write"] }
tasks:
  write:
    invoke: { tool: "nika:write", args: { path: "./result.txt", content: "value=${{ inputs.value }}" } }
"#
    .replace("TYPE", &ty.to_string())
}

fn start(dir: &tempfile::TempDir, ty: &Value) -> SessionRuntime {
    std::fs::write(dir.path().join("input.nika"), source(ty)).expect("workflow");
    let mut session = ready_with(dir.path(), vec![]);
    let question = session.turn("run input.nika");
    assert!(
        matches!(question, TurnOutcome::Question { ref key, .. } if key == "input.value"),
        "{question:?}"
    );
    session
}

fn parsed(ty: &Value, run: &crate::change::RunRequest) -> Value {
    let workflow = nika_schema::parse(
        &source(ty),
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("parse");
    nika_cli_host::var_inputs::parse_var_overrides(&run.vars, &workflow)
        .expect("canonical Run bindings")
        .values["value"]
        .clone()
}

#[test]
fn finite_input_choices_are_exact_checked_types_and_projection_has_no_effects() {
    let dir = tempfile::tempdir().expect("root");
    let session = start(&dir, &json!("bool"));
    let pending = session.run_inputs.clone();
    let work = session.work();
    let choices = session.waiting_input_choices().expect("finite input");
    assert_eq!(choices.0, "value");
    assert_eq!(choices.1, [json!(true), json!(false)]);
    assert_eq!(session.waiting_input_choices(), Some(choices));
    assert_eq!(session.run_inputs, pending);
    assert_eq!(session.work().waiting, work.waiting);
    assert_eq!(session.requested_run, None);
    assert!(!dir.path().join("result.txt").exists());
}

#[test]
fn bool_answers_reach_the_canonical_run_parser_as_bools() {
    for value in [true, false] {
        let dir = tempfile::tempdir().expect("root");
        let ty = json!("bool");
        let mut session = start(&dir, &ty);
        let shown = session.waiting();
        assert!(matches!(
            session.submit("yes", &shown),
            TurnOutcome::Refusal(_)
        ));
        assert_eq!(session.pending_input(), Some("value"));
        let TurnOutcome::RunRequested { run, .. } = session.submit(&value.to_string(), &shown)
        else {
            panic!("Run requested")
        };
        assert_eq!(run.vars, [format!("value={value}")]);
        assert_eq!(parsed(&ty, &run), json!(value));
        assert!(run.bytes.is_some());
        assert!(!dir.path().join("result.txt").exists());
    }
}

#[test]
fn enum_strings_never_turn_into_commands_bools_numbers_or_environment_channels() {
    for value in [
        "/quit",
        "cancel",
        "true",
        "1",
        "  spaced  ",
        "@env:NIKA_TEST_INPUT",
        "",
    ] {
        let dir = tempfile::tempdir().expect("root");
        let ty = json!({"enum": [value, "other"]});
        let mut session = start(&dir, &ty);
        let key = json!(value).to_string();
        let shown = session.waiting();
        let TurnOutcome::RunRequested { run, .. } = session.submit(&key, &shown) else {
            panic!("literal {value:?} binds")
        };
        assert_eq!(run.vars, [format!("value={key}")]);
        assert_eq!(parsed(&ty, &run), json!(value));
        assert!(!dir.path().join("result.txt").exists());
    }
}

#[test]
fn cancel_is_separate_from_a_literal_enum_cancel() {
    let dir = tempfile::tempdir().expect("root");
    let mut session = start(&dir, &json!({"enum": ["cancel", "keep"]}));
    assert!(matches!(
        session.submit("cancel", &session.waiting()),
        TurnOutcome::Facts(_)
    ));
    assert_eq!(session.run_inputs, None);
    assert_eq!(session.requested_run, None);
    assert!(!dir.path().join("result.txt").exists());
}

#[test]
fn input_identity_includes_request_context_and_never_survives_source_drift() {
    let dir = tempfile::tempdir().expect("root");
    let mut session = start(&dir, &json!("bool"));
    let original = session.waiting_input_choices().expect("choices").2;
    let base = session.run_inputs.clone().expect("request");
    let mut changed = vec![base.clone(); 4];
    changed[0].given.push("other=1".to_owned());
    changed[1].needed.push("next".to_owned());
    changed[2].max_cost_usd = 0.7;
    changed[3].access_pin = Some("api:fixture".to_owned());
    for inputs in changed {
        session.run_inputs = Some(inputs);
        assert_ne!(
            session.waiting_input_choices().expect("choices").2,
            original
        );
    }
    session.run_inputs = Some(base.clone());
    std::fs::write(
        dir.path().join("input.nika"),
        format!("{}\n", source(&json!("bool"))),
    )
    .expect("changed bytes");
    assert_eq!(session.waiting_input_choices(), None);
    assert_eq!(session.run_inputs, Some(base));
    std::fs::remove_file(dir.path().join("input.nika")).expect("remove source");
    assert_eq!(session.waiting_input_choices(), None);
}

#[test]
fn identical_runs_have_distinct_input_admissions_while_repainting_is_stable() {
    let dir = tempfile::tempdir().expect("root");
    let mut session = start(&dir, &json!("bool"));
    let first = session.waiting_input_choices().expect("first admission");
    assert_eq!(session.waiting_input_choices(), Some(first.clone()));
    assert!(matches!(session.turn("cancel"), TurnOutcome::Facts(_)));
    assert_eq!(session.waiting_input_choices(), None);
    let again = session.turn("run input.nika");
    assert!(matches!(again, TurnOutcome::Question { ref key, .. } if key == "input.value"));
    let current = session.waiting_input_choices().expect("new admission");
    assert_eq!(current.0, first.0);
    assert_eq!(current.1, first.1);
    assert_ne!(current.2, first.2);
    assert_eq!(session.waiting_input_choices(), Some(current));
    assert_eq!(session.requested_run, None);
    assert!(!dir.path().join("result.txt").exists());
}

#[test]
fn open_inputs_keep_the_existing_literal_escape_and_have_no_finite_menu() {
    for (ty, text, expected) in [
        (json!("string"), "\"true\"", "true"),
        (json!("string"), "\"  spaced  \"", "  spaced  "),
        (json!("string"), "\"/quit\"", "/quit"),
        (json!("integer"), "42", "42"),
        (json!("number"), "2.5", "2.5"),
        (json!("path"), "./data.csv", "./data.csv"),
    ] {
        let dir = tempfile::tempdir().expect("root");
        let mut session = start(&dir, &ty);
        assert_eq!(session.waiting_input_choices(), None);
        let TurnOutcome::RunRequested { run, .. } = session.submit(text, &session.waiting()) else {
            panic!("open input")
        };
        assert_eq!(run.vars, [format!("value={expected}")]);
    }
}
