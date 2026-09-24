// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A validate over nothing but format words the computation keeps by construction is folded
//! (sealed sv3-24 on the lane of 2026-09-22, gpt-5-mini: « mismas columnas y mismo orden »
//! was listed as a `validate` beside the drop-filter, and the candidate asked a model to
//! check what the jq guarantees, where the corpus expects zero calls).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::CompileRequest;
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const TEATRO: &str = "de ./teatro/reservas.csv (reserva, cliente, estado, importe) quita todas las q tienen estado cancelada y guarda el resto tal cual en ./out/activas.csv, mismas columnas y mismo orden";

fn teatro_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./teatro/reservas.csv (reserva, cliente, estado, importe)","evidence":"de ./teatro/reservas.csv (reserva, cliente, estado, importe)"},
        {"op":"compute","detail":"quita todas las q tienen estado cancelada","evidence":"quita todas las q tienen estado cancelada",
         "computation":{"present":true,"polarity":"drop","join":"and",
                        "clauses":[{"field":"estado","op":"==","value":"cancelada","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","order":"","columns":["reserva","cliente","estado","importe"],"derived":[],"limit":"","renames":[]}},
        {"op":"validate","detail":"mismas columnas y mismo orden","evidence":"mismas columnas y mismo orden"}],
        "effects":[{"verb":"write","target":"./out/activas.csv","policy":"automatic","evidence":"guarda el resto tal cual en ./out/activas.csv"}],
        "obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"de ./teatro/reservas.csv (reserva, cliente, estado, importe)","role":"operation"},
                   {"text":"quita todas las q tienen estado cancelada","role":"operation"},
                   {"text":"y guarda el resto tal cual en ./out/activas.csv,","role":"effect"},
                   {"text":"mismas columnas y mismo orden","role":"constraint"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_validate_over_format_words_is_folded_and_no_model_is_asked() {
    let provider = Provider::new(teatro_proposal());
    let req = CompileRequest::create(TEATRO).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(out.provenance.plan.is_some(), "a plan is merged: {out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["read", "compute"], "{out:#?}");
    assert!(
        !keys(&out).contains(&"model"),
        "no language step, no model asked: {out:#?}"
    );
    if let Some(source) = out.candidate.as_deref() {
        assert!(!source.contains("infer:"), "{source}");
    }
}
