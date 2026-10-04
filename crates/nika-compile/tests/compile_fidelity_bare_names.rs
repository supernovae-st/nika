// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The sketch door judges a graph's paths with the compile's observation: a request naming
//! `orders.csv` and `customers.csv` bare, over a project that keeps them in `./data/`, is READY
//! when the observation places exactly one file of each name and the candidate reads them there
//! in these self-contained inline examples. Without an observation, or with
//! two observed files of that name, the path law still refuses the same candidate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_compile_cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_provider,
};
use serde_json::{Value, json};
use std::time::Duration;

mod common;
use common::{Judged, Rotating};

const REGIONS: &str = "Read orders.csv and customers.csv from the data folder, join them on customer_id, sum amount_cents per region and write the totals to ./out/result.json.";

/// The seat's sketch: both files read where the project keeps them, both parsed, joined and
/// summed per region, the totals written.
fn seat() -> Vec<String> {
    seat_with(|_| {})
}

/// The same sketch with `change` applied to its graph.
fn seat_with(change: impl Fn(&mut Value)) -> Vec<String> {
    let task = |id: &str, tool: &str, extra: Value| {
        let mut t = json!({"id": id, "verb": "invoke", "tool": tool, "purpose": id});
        for (k, v) in extra.as_object().unwrap() {
            t[k] = v.clone();
        }
        t
    };
    let edge = |name: &str, from: &str| json!({"name": name, "from": from});
    let graph = json!({"name": "regional-totals", "tasks": [
        task("read_orders", "nika:read", json!({"reads": ["./data/orders.csv"]})),
        task("read_customers", "nika:read", json!({"reads": ["./data/customers.csv"]})),
        task("parse_orders", "nika:convert", json!({"with": [edge("document", "read_orders")]})),
        task("parse_customers", "nika:convert", json!({"with": [edge("document", "read_customers")]})),
        task("totals", "nika:jq", json!({"with": [edge("orders", "parse_orders"), edge("customers", "parse_customers")]})),
        task("write_totals", "nika:write", json!({"writes": ["./out/result.json"], "with": [edge("content", "totals")]})),
    ], "outputs": [{"name": "totals", "from": "totals"}], "questions": [], "gaps": [],
       "notes": "the files are read where the project keeps them"});
    let mut graph = graph;
    change(&mut graph);
    let fills = json!({"fills": [
        {"task": "parse_orders", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "parse_customers", "field": "args", "value": {"from": "csv", "to": "json"}},
        {"task": "totals", "field": "expression", "value": "(.customers | map({key: .customer_id, value: .region}) | from_entries) as $region | .orders | group_by($region[.customer_id]) | map({region: $region[.[0].customer_id], total_cents: (map(.amount_cents | tonumber) | add)})"}
    ], "notes": "three holes"});
    vec![graph.to_string(), fills.to_string()]
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
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
    compiled_by(seat(), world).await
}

async fn compiled_by(plans: Vec<String>, world: Option<Value>) -> CompileOutcome {
    let provider = Rotating::new(plans);
    let mut request = CompileRequest::create(REGIONS).with_authoring_policy(policy());
    if let Some(world) = world {
        request = request.with_knowledge(world);
    }
    compile_with_provider(&request, &Judged::approving(&provider))
        .await
        .unwrap()
}

/// Whether a round refused the bare `name` as an unrealized path: the fidelity law's words, or
/// the sketch law's, which admits only a stated literal or an answered value as a path.
fn refused(out: &CompileOutcome, name: &str) -> bool {
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    let rounds = rounds.to_string();
    rounds.contains(&format!("UNREALIZED PATH: the request names `{name}`"))
        || (rounds.contains(&format!("/{name}`, which the request never states")))
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
    // The join reads both tables: the totals' input carries the orders AND the customers.
    let doc: Value = serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap();
    let input = doc["tasks"]["totals"]["invoke"]["args"]["input"].to_string();
    assert!(
        input.contains("orders") && input.contains("customers"),
        "the join's input carries both tables: {input}"
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

/// Whether a round refused `task` reaching the `what` literal the request never states.
fn reaches(out: &CompileOutcome, task: &str, what: &str, literal: &str) -> bool {
    let rounds = &out.provenance.decision.as_ref().unwrap()["native"]["rounds"];
    (rounds.to_string()).contains(&format!(
        "`{task}` reaches the {what} `{literal}`, which the request never states"
    ))
}

#[tokio::test]
async fn a_placement_needs_an_observed_file_of_exactly_the_stated_name() {
    let customers = json!({"path": "./data/customers.csv", "state": "observed", "kind": "csv"});
    let cases = [
        (
            "absent from the observation",
            json!({"observed": [customers]}),
        ),
        (
            "observed as absent",
            json!({"observed": [customers, {"path": "./data/orders.csv", "state": "absent"}]}),
        ),
        (
            "another case",
            observed(&["./data/Orders.csv", "./data/customers.csv"]),
        ),
        (
            "a longer name",
            observed(&["./data/xorders.csv", "./data/customers.csv"]),
        ),
    ];
    for (case, world) in cases {
        let out = compiled(Some(world)).await;
        assert_ne!(out.status, CompileStatus::Ready, "{case}: {out:#?}");
        assert!(refused(&out, "orders.csv"), "{case}: {out:#?}");
        assert!(!refused(&out, "customers.csv"), "{case}: {out:#?}");
    }
}

#[tokio::test]
async fn a_placement_never_reaches_another_name_than_the_request_states() {
    // The sketch reads the file the observation places under a name the request never says.
    for path in [
        "./data/Orders.csv",
        "./data/xorders.csv",
        "./data/orders_2024.csv",
    ] {
        let plans = seat_with(|graph| graph["tasks"][0]["reads"] = json!([path]));
        let world = observed(&[path, "./data/orders.csv", "./data/customers.csv"]);
        let out = compiled_by(plans, Some(world)).await;
        assert_ne!(out.status, CompileStatus::Ready, "{path}: {out:#?}");
        assert!(
            reaches(&out, "read_orders", "path", path),
            "{path}: {out:#?}"
        );
    }
}

#[tokio::test]
async fn a_placement_grants_a_read_never_a_write_or_a_host() {
    let world = || Some(observed(&["./data/orders.csv", "./data/customers.csv"]));
    let plans = seat_with(|graph| graph["tasks"][5]["writes"] = json!(["./data/orders.csv"]));
    let out = compiled_by(plans, world()).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        reaches(&out, "write_totals", "path", "./data/orders.csv"),
        "{out:#?}"
    );
    let plans = seat_with(|graph| graph["tasks"][0]["hosts"] = json!(["data/orders.csv"]));
    let out = compiled_by(plans, world()).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        reaches(&out, "read_orders", "host", "data/orders.csv"),
        "{out:#?}"
    );
}

async fn replayed(request: &CompileRequest) -> CompileOutcome {
    compile_with_cognition(request, Cognition::<NoProvider>::default())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_recorded_placement_replays_only_under_the_same_observation() {
    let world = observed(&["./data/orders.csv", "./data/customers.csv"]);
    let out = compiled(Some(world.clone())).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.plan.clone().unwrap();
    let replay = |knowledge: Option<Value>| {
        let request = CompileRequest::create(REGIONS)
            .with_authoring_policy(policy())
            .with_plan(record.clone());
        match knowledge {
            Some(knowledge) => request.with_knowledge(knowledge),
            None => request,
        }
    };
    // Under the same observation the record rebuilds the same candidate, with zero calls.
    let again = replayed(&replay(Some(world))).await;
    assert!(again.candidate.is_some(), "{again:#?}");
    assert_eq!(again.candidate, out.candidate, "{again:#?}");
    assert!(
        again.provenance.authoring.is_none(),
        "zero calls: {again:#?}"
    );
    let moved = observed(&[
        "./data/orders.csv",
        "./archive/orders.csv",
        "./data/customers.csv",
    ]);
    let without = observed(&["./data/customers.csv"]);
    for (case, knowledge) in [
        ("ambiguous", Some(moved)),
        ("absent", Some(without)),
        ("none", None),
    ] {
        let out = replayed(&replay(knowledge)).await;
        assert!(out.candidate.is_none(), "{case}: {out:#?}");
        assert!(
            (out.diagnostics.iter())
                .any(|d| d.target == "recorded_plan" && d.message.contains("cannot be replayed")),
            "{case}: {out:#?}"
        );
    }
}
