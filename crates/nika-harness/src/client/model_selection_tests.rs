// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Model choice dialogues: an explicit variant never degrades to its family.
use super::*;

const BASE: &str = "gpt-6-astra";
const HIGH: &str = "gpt-6-astra[high]";
type PeerReader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type PeerWriter = tokio::io::WriteHalf<tokio::io::DuplexStream>;

async fn read(peer: &mut PeerReader) -> Option<Value> {
    let mut line = String::new();
    let size = tokio::time::timeout(std::time::Duration::from_secs(3), peer.read_line(&mut line))
        .await
        .expect("scripted dialogue must settle")
        .expect("read peer");
    (size != 0).then(|| serde_json::from_str(&line).expect("JSON request"))
}

async fn write(peer: &mut PeerWriter, message: Value) {
    let mut bytes = serde_json::to_vec(&message).expect("JSON response");
    bytes.push(b'\n');
    peer.write_all(&bytes).await.expect("write peer");
    peer.flush().await.expect("flush peer");
}

async fn reply(peer: &mut PeerWriter, request: &Value, result: Value) {
    write(
        peer,
        serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result}),
    )
    .await;
}

fn advertised(base: &str, legacy: &[&str]) -> Value {
    serde_json::json!({
        "configOptions":[{"id":"model","category":"model","currentValue":base,
            "options":[{"value":base,"name":base}]}],
        "models":{"currentModelId":base,"availableModels":legacy.iter()
            .map(|id| serde_json::json!({"modelId":id,"name":id})).collect::<Vec<_>>()}
    })
}

async fn offer(peer: &mut PeerReader, writer: &mut PeerWriter, mut choices: Value) {
    let initialize = read(peer).await.expect("initialize");
    assert_eq!(initialize["method"], "initialize");
    reply(
        writer,
        &initialize,
        serde_json::json!({"protocolVersion":1}),
    )
    .await;
    let session = read(peer).await.expect("session/new");
    assert_eq!(session["method"], "session/new");
    choices["sessionId"] = Value::String("model-choice".to_owned());
    reply(writer, &session, choices).await;
}

async fn end(peer: &mut PeerReader, writer: &mut PeerWriter) {
    let prompt = read(peer)
        .await
        .expect("prompt follows accepted exact selection");
    assert_eq!(prompt["method"], "session/prompt");
    assert_eq!(prompt["params"]["sessionId"], "model-choice");
    reply(
        writer,
        &prompt,
        serde_json::json!({"stopReason":"end_turn"}),
    )
    .await;
}

async fn result(mut stream: HarnessEventStream) -> Result<HarnessOutcome, HarnessError> {
    loop {
        match std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Ok(HarnessEvent::Completed { outcome })) => return Ok(*outcome),
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(error),
            None => panic!("scripted turn must complete or refuse"),
        }
    }
}

#[tokio::test]
async fn an_exact_legacy_variant_wins_over_a_config_family_before_the_prompt() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, mut peer_write) = tokio::io::split(theirs);
    let peer = tokio::spawn(async move {
        let mut reader = BufReader::new(peer_read);
        offer(
            &mut reader,
            &mut peer_write,
            advertised(BASE, &["gpt-6-astra[low]", HIGH]),
        )
        .await;
        let select = read(&mut reader).await.expect("exact legacy selection");
        assert_eq!(select["method"], "session/set_model");
        assert_eq!(select["params"]["modelId"], HIGH);
        assert_eq!(select["params"]["sessionId"], "model-choice");
        reply(&mut peer_write, &select, serde_json::json!({})).await;
        end(&mut reader, &mut peer_write).await;
    });
    let outcome = result(drive(
        client_read,
        client_write,
        HarnessRequest::new("hello", "/tmp").with_requested_model(format!("openai/{HIGH}")),
    ))
    .await
    .expect("offered complete variant accepted");
    peer.await.expect("scripted peer");
    assert_eq!(outcome.observed_model.as_deref(), Some(HIGH));
    assert_eq!(
        outcome.observed_model_source,
        Some(ModelProvenance::AcceptedRequest),
        "legacy acceptance is not an echoed configuration or response attestation"
    );
}

#[tokio::test]
async fn an_absent_variant_refuses_without_selection_or_prompt_even_when_its_base_is_offered() {
    for legacy in [Vec::new(), vec!["gpt-6-astra[low]"]] {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, mut peer_write) = tokio::io::split(theirs);
        let peer = tokio::spawn(async move {
            let mut reader = BufReader::new(peer_read);
            offer(&mut reader, &mut peer_write, advertised(BASE, &legacy)).await;
            assert!(
                read(&mut reader).await.is_none(),
                "no base selection or prompt is authorized"
            );
        });
        let error = result(drive(
            client_read,
            client_write,
            HarnessRequest::new("hello", "/tmp").with_requested_model(format!("openai/{HIGH}")),
        ))
        .await
        .expect_err("unoffered variant refuses");
        peer.await.expect("scripted peer sees EOF");
        match error {
            HarnessError::Refused { reason } => {
                assert!(reason.contains(HIGH) && reason.contains(BASE), "{reason}");
            }
            other => panic!("expected a model refusal, got {other}"),
        }
    }
}

#[tokio::test]
async fn a_rejected_exact_legacy_selection_never_retries_on_the_base_or_prompts() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, mut peer_write) = tokio::io::split(theirs);
    let peer = tokio::spawn(async move {
        let mut reader = BufReader::new(peer_read);
        offer(&mut reader, &mut peer_write, advertised(BASE, &[HIGH])).await;
        let select = read(&mut reader).await.expect("exact legacy selection");
        assert_eq!(select["method"], "session/set_model");
        assert_eq!(select["params"]["modelId"], HIGH);
        write(
            &mut peer_write,
            serde_json::json!({"jsonrpc":"2.0","id":select["id"],
            "error":{"code":-32602,"message":"variant rejected"}}),
        )
        .await;
        assert!(
            read(&mut reader).await.is_none(),
            "no fallback or prompt after refusal"
        );
    });
    let error = result(drive(
        client_read,
        client_write,
        HarnessRequest::new("hello", "/tmp").with_requested_model(format!("openai/{HIGH}")),
    ))
    .await
    .expect_err("peer rejected selection");
    peer.await.expect("scripted peer sees EOF");
    assert!(error.to_string().contains("variant rejected"));
}

#[tokio::test]
async fn exact_config_choices_keep_their_selection_including_base_variant_and_claude_alias() {
    for (provider, name) in [
        ("openai", BASE),
        ("openai", HIGH),
        ("anthropic", "opus[1m]"),
    ] {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, mut peer_write) = tokio::io::split(theirs);
        let peer = tokio::spawn(async move {
            let mut reader = BufReader::new(peer_read);
            let choices = advertised(name, &[name]);
            offer(&mut reader, &mut peer_write, choices.clone()).await;
            let select = read(&mut reader).await.expect("exact config selection");
            assert_eq!(select["method"], "session/set_config_option");
            assert_eq!(select["params"]["configId"], "model");
            assert_eq!(select["params"]["value"], name);
            reply(&mut peer_write, &select, choices).await;
            end(&mut reader, &mut peer_write).await;
        });
        let outcome = result(drive(
            client_read,
            client_write,
            HarnessRequest::new("hello", "/tmp").with_requested_model(format!("{provider}/{name}")),
        ))
        .await
        .expect("exact advertised config choice");
        peer.await.expect("scripted peer");
        assert_eq!(outcome.observed_model.as_deref(), Some(name));
        assert_eq!(
            outcome.observed_model_source,
            Some(ModelProvenance::ConfirmedSelection)
        );
    }
}

#[tokio::test]
async fn default_and_absent_requests_observe_the_current_choice_without_setting_any_model() {
    for requested in [None, Some("openai/default")] {
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (client_read, client_write) = tokio::io::split(ours);
        let (peer_read, mut peer_write) = tokio::io::split(theirs);
        let peer = tokio::spawn(async move {
            let mut reader = BufReader::new(peer_read);
            offer(&mut reader, &mut peer_write, advertised(BASE, &[HIGH])).await;
            end(&mut reader, &mut peer_write).await;
        });
        let mut request = HarnessRequest::new("hello", "/tmp");
        if let Some(model) = requested {
            request = request.with_requested_model(model);
        }
        let outcome = result(drive(client_read, client_write, request))
            .await
            .expect("default turn");
        peer.await.expect("scripted peer");
        assert_eq!(outcome.observed_model.as_deref(), Some(BASE));
        assert_eq!(
            outcome.observed_model_source,
            Some(ModelProvenance::SessionConfig)
        );
    }
}
