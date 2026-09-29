// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The explicit reasoning effort a Session names (R4 B16 · C11): resolved once by the shared
//! parser, kept in the context (bound into its identity only when named), said at `/status`, and
//! asked on every provider-seated authoring and repair call, as the exact bytes the provider
//! client dispatched show on a direct `DeepSeek` route the catalog qualifies. Loopback mechanics
//! through the cfg(test) HTTP substitution: nothing leaves the machine.

use nika_cli_host::compile::config::{AuthoringSettings, ConfigError};
use nika_onboard::compile::{AuthoringReasoning, CompileOutcome, CompileRequest};
use nika_providers::InferenceAdmission;
use serde_json::{Value, json};

use super::{AuthoringContext, AuthoringContextError};
use crate::authoring::{
    AuthoringError, AuthoringRound, AuthoringSeat, compile_in, compile_in_with_admission,
};
use crate::reasoner::test_transport;
use crate::runtime::inference_tests::wire::{Peer, response};

/// The one route whose catalog lists the three levels (`low` · `high` · `max`).
const QUALIFIED: &str = "deepseek/deepseek-v4-pro";
/// A route that asks efforts but lists no level of its own: an explicit level is refused there.
const UNLISTED: &str = "deepseek/deepseek-flash";
const REQUEST: &str = "Read ./orders.csv, keep the rows whose status is paid, write them to ./paid.csv with the same header and write their total amount as a number to ./paid-total.txt.";
const ORDERS: &str = "id,date,customer,status,amount,currency,ref\n1,2026-09-01,Acme,paid,120.50,EUR,A-1\n2,2026-09-02,Bolt,due,80,EUR,A-2\n";

/// A context a host names: its own words over an empty environment.
fn host(settings: &AuthoringSettings) -> AuthoringContext {
    AuthoringContext::from_settings(settings, &AuthoringSettings::none())
}

/// Every body the provider client sent while `round` compiled on `model` under `context`, as the
/// loopback received them, beside the outcome (or the refusal).
fn dispatched(
    context: &AuthoringContext,
    model: &str,
    round: &AuthoringRound,
) -> (Result<CompileOutcome, AuthoringError>, Vec<Value>) {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    let dir = tempfile::tempdir().expect("project");
    std::fs::write(dir.path().join("orders.csv"), ORDERS).expect("fixture");
    let seat = AuthoringSeat::Provider {
        model: model.to_owned(),
    };
    let context = context.clone().with_project_root(dir.path());
    let out = if round.answers.is_empty() {
        compile_in_with_admission(
            &seat,
            &context,
            &CompileRequest::create(round.intent.clone()),
            &round.intent,
            &InferenceAdmission::unbudgeted(),
        )
    } else {
        round.compile_with_admission(&seat, &context, &InferenceAdmission::unbudgeted())
    };
    (out, peer.bodies())
}

/// The reasoning each authoring call of an outcome recorded, by its call name.
fn recorded(out: &CompileOutcome) -> Vec<(String, Value)> {
    out.provenance
        .authoring
        .as_ref()
        .map_or_else(Vec::new, |receipt| {
            receipt
                .context
                .iter()
                .map(|call| {
                    let name = call["call"].as_str().unwrap_or("?").to_owned();
                    (name, call["reasoning"].clone())
                })
                .collect()
        })
}

/// A body asks the named effort, as the qualified route's two keys, and carries the request whole.
fn asks_max_for_the_whole_request(body: &Value) {
    assert_eq!(body["thinking"]["type"], "enabled", "{body}");
    assert_eq!(body["reasoning_effort"], "max", "{body}");
    assert!(
        body.to_string().contains(REQUEST),
        "the request whole: {body}"
    );
}

/// A body carries the project observed for the request (every column, the unworded ones too).
fn carries_the_observation(body: &Value) {
    let text = body.to_string();
    assert!(
        text.contains("observed_world") && text.contains("customer"),
        "the observation: {text}"
    );
}

#[test]
fn a_context_naming_no_effort_keeps_every_identity_it_hashed() {
    // The derived form every question identity and unknown-cost binding hashed before C11.
    assert_eq!(
        format!("{:?}", AuthoringContext::default()),
        "AuthoringContext { strategy: Escalate, knowledge: None, refusal: None, source: \"default\", decision: None, project: None }"
    );
    let named = format!(
        "{:?}",
        host(&AuthoringSettings::none().with_reasoning("max"))
    );
    assert!(named.ends_with(", reasoning: Max }"), "{named}");
    let high = format!(
        "{:?}",
        host(&AuthoringSettings::none().with_reasoning("high"))
    );
    assert_ne!(named, high, "another level is another identity");
}

#[test]
fn the_named_effort_is_resolved_once_by_the_shared_parser_and_said_at_status() {
    let named = host(&AuthoringSettings::none().with_reasoning("max"));
    assert_eq!(
        (named.reasoning(), named.source()),
        (Some(AuthoringReasoning::Max), "host")
    );
    // Scoped to the LLM calls (B19); with no decision seat named, no word of one.
    assert!(
        named
            .line()
            .contains(" · reasoning effort max asked of every LLM call")
            && !named.line().contains("TypeSafe"),
        "{}",
        named.line()
    );
    let env = AuthoringContext::from_settings(
        &AuthoringSettings::none(),
        &AuthoringSettings::none().with_reasoning("high"),
    );
    assert_eq!(
        (env.reasoning(), env.source()),
        (Some(AuthoringReasoning::High), "environment")
    );
    // The door's word outranks the environment's and never falls back to it.
    let both = AuthoringContext::from_settings(
        &AuthoringSettings::none().with_reasoning("max"),
        &AuthoringSettings::none().with_reasoning("low"),
    );
    assert_eq!(both.reasoning(), Some(AuthoringReasoning::Max));
    // Any other word is refused, never read as a lower level.
    for word in ["medium", "MAX", "maximum"] {
        let refused = host(&AuthoringSettings::none().with_reasoning(word));
        assert_eq!(refused.reasoning(), None, "{word}");
        assert!(
            matches!(
                refused.refusal(),
                Some(AuthoringContextError::Config(ConfigError::UnknownReasoning(w))) if w == word
            ),
            "{word}: {:?}",
            refused.refusal()
        );
    }
    let legacy = AuthoringContext::default();
    assert_eq!(legacy.reasoning(), None);
    assert!(!legacy.line().contains("reasoning"), "{}", legacy.line());
}

#[test]
fn every_seated_authoring_and_repair_call_sends_the_named_effort() {
    let context = host(&AuthoringSettings::none().with_reasoning("max"));
    let (out, bodies) = dispatched(&context, QUALIFIED, &AuthoringRound::new(REQUEST));
    let out = out.expect("an outcome");
    let calls = recorded(&out);
    assert!(
        bodies.len() >= 2 && bodies.len() == calls.len(),
        "a first call and its repair, each recorded: {} bodies · {calls:?}",
        bodies.len()
    );
    // The private plan reads the request alone; the observation rides the native door's calls.
    bodies.iter().for_each(asks_max_for_the_whole_request);
    for (call, reasoning) in &calls {
        assert_eq!(reasoning["configured"], "max", "{call}");
        assert_eq!(
            reasoning["transmitted"],
            json!({"thinking": "enabled", "effort": "max"}),
            "{call}: the keys read back from the bytes sent"
        );
        assert_eq!(reasoning["served"], "unknown", "{call}");
    }
    assert!(
        calls.iter().any(|(call, _)| call.contains("repair")),
        "a repair call is covered: {calls:?}"
    );
}

#[test]
fn the_native_door_sends_the_named_effort_with_the_request_and_its_observation() {
    let context = host(
        &AuthoringSettings::none()
            .with_strategy("only")
            .with_reasoning("max"),
    );
    let (out, bodies) = dispatched(&context, QUALIFIED, &AuthoringRound::new(REQUEST));
    let calls = recorded(&out.expect("an outcome"));
    assert!(
        !bodies.is_empty() && bodies.len() == calls.len(),
        "{calls:?}"
    );
    for body in &bodies {
        asks_max_for_the_whole_request(body);
        carries_the_observation(body);
    }
}

#[test]
fn a_later_round_sends_its_answers_and_the_named_effort() {
    let context = host(
        &AuthoringSettings::none()
            .with_strategy("only")
            .with_reasoning("max"),
    );
    // The first words named no file; the answered clarification replaces the request.
    let mut round = AuthoringRound::new("Sort the payments.");
    round.answers.insert(
        "intent.clarification".to_owned(),
        serde_json::to_string(REQUEST).expect("literal"),
    );
    let (_, bodies) = dispatched(&context, QUALIFIED, &round);
    assert!(!bodies.is_empty(), "the answer round asked the seat");
    for body in &bodies {
        asks_max_for_the_whole_request(body);
        carries_the_observation(body);
    }
}

#[test]
fn a_route_whose_catalog_lists_no_level_refuses_the_effort_before_any_byte() {
    let context = host(&AuthoringSettings::none().with_reasoning("max"));
    let (out, bodies) = dispatched(&context, UNLISTED, &AuthoringRound::new(REQUEST));
    assert!(bodies.is_empty(), "nothing was sent: {bodies:?}");
    if let Ok(out) = out {
        for (call, reasoning) in recorded(&out) {
            assert_eq!(reasoning["configured"], "max", "{call}");
            assert_eq!(reasoning["transmitted"], "unobserved", "{call}");
        }
    }
}

#[test]
fn a_session_naming_no_effort_sends_what_it_sent_before() {
    let (out, bodies) = dispatched(
        &AuthoringContext::default(),
        QUALIFIED,
        &AuthoringRound::new(REQUEST),
    );
    assert!(!bodies.is_empty(), "the seat was asked");
    for body in &bodies {
        assert!(body.get("thinking").is_none(), "{body}");
        assert_ne!(body["reasoning_effort"], "max", "{body}");
    }
    for (call, reasoning) in recorded(&out.expect("an outcome")) {
        assert_eq!(reasoning["configured"], Value::Null, "{call}");
    }
}

#[test]
fn a_subscription_seat_refuses_a_named_effort_before_any_call() {
    let peer = Peer::start(vec![(200, response("{}"))]);
    let _transport = test_transport::install(&peer.url);
    // An adapter no host provides: were the refusal gone, meeting it fails here, never a real
    // subscription process.
    let seat = AuthoringSeat::Harness {
        seat: "no-such-harness".to_owned(),
        model: None,
    };
    let context = host(&AuthoringSettings::none().with_reasoning("max"));
    let refused = compile_in(&seat, &context, &CompileRequest::create(REQUEST), REQUEST);
    assert!(
        matches!(&refused, Err(AuthoringError::Seat(why))
            if why.contains("cannot carry the explicit reasoning effort `max`")
                && why.contains("nothing was sent")),
        "{refused:?}"
    );
    assert!(peer.bodies().is_empty(), "no call was made");
}
