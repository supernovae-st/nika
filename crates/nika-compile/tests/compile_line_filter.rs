// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A line filter over a text source is a computation the compiler writes (sealed sv3-04 on
//! the lanes of 2026-09-22: an `extract` and a `draft`, two model calls, where the corpus
//! expects zero). Deterministically, « ./rando/guide.md : extrais toutes les lignes de titre
//! markdown (celles qui commencent par un ou plusieurs #) … → ./out/titres.txt » is a read,
//! a jq over the lines and a write; through a seat, the proposed `extract` over the same
//! words becomes that computation and the draft over the destination is folded.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const TITLES: &str = "./rando/guide.md : extrais toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne → ./out/titres.txt. Rien d'autre dans le fichier.";

#[test]
fn a_line_filter_compiles_to_a_jq_over_the_lines_with_no_model_call() {
    let out = compile(&CompileRequest::create(TITLES)).unwrap();
    assert!(
        out.candidate.is_some(),
        "a candidate is assembled: {out:#?}"
    );
    let source = out.candidate.as_deref().unwrap();
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    assert!(
        !source.contains("infer:"),
        "no model reads the file: {source}"
    );
    assert_eq!(
        doc["tasks"]["parse_source"]["invoke"]["tool"], "nika:jq",
        "the source is decoded into lines: {source}"
    );
    let compute = doc["tasks"]["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap_or_default();
    assert!(
        compute.contains("startswith(\"#\")"),
        "the kept lines start with #: {source}"
    );
    assert!(
        compute.contains("join(\"\\n\")"),
        "the kept lines are written back as lines: {source}"
    );
    assert!(
        source.contains("./out/titres.txt"),
        "the destination is written: {source}"
    );
}

/// The deterministic door admits the sealed request; a seat's proposal is judged only when
/// the door is closed, so the request is bent off it with a clause the reader cannot read.
const TITLES_AND_TONE: &str = "./rando/guide.md : extrais toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne → ./out/titres.txt. Rien d'autre dans le fichier. Harmonise ensuite le ton des titres dans ./out/titres.md.";

/// The seat's proposal on the sealed lane, replayed: an extract over the filter clause, a
/// draft over the destination and the structure law, a write; the tone clause as a draft.
fn titles_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./rando/guide.md","evidence":"./rando/guide.md"},
        {"op":"extract","detail":"toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne","evidence":"extrais toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne"},
        {"op":"draft","detail":"→ ./out/titres.txt. Rien d'autre dans le fichier.","evidence":"→ ./out/titres.txt. Rien d'autre dans le fichier."},
        {"op":"draft","detail":"le ton des titres","evidence":"Harmonise ensuite le ton des titres"}],
        "effects":[{"verb":"write","target":"./out/titres.txt","policy":"automatic","evidence":"→ ./out/titres.txt"},
                   {"verb":"write","target":"./out/titres.md","policy":"automatic","evidence":"Harmonise ensuite le ton des titres dans ./out/titres.md."}],
        "obligations":[],"constraints":["Rien d'autre dans le fichier."],"unknowns":[],
        "regions":[{"text":"./rando/guide.md :","role":"operation"},
                   {"text":"extrais toutes les lignes de titre markdown (celles qui commencent par un ou plusieurs #), telles quelles, dans l'ordre, une par ligne","role":"operation"},
                   {"text":"→ ./out/titres.txt.","role":"effect"},
                   {"text":"Rien d'autre dans le fichier.","role":"constraint"},
                   {"text":"Harmonise ensuite le ton des titres dans ./out/titres.md.","role":"operation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_proposed_extract_of_lines_by_a_pattern_is_the_computation() {
    let provider = Provider::new(titles_proposal());
    let req = CompileRequest::create(TITLES_AND_TONE).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let steps = plan["operations"].as_array().unwrap();
    let ops: Vec<&str> = steps.iter().map(|o| o["op"].as_str().unwrap()).collect();
    assert!(
        ops.contains(&"compute") && !ops.contains(&"extract"),
        "the extract over the filter clause is the computation: {ops:?}\n{out:#?}"
    );
    assert!(
        !steps
            .iter()
            .any(|o| o["op"] == "draft"
                && o["evidence"].as_str().unwrap_or_default().starts_with("→")),
        "nothing to draft over the destination and the structure law: {ops:?}\n{out:#?}"
    );
    let rules = plan["rules"].as_array().unwrap();
    assert!(
        rules.iter().any(|r| r["lines"] == true
            && r["jq"]
                .as_str()
                .unwrap_or_default()
                .contains("startswith(\"#\")")),
        "{out:#?}"
    );
}
