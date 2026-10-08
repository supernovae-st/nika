// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A known configuration refusal starts no authoring activity and claims only the
//! workflow-authoring request it refused.

use std::sync::{Arc, Mutex};

use nika_cli_host::compile::config::AuthoringSettings;

use super::*;
use crate::intelligence::{IntelligenceCensus, IntelligenceKind, UserIntelligencePreference};
use crate::reasoner::{NoReasoner, ReasonError, Reply, ScriptedReasoner, SessionReasoner};

#[test]
fn a_known_context_refusal_does_not_announce_generation_or_change_the_request() {
    let root = tempfile::tempdir().expect("project");
    let pref = UserIntelligencePreference::new(IntelligenceKind::None, None);
    let mut session = SessionRuntime::open_with(
        root.path(),
        IntelligenceCensus::empty(),
        &pref,
        None,
        Box::new(|_| Box::new(NoReasoner)),
    );
    // Exercise this exact seated entry with no monetary/provider route in the fixture.
    // The context must refuse before any compiler/provider or activity starts.
    session.seat = AuthoringSeat::Provider {
        model: "mock/echo".to_owned(),
    };
    let mut settings = AuthoringSettings::none();
    settings.strategy = Some("unsupported-strategy".to_owned());
    let context = AuthoringContext::from_settings(&settings, &AuthoringSettings::none());
    let refused = context.refusal().expect("known refusal").to_string();
    session.set_authoring_context(context);
    let request = "Read ./notes.md and draft a short summary";
    session.intent.goal = Some(request.to_owned());
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&observed);
    session.on_activity(Arc::new(move |activity| {
        if let Ok(mut phases) = sink.lock() {
            phases.push(activity.phase);
        }
    }));
    assert!(
        !session.money_blocks_cognition(),
        "fixture owns no paid route"
    );

    let outcome = session.compile_under_seat(AuthoringRound::new(request));

    let TurnOutcome::Refusal(refusal) = outcome else {
        panic!("expected context refusal, got {outcome:?}");
    };
    assert_eq!(refusal.class, RefusalClass::AuthoringRefused);
    assert!(refusal.text.contains(&refused), "{}", refusal.text);
    assert!(
        refusal
            .text
            .contains("this workflow-authoring request was not sent, nothing was written")
    );
    assert_eq!(session.intent.goal.as_deref(), Some(request));
    assert!(session.pending_proposal().is_none());
    assert_eq!(*observed.lock().expect("activity"), Vec::<Phase>::new());
}

/// The selected model, instrumented: every prompt it is handed, in order, answered by the
/// scripted fake. It names the authoring model, so the route and the seat reach one model.
struct Recorded(Arc<Mutex<Vec<String>>>, ScriptedReasoner);

impl SessionReasoner for Recorded {
    fn name(&self) -> String {
        "recorded fixture".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<Reply, ReasonError> {
        if let Ok(mut prompts) = self.0.lock() {
            prompts.push(prompt.to_owned());
        }
        self.1.reason(prompt)
    }

    fn authoring_model(&self) -> Option<String> {
        Some("mock/echo".to_owned())
    }
}

/// A line the deterministic reader does not settle (not work, then unsettled beside the kept
/// goal) is routed by the selected model before the seated compile meets the refused
/// configuration: the refusal states the workflow-authoring request it did not send, never that
/// nothing reached the model the route has just used.
#[test]
fn a_context_refusal_after_a_routed_line_claims_only_the_unsent_authoring_request() {
    let root = tempfile::tempdir().expect("project");
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&prompts);
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("mock".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "mock".to_owned(),
        },
        Some("mock/echo".to_owned()),
    );
    let mut session = SessionRuntime::open_with(
        root.path(),
        census,
        &pref,
        None,
        Box::new(move |_| {
            let label = ScriptedReasoner::new(vec!["NEW_WORK".to_owned()]);
            Box::new(Recorded(Arc::clone(&recorder), label))
        }),
    );
    let mut settings = AuthoringSettings::none();
    settings.strategy = Some("unsupported-strategy".to_owned());
    let context = AuthoringContext::from_settings(&settings, &AuthoringSettings::none());
    let refused = AuthoringError::Context(context.refusal().expect("known refusal").clone());
    session.set_authoring_context(context);
    // The interactive preparation policy, as the TUI opens it: costs are observed, never a
    // gate; the fake has no metered account and no transport.
    session.enable_continuous_preparation();
    let phases = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&phases);
    session.on_activity(Arc::new(move |activity| {
        if let Ok(mut seen) = sink.lock() {
            seen.push(activity.phase);
        }
    }));
    let seat = AuthoringSeat::Provider {
        model: "mock/echo".to_owned(),
    };
    assert_eq!(
        session.authoring_seat(),
        &seat,
        "the route's model is the seat's"
    );

    let mut claims = Vec::new();
    for line in [
        "build me a digest of the docs",
        "Read ./a.md and do something clever with it, then write ./b.md",
    ] {
        let before = prompts.lock().expect("prompts").len();
        let outcome = session.turn(line);
        // Dispatched: exactly one prompt, the routing label carrying this line.
        let calls = prompts.lock().expect("prompts");
        let routes = session.routes();
        assert_eq!(calls.len(), before + 1, "{line}: {outcome:?} {routes:?}");
        let label = &calls[before];
        assert!(
            label.starts_with("You route ONE line") && label.contains(&format!("«{line}»")),
            "{label}"
        );
        let route = routes.last().expect("the line was routed");
        assert_eq!(
            (route.phase, route.act, route.method),
            (SessionPhase::Idle, TurnAct::NewWork, RoutingMethod::Model)
        );
        let TurnOutcome::Refusal(refusal) = outcome else {
            panic!("{line}: expected the context refusal, got {outcome:?}");
        };
        assert_eq!(refusal.class, RefusalClass::AuthoringRefused);
        claims.push(refusal.text);
    }
    // Claimed: the refused workflow-authoring request alone, never « nothing was sent ».
    assert!(
        !claims
            .iter()
            .any(|claim| claim.contains("nothing was sent")),
        "each route sent its line to `mock/echo`: {claims:#?}"
    );
    let expected = format!(
        "{refused} · this workflow-authoring request was not sent, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again"
    );
    assert_eq!(claims, [expected.clone(), expected]);
    let phases = phases.lock().expect("activity");
    let authored = |phase: &Phase| matches!(phase, Phase::Authoring | Phase::Repairing);
    assert!(
        !phases.iter().any(authored),
        "no authoring started: {phases:?}"
    );
    assert!(session.pending_proposal().is_none());
    let written: Vec<_> = std::fs::read_dir(root.path())
        .expect("project")
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "nika"))
        .collect();
    assert!(written.is_empty(), "nothing was written: {written:?}");
}
