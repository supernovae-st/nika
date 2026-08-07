// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! **The Lane A promise, kept** (spec §2bis) — the OFFICIAL SDK judges
//! the engine's hand-rolled wire-v1 client.
//!
//! The engine never links the SDK (its `serde_json/preserve_order`
//! would flip every byte-attested surface); instead the SDK lives here,
//! in the quarantined workspace, plays the AGENT role, and the engine's
//! `nika_harness::drive` (a dev-dependency of THIS workspace only)
//! plays the client against it over real pipes. A frame the engine
//! emits that the SDK cannot deserialize fails the test — which is the
//! whole point: conformance proven by the authority, not by our own
//! mock talking to itself.
//!
//! Instrument note: the two sides are wired over `tokio::io::duplex`
//! (the SDK's transport takes byte streams). Zero network, zero spawn.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, ByteStreams};
use futures_core::Stream as _;
use nika_kernel::ai::harness::{HarnessEvent, HarnessRequest};
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

/// The SDK-side agent: asserts the ENGINE's frames deserialize into the
/// official types (that is the judgment), then answers the dialogue.
fn spawn_sdk_agent(
    reader: tokio::io::ReadHalf<tokio::io::DuplexStream>,
    writer: tokio::io::WriteHalf<tokio::io::DuplexStream>,
    seen: Arc<AtomicUsize>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let seen_init = Arc::clone(&seen);
        let seen_new = Arc::clone(&seen);
        let seen_prompt = Arc::clone(&seen);
        let _ = Agent
            .builder()
            .name("sdk-conformance-judge")
            .on_receive_request(
                async move |initialize: InitializeRequest, responder, _connection| {
                    // JUDGMENT 1: the engine's `initialize` frame parsed
                    // into the official InitializeRequest — camelCase
                    // keys and the integer protocol version included.
                    seen_init.fetch_add(1, Ordering::SeqCst);
                    responder.respond(
                        InitializeResponse::new(initialize.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |new_session: NewSessionRequest, responder, _connection| {
                    // JUDGMENT 2: `session/new` parsed — the cwd is an
                    // absolute path the official type accepted.
                    assert!(
                        new_session.cwd.is_absolute(),
                        "the engine must send an absolute cwd: {:?}",
                        new_session.cwd
                    );
                    seen_new.fetch_add(1, Ordering::SeqCst);
                    responder.respond(NewSessionResponse::new(SessionId::new("sdk-judged")))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |prompt: PromptRequest, responder, connection| {
                    // JUDGMENT 3: `session/prompt` parsed — the session
                    // id round-tripped and the content block is the
                    // baseline text variant every agent MUST support.
                    assert_eq!(prompt.session_id, SessionId::new("sdk-judged"));
                    assert!(
                        matches!(prompt.prompt.first(), Some(ContentBlock::Text(_))),
                        "the engine must send a text content block"
                    );
                    seen_prompt.fetch_add(1, Ordering::SeqCst);
                    let _ = connection.send_notification(SessionNotification::new(
                        prompt.session_id.clone(),
                        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("judged by the official sdk"),
                        ))),
                    ));
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(ByteStreams::new(writer.compat_write(), reader.compat()))
            .await;
    })
}

#[tokio::test]
async fn the_official_sdk_judges_the_engines_wire_client() {
    let (engine_side, sdk_side) = tokio::io::duplex(64 * 1024);
    let (engine_read, engine_write) = tokio::io::split(engine_side);
    let (sdk_read, sdk_write) = tokio::io::split(sdk_side);

    let seen = Arc::new(AtomicUsize::new(0));
    let agent = spawn_sdk_agent(sdk_read, sdk_write, Arc::clone(&seen));

    // The ENGINE's own client — the thing under judgment.
    let mut stream = nika_harness::drive(
        engine_read,
        engine_write,
        HarnessRequest::new("hello from the engine", "/tmp"),
    );

    let mut chunks = Vec::new();
    let mut output = None;
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::MessageChunk { text })) => chunks.push(text),
            Some(Ok(HarnessEvent::Completed { outcome })) => output = Some(outcome.output.clone()),
            Some(Ok(_)) => {}
            Some(Err(e)) => panic!("the engine's client failed against the SDK: {e}"),
            None => break,
        }
    }
    let _ = agent.await;

    // Every judgment fired: three official types deserialized the
    // engine's three frames.
    assert_eq!(
        seen.load(Ordering::SeqCst),
        3,
        "the SDK must have parsed initialize + session/new + session/prompt"
    );
    // And the engine read the SDK's own frames back correctly.
    assert_eq!(chunks, vec!["judged by the official sdk"]);
    assert_eq!(output.as_deref(), Some("judged by the official sdk"));
}
