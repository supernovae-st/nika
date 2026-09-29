// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An output shape the request states is a typed part of the plan (E38 V2). « write the sum as a
//! bare JSON number to ./out/result.json » had no typed carrier: a total the engine computes is
//! written to a JSON file as the object naming it (`{"sum": 70}`), so the plan left the sum to a
//! seat program outside the number law, and the whole-request judge refused that program without
//! placing a defect. The plan's write now states `alone`: the one typed total is written as its
//! value, under the number law, and what cannot hold one value alone is refused, never rounded
//! into another shape.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Judged, Provider, policy};

/// The frozen E38 C1 request, byte for byte.
const C1: &str = "read ./data/input.csv, sum qty over the rows where status is shipped, write the sum as a bare JSON number to ./out/result.json";
const INPUT: (&str, &[&str]) = ("./data/input.csv", &["id", "item", "status", "qty"]);

/// The plan a seat states for a read, one typed computation over it and one write, with the
/// write's own words and, when given, its `alone`.
fn proposal(
    intent: &str,
    computed: &str,
    aggregations: &Value,
    write: &str,
    alone: Option<bool>,
) -> Value {
    let mut effect =
        json!({"verb": "write", "target": write, "policy": "automatic", "evidence": write});
    if let Some(alone) = alone {
        effect["alone"] = json!(alone);
    }
    let read = "read ./data/input.csv";
    assert!(
        intent.contains(computed) && intent.contains(write),
        "{intent}"
    );
    json!({
        "steps": [
            {"op": "read", "detail": "./data/input.csv", "evidence": read},
            {"op": "compute", "detail": computed, "evidence": computed,
             "computation": {"present": true, "polarity": "keep", "join": "and",
                 "clauses": [{"field": "status", "op": "eq", "value": "shipped", "value_field": ""}],
                 "group_by": "", "aggregations": aggregations, "sort_by": "", "order": "",
                 "columns": [], "derived": [], "limit": "", "renames": [], "distinct_by": []}}
        ],
        "effects": [effect],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": format!("{read},"), "role": "operation"},
            {"text": format!("{computed},"), "role": "operation"},
            {"text": write, "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

fn sum_of_qty() -> Value {
    json!([{"field": "qty", "op": "sum", "as": "sum", "round": ""}])
}

/// A cold compile of `intent` whose seat answers `plan`, judged by the explicit approving double
/// (R4 A11): these tests read the emitted workflow, not a judgment.
async fn cold(intent: &str, plan: &Value) -> CompileOutcome {
    let provider = Provider::new(plan);
    let request = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .with_knowledge(common::observed(&[INPUT]));
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

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

fn workflow(out: &CompileOutcome) -> Value {
    serde_yaml_bw::from_str(out.candidate.as_deref().expect("a candidate")).unwrap()
}

/// What the one write task carries.
fn written(out: &CompileOutcome) -> Value {
    workflow(out)["tasks"]["write_output"]["with"]["content"].clone()
}

/// The findings an outcome states about its write.
fn write_findings(out: &CompileOutcome) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.target.starts_with("write_"))
        .map(|d| d.message.clone())
        .collect()
}

#[tokio::test]
async fn a_bare_total_the_request_states_is_written_as_its_value_under_the_number_law() {
    let computed = "sum qty over the rows where status is shipped";
    let write = "write the sum as a bare JSON number to ./out/result.json";
    let plan = proposal(C1, computed, &sum_of_qty(), write, Some(true));
    let out = cold(C1, &plan).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Cold));
    // The one total, alone: the value, never the object naming it.
    assert_eq!(written(&out), json!("${{ tasks.compute.output.sum }}"));
    // The engine's typed sum, every qty read under the number law (a blank or a word stops the
    // run naming `qty`), not a seat program.
    let compute = common::compute(out.candidate.as_deref().unwrap());
    assert!(
        compute.contains("select(.status == \"shipped\")"),
        "{compute}"
    );
    assert!(
        compute.contains("{\"sum\": (map((.qty | num))"),
        "{compute}"
    );
    // The plan records what it stated, and the record replays to the same bytes: INCOMPLETE
    // until a round judges it (Q2, R4 A11), READY under the judge with the same candidate.
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["effects"][0]["alone"], json!(true), "{record:#}");
    let request = CompileRequest::create(C1)
        .with_knowledge(common::observed(&[INPUT]))
        .with_plan(record.clone());
    let replayed = compile(&request).unwrap();
    assert_eq!(replayed.status, CompileStatus::Incomplete, "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
    let judged = judged_replay(&request).await;
    assert_eq!(judged.status, CompileStatus::Ready, "{judged:#?}");
    assert_eq!(judged.candidate, out.candidate);
    // A record whose write says nothing of `alone` keeps the object: the field is never
    // inferred from the words it was read from.
    let mut unstated = record;
    unstated["effects"][0]
        .as_object_mut()
        .unwrap()
        .remove("alone");
    let request = CompileRequest::create(C1)
        .with_knowledge(common::observed(&[INPUT]))
        .with_plan(unstated);
    let object = compile(&request).unwrap();
    assert_eq!(written(&object), json!("${{ tasks.compute.output }}"));
}

#[tokio::test]
async fn without_alone_a_total_to_json_stays_the_object_naming_it() {
    let intent = "read ./data/input.csv, sum qty over the rows where status is shipped, write the sum to ./out/result.json";
    for alone in [None, Some(false)] {
        let plan = proposal(
            intent,
            "sum qty over the rows where status is shipped",
            &sum_of_qty(),
            "write the sum to ./out/result.json",
            alone,
        );
        let out = cold(intent, &plan).await;
        assert_eq!(out.status, CompileStatus::Ready, "{alone:?}: {out:#?}");
        assert_eq!(
            written(&out),
            json!("${{ tasks.compute.output }}"),
            "{alone:?}"
        );
        let record = out.provenance.plan.clone().unwrap();
        // Unstated or false, the record carries no `alone`: older plans read and hash as before.
        assert!(record["effects"][0].get("alone").is_none(), "{record:#}");
    }
}

#[tokio::test]
async fn several_totals_cannot_be_one_value_alone() {
    let intent = "read ./data/input.csv, sum qty and count the rows where status is shipped, write the sum and the count as bare JSON numbers to ./out/result.json";
    let totals = json!([
        {"field": "qty", "op": "sum", "as": "sum", "round": ""},
        {"field": "", "op": "count", "as": "count", "round": ""}
    ]);
    let plan = proposal(
        intent,
        "sum qty and count the rows where status is shipped",
        &totals,
        "write the sum and the count as bare JSON numbers to ./out/result.json",
        Some(true),
    );
    let out = cold(intent, &plan).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let findings = write_findings(&out);
    assert!(
        findings
            .iter()
            .any(|m| m.contains("one value alone") && m.contains("`sum`") && m.contains("`count`")),
        "{findings:?}\n{out:#?}"
    );
}

#[tokio::test]
async fn a_table_or_a_document_format_cannot_hold_a_total_alone() {
    for path in ["./out/result.csv", "./out/result.yaml", "./out/result.toml"] {
        let write = format!("write the sum as a bare number to {path}");
        let intent = format!(
            "read ./data/input.csv, sum qty over the rows where status is shipped, {write}"
        );
        let plan = proposal(
            &intent,
            "sum qty over the rows where status is shipped",
            &sum_of_qty(),
            &write,
            Some(true),
        );
        let out = cold(&intent, &plan).await;
        assert_ne!(out.status, CompileStatus::Ready, "{path}: {out:#?}");
        assert!(out.candidate.is_none(), "{path}: {out:#?}");
        let findings = write_findings(&out);
        assert!(
            findings
                .iter()
                .any(|m| m.contains("one value alone") && m.contains(path)),
            "{path}: {findings:?}\n{out:#?}"
        );
    }
}

#[tokio::test]
async fn alone_changes_nothing_where_no_object_names_the_value() {
    // A prose file already takes the one total as its value (the conventions' prose law).
    let write = "write the sum as a bare number to ./out/result.txt";
    let intent =
        format!("read ./data/input.csv, sum qty over the rows where status is shipped, {write}");
    for alone in [Some(true), None] {
        let plan = proposal(
            &intent,
            "sum qty over the rows where status is shipped",
            &sum_of_qty(),
            write,
            alone,
        );
        let out = cold(&intent, &plan).await;
        assert_eq!(out.status, CompileStatus::Ready, "{alone:?}: {out:#?}");
        assert_eq!(
            written(&out),
            json!("${{ tasks.compute.output.sum }}"),
            "{alone:?}"
        );
    }
    // Rows are written as they are, an array: no object names them, `alone` removes nothing.
    let write = "write them to ./out/result.json";
    let intent = format!("read ./data/input.csv, keep the rows where status is shipped, {write}");
    let computed = "keep the rows where status is shipped";
    let stated = proposal(&intent, computed, &json!([]), write, Some(true));
    let out = cold(&intent, &stated).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(written(&out), json!("${{ tasks.compute.output }}"));
    let plain = cold(
        &intent,
        &proposal(&intent, computed, &json!([]), write, None),
    )
    .await;
    assert_eq!(plain.candidate, out.candidate);
}

/// The frozen E38 C3 request, byte for byte.
const C3: &str = "read ./data/orders.csv, keep only the rows whose region is north, order them by amount as a number from highest to lowest, and when two rows have the same amount keep the one that comes first in the file first, then keep the first 2 rows and write them to ./out/top.json as a JSON array of objects with exactly two fields each: id as the text written in the file and amount as a JSON number";
const C3_COMPUTED: &str = "keep only the rows whose region is north, order them by amount as a number from highest to lowest, and when two rows have the same amount keep the one that comes first in the file first, then keep the first 2 rows";
const C3_WRITE: &str = "write them to ./out/top.json as a JSON array of objects with exactly two fields each: id as the text written in the file and amount as a JSON number";
const ORDERS: (&str, &[&str]) = ("./data/orders.csv", &["id", "region", "amount", "item"]);

/// The plan a seat states for a C3-shaped request: the north rows ranked by amount, the first 2
/// kept, id and amount projected; `stages` overrides fields of that computation.
fn ranked(intent: &str, computed: &str, write: &str, stages: &Value) -> Value {
    let mut computation = json!({"present": true, "polarity": "keep", "join": "and",
        "clauses": [{"field": "region", "op": "eq", "value": "north", "value_field": ""}],
        "group_by": "", "aggregations": [], "sort_by": "amount", "order": "desc",
        "columns": ["id", "amount"], "derived": [], "limit": "2", "renames": [],
        "distinct_by": []});
    for (key, value) in stages.as_object().unwrap() {
        computation[key] = value.clone();
    }
    let read = "read ./data/orders.csv";
    assert!(
        intent.contains(computed) && intent.contains(write),
        "{intent}"
    );
    json!({
        "steps": [
            {"op": "read", "detail": "./data/orders.csv", "evidence": read},
            {"op": "compute", "detail": computed, "evidence": computed, "computation": computation}
        ],
        "effects": [{"verb": "write", "target": write, "policy": "automatic", "evidence": write}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"text": format!("{read},"), "role": "operation"},
            {"text": computed, "role": "operation"},
            {"text": format!("and {write}"), "role": "effect"}
        ],
        "approval_bypass": {"present": false, "evidence": ""}
    })
}

async fn cold_orders(intent: &str, plan: &Value) -> CompileOutcome {
    let provider = Provider::new(plan);
    let request = CompileRequest::create(intent)
        .with_authoring_policy(policy())
        .with_knowledge(common::observed(&[ORDERS]));
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// The rule a candidate's compute runs, after the laws in front of it, number law folded.
fn rule_of(out: &CompileOutcome) -> String {
    let compute = common::compute(out.candidate.as_deref().expect("a candidate"));
    compute.rsplit('\n').next().unwrap_or_default().to_owned()
}

#[tokio::test]
async fn a_stated_tie_rule_and_a_number_column_are_lowered_as_the_request_states_them() {
    let stages = json!({"ties": "first_in_file", "numbers": ["amount"]});
    let out = cold_orders(C3, &ranked(C3, C3_COMPUTED, C3_WRITE, &stages)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    // Equal amounts keep the file order (the reversed rows sorted stably, reversed back), the
    // cut keeps it too (no tie refusal), and amount is written as a number under the law.
    assert_eq!(
        rule_of(&out),
        "[.records[] | select(.region == \"north\")] | reverse | sort_by((.amount | num) | dkey) | reverse | .[:2] | map({\"id\": .id, \"amount\": (.amount | num)})"
    );
    // The record states both, and replays to the same bytes: INCOMPLETE until judged, READY
    // under the judge.
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(
        record["rules"][0]["shape"]["ties"], "first_in_file",
        "{record:#}"
    );
    assert_eq!(
        record["rules"][0]["shape"]["numbers"],
        json!(["amount"]),
        "{record:#}"
    );
    let request = CompileRequest::create(C3)
        .with_knowledge(common::observed(&[ORDERS]))
        .with_plan(record);
    let replayed = compile(&request).unwrap();
    assert_eq!(replayed.status, CompileStatus::Incomplete, "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
    let judged = judged_replay(&request).await;
    assert_eq!(judged.status, CompileStatus::Ready, "{judged:#?}");
    assert_eq!(judged.candidate, out.candidate);
}

#[tokio::test]
async fn an_ascending_order_with_the_stated_tie_rule_is_the_stable_sort_alone() {
    let computed = "keep only the rows whose region is north, order them by amount as a number from lowest to highest, and when two rows have the same amount keep the one that comes first in the file first, then keep the first 2 rows";
    let intent = format!("read ./data/orders.csv, {computed} and {C3_WRITE}");
    let stages = json!({"order": "asc", "ties": "first_in_file", "numbers": ["amount"]});
    let out = cold_orders(&intent, &ranked(&intent, computed, C3_WRITE, &stages)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        rule_of(&out),
        "[.records[] | select(.region == \"north\")] | sort_by((.amount | num) | dkey) | .[:2] | map({\"id\": .id, \"amount\": (.amount | num)})"
    );
}

#[tokio::test]
async fn without_a_stated_tie_rule_the_order_and_the_cut_are_unchanged() {
    // The lowering every recorded plan kept: descending by reversing the sort, and a cut
    // through distinct tied records stops the run rather than let input order choose.
    let out = cold_orders(C3, &ranked(C3, C3_COMPUTED, C3_WRITE, &json!({}))).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        rule_of(&out),
        "[.records[] | select(.region == \"north\")] | sort_by((.amount | num) | dkey) | reverse | dtie(2; (.amount | num) | dkey; {\"id\": .id, \"amount\": .amount}; \"`amount`\") | .[:2] | map({\"id\": .id, \"amount\": .amount})"
    );
    let record = out.provenance.plan.clone().unwrap();
    let shape = &record["rules"][0]["shape"];
    assert!(
        shape.get("ties").is_none() && shape.get("numbers").is_none(),
        "{record:#}"
    );
}

#[tokio::test]
async fn a_tie_rule_or_a_number_column_that_cannot_hold_is_no_rule() {
    for stages in [
        // A tie rule over no sort, or a word the law does not know.
        json!({"ties": "first_in_file", "sort_by": "", "order": ""}),
        json!({"ties": "last_in_file"}),
        // A number column the rows are not projected on.
        json!({"numbers": ["item"]}),
    ] {
        let out = cold_orders(C3, &ranked(C3, C3_COMPUTED, C3_WRITE, &stages)).await;
        assert_ne!(out.status, CompileStatus::Ready, "{stages}: {out:#?}");
        let candidate = out.candidate.as_deref().unwrap_or_default();
        assert!(
            !candidate.contains("reverse | sort_by"),
            "{stages}: {candidate}"
        );
    }
}

// V6.1: the same mechanisms in compositions E38 never stated. A tie rule over an ascending order
// on another column, two filter clauses, two number columns and a rename; a total written alone
// that is an average, and a count, not the sum C1 states.
const STOCK: (&str, &[&str]) = (
    "./data/orders.csv",
    &["id", "region", "amount", "item", "qty"],
);

#[tokio::test]
async fn a_new_composition_carries_the_tie_rule_and_the_number_columns() {
    let computed = "keep only the rows whose region is south and whose item is bolts, order them by qty from lowest to highest, keeping the rows with the same qty in the order of the file, then keep the first 3 rows";
    let write = "write them to ./out/low.json with id as text and qty and amount as JSON numbers, renaming qty to quantity";
    let intent = format!("read ./data/orders.csv, {computed} and {write}");
    let stages = json!({
        "clauses": [
            {"field": "region", "op": "eq", "value": "south", "value_field": ""},
            {"field": "item", "op": "eq", "value": "bolts", "value_field": ""}
        ],
        "sort_by": "qty", "order": "asc", "ties": "first_in_file", "limit": "3",
        "columns": ["id", "qty", "amount"], "numbers": ["qty", "amount"],
        "renames": [{"from": "qty", "to": "quantity"}]
    });
    let provider = Provider::new(ranked(&intent, computed, write, &stages));
    let request = CompileRequest::create(&intent)
        .with_authoring_policy(policy())
        .with_knowledge(common::observed(&[STOCK]));
    let out = compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        rule_of(&out),
        "[.records[] | select(.region == \"south\" and .item == \"bolts\")] | sort_by((.qty | num) | dkey) | .[:3] | map({\"id\": .id, \"qty\": (.qty | num), \"amount\": (.amount | num)}) | map(with_entries(if .key == \"qty\" then .key = \"quantity\" else . end))"
    );
    let record = out.provenance.plan.clone().unwrap();
    let shape = &record["rules"][0]["shape"];
    assert_eq!(shape["ties"], "first_in_file", "{record:#}");
    assert_eq!(shape["numbers"], json!(["qty", "amount"]), "{record:#}");
    // The same request without the tie clause is another program: the cut through a tie of
    // distinct records stops the run instead of keeping the file order.
    let mut unstated = stages;
    unstated["ties"] = json!("");
    let plain_intent = intent.replace(
        ", keeping the rows with the same qty in the order of the file",
        "",
    );
    let plain_computed = computed.replace(
        ", keeping the rows with the same qty in the order of the file",
        "",
    );
    let provider = Provider::new(ranked(&plain_intent, &plain_computed, write, &unstated));
    let request = CompileRequest::create(&plain_intent)
        .with_authoring_policy(policy())
        .with_knowledge(common::observed(&[STOCK]));
    let plain = compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap();
    assert_eq!(plain.status, CompileStatus::Ready, "{plain:#?}");
    assert!(
        rule_of(&plain).contains("| dtie(3; "),
        "{}",
        rule_of(&plain)
    );
    assert_ne!(rule_of(&plain), rule_of(&out));
}

// A controlled semantic plan: the seat double states the plan, so this pins the law for the
// plan's `alone` over the same words, not a seat's reading of them (the natural-language
// neighbour is `a_request_naming_the_object_keeps_it_for_an_average_or_a_count`).
#[tokio::test]
async fn an_average_or_a_count_written_alone_is_its_value() {
    for (computed, total, name) in [
        (
            "the average of qty over the rows where status is shipped",
            json!([{"field": "qty", "op": "avg", "as": "average", "round": ""}]),
            "average",
        ),
        (
            "count the rows where status is shipped",
            json!([{"field": "", "op": "count", "as": "count", "round": ""}]),
            "count",
        ),
    ] {
        let write = "write only that number to ./out/result.json";
        let intent = format!("read ./data/input.csv, {computed}, {write}");
        let out = cold(
            &intent,
            &proposal(&intent, computed, &total, write, Some(true)),
        )
        .await;
        assert_eq!(out.status, CompileStatus::Ready, "{name}: {out:#?}");
        assert_eq!(
            written(&out),
            json!(format!("${{{{ tasks.compute.output.{name} }}}}")),
            "{name}"
        );
        // The neighbour without alone keeps the object naming the total.
        let object = cold(&intent, &proposal(&intent, computed, &total, write, None)).await;
        assert_eq!(object.status, CompileStatus::Ready, "{name}: {object:#?}");
        assert_eq!(
            written(&object),
            json!("${{ tasks.compute.output }}"),
            "{name}"
        );
    }
}

// A valid natural-language neighbour (V6.1): a request that names the object and its field keeps
// the object, with a plan that states no `alone`.
#[tokio::test]
async fn a_request_naming_the_object_keeps_it_for_an_average_or_a_count() {
    for (computed, total, name) in [
        (
            "the average of qty over the rows where status is shipped",
            json!([{"field": "qty", "op": "avg", "as": "average", "round": ""}]),
            "average",
        ),
        (
            "count the rows where status is shipped",
            json!([{"field": "", "op": "count", "as": "count", "round": ""}]),
            "count",
        ),
    ] {
        let write =
            format!("write it as a JSON object with one field named {name} to ./out/result.json");
        let intent = format!("read ./data/input.csv, {computed}, {write}");
        let out = cold(&intent, &proposal(&intent, computed, &total, &write, None)).await;
        assert_eq!(out.status, CompileStatus::Ready, "{name}: {out:#?}");
        assert_eq!(
            written(&out),
            json!("${{ tasks.compute.output }}"),
            "{name}"
        );
        let record = out.provenance.plan.clone().unwrap();
        assert!(record["effects"][0].get("alone").is_none(), "{record:#}");
    }
}
