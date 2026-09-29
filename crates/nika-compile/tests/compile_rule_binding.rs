// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Every recorded rule is bound to its words by the law that created it (R4 S0; the E14
//! near-misses of rounds 1 to 5, whose requests and well-typed mutations these tests reuse):
//! - a record that changed what a rule computes (its value, comparator, field or operand kind,
//!   junction, clause set, aggregate, group or sort key, direction, limit, a flag, a deleted
//!   key, its words moved to another excerpt) is refused, the rule named;
//! - a program standing where the words state a typed rule is refused;
//! - the record as written replays to the same candidate, and so does a record whose rules were
//!   emptied or whose renames were deleted (the reader re-derives them);
//! - the same law holds where a pending or verified transform continues.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

/// An answer round under this round's judge, the explicit approving double over a seat that
/// settles no other choice (R4 A11).
async fn judged_replay(request: &CompileRequest) -> CompileOutcome {
    let judge = common::JudgedSeat::approving(&common::NoChoice);
    let cognition = nika_compile_cognition::Cognition::<nika_compile_cognition::NoProvider> {
        provider: None,
        seat: Some(&judge),
    };
    nika_compile_cognition::compile_with_cognition(request, cognition)
        .await
        .unwrap()
}

const TEXT: &str = "Every weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./out.json";
const NUM: &str = "Every weekday at 8, read ./tickets.json, keep only the rows whose amount is strictly greater than 50 and write them to ./out.json";
const BOTH: &str = "Every weekday at 8, read ./tickets.json, keep only the rows whose status is open and whose amount is strictly greater than 50 and write them to ./out.json";
const TOP: &str = "Every weekday at 8, read ./sales.csv, keep the 2 rows with the highest amount and write them to ./top.csv";
const GROUP: &str = "Every weekday at 8, read ./sales.csv, count the rows per client and write the counts to ./per-client.json";
const AVG: &str = "Every weekday at 8, read ./sales.csv, compute the average of the amount column and write it to ./avg.txt";
const AVGC: &str = "Every weekday at 8, read ./sales.csv, compute the average of the amount column per client and write it to ./avg.json";

/// The first round: READY, its record and its candidate.
fn recorded(intent: &str) -> (Value, Option<String>) {
    // The CLI observed the E14 fixtures; the record carries that observation to its replays.
    let out = compile(&CompileRequest::create(intent).with_knowledge(common::e14_world())).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    (out.provenance.plan.unwrap(), out.candidate)
}

fn replay(intent: &str, record: Value) -> CompileOutcome {
    compile(&CompileRequest::create(intent).with_plan(record)).unwrap()
}

/// A clause of the first rule changed, its single-clause mirrors kept consistent and the
/// observational keys dropped (E14's `clause` and `strip`).
fn clause(record: &mut Value, index: usize, key: &str, value: &Value) {
    let rule = &mut record["rules"][0];
    rule["clauses"][index][key] = value.clone();
    if index == 0 && rule.get(key).is_some() {
        rule[key] = value.clone();
    }
    strip(rule);
}
fn strip(rule: &mut Value) {
    let fields = rule.as_object_mut().unwrap();
    fields.remove("jq");
    fields.remove("fields");
}
fn shape(record: &mut Value, key: &str, value: Option<Value>) {
    let rule = &mut record["rules"][0];
    let shape = rule["shape"].as_object_mut().unwrap();
    match value {
        Some(value) => shape.insert(key.to_owned(), value),
        None => shape.remove(key),
    };
    strip(rule);
}
fn aggregates(record: &mut Value, key: &str, value: &Value) {
    let rule = &mut record["rules"][0];
    for aggregation in rule["shape"]["aggregations"].as_array_mut().unwrap() {
        aggregation[key] = value.clone();
    }
    strip(rule);
}
/// The first rule replaced by a program under the same words (E14 round 4 `program_only`).
fn program_only(record: &mut Value, jq: &str, columns: &[&str]) {
    let text = record["rules"][0]["text"].clone();
    record["rules"][0] = json!({"text": text, "clauses": [], "junction": "and", "summary": false,
        "shape": {"aggregations": [], "columns": [], "derived": [], "descending": false,
                  "distinct": false, "distinct_by": [], "group_by": null, "join_on": null,
                  "limit": null, "renames": [], "sort_by": null},
        "lines": false, "program": {"jq": jq, "columns": columns}});
}

/// The replay is refused: no candidate, and the finding names the recorded plan.
fn refused(name: &str, out: &CompileOutcome) -> String {
    assert_ne!(out.status, CompileStatus::Ready, "{name}: {out:#?}");
    assert!(out.candidate.is_none(), "{name}: {out:#?}");
    let finding = out.diagnostics.iter().find(|d| d.target == "recorded_plan");
    assert!(finding.is_some(), "{name}: {out:#?}");
    finding.unwrap().message.clone()
}

type Mutation = fn(&mut Value);

/// Every near-miss of E14 rounds 1 to 5 the grammar's re-reading refuses, the rule named.
const NEAR_MISSES: &[(&str, &str, Mutation)] = &[
    ("t1-text-value-substituted", TEXT, |r| {
        clause(r, 0, "value", &json!("closed"));
    }),
    ("t2-comparator-negated", TEXT, |r| {
        clause(r, 0, "comparator", &json!("!="));
    }),
    ("t3-field-substituted", TEXT, |r| {
        clause(r, 0, "field", &json!("id"));
    }),
    ("t4-operand-kind-column", TEXT, |r| {
        clause(r, 0, "value", &json!("status"));
        clause(r, 0, "value_kind", &json!("column"));
    }),
    ("n1-comparator-flipped", NUM, |r| {
        clause(r, 0, "comparator", &json!("<"));
    }),
    ("n2-comparator-boundary", NUM, |r| {
        clause(r, 0, "comparator", &json!(">="));
    }),
    ("n3-field-substituted", NUM, |r| {
        clause(r, 0, "field", &json!("score"));
    }),
    ("n4-number-from-schedule", NUM, |r| {
        clause(r, 0, "value", &json!("8"));
    }),
    ("n5-column-operand", NUM, |r| {
        clause(r, 0, "value", &json!("score"));
        clause(r, 0, "value_kind", &json!("column"));
    }),
    ("n6-comparator-equal", NUM, |r| {
        clause(r, 0, "comparator", &json!("=="));
    }),
    ("b1-junction-flipped", BOTH, |r| {
        r["rules"][0]["junction"] = json!("or");
        strip(&mut r["rules"][0]);
    }),
    ("b2-clause-dropped", BOTH, |r| {
        r["rules"][0]["clauses"].as_array_mut().unwrap().truncate(1);
        strip(&mut r["rules"][0]);
    }),
    ("l1-limit-changed", TOP, |r| {
        shape(r, "limit", Some(json!(3)));
    }),
    ("l2-direction-flipped", TOP, |r| {
        shape(r, "descending", Some(json!(false)));
    }),
    ("l3-sort-field-substituted", TOP, |r| {
        shape(r, "sort_by", Some(json!("client")));
    }),
    ("g1-group-key-substituted", GROUP, |r| {
        shape(r, "group_by", Some(json!("status")));
    }),
    ("g2-count-named-as-group-key", GROUP, |r| {
        aggregates(r, "name", &json!("client"));
    }),
    ("a1-aggregate-op-substituted", AVG, |r| {
        aggregates(r, "op", &json!("sum"));
    }),
    ("a2-field-substituted", AVGC, |r| {
        aggregates(r, "field", &json!("montant"));
    }),
    ("e1-delete-limit", TOP, |r| shape(r, "limit", None)),
    ("e2-delete-descending", TOP, |r| {
        shape(r, "descending", None);
    }),
    ("e3-delete-sort-by", TOP, |r| shape(r, "sort_by", None)),
    ("e4-delete-group-by", GROUP, |r| shape(r, "group_by", None)),
    ("e5-delete-aggregations", AVGC, |r| {
        shape(r, "aggregations", None);
    }),
    ("s1-summary-flag", TEXT, |r| {
        r["rules"][0]["summary"] = json!(true);
        strip(&mut r["rules"][0]);
    }),
    ("s2-lines-flag", TEXT, |r| {
        r["rules"][0]["lines"] = json!(true);
        strip(&mut r["rules"][0]);
    }),
    ("t5-field-record-itself", TEXT, |r| {
        clause(r, 0, "field", &json!("."));
    }),
    ("r1-rename-onto-filtered-column", TEXT, |r| {
        shape(
            r,
            "renames",
            Some(json!([{"from": "amount", "to": "status"}])),
        );
    }),
    ("d1-derived-overwrites-column", TEXT, |r| {
        shape(
            r,
            "derived",
            Some(json!([{"name": "status", "op": "mul",
            "left": {"name": "amount"}, "right": {"number": "8"}}])),
        );
    }),
    ("x1-rule-text-other-excerpt", TEXT, |r| {
        r["rules"][0]["text"] = json!("status is open");
        clause(r, 0, "value", &json!("closed"));
    }),
    ("c1-clause-added", NUM, |r| {
        let rule = &mut r["rules"][0];
        rule["clauses"].as_array_mut().unwrap().push(
            json!({"comparator": ">", "field": "score", "value": "50", "value_kind": "number"}),
        );
        for key in ["field", "comparator", "value"] {
            rule.as_object_mut().unwrap().remove(key);
        }
        strip(rule);
    }),
    ("g4-rename-count-onto-group-key", GROUP, |r| {
        shape(
            r,
            "renames",
            Some(json!([{"from": "count", "to": "client"}])),
        );
    }),
];

#[test]
fn a_record_that_changed_what_a_rule_computes_is_refused_by_name() {
    let (mut accepted, mut by_decoder) = (Vec::new(), Vec::new());
    for (name, intent, mutate) in NEAR_MISSES {
        let (mut record, _) = recorded(intent);
        let text = record["rules"][0]["text"].as_str().unwrap().to_owned();
        mutate(&mut record);
        let out = replay(intent, record);
        if out.status == CompileStatus::Ready {
            accepted.push(*name);
            continue;
        }
        let message = refused(name, &out);
        // The strict decoder refuses a malformed record first and names the field it read;
        // every other refusal is the binding law's, and it names the rule.
        if message.contains("cannot be replayed (`rules[") {
            by_decoder.push(*name);
            continue;
        }
        let named = message.contains(&text) || message.contains("status is open");
        assert!(named, "{name}: the rule is named: {message}");
    }
    assert!(accepted.is_empty(), "READY near-misses: {accepted:?}");
    // Only a descending order left without its sort key is malformed before any reading.
    assert_eq!(by_decoder, ["e3-delete-sort-by"]);
}

/// E14 round 4: a program-only rule under the stated words replaces the typed filter or the
/// grouping (identity, inverted predicate, a column the request never names).
#[test]
fn a_program_standing_where_the_words_state_a_typed_rule_is_refused() {
    for (name, intent, jq, columns) in [
        (
            "p1-program-only-identity",
            TEXT,
            ".records",
            &["status"][..],
        ),
        (
            "p2-program-only-inverted",
            TEXT,
            "[.records[] | select(.status != \"open\")]",
            &["status"][..],
        ),
        (
            "p3-program-only-unnamed-column",
            TEXT,
            "[.records[] | select(.id != \"open-a\")]",
            &["id"][..],
        ),
        (
            "p4-program-only-replaces-grouping",
            GROUP,
            ".records | map({client: .client, count: 1})",
            &["client"][..],
        ),
    ] {
        let (mut record, _) = recorded(intent);
        program_only(&mut record, jq, columns);
        let message = refused(name, &replay(intent, record));
        assert!(
            message.contains("stands where its words state a typed rule"),
            "{name}: {message}"
        );
    }
}

/// The controls: the record as written, and the absences the reader re-derives.
#[test]
fn the_record_as_written_and_what_the_reader_rederives_replay_unchanged() {
    for intent in [TEXT, NUM, BOTH, TOP, GROUP, AVG, AVGC] {
        let (record, candidate) = recorded(intent);
        let out = replay(intent, record.clone());
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        assert_eq!(out.candidate, candidate, "{intent}");
        let mut emptied = record.clone();
        emptied["rules"] = json!([]);
        let out = replay(intent, emptied);
        assert_eq!(
            out.status,
            CompileStatus::Ready,
            "{intent} (rules emptied): {out:#?}"
        );
        assert_eq!(out.candidate, candidate, "{intent} (rules emptied)");
    }
    let (mut record, candidate) = recorded(AVGC);
    shape(&mut record, "renames", None);
    let out = replay(AVGC, record);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate, candidate);
}

/// The pending door reads the same law before any continuation: the historical verified f7
/// record with its stated filter inverted is refused by name, whatever its hash says.
#[test]
fn the_pending_door_binds_its_rules_by_the_same_law() {
    let path = format!(
        "{}/tests/fixtures/historical/f7-verified.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut record: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let intent = record["verified_transform"]["intent"]
        .as_str()
        .unwrap()
        .to_owned();
    let rule = &mut record["rules"][0];
    rule["clauses"][0]["value"] = json!("closed");
    rule["value"] = json!("closed");
    strip(rule);
    let out = replay(&intent, record);
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let finding = out
        .diagnostics
        .iter()
        .find(|d| d.target == "pending_transform")
        .unwrap();
    assert!(
        finding
            .message
            .contains("keep only the rows whose status is active"),
        "{}",
        finding.message
    );
}

/// A request whose computation the grammar cannot read: a seat states it as typed meaning.
const TILL: &str = "Compute the day's sales total from ./till.csv (columns ticket,time,amount_cents): the number of tickets and the sum of amount_cents. Send that summary in one POST to http://127.0.0.1:18471/hooks/till with the JSON body {tickets, total_cents} and write the same object to ./out/till.json.";

/// The recorded plan of the till request with the seat's typed computation as the law admits it.
fn till_record() -> Value {
    let compute = "the number of tickets and the sum of amount_cents";
    json!({"operations":[
        {"op":"read","detail":"./till.csv","evidence":"Compute the day's sales total from ./till.csv (columns ticket,time,amount_cents)","categories":[]},
        {"op":"compute","detail":compute,"evidence":compute,"categories":[]}],
      "effects":[
        {"verb":"send","target":"one POST to http://127.0.0.1:18471/hooks/till with the JSON body {tickets, total_cents}","policy":"automatic","evidence":"Send that summary in one POST to http://127.0.0.1:18471/hooks/till with the JSON body {tickets, total_cents}","policy_literal":null},
        {"verb":"write","target":"./out/till.json","policy":"automatic","evidence":"write the same object to ./out/till.json","policy_literal":null}],
      "obligations":[],"bindings":[{"role":"path","literal":"./till.csv"},{"role":"url","literal":"http://127.0.0.1:18471/hooks/till"},{"role":"path","literal":"./out/till.json"}],
      "constraints":[],"unknowns":[],"trigger":null,"strategy":"cold",
      "rules":[{"text":compute,"clauses":[],"junction":"and","summary":false,
                "shape":{"group_by":null,
                         "aggregations":[{"field":null,"op":"count","name":"tickets","round":null},
                                         {"field":"amount_cents","op":"sum","name":"total_cents","round":null}],
                         "sort_by":null,"descending":false,"columns":[],"derived":[]}}]})
}

/// A seat's typed computation is the fixpoint of the law that admitted it (R4 S0 B3): the
/// record as written replays; an element that law would never admit is refused by name. No law
/// reads the typed meaning from the words, so a plain replay names it INCOMPLETE and a round's
/// judge settles it (Q2, R4 A11).
#[tokio::test]
async fn a_seat_typed_rule_replays_only_as_the_law_that_admitted_it() {
    let out = replay(TILL, till_record());
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let open = &out.provenance.decision.as_ref().unwrap()["pending"]["open"];
    assert_eq!(open[0]["witness"], "unverified", "{open:#}");
    let request = CompileRequest::create(TILL).with_plan(till_record());
    let judged = judged_replay(&request).await;
    assert_eq!(judged.status, CompileStatus::Ready, "{judged:#?}");
    assert_eq!(judged.candidate, out.candidate);
    let aggregation = |record: &mut Value, key: &str, value: Value| {
        record["rules"][0]["shape"]["aggregations"][1][key] = value;
    };
    let forged: &[(&str, Mutation)] = &[
        ("an output name the request never states", |r| {
            r["rules"][0]["shape"]["aggregations"][1]["name"] = json!("grand_total");
        }),
        ("a field the request never names", |r| {
            r["rules"][0]["shape"]["aggregations"][1]["field"] = json!("tax_cents");
        }),
        ("a summary the law never lowers", |r| {
            r["rules"][0]["summary"] = json!(true);
        }),
        ("plain duplicates the law never lowers", |r| {
            r["rules"][0]["shape"]["distinct"] = json!(true);
        }),
    ];
    for (name, mutate) in forged {
        let mut record = till_record();
        mutate(&mut record);
        let message = refused(name, &replay(TILL, record));
        assert!(
            message.contains("the number of tickets and the sum of amount_cents"),
            "{name}: {message}"
        );
    }
    // OPEN, stated as such (not closed by this law, see the crate spec): an aggregate or a
    // listed column the law admits can replace the recorded one, because the law grounds a
    // field only among the request's columns and an aggregate nowhere. Grounding literals in
    // their clause (option 2) closes values and numbers, not these. Narrowed by R4 A11 (Q2):
    // no longer READY unseen, it replays INCOMPLETE, the seat's typed meaning a remainder the
    // round's judge reads against the whole request.
    for (key, value) in [("op", json!("max")), ("field", json!("ticket"))] {
        let mut record = till_record();
        aggregation(&mut record, key, value);
        let out = replay(TILL, record);
        assert_eq!(
            out.status,
            CompileStatus::Incomplete,
            "OPEN hole moved: {key}: {out:#?}"
        );
    }
}

/// The orders request with a schedule whose hour is a number of another clause.
const ORDERS: &str = "Every weekday at 8, read ./data/orders.csv (columns order_id,customer,amount,status) and keep the rows that matter, then write them to ./out/kept.csv.";
/// The same request with the threshold stated in the filter's own clause.
const ORDERS_STATED: &str = "Every weekday at 8, read ./data/orders.csv (columns order_id,customer,amount,status) and keep the rows that matter, above 8, then write them to ./out/kept.csv.";

/// A seat's plan whose computation keeps the rows whose amount exceeds `value`.
fn orders_plan(value: &str) -> Value {
    json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": "read ./data/orders.csv"},
            {"op": "compute", "detail": "the rows that matter", "evidence": "keep the rows that matter",
             "computation": {"present": true, "join": "and",
                             "clauses": [{"field": "amount", "op": "gt", "value": value, "value_field": ""}]}}
        ],
        "effects": [{"verb": "write", "target": "./out/kept.csv", "policy": "automatic", "evidence": "write them to ./out/kept.csv"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

/// Option 2 (R4 S0): a seat's literal is grounded in the reader's own clause that holds its
/// evidence, never elsewhere in the request. The schedule's « 8 » is another clause's number:
/// the seat's predicate over it is not admitted (the rule is asked, never guessed), the same
/// number stated in the filter's clause is, and a record carrying it is refused where the
/// request states it only as the schedule's hour.
#[tokio::test]
async fn a_seat_literal_is_grounded_in_its_own_clause() {
    let unstated = compile_with_provider(
        &CompileRequest::create(ORDERS).with_authoring_policy(policy()),
        &Provider::new(orders_plan("8")),
    )
    .await
    .unwrap();
    assert!(
        keys(&unstated).contains(&"const.rule_expression"),
        "{unstated:#?}"
    );
    let stated = compile_with_provider(
        &CompileRequest::create(ORDERS_STATED).with_authoring_policy(policy()),
        &Provider::new(orders_plan("8")),
    )
    .await
    .unwrap();
    assert!(
        !keys(&stated).contains(&"const.rule_expression"),
        "{stated:#?}"
    );
    let record = stated.provenance.plan.clone().unwrap();
    assert_eq!(record["rules"][0]["value"], "8", "{record:#}");
    // The record admitted where the clause states « 8 » is refused where only the schedule does.
    let out = replay(ORDERS, record.clone());
    let message = refused("the schedule's hour", &out);
    assert!(message.contains("keep the rows that matter"), "{message}");
    let again = replay(ORDERS_STATED, record);
    assert!(stale_free(&again), "{again:#?}");
}

/// No recorded-plan refusal: the record replayed as written.
fn stale_free(out: &CompileOutcome) -> bool {
    !out.diagnostics.iter().any(|d| d.target == "recorded_plan")
}
