// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One clause is one effect (eco-60 on nika-5d7ed9dc, gpt-5-mini): E09B assembled a `send`
//! beside the `publish` over « Poste ensuite la réponse dans le fil Slack » — the citations
//! differ by a trailing period — and a pathless `write` over « conserve un état à reprendre
//! manuellement », a constraint the reading carries as guidance; E17B the same write over
//! « puis conserve un état à reprendre manuellement. ». Effects compare by their clause key;
//! a write naming no place over a stated constraint is that constraint.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile_with_provider};
use serde_json::{Value, json};

mod common;
use common::{Provider, keys, policy};

fn ops(out: &nika_compile::CompileOutcome) -> Vec<String> {
    out.provenance.plan.as_ref().unwrap()["operations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["op"].as_str().unwrap().to_owned())
        .collect()
}

fn effects(out: &nika_compile::CompileOutcome) -> Vec<(String, String)> {
    out.provenance.plan.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["verb"].as_str().unwrap().to_owned(),
                e["policy"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

const FAQ: &str = "Quand une question arrive dans Slack, retrouve les passages pertinents du guide interne et rédige une réponse avec références. Si les passages ne suffisent pas, la réponse doit le dire. Poste ensuite la réponse dans le fil Slack. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution. Vérifie de nouveau la version courante juste avant cette action ; toute modification annule l’accord précédent. Limite les nouvelles tentatives à 2, puis conserve un état à reprendre manuellement.";

fn faq_proposal() -> Value {
    json!({"steps":[
        {"op":"search","detail":"Retrouver les passages pertinents du guide interne correspondant à la question","evidence":"retrouve les passages pertinents du guide interne"},
        {"op":"draft","detail":"Rédiger une réponse avec références incluant les passages extraits","evidence":"rédige une réponse avec références"}],
        "effects":[
        {"verb":"publish","target":"la réponse dans le fil Slack","policy":"human_first","evidence":"Poste ensuite la réponse dans le fil Slack"},
        {"verb":"send","target":"Poste ensuite la réponse dans le fil Slack.","policy":"human_first","evidence":"Poste ensuite la réponse dans le fil Slack."},
        {"verb":"write","target":"conserve un état à reprendre manuellement","policy":"automatic","evidence":"conserve un état à reprendre manuellement"}],
        "obligations":[
        {"kind":"revision_check","evidence":"Vérifie de nouveau la version courante juste avant cette action"},
        {"kind":"retry_bound","value":2,"evidence":"Limite les nouvelles tentatives à 2"}],
        "constraints":["Si les passages ne suffisent pas, la réponse doit le dire.","toute modification annule l’accord précédent"],"unknowns":[],
        "regions":[{"text":"Quand une question arrive dans Slack,","role":"context"},
                   {"text":"retrouve les passages pertinents du guide interne","role":"operation"},
                   {"text":"et rédige une réponse avec références.","role":"operation"},
                   {"text":"Si les passages ne suffisent pas, la réponse doit le dire.","role":"constraint"},
                   {"text":"Poste ensuite la réponse dans le fil Slack.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"},
                   {"text":"Vérifie de nouveau la version courante juste avant cette action ; toute modification annule l’accord précédent.","role":"obligation"},
                   {"text":"Limite les nouvelles tentatives à 2, puis conserve un état à reprendre manuellement.","role":"obligation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_send_twin_differing_by_a_period_and_a_pathless_write_over_a_constraint_are_folded() {
    let provider = Provider::new(faq_proposal());
    let req = CompileRequest::create(FAQ).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["search", "draft"], "{out:#?}");
    assert_eq!(
        effects(&out),
        [("publish".to_owned(), "human_first".to_owned())],
        "{out:#?}"
    );
}

const REFUND: &str = "Retrouve l’historique du client, classe son ticket en facturation ou technique, et prépare une réponse fidèle aux faits. Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier. Rembourse ensuite le double débit. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution. Vérifie de nouveau la version courante juste avant cette action ; toute modification annule l’accord précédent. Limite les nouvelles tentatives à 2, puis conserve un état à reprendre manuellement.";

fn refund_proposal() -> Value {
    json!({"steps":[
        {"op":"lookup","detail":"Retrouve l'historique du client","evidence":"Retrouve l’historique du client"},
        {"op":"classify","detail":"classe son ticket en facturation ou technique","evidence":"classe son ticket en facturation ou technique","categories":["facturation","technique"]},
        {"op":"draft","detail":"prépare une réponse fidèle aux faits","evidence":"prépare une réponse fidèle aux faits"}],
        "effects":[
        {"verb":"refund","target":"le double débit","policy":"human_first","evidence":"Rembourse ensuite le double débit"},
        {"verb":"write","target":"conserve un état à reprendre manuellement.","policy":"automatic","evidence":"puis conserve un état à reprendre manuellement."}],
        "obligations":[
        {"kind":"revision_check","evidence":"Vérifie de nouveau la version courante juste avant cette action"},
        {"kind":"retry_bound","value":2,"evidence":"Limite les nouvelles tentatives à 2"}],
        "constraints":["Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier","toute modification annule l’accord précédent"],"unknowns":[],
        "regions":[{"text":"Retrouve l’historique du client,","role":"operation"},
                   {"text":"classe son ticket en facturation ou technique,","role":"operation"},
                   {"text":"et prépare une réponse fidèle aux faits.","role":"operation"},
                   {"text":"Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier.","role":"constraint"},
                   {"text":"Rembourse ensuite le double débit.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"},
                   {"text":"Vérifie de nouveau la version courante juste avant cette action ; toute modification annule l’accord précédent.","role":"obligation"},
                   {"text":"Limite les nouvelles tentatives à 2, puis conserve un état à reprendre manuellement.","role":"obligation"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_pathless_write_over_the_resume_constraint_is_the_constraint() {
    let provider = Provider::new(refund_proposal());
    let req = CompileRequest::create(REFUND).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["lookup", "classify", "draft"], "{out:#?}");
    assert_eq!(
        effects(&out),
        [("refund".to_owned(), "human_first".to_owned())],
        "{out:#?}"
    );
}
