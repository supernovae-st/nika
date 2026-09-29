// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A bare file name the request states (`orders.csv`) is realized where the host's observation
//! places it (`./data/orders.csv`): exactly one observed file of that name, covered by
//! `permits.fs.read` and opened by a task. Without a positive observation, with two observed
//! files of that name, with another name, or with no task opening it, the path law still
//! refuses it, and a destination keeps its own law. The shape of the E39 PILOT14 V6-DEV3-P1
//! refusals (FR and EN), rebuilt here in its own words.
use nika_compile_fidelity::fidelity::{Diagnostic, laws_observed};
use nika_compile_reader::plan::Plan;
use serde_json::{Value, json};

const EN: &str = "Read orders.csv and customers.csv from the data folder, join them on customer_id, sum amount_cents per region and write the totals to ./out/result.json.";
const FR: &str = "Lis orders.csv et customers.csv du dossier data, joins-les sur customer_id, additionne amount_cents par région et écris les totaux dans ./out/result.json.";

const BOTH: [&str; 2] = ["./data/orders.csv", "./data/customers.csv"];

/// The host's observation: one row per path, in the state the observer reports it.
fn world(rows: &[(&str, &str)]) -> Value {
    let rows: Vec<Value> = rows
        .iter()
        .map(|(path, state)| json!({"path": path, "state": state, "kind": "csv", "complete": false, "columns": ["id"]}))
        .collect();
    json!({ "observed": rows })
}

/// A task reading `path` (a literal or a `${{ }}` reference).
fn read(path: &str) -> Value {
    json!({"invoke": {"tool": "nika:read", "args": {"path": path}}})
}

/// A candidate reading under `read` with `tasks`, and writing ./out/result.json.
fn candidate(read: &[&str], tasks: &Value) -> Value {
    json!({
        "nika": "compiled-workflow",
        "const": {"customers_path": "./data/customers.csv"},
        "permits": {
            "fs": {"read": read, "write": ["./out/result.json"]},
            "tools": ["nika:read", "nika:jq", "nika:write"]
        },
        "tasks": tasks,
    })
}

/// Tasks opening ./data/orders.csv literally and ./data/customers.csv through a const.
fn opening_both() -> Value {
    json!({
        "read_orders": read("./data/orders.csv"),
        "read_customers": read("${{ const.customers_path }}"),
        "totals": {
            "with": {"orders": "${{ tasks.read_orders.output }}"},
            "invoke": {"tool": "nika:jq", "args": {"expression": ".", "input": "${{ with.orders }}"}}
        },
        "write_totals": {
            "with": {"totals": "${{ tasks.totals.output }}"},
            "invoke": {"tool": "nika:write", "args": {"path": "./out/result.json", "content": "${{ with.totals }}"}}
        }
    })
}

/// The stated paths the path law still refuses as unrealized, in the law's order.
fn unrealized(intent: &str, doc: &Value, world: Option<&Value>) -> Vec<String> {
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_observed(
        intent,
        &Plan::default(),
        doc,
        &[],
        &[],
        &[],
        world,
        &mut out,
    );
    out.iter()
        .filter(|d| d.kind == "path" && d.message.starts_with("UNREALIZED PATH"))
        .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
        .collect()
}

#[test]
fn a_bare_name_is_realized_where_the_observation_places_it() {
    let observed = world(&[
        ("./data/orders.csv", "observed"),
        ("./data/customers.csv", "observed"),
    ]);
    for intent in [EN, FR] {
        assert_eq!(
            unrealized(intent, &candidate(&BOTH, &opening_both()), Some(&observed)),
            Vec::<String>::new(),
            "{intent}"
        );
    }
}

#[test]
fn without_a_positive_observation_a_bare_name_stays_unrealized() {
    let doc = candidate(&BOTH, &opening_both());
    let absent = world(&[("orders.csv", "absent"), ("customers.csv", "absent")]);
    let outside = world(&[
        ("./data/orders.csv", "outside_project"),
        ("./data/customers.csv", "unreadable"),
    ]);
    for observed in [None, Some(&absent), Some(&outside)] {
        assert_eq!(
            unrealized(EN, &doc, observed),
            ["orders.csv", "customers.csv"],
            "{observed:?}"
        );
    }
}

#[test]
fn two_observed_files_of_that_name_keep_the_refusal() {
    let observed = world(&[
        ("./a/orders.csv", "observed"),
        ("./b/orders.csv", "observed"),
        ("./data/customers.csv", "observed"),
    ]);
    let tasks = json!({
        "read_orders": read("./a/orders.csv"),
        "read_customers": read("./data/customers.csv"),
    });
    let doc = candidate(&["./a/orders.csv", "./data/customers.csv"], &tasks);
    assert_eq!(unrealized(EN, &doc, Some(&observed)), ["orders.csv"]);
}

#[test]
fn another_observed_name_realizes_nothing() {
    for other in ["./data/orders_old.csv", "./data/Orders.csv"] {
        let observed = world(&[(other, "observed"), ("./data/customers.csv", "observed")]);
        let tasks = json!({
            "read_orders": read(other),
            "read_customers": read("./data/customers.csv"),
        });
        let doc = candidate(&[other, "./data/customers.csv"], &tasks);
        assert_eq!(
            unrealized(EN, &doc, Some(&observed)),
            ["orders.csv"],
            "{other}"
        );
    }
}

#[test]
fn a_program_that_opens_nothing_keeps_the_refusal() {
    let observed = world(&[
        ("./data/orders.csv", "observed"),
        ("./data/customers.csv", "observed"),
    ]);
    // The boundary covers the observed file, but no task opens it.
    let only_customers = json!({ "read_customers": read("./data/customers.csv") });
    assert_eq!(
        unrealized(EN, &candidate(&BOTH, &only_customers), Some(&observed)),
        ["orders.csv"]
    );
    // A task opens it, but the boundary does not cover it.
    assert_eq!(
        unrealized(
            EN,
            &candidate(&["./data/customers.csv"], &opening_both()),
            Some(&observed)
        ),
        ["orders.csv"]
    );
}

#[test]
fn a_destination_keeps_its_own_law() {
    let intent = "Read ./data/orders.csv and write the totals to summary.csv.";
    let observed = world(&[
        ("./data/orders.csv", "observed"),
        ("./reports/summary.csv", "observed"),
    ]);
    let doc = json!({
        "permits": {"fs": {"read": ["./data/orders.csv"], "write": ["./reports/summary.csv"]}},
        "tasks": {
            "read_orders": read("./data/orders.csv"),
            "write_totals": {"invoke": {"tool": "nika:write", "args": {"path": "./reports/summary.csv", "content": "x"}}}
        }
    });
    assert_eq!(unrealized(intent, &doc, Some(&observed)), ["summary.csv"]);
}
