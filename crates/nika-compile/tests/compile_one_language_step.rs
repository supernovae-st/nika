// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One clause is one step (eco-60 on nika-4d89702a, nika-a41a4fd2 and nika-5d7ed9dc,
//! gpt-5-mini): E11A came out READY with an `extract` over the draft's own clause, E09B with an
//! `extract` over the search's clause, a `validate` over a stated constraint, an `explore` over
//! the send clause and a `read` over the event clause, E17B with a `validate` over a constraint
//! differing by a trailing period and an `extract` over the refund clause, and E16C READY once
//! more because the seat's read detail said « les disponibilités fournies » — a word the
//! request never wrote. The request's words decide what is supplied; a second language step
//! over a carried clause, a validate over a constraint, a step over an effect's clause and a
//! read over the event are folded.
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

const INCIDENT: &str = "Un signalement contient les symptômes et le service concerné. Consulte le runbook, classe le signalement en infrastructure ou application et prépare une fiche d'incident avec les faits fournis. Ouvre ensuite un ticket d'incident dans Jira.";

fn incident_proposal() -> Value {
    json!({"steps":[
        {"op":"search","detail":"le runbook pertinent","evidence":"Consulte le runbook"},
        {"op":"classify","detail":"le signalement en infrastructure ou application","evidence":"classe le signalement en infrastructure ou application","categories":["infrastructure","application"]},
        {"op":"draft","detail":"une fiche d'incident avec les faits fournis","evidence":"prépare une fiche d'incident avec les faits fournis"},
        {"op":"extract","detail":"les champs nécessaires pour créer le ticket (titre, description)","evidence":"prépare une fiche d'incident avec les faits fournis"}],
        "effects":[{"verb":"create","target":"un ticket d'incident dans Jira","policy":"automatic","evidence":"Ouvre ensuite un ticket d'incident dans Jira."}],
        "obligations":[],"constraints":[],"unknowns":[],
        "regions":[{"text":"Un signalement contient les symptômes et le service concerné.","role":"context"},
                   {"text":"Consulte le runbook,","role":"operation"},
                   {"text":"classe le signalement en infrastructure ou application","role":"operation"},
                   {"text":"et prépare une fiche d'incident avec les faits fournis.","role":"operation"},
                   {"text":"Ouvre ensuite un ticket d'incident dans Jira.","role":"effect"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_second_language_step_over_a_carried_clause_is_folded() {
    let provider = Provider::new(incident_proposal());
    let req = CompileRequest::create(INCIDENT).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["search", "classify", "draft"], "{out:#?}");
}

const FAQ: &str = "Quand une question arrive dans Slack, retrouve les passages pertinents du guide interne et rédige une réponse avec références. Si les passages ne suffisent pas, la réponse doit le dire. Poste ensuite la réponse dans le fil Slack. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.";

fn faq_proposal() -> Value {
    json!({"steps":[
        {"op":"search","detail":"les passages pertinents du guide interne","evidence":"retrouve les passages pertinents du guide interne"},
        {"op":"extract","detail":"les passages pertinents identifiés du guide interne","evidence":"retrouve les passages pertinents du guide interne"},
        {"op":"draft","detail":"une réponse avec références","evidence":"rédige une réponse avec références"},
        {"op":"validate","detail":"vérifier si les passages suffisent ; sinon, le dire","evidence":"Si les passages ne suffisent pas, la réponse doit le dire."},
        {"op":"explore","detail":"après validation humaine, procéder à la publication","evidence":"Poste ensuite la réponse dans le fil Slack."}],
        "effects":[{"verb":"publish","target":"la réponse dans le fil Slack","policy":"human_first","evidence":"Poste ensuite la réponse dans le fil Slack."}],
        "obligations":[],
        "constraints":["Si les passages ne suffisent pas, la réponse doit le dire."],"unknowns":[],
        "regions":[{"text":"Quand une question arrive dans Slack,","role":"context"},
                   {"text":"retrouve les passages pertinents du guide interne","role":"operation"},
                   {"text":"et rédige une réponse avec références.","role":"operation"},
                   {"text":"Si les passages ne suffisent pas, la réponse doit le dire.","role":"constraint"},
                   {"text":"Poste ensuite la réponse dans le fil Slack.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn an_extract_over_the_search_a_validate_over_a_constraint_and_an_explore_over_the_send_are_folded()
 {
    let provider = Provider::new(faq_proposal());
    let req = CompileRequest::create(FAQ).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["search", "draft"], "{out:#?}");
}

/// The seat of the tenth-law lane listed a `read` over the event clause itself, its detail
/// anchored in words outside the clause (« lire la question entrante et capturer le fil et
/// l'identifiant de message pour la réponse »): an event is never a file to read.
fn faq_event_read_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"Quand une question arrive dans Slack, lire la question entrante et capturer le fil et l'identifiant de message pour la réponse","evidence":"Quand une question arrive dans Slack"},
        {"op":"search","detail":"Retrouver les passages pertinents du guide interne correspondant à la question","evidence":"retrouve les passages pertinents du guide interne"},
        {"op":"extract","detail":"Extraire les passages pertinents identifiés (citations exactes, références de section)","evidence":"retrouve les passages pertinents du guide interne"},
        {"op":"draft","detail":"Rédiger une réponse avec références incluant les passages extraits","evidence":"rédige une réponse avec références"}],
        "effects":[{"verb":"publish","target":"la réponse dans le fil Slack","policy":"human_first","evidence":"Poste ensuite la réponse dans le fil Slack"}],
        "obligations":[],
        "constraints":["Si les passages ne suffisent pas, la réponse doit le dire."],"unknowns":[],
        "regions":[{"text":"Quand une question arrive dans Slack,","role":"context"},
                   {"text":"retrouve les passages pertinents du guide interne","role":"operation"},
                   {"text":"et rédige une réponse avec références.","role":"operation"},
                   {"text":"Si les passages ne suffisent pas, la réponse doit le dire.","role":"constraint"},
                   {"text":"Poste ensuite la réponse dans le fil Slack.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_read_the_seat_lists_over_the_event_clause_is_the_trigger() {
    let provider = Provider::new(faq_event_read_proposal());
    let req = CompileRequest::create(FAQ).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["search", "draft"], "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(
        plan["trigger"].as_str(),
        Some("quand une question arrive dans slack"),
        "{plan:#?}"
    );
}

const REFUND: &str = "Retrouve l’historique du client, classe son ticket en facturation ou technique, et prépare une réponse fidèle aux faits. Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier. Rembourse ensuite le double débit. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.";

fn refund_proposal() -> Value {
    json!({"steps":[
        {"op":"lookup","detail":"Retrouve l'historique du client","evidence":"Retrouve l’historique du client"},
        {"op":"classify","detail":"classe son ticket en facturation ou technique","evidence":"classe son ticket en facturation ou technique","categories":["facturation","technique"]},
        {"op":"draft","detail":"prépare une réponse fidèle aux faits","evidence":"prépare une réponse fidèle aux faits"},
        {"op":"validate","detail":"Valide l'éligibilité : seuls les doubles débits vérifiés sont éligibles et le montant remboursable doit être <= 40 euros","evidence":"Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier."},
        {"op":"extract","detail":"Extrait les détails nécessaires au remboursement (transactions en double, montants, identifiants)","evidence":"Rembourse ensuite le double débit."}],
        "effects":[{"verb":"refund","target":"le double débit","policy":"human_first","evidence":"Rembourse ensuite le double débit"}],
        "obligations":[],
        "constraints":["Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier"],"unknowns":[],
        "regions":[{"text":"Retrouve l’historique du client,","role":"operation"},
                   {"text":"classe son ticket en facturation ou technique,","role":"operation"},
                   {"text":"et prépare une réponse fidèle aux faits.","role":"operation"},
                   {"text":"Seuls les doubles débits vérifiés sont éligibles, dans la limite de 40 euros par dossier.","role":"constraint"},
                   {"text":"Rembourse ensuite le double débit.","role":"effect"},
                   {"text":"Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution.","role":"policy"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn a_validate_over_a_constraint_with_a_period_and_an_extract_over_the_refund_clause_are_folded()
 {
    let provider = Provider::new(refund_proposal());
    let req = CompileRequest::create(REFUND).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert!(!keys(&out).contains(&"intent.clarification"), "{out:#?}");
    assert_eq!(ops(&out), ["lookup", "classify", "draft"], "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let verbs: Vec<&str> = plan["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["verb"].as_str().unwrap())
        .collect();
    assert_eq!(verbs, ["refund"], "{plan:#?}");
}

const SLOTS: &str = "Lis mes disponibilités et celles des participants, puis propose par écrit trois créneaux compatibles dans le fuseau Europe/Paris. Arrête-toi après ces étapes ; aucune autre action n'est demandée.";

fn slots_proposal() -> Value {
    json!({"steps":[
        {"op":"read","detail":"Lire les disponibilités fournies : mes disponibilités et celles des participants","evidence":"Lis mes disponibilités et celles des participants"},
        {"op":"draft","detail":"trois créneaux compatibles dans le fuseau Europe/Paris","evidence":"propose par écrit trois créneaux compatibles dans le fuseau Europe/Paris"}],
        "effects":[],"obligations":[],
        "constraints":["Arrête-toi après ces étapes ; aucune autre action n'est demandée."],"unknowns":[],
        "regions":[{"text":"Lis mes disponibilités et celles des participants,","role":"operation"},
                   {"text":"puis propose par écrit trois créneaux compatibles dans le fuseau Europe/Paris.","role":"operation"},
                   {"text":"Arrête-toi après ces étapes ; aucune autre action n'est demandée.","role":"constraint"}],
        "approval_bypass":{"present":false,"evidence":""}})
}

#[tokio::test]
async fn the_requests_words_decide_what_is_supplied_never_the_seats_paraphrase() {
    let provider = Provider::new(slots_proposal());
    let req = CompileRequest::create(SLOTS).with_authoring_policy(policy());
    let out = compile_with_provider(&req, &provider).await.unwrap();
    assert_eq!(ops(&out), ["lookup", "draft"], "{out:#?}");
    assert!(
        out.candidate.is_none(),
        "no READY before the records' place is known: {out:#?}"
    );
}
