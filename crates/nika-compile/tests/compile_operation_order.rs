// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A count the request states over the rows it filters keeps both operations (R4 F1, V9 A10).
//! « count the rows where status is paid » compiled READY into a filter that wrote the rows: the
//! reader consumed the words before the relative clause without reading them. A stage those
//! words state (a count, an aggregate) now runs after the filter; words the grammar cannot
//! account for leave the clause unread (cognition, never READY); a plan recorded with the old
//! filter-only reading is refused on replay, and a fresh compile recovers. Lane tests (A10):
//! the CLI and Session journeys belong to the primary's frozen artifact.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, HotPolicy, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;

const CSV: (&str, &[&str]) = ("./data/input.csv", &["id", "amount_usd", "status"]);

fn compiled(intent: &str) -> CompileOutcome {
    let request = CompileRequest::create(intent).with_knowledge(common::observed(&[CSV]));
    compile(&request).unwrap()
}

/// The rule a READY candidate's compute runs, after any law in front of it, number law folded.
fn ready_rule(intent: &str) -> String {
    let out = compiled(intent);
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    let compute = common::compute(out.candidate.as_deref().unwrap());
    compute.rsplit('\n').next().unwrap_or_default().to_owned()
}

fn rule_record(out: &CompileOutcome) -> Value {
    out.provenance.plan.as_ref().unwrap()["rules"][0].clone()
}

const FUSED: &str = "read ./data/input.csv, count the rows where status is paid, write the count to ./out/result.json";

#[test]
fn a_fused_count_keeps_its_filter_and_its_count() {
    let paid = "[.records[] | select(.status == \"paid\")] | {\"count\": length}";
    let over = "[.records[] | select(((.amount_usd | num) | dkey) > (\"10\" | dkey))] | {\"count\": length}";
    let both = "[.records[] | select(.status == \"paid\" and ((.amount_usd | num) | dkey) > (\"10\" | dkey))] | {\"count\": length}";
    for (intent, rule) in [
        (FUSED, paid),
        (
            "read ./data/input.csv, count the rows where amount_usd is over 10, write the count to ./out/result.json",
            over,
        ),
        (
            "read ./data/input.csv, count the rows where status is paid and amount_usd is over 10, write the count to ./out/result.json",
            both,
        ),
        // The records' own noun stands for the rows the relative clause keeps.
        (
            "read ./data/input.csv, count the orders where status is paid, write the count to ./out/result.json",
            paid,
        ),
        (
            "read ./data/input.csv, compte les lignes dont le status est paid, write it to ./out/result.json",
            paid,
        ),
        (
            "read ./data/input.csv, the number of rows whose status is paid, write it to ./out/result.json",
            "[.records[] | select(.status == \"paid\")] | {\"number\": length}",
        ),
    ] {
        assert_eq!(ready_rule(intent), rule, "{intent}");
    }
    // The plan carries both operations: the clause and the count after it.
    let record = rule_record(&compiled(FUSED));
    assert_eq!(record["clauses"][0]["field"], "status", "{record}");
    assert_eq!(
        record["shape"]["aggregations"][0]["op"], "count",
        "{record}"
    );
    assert_eq!(
        record["shape"]["aggregations"][0]["name"], "count",
        "{record}"
    );
}

#[test]
fn split_forms_and_plain_filters_keep_their_reading() {
    let paid = "[.records[] | select(.status == \"paid\")]";
    for (intent, rule) in [
        (
            "read ./data/input.csv, keep the rows where status is paid, write them to ./out/result.json",
            paid.to_owned(),
        ),
        (
            "read ./data/input.csv, keep the rows where status is paid, count them, write the count to ./out/result.json",
            format!("{paid} | {{\"count\": length}}"),
        ),
        (
            "read ./data/input.csv, keep the rows where status is paid, keep the rows where amount_usd is over 10, count them, write the count to ./out/result.json",
            "[.records[] | select(.status == \"paid\" and ((.amount_usd | num) | dkey) > (\"10\" | dkey))] | {\"count\": length}".to_owned(),
        ),
        // A verb the reader does not list leads a plain filter, as before: never dropped work.
        (
            "read ./data/input.csv, filter the rows where status is paid, write them to ./out/result.json",
            paid.to_owned(),
        ),
        (
            "read ./data/input.csv, show me the rows where status is paid, write them to ./out/result.json",
            paid.to_owned(),
        ),
    ] {
        assert_eq!(ready_rule(intent), rule, "{intent}");
    }
}

#[test]
fn a_lead_the_grammar_cannot_account_for_is_never_ready() {
    for intent in [
        // A modifier between the determiner and the records' noun is a predicate.
        "read ./data/input.csv, keep the paid rows where amount_usd is over 10, write them to ./out/result.json",
        "read ./data/input.csv, count the paid rows where amount_usd is over 10, write the count to ./out/result.json",
        // A stage word the stage grammar does not read whole over the rows.
        "read ./data/input.csv, sort the rows where status is paid, write them to ./out/result.json",
        "read ./data/input.csv, how many rows where status is paid, write it to ./out/result.json",
    ] {
        let out = compiled(intent);
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    }
}

/// The plan the pre-fix reader recorded for [`FUSED`] (captured at 342c90068 with the preserved
/// A8 WIP present): its rule is the filter alone, the count dropped.
const PRE_FIX_PLAN: &str = r#"{"bindings":[{"literal":"./out/result.json","role":"path"},{"literal":"./data/input.csv","role":"path"},{"literal":"./out/result.json","role":"path"}],"constraints":[],"effects":[{"evidence":"write the count to ./out/result.json","policy":"automatic","policy_literal":null,"target":"./out/result.json","verb":"write"}],"obligations":[],"observed_world":{"observed":[{"columns":["id","amount_usd","status"],"complete":false,"kind":"csv","path":"./data/input.csv","state":"observed"}]},"operations":[{"categories":[],"detail":"./data/input.csv","evidence":"read ./data/input.csv","op":"read"},{"categories":[],"detail":"count the rows where status is paid","evidence":"count the rows where status is paid","op":"compute"}],"rules":[{"clauses":[{"comparator":"==","field":"status","value":"paid","value_kind":"text"}],"comparator":"==","field":"status","fields":["status"],"jq":"[.records[] | select(.status == \"paid\")]","junction":"and","lines":false,"program":null,"shape":{"aggregations":[],"columns":[],"derived":[],"descending":false,"distinct":false,"distinct_by":[],"group_by":null,"join_on":null,"limit":null,"renames":[],"sort_by":null},"summary":false,"synthesized":true,"text":"count the rows where status is paid","value":"paid"}],"slots":[],"strategy":"hot","trigger":null,"unknowns":[]}"#;

#[test]
fn a_filter_only_plan_recorded_for_a_fused_count_is_refused_then_recompiled() {
    let plan: Value = serde_json::from_str(PRE_FIX_PLAN).unwrap();
    let request = CompileRequest::create(FUSED)
        .with_knowledge(common::observed(&[CSV]))
        .with_plan(plan);
    let out = compile(&request).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics.iter().any(|d| d.message.contains(
            "the recorded rule for `count the rows where status is paid` is not what its words say"
        ) && d
            .message
            .contains("Compile the intent again without it")),
        "{out:#?}"
    );
    // The same request compiled afresh counts.
    assert_eq!(
        ready_rule(FUSED),
        "[.records[] | select(.status == \"paid\")] | {\"count\": length}"
    );
}

/// The order the request states is the order the emitted program runs (R4 F5): a top-N then a
/// filter, and a filter then a top-N, are two different programs, with or without « then ».
#[test]
fn a_filter_and_a_top_n_run_in_the_order_the_request_states() {
    let top2 = "sort_by((.amount_usd | num) | dkey) | reverse | dtie(2; (.amount_usd | num) | dkey; .; \"`amount_usd`\") | .[:2]";
    let paid = "select(.status == \"paid\")";
    let intent = |clauses: &str, write: &str| {
        format!("read ./data/input.csv, {clauses}, {write} to ./out/result.json")
    };
    for (clauses, write, rule) in [
        (
            "keep the rows where status is paid, keep the 2 rows with the highest amount_usd",
            "write them",
            format!("[.records[] | {paid}] | {top2}"),
        ),
        (
            "keep the rows where status is paid, then keep the 2 rows with the highest amount_usd",
            "write them",
            format!("[.records[] | {paid}] | {top2}"),
        ),
        (
            "keep the 2 rows with the highest amount_usd, keep the rows where status is paid",
            "write them",
            format!(".records | {top2} | map({paid})"),
        ),
        (
            "keep the 2 rows with the highest amount_usd, then keep the rows where status is paid",
            "write them",
            format!(".records | {top2} | map({paid})"),
        ),
        (
            "keep the rows where status is paid, keep the 2 rows with the highest amount_usd, keep the rows where amount_usd is over 50",
            "write them",
            format!(
                "[.records[] | {paid}] | {top2} | map(select(((.amount_usd | num) | dkey) > (\"50\" | dkey)))"
            ),
        ),
        (
            "keep the 2 rows with the highest amount_usd, count them",
            "write the count",
            format!(".records | {top2} | {{\"count\": length}}"),
        ),
    ] {
        assert_eq!(ready_rule(&intent(clauses, write)), rule, "{clauses}");
    }
}

/// The typed duties a READY compile records (R4 A3), each (kind, anchored excerpt, place in
/// the stated order, fields read, carrier). The plan's own obligation list stays empty: the
/// ledger witnesses operations, it never claims the request closed; and no filter duty is
/// carried by a label any more.
fn typed_duties(out: &CompileOutcome) -> Vec<(String, String, u64, Value, String)> {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["obligations"],
        json!([])
    );
    let ledger = out.provenance.decision.as_ref().unwrap()["ledger"]
        .as_array()
        .unwrap()
        .clone();
    assert!(
        ledger
            .iter()
            .all(|d| d["kind"] != "filter" || d.get("position").is_some()),
        "{ledger:#?}"
    );
    ledger
        .iter()
        .filter(|d| d.get("position").is_some())
        .map(|d| {
            (
                d["kind"].as_str().unwrap().to_owned(),
                d["evidence"].as_str().unwrap().to_owned(),
                d["position"].as_u64().unwrap(),
                d["reads"].clone(),
                d["realized_by"].as_str().unwrap_or("unresolved").to_owned(),
            )
        })
        .collect()
}

fn duty(
    kind: &str,
    evidence: &str,
    at: u64,
    reads: &[&str],
) -> (String, String, u64, Value, String) {
    let carrier = "compute".to_owned();
    (
        kind.to_owned(),
        evidence.to_owned(),
        at,
        json!(reads),
        carrier,
    )
}

const PAID: &str = "keep the rows where status is paid";
const TOP2: &str = "keep the 2 rows with the highest amount_usd";
const COUNT: &str = "count the rows where status is paid";
const WRITE: &str = "write the count to ./out/result.json";
const THEM: &str = "write them to ./out/result.json";

/// Every operation the request states is a typed duty the emitted computation realizes (R4
/// A3): one per filter clause, count, order and cut, anchored on the excerpt that states it,
/// in the stated order.
#[test]
fn each_stated_operation_is_a_duty_the_emitted_computation_realizes() {
    assert_eq!(
        typed_duties(&compiled(FUSED)),
        [
            duty("filter", COUNT, 0, &["status"]),
            duty("count", COUNT, 1, &[])
        ]
    );
    let then =
        format!("read ./data/input.csv, {TOP2}, then {PAID}, write them to ./out/result.json");
    assert_eq!(
        typed_duties(&compiled(&then)),
        [
            duty("order", TOP2, 0, &["amount_usd"]),
            duty("limit", TOP2, 1, &[]),
            duty("filter", PAID, 2, &["status"]),
        ]
    );
    let over = "keep the rows where amount_usd is over 10";
    let both = format!(
        "read ./data/input.csv, {PAID}, {over}, count them, write the count to ./out/result.json"
    );
    assert_eq!(
        typed_duties(&compiled(&both)),
        [
            duty("filter", PAID, 0, &["status"]),
            duty("filter", over, 1, &["amount_usd"]),
            duty("count", "count them", 2, &[]),
        ]
    );
}

/// A request reading the CSV, stating `evidence` and then `write`, and a seat's plan for it: a
/// read, one compute step anchored on `evidence` and labelled `detail` proposing
/// `computation`, and the write; its regions are the request's three clauses. Nothing else is
/// annotated.
fn seat(evidence: &str, write: &str, detail: &str, computation: &Value) -> (String, Value) {
    let intent = format!("read ./data/input.csv, {evidence}, {write}");
    let plan = json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": "read ./data/input.csv"},
            {"op": "compute", "detail": detail, "evidence": evidence, "computation": computation}
        ],
        "effects": [{"verb": "write", "target": "./out/result.json", "policy": "automatic", "evidence": write}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": "read ./data/input.csv,", "role": "operation"},
            {"text": format!("{evidence},"), "role": "operation"},
            {"text": write, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    });
    (intent, plan)
}

/// A cold compile of `intent` whose one seat answer is `plan` (HOT is off, so the seat plans),
/// judged by the explicit approving double (R4 A11): these tests read the bound rule.
async fn cold((intent, plan): (String, Value)) -> CompileOutcome {
    let request = CompileRequest::create(&intent)
        .with_knowledge(common::observed(&[CSV]))
        .with_hot_policy(HotPolicy::Off)
        .with_authoring_policy(common::policy());
    let provider = common::Provider::new(plan);
    compile_with_provider(&request, &common::Judged::approving(&provider))
        .await
        .unwrap()
}

fn bound_rule(out: &CompileOutcome) -> String {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let compute = common::compute(out.candidate.as_deref().unwrap());
    compute.rsplit('\n').next().unwrap_or_default().to_owned()
}

fn filtering(clauses: &Value) -> Value {
    json!({"present": true, "join": "and", "clauses": clauses})
}

fn ranking(order: &str, limit: &str, clauses: &Value) -> Value {
    json!({"present": true, "join": "and", "clauses": clauses, "sort_by": "amount_usd", "order": order, "limit": limit})
}

/// A seat proposes a filter-only rule for « count the rows where status is paid » and labels
/// its step a count (R4 A3). The reader's rule for those very words, promoted beside it, binds
/// (the existing twin law already did before A3): the workflow writes the count, and the ledger
/// now says why, a filter and a count realized by the emitted computation, not by the label.
#[tokio::test]
async fn a_count_free_proposal_yields_to_the_reading_of_the_request() {
    let paid = json!([{"field": "status", "op": "eq", "value": "paid", "value_field": ""}]);
    let (intent, plan) = seat(COUNT, WRITE, "count the paid rows", &filtering(&paid));
    assert_eq!(intent, FUSED);
    let out = cold((intent, plan)).await;
    assert_eq!(
        bound_rule(&out),
        "[.records[] | select(.status == \"paid\")] | {\"count\": length}"
    );
    assert_eq!(
        typed_duties(&out),
        [
            duty("filter", COUNT, 0, &["status"]),
            duty("count", COUNT, 1, &[])
        ]
    );
}

/// Each parameter a seat changes is caught through the whole pipeline (R4 A3). A field, a
/// comparator, a literal, a direction or a count the proposal changes was already caught before
/// A3 (the reader's promoted rule for the same words binds, or the seat law refuses a literal or
/// a count the request never states). An omitted later step and a reordered one were READY
/// before A3 (`.records | top 2`, then `[paid] | top 2`): the witness catches them, and the
/// reading of the request's own clauses binds. Every typed duty is realized by the rule bound.
#[tokio::test]
async fn a_proposal_changing_a_parameter_is_never_the_rule_bound() {
    let clause = |field: &str, op: &str, value: &str| json!([{"field": field, "op": op, "value": value, "value_field": ""}]);
    let count = "[.records[] | select(.status == \"paid\")] | {\"count\": length}";
    for computation in [
        filtering(&clause("id", "eq", "paid")),
        filtering(&clause("status", "ne", "paid")),
        filtering(&clause("status", "eq", "open")),
    ] {
        let out = cold(seat(COUNT, WRITE, COUNT, &computation)).await;
        assert_eq!(bound_rule(&out), count, "{computation}");
        assert_eq!(typed_duties(&out).len(), 2, "{computation}");
    }
    let top2 = "sort_by((.amount_usd | num) | dkey) | reverse | dtie(2; (.amount_usd | num) | dkey; .; \"`amount_usd`\") | .[:2]";
    for computation in [
        ranking("asc", "2", &json!([])),
        ranking("desc", "3", &json!([])),
    ] {
        let out = cold(seat(TOP2, THEM, TOP2, &computation)).await;
        assert_eq!(
            bound_rule(&out),
            format!(".records | {top2}"),
            "{computation}"
        );
        assert_eq!(typed_duties(&out).len(), 2, "{computation}");
    }
    let stated = format!("{TOP2}, then {PAID}");
    for computation in [
        ranking("desc", "2", &json!([])),
        ranking("desc", "2", &clause("status", "eq", "paid")),
    ] {
        let out = cold(seat(&stated, THEM, &stated, &computation)).await;
        assert_eq!(
            bound_rule(&out),
            format!(".records | {top2} | map(select(.status == \"paid\"))"),
            "{computation}"
        );
        assert_eq!(typed_duties(&out).len(), 3, "{computation}");
    }
}

/// A selection of the rows the rule grammar cannot read is named work, never context (R4 A10).
/// « keep the rows whose status is a » compiled READY with its filter dropped: the literal
/// defeated the grammar, and the clause was recorded as context the material realizes. It is
/// now an unresolved clause: HOT is rejected and the door, with no cognition configured, names
/// it and says it needs cognition. The same selection over a literal the grammar reads still
/// runs as the program's filter, after the sort.
#[test]
fn an_unread_selection_of_the_rows_is_work_never_context() {
    let clause = "keep the rows whose status is a";
    let out = compiled(&format!(
        "read ./data/input.csv, sort the rows by amount_usd, then {clause}, write them to ./out/result.json"
    ));
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let ledger = out.provenance.decision.as_ref().unwrap()["ledger"].clone();
    let stated = ledger
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["evidence"] == clause);
    assert!(
        stated.is_some_and(|d| d["kind"] == "work" && d["state"] == "unresolved"),
        "{ledger:#}"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["route"],
        json!(["hot rejected: 1 unresolved clause(s)", "needs cognition"])
    );
    let unresolved = format!("Unresolved clause: {clause}. No requested operation was dropped");
    assert!(format!("{out:?}").contains(&unresolved), "{out:#?}");
    let sorted = "sort the rows by amount_usd";
    let paid = compiled(&format!(
        "read ./data/input.csv, {sorted}, then {PAID}, write them to ./out/result.json"
    ));
    assert_eq!(
        bound_rule(&paid),
        ".records | sort_by(.amount_usd | tonumber? // .) | map(select(.status == \"paid\"))"
    );
    assert_eq!(
        typed_duties(&paid),
        [
            duty("order", sorted, 0, &["amount_usd"]),
            duty("filter", PAID, 1, &["status"]),
        ]
    );
}

/// An operation the request does not state is never run (R4 A3, the primary's review
/// hypothesis, reproduced before this change): a seat's compute detail, or a replayed record's,
/// joining the request's own clauses with an extra filter before the cut (« paid ; top 2 ;
/// paid ») compiled READY, and its program ranked only the paid rows. Every part is read, so
/// the rule bound runs exactly the stated operations: the reading of the request's clauses.
#[tokio::test]
async fn an_operation_the_request_does_not_state_is_never_run() {
    let stated = format!("{TOP2}, then {PAID}");
    let extra = format!("{PAID} ; {TOP2} ; {PAID}");
    let top2 = "sort_by((.amount_usd | num) | dkey) | reverse | dtie(2; (.amount_usd | num) | dkey; .; \"`amount_usd`\") | .[:2]";
    let reading = format!(".records | {top2} | map(select(.status == \"paid\"))");
    let (intent, mut plan) = seat(&stated, THEM, &extra, &json!({}));
    plan["steps"][1]
        .as_object_mut()
        .unwrap()
        .remove("computation");
    let out = cold((intent.clone(), plan)).await;
    assert_eq!(bound_rule(&out), reading);
    assert_eq!(typed_duties(&out).len(), 3);
    let mut record = compiled(&intent).provenance.plan.clone().unwrap();
    record["operations"][1]["detail"] = json!(extra);
    let request = CompileRequest::create(&intent)
        .with_knowledge(common::observed(&[CSV]))
        .with_plan(record);
    let out = compile(&request).unwrap();
    assert_eq!(bound_rule(&out), reading);
    assert_eq!(typed_duties(&out).len(), 3);
}

/// A configured cognition carries the selection the grammar cannot read (R4 A10): HOT is
/// rejected on the unresolved clause, the seat is actually called, and its typed filter over the
/// request's own literal is bound beside the sort the grammar reads (the order duty realized;
/// the unread words carried unverified, never a closure claim).
#[tokio::test]
async fn a_configured_seat_carries_the_selection_the_grammar_cannot_read() {
    let stated = "sort the rows by amount_usd, then keep the rows whose status is a";
    let computation = json!({"present": true, "join": "and", "sort_by": "amount_usd", "order": "asc",
        "clauses": [{"field": "status", "op": "eq", "value": "a", "value_field": ""}]});
    let (intent, plan) = seat(stated, THEM, stated, &computation);
    let request = CompileRequest::create(&intent)
        .with_knowledge(common::observed(&[CSV]))
        .with_authoring_policy(common::policy());
    let provider = common::Provider::new(plan);
    // Judged by the explicit approving double (R4 A11): this test reads the emitted workflow.
    let out = compile_with_provider(&request, &common::Judged::approving(&provider))
        .await
        .unwrap();
    assert!(provider.calls.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    let route = out.provenance.decision.as_ref().unwrap()["route"].clone();
    assert_eq!(route[0], "hot rejected: 1 unresolved clause(s)", "{route}");
    assert!(route.to_string().contains("cold"), "{route}");
    assert_eq!(
        bound_rule(&out),
        "[.records[] | select(.status == \"a\")] | sort_by(.amount_usd | tonumber? // .)"
    );
    assert_eq!(
        typed_duties(&out),
        [duty(
            "order",
            "sort the rows by amount_usd",
            0,
            &["amount_usd"]
        )]
    );
    let ledger = out.provenance.decision.as_ref().unwrap()["ledger"].clone();
    let same: Vec<&Value> = ledger
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["evidence"] == stated)
        .collect();
    assert_eq!(same.len(), 1, "one duty per excerpt: {ledger:#}");
    // No law reads these words from the bytes: the duty waited, unverified, for the judgment
    // this round made over the candidate (R4 A11), which settled it.
    assert_eq!(same[0]["witness"], "judged", "{ledger:#}");
    assert_eq!(same[0]["state"], "realized", "{ledger:#}");
    let note = same[0]["note"].as_str().unwrap_or_default();
    assert!(note.starts_with("judged carried by "), "{ledger:#}");
}
