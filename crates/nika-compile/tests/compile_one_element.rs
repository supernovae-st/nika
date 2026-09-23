// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One clause is one element (eco-60 on the binary of 2026-09-22 04:10Z, gpt-5-mini: three
//! requests were assembled with operations and effects the seat had double-listed): a step
//! over the trigger clause is the trigger, a step over an obligation's words is the
//! safeguard, an effect over a language step's own words is that step, and two effects
//! with kindred verbs over one clause are one effect.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

fn ops_and_effects(out: &nika_compile::CompileOutcome) -> (Vec<String>, Vec<String>) {
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap().to_owned())
        .collect();
    let effects = plan["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["verb"].as_str().unwrap().to_owned())
        .collect();
    (ops, effects)
}

/// E05C: the seat listed « rédige un compte rendu » as the draft AND as a write effect.
const MINUTES: &str = "À partir de la transcription fournie, extrais décisions, responsables et échéances exactes, puis rédige un compte rendu. Laisse les échéances absentes à null. Ne consulte aucune autre source. Il est absolument interdit de envoyer le compte rendu aux participants.";

fn minutes_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"la transcription fournie","evidence":"À partir de la transcription fournie"},
        {"op":"extract","detail":"décisions, responsables et échéances exactes","evidence":"extrais décisions, responsables et échéances exactes"},
        {"op":"draft","detail":"un compte rendu","evidence":"rédige un compte rendu"}],
        "effects":[{"verb":"send","target":"envoyer le compte rendu aux participants","policy":"forbidden","evidence":"Il est absolument interdit de envoyer le compte rendu aux participants."},
                   {"verb":"write","target":"rédige un compte rendu","policy":"automatic","evidence":"rédige un compte rendu"}],
        "obligations":[],"constraints":["Laisse les échéances absentes à null.","Ne consulte aucune autre source."],"unknowns":[],
        "regions":[{"text":"À partir de la transcription fournie, extrais décisions, responsables et échéances exactes,","role":"operation"},
                   {"text":"puis rédige un compte rendu.","role":"operation"},
                   {"text":"Laisse les échéances absentes à null.","role":"constraint"},
                   {"text":"Ne consulte aucune autre source.","role":"constraint"},
                   {"text":"Il est absolument interdit de envoyer le compte rendu aux participants.","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn an_effect_over_a_drafts_own_words_is_the_draft() {
    let provider = Provider::new(minutes_proposal());
    let req = CompileRequest::create(MINUTES).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    let (ops, effects) = ops_and_effects(&out);
    assert_eq!(ops, ["read", "extract", "draft"], "{out:#?}");
    assert_eq!(effects, ["send"], "no write was requested: {out:#?}");
}

/// E09B: a lookup over the trigger clause, a publish AND a send over the post clause, a
/// validate over the gate, a lookup over the revision-check words.
const FAQ: &str = "Quand une question arrive dans Slack, retrouve les passages pertinents du guide interne et rédige une réponse avec références. Si les passages ne suffisent pas, la réponse doit le dire. Poste ensuite la réponse dans le fil Slack. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution. Vérifie de nouveau la version courante juste avant cette action ; toute modification annule l'accord précédent.";

fn faq_proposal() -> Value {
    json!({"steps":[
        {"op":"lookup","detail":"la question qui arrive dans Slack","evidence":"Quand une question arrive dans Slack"},
        {"op":"search","detail":"les passages pertinents du guide interne","evidence":"retrouve les passages pertinents du guide interne"},
        {"op":"draft","detail":"une réponse avec références","evidence":"rédige une réponse avec références"},
        {"op":"validate","detail":"la validation humaine de ce dossier précis","evidence":"cette action finale exige la validation humaine de ce dossier précis, avant son exécution"},
        {"op":"lookup","detail":"la version courante","evidence":"Vérifie de nouveau la version courante juste avant cette action"}],
        "effects":[{"verb":"publish","target":"la réponse dans le fil Slack","policy":"human_first","evidence":"Poste ensuite la réponse dans le fil Slack."},
                   {"verb":"send","target":"Poste ensuite la réponse dans le fil Slack","policy":"human_first","evidence":"Poste ensuite la réponse dans le fil Slack."}],
        "obligations":[{"kind":"revision_check","value":null,"evidence":"Vérifie de nouveau la version courante juste avant cette action"}],
        "constraints":["Si les passages ne suffisent pas, la réponse doit le dire."],"unknowns":[],
        "regions":[{"text":"Quand une question arrive dans Slack,","role":"context"},
                   {"text":"retrouve les passages pertinents du guide interne","role":"operation"},
                   {"text":"et rédige une réponse avec références.","role":"operation"},
                   {"text":"Si les passages ne suffisent pas, la réponse doit le dire.","role":"constraint"},
                   {"text":"Poste ensuite la réponse dans le fil Slack.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"},
                   {"text":"Vérifie de nouveau la version courante juste avant cette action ; toute modification annule l'accord précédent.","role":"obligation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_step_over_the_trigger_or_a_safeguard_and_a_twin_effect_are_folded() {
    let provider = Provider::new(faq_proposal());
    let req = CompileRequest::create(FAQ).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("no retrievable source")),
        "the search over the guide is the source the recheck reruns: {out:#?}"
    );
    let (ops, effects) = ops_and_effects(&out);
    assert_eq!(ops, ["search", "draft"], "{out:#?}");
    assert_eq!(
        effects.len(),
        1,
        "one post, not a publish and a send: {out:#?}"
    );
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["effects"][0]["policy"],
        "human_first"
    );
}
