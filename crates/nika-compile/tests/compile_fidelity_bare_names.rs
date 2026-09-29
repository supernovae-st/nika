// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The native door judges a candidate's paths with the compile's observation: a request naming
//! `orders.csv` and `customers.csv` bare, over a project that keeps them in `./data/`, is READY
//! when the observation places exactly one file of each name and the candidate reads them there
//! (the E39 PILOT14 V6-DEV3-P1 shape, rebuilt in its own words). Without an observation, or with
//! two observed files of that name, the path law still refuses the same candidate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::time::Duration;

mod common;
use common::{Judged, Rotating};

const REGIONS: &str = "Read orders.csv and customers.csv from the data folder, join them on customer_id, sum amount_cents per region and write the totals to ./out/result.json.";

/// The seat's candidate: both files read where the project keeps them.
const CANDIDATE: &str = r#"nika: regional-totals
const:
  orders_path: ./data/orders.csv
  customers_path: ./data/customers.csv
  output_path: ./out/result.json
permits:
  tools: ["nika:read", "nika:convert", "nika:jq", "nika:write"]
  fs:
    read: ["./data/orders.csv", "./data/customers.csv"]
    write: ["./out/result.json"]
tasks:
  read_orders:
    invoke:
      tool: "nika:read"
      args:
        path: "${{ const.orders_path }}"
  read_customers:
    invoke:
      tool: "nika:read"
      args:
        path: "${{ const.customers_path }}"
  parse_orders:
    with:
      document: "${{ tasks.read_orders.output }}"
    invoke:
      tool: "nika:convert"
      args:
        input: "${{ with.document }}"
        from: csv
        to: json
  parse_customers:
    with:
      document: "${{ tasks.read_customers.output }}"
    invoke:
      tool: "nika:convert"
      args:
        input: "${{ with.document }}"
        from: csv
        to: json
  totals:
    with:
      orders: "${{ tasks.parse_orders.output }}"
      customers: "${{ tasks.parse_customers.output }}"
    invoke:
      tool: "nika:jq"
      args:
        input:
          orders: "${{ with.orders }}"
          customers: "${{ with.customers }}"
        expression: '(.customers | map({key: .customer_id, value: .region}) | from_entries) as $region | .orders | group_by($region[.customer_id]) | map({region: $region[.[0].customer_id], total_cents: (map(.amount_cents | tonumber) | add)})'
  write_totals:
    with:
      content: "${{ tasks.totals.output }}"
    invoke:
      tool: "nika:write"
      args:
        path: "${{ const.output_path }}"
        content: "${{ with.content }}"
        overwrite: true
        create_dirs: true
outputs:
  totals: "${{ tasks.totals.output }}"
"#;

fn seat() -> String {
    json!({"candidate": CANDIDATE, "questions": [], "gaps": [], "notes": "the files are read where the project keeps them"}).to_string()
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Only)
        .with_repairs(1)
}

/// The host's observation: one positive row per path.
fn observed(paths: &[&str]) -> Value {
    let rows: Vec<Value> = paths
        .iter()
        .map(|path| json!({"path": path, "state": "observed", "kind": "csv", "complete": false, "columns": ["customer_id"]}))
        .collect();
    json!({ "observed": rows })
}

async fn compiled(world: Option<Value>) -> CompileOutcome {
    let provider = Rotating::new(vec![seat()]);
    let mut request = CompileRequest::create(REGIONS).with_authoring_policy(policy());
    if let Some(world) = world {
        request = request.with_knowledge(world);
    }
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// Whether a native round refused the bare `name` as an unrealized path.
fn refused(out: &CompileOutcome, name: &str) -> bool {
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    rounds
        .to_string()
        .contains(&format!("UNREALIZED PATH: the request names `{name}`"))
}

#[tokio::test]
async fn the_native_door_realizes_bare_names_the_observation_places_once() {
    let out = compiled(Some(observed(&[
        "./data/orders.csv",
        "./data/customers.csv",
    ])))
    .await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        !refused(&out, "orders.csv") && !refused(&out, "customers.csv"),
        "{out:#?}"
    );
}

#[tokio::test]
async fn the_native_door_keeps_the_refusal_without_one_placement() {
    for world in [
        None,
        Some(observed(&[
            "./a/orders.csv",
            "./b/orders.csv",
            "./data/customers.csv",
        ])),
    ] {
        let out = compiled(world.clone()).await;
        assert_ne!(out.status, CompileStatus::Ready, "{world:?}: {out:#?}");
        assert!(refused(&out, "orders.csv"), "{world:?}: {out:#?}");
    }
}
