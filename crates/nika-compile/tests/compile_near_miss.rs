// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A citation two characters away from a unique span of the request names that span (sealed
//! sv3-26 on the lane of 2026-09-22, gpt-5-mini: the seat cited « pregúntame y espere mi
//! aprobación » where the request wrote « espera »; the repair call did not fix it and the
//! candidate was refused for an unanchored effect). The request's own words are used, never
//! the seat's; a citation that could name two spans, or a short one, is still refused.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

/// The clinic request of the sealed lane, with a tone clause the reader cannot read appended:
/// the deterministic door admits the bare request, and the seat's citation is judged only
/// when the door is closed.
const CLINICA: &str = "Hola, ¿podrías por favor leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada) y preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas? Antes de escribir el archivo, pregúntame y espera mi aprobación. Luego armoniza el tono de los nombres en ./out/nombres.md. ¡Muchas gracias!";

fn clinica_proposal(gate_evidence: &str) -> Value {
    json!({"steps":[
        {"op":"read","detail":"./clinica/citas.csv (columnas cita, paciente, hora, confirmada)","evidence":"leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada)"},
        {"op":"compute","detail":"las citas cuya columna confirmada sea « no »","evidence":"las citas cuya columna confirmada sea « no »",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"confirmada","op":"==","value":"no","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","order":"","columns":["cita","paciente","hora","confirmada"],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":"el tono de los nombres","evidence":"armoniza el tono de los nombres"}],
        "effects":[{"verb":"write","target":"./out/sin-confirmar.csv","policy":"human_first","evidence":gate_evidence},
                   {"verb":"write","target":"./out/nombres.md","policy":"automatic","evidence":"armoniza el tono de los nombres en ./out/nombres.md"}],
        "obligations":[],"constraints":["mismas columnas"],"unknowns":[],
        "regions":[{"text":"Hola, ¿podrías por favor leer ./clinica/citas.csv (columnas cita, paciente, hora, confirmada)","role":"operation"},
                   {"text":"y preparar ./out/sin-confirmar.csv con las citas cuya columna confirmada sea « no », mismas columnas?","role":"effect"},
                   {"text":"Antes de escribir el archivo, pregúntame y espera mi aprobación.","role":"policy"},
                   {"text":"Luego armoniza el tono de los nombres en ./out/nombres.md.","role":"operation"},
                   {"text":"¡Muchas gracias!","role":"context"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_near_miss_citation_names_the_request_span() {
    let provider = Provider::new(clinica_proposal(
        "Antes de escribir el archivo, pregúntame y espere mi aprobación.",
    ));
    let req = CompileRequest::create(CLINICA).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("lacks an exact source excerpt")),
        "{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let effects = plan["effects"].as_array().unwrap();
    let write = effects
        .iter()
        .find(|e| e["target"] == "./out/sin-confirmar.csv")
        .unwrap();
    assert_eq!(write["verb"], "write", "{out:#?}");
    assert_eq!(write["policy"], "human_first", "{out:#?}");
    let evidence = write["evidence"].as_str().unwrap_or_default();
    assert!(
        evidence.contains("pregúntame y espera mi aprobación") && !evidence.contains("espere"),
        "the request's own words, never the seat's: {evidence}"
    );
}

#[tokio::test]
async fn a_citation_far_from_the_request_is_still_refused() {
    let provider = Provider::new(clinica_proposal(
        "Antes de guardar el fichero, avísame y aguarda mi visto bueno.",
    ));
    let req = CompileRequest::create(CLINICA).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("lacks an exact source excerpt")
                || d.message.contains("which the request never wrote")),
        "{out:#?}"
    );
}
