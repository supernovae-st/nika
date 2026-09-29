// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The conversation's own calls ask the session's explicit reasoning effort (R4 B16 · C11): the
//! turn-routing label, from the fresh reasoner the factory builds for every routed turn, and the
//! conversational turn carry the level the session holds when the call is made (a host may
//! replace the context after open), on the same call: the same ceiling, temperature and words,
//! the effort keys added. A path that cannot carry the level refuses it before any byte or call,
//! and a session naming none sends what it sent before. Loopback mechanics through the cfg(test)
//! HTTP substitution: nothing leaves the machine.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_cli_host::compile::config::AuthoringSettings;
use nika_error::cost::Cost;
use nika_onboard::compile::AuthoringReasoning;
use nika_providers::InferenceAdmission;
use serde_json::Value;

use super::*;
use crate::DataLocus;
use crate::authoring::AuthoringContext;
use crate::reasoner::{ProviderReasoner, ReasonError, Reply, test_transport};
use crate::runtime::inference_tests::wire::{Peer, response};
use crate::turn::{ReasonerClassifier, RoutingMethod, SessionPhase, TurnClassifier, TurnContext};

/// The one route whose catalog lists the three levels.
const QUALIFIED: &str = "deepseek/deepseek-v4-pro";
/// A route that lists no level of its own: an explicit level is refused there.
const UNLISTED: &str = "deepseek/deepseek-flash";
/// A line the label reads whole.
const RAW: &str = "Explain this destination, without changing it.\nKeep my exact words.";

fn seat(model: &str) -> ProviderReasoner {
    ProviderReasoner {
        model: model.into(),
        label: "DeepSeek".into(),
    }
}

/// A context a host names after open: its own word over an empty environment.
fn named(word: &str) -> AuthoringContext {
    AuthoringContext::from_settings(
        &AuthoringSettings::none().with_reasoning(word),
        &AuthoringSettings::none(),
    )
}

fn account() -> InferenceAdmission {
    InferenceAdmission::new(Cost::new(20_000_000_000)).expect("account")
}

fn question() -> TurnContext {
    TurnContext {
        phase: SessionPhase::QuestionPending,
        automation: Some("the existing work".into()),
        last_prompt: Some("Which destination?".into()),
    }
}

/// A session on the qualified route, opened naming no level, whose factory counts the fresh
/// reasoners it builds.
fn open(root: &Path, built: &Arc<AtomicUsize>) -> SessionRuntime {
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: Some(QUALIFIED.into()),
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let mut s = SessionRuntime::open(root, selected, Box::new(seat(QUALIFIED)));
    let count = Arc::clone(built);
    s.factory = Some(Box::new(move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        Box::new(seat(QUALIFIED))
    }));
    assert_eq!(
        s.authoring_context().reasoning(),
        None,
        "opened naming none"
    );
    s
}

/// The body asks `max` as the qualified route's two keys.
fn asks_max(body: &Value) {
    assert_eq!(body["thinking"]["type"], "enabled", "{body}");
    assert_eq!(body["reasoning_effort"], "max", "{body}");
}

/// The body carries no effort key.
fn asks_none(body: &Value) {
    assert!(body.get("thinking").is_none(), "{body}");
    assert!(body.get("reasoning_effort").is_none(), "{body}");
}

#[test]
fn every_routed_label_and_turn_ask_the_level_the_session_holds_when_it_calls() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let built = Arc::new(AtomicUsize::new(0));
    let mut s = open(dir.path(), &built);
    s.admit_money("budget 2 USD", false).expect("admit");
    // The host replaces the context after open: the next calls ask its level.
    s.set_authoring_context(named("max"));
    for raw in ["tell me more about it", "and then?"] {
        assert_eq!(
            s.classify(SessionPhase::Idle, raw).method,
            RoutingMethod::Model
        );
    }
    assert_eq!(
        built.load(Ordering::SeqCst),
        2,
        "a fresh reasoner each routed turn"
    );
    s.reason_with_money(RAW, false).expect("the turn");
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 3, "two labels and one turn");
    bodies.iter().for_each(asks_max);
    assert!(bodies[1].to_string().contains("and then?"), "{}", bodies[1]);
    assert!(bodies[2].to_string().contains("Keep my exact words."));
    assert_eq!(
        (&bodies[0]["max_tokens"], &bodies[2]["max_tokens"]),
        (&Value::from(4096), &Value::from(8192)),
        "the label and turn ceilings, unchanged by the level"
    );
    // Replaced again, naming none: the next label reads it then, never an earlier turn's level.
    s.set_authoring_context(AuthoringContext::default());
    s.classify(SessionPhase::Idle, "one more");
    s.reason_with_money("and the turn", false)
        .expect("the turn");
    let bodies = peer.bodies();
    assert_eq!((bodies.len(), built.load(Ordering::SeqCst)), (5, 3));
    bodies[3..].iter().for_each(asks_none);
}

/// One label on the qualified route through the classifier under the shared allowance (a direct
/// priced route is only ever called under one), asking `effort`, and its body.
fn label_body(effort: Option<AuthoringReasoning>) -> Value {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let decision = ReasonerClassifier::new(Box::new(seat(QUALIFIED)))
        .asking(effort)
        .classify_with_admission(&question(), RAW, &account());
    assert_eq!(decision.method, RoutingMethod::Model, "{decision:?}");
    let mut bodies = peer.bodies();
    assert_eq!(bodies.len(), 1);
    bodies.remove(0)
}

#[test]
fn a_label_asking_the_level_is_the_same_label_call_with_the_effort_keys_added() {
    let before = label_body(None);
    asks_none(&before);
    assert_eq!(before["max_tokens"], 4096, "{before}");
    assert_eq!(before["temperature"], 0.0, "{before}");
    let mut asked = label_body(Some(AuthoringReasoning::Max));
    asks_max(&asked);
    let keys = asked.as_object_mut().expect("a request object");
    keys.remove("thinking");
    keys.remove("reasoning_effort");
    assert_eq!(asked, before, "the same ceiling, temperature and words");
}

/// A path the conversation cannot ask a level on, counting every call it receives.
struct Counting(Arc<AtomicUsize>);

impl crate::reasoner::SessionReasoner for Counting {
    fn name(&self) -> String {
        "counting fixture".into()
    }

    fn reason(&mut self, _: &str) -> Result<Reply, ReasonError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ReasonError::Seat("a fixture never answers".into()))
    }
}

#[test]
fn a_path_that_cannot_carry_the_level_refuses_it_before_any_byte_or_call() {
    // The route lists no level: the provider refuses before a byte, the allowance untouched.
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let account = account();
    let decision = ReasonerClassifier::new(Box::new(seat(UNLISTED)))
        .asking(Some(AuthoringReasoning::Max))
        .classify_with_admission(&question(), RAW, &account);
    assert_eq!(decision.method, RoutingMethod::Failed, "{decision:?}");
    assert!(peer.bodies().is_empty(), "{:?}", peer.bodies());
    assert!(account.snapshot().expect("receipt").attempts.is_empty());
    // A reasoner that cannot carry a level refuses the label and the turn before calling.
    let calls = Arc::new(AtomicUsize::new(0));
    let decision = ReasonerClassifier::new(Box::new(Counting(Arc::clone(&calls))))
        .asking(Some(AuthoringReasoning::Max))
        .classify(&question(), RAW);
    assert_eq!(decision.method, RoutingMethod::Failed);
    assert!(
        decision.note.as_deref().is_some_and(|note| {
            note.contains("cannot carry the explicit reasoning effort `max`")
                && note.contains("nothing was sent")
        }),
        "{decision:?}"
    );
    // A local path the session lets reason without an allowance, so only the level refuses.
    let dir = tempfile::tempdir().expect("root");
    let mut s = SessionRuntime::open(
        dir.path(),
        ResolvedSessionIntelligence {
            kind: IntelligenceKind::Local {
                provider: "fixture".into(),
            },
            model: None,
            locus: DataLocus::Local,
            ready: true,
            why: None,
        },
        Box::new(Counting(Arc::clone(&calls))),
    );
    s.set_authoring_context(named("max"));
    let refused = s.reason_with_money(RAW, false);
    assert!(
        matches!(&refused, Err(ReasonError::Provider(why)) if why.contains("`max`")),
        "{refused:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no call was made");
    // Naming none, the same reasoner is called as before: the refusal was the level's.
    s.set_authoring_context(AuthoringContext::default());
    assert!(s.reason_with_money(RAW, false).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// Work the deterministic reader knows, on this project's two files.
const WORK: &str =
    "Je veux que sortie.txt contienne exactement les octets présents dans entree.txt.";

/// The native candidate the loopback answers: the copy fixture on this project's paths.
fn native() -> String {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/compile/copy-fr.json"))
            .expect("fixture");
    let candidate = fixture["candidate"]
        .as_str()
        .expect("candidate")
        .replace("./notes/brief.md", "./entree.txt")
        .replace("./out/copie.md", "./sortie.txt");
    serde_json::json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "copy"})
        .to_string()
}

#[test]
fn details_says_the_named_effort_as_the_receipt_recorded_it_and_nothing_without_one() {
    for word in [Some("max"), None] {
        let peer = Peer::start(vec![(200, response(&native()))]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().expect("root");
        std::fs::write(dir.path().join("entree.txt"), "A\n").expect("input");
        let mut s = open(dir.path(), &Arc::new(AtomicUsize::new(0)));
        let settings = AuthoringSettings::none().with_strategy("only");
        let settings = match word {
            Some(word) => settings.with_reasoning(word),
            None => settings,
        };
        s.set_authoring_context(AuthoringContext::from_settings(
            &settings,
            &AuthoringSettings::none(),
        ));
        let out = s.turn(&format!("{WORK} budget 2 USD."));
        assert!(matches!(out, TurnOutcome::Proposal { .. }), "{out:?}");
        let bodies = peer.bodies();
        assert_eq!(bodies.len(), 1, "one native call");
        let details = s.details();
        if word.is_some() {
            asks_max(&bodies[0]);
            assert!(
                details.contains("call · reasoning effort max configured · keys read back from the sent body: thinking enabled · effort max · effort served unknown · reasoning tokens not reported · usage 100 in / 20 out tokens · response model deepseek-v4-pro"),
                "{details}"
            );
        } else {
            asks_none(&bodies[0]);
            assert!(!details.contains("reasoning effort"), "{details}");
        }
        assert!(
            details.contains("\n  cost: the compiler meters tokens, not money · a run's cost is in its result and `/proof`"),
            "{details}"
        );
    }
}

#[test]
fn a_named_word_the_session_cannot_ask_refuses_every_call_before_any_byte() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let built = Arc::new(AtomicUsize::new(0));
    let mut s = open(dir.path(), &built);
    s.admit_money("budget 2 USD", false).expect("admit");
    // The shared parser refuses the word; a paid account stands, and nothing may be asked.
    s.set_authoring_context(named("maximum"));
    assert!(s.authoring_context().refusal().is_some());
    let refused = |why: &str| why.contains("`maximum` is not a reasoning effort");
    let label = s.classify(SessionPhase::Idle, "tell me more about it");
    assert_eq!(label.method, RoutingMethod::Failed, "{label:?}");
    assert!(
        label
            .note
            .as_deref()
            .is_some_and(|note| refused(note) && note.contains("nothing was sent")),
        "{label:?}"
    );
    let turn = s.reason_with_money(RAW, false);
    assert!(
        matches!(&turn, Err(ReasonError::Provider(why)) if refused(why)),
        "{turn:?}"
    );
    assert!(peer.bodies().is_empty(), "{:?}", peer.bodies());
    assert_eq!(built.load(Ordering::SeqCst), 0, "no reasoner was built");
    // Through the door: an open line and a greeting send nothing either, and say why.
    for line in ["tell me more about it", "hello"] {
        let outcome = s.turn(line);
        assert!(refused(&format!("{outcome:?}")), "{line}: {outcome:?}");
    }
    assert!(peer.bodies().is_empty(), "{:?}", peer.bodies());
    assert_eq!(built.load(Ordering::SeqCst), 0, "no reasoner was built");
    // Authoring refuses the context whole, before any byte, as it did.
    let compiled = s.compile_round(&crate::authoring::AuthoringRound::new(RAW), &s.seat);
    assert!(
        matches!(&compiled, Err(crate::authoring::AuthoringError::Context(_))),
        "{compiled:?}"
    );
    assert!(peer.bodies().is_empty(), "{:?}", peer.bodies());
    let receipt = s.inference_receipt().expect("receipt").expect("account");
    assert!(receipt.attempts.is_empty(), "the allowance is untouched");
}

#[test]
fn another_refused_setting_never_drops_a_valid_named_level() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let mut s = open(dir.path(), &Arc::new(AtomicUsize::new(0)));
    s.admit_money("budget 2 USD", false).expect("admit");
    // The strategy word is refused (authoring refuses it); the named level is what calls ask.
    s.set_authoring_context(AuthoringContext::from_settings(
        &AuthoringSettings::none()
            .with_strategy("sometimes")
            .with_reasoning("max"),
        &AuthoringSettings::none(),
    ));
    assert!(s.authoring_context().refusal().is_some());
    assert_eq!(
        s.authoring_context().reasoning(),
        Some(AuthoringReasoning::Max)
    );
    // Its own refusal: authoring refuses the refused strategy whole, before any byte.
    let compiled = s.compile_round(&crate::authoring::AuthoringRound::new(RAW), &s.seat);
    assert!(
        matches!(&compiled, Err(crate::authoring::AuthoringError::Context(_))),
        "{compiled:?}"
    );
    assert!(peer.bodies().is_empty(), "{:?}", peer.bodies());
    // Its propagation: the label and the turn ask the valid named level, never none.
    assert_eq!(
        s.classify(SessionPhase::Idle, "tell me more about it")
            .method,
        RoutingMethod::Model
    );
    s.reason_with_money(RAW, false).expect("the turn");
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 2, "one label and one turn");
    bodies.iter().for_each(asks_max);
}
