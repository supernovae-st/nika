// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A value the request alludes to without stating it is a slot the compiler asks for (sealed
//! lane4 of 2026-09-22, gpt-5-mini: « les capteurs dont la température dépasse le seuil
//! d'alerte » — the seat compared `temperature` to the words « seuil d'alerte » and listed
//! them as unknown; the candidate was refused as unresolved work where the corpus expects a
//! typed `const.*` question). The slot is asked as a const, and the answered value rides
//! beside the records into the rule's jq.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, QuestionType, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

const SERRE: &str = "Veuillez lire ./serre/capteurs.csv (colonnes capteur, zone, temperature) et écrire dans ./out/alerte.csv, avec les mêmes colonnes, les capteurs dont la température dépasse le seuil d'alerte.";

fn serre_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"./serre/capteurs.csv (colonnes capteur, zone, temperature)","evidence":"lire ./serre/capteurs.csv (colonnes capteur, zone, temperature)"},
        {"op":"compute","detail":"les capteurs dont la température dépasse le seuil d'alerte","evidence":"les capteurs dont la température dépasse le seuil d'alerte",
         "computation":{"present":true,"polarity":"keep","join":"and",
                        "clauses":[{"field":"temperature","op":">","value":"seuil d'alerte","value_field":""}],
                        "aggregations":[],"group_by":"","sort_by":"","limit":"","columns":["capteur","zone","temperature"],"derived":[],"renames":[]}}],
        "effects":[{"verb":"write","target":"./out/alerte.csv","policy":"automatic","evidence":"écrire dans ./out/alerte.csv, avec les mêmes colonnes, les capteurs dont la température dépasse le seuil d'alerte"}],
        "obligations":[],"constraints":["avec les mêmes colonnes"],"unknowns":["seuil d'alerte"],
        "regions":[{"text":"Veuillez lire ./serre/capteurs.csv (colonnes capteur, zone, temperature)","role":"operation"},
                   {"text":"et écrire dans ./out/alerte.csv, avec les mêmes colonnes, les capteurs dont la température dépasse le seuil d'alerte.","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn an_alluded_value_is_asked_as_a_const_and_read_by_the_rule() {
    let provider = Provider::new(serre_proposal());
    let req = CompileRequest::create(SERRE).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let asked = |q: &nika_compile::CompileQuestion| {
        q.key.starts_with("const.") && q.label.contains("seuil d'alerte")
    };
    assert!(
        out.questions.iter().any(&asked),
        "the threshold is asked as a const: {out:#?}"
    );
    let slot = out.questions.iter().find(|q| asked(q)).unwrap();
    assert_eq!(slot.answer_type, QuestionType::Literal, "{out:#?}");
    assert!(slot.mandatory, "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(
        plan["unknowns"],
        json!([]),
        "the slot is no unresolved work: {out:#?}"
    );
    let key = slot.key.clone();

    let provider = Provider::new(serre_proposal());
    let req = CompileRequest::create(SERRE)
        .with_authoring_policy(policy())
        .answer(&key, "70")
        .answer("model", r#""mock/echo""#);
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(
        out.candidate.is_some(),
        "READY once the value is known: {out:#?}"
    );
    let source = out.candidate.as_deref().unwrap();
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    let slug = key.trim_start_matches("const.");
    assert_eq!(
        doc["const"][slug],
        json!(70),
        "the answer is baked as a const: {source}"
    );
    let compute = doc["tasks"]["compute"]["invoke"]["args"]["expression"]
        .as_str()
        .unwrap_or_default();
    assert!(compute.starts_with(". as $in | "), "{source}");
    assert!(
        compute.contains(&format!("($in.slots.{slug} | tonumber)")),
        "the rule compares to the slot, never to the words: {source}"
    );
    assert!(
        source.contains(&format!("{slug}: ${{{{ const.{slug} }}}}")),
        "the slot rides beside the records: {source}"
    );
}
