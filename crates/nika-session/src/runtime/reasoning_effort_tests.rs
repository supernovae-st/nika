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
use crate::runtime::inference_tests::JUDGE_APPROVES;
use crate::runtime::inference_tests::wire::{Peer, response};
use crate::turn::{
    ConservativeFallback, ReasonerClassifier, RoutingMethod, SessionPhase, TurnAct, TurnClassifier,
    TurnContext, TurnDecision,
};

/// A route whose catalog lists the three levels.
const QUALIFIED: &str = "deepseek/deepseek-v4-pro";
/// The exact Flash ID, whose catalog lists low, high and max of its own (CALIBRATION-01).
const FLASH: &str = "deepseek/deepseek-flash";
/// A route that lists no level of its own (a suffixed Flash name the catalog never qualifies): an
/// explicit level is refused there.
const UNLISTED: &str = "deepseek/deepseek-flash-0731";
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
    open_on(root, built, QUALIFIED)
}

/// A session on `model`, opened naming no level, whose factory counts the fresh reasoners it
/// builds on that same model.
fn open_on(root: &Path, built: &Arc<AtomicUsize>, model: &'static str) -> SessionRuntime {
    let selected = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Api {
            provider: "deepseek".into(),
        },
        model: Some(model.into()),
        locus: DataLocus::Metered {
            provider: "deepseek".into(),
        },
        ready: true,
        why: None,
    };
    let mut s = SessionRuntime::open(root, selected, Box::new(seat(model)));
    let count = Arc::clone(built);
    s.factory = Some(Box::new(move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        Box::new(seat(model))
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

/// The loopback's answer `text` on the exact Flash route, naming that model as the provider does
/// (a Flash call answered as Pro would be contradictory usage, rightly refused).
fn flash(text: &str) -> Value {
    let mut answer = response(text);
    answer["model"] = Value::from("deepseek-flash");
    answer
}

/// The body asks `low` as the exact Flash model's two keys, on that model.
fn asks_flash_low(body: &Value) {
    assert_eq!(body["model"], "deepseek-flash", "{body}");
    assert_eq!(body["thinking"]["type"], "enabled", "{body}");
    assert_eq!(body["reasoning_effort"], "low", "{body}");
}

/// The body without the two effort keys.
fn without_effort(body: &Value) -> Value {
    let mut body = body.clone();
    if let Some(keys) = body.as_object_mut() {
        keys.remove("thinking");
        keys.remove("reasoning_effort");
    }
    body
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
    s.admit_money("budget 2 USD", false, false).expect("admit");
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
    label_body_on(QUALIFIED, effort)
}

/// [`label_body`] on `model`.
fn label_body_on(model: &str, effort: Option<AuthoringReasoning>) -> Value {
    let mut answer = response("DISCUSS");
    answer["model"] = Value::from(model.rsplit('/').next().unwrap_or(model));
    let peer = Peer::start(vec![(200, answer)]);
    let _transport = test_transport::install(&peer.url);
    let decision = ReasonerClassifier::new(Box::new(seat(model)))
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

/// CALIBRATION-01 · the session's routed labels and conversational turn on the exact Flash ID ask
/// `low` as two keys on the same calls: the same model, the label and turn ceilings (4096 and
/// 8192), the same words and the same number of calls; naming none again sends the bytes it sent
/// before, the keys alone removed.
#[test]
fn flash_low_rides_every_routed_label_and_turn_with_the_same_ceilings_and_words() {
    let peer = Peer::start(vec![(200, flash("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let built = Arc::new(AtomicUsize::new(0));
    let mut s = open_on(dir.path(), &built, FLASH);
    s.admit_money("budget 2 USD", false, false).expect("admit");
    s.set_authoring_context(named("low"));
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
    bodies.iter().for_each(asks_flash_low);
    assert!(bodies[1].to_string().contains("and then?"), "{}", bodies[1]);
    assert!(bodies[2].to_string().contains("Keep my exact words."));
    assert_eq!(
        (
            &bodies[0]["max_tokens"],
            &bodies[1]["max_tokens"],
            &bodies[2]["max_tokens"]
        ),
        (&Value::from(4096), &Value::from(4096), &Value::from(8192)),
        "the label and turn ceilings, unchanged by the level"
    );
    s.set_authoring_context(AuthoringContext::default());
    s.classify(SessionPhase::Idle, "one more");
    s.reason_with_money("and the turn", false)
        .expect("the turn");
    let bodies = peer.bodies();
    assert_eq!((bodies.len(), built.load(Ordering::SeqCst)), (5, 3));
    bodies[3..].iter().for_each(asks_none);
    // One label alone, asking low then none: the same call, the two keys apart.
    let asked = label_body_on(FLASH, Some(AuthoringReasoning::Low));
    asks_flash_low(&asked);
    let before = label_body_on(FLASH, None);
    asks_none(&before);
    assert_eq!(before["max_tokens"], 4096, "{before}");
    assert_eq!(
        without_effort(&asked),
        before,
        "the same ceiling, temperature and words"
    );
}

/// CALIBRATION-01 · the authoring call and its current judgment on the exact Flash ID, through the
/// Session's own turn: each asks `low`, the ceilings, the number of calls and the allowance's
/// attempts are those of the same turn naming none, and `/details` says configured `low`, the keys
/// read back from the bytes sent, and the effort served unknown.
#[test]
fn flash_low_rides_the_authoring_and_judgment_calls_with_the_same_caps_and_accounting() {
    let mut seen = Vec::new();
    for word in [Some("low"), None] {
        let peer = Peer::start(vec![(200, flash(&native())), (200, flash(JUDGE_APPROVES))]);
        let _transport = test_transport::install(&peer.url);
        let dir = tempfile::tempdir().expect("root");
        std::fs::write(dir.path().join("entree.txt"), "A\n").expect("input");
        let mut s = open_on(dir.path(), &Arc::new(AtomicUsize::new(0)), FLASH);
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
        assert_eq!(bodies.len(), 2, "one authoring call, then the judgment's");
        let details = s.details();
        if word.is_some() {
            bodies.iter().for_each(asks_flash_low);
            assert!(
                details.contains("call · reasoning effort low configured · keys read back from the sent body: thinking enabled · effort low · effort served unknown"),
                "{details}"
            );
        } else {
            assert!(
                bodies.iter().all(|b| b.get("thinking").is_none()),
                "{bodies:?}"
            );
            assert!(!details.contains("reasoning effort"), "{details}");
        }
        // The calls the reading recorded, by role: one authoring call, then one `judge_request`.
        let outcome = s.last_outcome.as_ref().expect("the reading");
        let calls: Vec<(String, Value)> = (outcome.provenance.authoring.as_ref())
            .map(|receipt| {
                (receipt.context.iter())
                    .map(|call| {
                        (
                            call["call"].as_str().unwrap_or("?").to_owned(),
                            call["reasoning"].clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let roles: Vec<&str> = calls.iter().map(|(call, _)| call.as_str()).collect();
        assert_eq!(roles, ["native", "judge_request"], "{calls:?}");
        for (call, reasoning) in &calls {
            if word.is_some() {
                assert_eq!(reasoning["configured"], "low", "{call}");
                assert_eq!(
                    reasoning["transmitted"],
                    serde_json::json!({"thinking": "enabled", "effort": "low"}),
                    "{call}: the keys read back from the bytes sent"
                );
                assert_eq!(reasoning["served"], "unknown", "{call}");
            } else {
                assert_eq!(reasoning["configured"], Value::Null, "{call}");
            }
        }
        let receipt = s.inference_receipt().expect("receipt").expect("account");
        let caps: Vec<Value> = bodies.iter().map(|b| b["max_tokens"].clone()).collect();
        seen.push((
            roles.iter().map(|r| (*r).to_owned()).collect::<Vec<_>>(),
            caps,
            receipt.attempts.len(),
        ));
    }
    assert_eq!(
        seen[0], seen[1],
        "the same roles, ceilings and accounting, the level apart"
    );
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
        let peer = Peer::start(vec![
            (200, response(&native())),
            (200, response(JUDGE_APPROVES)),
        ]);
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
        assert_eq!(bodies.len(), 2, "one native call, then the judge's");
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
    s.admit_money("budget 2 USD", false, false).expect("admit");
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
    let compiled = s.compile_round(&crate::authoring::AuthoringRound::new(RAW), &s.seat.clone());
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
    s.admit_money("budget 2 USD", false, false).expect("admit");
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
    let compiled = s.compile_round(&crate::authoring::AuthoringRound::new(RAW), &s.seat.clone());
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

/// A door's own classifier with no reasoning hook, counting every label it would send.
struct Door(Arc<AtomicUsize>);

impl TurnClassifier for Door {
    fn classify_with_admission(
        &mut self,
        context: &TurnContext,
        raw: &str,
        _: &InferenceAdmission,
    ) -> TurnDecision {
        self.classify(context, raw)
    }

    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        self.0.fetch_add(1, Ordering::SeqCst);
        TurnDecision::new(TurnAct::Discuss, RoutingMethod::Model)
    }
}

/// A paid session on the qualified route whose door installed `classifier`.
fn door(
    root: &Path,
    built: &Arc<AtomicUsize>,
    classifier: Box<dyn TurnClassifier>,
) -> SessionRuntime {
    let mut s = open(root, built);
    s.admit_money("budget 2 USD", false, false).expect("admit");
    s.with_classifier(classifier);
    s
}

/// The label a line at rest gets.
fn label(s: &mut SessionRuntime) -> TurnDecision {
    s.classify(SessionPhase::Idle, "tell me more about it")
}

/// B19 F2 · the session's own classifier installed by a door asks the level the session holds on
/// every label, then none when none is named, as the routed one does.
#[test]
fn a_door_classifier_asks_the_level_the_session_holds_on_every_label() {
    let peer = Peer::start(vec![(200, response("DISCUSS")), (200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let built = Arc::new(AtomicUsize::new(0));
    let classifier = ReasonerClassifier::new(Box::new(seat(QUALIFIED)));
    let mut s = door(dir.path(), &built, Box::new(classifier));
    s.set_authoring_context(named("max"));
    assert_eq!(label(&mut s).method, RoutingMethod::Model);
    s.set_authoring_context(AuthoringContext::default());
    assert_eq!(label(&mut s).method, RoutingMethod::Model);
    let bodies = peer.bodies();
    assert_eq!(bodies.len(), 2, "two labels");
    asks_max(&bodies[0]);
    asks_none(&bodies[1]);
    assert_eq!(
        built.load(Ordering::SeqCst),
        0,
        "the door's classifier, no fresh reasoner"
    );
}

/// B19 F2 · a word the session cannot ask never reaches a door's classifier, and a level its route
/// cannot carry is refused by the provider: nothing is sent either way, and the route says why.
#[test]
fn a_door_classifier_sends_nothing_for_a_word_or_a_route_that_cannot_carry_the_level() {
    let peer = Peer::start(vec![(200, response("DISCUSS"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("root");
    let classifier = ReasonerClassifier::new(Box::new(seat(QUALIFIED)));
    let mut s = door(
        dir.path(),
        &Arc::new(AtomicUsize::new(0)),
        Box::new(classifier),
    );
    s.set_authoring_context(named("maximum"));
    let refused = label(&mut s);
    assert_eq!(refused.method, RoutingMethod::Failed, "{refused:?}");
    assert!(
        refused.note.as_deref().is_some_and(|note| {
            note.contains("`maximum` is not a reasoning effort")
                && note.contains("nothing was sent")
        }),
        "{refused:?}"
    );
    s.with_classifier(Box::new(ReasonerClassifier::new(Box::new(seat(UNLISTED)))));
    s.set_authoring_context(named("max"));
    assert_eq!(label(&mut s).method, RoutingMethod::Failed);
    assert!(peer.bodies().is_empty(), "{:?}", peer.bodies());
    let receipt = s.inference_receipt().expect("receipt").expect("account");
    assert!(receipt.attempts.is_empty(), "the allowance is untouched");
}

/// B19 F2 · a door's classifier with no way to carry a level is never called with one: the label
/// refuses before its call, and naming none calls it as before. The fallback makes no call, so it
/// has nothing to carry: it accepts the level.
#[test]
fn a_door_classifier_that_cannot_carry_the_level_is_never_called_with_it() {
    let dir = tempfile::tempdir().expect("root");
    let calls = Arc::new(AtomicUsize::new(0));
    let classifier = Door(Arc::clone(&calls));
    let mut s = door(
        dir.path(),
        &Arc::new(AtomicUsize::new(0)),
        Box::new(classifier),
    );
    s.set_authoring_context(named("max"));
    let refused = label(&mut s);
    assert_eq!(refused.method, RoutingMethod::Failed, "{refused:?}");
    assert!(
        refused.note.as_deref().is_some_and(|note| {
            note.contains("cannot carry the explicit reasoning effort `max`")
                && note.contains("nothing was sent")
        }),
        "{refused:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "never called with the level"
    );
    s.set_authoring_context(AuthoringContext::default());
    assert_eq!(label(&mut s).method, RoutingMethod::Model);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "called as before");
    // The refusal is the reasoner's typed error, never a bare string (FCI-019 · B19 review).
    let refused = Door(Arc::clone(&calls)).carry_effort(Some(AuthoringReasoning::Max));
    assert!(
        matches!(&refused, Err(ReasonError::Provider(why))
            if why.contains("cannot carry the explicit reasoning effort `max`")),
        "{refused:?}"
    );
    assert!(
        Door(calls).carry_effort(None).is_ok(),
        "no level: nothing to carry"
    );
    let mut fallback = ConservativeFallback;
    assert!(fallback.carry_effort(Some(AuthoringReasoning::Max)).is_ok());
}

/// B19 · the operator-selected `TypeSafe` decision seat (`Jev`) is a separate backend: a named
/// level rides the LLM calls only. The seat's request carries no effort (none is invented for
/// it), its receipt says the level does not apply, and `/status` scopes the level to the LLM
/// calls.
#[test]
fn the_decision_seat_is_a_separate_backend_the_named_level_never_reaches() {
    use crate::authoring::DecisionSetup;
    use crate::authoring::decision::tests::{KEY, Peer as SystemOne, Reply, SEAT, TICKETS, answer};
    let deepseek = Peer::start(vec![(200, response("unused"))]);
    let _transport = test_transport::install(&deepseek.url);
    let dir = tempfile::tempdir().expect("root");
    let tickets =
        r#"[{"id": "41", "title": "synthetic one"}, {"id": "42", "title": "synthetic two"}]"#;
    std::fs::write(dir.path().join("tickets.json"), tickets).expect("fixture");
    let jev = SystemOne::start(vec![Reply::Json(200, answer("lookup"))]);
    let mut s = open(dir.path(), &Arc::new(AtomicUsize::new(0)));
    let seat = DecisionSetup::with_key(SEAT, Some(KEY.to_owned()), Some(&jev.base));
    s.set_authoring_context(named("max").with_decision(Some(seat)));
    let out = s.turn(TICKETS);
    assert!(matches!(&out, TurnOutcome::Question { .. }), "{out:?}");
    let requests = jev.requests();
    assert_eq!(requests.len(), 1, "one decision call");
    let body = &requests[0].2;
    for key in ["thinking", "reasoning_effort", "reasoning", "effort"] {
        assert!(body.get(key).is_none(), "{key}: {body}");
    }
    assert!(!body.to_string().contains("\"max\""), "{body}");
    assert!(deepseek.bodies().is_empty(), "WARM settled it: no LLM call");
    let outcome = s.last_outcome.as_ref().expect("the reading");
    let decision = outcome
        .provenance
        .decision
        .as_ref()
        .expect("the decision record");
    assert_eq!(
        decision["session"]["decision_seat"]["reasoning_effort"],
        "not applicable · the named level `max` rides the LLM calls only; the TypeSafe request carries no effort",
        "{decision}"
    );
    let status = s.status();
    assert!(
        status.contains(" · reasoning effort max asked of every LLM call; the TypeSafe decision seat is a separate backend: no effort is sent to it"),
        "{status}"
    );
}
