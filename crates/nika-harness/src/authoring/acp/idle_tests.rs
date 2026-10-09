// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The authoring deadline bounds a call's silence, not its duration, through the real authoring
//! call over scripted completion doors: paused virtual time, no installed harness, credential or
//! model. A call whose agent keeps working outlives its allowance; one that goes silent times out
//! one allowance after its last activity frame and says so; Stop still ends a working call at
//! once; and no other session's or call's frames, nor adapter bookkeeping, re-arm it.
use super::deadline_tests::{Peer, SECRET, authoring, door, handshake, observed, request, write};
use super::*;
use nika_kernel::ai::provider::{ContentBlock, ProviderError, ProviderInferDyn};
use nika_types::cancel::CancelCtx;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::Instant;

const HUNDRED: Duration = Duration::from_secs(100);

/// What a scripted peer writes after the prompt.
#[derive(Clone)]
enum Beat {
    /// A `session/update` notification of `session` carrying `update`.
    Update(&'static str, Value),
    /// The whole answer `{}`, then the end of the prompt's turn.
    Answer,
}

/// A thought chunk of `session`, its text the secret marker.
fn thought(session: &'static str) -> Beat {
    let chunk =
        json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":SECRET}});
    Beat::Update(session, chunk)
}

/// `count` copies of `beat`, one every `gap`.
fn every(count: usize, gap: Duration, beat: &Beat) -> Vec<(Duration, Beat)> {
    vec![(gap, beat.clone()); count]
}

/// What a scripted peer saw: the prompts it was sent, and when its input ended.
#[derive(Default)]
struct Seen {
    prompts: AtomicUsize,
    released: Mutex<Option<Instant>>,
}

/// A peer that admits, opens session `s`, reads the prompt (counting it), writes each beat of
/// `script` after its delay, then reads its input until the end of file and notes when it came.
fn peer(script: Vec<(Duration, Beat)>, seen: Arc<Seen>) -> Peer {
    Box::new(move |theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let mut r = BufReader::new(r);
            let prompt = handshake(&mut r, &mut w).await;
            if prompt["method"] == "session/prompt" {
                seen.prompts.fetch_add(1, Ordering::SeqCst);
            }
            for (delay, beat) in script {
                tokio::time::sleep(delay).await;
                let (session, update, ends) = match beat {
                    Beat::Update(session, update) => (session, update, false),
                    Beat::Answer => {
                        let chunk = json!({"sessionUpdate":"agent_message_chunk",
                            "content":{"type":"text","text":"{}"}});
                        ("s", chunk, true)
                    }
                };
                let frame = json!({"jsonrpc":"2.0","method":"session/update",
                    "params":{"sessionId":session,"update":update}});
                let mut written = write(&mut w, &frame).await;
                if ends {
                    let done = json!({"jsonrpc":"2.0","id":prompt["id"],
                        "result":{"stopReason":ACCEPTED_STOP}});
                    written = written && write(&mut w, &done).await;
                }
                if !written {
                    return;
                }
            }
            let mut rest = String::new();
            while matches!(r.read_line(&mut rest).await, Ok(n) if n > 0) {
                rest.clear();
            }
            *seen.released.lock().unwrap() = Some(Instant::now());
        })
    })
}

/// Every category of `words`, zero unless `counted` names it.
fn frames(words: &[&str], counted: &[(&str, u32)]) -> Value {
    let count = |word: &str| {
        let named = counted.iter().find(|(w, _)| *w == word);
        named.map_or(0, |(_, n)| *n)
    };
    let pairs = words.iter().map(|w| ((*w).to_owned(), json!(count(w))));
    Value::Object(pairs.collect())
}

/// The activity a record must carry, its window opened at the written prompt (0 ms: the scripted
/// door spends no setup): this session's frames and the last one's time, the other frames and the
/// last one's time, and what ended the driver.
fn activity(
    session: (&[(&str, u32)], Option<u64>),
    foreign: (&[(&str, u32)], Option<u64>),
    ended_by: Option<&str>,
) -> Value {
    let own = [
        "answer",
        "thought",
        "usage",
        "status",
        "other_update",
        "client_request",
        "prompt_result",
        "prompt_error",
    ];
    let others = ["other_session", "uncorrelated", "unreadable"];
    json!({"from_ms": 0,
        "session": {"frames": frames(&own, session.0), "last_ms": session.1},
        "foreign": {"frames": frames(&others, foreign.0), "last_ms": foreign.1},
        "ended_by": ended_by})
}

/// The bounds a record must carry: no total deadline, the 600 s silence allowance, when an
/// activity frame last re-armed it, and the whole allowance given to the transport.
fn bounds(rearmed_ms: Option<u64>) -> Value {
    json!({"deadline_ms": null, "idle_ms": 600_000, "rearmed_ms": rearmed_ms,
        "transport_allowance_ms": 600_000})
}

/// A call whose agent keeps showing work streams thoughts past the former 600 s total and then
/// answers: the answer is accepted at 1450 s, and the record keeps that duration, the allowance
/// and the answer's frame as the last re-arming, with every frame counted. One prompt, one call.
#[tokio::test(start_paused = true)]
async fn a_call_streaming_activity_past_the_former_total_completes() {
    let seen = Arc::new(Seen::default());
    let mut script = every(14, HUNDRED, &thought("s"));
    script.push((Duration::from_secs(50), Beat::Answer));
    let door = door(Duration::ZERO, peer(script, Arc::clone(&seen)));
    let seat = authoring(&door);
    let response = seat.infer(request(None)).await;
    let response = response.expect("a call that keeps working is not cut at 600 s");
    assert!(matches!(&response.content[..], [ContentBlock::Text { text }] if text == "{}"));
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "invoking, then returned: {observed:?}");
    let returned = &observed[1];
    let stood = (
        &returned["status"],
        &returned["phase"],
        &returned["last_milestone"],
        &returned["stop_reason"],
    );
    let expected = (
        &json!("returned"),
        &json!("completion"),
        &json!("answer_chunk"),
        &json!(ACCEPTED_STOP),
    );
    assert_eq!(stood, expected);
    assert_eq!(returned["elapsed_ms"], 1_450_000);
    assert_eq!(returned["bounds"], bounds(Some(1_450_000)));
    let own = [("thought", 14), ("answer", 1), ("prompt_result", 1)];
    let received = activity((&own, Some(1_450_000)), (&[], None), Some("completed"));
    assert_eq!(returned["activity"], received);
    assert_eq!(seen.prompts.load(Ordering::SeqCst), 1);
}

/// The same agent going silent after its ninth thought: the call times out one allowance after
/// that thought, at 1500 s, in the established timeout form, and its record says how long it ran,
/// when the last activity frame came (900 s) and the allowance it kept silent for. The transport
/// is given up at that same instant.
#[tokio::test(start_paused = true)]
async fn a_call_that_goes_silent_times_out_one_allowance_after_its_last_frame() {
    let seen = Arc::new(Seen::default());
    let script = every(9, HUNDRED, &thought("s"));
    let door = door(Duration::ZERO, peer(script, Arc::clone(&seen)));
    let seat = authoring(&door);
    let started = Instant::now();
    let refused = seat.infer(request(None)).await;
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("timed out; no answer accepted")),
        "{refused:?}"
    );
    assert!(
        matches!(&refused, Err(ProviderError::Api { status: 408, .. })),
        "{refused:?}"
    );
    let code = (refused.as_ref().err()).map(nika_error::traits::NikaErrorCode::nika_code);
    assert_eq!(code, Some(nika_kernel::ai::errors::NIKA_330));
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "invoking, then timed out: {observed:?}");
    let timed_out = &observed[1];
    let stood = (
        &timed_out["status"],
        &timed_out["phase"],
        &timed_out["last_milestone"],
        &timed_out["answer_accepted"],
    );
    let expected = (
        &json!("timed_out"),
        &json!("session"),
        &json!("prompt_written"),
        &json!(false),
    );
    assert_eq!(stood, expected);
    assert_eq!(timed_out["elapsed_ms"], 1_500_000);
    assert_eq!(timed_out["bounds"], bounds(Some(900_000)));
    let received = activity((&[("thought", 9)], Some(900_000)), (&[], None), None);
    assert_eq!(timed_out["activity"], received);
    assert!(timed_out.get("stop_reason").is_none(), "{timed_out}");
    tokio::time::sleep(Duration::from_secs(1)).await;
    let released = *seen.released.lock().unwrap();
    assert_eq!(released, Some(started + Duration::from_secs(1500)));
    assert_eq!(seen.prompts.load(Ordering::SeqCst), 1);
}

/// Stop while the agent is streaming past the former total still ends the call at once (within
/// the Stop poll), with the activity it had shown; the answer the agent sends 50 s later is never
/// accepted or recorded, and no second prompt or call is made.
#[tokio::test(start_paused = true)]
async fn stop_during_a_streaming_call_stops_promptly_and_accepts_no_late_answer() {
    let seen = Arc::new(Seen::default());
    let ten = Duration::from_secs(10);
    let mut script = every(74, ten, &thought("s"));
    script.push((ten, Beat::Answer));
    let door = door(Duration::ZERO, peer(script, Arc::clone(&seen)));
    let seat = authoring(&door);
    let cancel = CancelCtx::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(700)).await;
        stop.cancel();
    });
    let started = Instant::now();
    let refused = seat.infer(request(Some(cancel))).await;
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("cancelled; no answer accepted")),
        "{refused:?}"
    );
    tokio::time::sleep(Duration::from_secs(200)).await;
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "no late record: {observed:?}");
    let cancelled = &observed[1];
    let stood = (
        &cancelled["status"],
        &cancelled["phase"],
        &cancelled["last_milestone"],
        &cancelled["answer_accepted"],
    );
    let expected = (
        &json!("cancelled"),
        &json!("session"),
        &json!("prompt_written"),
        &json!(false),
    );
    assert_eq!(stood, expected);
    let elapsed = cancelled["elapsed_ms"].as_u64().expect("ms");
    assert!((700_000..=700_010).contains(&elapsed), "{elapsed}");
    assert_eq!(cancelled["bounds"], bounds(Some(700_000)));
    let received = activity((&[("thought", 70)], Some(700_000)), (&[], None), None);
    assert_eq!(cancelled["activity"], received);
    assert_eq!(seen.prompts.load(Ordering::SeqCst), 1);
    let released = *seen.released.lock().unwrap();
    let answered = started + Duration::from_secs(750);
    assert_eq!(released, Some(answered), "released at the turn's end");
}

/// One call through `script` that must time out 600 s after its start, nothing having re-armed
/// its deadline: its terminal record.
async fn never_rearmed(script: Vec<(Duration, Beat)>) -> Value {
    let door = door(Duration::ZERO, peer(script, Arc::new(Seen::default())));
    let seat = authoring(&door);
    let refused = seat.infer(request(None)).await;
    assert!(
        matches!(&refused, Err(ProviderError::Api { status: 408, .. })),
        "{refused:?}"
    );
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "invoking, then timed out: {observed:?}");
    let timed_out = observed[1].clone();
    assert_eq!(timed_out["status"], "timed_out");
    assert_eq!(timed_out["elapsed_ms"], 600_000);
    assert_eq!(timed_out["bounds"], bounds(None));
    timed_out
}

/// Another session's thoughts on the same connection, then this session's usage and status
/// updates (adapter bookkeeping), arrive every 50 s for 550 s: none shows this call's agent
/// working, so each call still times out 600 s after its start, every frame counted.
#[tokio::test(start_paused = true)]
async fn another_session_and_adapter_bookkeeping_never_rearm_the_deadline() {
    let fifty = Duration::from_secs(50);
    let record = never_rearmed(every(11, fifty, &thought("s-other"))).await;
    let foreign = (&[("other_session", 11)][..], Some(550_000));
    assert_eq!(record["activity"], activity((&[], None), foreign, None));

    let usage = json!({"sessionUpdate":"usage_update","used":53_000,"size":200_000});
    let commands = json!({"sessionUpdate":"available_commands_update","availableCommands":[]});
    let mut bookkeeping = every(6, fifty, &Beat::Update("s", usage));
    bookkeeping.extend(every(5, fifty, &Beat::Update("s", commands)));
    let record = never_rearmed(bookkeeping).await;
    let own = (&[("usage", 6), ("status", 5)][..], Some(550_000));
    assert_eq!(record["activity"], activity(own, (&[], None), None));
}

/// Two calls at once, each over its own door: one agent thinks every 100 s and answers at 950 s,
/// the other stays silent after its prompt. The working call's frames never re-arm the silent
/// one, which times out 600 s after its start with no frame of its own, while the working call
/// completes.
#[tokio::test(start_paused = true)]
async fn another_call_activity_never_rearms_this_call() {
    let mut script = every(9, HUNDRED, &thought("s"));
    script.push((Duration::from_secs(50), Beat::Answer));
    let working = door(Duration::ZERO, peer(script, Arc::new(Seen::default())));
    let working = authoring(&working);
    let (answered, silent) = tokio::join!(working.infer(request(None)), never_rearmed(Vec::new()));
    answered.expect("the working call completes at 950 s");
    assert_eq!(observed(&working)[1]["elapsed_ms"], 950_000);
    assert_eq!(silent["activity"], activity((&[], None), (&[], None), None));
}
