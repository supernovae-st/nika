// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A field question whose answer the observation already holds is not asked (F2-Q1, E39 PILOT14
//! DEV rows). « Sum integer `amount_cents` of paid rows by customer » names the VALUE `paid`, never
//! the field: the seat reads it as `status == "paid"` (a typed clause in EN, a verified program in
//! FR), and the cold round asked « Which observed field in `./data/input.csv` does `status` mean? »
//! although the host had observed `paid` in `status` and in no other column. The stated value now
//! witnesses the seat's field; a value the host recorded in no column (DEV7-P3-EN: two sampled
//! rows, no categorical set) or in two columns keeps the question.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Judged, Provider, Rotating, policy};

const READ: &str = "Read ./data/input.csv.";
const COMPUTED: &str = "Sum integer amount_cents of paid rows by customer";
const FORMAT: &str = "every duplicate contributes and empty input creates no groups";
const WRITE: &str = "Write a JSON array sorted by customer with exactly textual customer and integer total_cents per group to ./out/result.json.";

/// V6-DEV2-P4-EN, verbatim.
fn intent() -> String {
    format!("{READ} {COMPUTED}; {FORMAT}. {WRITE}")
}

/// The plan a seat states for [`intent`]: the read, the typed computation over [`COMPUTED`] with
/// `clauses`, the format constraint and the write.
fn proposal(clauses: &Value) -> Value {
    let computation = json!({"present": true, "polarity": "keep", "join": "and",
        "clauses": clauses, "group_by": "customer",
        "aggregations": [{"field": "amount_cents", "op": "sum", "as": "total_cents", "round": ""}],
        "sort_by": "customer", "order": "asc", "ties": "", "columns": ["customer", "total_cents"],
        "numbers": ["total_cents"], "derived": [], "limit": "", "renames": [], "distinct_by": []});
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": READ},
            {"op": "compute", "detail": COMPUTED, "evidence": COMPUTED, "computation": computation}
        ],
        "effects": [{"verb": "write", "target": WRITE, "policy": "automatic", "evidence": WRITE}],
        "obligations": [], "constraints": [FORMAT], "unknowns": [],
        "regions": [
            {"text": READ, "role": "operation"},
            {"text": format!("{COMPUTED};"), "role": "operation"},
            {"text": format!("{FORMAT}."), "role": "constraint"},
            {"text": WRITE, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

/// The seat's clause: `status == "paid"`.
fn paid() -> Value {
    json!([{"field": "status", "op": "eq", "value": "paid", "value_field": ""}])
}

/// What the host observed of `./data/input.csv` in the PILOT14 round (DEV2-P4 nominal): the CSV
/// header, the categorical values it recorded and the kinds of the sampled values.
fn dev2_world(values: &Value) -> Value {
    json!({
        "kinds": {"./data/input.csv": {"sampled": 3, "keys": {
            "amount_cents": {"number_text": 3}, "customer": {"text": 3}, "id": {"text": 3},
            "kind": {"text": 3}, "status": {"text": 3}}}},
        "observed": [
            {"path": "./data/input.csv", "state": "observed", "kind": "csv", "complete": false,
             "delimiter": ",", "bytes": 102,
             "columns": ["id", "customer", "status", "kind", "amount_cents"], "values": values},
            {"path": "./out/result.json", "state": "absent", "complete": false}
        ]
    })
}

/// A cold compile whose seat answers `plan` over `world`, judged by the approving double: these
/// tests read the field grounding, not a judgment.
async fn cold(plan: &Value, world: Value) -> CompileOutcome {
    let provider = Provider::new(plan);
    let request = CompileRequest::create(intent())
        .with_authoring_policy(policy())
        .with_knowledge(world);
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// The recorded grounding entry of `field`, null when none.
fn grounding_of(out: &CompileOutcome, field: &str) -> Value {
    let decision = out.provenance.decision.as_ref();
    decision
        .and_then(|d| d["grounding"].as_array())
        .and_then(|entries| entries.iter().find(|e| e["field"] == field))
        .cloned()
        .unwrap_or(Value::Null)
}

fn field_questions(out: &CompileOutcome) -> Vec<&str> {
    common::keys(out)
        .into_iter()
        .filter(|key| key.starts_with("const.rule_field_"))
        .collect()
}

#[tokio::test]
async fn a_value_observed_in_one_column_answers_the_field_question() {
    let values = json!({"amount_cents": ["100", "50"], "customer": ["acme", "beta"],
        "id": ["r1", "r2"], "kind": ["sale"], "status": ["paid"]});
    let out = cold(&proposal(&paid()), dev2_world(&values)).await;
    assert!(field_questions(&out).is_empty(), "{:#?}", out.questions);
    let status = grounding_of(&out, "status");
    assert_eq!(status["bound_by"], "observation", "{status:#}");
    assert_eq!(status["admissible"], true, "{status:#}");
    assert_eq!(status["witness"], "paid", "{status:#}");
    // The fields the request names stay bound by its words.
    assert_eq!(grounding_of(&out, "customer")["bound_by"], "request");
    assert_eq!(grounding_of(&out, "amount_cents")["bound_by"], "request");
}

#[tokio::test]
async fn a_value_the_host_recorded_in_no_column_keeps_the_field_question() {
    // No categorical set for status: the sample never shows where paid lives.
    let values = json!({"kind": ["sale"]});
    let out = cold(&proposal(&paid()), dev2_world(&values)).await;
    assert_eq!(
        field_questions(&out),
        ["const.rule_field_1"],
        "{:#?}",
        out.questions
    );
    assert_eq!(grounding_of(&out, "status")["bound_by"], Value::Null);
}

#[tokio::test]
async fn a_value_observed_in_two_columns_keeps_the_field_question() {
    let values = json!({"kind": ["sale", "paid"], "status": ["paid", "open"]});
    let out = cold(&proposal(&paid()), dev2_world(&values)).await;
    assert_eq!(
        field_questions(&out),
        ["const.rule_field_1"],
        "{:#?}",
        out.questions
    );
    assert_eq!(grounding_of(&out, "status")["bound_by"], Value::Null);
}

const FR_READ: &str = "Lis ./data/input.csv.";
const FR_COMPUTED: &str = "Somme les amount_cents entiers des lignes paid par customer";
const FR_FORMAT: &str =
    "chaque doublon compte et aucun groupe ne doit être inventé sur une entrée vide";
const FR_WRITE: &str = "Écris un tableau JSON trié par customer, avec exactement customer textuel et total_cents entier par groupe dans ./out/result.json.";

/// V6-DEV2-P4-FR, verbatim: the typed stages cannot state it, so the seat writes a program.
fn intent_fr() -> String {
    format!("{FR_READ} {FR_COMPUTED} ; {FR_FORMAT}. {FR_WRITE}")
}

fn plan_fr() -> Value {
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": FR_READ},
            {"op": "compute", "detail": FR_COMPUTED, "evidence": FR_COMPUTED,
             "computation": {"present": false}}
        ],
        "effects": [{"verb": "write", "target": FR_WRITE, "policy": "automatic", "evidence": FR_WRITE}],
        "obligations": [], "constraints": [FR_FORMAT], "unknowns": [],
        "regions": [
            {"text": FR_READ, "role": "operation"},
            {"text": format!("{FR_COMPUTED} ;"), "role": "operation"},
            {"text": format!("{FR_FORMAT}."), "role": "constraint"},
            {"text": FR_WRITE, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

/// The DEV2-P4-FR seat program, verbatim, with the example it is verified on.
fn paid_program() -> Value {
    let jq = ".records | map(select(.status == \"paid\")) | group_by(.customer) | map({customer: .[0].customer, total_cents: (map(.amount_cents | tonumber) | add)}) | sort_by(.customer)";
    json!({"jq": jq, "columns_read": ["customer", "status", "amount_cents"],
        "example_input": [
            {"customer": "acme", "status": "paid", "amount_cents": "100"},
            {"customer": "acme", "status": "paid", "amount_cents": "100"},
            {"customer": "beta", "status": "open", "amount_cents": "50"}],
        "expected_output": [{"customer": "acme", "total_cents": 200}]})
}

/// A cold compile of [`intent_fr`] whose seat answers the plan, then the program, over `world`.
async fn cold_fr(world: Value) -> CompileOutcome {
    let provider = Rotating::new(vec![plan_fr().to_string(), paid_program().to_string()]);
    let request = CompileRequest::create(intent_fr())
        .with_authoring_policy(policy())
        .with_knowledge(world);
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_program_comparing_the_field_to_the_observed_value_answers_the_field_question() {
    let values = json!({"amount_cents": ["100", "50"], "customer": ["acme", "beta"],
        "id": ["r1", "r2"], "kind": ["sale"], "status": ["paid"]});
    let out = cold_fr(dev2_world(&values)).await;
    assert!(field_questions(&out).is_empty(), "{:#?}", out.questions);
    let status = grounding_of(&out, "status");
    assert_eq!(status["bound_by"], "observation", "{status:#}");
    assert_eq!(status["witness"], "paid", "{status:#}");
}

#[tokio::test]
async fn a_program_over_a_column_with_no_recorded_values_keeps_the_field_question() {
    let out = cold_fr(dev2_world(&json!({"kind": ["sale"]}))).await;
    assert_eq!(
        field_questions(&out),
        ["const.rule_field_2"],
        "{:#?}",
        out.questions
    );
    assert_eq!(grounding_of(&out, "status")["bound_by"], Value::Null);
}
