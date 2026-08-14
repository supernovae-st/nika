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

use nika_check::analyze;
use nika_schema::{FileId, ParseMode, parse};

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
    format!("nika: sbp\ntasks:{PRODUCER}{consumer}")
}

// ───────────────────── provably invalid → NIKA-VAR-003 ─────────────────────

#[test]
fn misspelled_key_on_closed_level_is_rejected() {
    let yaml = wf(r#"
  report:
    with: { p: "${{ tasks.extract.output.entitties }}" }
    exec:
      command: ["report", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn member_step_into_scalar_typed_property_is_rejected() {
    // `count` is an integer — `.value` beneath it is provably invalid.
    let yaml = wf(r#"
  report:
    with: { p: "${{ tasks.extract.output.count.value }}" }
    exec:
      command: ["report", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn index_step_on_non_array_level_is_rejected() {
    // the root output is type: object — indexing it is provably invalid.
    let yaml = wf(r#"
  report:
    with: { p: "${{ tasks.extract.output[0] }}" }
    exec:
      command: ["report", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn member_step_into_array_items_scalar_is_rejected() {
    // entities[0] is a string — `.name` beneath it is provably invalid.
    let yaml = wf(r#"
  report:
    with: { p: "${{ tasks.extract.output.entities[0].name }}" }
    exec:
      command: ["report", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn with_block_and_invoke_args_are_scanned_too() {
    let yaml = wf(r#"
  report:
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
    with:
      entities: "${{ tasks.extract.output.entities }}"
      count: "${{ tasks.extract.output.count }}"
    exec:
      shell: "report ${{ with.entities }} (${{ with.count }})"
"#);
    assert_eq!(codes(&yaml), Vec::<String>::new());
}

#[test]
fn valid_index_and_member_chain_is_accepted() {
    let yaml = wf(r#"
  report:
    with: { p: "${{ tasks.extract.output.entities[0] }}" }
    exec:
      command: ["first:", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), Vec::<String>::new());
}

#[test]
fn a_level_that_declares_nothing_is_never_rejected() {
    // The soundness half, and it survives the 2026-07-30 lock: `meta` is a
    // bare `type: object` with no `properties`, so nothing is declared and
    // nothing beneath it can be contradicted (spec 04 §Static binding
    // validation). This case used to also assert that `surprise` — an
    // undeclared SIBLING of a declared key — stayed legal; the lock decides
    // that the other way, and it is now the test below.
    let yaml = r#"
nika: sbp-open
tasks:
  extract:
    infer:
      prompt: "Extract"
      schema:
        type: object
        properties:
          meta: { type: object }
  report:
    with:
      b: "${{ tasks.extract.output.meta.anything.deep }}"
    exec:
      command: ["r", "${{ with.b }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn an_undeclared_sibling_of_a_declared_key_is_rejected() {
    // The strict half (operator lock 2026-07-30 · conformance fixture
    // core/variables/014): declaring `properties:` CLOSES the level for
    // binding, so the misspelled-key class refuses without waiting for an
    // explicit `additionalProperties: false`.
    let yaml = r#"
nika: sbp-sibling
tasks:
  extract:
    infer:
      prompt: "Extract"
      schema:
        type: object
        properties:
          meta: { type: object }
  report:
    with:
      a: "${{ tasks.extract.output.surprise }}"
    exec:
      command: ["r", "${{ with.a }}"]
"#;
    assert_eq!(codes(yaml), vec!["NIKA-VAR-003".to_string()]);
}

#[test]
fn explicitly_reopening_a_declared_level_makes_it_legal_again() {
    // The one-line fix the message prescribes, proven end to end.
    let yaml = r#"
nika: sbp-reopened
tasks:
  extract:
    infer:
      prompt: "Extract"
      schema:
        type: object
        additionalProperties: true
        properties:
          meta: { type: object }
  report:
    with:
      a: "${{ tasks.extract.output.surprise }}"
    exec:
      command: ["r", "${{ with.a }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn non_subset_construct_makes_the_level_open() {
    // oneOf at a level → the walk stops · nothing beneath is rejected.
    let yaml = r#"
nika: sbp-oneof
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
    with: { p: "${{ tasks.extract.output.result.maybe.deep }}" }
    exec:
      command: ["r", "${{ with.p }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn schema_less_producer_is_fully_dynamic() {
    let yaml = r#"
nika: sbp-dyn
tasks:
  dump:
    exec: { command: ["./dump.sh"] }
  report:
    with: { p: "${{ tasks.dump.output.whatever.deep[3] }}" }
    exec:
      command: ["r", "${{ with.p }}"]
"#;
    assert_eq!(codes(yaml), Vec::<String>::new());
}

#[test]
fn dynamic_index_step_ends_the_static_walk() {
    // a non-literal index makes the rest of the chain unknowable —
    // nothing is rejected (the prefix `entities` itself is valid).
    let yaml = wf(r#"
  report:
    with:
      i: "0"
      p: "${{ tasks.extract.output.entities[with.i] }}"
    exec:
      command: ["r", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), Vec::<String>::new());
}

#[test]
fn string_index_form_counts_as_member_step() {
    // tasks.extract.output['entitties'] — same misspelling via index-form.
    let yaml = wf(r#"
  report:
    with: { p: "${{ tasks.extract.output['entitties'] }}" }
    exec:
      command: ["r", "${{ with.p }}"]
"#);
    assert_eq!(codes(&yaml), vec!["NIKA-VAR-003".to_string()]);
}
