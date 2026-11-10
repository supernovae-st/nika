// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scripted ACP peers for a session's offer: the exact requests and their
//! order, the offers read on the answer to the model selection, every case
//! where the model does not hold, and the proof on each dialogue that the
//! peer receives nothing after the model selection: no effort or tier
//! setter, no mode, no prompt.

use std::pin::Pin;
use std::sync::{Arc, Mutex};

use futures_core::Stream as _;
use nika_kernel::ai::harness::{HarnessError, HarnessEvent, HarnessRequest};
use nika_types::access::ServiceTier;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::offer::{EffortOffer, MAX_OFFERED_VALUES, ModelOffer, ServiceTierOffer, SessionOffer};

type PeerReader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type PeerWriter = tokio::io::WriteHalf<tokio::io::DuplexStream>;

/// The name codex-acp reports on `initialize`: the key of its measured rows.
const CODEX_ACP: &str = "@agentclientprotocol/codex-acp";

/// One line from the client, `None` at its EOF (the client is done).
async fn read(peer: &mut PeerReader) -> Option<Value> {
    let mut line = String::new();
    let size = tokio::time::timeout(std::time::Duration::from_secs(5), peer.read_line(&mut line))
        .await
        .expect("scripted dialogue must settle")
        .expect("read peer");
    (size != 0).then(|| serde_json::from_str(&line).expect("JSON request"))
}

async fn reply(peer: &mut PeerWriter, request: &Value, result: Value) {
    let mut bytes =
        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":request["id"],"result":result}))
            .expect("JSON");
    bytes.push(b'\n');
    peer.write_all(&bytes).await.expect("write peer");
    peer.flush().await.expect("flush peer");
}

fn select(id: &str, category: &str, current: &str, values: &[&str]) -> Value {
    json!({"id":id,"name":id,"category":category,"type":"select","currentValue":current,
        "options":values.iter().map(|v| json!({"value":v,"name":v})).collect::<Vec<_>>()})
}

/// The codex-acp shape: `mode`, `model`, `reasoning_effort` with `efforts`
/// and, only for a model that supports a fast tier, `fast-mode`.
fn codex(model: &str, effort: &str, efforts: &[&str], fast: Option<&str>) -> Value {
    let mut options = vec![
        select("mode", "mode", "agent", &["read-only", "agent"]),
        select("model", "model", model, &["gpt-5.4", "gpt-5.5"]),
        select("reasoning_effort", "thought_level", effort, efforts),
    ];
    options
        .extend(fast.map(|current| select("fast-mode", "model_config", current, &["off", "on"])));
    Value::Array(options)
}

/// Every method the peer received, in order: the witness each test reads.
type Seen = Arc<Mutex<Vec<String>>>;

fn note(seen: &Seen, request: &Value) {
    let method = request["method"].as_str().unwrap_or("?").to_owned();
    seen.lock().expect("seen").push(method);
}

/// Answer `initialize` as `agent_info` and `session/new` with `session`,
/// then answer each later request with the next of `answers`. A request
/// beyond them, or a prompt, is recorded and left unanswered.
async fn peer(
    mut r: PeerReader,
    mut w: PeerWriter,
    seen: Seen,
    agent_info: Option<Value>,
    session: Value,
    answers: Vec<Value>,
) {
    let init = read(&mut r).await.expect("initialize");
    note(&seen, &init);
    let mut answer = json!({"protocolVersion":1});
    if let Some(agent_info) = agent_info {
        answer["agentInfo"] = agent_info;
    }
    reply(&mut w, &init, answer).await;
    let new = read(&mut r).await.expect("session/new");
    note(&seen, &new);
    reply(&mut w, &new, session).await;
    let mut answers = answers.into_iter();
    while let Some(request) = read(&mut r).await {
        note(&seen, &request);
        seen.lock()
            .expect("seen")
            .push(request["params"].to_string());
        let Some(answer) = answers.next() else {
            continue;
        };
        reply(&mut w, &request, answer).await;
    }
}

/// Read the offer for `model` from a scripted peer; return it with every
/// method the peer received. The peer is awaited to the client's EOF, so
/// nothing the client sent is missed.
async fn offer_from(
    model: Option<&str>,
    agent_info: Option<Value>,
    session: Value,
    answers: Vec<Value>,
) -> (Result<SessionOffer, HarnessError>, Vec<String>) {
    let (ours, theirs) = tokio::io::duplex(256 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, peer_write) = tokio::io::split(theirs);
    let seen = Seen::default();
    let agent = tokio::spawn(peer(
        BufReader::new(peer_read),
        peer_write,
        Arc::clone(&seen),
        agent_info,
        session,
        answers,
    ));
    let mut request = HarnessRequest::new("this prompt must never be sent", "/tmp");
    if let Some(model) = model {
        request = request.with_requested_model(model);
    }
    let offer = super::read_offer(
        client_read,
        client_write,
        request,
        std::time::Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await;
    agent.await.expect("scripted peer completes");
    let seen = seen.lock().expect("seen").clone();
    (offer, seen)
}

fn measured() -> Value {
    json!({"name":CODEX_ACP,"version":"1.13.1"})
}

fn new_session(config: &Value) -> Value {
    json!({"sessionId":"s-offer","configOptions":config})
}

/// The whole reading on the measured adapter: the model is selected with the
/// payload a run sends, the efforts and the tiers are read on the answer to
/// that selection (the default model lists other efforts and no speed
/// option), and the peer receives nothing after it.
#[tokio::test]
async fn an_offer_selects_the_model_reads_what_it_lists_and_sends_nothing_after() {
    let (offer, seen) = offer_from(
        Some("openai/gpt-5.5"),
        Some(measured()),
        new_session(&codex("gpt-5.4", "medium", &["low", "medium"], None)),
        vec![json!({"configOptions": codex(
            "gpt-5.5",
            "medium",
            &["low", "medium", "high"],
            Some("off"),
        )})],
    )
    .await;
    let offer = offer.expect("an offer");
    assert_eq!(
        seen,
        [
            "initialize",
            "session/new",
            "session/set_config_option",
            r#"{"configId":"model","sessionId":"s-offer","value":"gpt-5.5"}"#,
        ],
        "one setter, the model, with the exact payload; no effort, tier, mode or prompt"
    );
    assert_eq!(offer.agent_name.as_deref(), Some(CODEX_ACP));
    assert_eq!(offer.agent_version.as_deref(), Some("1.13.1"));
    assert_eq!(offer.models, ["gpt-5.4", "gpt-5.5"]);
    assert_eq!(offer.models_omitted, 0);
    assert_eq!(
        offer.model,
        ModelOffer::Confirmed {
            native: "gpt-5.5".to_owned()
        }
    );
    assert_eq!(
        offer.effort,
        EffortOffer::Offered {
            option: "reasoning_effort".to_owned(),
            values: vec!["low".to_owned(), "medium".to_owned(), "high".to_owned()],
            omitted: 0,
            current: Some("medium".to_owned()),
        },
        "the efforts of the selected model, never the default model's"
    );
    assert_eq!(
        offer.service_tier,
        ServiceTierOffer::Offered {
            option: "fast-mode".to_owned(),
            tiers: vec![ServiceTier::Standard, ServiceTier::Fast],
            current: Some(ServiceTier::Standard),
        }
    );
}

/// A model the session does not list is the answer: nothing is selected, the
/// listed models are reported and what depends on the model stays unobserved
/// rather than read from the default model's configuration.
#[tokio::test]
async fn a_model_the_session_does_not_list_is_reported_and_nothing_is_selected() {
    let (offer, seen) = offer_from(
        Some("openai/gpt-6.1-sol"),
        Some(measured()),
        new_session(&codex("gpt-5.4", "medium", &["low", "medium"], Some("off"))),
        Vec::new(),
    )
    .await;
    let offer = offer.expect("an offer, not a refusal");
    assert_eq!(seen, ["initialize", "session/new"]);
    assert_eq!(offer.model, ModelOffer::NotOffered);
    assert_eq!(offer.models, ["gpt-5.4", "gpt-5.5"]);
    let EffortOffer::Unobserved { reason } = &offer.effort else {
        panic!(
            "the default model's efforts must not be reported: {:?}",
            offer.effort
        )
    };
    assert_eq!(reason, "the harness does not list the model");
    assert!(matches!(
        offer.service_tier,
        ServiceTierOffer::Unobserved { .. }
    ));
}

/// Without a model, `default` included, nothing is sent after `session/new`
/// and the session's own configuration is what is read.
#[tokio::test]
async fn without_a_model_the_sessions_own_configuration_is_read_and_nothing_is_sent() {
    for model in [None, Some("default"), Some("openai/default")] {
        let (offer, seen) = offer_from(
            model,
            Some(measured()),
            new_session(&codex("gpt-5.4", "low", &["low", "medium"], Some("on"))),
            Vec::new(),
        )
        .await;
        let offer = offer.expect("an offer");
        assert_eq!(seen, ["initialize", "session/new"], "{model:?}");
        assert_eq!(
            offer.model,
            ModelOffer::SessionDefault {
                current: Some("gpt-5.4".to_owned())
            }
        );
        assert_eq!(
            offer.effort,
            EffortOffer::Offered {
                option: "reasoning_effort".to_owned(),
                values: vec!["low".to_owned(), "medium".to_owned()],
                omitted: 0,
                current: Some("low".to_owned()),
            }
        );
        assert_eq!(
            offer.service_tier,
            ServiceTierOffer::Offered {
                option: "fast-mode".to_owned(),
                tiers: vec![ServiceTier::Standard, ServiceTier::Fast],
                current: Some(ServiceTier::Fast),
            }
        );
    }
}

/// The session lists the model and answers its selection with another one:
/// the answer's configuration describes that other model, so it is not read.
#[tokio::test]
async fn a_selection_the_session_does_not_confirm_leaves_the_rest_unobserved() {
    let (offer, seen) = offer_from(
        Some("openai/gpt-5.5"),
        Some(measured()),
        new_session(&codex("gpt-5.4", "medium", &["low", "medium"], None)),
        vec![json!({"configOptions": codex(
            "gpt-5.4",
            "medium",
            &["low", "medium", "high"],
            Some("off"),
        )})],
    )
    .await;
    let offer = offer.expect("an offer");
    assert_eq!(
        seen.len(),
        4,
        "the one model setter and nothing after: {seen:?}"
    );
    assert_eq!(
        offer.model,
        ModelOffer::NotConfirmed {
            reported: Some("gpt-5.4".to_owned())
        }
    );
    assert!(matches!(offer.effort, EffortOffer::Unobserved { .. }));
    assert!(matches!(
        offer.service_tier,
        ServiceTierOffer::Unobserved { .. }
    ));
}

/// A model only the legacy list names: selecting it there answers no
/// configuration, so nothing is selected and nothing dependent is read.
#[tokio::test]
async fn a_model_only_the_legacy_list_names_is_reported_without_a_selection() {
    let session = json!({
        "sessionId":"s-offer",
        "configOptions": codex("gpt-5.4", "medium", &["low", "medium"], None),
        "models": {"currentModelId":"gpt-5.4","availableModels":[
            {"modelId":"gpt-5.4","name":"gpt-5.4"},
            {"modelId":"gpt-6-astra[high]","name":"gpt-6-astra[high]"}]}
    });
    let (offer, seen) = offer_from(
        Some("openai/gpt-6-astra[high]"),
        Some(measured()),
        session,
        Vec::new(),
    )
    .await;
    let offer = offer.expect("an offer");
    assert_eq!(seen, ["initialize", "session/new"]);
    assert_eq!(
        offer.model,
        ModelOffer::LegacyList {
            native: "gpt-6-astra[high]".to_owned()
        }
    );
    assert_eq!(
        offer.models,
        ["gpt-5.4", "gpt-5.5", "gpt-5.4", "gpt-6-astra[high]"],
        "the option's choices, then the legacy list's ids"
    );
    assert!(matches!(offer.effort, EffortOffer::Unobserved { .. }));
}

/// The speed control is measured for one adapter at one release: another
/// release, another adapter or an adapter that names itself nothing reports
/// the tier unobserved with the reason, and the efforts are still read.
#[tokio::test]
async fn a_release_or_an_adapter_that_is_not_measured_reports_the_tier_unobserved() {
    let cases = [
        (
            Some(json!({"name":CODEX_ACP,"version":"2.6.0"})),
            "measured on codex-acp 1.13.1 and this session answers as release `2.6.0`",
        ),
        (
            Some(json!({"name":"some-other-acp","version":"1.13.1"})),
            "no speed control is measured for the harness that answers as `some-other-acp`",
        ),
        (
            None,
            "no speed control is measured for the harness that answers as ``",
        ),
    ];
    for (agent_info, expected) in cases {
        let (offer, _) = offer_from(
            None,
            agent_info,
            new_session(&codex("gpt-5.4", "low", &["low", "medium"], Some("off"))),
            Vec::new(),
        )
        .await;
        let offer = offer.expect("an offer");
        let ServiceTierOffer::Unobserved { reason } = &offer.service_tier else {
            panic!(
                "an unmeasured speed option must not be reported: {:?}",
                offer.service_tier
            )
        };
        assert!(reason.contains(expected), "{reason}");
        assert!(matches!(offer.effort, EffortOffer::Offered { .. }));
    }
}

/// On the measured adapter, a model without the speed option is `NotOffered`;
/// the id under another category or advertised twice is not the measured
/// control; an option that lists one native value offers one word.
#[tokio::test]
async fn the_speed_option_is_read_by_its_id_and_category_and_word_by_word() {
    let base = |extra: Vec<Value>| {
        let mut options = vec![
            select("model", "model", "gpt-5.4", &["gpt-5.4"]),
            select("reasoning_effort", "thought_level", "low", &["low"]),
        ];
        options.extend(extra);
        new_session(&Value::Array(options))
    };
    let (absent, _) = offer_from(None, Some(measured()), base(Vec::new()), Vec::new()).await;
    assert_eq!(
        absent.expect("an offer").service_tier,
        ServiceTierOffer::NotOffered
    );

    let elsewhere = base(vec![select("fast-mode", "other", "off", &["off", "on"])]);
    let (elsewhere, _) = offer_from(None, Some(measured()), elsewhere, Vec::new()).await;
    assert!(matches!(
        elsewhere.expect("an offer").service_tier,
        ServiceTierOffer::Unobserved { .. }
    ));

    let twice = base(vec![
        select("fast-mode", "model_config", "off", &["off", "on"]),
        select("fast-mode", "model_config", "on", &["off", "on"]),
    ]);
    let (twice, _) = offer_from(None, Some(measured()), twice, Vec::new()).await;
    assert!(matches!(
        twice.expect("an offer").service_tier,
        ServiceTierOffer::Unobserved { .. }
    ));

    let boolean = base(vec![json!({"id":"fast-mode","category":"model_config",
        "type":"boolean","currentValue":false})]);
    let (boolean, _) = offer_from(None, Some(measured()), boolean, Vec::new()).await;
    assert!(matches!(
        boolean.expect("an offer").service_tier,
        ServiceTierOffer::Unobserved { .. }
    ));

    let one = base(vec![select(
        "fast-mode",
        "model_config",
        "priority",
        &["on"],
    )]);
    let (one, _) = offer_from(None, Some(measured()), one, Vec::new()).await;
    assert_eq!(
        one.expect("an offer").service_tier,
        ServiceTierOffer::Offered {
            option: "fast-mode".to_owned(),
            tiers: vec![ServiceTier::Fast],
            current: None,
        },
        "a native value that is neither measured word is no current tier"
    );
}

/// No reasoning option is `NotOffered`; two are ambiguous and unobserved; one
/// that is not a select is unobserved.
#[tokio::test]
async fn the_reasoning_option_is_read_only_when_it_is_the_one_select() {
    let with = |extra: Vec<Value>| {
        let mut options = vec![select("model", "model", "gpt-5.4", &["gpt-5.4"])];
        options.extend(extra);
        new_session(&Value::Array(options))
    };
    let (none, _) = offer_from(None, Some(measured()), with(Vec::new()), Vec::new()).await;
    assert_eq!(none.expect("an offer").effort, EffortOffer::NotOffered);

    let two = with(vec![
        select("reasoning_effort", "thought_level", "low", &["low"]),
        select("thinking", "thought_level", "on", &["on"]),
    ]);
    let (two, _) = offer_from(None, Some(measured()), two, Vec::new()).await;
    let EffortOffer::Unobserved { reason } = two.expect("an offer").effort else {
        panic!("two reasoning options are ambiguous")
    };
    assert!(reason.contains("reasoning_effort · thinking"), "{reason}");

    let toggle = with(vec![json!({"id":"thinking","category":"thought_level",
        "type":"boolean","currentValue":true})]);
    let (toggle, _) = offer_from(None, Some(measured()), toggle, Vec::new()).await;
    assert!(matches!(
        toggle.expect("an offer").effort,
        EffortOffer::Unobserved { .. }
    ));
}

/// A session that lists more models than an offer carries has the rest
/// counted; the requested model is still judged on the whole list.
#[tokio::test]
async fn a_long_model_list_is_bounded_and_the_requested_model_is_judged_on_all_of_it() {
    let names: Vec<String> = (0..MAX_OFFERED_VALUES + 6)
        .map(|n| format!("model-{n}"))
        .collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let last = refs[refs.len() - 1];
    let config = |current: &str| {
        Value::Array(vec![
            select("model", "model", current, &refs),
            select("reasoning_effort", "thought_level", "low", &["low"]),
        ])
    };
    let (offer, _) = offer_from(
        Some(last),
        Some(measured()),
        new_session(&config("model-0")),
        vec![json!({"configOptions": config(last)})],
    )
    .await;
    let offer = offer.expect("an offer");
    assert_eq!(offer.models.len(), MAX_OFFERED_VALUES);
    assert_eq!(offer.models_omitted, 6);
    assert_eq!(
        offer.model,
        ModelOffer::Confirmed {
            native: last.to_owned()
        },
        "a model beyond the reported bound is still found and selected"
    );
}

/// An agent that settles on another protocol version, or that answers the
/// model selection with an error, fails the reading like it fails a run, and
/// the peer still receives no prompt.
#[tokio::test]
async fn a_session_that_refuses_the_handshake_or_the_selection_is_an_error_without_a_prompt() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, mut peer_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(async move {
        let mut reader = BufReader::new(peer_read);
        let init = read(&mut reader).await.expect("initialize");
        reply(&mut peer_write, &init, json!({"protocolVersion":2})).await;
        assert!(
            read(&mut reader).await.is_none(),
            "nothing after a refused handshake"
        );
    });
    let refused = super::read_offer(
        client_read,
        client_write,
        HarnessRequest::new("never sent", "/tmp"),
        std::time::Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect_err("protocol v2 is refused");
    agent.await.expect("scripted peer completes");
    assert!(
        matches!(refused, HarnessError::Refused { .. }),
        "{refused:?}"
    );

    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, mut peer_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(async move {
        let mut reader = BufReader::new(peer_read);
        let init = read(&mut reader).await.expect("initialize");
        reply(&mut peer_write, &init, json!({"protocolVersion":1})).await;
        let new = read(&mut reader).await.expect("session/new");
        let config = codex("gpt-5.4", "low", &["low"], None);
        reply(&mut peer_write, &new, new_session(&config)).await;
        let set = read(&mut reader).await.expect("the model selection");
        let error = json!({"jsonrpc":"2.0","id":set["id"],
            "error":{"code":-32602,"message":"model unavailable for this account"}});
        let mut bytes = serde_json::to_vec(&error).expect("JSON");
        bytes.push(b'\n');
        peer_write.write_all(&bytes).await.expect("write peer");
        peer_write.flush().await.expect("flush peer");
        assert!(
            read(&mut reader).await.is_none(),
            "nothing after a refused selection"
        );
    });
    let refused = super::read_offer(
        client_read,
        client_write,
        HarnessRequest::new("never sent", "/tmp").with_requested_model("openai/gpt-5.5"),
        std::time::Duration::from_secs(5),
        None,
        None,
        None,
    )
    .await
    .expect_err("the adapter's own refusal of the selection");
    agent.await.expect("scripted peer completes");
    let HarnessError::Refused { reason } = refused else {
        panic!("the adapter's refusal, got {refused:?}")
    };
    assert_eq!(
        reason,
        "model unavailable for this account (jsonrpc -32602)"
    );
}

async fn run_result(
    mut stream: nika_kernel::ai::harness::HarnessEventStream,
) -> Result<(), HarnessError> {
    loop {
        match std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Ok(HarnessEvent::Completed { .. })) => return Ok(()),
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(error),
            None => panic!("scripted turn must complete or refuse"),
        }
    }
}

/// Drive a run that asks `effort` and `tier` for `gpt-5.5` against the same
/// scripted configurations an offer reads, answering every setter with the
/// configuration that holds what it set.
async fn run_accepts(effort: Option<&str>, tier: Option<&str>, after_model: Value) -> bool {
    let (ours, theirs) = tokio::io::duplex(256 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, mut peer_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(async move {
        let mut reader = BufReader::new(peer_read);
        let init = read(&mut reader).await.expect("initialize");
        let info = json!({"protocolVersion":1,"agentInfo":{"name":CODEX_ACP,"version":"1.13.1"}});
        reply(&mut peer_write, &init, info).await;
        let new = read(&mut reader).await.expect("session/new");
        let before = codex("gpt-5.4", "medium", &["low", "medium"], None);
        reply(&mut peer_write, &new, new_session(&before)).await;
        let mut config = after_model;
        while let Some(request) = read(&mut reader).await {
            if request["method"] == "session/prompt" {
                reply(&mut peer_write, &request, json!({"stopReason":"end_turn"})).await;
                continue;
            }
            let (id, value) = (
                request["params"]["configId"]
                    .as_str()
                    .expect("a config id")
                    .to_owned(),
                request["params"]["value"].clone(),
            );
            if id != "model" {
                for option in config.as_array_mut().expect("options") {
                    if option["id"] == id.as_str() {
                        option["currentValue"] = value.clone();
                    }
                }
            }
            reply(
                &mut peer_write,
                &request,
                json!({"configOptions": config.clone()}),
            )
            .await;
        }
    });
    let request = HarnessRequest::new("hello", "/tmp")
        .with_requested_model("openai/gpt-5.5")
        .with_requested_effort(effort.map(str::to_owned))
        .with_requested_service_tier(tier.map(str::to_owned));
    let accepted = run_result(crate::client::drive(client_read, client_write, request))
        .await
        .is_ok();
    agent.await.expect("scripted peer completes");
    accepted
}

/// An offer and a run read the same configuration the same way: every effort
/// and tier an offer lists is one a run on that session accepts, and one it
/// does not list is one the run refuses.
#[tokio::test]
async fn what_an_offer_lists_a_run_accepts_and_what_it_does_not_list_a_run_refuses() {
    for fast in [None, Some("off")] {
        let after_model = codex("gpt-5.5", "medium", &["low", "medium", "high"], fast);
        let (offer, _) = offer_from(
            Some("openai/gpt-5.5"),
            Some(measured()),
            new_session(&codex("gpt-5.4", "medium", &["low", "medium"], None)),
            vec![json!({"configOptions": after_model.clone()})],
        )
        .await;
        let offer = offer.expect("an offer");
        let EffortOffer::Offered { values, .. } = &offer.effort else {
            panic!("efforts are listed")
        };
        for effort in ["low", "medium", "high", "xhigh"] {
            assert_eq!(
                run_accepts(Some(effort), None, after_model.clone()).await,
                values.iter().any(|value| value == effort),
                "effort `{effort}`"
            );
        }
        let offered: Vec<ServiceTier> = match &offer.service_tier {
            ServiceTierOffer::Offered { tiers, .. } => tiers.clone(),
            ServiceTierOffer::NotOffered => Vec::new(),
            other => panic!("the measured adapter reads its tiers: {other:?}"),
        };
        for tier in ServiceTier::ALL {
            assert_eq!(
                run_accepts(None, Some(tier.as_str()), after_model.clone()).await,
                offered.contains(&tier),
                "tier `{tier}` with fast-mode {fast:?}"
            );
        }
    }
}
