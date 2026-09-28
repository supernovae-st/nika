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
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::Value;

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
