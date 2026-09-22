// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A number the request spells beyond ten (« duzentos », « cien », « twenty ») is a literal
//! of the request, never an invented one (sealed-v3 treatment lane, 2026-09-22: sv3-57).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

/// sv3-57 as the seat read it: the threshold the request spells (« duzentos ») is the
/// literal the candidate writes as digits.
const AGUA: &str = "O ficheiro ./agua/leituras.csv tem as colunas contador, rua e consumo_m3. Guarda em ./out/altos.csv, com as mesmas colunas, os contadores com consumo_m3 acima de duzentos (estritamente).";

fn agua_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./agua/leituras.csv","evidence":"O ficheiro ./agua/leituras.csv"},
        {"op":"compute","detail":"filtra linhas com consumo_m3 > 200 e mantém colunas contador, rua, consumo_m3","evidence":"os contadores com consumo_m3 acima de duzentos (estritamente)",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[{"field":"consumo_m3","op":"gt","value":"200","value_field":""}],"group_by":"",
                        "aggregations":[],"sort_by":"","order":"","columns":["contador","rua","consumo_m3"],"derived":[],"limit":"","renames":[]}}],
        "effects":[{"verb":"write","target":"./out/altos.csv","policy":"automatic","evidence":"Guarda em ./out/altos.csv"}],
        "obligations":[],"constraints":["com as mesmas colunas"],"unknowns":[],
        "regions":[{"text":"O ficheiro ./agua/leituras.csv tem as colunas contador, rua e consumo_m3.","role":"operation"},
                   {"text":"Guarda em ./out/altos.csv, com as mesmas colunas,","role":"effect"},
                   {"text":"os contadores com consumo_m3 acima de duzentos (estritamente).","role":"operation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_number_the_request_spells_is_not_an_invented_literal() {
    let provider = Provider::new(agua_proposal());
    let req = CompileRequest::create(AGUA).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics.iter().any(|d| d
            .message
            .contains("the literal `200` is not in the request")),
        "{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
}
