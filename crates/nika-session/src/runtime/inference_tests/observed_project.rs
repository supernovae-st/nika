// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The project the Session observes for its seat: the first generation is told every
//! column of the file the request names (not only the ones it words), an answer round and a
//! revision are told the same project for the intent the compiler reads, and a link that leads
//! outside the project is never read. Loopback mechanics only; nothing leaves the machine.
use super::*;
use crate::authoring::{
    AUTHORING_CALLS_PER_COMPILE, AUTHORING_INITIAL_TOKENS, AUTHORING_MAX_TOKENS, AUTHORING_REPAIRS,
    AUTHORING_TIMEOUT, AuthoringContext, AuthoringRound, AuthoringSeat, compile_in_with_admission,
    session_policy,
};
use nika_onboard::compile::{CompileRequest, NativeMode, revise_intent};
use nika_providers::InferenceAdmission;

const SALES: &str = "Lis ventes.csv, garde les lignes dont statut vaut paye, écris-les dans paiements.csv avec le même en-tête et écris leur montant total sous forme de nombre dans paiements-total.txt.";
const CHANGE: &str =
    "Appelle finalement le fichier règlements.csv ; conserve le filtre et le total.";
/// Columns the request never words: the observation must still carry them.
const UNWORDED: [&str; 3] = ["client", "devise", "reference_interne"];

fn sales(root: &Path) {
    std::fs::write(
        root.join("ventes.csv"),
        "id,date,client,statut,montant,devise,reference_interne\n1,2026-09-01,Acme,paye,120.50,EUR,A-1\n2,2026-09-02,Bolt,impaye,80,EUR,A-2\n3,2026-09-03,Cora,paye,42,EUR,A-3\n",
    )
    .expect("fixture");
}

/// The native-first context the root's live attempt ran under, observing `root`.
fn context(root: &Path) -> AuthoringContext {
    AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("only"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    )
    .with_project_root(root.to_path_buf())
}

fn seat() -> AuthoringSeat {
    AuthoringSeat::Provider {
        model: MODEL.to_owned(),
    }
}

/// The text of the first request the seat received.
fn first_request(peer: &Peer) -> String {
    peer.bodies()
        .first()
        .map(Value::to_string)
        .expect("a generation was sent")
}

#[test]
fn the_first_generation_is_told_every_observed_column_of_the_named_source() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let out = compile_in_with_admission(
        &seat(),
        &context(dir.path()),
        &CompileRequest::create(SALES),
        SALES,
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    assert!(first.contains("observed_world"), "{first}");
    for column in ["statut", "montant"].iter().chain(UNWORDED.iter()) {
        assert!(first.contains(column), "{column} missing: {first}");
    }
    assert!(
        first.contains("impaye"),
        "the categorical values ride along: {first}"
    );
    // Data only: no row value that is not categorical (a client name, an amount) is presented.
    assert!(
        !first.contains("Acme") && !first.contains("120.50"),
        "{first}"
    );
    // The receipt says what was observed and presented, by path and count.
    if let Ok(out) = out {
        let observed = &out.provenance.decision.as_ref().expect("record")["session"]["observed"];
        assert_eq!(observed["attached"], true);
        assert_eq!(observed["presented"], true);
        assert_eq!(observed["rows"][0]["path"], "ventes.csv", "{observed}");
        assert_eq!(observed["rows"][0]["state"], "observed", "{observed}");
        assert_eq!(observed["rows"][0]["columns"], 7, "{observed}");
    }
}

#[test]
fn an_answer_round_is_told_the_project_for_the_intent_it_reads() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    // The first words named no file; the answered clarification replaces the request.
    let mut round = AuthoringRound::new("Fais le tri des paiements.");
    round.answers.insert(
        "intent.clarification".to_owned(),
        serde_json::to_string(SALES).unwrap(),
    );
    let _ = round.compile_with_admission(
        &seat(),
        &context(dir.path()),
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    for column in UNWORDED {
        assert!(first.contains(column), "{column} missing: {first}");
    }
}

#[test]
fn a_revision_is_told_the_base_requests_project_and_its_change() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let base: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/compile/copy-fr.json"))
            .expect("fixture");
    let base = base["candidate"].as_str().expect("candidate").to_owned();
    let request = CompileRequest::edit(base, CHANGE).with_original_intent(SALES);
    let revised = revise_intent(&request).expect("the revision reads the base request");
    let _ = compile_in_with_admission(
        &seat(),
        &context(dir.path()),
        &request,
        &revised,
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    for column in UNWORDED {
        assert!(
            first.contains(column),
            "{column} missing from the revision: {first}"
        );
    }
    assert!(
        first.contains("r\\u00e8glements.csv") || first.contains("règlements.csv"),
        "the change's own destination is observed too: {first}"
    );
}

#[cfg(unix)]
#[test]
fn a_link_leading_outside_the_project_is_never_presented() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("secret.csv"),
        "salaire_confidentiel,iban_prive\n1,FR76\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.csv"),
        dir.path().join("ventes.csv"),
    )
    .unwrap();
    let _ = compile_in_with_admission(
        &seat(),
        &context(dir.path()),
        &CompileRequest::create(SALES),
        SALES,
        &InferenceAdmission::unbudgeted(),
    );
    let first = first_request(&peer);
    for leaked in ["salaire_confidentiel", "iban_prive", "FR76"] {
        assert!(!first.contains(leaked), "{leaked} leaked: {first}");
    }
    assert!(first.contains("outside_project"), "{first}");
}

#[test]
fn without_a_project_root_nothing_is_observed() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let context = AuthoringContext::from_settings(
        &nika_cli_host::compile::config::AuthoringSettings::none().with_strategy("only"),
        &nika_cli_host::compile::config::AuthoringSettings::none(),
    );
    let _ = compile_in_with_admission(
        &seat(),
        &context,
        &CompileRequest::create(SALES),
        SALES,
        &InferenceAdmission::unbudgeted(),
    );
    assert!(!first_request(&peer).contains("reference_interne"));
}

#[test]
fn the_session_budget_and_its_review_are_one_bound() {
    let policy = session_policy(MODEL, false, NativeMode::Escalate);
    assert_eq!(policy.max_tokens, AUTHORING_MAX_TOKENS);
    assert_eq!(policy.initial_max_tokens, Some(AUTHORING_INITIAL_TOKENS));
    const { assert!(AUTHORING_INITIAL_TOKENS < AUTHORING_MAX_TOKENS) };
    assert_eq!(policy.timeout, AUTHORING_TIMEOUT);
    assert_eq!(policy.repairs, AUTHORING_REPAIRS);
    assert_eq!(
        session_policy(MODEL, true, NativeMode::Escalate).timeout,
        std::time::Duration::from_secs(300),
        "a subscription harness keeps its own deadline"
    );
    // One classification + one seated compile (COLD plan and evidence repair, native opening and
    // its repairs) is exactly what a fresh unknown-cost review admits, at the same ceilings.
    assert_eq!(
        nika_providers::admission::SESSION_REVIEW_MAX_REQUESTS,
        1 + AUTHORING_CALLS_PER_COMPILE
    );
    assert_eq!(
        nika_providers::admission::SESSION_REVIEW_MAX_OUTPUT_TOKENS,
        AUTHORING_MAX_TOKENS
    );
    assert_eq!(
        nika_providers::admission::SESSION_REVIEW_TIMEOUT,
        AUTHORING_TIMEOUT
    );
}

#[test]
fn a_hot_copy_observes_the_source_without_claiming_model_presentation() {
    let peer = Peer::start(vec![]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().unwrap();
    sales(dir.path());
    let intent = "Copy ./ventes.csv to ./copie.csv";
    let out = compile_in_with_admission(
        &seat(),
        &AuthoringContext::default().with_project_root(dir.path()),
        &CompileRequest::create(intent),
        intent,
        &InferenceAdmission::unbudgeted(),
    )
    .expect("hot copy");
    assert!(peer.bodies().is_empty());
    let observed = &out.provenance.decision.expect("record")["session"]["observed"];
    assert_eq!(observed["attached"], true);
    assert_eq!(observed["presented"], false);
    assert_eq!(observed["rows"][0]["state"], "observed");
}
