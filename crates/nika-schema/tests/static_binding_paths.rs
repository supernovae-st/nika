// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Static binding validation · spec `04-variables.md` §Static binding
//! validation against a declared `schema:` (normative).
//!
//! The killer authoring feature · when the producing task declares a
//! structured-output `schema:`, paths into `tasks.X.output` are checked
//! at PARSE time. The contract is SOUNDNESS · only provably-invalid
//! paths are rejected (`NIKA-VAR-003`) · open levels and schema-less
//! producers are NEVER rejected.

use nika_schema::{FileId, ParseMode, analyze, parse};

/// Parse + analyze · return the spec codes of all analysis errors
/// (empty = the workflow is accepted).
fn codes(yaml: &str) -> Vec<String> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture must parse");
    match analyze(&wf) {
        Ok(_) => vec![],
        Err(errors) => errors.iter().map(|e| e.spec_code().to_string()).collect(),
    }
}

/// The closed-schema producer used across fixtures.
const PRODUCER: &str = r#"
  extract:
    infer:
      prompt: "Extract entities"
      schema:
        type: object
        additionalProperties: false
        required: [entities]
        properties:
          entities:
            type: array
            items: { type: string }
          count: { type: integer }
"#;

fn wf(consumer: &str) -> String {
    format!("nika: v1\nworkflow:\n  id: sbp\ntasks:{PRODUCER}{consumer}")
}

// ───────────────────── provably invalid → NIKA-VAR-003 ─────────────────────

#[test]
fn misspelled_key_on_closed_level_is_rejected() {
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      command: ["report", "${{ tasks.extract.output.entitties }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn member_step_into_scalar_typed_property_is_rejected() {
    // `count` is an integer — `.value` beneath it is provably invalid.
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      command: ["report", "${{ tasks.extract.output.count.value }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn index_step_on_non_array_level_is_rejected() {
    // the root output is type: object — indexing it is provably invalid.
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      command: ["report", "${{ tasks.extract.output[0] }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn member_step_into_array_items_scalar_is_rejected() {
    // entities[0] is a string — `.name` beneath it is provably invalid.
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      command: ["report", "${{ tasks.extract.output.entities[0].name }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn with_block_and_invoke_args_are_scanned_too() {
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    with:
      payload: "${{ tasks.extract.output.entitties }}"
    invoke:
      tool: "nika:notify"
      args: { channel: webhook, target: "https://h.example.com", message: "${{ with.payload }}" }
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

// ───────────────────── sound · never reject valid-or-open ─────────────────────

#[test]
fn declared_property_path_is_accepted() {
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      shell: "report ${{ tasks.extract.output.entities }} (${{ tasks.extract.output.count }})"
"#);
    assert_eq!(codes(&yaml), Vec::<String>::new());
}

#[test]
fn valid_index_and_member_chain_is_accepted() {
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      command: ["first:", "${{ tasks.extract.output.entities[0] }}"]
"#);
    assert_eq!(codes(&yaml), Vec::<String>::new());
}

#[test]
fn open_level_is_never_rejected() {
    // no additionalProperties: false → unknown keys stay legal.
    let yaml = r#"
nika: v1
workflow:
  id: sbp-open
tasks:
  extract:
    infer:
      prompt: "Extract"
      schema:
        type: object
        properties:
          meta: { type: object }
  report:
    depends_on: [extract]
    exec:
      command: ["r", "${{ tasks.extract.output.surprise }}", "${{ tasks.extract.output.meta.anything.deep }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn non_subset_construct_makes_the_level_open() {
    // oneOf at a level → the walk stops · nothing beneath is rejected.
    let yaml = r#"
nika: v1
workflow:
  id: sbp-oneof
tasks:
  extract:
    infer:
      prompt: "Extract"
      schema:
        type: object
        additionalProperties: false
        properties:
          result:
            oneOf:
              - { type: string }
              - { type: object }
  report:
    depends_on: [extract]
    exec:
      command: ["r", "${{ tasks.extract.output.result.maybe.deep }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn schema_less_producer_is_fully_dynamic() {
    let yaml = r#"
nika: v1
workflow:
  id: sbp-dyn
tasks:
  dump:
    exec: { command: ["./dump.sh"] }
  report:
    depends_on: [dump]
    exec:
      command: ["r", "${{ tasks.dump.output.whatever.deep[3] }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn dynamic_index_step_ends_the_static_walk() {
    // a non-literal index makes the rest of the chain unknowable —
    // nothing is rejected (the prefix `entities` itself is valid).
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    with:
      i: "0"
    exec:
      command: ["r", "${{ tasks.extract.output.entities[with.i] }}"]
"#);
    assert_eq!(codes(&yaml), Vec::<String>::new());
}

#[test]
fn string_index_form_counts_as_member_step() {
    // tasks.extract.output['entitties'] — same misspelling via index-form.
    let yaml = wf(r#"
  report:
    depends_on: [extract]
    exec:
      command: ["r", "${{ tasks.extract.output['entitties'] }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}
