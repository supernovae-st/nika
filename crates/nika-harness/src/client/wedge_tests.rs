// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The driver's idle-deadline wedges (moved verbatim from `client.rs`).

use super::*;
use nika_kernel::ai::harness::HarnessRequest;
const FAST_IDLE: std::time::Duration = std::time::Duration::from_millis(80);

/// The refuter's counterexample (2026-08-06), pinned: a peer that
/// writes half a line and goes silent WITHOUT closing its pipe used
/// to hang the driver forever (a size bound, no time bound).
#[tokio::test]
async fn a_silent_partial_line_ends_at_the_idle_deadline() {
    let (ours, theirs) = tokio::io::duplex(4096);
    let (client_read, client_write) = tokio::io::split(ours);
    let (_agent_read, mut agent_write) = tokio::io::split(theirs);
    agent_write
        .write_all(b"{\"jsonrpc\":\"2.0\"")
        .await
        .expect("partial write");

    let mut stream = drive_with_idle(
        client_read,
        client_write,
        HarnessRequest::new("hi", "/tmp"),
        FAST_IDLE,
    );
    let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
    let Some(Err(HarnessError::Session { reason })) = next else {
        panic!("the idle deadline must end a silent peer, got {next:?}");
    };
    assert!(reason.contains("idle deadline"), "{reason}");
    drop(agent_write); // the pipe stayed OPEN for the whole wait
}

/// Progress resets the deadline: an agent that answers slowly (each
/// beat inside the window) is never killed mid-turn.
#[tokio::test]
async fn progress_resets_the_idle_deadline() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(ours);
    let (agent_read, mut agent_write) = tokio::io::split(theirs);

    let agent = tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt as _;
        let mut r = tokio::io::BufReader::new(agent_read);
        let mut line = String::new();
        let answer = async |w: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, v: Value| {
            let mut s = serde_json::to_string(&v).expect("json");
            s.push('\n');
            w.write_all(s.as_bytes()).await.expect("write");
        };
        for step in 0..3 {
            // Each beat lands at 60% of the window: three gaps in a
            // row, none fatal — the reset is what proves it.
            tokio::time::sleep(FAST_IDLE.mul_f32(0.6)).await;
            line.clear();
            r.read_line(&mut line).await.expect("read");
            let req: Value = serde_json::from_str(line.trim_end()).expect("json");
            let result = match step {
                0 => serde_json::json!({"protocolVersion": 1}),
                1 => serde_json::json!({"sessionId": "s-slow"}),
                _ => serde_json::json!({"stopReason": "end_turn"}),
            };
            answer(
                &mut agent_write,
                serde_json::json!({"jsonrpc":"2.0","id":req["id"],"result":result}),
            )
            .await;
        }
    });

    let mut stream = drive_with_idle(
        client_read,
        client_write,
        HarnessRequest::new("slow", "/tmp"),
        FAST_IDLE,
    );
    let mut completed = false;
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::Completed { .. })) => completed = true,
            Some(Ok(_)) => {}
            Some(Err(e)) => panic!("a slow BUT ALIVE agent must survive: {e}"),
            None => break,
        }
    }
    agent.await.expect("scripted slow agent");
    assert!(
        completed,
        "the turn completes across three near-deadline gaps"
    );
}
