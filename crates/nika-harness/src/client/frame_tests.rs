// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What the generic transport bound actually measures, at its production scale in paused
//! virtual time: one frame (a newline-terminated line) or one write, never a single byte.
use super::*;
use std::time::Duration;

const BOUND: Duration = Duration::from_secs(IDLE_TIMEOUT_SECS);

async fn first_error(mut stream: HarnessEventStream) -> Option<HarnessError> {
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        if let Err(error) = event {
            return Some(error);
        }
    }
    None
}

/// The bytes of an unfinished frame do not reset the bound: a peer trickling one byte every
/// 180 s, never sending the newline, is abandoned 300 s after the read began, not later.
#[tokio::test(start_paused = true)]
async fn a_trickling_unfinished_frame_ends_at_the_bound() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (agent_read, mut agent_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(async move {
        let mut reader = BufReader::new(agent_read);
        let mut line = String::new();
        reader.read_line(&mut line).await.expect("initialize");
        agent_write.write_all(b"{\"jsonrpc\":").await.expect("half");
        for _ in 0..5 {
            tokio::time::sleep(BOUND.mul_f32(0.6)).await;
            if agent_write.write_all(b" ").await.is_err() {
                return;
            }
        }
    });
    let started = tokio::time::Instant::now();
    let error = first_error(drive(
        client_read,
        client_write,
        HarnessRequest::new("hi", "/tmp"),
    ))
    .await;
    let elapsed = started.elapsed();
    assert!(
        matches!(&error, Some(HarnessError::Session { reason }) if reason.contains("idle deadline")),
        "{error:?}"
    );
    assert!(
        elapsed >= BOUND && elapsed < BOUND + Duration::from_secs(1),
        "the trickle did not extend the bound: ended after {elapsed:?}"
    );
    agent.abort();
}

/// Each completed frame resets the bound: three answers, each 250 s after its request, carry a
/// 750 s turn to completion under the 300 s bound.
#[tokio::test(start_paused = true)]
async fn each_completed_frame_resets_the_bound() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (agent_read, mut agent_write) = tokio::io::split(theirs);
    let agent = tokio::spawn(async move {
        let mut reader = BufReader::new(agent_read);
        for step in 0..3 {
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("request");
            let request: Value = serde_json::from_str(line.trim_end()).expect("json");
            tokio::time::sleep(Duration::from_secs(250)).await;
            let result = match step {
                0 => serde_json::json!({"protocolVersion": 1}),
                1 => serde_json::json!({"sessionId": "s-slow"}),
                _ => serde_json::json!({"stopReason": "end_turn"}),
            };
            let answer = serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result});
            agent_write
                .write_all(format!("{answer}\n").as_bytes())
                .await
                .expect("answer");
        }
    });
    let started = tokio::time::Instant::now();
    let mut stream = drive(
        client_read,
        client_write,
        HarnessRequest::new("slow", "/tmp"),
    );
    let mut completed = false;
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        match event {
            Ok(HarnessEvent::Completed { .. }) => completed = true,
            Ok(_) => {}
            Err(error) => panic!("three frames inside the bound must complete: {error}"),
        }
    }
    assert!(completed, "the turn completes");
    assert!(started.elapsed() >= Duration::from_secs(750));
    agent.await.expect("agent");
}

/// A peer that never reads its input blocks the write, which is bounded too: the session is
/// abandoned at the bound with the write deadline's words.
#[tokio::test(start_paused = true)]
async fn a_blocked_write_ends_at_the_bound() {
    let (ours, theirs) = tokio::io::duplex(8);
    let (client_read, client_write) = tokio::io::split(ours);
    let started = tokio::time::Instant::now();
    let error = first_error(drive(
        client_read,
        client_write,
        HarnessRequest::new("hi", "/tmp"),
    ))
    .await;
    let elapsed = started.elapsed();
    assert!(
        matches!(&error, Some(HarnessError::Session { reason }) if reason.contains("write deadline")),
        "{error:?}"
    );
    assert!(
        elapsed >= BOUND && elapsed < BOUND + Duration::from_secs(1),
        "{elapsed:?}"
    );
    drop(theirs);
}
