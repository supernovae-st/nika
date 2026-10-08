// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Duplex ACP peers; no installed harness, credential or model is consulted.
use super::*;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

fn identity() -> Value {
    json!({"protocolVersion":1,"agentInfo":{"name":NAME,"version":VERSION}})
}
async fn read(r: &mut BufReader<ReadHalf<DuplexStream>>) -> Value {
    let mut line = String::new();
    r.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap()
}
async fn write(w: &mut WriteHalf<DuplexStream>, value: Value) {
    w.write_all(format!("{value}\n").as_bytes()).await.unwrap();
}

#[test]
fn the_profile_disables_builtins_disk_settings_and_other_mcp_before_query() {
    let meta = profile();
    let options = &meta["claudeCode"]["options"];
    for key in ["tools", "settingSources", "plugins", "skills"] {
        assert_eq!(options[key], json!([]));
    }
    assert_eq!(options["mcpServers"], json!({}));
    assert_eq!(options["strictMcpConfig"], true);
    assert_eq!(options["maxTurns"], 1);
    assert_eq!(options["allowDangerouslySkipPermissions"], false);
    assert_eq!(options["persistSession"], false);
    assert!(admit(&identity(), Completion::Authoring).is_ok());
    for value in [
        json!({}),
        json!({"protocolVersion":1,"agentInfo":{"name":NAME,"version":"0.23.1"}}),
        json!({"protocolVersion":1,"agentInfo":{"name":"Codex","version":VERSION}}),
    ] {
        assert!(admit(&value, Completion::Authoring).is_err());
    }
}

#[test]
fn text_only_and_configured_identity_never_become_served_or_free() {
    for tag in ["tool_call", "tool_call_update", "unexpected"] {
        assert!(judge_update(&json!({"sessionUpdate":tag}), Completion::Authoring).is_err());
    }
    assert!(judge_update(&json!({"sessionUpdate":"agent_message_chunk","content":{"type":"image","data":"ignored"}}), Completion::Authoring).is_err());
    assert!(
        judge_update(
            &json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"whole"}}),
            Completion::Authoring
        )
        .is_ok()
    );
    let d = descriptor(
        "claude-code",
        Some("claude-code/test"),
        "claude-code/test",
        &[],
    );
    assert!(d["served_model"].is_null());
    assert!(d["billed_cost_usd"].is_null());
    assert_eq!(d["numeric_usage_reported"], false);
}

#[tokio::test]
async fn unsupported_identity_stops_before_session_or_prompt() {
    let (ours, theirs) = tokio::io::duplex(65536);
    let (r, w) = tokio::io::split(ours);
    let (r2, mut w2) = tokio::io::split(theirs);
    let peer = tokio::spawn(async move {
        let mut r2 = BufReader::new(r2);
        let init = read(&mut r2).await;
        write(&mut w2,json!({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1,"agentInfo":{"name":NAME,"version":"0.23.1"}}})).await;
        let mut line = String::new();
        assert_eq!(
            r2.read_line(&mut line).await.unwrap(),
            0,
            "no session/new or model prompt"
        );
    });
    let mut stream = crate::client::drive_profile(
        r,
        w,
        HarnessRequest::new("private prompt", "/tmp"),
        Duration::from_secs(1),
        Some(Completion::Authoring),
    );
    let first = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
    assert!(matches!(first, Some(Err(HarnessError::Refused { .. }))));
    tokio::time::timeout(Duration::from_secs(1), peer)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn admitted_wire_carries_strict_options_and_keeps_the_whole_answer() {
    let (ours, theirs) = tokio::io::duplex(65536);
    let (r, w) = tokio::io::split(ours);
    let (r2, mut w2) = tokio::io::split(theirs);
    let peer = tokio::spawn(async move {
        let mut r2 = BufReader::new(r2);
        let init = read(&mut r2).await;
        write(
            &mut w2,
            json!({"jsonrpc":"2.0","id":init["id"],"result":identity()}),
        )
        .await;
        let new = read(&mut r2).await;
        assert_eq!(new["method"], "session/new");
        assert_eq!(new["params"]["_meta"], profile());
        assert_eq!(new["params"]["mcpServers"], json!([]));
        write(&mut w2,json!({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s","models":{"currentModelId":"served-config","availableModels":[]}}})).await;
        let prompt = read(&mut r2).await;
        assert_eq!(prompt["method"], "session/prompt");
        for text in ["prefix ", "{\"candidate\":true}", " suffix"] {
            write(&mut w2,json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}}})).await;
        }
        write(
            &mut w2,
            json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}}),
        )
        .await;
    });
    let mut stream = crate::client::drive_profile(
        r,
        w,
        HarnessRequest::new("private prompt", "/tmp"),
        Duration::from_secs(1),
        Some(Completion::Authoring),
    );
    let mut answer = None;
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        if let HarnessEvent::Completed { outcome } = event.unwrap() {
            answer = Some(outcome);
            break;
        }
    }
    let answer = answer.unwrap();
    assert_eq!(answer.output, "prefix {\"candidate\":true} suffix");
    assert_eq!(answer.observed_model.as_deref(), Some("served-config"));
    assert_eq!(
        answer.observed_model_source,
        Some(nika_kernel::ai::harness::ModelProvenance::SessionConfig)
    );
    tokio::time::timeout(Duration::from_secs(1), peer)
        .await
        .unwrap()
        .unwrap();
}

async fn refuses_after_prompt(mode: &str) {
    let mode = mode.to_owned();
    let (ours, theirs) = tokio::io::duplex(65536);
    let (r, w) = tokio::io::split(ours);
    let (r2, mut w2) = tokio::io::split(theirs);
    let peer = tokio::spawn(async move {
        let mut r2 = BufReader::new(r2);
        let init = read(&mut r2).await;
        write(
            &mut w2,
            json!({"jsonrpc":"2.0","id":init["id"],"result":identity()}),
        )
        .await;
        let new = read(&mut r2).await;
        write(
            &mut w2,
            json!({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s"}}),
        )
        .await;
        let prompt = read(&mut r2).await;
        assert_eq!(prompt["method"], "session/prompt");
        match mode.as_str() {
            "eof" => {}
            "permission" => {
                write(&mut w2,json!({"jsonrpc":"2.0","id":90,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"title":"forbidden"},"options":[]}})).await;
                let denied = read(&mut r2).await;
                assert_eq!(denied["result"]["outcome"]["outcome"], "cancelled");
            }
            reason => {
                write(
                    &mut w2,
                    json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":reason}}),
                )
                .await;
            }
        }
    });
    let mut stream = crate::client::drive_profile(
        r,
        w,
        HarnessRequest::new("private prompt", "/tmp"),
        Duration::from_secs(1),
        Some(Completion::Authoring),
    );
    let first = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
    assert!(
        matches!(first, Some(Err(_))),
        "no completed candidate: {first:?}"
    );
    tokio::time::timeout(Duration::from_secs(1), peer)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn truncation_turn_limit_refusal_and_eof_never_become_completed() {
    for reason in [
        "max_tokens",
        "max_turn_requests",
        "refusal",
        "cancelled",
        "eof",
    ] {
        refuses_after_prompt(reason).await;
    }
}
#[tokio::test]
async fn an_unexpected_permission_is_denied_and_the_whole_answer_refused() {
    refuses_after_prompt("permission").await;
}
/// A lost sign-in is told as one, in our own words: the human signs in again in the app.
#[test]
fn a_lost_sign_in_is_told_as_one() {
    let why = safe_error(&HarnessError::Unavailable {
        reason: crate::client::SIGN_IN_EXPIRED.into(),
    });
    assert!(why.contains("sign-in has expired or was revoked"), "{why}");
    assert!(why.contains("sign in again"), "{why}");
    let other = safe_error(&HarnessError::Unavailable {
        reason: "OAuth token for someone@example.invalid".into(),
    });
    assert!(!other.contains("someone@example.invalid"), "{other}");
}
#[test]
fn transport_errors_do_not_publish_peer_secrets() {
    let why = safe_error(&HarnessError::Refused {
        reason: "private-key-test-marker and private prompt".into(),
    });
    assert!(!why.contains("private-key-test-marker"));
    assert!(!why.contains("private prompt"));
    assert!(why.contains("no answer accepted"));
}

/// Nika's own selection refusal reaches the author verbatim (the option,
/// the value and the discovered offer) while adapter text stays withheld.
#[test]
fn a_selection_refusal_is_shown_in_nika_own_words() {
    let reason = "the harness offers reasoning effort low · medium · high — `ultra` is not one \
                  (no prompt was sent)";
    let why = safe_error(&HarnessError::Selection {
        reason: reason.into(),
    });
    assert_eq!(
        why,
        format!("ACP authoring refused: {reason}; no answer accepted")
    );
}

/// The audit's loss hazard, ACP half: an explicit authoring effort rides the
/// session request verbatim (the client then applies it through the session's
/// own reasoning option and reads it back, or refuses before the prompt).
#[test]
fn an_explicit_authoring_effort_rides_the_session_request() {
    let native = crate::HarnessInferRequest::new("draft", "claude-code/opus")
        .with_system(Some("card".into()))
        .with_effort(Some("high".into()));
    let request = session_request(
        native,
        Some("claude-code/opus"),
        std::path::Path::new("/tmp"),
    );
    assert_eq!(request.requested_effort.as_deref(), Some("high"));
    assert_eq!(request.requested_model.as_deref(), Some("claude-code/opus"));
    assert_eq!(request.system.as_deref(), Some("card"));
    let plain = session_request(
        crate::HarnessInferRequest::new("draft", "session"),
        None,
        std::path::Path::new("/tmp"),
    );
    assert_eq!(
        (plain.requested_effort, plain.requested_model),
        (None, None)
    );
}
