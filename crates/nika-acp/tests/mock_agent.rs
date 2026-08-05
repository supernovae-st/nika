// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The mock ACP agent — the load-bearing instrument (spec §5 · B1).
//!
//! A deterministic scripted agent speaking wire v1 from the AGENT role
//! of the SAME official SDK the production client drives — conformance
//! by construction. Wired in-process over `Channel::duplex()` (better
//! than the spec's helper-bin sketch: zero spawn · zero network · zero
//! OS surface in CI). Instrument law: the mock is proven by scripting
//! a KNOWN dialogue and asserting every beat, BEFORE any production
//! consumer trusts it.

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Channel, Client, ConnectionTo};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The scripted mock: initialize → capabilities · session/new → a fixed
/// id · prompt → ONE text update then end_turn. Every beat counted.
fn spawn_mock_agent(channel: Channel) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = Agent
            .builder()
            .name("nika-mock-agent")
            .on_receive_request(
                async move |initialize: InitializeRequest, responder, _connection| {
                    responder.respond(
                        InitializeResponse::new(initialize.protocol_version)
                            .agent_capabilities(AgentCapabilities::new()),
                    )
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |_new_session: NewSessionRequest, responder, _connection| {
                    responder.respond(NewSessionResponse::new(SessionId::new("mock-session-1")))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |prompt: PromptRequest, responder, connection| {
                    // Echo the scripted line as one session/update, then
                    // close the turn — the smallest COMPLETE dialogue.
                    let _ = connection.send_notification(SessionNotification::new(
                        prompt.session_id.clone(),
                        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                            TextContent::new("scripted: the mock heard you"),
                        ))),
                    ));
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_to(channel)
            .await;
    })
}

#[tokio::test]
async fn the_instrument_speaks_a_known_dialogue() {
    let (agent_side, client_side) = Channel::duplex();
    let _agent = spawn_mock_agent(agent_side);

    let updates = Arc::new(AtomicUsize::new(0));
    let updates_in_handler = Arc::clone(&updates);

    let outcome: Result<StopReason, agent_client_protocol::Error> = Client
        .builder()
        .name("nika-test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                // The scripted beat: exactly the text the mock scripted.
                let rendered = format!("{:?}", notification.update);
                assert!(
                    rendered.contains("scripted: the mock heard you"),
                    "unexpected update: {rendered}"
                );
                updates_in_handler.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(client_side, |connection: ConnectionTo<Agent>| async move {
            let init = connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            assert_eq!(init.protocol_version, ProtocolVersion::V1);

            let session = connection
                .send_request(NewSessionRequest::new(std::path::PathBuf::from("/")))
                .block_task()
                .await?;
            assert_eq!(session.session_id, SessionId::new("mock-session-1"));

            let prompt = connection
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new("hello mock"))],
                ))
                .block_task()
                .await?;
            Ok(prompt.stop_reason)
        })
        .await;

    assert_eq!(
        outcome.expect("the scripted dialogue completes"),
        StopReason::EndTurn
    );
    assert_eq!(
        updates.load(Ordering::SeqCst),
        1,
        "exactly ONE scripted update must have arrived"
    );
}
