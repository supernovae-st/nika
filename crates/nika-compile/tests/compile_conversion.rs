// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A conversion between two structured files is the identity over the parsed records,
//! written in the other format (sealed sv3-14 on the lane of 2026-09-22, gpt-5-mini: the
//! reader recognized nothing and the seat parsed the CSV with a model, where the corpus
//! expects zero calls). Deterministically the clause is a read, a compute and a write;
//! through a seat, the proposed `extract` that names the conversion becomes that computation.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const MILEAGE: &str = "convert ./fleet/mileage.csv (columns vehicle, driver, km) into ./out/mileage.json, a JSON array with one object per row using the column names as keys, same row order";

#[test]
fn a_conversion_compiles_to_the_parsed_records_written_as_json_with_no_model_call() {
    let out = compile(&CompileRequest::create(MILEAGE)).unwrap();
    assert!(
        out.candidate.is_some(),
        "a candidate is assembled: {out:#?}"
    );
    let source = out.candidate.as_deref().unwrap();
    assert!(
        !source.contains("infer:"),
        "no model reads the rows: {source}"
    );
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    assert_eq!(
        doc["tasks"]["compute"]["invoke"]["args"]["expression"], ".records",
        "the identity over the parsed records: {source}"
    );
    assert!(
        source.contains("./out/mileage.json"),
        "the destination is written: {source}"
    );
}

/// The seat's proposal on the sealed lane, replayed: a read, an extract that parses the CSV
/// into a JSON array, a write.
fn mileage_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"Read the CSV file at ./fleet/mileage.csv.","evidence":"convert ./fleet/mileage.csv (columns vehicle, driver, km)"},
        {"op":"extract","detail":"Parse the CSV preserving row order and column names (vehicle, driver, km); produce a JSON array where each CSV row becomes an object keyed by the column names.","evidence":"a JSON array with one object per row using the column names as keys, same row order"},
        {"op":"draft","detail":"the tone of the driver names","evidence":"harmonise the tone of the driver names"}],
        "effects":[{"verb":"write","target":"./out/mileage.json","policy":"automatic","evidence":"into ./out/mileage.json"},
                   {"verb":"write","target":"./out/drivers.md","policy":"automatic","evidence":"harmonise the tone of the driver names in ./out/drivers.md"}],
        "obligations":[],"constraints":["(columns vehicle, driver, km)"],"unknowns":[],
        "regions":[{"text":"convert ./fleet/mileage.csv (columns vehicle, driver, km)","role":"operation"},
                   {"text":"into ./out/mileage.json,","role":"effect"},
                   {"text":"a JSON array with one object per row using the column names as keys, same row order","role":"operation"},
                   {"text":"Then harmonise the tone of the driver names in ./out/drivers.md.","role":"operation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_proposed_extract_that_names_the_conversion_is_the_computation() {
    let provider = Provider::new(mileage_proposal());
    // The deterministic door admits the request; the proposal is judged when the door is
    // closed, so a clause the reader cannot read is appended.
    let intent =
        format!("{MILEAGE}. Then harmonise the tone of the driver names in ./out/drivers.md.");
    let req = CompileRequest::create(&intent).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    let plan = out.provenance.plan.as_ref();
    assert!(plan.is_some(), "a plan is merged: {out:#?}");
    let ops: Vec<&str> = plan.unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert!(
        ops.contains(&"compute") && !ops.contains(&"extract"),
        "the extract is the conversion: {ops:?}\n{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
}
