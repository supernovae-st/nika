// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The HOSTILE-AGENT gauntlet — every scenario a broken or malicious
//! adapter can put on the wire, run against the real client over real
//! pipes. Each case asserts the engine's answer is a REFUSAL WITH
//! WORDS or a bounded survival, never a hang, a panic, or a silent
//! allow.
//!
//! Instrument note: every agent here is a python3 child (the same
//! interpreter the estate belt already requires), so the gauntlet
//! exercises the ACTUAL spawn seam — the pure-unit versions of these
//! properties live beside the code; these prove them end to end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::pin::Pin;

use futures_core::Stream as _;
use nika_harness::{HarnessAdapter, SpawnedHarness, VersionPin};
use nika_kernel::ai::harness::{AgentBackend as _, HarnessError, HarnessEvent, HarnessRequest};

/// Stage a python3 agent script and return its adapter.
fn agent(name: &str, script: &str) -> (HarnessAdapter, Guard) {
    let dir = std::env::temp_dir().join(format!("nika-gauntlet-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmpdir");
    let path = dir.join("agent.py");
    std::fs::write(&path, script).expect("script");
    let adapter = HarnessAdapter::new(format!("gauntlet-{name}"), "python3")
        .expect("id ok")
        .with_args(vec![path.to_string_lossy().into_owned()]);
    (adapter, Guard(dir))
}

struct Guard(std::path::PathBuf);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Drain a run to its verdict: (chunks, output, error).
async fn drain(adapter: HarnessAdapter) -> (Vec<String>, Option<String>, Option<HarnessError>) {
    let harness = SpawnedHarness::new(adapter);
    let mut stream = match harness.run_agent(HarnessRequest::new("go", "/tmp")).await {
        Ok(s) => s,
        Err(e) => return (Vec::new(), None, Some(e)),
    };
    let (mut chunks, mut output, mut error) = (Vec::new(), None, None);
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::MessageChunk { text })) => chunks.push(text),
            Some(Ok(HarnessEvent::Completed { outcome })) => output = Some(outcome.output.clone()),
            Some(Ok(HarnessEvent::PermissionAsked { reply, .. })) => {
                // The gauntlet plays the engine's B4 posture: deny.
                reply.respond(nika_kernel::ai::harness::PermissionDecision::Deny);
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => error = Some(e),
            None => break,
        }
    }
    (chunks, output, error)
}

const HANDSHAKE: &str = r#"
import json, sys
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def recv():
    return json.loads(sys.stdin.readline())
init = recv()
send({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1}})
new = recv()
send({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s-g"}})
prompt = recv()
"#;

#[tokio::test]
async fn g1_an_agent_that_dies_after_the_handshake_is_a_session_death() {
    let (a, _g) = agent("die", &format!("{HANDSHAKE}\nsys.exit(0)\n"));
    let (_, output, error) = drain(a).await;
    assert!(output.is_none(), "a dead agent produces no outcome");
    let e = error.expect("the death must surface");
    assert!(
        matches!(e, HarnessError::Session { .. }),
        "an EOF mid-turn is a session death, got {e:?}"
    );
}

#[tokio::test]
async fn g2_a_garbage_line_refuses_with_words_never_a_panic() {
    let (a, _g) = agent(
        "garbage",
        &format!("{HANDSHAKE}\nsys.stdout.write('this is not json\\n'); sys.stdout.flush()\n"),
    );
    let (_, _, error) = drain(a).await;
    let e = error.expect("garbage must refuse");
    assert!(
        e.to_string().contains("wire") || e.to_string().contains("JSON"),
        "{e}"
    );
}

#[tokio::test]
async fn g3_a_jsonrpc_error_response_becomes_a_session_error_with_its_message() {
    let (a, _g) = agent(
        "rpcerr",
        &format!(
            "{HANDSHAKE}\nsend({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\
             \"error\":{{\"code\":-32000,\"message\":\"the agent is out of quota\"}}}})\n"
        ),
    );
    let (_, _, error) = drain(a).await;
    let e = error.expect("an error response must surface");
    assert!(e.to_string().contains("out of quota"), "{e}");
}

#[tokio::test]
async fn g4_updates_for_another_session_are_ignored_never_mixed_in() {
    let (a, _g) = agent(
        "foreign",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"SOMEONE-ELSE\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\"text\":\"NOT OURS\"}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"s-g\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\"text\":\"ours\"}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    let (chunks, output, error) = drain(a).await;
    assert!(error.is_none(), "{error:?}");
    assert_eq!(
        chunks,
        vec!["ours"],
        "a foreign session's text must never mix in"
    );
    assert_eq!(output.as_deref(), Some("ours"));
}

#[tokio::test]
async fn g5_an_unknown_update_kind_is_observed_never_fatal() {
    let (a, _g) = agent(
        "future",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"s-g\",\"update\":{{\"sessionUpdate\":\"quantum_thought_v9\",\
             \"payload\":{{\"anything\":1}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    let (chunks, output, error) = drain(a).await;
    assert!(
        error.is_none(),
        "a future update kind must not kill the run: {error:?}"
    );
    assert!(chunks.is_empty());
    assert_eq!(output.as_deref(), Some(""));
}

#[tokio::test]
async fn g6_an_ask_storm_is_answered_deny_and_the_turn_still_ends() {
    let asks = (0..50)
        .map(|i| {
            format!(
                "send({{\"jsonrpc\":\"2.0\",\"id\":\"ask-{i}\",\"method\":\"session/request_permission\",\
                 \"params\":{{\"sessionId\":\"s-g\",\"toolCall\":{{\"title\":\"do {i}\"}},\
                 \"options\":[{{\"optionId\":\"o\",\"name\":\"Once\",\"kind\":\"allow_once\"}}]}}}})"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let (a, _g) = agent(
        "storm",
        &format!(
            "{HANDSHAKE}\n{asks}\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    let (_, output, error) = drain(a).await;
    assert!(
        error.is_none(),
        "an ask storm must not wedge the driver: {error:?}"
    );
    assert!(output.is_some(), "the turn still ends");
}

#[tokio::test]
async fn g7_an_oversized_line_refuses_at_the_bound_never_an_oom() {
    let (a, _g) = agent(
        "flood",
        &format!(
            "{HANDSHAKE}\n\
             sys.stdout.write('{{\"jsonrpc\":\"2.0\",\"x\":\"' + 'A'*2000000 + '\"}}\\n')\n\
             sys.stdout.flush()\n"
        ),
    );
    let (_, _, error) = drain(a).await;
    let e = error.expect("an oversized line must refuse");
    assert!(
        e.to_string().contains("bound") || e.to_string().contains("overflow"),
        "{e}"
    );
}

#[tokio::test]
async fn g8_a_spoofed_early_response_cannot_forge_the_turn_result() {
    // The agent answers the PROMPT id before the prompt is even sent
    // (a replay of our fixed correlation ids). The client must not
    // treat a pre-session frame as this turn's result.
    let (a, _g) = agent(
        "spoof",
        r#"
import json, sys
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def recv():
    return json.loads(sys.stdin.readline())
init = recv()
# Forge the PROMPT response (id 3) before anything legitimate.
send({"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}})
send({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1}})
new = recv()
send({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s-g"}})
prompt = recv()
send({"jsonrpc":"2.0","method":"session/update","params":{
    "sessionId":"s-g",
    "update":{"sessionUpdate":"agent_message_chunk",
              "content":{"type":"text","text":"real answer"}}}})
send({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}})
"#,
    );
    let (chunks, output, error) = drain(a).await;
    assert!(error.is_none(), "{error:?}");
    // The forged early frame must not have closed the turn: the REAL
    // chunk arrived and rode into the outcome.
    assert_eq!(chunks, vec!["real answer"]);
    assert_eq!(output.as_deref(), Some("real answer"));
}

#[tokio::test]
async fn g9_a_version_outside_the_pin_refuses_before_any_session() {
    let (base, _g) = agent(
        "oldver",
        "import sys\nif '--version' in sys.argv: print('oldver 0.1.0'); sys.exit(0)\nsys.exit(1)\n",
    );
    let script = base.args.first().cloned().expect("script arg");
    let adapter = base
        .clone()
        .with_version_pin(VersionPin::new((1, 0), 2))
        .with_version_args(vec![script, "--version".to_owned()]);
    let (_, output, error) = drain(adapter).await;
    assert!(output.is_none(), "no session may start");
    let e = error.expect("the pin must refuse");
    assert!(
        matches!(e, HarnessError::Unavailable { .. }),
        "an out-of-pin version is Unavailable, got {e:?}"
    );
    assert!(e.to_string().contains("0.1"), "{e}");
}

#[tokio::test]
async fn g10_a_version_inside_the_pin_proceeds_to_the_session() {
    let script = format!(
        "import sys\nif '--version' in sys.argv:\n    print('newver 1.4.0')\n    sys.exit(0)\n{HANDSHAKE}\n\
         send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
    );
    let (base, _g) = agent("newver", &script);
    let script = base.args.first().cloned().expect("script arg");
    let adapter = base
        .clone()
        .with_version_pin(VersionPin::new((1, 0), 2))
        .with_version_args(vec![script, "--version".to_owned()]);
    let (_, output, error) = drain(adapter).await;
    assert!(error.is_none(), "an in-pin adapter runs: {error:?}");
    assert!(output.is_some(), "the session completed");
}

/// **The crossing case** — the one the first gauntlet missed. G6 had
/// concurrent replies with tiny lines; G7 had a huge line with no
/// reply in flight. The bug lived in their PRODUCT: a line big enough
/// to straddle several `fill_buf` rounds WHILE a permission reply
/// races the read in the driver's `select!`. With a future-local
/// accumulator the partial bytes vanished and the frame parsed as
/// garbage (review 2026-08-06 · verified against tokio's own
/// `read_until` cancel-safety contract). The accumulator now lives on
/// the driver; this proves it.
#[tokio::test]
async fn g11_a_big_line_survives_a_concurrent_permission_reply() {
    // 400 KiB of payload: many pipe-buffer rounds, so the read future
    // is guaranteed to be mid-line when the reply lands.
    let (a, _g) = agent(
        "crossing",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":\"ask-x\",\"method\":\"session/request_permission\",\
             \"params\":{{\"sessionId\":\"s-g\",\"toolCall\":{{\"title\":\"cross\"}},\
             \"options\":[{{\"optionId\":\"o\",\"name\":\"Once\",\"kind\":\"allow_once\"}}]}}}})\n\
             big = 'B'*400000\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"s-g\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\"text\":big}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    let (chunks, output, error) = drain(a).await;
    assert!(
        error.is_none(),
        "a big frame crossing a reply must survive whole: {error:?}"
    );
    assert_eq!(chunks.len(), 1, "exactly one chunk");
    assert_eq!(
        chunks[0].len(),
        400_000,
        "not one byte may be eaten by the select's cancellation"
    );
    assert_eq!(output.as_deref().map(str::len), Some(400_000));
}
