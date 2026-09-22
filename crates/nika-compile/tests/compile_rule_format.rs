// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A format the rule itself states is carried by the compute task (sealed-v3 lane of
//! 2026-09-22, sv3-21: the rule's own words and its ordering phrase blocked READY as silent
//! obligations), and a single digit in a language step's paraphrase is an enumeration, not
//! a literal the workflow carries (sv3-15, E19B).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const PODIO: &str = "./carrera/resultados.csv → los tres corredores más rápidos (menor tiempo_seg), del más rápido al más lento, mismas columnas → ./out/podio.csv";

fn podio_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./carrera/resultados.csv","evidence":"./carrera/resultados.csv"},
        {"op":"compute","detail":"Seleccionar los tres corredores con menor tiempo_seg, ordenados de menor a mayor tiempo_seg, mismas columnas.","evidence":"los tres corredores más rápidos (menor tiempo_seg)",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[],"group_by":"","aggregations":[],
                        "sort_by":"tiempo_seg","order":"asc","columns":[],"derived":[],"limit":"3","renames":[]}}],
        "effects":[{"verb":"write","target":"./out/podio.csv","policy":"automatic","evidence":"→ ./out/podio.csv"}],
        "obligations":[],"constraints":["del más rápido al más lento","mismas columnas"],"unknowns":[],
        "regions":[{"text":"./carrera/resultados.csv","role":"operation"},{"text":"→ los tres corredores más rápidos (menor tiempo_seg), del más rápido al más lento, mismas columnas","role":"operation"},{"text":"→ ./out/podio.csv","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_format_the_rule_states_is_carried_by_the_compute_task() {
    let provider = Provider::new(podio_proposal());
    let req = CompileRequest::create(PODIO).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("silent obligation")),
        "{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let source = out.candidate.as_deref().unwrap();
    assert!(source.contains("sort_by(.tiempo_seg"), "{source}");
    assert!(source.contains("[:3]"), "{source}");
}

const RAIL: &str = "Read ./rail/incidents.json, an array of incidents with id, line, minutes and cause. Write ./out/digest.md in English with three headings in this order: a line '# Delay digest', then '## Worst incident', then '## Totals'. Under Totals, state the total minutes.";

fn rail_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./rail/incidents.json","evidence":"Read ./rail/incidents.json"},
        {"op":"compute","detail":"total minutes over the incidents","evidence":"state the total minutes",
         "computation":{"present":true,"polarity":"keep","join":"and","clauses":[],"group_by":"","aggregations":[{"field":"minutes","op":"sum","as":"total minutes","round":""}],
                        "sort_by":"","order":"","columns":[],"derived":[],"limit":"","renames":[]}},
        {"op":"draft","detail":"Write ./out/digest.md in English with three headings (heading 1: '# Delay digest', heading 2: '## Worst incident', heading 3: '## Totals')","evidence":"Write ./out/digest.md in English with three headings in this order"}],
        "effects":[{"verb":"write","target":"./out/digest.md","policy":"automatic","evidence":"Write ./out/digest.md"}],
        "obligations":[],"constraints":["in English","three headings in this order"],"unknowns":[],
        "regions":[{"text":"Read ./rail/incidents.json, an array of incidents with id, line, minutes and cause.","role":"operation"},
                   {"text":"Write ./out/digest.md in English with three headings in this order: a line '# Delay digest', then '## Worst incident', then '## Totals'.","role":"effect"},
                   {"text":"Under Totals, state the total minutes.","role":"operation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_single_digit_in_a_drafts_paraphrase_is_not_an_invented_literal() {
    let provider = Provider::new(rail_proposal());
    let req = CompileRequest::create(RAIL).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("is not in the request")),
        "{out:#?}"
    );
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
}
