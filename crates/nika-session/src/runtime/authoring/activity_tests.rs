// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A known configuration refusal starts no authoring activity.

use std::sync::{Arc, Mutex};

use nika_cli_host::compile::config::AuthoringSettings;

use super::*;
use crate::intelligence::{IntelligenceCensus, IntelligenceKind, UserIntelligencePreference};
use crate::reasoner::NoReasoner;

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
            .contains("nothing was sent to the authoring model")
    );
    assert_eq!(session.intent.goal.as_deref(), Some(request));
    assert!(session.pending_proposal().is_none());
    assert_eq!(*observed.lock().expect("activity"), Vec::<Phase>::new());
}
