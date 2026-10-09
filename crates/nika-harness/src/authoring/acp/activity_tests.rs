// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What an authoring call received after writing its prompt, through the real call over scripted
//! completion doors: paused virtual time, no installed harness, credential or model. The record
//! keeps closed categories, counts and times only, never a frame's text (the secret marker).
use super::deadline_tests::{
    Peer, SECRET, authoring, door, handshake, holds_initialize, observed, request, write,
};
use super::*;
use nika_kernel::ai::provider::{ContentBlock, InferResponse, ProviderError, ProviderInferDyn};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufReader};

/// The categories a call attributes to itself, then the ones it never does, in record order.
const SESSION: [&str; 8] = [
    "answer",
    "thought",
    "usage",
    "status",
    "other_update",
    "client_request",
    "prompt_result",
    "prompt_error",
];
const FOREIGN: [&str; 3] = ["other_session", "uncorrelated", "unreadable"];

const FIVE: Duration = Duration::from_secs(5);

/// One line a scripted peer writes after the prompt.
enum Line {
    /// A whole JSON-RPC frame.
    Frame(Value),
    /// The answer to the prompt, carrying this `result`.
    Result(Value),
    /// A JSON-RPC error answering the prompt.
    Error(Value),
    /// A line that is no JSON at all.
    Raw(String),
}

/// A `session/update` notification of `update` for `session`.
fn update(session: &str, update: &Value) -> Line {
    Line::Frame(json!({"jsonrpc":"2.0","method":"session/update",
        "params":{"sessionId":session,"update":update}}))
}

/// This session's thought chunk.
fn thought(text: &str) -> Line {
    let chunk =
        json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":text}});
    update("s", &chunk)
}

/// A peer that admits, opens session `s`, reads the prompt, writes each line of `script` after
/// its delay, then holds its transport open, or closes it when `close` is set.
fn scripted(script: Vec<(Duration, Line)>, close: bool) -> Peer {
    Box::new(move |theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let prompt = handshake(&mut BufReader::new(r), &mut w).await;
            for (delay, line) in script {
                tokio::time::sleep(delay).await;
                let id = prompt["id"].clone();
                let written = match line {
                    Line::Frame(frame) => write(&mut w, &frame).await,
                    Line::Result(result) => {
                        write(&mut w, &json!({"jsonrpc":"2.0","id":id,"result":result})).await
                    }
                    Line::Error(error) => {
                        write(&mut w, &json!({"jsonrpc":"2.0","id":id,"error":error})).await
                    }
                    Line::Raw(raw) => w.write_all(format!("{raw}\n").as_bytes()).await.is_ok(),
                };
                if !written {
                    return;
                }
            }
            if !close {
                std::future::pending::<()>().await;
            }
        })
    })
}

/// One authoring call through `peer` under the fixed 600 s deadline: its answer, and its
/// terminal record (the descriptor holds no secret marker anywhere).
async fn call(peer: Peer) -> (Result<InferResponse, ProviderError>, Value) {
    let door = door(Duration::ZERO, peer);
    let seat = authoring(&door);
    let answer = seat.infer(request(None)).await;
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "invoking, then terminal: {observed:?}");
    (answer, observed[1].clone())
}

/// Every category of `words`, zero unless `counted` names it.
fn frames(words: &[&str], counted: &[(&str, u32)]) -> Value {
    assert!(
        counted.iter().all(|(word, _)| words.contains(word)),
        "{counted:?}"
    );
    let count = |word: &str| {
        counted
            .iter()
            .find(|(w, _)| *w == word)
            .map_or(0, |(_, n)| *n)
    };
    Value::Object(
        words
            .iter()
            .map(|word| ((*word).to_owned(), json!(count(word))))
            .collect(),
    )
}

/// The activity a record must carry: the window opened at the written prompt (0 ms: the scripted
/// door spends no setup), this session's frames and when the last arrived, the frames it cannot
/// attribute to itself and when the last of those arrived, and what ended the driver.
fn activity(
    session: &[(&str, u32)],
    session_last: Option<u64>,
    foreign: &[(&str, u32)],
    foreign_last: Option<u64>,
    ended_by: Option<&str>,
) -> Value {
    json!({"from_ms": 0,
        "session": {"frames": frames(&SESSION, session), "last_ms": session_last},
        "foreign": {"frames": frames(&FOREIGN, foreign), "last_ms": foreign_last},
        "ended_by": ended_by})
}

/// The record without what this observation adds, and without its own elapsed time.
fn kept_before(record: &Value) -> Value {
    let mut kept = record.clone();
    if let Some(fields) = kept.as_object_mut() {
        fields.remove("activity");
        fields.remove("elapsed_ms");
    }
    kept
}

/// The blind spot, then its correction. A peer silent after the prompt and a peer that thinks and
/// reports usage and status without ever answering end alike at the same deadline: timed out
/// after the written prompt, nothing accepted, and every fact the record already kept reads the
/// same. What arrived after the prompt now tells them apart, by exact counts and time.
#[tokio::test(start_paused = true)]
async fn the_same_timed_out_prompt_tells_silence_from_unanswered_activity() {
    let minute = Duration::from_secs(60);
    let usage = json!({"sessionUpdate":"usage_update","used":53_000,"size":200_000});
    let info = json!({"sessionUpdate":"session_info_update","title":SECRET});
    let mode = json!({"sessionUpdate":"current_mode_update","currentModeId":"default"});
    let busy = vec![
        (minute, thought(&format!("weighing {SECRET}"))),
        (minute, update("s", &usage)),
        (minute, update("s", &info)),
        (minute, update("s", &mode)),
        (minute, thought(SECRET)),
    ];
    let (silent_answer, silent) = call(scripted(Vec::new(), false)).await;
    let (busy_answer, busy) = call(scripted(busy, false)).await;
    for (answer, record) in [(&silent_answer, &silent), (&busy_answer, &busy)] {
        assert!(
            matches!(answer, Err(ProviderError::Api { status: 408, .. })),
            "{answer:?}"
        );
        let stood = (
            &record["status"],
            &record["phase"],
            &record["last_milestone"],
            &record["answer_accepted"],
        );
        let expected = (
            &json!("timed_out"),
            &json!("session"),
            &json!("prompt_written"),
            &json!(false),
        );
        assert_eq!(stood, expected);
        let elapsed = record["elapsed_ms"].as_u64().expect("ms");
        assert!((600_000..601_000).contains(&elapsed), "{elapsed}");
    }
    // The blind spot: without what arrived after the prompt, the two records are one record.
    assert_eq!(kept_before(&silent), kept_before(&busy));
    // The correction: no completed frame at all, against five of this session's own.
    assert_eq!(silent["activity"], activity(&[], None, &[], None, None));
    let observed = activity(
        &[("thought", 2), ("usage", 1), ("status", 2)],
        Some(300_000),
        &[],
        None,
        None,
    );
    assert_eq!(busy["activity"], observed);
}

/// Another session's beats, a response to no request in flight and a notification of another
/// method never read as this call advancing: its own counts stay zero and its own last receipt
/// null, while the others are counted apart with their own time.
#[tokio::test(start_paused = true)]
async fn foreign_traffic_never_reads_as_this_call_advancing() {
    let ten = Duration::from_secs(10);
    let answer =
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":SECRET}});
    let thinking =
        json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":SECRET}});
    let late = json!({"jsonrpc":"2.0","id":99,"result":{"stopReason":"end_turn","note":SECRET}});
    let vendor = json!({"jsonrpc":"2.0","method":format!("_{SECRET}/ping"),
        "params":{"sessionId":"s"}});
    let script = vec![
        (ten, update("s-other", &answer)),
        (ten, update("s-other", &thinking)),
        (ten, Line::Frame(late)),
        (ten, Line::Frame(vendor)),
    ];
    let (answer, record) = call(scripted(script, false)).await;
    assert!(
        matches!(answer, Err(ProviderError::Api { status: 408, .. })),
        "{answer:?}"
    );
    let stood = (
        &record["status"],
        &record["phase"],
        &record["last_milestone"],
    );
    let expected = (
        &json!("timed_out"),
        &json!("session"),
        &json!("prompt_written"),
    );
    assert_eq!(stood, expected);
    let foreign = [("other_session", 2), ("uncorrelated", 2)];
    let observed = activity(&[], None, &foreign, Some(40_000), None);
    assert_eq!(record["activity"], observed);
}

/// A call whose script ends it after its prompt fails under `class`, records exactly `expected`,
/// and accepts nothing; a failed record names no stop, so no wire stop is spelled anywhere in it.
async fn ends(script: Vec<(Duration, Line)>, close: bool, class: &str, expected: &Value) {
    let (answer, record) = call(scripted(script, close)).await;
    assert!(answer.is_err(), "{expected}");
    let wire_stops = [
        "end_turn",
        "max_tokens",
        "max_turn_requests",
        "refusal",
        "cancelled",
    ];
    let text = record.to_string();
    assert!(wire_stops.iter().all(|stop| !text.contains(stop)), "{text}");
    let stood = (
        &record["status"],
        &record["failure"]["class"],
        &record["last_milestone"],
        &record["answer_accepted"],
    );
    let wanted = (
        &json!("failed"),
        &json!(class),
        &json!("prompt_written"),
        &json!(false),
    );
    assert_eq!(stood, wanted, "{expected}");
    assert_eq!(&record["activity"], expected);
}

/// One frame of this session, 5 s after the prompt, that ends the call as `ended_by`, counted
/// under `counted`.
async fn ends_on(line: Line, class: &str, counted: &str, ended_by: &str) {
    let expected = activity(&[(counted, 1)], Some(5_000), &[], None, Some(ended_by));
    ends(vec![(FIVE, line)], false, class, &expected).await;
}

/// Each frame the completion profile refuses after the prompt is named by its exact closed
/// category, where the safe words flatten every refusal into one sentence; nothing of the
/// frame's text, method, identifiers, paths or arguments is kept.
#[tokio::test(start_paused = true)]
async fn a_refused_frame_is_named_by_its_exact_category() {
    let tool = json!({"sessionUpdate":"tool_call","toolCallId":SECRET,"title":SECRET,
        "kind":"execute","rawInput":{"command":[SECRET]}});
    let image = json!({"sessionUpdate":"agent_message_chunk",
        "content":{"type":"image","mimeType":"image/png","data":SECRET}});
    let plan = json!({"sessionUpdate":"plan","entries":[{"content":SECRET}]});
    let permission = json!({"jsonrpc":"2.0","id":90,"method":"session/request_permission",
        "params":{"sessionId":"s","toolCall":{"title":SECRET},"options":[]}});
    let read_file = json!({"jsonrpc":"2.0","id":91,"method":"fs/read_text_file",
        "params":{"sessionId":"s","path":format!("/data/{SECRET}.txt")}});
    let long = json!({"sessionUpdate":"agent_message_chunk",
        "content":{"type":"text","text":format!("{SECRET}{}", "x".repeat(MAX_ANSWER))}});
    let stop = |word: String| Line::Result(json!({"stopReason":word}));
    let r = "refused";
    ends_on(stop("refusal".into()), r, "prompt_result", "turn_declined").await;
    ends_on(
        stop("max_tokens".into()),
        r,
        "prompt_result",
        "turn_token_limit",
    )
    .await;
    let turns = stop("max_turn_requests".into());
    ends_on(turns, r, "prompt_result", "turn_request_limit").await;
    ends_on(
        stop("cancelled".into()),
        r,
        "prompt_result",
        "turn_called_off",
    )
    .await;
    let weird = stop(format!("weird_{SECRET}"));
    ends_on(weird, r, "prompt_result", "turn_unknown_stop").await;
    ends_on(update("s", &tool), r, "other_update", "tool_update").await;
    ends_on(update("s", &image), r, "other_update", "media_update").await;
    ends_on(update("s", &plan), r, "other_update", "other_update").await;
    let asked = Line::Frame(permission);
    ends_on(asked, r, "client_request", "permission_request").await;
    ends_on(
        Line::Frame(read_file),
        r,
        "client_request",
        "client_request",
    )
    .await;
    ends_on(update("s", &long), r, "answer", "answer").await;
}

/// A session that ends after the prompt without a refusal keeps one safe sentence too; the record
/// names how: an error answering the prompt, the transport closing after a thought or failing the
/// write that denies a permission (the request still counted), or a line that is no message
/// (which this call cannot attribute to itself).
#[tokio::test(start_paused = true)]
async fn a_session_end_is_named_by_its_exact_category() {
    let error = Line::Error(json!({"code":-32603,"message":format!("internal {SECRET}")}));
    ends_on(error, "session", "prompt_error", "prompt_error").await;
    let closed = activity(&[("thought", 1)], Some(5_000), &[], None, Some("transport"));
    ends(vec![(FIVE, thought(SECRET))], true, "session", &closed).await;
    // The peer asks a permission and closes: the denial this side writes back cannot be written.
    let asked = Line::Frame(json!({"jsonrpc":"2.0","id":90,
        "method":"session/request_permission",
        "params":{"sessionId":"s","toolCall":{"title":SECRET},"options":[]}}));
    let denied = activity(
        &[("client_request", 1)],
        Some(5_000),
        &[],
        None,
        Some("transport"),
    );
    ends(vec![(FIVE, asked)], true, "session", &denied).await;
    let unreadable = activity(
        &[],
        None,
        &[("unreadable", 1)],
        Some(5_000),
        Some("unreadable"),
    );
    let raw = Line::Raw(format!("not json {SECRET}"));
    ends(vec![(FIVE, raw)], false, "session", &unreadable).await;
}

/// A completed answer is still accepted, unchanged, and its record says what preceded it and
/// that the turn closed.
#[tokio::test(start_paused = true)]
async fn a_completed_answer_records_what_preceded_and_closed_it() {
    let chunk =
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"{}"}});
    let script = vec![
        (FIVE, thought(SECRET)),
        (FIVE, update("s", &chunk)),
        (FIVE, Line::Result(json!({"stopReason":"end_turn"}))),
    ];
    let (answer, record) = call(scripted(script, false)).await;
    let answer = answer.expect("the completed answer is accepted");
    assert!(matches!(&answer.content[..], [ContentBlock::Text { text }] if text == "{}"));
    let stood = (
        &record["status"],
        &record["phase"],
        &record["last_milestone"],
        &record["stop_reason"],
    );
    let expected = (
        &json!("returned"),
        &json!("completion"),
        &json!("answer_chunk"),
        &json!(ACCEPTED_STOP),
    );
    assert_eq!(stood, expected);
    let observed = activity(
        &[("thought", 1), ("answer", 1), ("prompt_result", 1)],
        Some(15_000),
        &[],
        None,
        Some("completed"),
    );
    assert_eq!(record["activity"], observed);
}

/// A call that never wrote its prompt opens no window: its record says so with an explicit null.
#[tokio::test(start_paused = true)]
async fn a_call_that_never_wrote_its_prompt_records_no_activity() {
    let (answer, record) = call(holds_initialize()).await;
    assert!(answer.is_err());
    assert_eq!(
        (&record["status"], &record["last_milestone"]),
        (&json!("timed_out"), &json!("stream_opened"))
    );
    assert_eq!(record.get("activity"), Some(&Value::Null));
}
