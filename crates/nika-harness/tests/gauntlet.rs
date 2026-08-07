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
use nika_harness::{HarnessAdapter, MAX_LINE_BYTES, SpawnedHarness, VersionPin};
use nika_kernel::ai::harness::{
    AgentBackend as _, HarnessError, HarnessEvent, HarnessRequest, PermissionDecision,
};

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

/// What the engine surfaced of ONE permission ask — the question plus
/// B5's judged facts (kind · locations · command · url), carried back
/// so the test asserts the CONTRACT, never a log line.
type AskSeen = (
    String,
    Option<String>,
    Vec<String>,
    Vec<String>,
    Option<String>,
);

/// Drain a run with the engine's ANSWERING posture: every permission
/// ask gets `Some(decision)` — or its reply DROPPED on `None` (the
/// fail-closed half: an unanswered drop reads as Deny and the agent
/// hears `cancelled`, A-5's twin). Every ask's judged facts ride back.
async fn drain_verdict(
    adapter: HarnessAdapter,
    decision: Option<PermissionDecision>,
) -> (
    Vec<String>,
    Option<String>,
    Option<HarnessError>,
    Vec<AskSeen>,
) {
    let harness = SpawnedHarness::new(adapter);
    let mut stream = match harness.run_agent(HarnessRequest::new("go", "/tmp")).await {
        Ok(s) => s,
        Err(e) => return (Vec::new(), None, Some(e), Vec::new()),
    };
    let (mut chunks, mut output, mut error, mut asks) = (Vec::new(), None, None, Vec::new());
    loop {
        let next = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(HarnessEvent::MessageChunk { text })) => chunks.push(text),
            Some(Ok(HarnessEvent::Completed { outcome })) => output = Some(outcome.output.clone()),
            Some(Ok(HarnessEvent::PermissionAsked {
                question,
                reply,
                kind,
                locations,
                command,
                url,
            })) => {
                asks.push((question, kind, locations, command, url));
                match decision {
                    Some(d) => reply.respond(d),
                    // Unanswered: the guard's drop answers Deny (A-5 twin).
                    None => drop(reply),
                }
            }
            Some(Ok(_)) => {}
            Some(Err(e)) => error = Some(e),
            None => break,
        }
    }
    (chunks, output, error, asks)
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

/// g12 · session-routing × the reply lane — the PRODUCT g4 and the
/// bridge tests each held apart: a chunk for a DIFFERENT session id
/// rides in WHILE a permission ask is still unanswered. The router
/// must still drop the foreign text, and the engine's `AllowOnce` must
/// still reach the ask it answered — the child reports verbatim the
/// answer it read (its id, its outcome, its selected option), so a
/// mis-routed reply fails here with the child's own words.
#[tokio::test]
async fn g12_a_foreign_chunk_crossing_a_pending_ask_stays_out_of_the_outcome() {
    let (a, _g) = agent(
        "crossask",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":\"ask-g12\",\"method\":\"session/request_permission\",\
             \"params\":{{\"sessionId\":\"s-g\",\"toolCall\":{{\"title\":\"cross the lanes\"}},\
             \"options\":[{{\"optionId\":\"opt-once\",\"name\":\"Once\",\"kind\":\"allow_once\"}}]}}}})\n\
             # WHILE the ask hangs unanswered, another session's text.\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"SOMEONE-ELSE\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\"text\":\"FOREIGN\"}}}}}}}})\n\
             ans = recv()\n\
             outcome = ans[\"result\"][\"outcome\"]\n\
             verdict = ans[\"id\"] + \":\" + outcome[\"outcome\"] + \":\" + outcome.get(\"optionId\", \"\")\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"s-g\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\"text\":verdict}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    let (chunks, output, error, asks) = drain_verdict(a, Some(PermissionDecision::AllowOnce)).await;
    assert!(error.is_none(), "{error:?}");
    assert_eq!(asks.len(), 1, "exactly one ask rode in");
    // The reply-lane half: the answer routed to the RIGHT ask, and the
    // selected option is the allow_once the engine picked.
    assert_eq!(chunks, vec!["ask-g12:selected:opt-once"]);
    // The routing half: the foreign session's text never mixed in —
    // not in the chunks, not in the completed output.
    assert_eq!(output.as_deref(), Some("ask-g12:selected:opt-once"));
    assert!(
        !output.as_deref().unwrap_or_default().contains("FOREIGN"),
        "a foreign session's text must never ride the completed output"
    );
}

/// g13 · the reply lane × session death — the ask is IN FLIGHT when
/// the child dies: no clean close, no answer, no more frames. The EOF
/// must end the run as a Session error; the pending reply lane must
/// never wedge the driver waiting on a corpse. (The raw-seam wedge
/// tests prove the idle deadline at `FAST_IDLE` scale; a process exit
/// closes the pipe, so the death surfaces at once — the timeout is
/// the explicit bound on "never a hang".)
#[tokio::test]
async fn g13_a_death_mid_ask_is_a_session_death_never_a_wedge() {
    let (a, _g) = agent(
        "diemidask",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":\"ask-g13\",\"method\":\"session/request_permission\",\
             \"params\":{{\"sessionId\":\"s-g\",\"toolCall\":{{\"title\":\"die asking\"}},\
             \"options\":[{{\"optionId\":\"o\",\"name\":\"Once\",\"kind\":\"allow_once\"}}]}}}})\n\
             sys.exit(0)\n"
        ),
    );
    let (chunks, output, error) =
        tokio::time::timeout(std::time::Duration::from_secs(10), drain(a))
            .await
            .expect("a death mid-ask must end in bounded time, never hang");
    assert!(chunks.is_empty(), "a corpse speaks no text");
    assert!(output.is_none(), "a dead agent produces no outcome");
    let e = error.expect("the death must surface");
    assert!(
        matches!(e, HarnessError::Session { .. }),
        "an EOF with an ask in flight is a session death, got {e:?}"
    );
}

/// g14 · the ask storm × the size bound — g6 had fifty asks with tiny
/// lines, g7 had the flood with nothing in flight; their PRODUCT is
/// the point (the same lesson g11 pinned for the accumulator). Fifty
/// asks sit unanswered — their replies DROPPED, cancelled fail-closed
/// — while one line over the 1 MiB bound rides in behind them. The
/// bound must still kill the session with its own words, and the
/// reply lane must never wedge the driver while the flood accumulates.
#[tokio::test]
async fn g14_an_oversized_line_kills_the_session_even_with_fifty_asks_in_flight() {
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
        "stormflood",
        &format!(
            "{HANDSHAKE}\n{asks}\n\
             sys.stdout.write('{{\"jsonrpc\":\"2.0\",\"x\":\"' + 'A'*2000000 + '\"}}\\n')\n\
             sys.stdout.flush()\n"
        ),
    );
    let (_, output, error, asks) = drain_verdict(a, None).await;
    assert_eq!(
        asks.len(),
        50,
        "every ask surfaced before the flood hit the bound"
    );
    assert!(output.is_none(), "a killed session completes no turn");
    let e = error.expect("the oversized line must refuse");
    assert!(
        e.to_string().contains("bound"),
        "the bound's own words, even with fifty asks in flight: {e}"
    );
    assert!(
        e.to_string().contains(&MAX_LINE_BYTES.to_string()),
        "the refusal names the bound itself: {e}"
    );
}

/// g15 · mis-shaped JSON × the judge's facts — a `toolCall` whose
/// every field carries the WRONG TYPE still deserializes (the reader
/// is lenient by posture, never a panic): each fact extracts as
/// ABSENT, the question falls back to the generic words, and the
/// dropped reply still routes — the child reads `cancelled` and says
/// so on its own session.
#[tokio::test]
async fn g15_misshaped_toolcall_facts_extract_absent_and_the_answer_still_routes() {
    let (a, _g) = agent(
        "misshaped",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":\"ask-g15\",\"method\":\"session/request_permission\",\
             \"params\":{{\"sessionId\":\"s-g\",\
             \"toolCall\":{{\"title\":42,\"kind\":[\"execute\"],\"locations\":\"not-an-array\",\"rawInput\":None}},\
             \"options\":[{{\"optionId\":\"o\",\"name\":\"Once\",\"kind\":\"allow_once\"}}]}}}})\n\
             ans = recv()\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"s-g\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\
             \"text\":\"heard:\" + ans[\"result\"][\"outcome\"][\"outcome\"]}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    let (chunks, output, error, asks) = drain_verdict(a, None).await;
    assert!(
        error.is_none(),
        "mis-shaped facts are never fatal: {error:?}"
    );
    assert_eq!(asks.len(), 1, "exactly one ask rode in");
    let (question, kind, locations, command, url) = &asks[0];
    assert_eq!(
        question, "the harness asked to perform an action",
        "a non-string title falls back to the generic question"
    );
    assert!(kind.is_none(), "an array `kind` is absent, never guessed");
    assert!(locations.is_empty(), "a non-array `locations` is absent");
    assert!(command.is_empty(), "a null `rawInput` carries no argv");
    assert!(url.is_none(), "a null `rawInput` carries no url");
    // The answer still routed: the drop read cancelled, and the child
    // heard it with its own ears.
    assert_eq!(chunks, vec!["heard:cancelled"]);
    assert_eq!(output.as_deref(), Some("heard:cancelled"));
}

/// g16 · A-5 × the only-allow corner — `allow_always` is NEVER
/// selected, so an ask offering ONLY it must hear `cancelled` even
/// when the engine answers `AllowOnce`: the bridge finds no `allow_once`
/// to select and fails closed instead of promoting the always. The
/// child reports what the wire actually said.
#[tokio::test]
async fn g16_allow_once_against_an_only_allow_always_offer_cancels() {
    let (a, _g) = agent(
        "onlyalways",
        &format!(
            "{HANDSHAKE}\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":\"ask-g16\",\"method\":\"session/request_permission\",\
             \"params\":{{\"sessionId\":\"s-g\",\"toolCall\":{{\"title\":\"only always on offer\"}},\
             \"options\":[{{\"optionId\":\"opt-always\",\"name\":\"Always\",\"kind\":\"allow_always\"}}]}}}})\n\
             ans = recv()\n\
             send({{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{{\
             \"sessionId\":\"s-g\",\"update\":{{\"sessionUpdate\":\"agent_message_chunk\",\
             \"content\":{{\"type\":\"text\",\
             \"text\":\"heard:\" + ans[\"result\"][\"outcome\"][\"outcome\"]}}}}}}}})\n\
             send({{\"jsonrpc\":\"2.0\",\"id\":prompt[\"id\"],\"result\":{{\"stopReason\":\"end_turn\"}}}})\n"
        ),
    );
    // The engine says AllowOnce — and the wire must STILL say cancelled.
    let (chunks, output, error, asks) = drain_verdict(a, Some(PermissionDecision::AllowOnce)).await;
    assert!(error.is_none(), "{error:?}");
    assert_eq!(asks.len(), 1, "exactly one ask rode in");
    assert_eq!(
        chunks,
        vec!["heard:cancelled"],
        "allow_always is never selected, even when it is the only allow (A-5)"
    );
    assert_eq!(output.as_deref(), Some("heard:cancelled"));
}

/// g17 · the version pin × the authority lane — an adapter OUTSIDE
/// its pin that WOULD have asked a permission in its session never
/// gets the session the ask would ride: the refusal lands BEFORE the
/// spawn (identity before dialect, spec §4), so no session frame is
/// ever read. G9 pinned the refusal; this pins the no-traffic half —
/// the child writes a marker file if `session/new` ever arrives, and
/// the marker must never appear.
#[tokio::test]
async fn g17_an_out_of_pin_adapter_never_sees_a_session_frame() {
    let script = r#"
import json, os, sys
if '--version' in sys.argv:
    print('gauntlet-pincross 0.1.0')
    sys.exit(0)
def send(o):
    sys.stdout.write(json.dumps(o) + "\n"); sys.stdout.flush()
def recv():
    return json.loads(sys.stdin.readline())
init = recv()
send({"jsonrpc":"2.0","id":init["id"],"result":{"protocolVersion":1}})
new = recv()
# THE MARKER: if session/new ever arrives, the pin refused nothing.
marker = os.path.join(os.path.dirname(__file__), "session-new.seen")
with open(marker, "w") as f:
    f.write("session/new arrived")
send({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s-pin"}})
prompt = recv()
# The ask this adapter WOULD have made — the authority lane never opens.
send({"jsonrpc":"2.0","id":"ask-g17","method":"session/request_permission","params":{
    "sessionId":"s-pin","toolCall":{"title":"never asked"},
    "options":[{"optionId":"o","name":"Once","kind":"allow_once"}]}})
recv()
send({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":"end_turn"}})
"#;
    let (base, _g) = agent("pincross", script);
    let script_arg = base.args.first().cloned().expect("script arg");
    // The marker rides beside agent.py in the gauntlet tmpdir — both
    // sides derive it from the script's own path.
    let marker = std::path::Path::new(&script_arg).with_file_name("session-new.seen");
    let adapter = base
        .clone()
        .with_version_pin(VersionPin::new((1, 0), 2))
        .with_version_args(vec![script_arg, "--version".to_owned()]);
    let (_, output, error) = drain(adapter).await;
    assert!(output.is_none(), "no session may start");
    let e = error.expect("the pin must refuse");
    assert!(
        matches!(e, HarnessError::Unavailable { .. }),
        "an out-of-pin version is Unavailable, got {e:?}"
    );
    assert!(e.to_string().contains("0.1"), "{e}");
    assert!(
        !marker.exists(),
        "session/new must NEVER arrive: the refusal precedes the spawn"
    );
}
