// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Stated numbers stay exact through generated workflows (R4 A8). Root's public counterexample
//! (`a5-decimal-order-root`): « keep the top 2 rows by points » over the texts
//! 1.000000000000000001 / …002 / …003 checked, ran and wrote five wrong orders of six, because
//! every number crossed jq's f64 and the JSON transport between tasks carries a JSON number as an
//! f64. The one runtime jq now runs exact decimal laws the compile emits (`laws/order.jq`), and a
//! JSON source's numbers keep their exact value past the decode, or the run stops before any
//! effect naming the one that would not.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;

mod common;

const SUPPORT: &str =
    "Route support tickets, look up the customer, draft a reply, and ask me before any refund";

/// The candidate a request compiles to over the observed files, READY.
fn ready(intent: &str, files: &[(&str, &[&str])]) -> Value {
    let request = CompileRequest::create(intent).with_knowledge(common::observed(files));
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn expression(doc: &Value, task: &str) -> String {
    doc["tasks"][task]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// The laws a guarded expression carries in front of it: the readable order laws, whole.
fn carries_the_order_laws(expression: &str) {
    assert!(
        expression.starts_with("# Exact decimal order laws (R4 A8)"),
        "{expression}"
    );
    for law in [
        "def dkey:",
        "def _dkept:",
        "def dguard($fields):",
        "def dtie(",
    ] {
        assert!(expression.contains(law), "{law} missing: {expression}");
    }
}

#[test]
fn a_json_source_is_decoded_with_every_number_it_may_write_kept_exact() {
    let doc = ready(
        "Read ./rows.json and write it to ./copy.csv",
        &[("./rows.json", &["name", "points", "id", "weight"])],
    );
    let parse = expression(&doc, "parse_source");
    carries_the_order_laws(&parse);
    // Every row is written whole: every number of the source is in the guard's scope.
    assert!(parse.ends_with("\nfromjson | dguard(null)"), "{parse}");
}

#[test]
fn a_csv_source_is_converted_without_a_guard_its_cells_are_text() {
    let doc = ready(
        "Read ./rows.csv, keep only the rows whose status is open and write them to ./open.csv",
        &[("./rows.csv", &["name", "status"])],
    );
    assert_eq!(
        doc["tasks"]["parse_source"]["invoke"]["tool"],
        "nika:convert"
    );
    assert!(expression(&doc, "parse_source").is_empty());
}

#[test]
fn each_json_file_a_join_decodes_is_guarded_apart() {
    let doc = ready(
        "Read ./a.json and ./b.json, merge them on the id column and write the result to ./merged.json",
        &[("./a.json", &["id", "x"]), ("./b.json", &["id", "y"])],
    );
    assert_eq!(
        doc["tasks"]["parse_source"]["for_each"]["items"],
        "${{ with.texts }}"
    );
    let parse = expression(&doc, "parse_source");
    carries_the_order_laws(&parse);
    assert!(parse.ends_with("\nfromjson | dguard(null)"), "{parse}");
}

#[test]
fn the_record_the_support_lookup_selects_is_guarded() {
    let request = CompileRequest::create(SUPPORT)
        .answer("model", r#""mock/echo""#)
        .answer("const.customer_directory", r#""customers.json""#)
        .answer(
            "const.refund_endpoint",
            r#""https://refund.example.invalid/refunds""#,
        )
        .answer(
            "const.refund_policy",
            r#"{"cap":100,"currency":"EUR","criteria":"unused purchase within 14 days"}"#,
        );
    let out = compile(&request).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let pick = expression(&doc, "lookup_customer");
    carries_the_order_laws(&pick);
    assert!(
        pick.ends_with(
            "\n. as $lookup | ($lookup.directory | fromjson)[$lookup.id] | dguard(null)"
        ),
        "{pick}"
    );
}
