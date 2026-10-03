// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
use crate::rules::{Clause, Comparator, Junction, Operand, Rule, Shape};
use serde_json::json;
use std::collections::BTreeSet;

fn request(intent: &str) -> CompileRequest {
    CompileRequest::create(intent).with_knowledge(json!({"observed": [{
        "path":"./orders.json", "columns":["id", "status"],
        "common_columns":["id", "status"], "state":"observed", "complete":false
    }]}))
}
fn filter(field: &str) -> Rule {
    Rule::typed(
        "garde les commandes dont le statut est delivered",
        vec![Clause::new(
            field,
            Comparator::Eq,
            Operand::Text("delivered".into()),
        )],
        Junction::And,
        Shape::default(),
    )
}
#[test]
fn translated_status_is_a_closed_choice_and_only_an_offered_answer_rebinds_it() {
    let req = request("Lis ./orders.json, garde les commandes dont le statut est delivered");
    let mut out = crate::initial();
    let mut recognized = BTreeSet::new();
    assert!(
        ground_rule(
            filter("statut"),
            "./orders.json",
            &req,
            &mut out,
            &mut recognized
        )
        .is_none()
    );
    let q = &out.questions[0];
    assert_eq!(q.answer_type, crate::QuestionType::Choice);
    assert_eq!(
        q.options.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
        ["id", "status"]
    );
    assert!(recognized.contains("const.rule_field_1"));
    let accepted = req.clone().answer("const.rule_field_1", "\"status\"");
    let rebound = ground_rule(
        filter("statut"),
        "orders.json",
        &accepted,
        &mut crate::initial(),
        &mut BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(rebound.source_fields(), ["status"]);
    assert!(rebound.jq().contains("status"));
    for invalid in ["\"statut\"", "\"STATUS\"", "42", "\"missing\""] {
        let wrong = req.clone().answer("const.rule_field_1", invalid);
        let mut out = crate::initial();
        assert!(
            ground_rule(
                filter("statut"),
                "orders.json",
                &wrong,
                &mut out,
                &mut BTreeSet::new()
            )
            .is_none()
        );
        assert_eq!(out.questions[0].answer_type, crate::QuestionType::Choice);
    }
}
#[test]
fn exact_id_is_preserved_but_a_ticket_to_id_mapping_is_not_invented() {
    let exact = request("Read ./orders.json and keep id equal to 42");
    let rule = Rule::typed(
        "id equal to 42",
        vec![Clause::new(
            "id",
            Comparator::Eq,
            Operand::Number("42".into()),
        )],
        Junction::And,
        Shape::default(),
    );
    let mut out = crate::initial();
    let accepted = ground_rule(
        rule.clone(),
        "orders.json",
        &exact,
        &mut out,
        &mut BTreeSet::new(),
    )
    .unwrap();
    // The exact key is kept; grounding states only its number policy (R4 A5).
    let stated = rule.with_number_policy("id", crate::rules::NumberPolicy::Fail);
    assert_eq!(Some(accepted), stated);
    assert!(out.questions.is_empty());
    let ambiguous = request("Read ./orders.json and find ticket 42");
    assert!(
        ground_rule(
            rule,
            "orders.json",
            &ambiguous,
            &mut out,
            &mut BTreeSet::new()
        )
        .is_none()
    );
    assert_eq!(out.questions[0].answer_type, crate::QuestionType::Choice);
}
#[test]
fn absent_empty_unknown_and_unreadable_are_not_empty_complete_schemas() {
    assert_eq!(columns(None, "orders.json"), None);
    for state in ["absent", "empty", "unknown", "unreadable"] {
        let world = json!({"observed":[{"path":"orders.json", "state":state, "complete":false}]});
        assert_eq!(columns(Some(&world), "orders.json"), None);
    }
    let mixed = json!({"observed":[{"path":"orders.json", "state":"observed", "columns":["id", "status"], "common_columns":[], "complete":false}]});
    assert_eq!(columns(Some(&mixed), "orders.json"), Some(vec![]));
    assert_eq!(columns(Some(&mixed), "other.json"), None);
}
/// With nothing observed, a key the request only names is inferred (R4 S1): it is asked for,
/// never lowered; the request's own column list, or an answer, grounds it as asserted.
#[test]
fn an_unobserved_key_is_asked_unless_the_request_declares_it_or_an_answer_names_it() {
    let rule = filter("status");
    let named = CompileRequest::create("Read ./future.json and keep status equal to delivered");
    let mut out = crate::initial();
    let asked = ground_rule(
        rule.clone(),
        "future.json",
        &named,
        &mut out,
        &mut BTreeSet::new(),
    );
    assert!(asked.is_none());
    assert_eq!(out.questions[0].key, "const.rule_field_1");
    assert_eq!(out.questions[0].answer_type, crate::QuestionType::Text);
    let grounding = &out.provenance.decision.as_ref().unwrap()["grounding"][0];
    assert_eq!(grounding["grade"], "inferred");
    assert_eq!(grounding["admissible"], false);
    let declared = CompileRequest::create(
        "Read ./future.json (columns id, status) and keep status equal to delivered",
    );
    let mut out = crate::initial();
    let kept = ground_rule(
        rule.clone(),
        "future.json",
        &declared,
        &mut out,
        &mut BTreeSet::new(),
    );
    assert_eq!(kept.unwrap().jq(), rule.jq());
    assert!(out.questions.is_empty());
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["grounding"][0]["grade"],
        "user_asserted"
    );
    let answered = named.answer("const.rule_field_1", "\"state\"");
    let mut out = crate::initial();
    let rebound = ground_rule(
        rule,
        "future.json",
        &answered,
        &mut out,
        &mut BTreeSet::new(),
    );
    assert_eq!(rebound.unwrap().source_fields(), ["state"]);
    let grounding = &out.provenance.decision.as_ref().unwrap()["grounding"][0];
    assert_eq!(grounding["bound_by"], "answer");
    assert_eq!(grounding["grade"], "user_asserted");
}
#[test]
fn replay_retains_the_observed_choices_and_fresh_observations_take_precedence() {
    let req = request("Read ./orders.json");
    let mut out = crate::initial();
    out.provenance.plan = Some(json!({"rules":[]}));
    record(&req, &mut out);
    let replay =
        CompileRequest::create("Read ./orders.json").with_plan(out.provenance.plan.unwrap());
    assert_eq!(
        columns(world(&replay), "orders.json"),
        Some(vec!["id".into(), "status".into()])
    );
    let fresh =
        replay.with_knowledge(json!({"observed":[{"path":"orders.json", "state":"absent"}]}));
    assert_eq!(columns(world(&fresh), "orders.json"), None);
}
/// The DEV2-P4 requests (E39 PILOT14, DEV public rows): they name `amount_cents` and `customer`,
/// never `status`; a seat reads « paid rows » as `status == "paid"` (F2-Q1).
const DEV2_P4_EN: &str = "Read ./data/input.csv. Sum integer amount_cents of paid rows by customer; every duplicate contributes and empty input creates no groups. Write a JSON array sorted by customer with exactly textual customer and integer total_cents per group to ./out/result.json.";
const DEV2_P4_FR: &str = "Lis ./data/input.csv. Somme les amount_cents entiers des lignes paid par customer ; chaque doublon compte et aucun groupe ne doit être inventé sur une entrée vide. Écris un tableau JSON trié par customer, avec exactement customer textuel et total_cents entier par groupe dans ./out/result.json.";
const PAID_EN: &str = "Sum integer amount_cents of paid rows by customer";
const PAID_FR: &str = "Somme les amount_cents entiers des lignes paid par customer";
const DEV2_COLUMNS: [&str; 5] = ["id", "customer", "status", "kind", "amount_cents"];

/// The host's observation of `./data/input.csv`: a CSV header and the categorical values it
/// recorded (DEV2-P4 nominal: status holds only paid).
fn observed_csv(columns: &[&str], values: &Value) -> Value {
    json!({"observed": [{"path": "./data/input.csv", "state": "observed", "kind": "csv",
        "complete": false, "columns": columns, "values": values}]})
}
fn dev2_values() -> Value {
    json!({"amount_cents": ["100", "50"], "customer": ["acme", "beta"], "id": ["r1", "r2"],
        "kind": ["sale"], "status": ["paid"]})
}
/// A seat's typed rule over the clause: `field` compared to the text `literal`.
fn seat_rule(text: &str, field: &str, comparator: Comparator, literal: &str) -> Rule {
    Rule::typed(
        text,
        vec![Clause::new(
            field,
            comparator,
            Operand::Text(literal.into()),
        )],
        Junction::And,
        Shape::default(),
    )
}
/// `rule` grounded over `./data/input.csv` for `intent`, `world` observed, `answers` given.
fn grounded(
    rule: Rule,
    intent: &str,
    world: Value,
    answers: &[(&str, &str)],
) -> (Option<Rule>, CompileOutcome) {
    let mut request = CompileRequest::create(intent).with_knowledge(world);
    for &(key, raw) in answers {
        request = request.answer(key, raw);
    }
    let mut out = crate::initial();
    let bound = ground_rule(
        rule,
        "./data/input.csv",
        &request,
        &mut out,
        &mut BTreeSet::new(),
    );
    (bound, out)
}
/// The decision's grounding entry of `field` (null when none).
fn grounding_of(out: &CompileOutcome, field: &str) -> Value {
    let entries = out.provenance.decision.as_ref();
    entries
        .and_then(|d| d["grounding"].as_array())
        .and_then(|e| e.iter().find(|e| e["field"] == field))
        .cloned()
        .unwrap_or(Value::Null)
}
/// F2-Q1: the request states the VALUE (`paid`), never the field; the host observed that value
/// in `status` alone, so the seat's `status` is bound by the observation, no question asked.
#[test]
fn a_stated_value_observed_in_one_column_binds_the_seat_field() {
    let not_paid = "Sum integer amount_cents of rows that are not paid by customer";
    let cases = [
        (DEV2_P4_EN, PAID_EN, Comparator::Eq),
        (DEV2_P4_FR, PAID_FR, Comparator::Eq),
        (not_paid, not_paid, Comparator::Ne),
    ];
    for (intent, text, comparator) in cases {
        let rule = seat_rule(text, "status", comparator, "paid");
        let world = observed_csv(&DEV2_COLUMNS, &dev2_values());
        let (bound, out) = grounded(rule.clone(), intent, world, &[]);
        assert_eq!(bound, Some(rule), "{intent}: {:#?}", out.questions);
        assert!(out.questions.is_empty(), "{intent}: {:#?}", out.questions);
        let status = grounding_of(&out, "status");
        assert_eq!(status["bound_by"], "observation", "{status:#}");
        assert_eq!(status["admissible"], true, "{status:#}");
        assert_eq!(status["grade"], "declared", "{status:#}");
        assert_eq!(status["witness"], "paid", "{status:#}");
    }
}
/// The witness is the one spelling law's (R4 A5): an observed spelling canonically equivalent to
/// the stated literal is the literal.
#[test]
fn a_canonically_equivalent_observed_spelling_witnesses_the_stated_literal() {
    let intent = "Read ./data/input.csv and keep the livr\u{e9} rows";
    let rule = seat_rule(
        "keep the livr\u{e9} rows",
        "status",
        Comparator::Eq,
        "livr\u{e9}",
    );
    let values = json!({"status": ["livre\u{301}", "open"], "kind": ["sale"]});
    let (bound, out) = grounded(rule, intent, observed_csv(&DEV2_COLUMNS, &values), &[]);
    assert!(bound.is_some(), "{:#?}", out.questions);
    assert_eq!(grounding_of(&out, "status")["bound_by"], "observation");
}
/// Every adverse shape keeps the closed field question (F2-Q1): no witness, no binding.
#[test]
fn a_value_the_observation_does_not_place_in_one_column_keeps_the_question() {
    let two_columns = json!({"status": ["paid", "open"], "payment": ["paid", "unpaid"]});
    let partial = json!({"status": ["pending", "refunded"], "kind": ["sale"]});
    let with_paid_column = ["id", "customer", "status", "paid", "amount_cents"];
    let settled = DEV2_P4_EN.replace("paid rows", "settled rows");
    let paid = || seat_rule(PAID_EN, "status", Comparator::Eq, "paid");
    let cases: [(&str, Rule, &str, Value); 6] = [
        // The literal is observed in two columns.
        (
            "two columns",
            paid(),
            DEV2_P4_EN,
            observed_csv(
                &["id", "customer", "status", "payment", "amount_cents"],
                &two_columns,
            ),
        ),
        // A partial sample that never shows the literal proves nothing.
        (
            "partial",
            paid(),
            DEV2_P4_EN,
            observed_csv(&DEV2_COLUMNS, &partial),
        ),
        // The request never states the literal: a seat's word (« settled » read as paid).
        (
            "not stated",
            paid(),
            &settled,
            observed_csv(&DEV2_COLUMNS, &dev2_values()),
        ),
        // The stated word also names an observed column.
        (
            "column name",
            paid(),
            DEV2_P4_EN,
            observed_csv(&with_paid_column, &json!({"status": ["paid", "open"]})),
        ),
        // A text containment is no equality with the literal.
        (
            "contains",
            seat_rule(PAID_EN, "status", Comparator::Contains, "paid"),
            DEV2_P4_EN,
            observed_csv(&DEV2_COLUMNS, &dev2_values()),
        ),
        // The seat's field is no observed key: never rebound to the witnessed column.
        (
            "invented field",
            seat_rule(PAID_EN, "statut", Comparator::Eq, "paid"),
            DEV2_P4_EN,
            observed_csv(&DEV2_COLUMNS, &dev2_values()),
        ),
    ];
    for (case, rule, intent, world) in cases {
        let field = rule.source_fields()[0].clone();
        let (bound, out) = grounded(rule, intent, world, &[]);
        assert!(bound.is_none(), "{case}: bound without a witness");
        assert_eq!(out.questions[0].key, "const.rule_field_1", "{case}");
        assert_eq!(
            out.questions[0].answer_type,
            crate::QuestionType::Choice,
            "{case}"
        );
        let entry = grounding_of(&out, &field);
        assert_eq!(entry["bound_by"], Value::Null, "{case}: {entry:#}");
        assert_eq!(entry["admissible"], false, "{case}: {entry:#}");
    }
}
/// DEV7-P3-EN: the host recorded no value of `status` (two sampled rows, two distinct values: no
/// categorical set), so nothing witnesses it and the question stays (F2-Q1).
#[test]
fn a_column_with_no_recorded_values_keeps_the_question() {
    let intent = "Read ./data/input.csv. Sum integer amount_cents of paid rows. If the total is below 100, finish without a report; otherwise display the total and request human confirmation before writing. Only after approval, the report written to ./out/result.json is a JSON object with integer total_cents, and integer count only when explicitly requested.";
    let columns = ["id", "status", "kind", "amount_cents", "approved"];
    let values = json!({"approved": ["false"], "kind": ["sale"]});
    let text = "Sum integer amount_cents of paid rows.";
    // The seat's typed reading, and its verbatim DEV7-P3-EN program.
    let rules = [
        seat_rule(text, "status", Comparator::Eq, "paid"),
        Rule::program(
            text,
            ".records | map(select(.status == \"paid\") | .amount_cents | tonumber) | add // 0",
            vec!["status".into(), "amount_cents".into()],
        ),
    ];
    for rule in rules {
        let (bound, out) = grounded(rule, intent, observed_csv(&columns, &values), &[]);
        assert!(bound.is_none());
        assert_eq!(out.questions[0].key, "const.rule_field_1");
        assert_eq!(grounding_of(&out, "status")["bound_by"], Value::Null);
    }
}
/// The DEV2-P4-FR seat program, verbatim: it compares `status` to `paid` literally.
const PAID_PROGRAM: &str = ".records | map(select(.status == \"paid\")) | group_by(.customer) | map({customer: .[0].customer, total_cents: (map(.amount_cents | tonumber) | add)}) | sort_by(.customer)";
fn program(jq: &str, columns: &[&str]) -> Rule {
    Rule::program(
        PAID_FR,
        jq,
        columns.iter().map(|c| (*c).to_owned()).collect(),
    )
}
/// F2-Q1: a verified program witnesses its field only by a literal comparison of that field to
/// the stated value (`.F == "L"`, `.F != "L"`, `."F" == "L"`, or the reverse operand order).
#[test]
fn a_program_comparing_the_field_to_the_stated_value_binds_it() {
    let forms = [
        PAID_PROGRAM,
        "[.records[] | select(\"paid\" == .status) | .amount_cents | tonumber] | add",
        "[.records[] | select(.\"status\" == \"paid\") | .amount_cents | tonumber] | add",
        "[.records[] | select(.status != \"paid\") | .amount_cents | tonumber] | add",
        "[.records[] | if .status==\"paid\" then .amount_cents | tonumber else 0 end] | add",
    ];
    for jq in forms {
        let rule = program(jq, &["customer", "status", "amount_cents"]);
        let world = observed_csv(&DEV2_COLUMNS, &dev2_values());
        let (bound, out) = grounded(rule.clone(), DEV2_P4_FR, world, &[]);
        assert_eq!(bound, Some(rule), "{jq}: {:#?}", out.questions);
        let status = grounding_of(&out, "status");
        assert_eq!(status["bound_by"], "observation", "{jq}: {status:#}");
        assert_eq!(status["witness"], "paid", "{jq}: {status:#}");
    }
}
/// A program that holds the value anywhere but in a literal comparison of the field keeps the
/// question: another field compared, the value elsewhere, a nested path, a tighter operator, a
/// longer field name.
#[test]
fn a_program_without_that_literal_comparison_keeps_the_question() {
    let cases = [
        "[.records[] | select(.kind == \"paid\") | .status]",
        "{status: \"paid\", total: ([.records[] | .amount_cents | tonumber] | add)}",
        "[.records[] | select(.meta.status == \"paid\") | .status]",
        "[.records[] | select(.status == \"paid\" + \"x\") | .status]",
        "[.records[] | select(.status_code == \"paid\") | .status]",
    ];
    for jq in cases {
        let rule = program(jq, &["status", "kind"]);
        let world = observed_csv(&DEV2_COLUMNS, &dev2_values());
        let (bound, out) = grounded(rule, DEV2_P4_FR, world, &[]);
        assert!(bound.is_none(), "{jq}: bound without a literal comparison");
        assert_eq!(out.questions[0].key, "const.rule_field_1", "{jq}");
        assert_eq!(
            grounding_of(&out, "status")["bound_by"],
            Value::Null,
            "{jq}"
        );
    }
}
/// An answer the round carries for the field question is read, never overridden by a witness
/// (R4 A6): the human's choice rebinds the rule.
#[test]
fn an_answer_to_the_field_question_wins_over_the_observation() {
    let rule = seat_rule(PAID_EN, "status", Comparator::Eq, "paid");
    let world = observed_csv(&DEV2_COLUMNS, &dev2_values());
    let answer = [("const.rule_field_1", "\"kind\"")];
    let (bound, out) = grounded(rule, DEV2_P4_EN, world, &answer);
    assert_eq!(
        bound.map(|r| r.source_fields()),
        Some(vec!["kind".to_owned()])
    );
    assert_eq!(grounding_of(&out, "kind")["bound_by"], "answer");
}
#[test]
fn arbitrary_program_bytes_are_never_renamed() {
    let rule = Rule::program(
        "ticket 42",
        "[.records[] | select(.ticket == 42)]",
        vec!["ticket".into()],
    );
    assert!(rule.with_source_field("ticket", "id").is_none());
    assert_eq!(
        rule.with_source_field("ticket", "ticket").unwrap().jq(),
        rule.jq()
    );
}

#[test]
fn protected_parent_does_not_hide_the_only_observed_source() {
    let sample = crate::observation::records(&[json!({"id": 1, "status": "open"})]);
    let world = json!({"observed": [{
        "path": "./records/input.json", "state": "observed", "kind": "json",
        "complete": true, "columns": sample.columns, "common_columns": sample.common,
    }]});
    let intent = "Lis ./records/input.json. Ne modifie rien dans ./records.";
    assert_eq!(
        for_intent(Some(&world), intent),
        Some(vec!["id".into(), "status".into()])
    );
    assert_eq!(for_intent(Some(&world), "Lis ./records"), None);
    assert_eq!(columns(Some(&world), "./records"), None);
}
