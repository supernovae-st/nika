// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_compile_fidelity::fidelity::resolution::{ResolutionKind, ResolutionRole};
use nika_session_change::outcome::{Incarnation, QuestionId};
use nika_session_change::tools::ToolReply;
use nika_session_change::work::{ProvenanceKind, ValueRole};
use serde_json::{Value, json};

use super::super::{Citations, Conversation, Pick};

const HAIKU: &str = "anthropic/claude-haiku-5-5";
const SONNET: &str = "anthropic/claude-sonnet-5-5";
/// The person's words that leave the run model to Nika (QA gauntlet q04).
const LEFT: &str = "carte blanche, choisis le meilleur modèle toi-même";

fn citations(lines: &[(&str, &str)]) -> Citations {
    let mut out = Citations::default();
    for (k, (cite, text)) in lines.iter().enumerate() {
        let at = u64::try_from(k).unwrap() * 3 + 1;
        out.record(at, Some(((*cite).into(), (*text).into())));
    }
    out
}

fn ask(conversation: &mut Conversation, lines: &Citations, questions: &Value) -> ToolReply {
    let asker = Arc::new(Incarnation);
    let mut mint = |context: &str| QuestionId::new(context.to_owned(), &asker);
    conversation.ask(
        lines,
        questions,
        Some("c1"),
        (&mut mint, &mut |_: &str| None),
    )
}

/// The run model asked with no offer, and left to Nika by the person's u2.
fn left() -> (Conversation, Citations, Vec<String>) {
    let mut conversation = Conversation::default();
    let lines = citations(&[("u1", "passe l'étape IA sur un autre modèle"), ("u2", LEFT)]);
    let which = json!({"questions": [{"key": "run_model", "role": "run_model",
        "question": "Quel modèle pour l'étape IA ?"}]});
    assert!(ask(&mut conversation, &lines, &which).ends_turn);
    let read = BTreeMap::from([("run_model".to_owned(), Pick::Delegated)]);
    let said = conversation.answered_by("u2", LEFT, &read);
    (conversation, lines, said)
}

/// The models offered again, Haiku recommended, under the question `key`.
fn offered(key: &str, recommended: bool) -> Value {
    json!({"questions": [{"key": key, "question": "Quel modèle pour l'étape IA ?", "options": [
        {"key": "haiku", "label": "Claude Haiku 5.5", "recommended": recommended,
         "values": [{"role": "run_model", "value": HAIKU}]},
        {"key": "sonnet", "label": "Claude Sonnet 5.5",
         "values": [{"role": "run_model", "value": SONNET}]}]}]})
}

fn refused(reply: &ToolReply) -> Vec<Value> {
    let text: Value = serde_json::from_str(&reply.text).expect("the reply is JSON");
    text["refused"].as_array().cloned().unwrap_or_default()
}

#[test]
fn a_run_model_left_to_nika_is_offered_again_never_bound_by_the_author() {
    let (conversation, _, said) = left();
    assert!(
        said[0].contains("a run model is bound only from an offer"),
        "{said:?}"
    );
    assert!(!said[0].contains("bind it as delegated"), "{said:?}");
    let delegation = &conversation.delegations()[0];
    assert_eq!(
        (delegation.message.as_str(), delegation.scope),
        ("u2", ValueRole::RunModel)
    );
}

#[test]
fn the_recommended_model_offered_after_is_taken_for_the_person_unasked() {
    for key in ["run_model", "model"] {
        let (mut conversation, lines, _) = left();
        let reply = ask(&mut conversation, &lines, &offered(key, true));
        assert!(!reply.ends_turn, "{key}: nothing is asked: {reply:?}");
        assert!(conversation.questions().is_empty(), "{key}");
        let taken = refused(&reply);
        assert_eq!(taken[0]["reason"], "delegated", "{key}");
        assert_eq!(taken[0]["taken"], "haiku", "{key}");
        let bound = &conversation.bindings()[0];
        assert_eq!(
            (
                bound.value.as_str(),
                bound.role,
                bound.provenance.kind,
                bound.provenance.message.as_str()
            ),
            (HAIKU, ValueRole::RunModel, ProvenanceKind::Delegated, "u2"),
            "{key}"
        );
        // The Session states the selection: kept, which the fidelity law admits for a model.
        let (_, host) = conversation.selections();
        let row = host
            .iter()
            .find(|row| row.value == HAIKU)
            .expect("the host row");
        assert_eq!(
            (row.kind, row.role),
            (ResolutionKind::Retained, ResolutionRole::RunModel)
        );
    }
}

#[test]
fn with_no_recommended_model_the_author_is_told_to_offer_one() {
    let (mut conversation, lines, _) = left();
    let reply = ask(&mut conversation, &lines, &offered("model", false));
    assert!(!reply.ends_turn, "{reply:?}");
    let told = refused(&reply);
    let why = told[0]["why"].as_str().unwrap_or_default();
    assert!(why.contains("one marked recommended"), "{why}");
    assert!(conversation.bindings().is_empty());
    assert!(conversation.questions().is_empty());
}

#[test]
fn a_model_the_person_named_after_is_theirs_and_never_taken() {
    let (mut conversation, mut lines, _) = left();
    lines.record(20, Some(("u3".into(), format!("prends {SONNET}"))));
    let named = json!({"answered": [{"key": "run_model", "value": SONNET, "message": "u3",
        "excerpt": SONNET}]});
    let reply = ask(&mut conversation, &lines, &named);
    assert!(!reply.is_error, "{reply:?}");
    let reply = ask(&mut conversation, &lines, &offered("model", true));
    let told = refused(&reply);
    assert_eq!(told[0]["reason"], "settled", "{told:?}");
    let models: Vec<&str> = (conversation.bindings().iter())
        .filter(|b| b.role == ValueRole::RunModel)
        .map(|b| b.value.as_str())
        .collect();
    assert_eq!(models, [SONNET]);
}
