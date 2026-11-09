// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A judged session start against scripted peers: the registry row's own
//! judgment meets the session's first answer, a refused identity never
//! reaches `session/new`, and each failure keeps the class the standalone
//! probe gave it.

use std::pin::Pin;
use std::time::Duration;

use futures_core::Stream as _;
use nika_kernel::ai::harness::{HarnessError, HarnessEvent, HarnessOutcome, HarnessRequest};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::probe::VersionPin;
use crate::spawn::{HarnessAdapter, SpawnedHarness};

type PeerReader = BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>;
type PeerWriter = tokio::io::WriteHalf<tokio::io::DuplexStream>;

/// One line from the client, `None` at its EOF (the client gave up).
async fn read(peer: &mut PeerReader) -> Option<Value> {
    let mut line = String::new();
    let size = peer.read_line(&mut line).await.expect("read peer");
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

/// The judgment of the shipped `codex` row: it answers as `codex-acp` (its
/// command), under its npm scope or as `Codex`, from 0.16 up to any 1.x.
fn codex_judge() -> super::StartJudge {
    let row = HarnessAdapter::new("codex", "codex-acp")
        .expect("row")
        .with_identities(vec!["Codex".to_owned()])
        .with_handshake_probe()
        .with_version_pin(VersionPin::new((0, 16), 1));
    SpawnedHarness::new(row).start_judge()
}

/// Drive one judged session with the frame deadline `idle` against a scripted
/// peer. The peer task is awaited, so an assertion it makes fails the test.
async fn judged<F, Fut>(idle: Duration, peer: F) -> Result<HarnessOutcome, HarnessError>
where
    F: FnOnce(PeerReader, PeerWriter) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (peer_read, peer_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(peer(BufReader::new(peer_read), peer_write));
    let mut stream = crate::client::drive_profile(
        client_read,
        client_write,
        HarnessRequest::new("hello", "/tmp"),
        idle,
        None,
        None,
        None,
        Some(codex_judge()),
    );
    let outcome = loop {
        match std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
            Some(Ok(HarnessEvent::Completed { outcome })) => break Ok(*outcome),
            Some(Ok(_)) => {}
            Some(Err(error)) => break Err(error),
            None => panic!("a scripted session completes or refuses"),
        }
    };
    drop(stream);
    agent.await.expect("scripted peer completes");
    outcome
}

const IDLE: Duration = Duration::from_secs(crate::IDLE_TIMEOUT_SECS);

/// Answer `initialize` with `agent_info`, then assert the client hung up:
/// a refused identity never reaches `session/new`.
async fn refused_at_initialize(mut r: PeerReader, mut w: PeerWriter, agent_info: Value) {
    let init = read(&mut r).await.expect("initialize");
    assert_eq!(init["method"], "initialize");
    reply(
        &mut w,
        &init,
        json!({"protocolVersion":1,"agentInfo":agent_info}),
    )
    .await;
    let next = read(&mut r).await;
    assert!(next.is_none(), "no session/new after a refusal: {next:?}");
}

fn unavailable(result: Result<HarnessOutcome, HarnessError>) -> String {
    let error = result.expect_err("refused at the start");
    assert!(!error.is_transient(), "no retry heals it: {error:?}");
    let HarnessError::Unavailable { reason } = error else {
        panic!("an unavailable adapter, got {error:?}")
    };
    assert!(reason.starts_with("adapter `codex`: "), "{reason}");
    reason
}

/// The session's own first answer is the adapter's identity: admitted, the
/// session goes on to `session/new` and its prompt on the same connection.
#[tokio::test]
async fn the_sessions_own_initialize_admits_the_adapter() {
    let outcome = judged(IDLE, |mut r, mut w| async move {
        let init = read(&mut r).await.expect("initialize");
        assert_eq!(init["method"], "initialize");
        assert!(
            init["params"]["clientCapabilities"].is_object(),
            "the session's own request, with its real capabilities: {init}"
        );
        let info = json!({"name":"@agentclientprotocol/codex-acp","version":"1.13.1"});
        reply(&mut w, &init, json!({"protocolVersion":1,"agentInfo":info})).await;
        let new = read(&mut r).await.expect("session/new");
        assert_eq!(new["method"], "session/new");
        reply(&mut w, &new, json!({"sessionId":"s-start"})).await;
        let prompt = read(&mut r).await.expect("prompt");
        assert_eq!(prompt["method"], "session/prompt");
        reply(&mut w, &prompt, json!({"stopReason":"end_turn"})).await;
    })
    .await
    .expect("admitted and answered");
    assert_eq!(outcome.output, "");
}

/// Another program answering is unavailable, in the probe's own words,
/// before any session exists.
#[tokio::test]
async fn another_program_is_unavailable_before_any_session() {
    let reason = unavailable(
        judged(IDLE, |r, w| {
            refused_at_initialize(r, w, json!({"name":"qwen-code","version":"0.21.0"}))
        })
        .await,
    );
    assert!(
        reason.contains("the handshake answered as `qwen-code`")
            && reason.contains("it answers as `codex` · `Codex`"),
        "{reason}"
    );
}

/// A release outside the row's pin, or an answer that names none, is
/// unavailable before any session.
#[tokio::test]
async fn a_release_outside_the_pin_is_unavailable_before_any_session() {
    for (agent_info, needle) in [
        (
            json!({"name":"codex-acp","version":"2.6.0"}),
            "handshake version 2.6 is outside the pin (>= 0.16 · major <= 1)",
        ),
        (
            json!({"name":"codex-acp","version":"0.9.0"}),
            "handshake version 0.9 is outside the pin",
        ),
        (
            json!({"name":"codex-acp"}),
            "the initialize answer carried no `agentInfo.version`",
        ),
    ] {
        let reason =
            unavailable(judged(IDLE, move |r, w| refused_at_initialize(r, w, agent_info)).await);
        assert!(reason.contains(needle), "{needle}: {reason}");
    }
}

/// An adapter that reads `initialize` and never answers is unavailable at the
/// start bound, well before the five-minute frame deadline.
#[tokio::test(start_paused = true)]
async fn a_silent_adapter_is_unavailable_at_the_start_bound() {
    let started = tokio::time::Instant::now();
    let reason = unavailable(
        judged(IDLE, |mut r, w| async move {
            let init = read(&mut r).await.expect("initialize");
            assert_eq!(init["method"], "initialize");
            // Hold the pipe open and say nothing; the client gives up first.
            assert!(read(&mut r).await.is_none(), "nothing follows");
            drop(w);
        })
        .await,
    );
    assert_eq!(
        reason,
        "adapter `codex`: the initialize handshake did not answer in 30s"
    );
    let waited = started.elapsed();
    assert!(
        waited >= super::START_TIMEOUT && waited < super::START_TIMEOUT + Duration::from_secs(1),
        "bounded by the start timeout, not the frame deadline: {waited:?}"
    );
}

/// An adapter that exits before answering is unavailable, not a session
/// death a retry would replay.
#[tokio::test]
async fn an_adapter_that_closes_its_pipe_is_unavailable() {
    let reason = unavailable(
        judged(IDLE, |mut r, w| async move {
            let init = read(&mut r).await.expect("initialize");
            assert_eq!(init["method"], "initialize");
            drop(w);
            drop(r);
        })
        .await,
    );
    assert_eq!(
        reason,
        "adapter `codex`: the adapter closed its pipe without answering initialize"
    );
}

/// A call whose own frame deadline is shorter than the start bound keeps
/// its class: the silence is the session death it always was.
#[tokio::test(start_paused = true)]
async fn a_shorter_frame_deadline_keeps_its_session_class() {
    let error = judged(Duration::from_secs(5), |mut r, w| async move {
        let init = read(&mut r).await.expect("initialize");
        assert_eq!(init["method"], "initialize");
        assert!(read(&mut r).await.is_none(), "nothing follows");
        drop(w);
    })
    .await
    .expect_err("silent past the call's own deadline");
    assert!(error.is_transient(), "{error:?}");
    assert!(error.to_string().contains("sent nothing for 5s"), "{error}");
}
