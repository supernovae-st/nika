// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The explicit reasoning effort a Session names: resolved once by the shared
//! parser, kept in the context (bound into its identity only when named), said at `/status`, and
//! asked on every provider-seated authoring and repair call, as the exact bytes the provider
//! client dispatched show on a direct `DeepSeek` route the catalog qualifies. Loopback mechanics
//! through the cfg(test) HTTP substitution: nothing leaves the machine.

use nika_cli_host::compile::config::{AuthoringSettings, ConfigError};
use nika_onboard::compile::{AuthoringReasoning, CompileOutcome, CompileRequest};
use nika_onboard::knowledge::pin::KnowledgePin;
use nika_providers::InferenceAdmission;
use serde_json::{Value, json};

use super::{AuthoringContext, AuthoringContextError};
use crate::authoring::{
    AuthoringError, AuthoringRound, AuthoringSeat, compile_in, compile_in_with_admission,
};
use crate::reasoner::test_transport;
use crate::runtime::inference_tests::wire::{Peer, response};

/// A route whose catalog lists the three levels (`low` · `high` · `max`).
const QUALIFIED: &str = "deepseek/deepseek-v4-pro";
/// The exact Flash ID, whose catalog lists the three levels of its own (CALIBRATION-01).
const FLASH: &str = "deepseek/deepseek-flash";
/// A route that lists no level of its own (a suffixed Flash name the catalog never qualifies): an
/// explicit level is refused there.
const UNLISTED: &str = "deepseek/deepseek-flash-0731";
const REQUEST: &str = "Read ./orders.csv, keep the rows whose status is paid, write them to ./paid.csv with the same header and write their total amount as a number to ./paid-total.txt.";
const ORDERS: &str = "id,date,customer,status,amount,currency,ref\n1,2026-09-01,Acme,paid,120.50,EUR,A-1\n2,2026-09-02,Bolt,due,80,EUR,A-2\n";

/// The loopback's answer `text` on `model`'s route, naming that route's own model as the provider
/// does (a Flash call answered as Pro would be contradictory usage, rightly refused).
fn answer(model: &str, text: &str) -> Value {
    let mut answer = response(text);
    answer["model"] = Value::from(model.rsplit('/').next().unwrap_or(model));
    answer
}

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
    let peer = Peer::start(vec![(200, answer(model, "{}"))]);
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
fn a_context_naming_no_effort_hashes_its_derived_form_and_the_default_names_its_pin() {
    // The derived form every question identity and unknown-cost binding hashes, with no effort
    // field while none is named: a context reading no knowledge names none, and the default names
    // the embedded release it pinned (its identity changed when the release was embedded).
    let off = host(&AuthoringSettings::none().with_knowledge_off());
    assert_eq!(
        format!("{off:?}"),
        "AuthoringContext { strategy: Escalate, knowledge: None, refusal: None, source: \"host\", decision: None, project: None }"
    );
    let pin = KnowledgePin::embedded(None).expect("this build's embedded release");
    assert_eq!(
        format!("{:?}", AuthoringContext::default()),
        format!(
            "AuthoringContext {{ strategy: Escalate, knowledge: Some({pin:?}), refusal: None, source: \"default\", decision: None, project: None }}"
        )
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
            .with_strategy("sketch")
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
            .with_strategy("sketch")
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

/// A body asks `low` as the exact Flash model's two keys, on that model, and carries the request
/// whole.
fn asks_flash_low_for_the_whole_request(body: &Value) {
    assert_eq!(body["model"], "deepseek-flash", "{body}");
    assert_eq!(body["thinking"]["type"], "enabled", "{body}");
    assert_eq!(body["reasoning_effort"], "low", "{body}");
    assert!(
        body.to_string().contains(REQUEST),
        "the request whole: {body}"
    );
}

/// CALIBRATION-01 · every seated authoring and repair call on the exact Flash ID asks `low`, under
/// the ceilings and in the number of calls of the same round naming none; each call's record says
/// configured `low`, the keys read back from the bytes sent, and the effort served unknown.
#[test]
fn flash_low_rides_every_authoring_and_repair_call_with_the_same_caps() {
    let context = host(&AuthoringSettings::none().with_reasoning("low"));
    let (out, bodies) = dispatched(&context, FLASH, &AuthoringRound::new(REQUEST));
    let calls = recorded(&out.expect("an outcome"));
    assert!(
        bodies.len() >= 2 && bodies.len() == calls.len(),
        "a first call and its repair, each recorded: {} bodies · {calls:?}",
        bodies.len()
    );
    bodies.iter().for_each(asks_flash_low_for_the_whole_request);
    for (call, reasoning) in &calls {
        assert_eq!(reasoning["configured"], "low", "{call}");
        assert_eq!(
            reasoning["transmitted"],
            json!({"thinking": "enabled", "effort": "low"}),
            "{call}: the keys read back from the bytes sent"
        );
        assert_eq!(reasoning["served"], "unknown", "{call}");
    }
    assert!(
        calls.iter().any(|(call, _)| call.contains("repair")),
        "a repair call is covered: {calls:?}"
    );
    let (_, before) = dispatched(
        &AuthoringContext::default(),
        FLASH,
        &AuthoringRound::new(REQUEST),
    );
    let caps = |bodies: &[Value]| {
        bodies
            .iter()
            .map(|b| b["max_tokens"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        caps(&bodies),
        caps(&before),
        "the same calls under the same ceilings"
    );
}

/// CALIBRATION-01 · on the exact Flash ID, the sketch door (the explicit semantic door that
/// authors the structure) and the round that answers its question ask `low` with the request and
/// the project observed for it.
#[test]
fn flash_low_rides_the_native_door_and_the_next_round_with_their_context() {
    let context = host(
        &AuthoringSettings::none()
            .with_strategy("sketch")
            .with_reasoning("low"),
    );
    let (out, bodies) = dispatched(&context, FLASH, &AuthoringRound::new(REQUEST));
    let calls = recorded(&out.expect("an outcome"));
    assert!(
        !bodies.is_empty() && bodies.len() == calls.len(),
        "{calls:?}"
    );
    for body in &bodies {
        asks_flash_low_for_the_whole_request(body);
        carries_the_observation(body);
    }
    let mut round = AuthoringRound::new("Sort the payments.");
    round.answers.insert(
        "intent.clarification".to_owned(),
        serde_json::to_string(REQUEST).expect("literal"),
    );
    let (_, bodies) = dispatched(&context, FLASH, &round);
    assert!(!bodies.is_empty(), "the answer round asked the seat");
    for body in &bodies {
        asks_flash_low_for_the_whole_request(body);
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

fn wire_message<'a>(body: &'a Value, role: &str) -> &'a str {
    body["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|message| message["role"] == role)
        .expect("role")["content"]
        .as_str()
        .expect("text message")
}

/// The receipt identifies the document's instruction (the call the default route opens with);
/// classification calls do not read the pack.
fn opening_body<'a>(out: &CompileOutcome, bodies: &'a [Value]) -> &'a Value {
    let call = out
        .provenance
        .authoring
        .as_ref()
        .expect("authoring receipt")
        .context
        .iter()
        .find(|call| call["call"] == "document")
        .expect("a document authoring call");
    let digest = call["instruction_sha256"]
        .as_str()
        .expect("instruction digest");
    bodies
        .iter()
        .find(|body| {
            nika_event::source_id::sha256_hex(wire_message(body, "system").as_bytes()) == digest
        })
        .expect("the local server received that document instruction")
}

fn knowledge_record(out: &CompileOutcome) -> &Value {
    &out.provenance.decision.as_ref().expect("stamped")["session"]["authoring"]["knowledge"]
}

/// This provider's JSON-object route appends the answer schema to the document's opening (the
/// request among the door's fields). Validate both frames, including the schema's receipt,
/// rather than ignoring trailing text.
fn assert_opening_request(out: &CompileOutcome, body: &Value, intent: &str) {
    let separator = concat!(
        "\n\nReply with ONLY a JSON value that satisfies this JSON Schema, no prose, no code ",
        "fences. Every property that lists an enum takes exactly one of the listed values, ",
        "spelled as listed; every required property is present; no property outside the ",
        "schema:\n",
    );
    let (opening, schema_text) = wire_message(body, "user")
        .split_once(separator)
        .expect("the JSON-object provider's complete schema instruction");
    let opening: Value = serde_json::from_str(opening).expect("the door's opening fields");
    assert_eq!(
        opening["request"], intent,
        "the original intention is retained whole"
    );
    let schema: Value = serde_json::from_str(schema_text).expect("complete schema JSON");
    assert!(schema.is_object());
    assert_eq!(body["response_format"]["type"], "json_object");
    let call = out
        .provenance
        .authoring
        .as_ref()
        .expect("authoring receipt")
        .context
        .iter()
        .find(|call| call["call"] == "document")
        .expect("document call");
    assert_eq!(
        call["schema_sha256"],
        nika_event::source_id::sha256_hex(schema_text.as_bytes())
    );
}

/// A default session presents the recalled material in the instruction of its document call,
/// the semantic call that opens the round. The off control sends the same intention without those
/// references. A canned answer proves delivery only.
#[test]
fn a_default_session_presents_recalled_references_in_the_native_instruction() {
    let intent = format!(
        "{REQUEST} Declare typed workflow inputs and outputs, and expose the total amount as a number output."
    );
    let context = AuthoringContext::default();
    let pack = context
        .compose(&intent)
        .expect("admitted")
        .expect("composed");
    let expected: Vec<_> = [
        ("pattern", "pattern:typed-output"),
        ("block", "block:typed-inputs-outputs"),
    ]
    .into_iter()
    .map(|(kind, id)| {
        pack.references
            .iter()
            .find(|r| r.kind == kind && r.id == id)
            .expect("expected reference recalled")
    })
    .collect();
    let round = AuthoringRound::new(intent.clone());
    let (out, bodies) = dispatched(&context, QUALIFIED, &round);
    let out = out.expect("an outcome");
    let record = knowledge_record(&out);
    assert_eq!(record["identity"]["source"], "embedded");
    assert_eq!(
        record["identity"]["snapshot_sha256"],
        "b7f3861c55c785ba78fbf3fcfbb495ab79154b30f1bcb8483ce66018cc4659a9"
    );
    assert!(record["identity"].get("dir").is_none());
    assert_eq!(record["presented"], true);
    assert_eq!(record["pack_sha256"], pack.identity["door"]["pack_sha256"]);
    let body = opening_body(&out, &bodies);
    let system = wire_message(body, "system");
    let digest = nika_event::source_id::sha256_hex(system.as_bytes());
    assert!(
        record["calls"]
            .as_array()
            .expect("calls")
            .iter()
            .any(|call| { call["call"] == "document" && call["instruction_sha256"] == digest })
    );
    let off = host(&AuthoringSettings::none().with_knowledge_off());
    let (unread, unread_bodies) = dispatched(&off, QUALIFIED, &round);
    let unread = unread.expect("off outcome");
    assert!(
        knowledge_record(&unread).is_null(),
        "knowledge off attaches nothing"
    );
    let off_body = opening_body(&unread, &unread_bodies);
    for reference in expected {
        assert!(
            system.contains(&reference.text),
            "the document's instruction carries {}",
            reference.id
        );
        assert!(!wire_message(off_body, "system").contains(&reference.text));
        assert!(
            record["references"]
                .as_array()
                .expect("references")
                .iter()
                .any(|r| {
                    r["kind"] == reference.kind
                        && r["id"] == reference.id
                        && r["bytes"] == reference.text.len()
                        && r["sha256"]
                            == nika_event::source_id::sha256_hex(reference.text.as_bytes())
                })
        );
    }
    assert_opening_request(&out, body, &intent);
    assert_opening_request(&unread, off_body, &intent);
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
        transport: nika_types::access::HarnessTransport::Native,
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

/// Source recovery rides the context as resolved: absent or zero, the identity bytes every
/// question and cost binding hashes carry nothing new; named, they carry it; a word the parser
/// refuses is the context's refusal, never a silent zero.
#[test]
fn source_recovery_binds_the_identity_only_when_named() {
    let typed = |word: &str| {
        let mut settings = AuthoringSettings::none();
        settings.source_recovery = Some(word.to_owned());
        AuthoringContext::from_settings(&settings, &AuthoringSettings::none())
    };
    let plain =
        AuthoringContext::from_settings(&AuthoringSettings::none(), &AuthoringSettings::none());
    let (zero, two) = (typed("0"), typed("2"));
    assert_eq!((plain.recovery, zero.recovery, two.recovery), (0, 0, 2));
    assert!(
        !format!("{plain:?}").contains("source_recovery"),
        "{plain:?}"
    );
    assert!(!format!("{zero:?}").contains("source_recovery"), "{zero:?}");
    assert!(format!("{two:?}").contains("source_recovery: 2"), "{two:?}");
    let nine = typed("9");
    assert_eq!(nine.recovery, 9);
    assert!(nine.refusal().is_none());
    let refused = typed("4294967296");
    let error = AuthoringContextError::Config(ConfigError::SourceRecovery("4294967296".into()));
    assert_eq!(refused.refusal(), Some(&error));
}
