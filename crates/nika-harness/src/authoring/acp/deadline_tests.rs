// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The authoring deadline against the completion transport, through the real authoring call over
//! a scripted completion door: paused virtual time, no installed harness, credential or model.
use super::*;
use crate::authoring::HarnessAuthoring;
use nika_kernel::ai::harness::{HarnessEventStream, HarnessOutcome};
use nika_kernel::ai::provider::{ContentBlock, InferRequest, Message, ProviderInferDyn, Role};
use nika_types::cancel::CancelCtx;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

const DEADLINE: Duration = Duration::from_secs(600);
const SECRET: &str = "sk-secret-marker-7731";

fn claude() -> OneShot {
    OneShot {
        role: Completion::Authoring,
        profile: Profile::ClaudeCode,
    }
}

type Peer = Box<dyn FnOnce(DuplexStream) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// One completion door over a scripted ACP peer: each open spends `setup` (the probes and the
/// spawn it stands for), opens nothing once the deadline has passed, then drives the peer under
/// the completion profile with what the deadline leaves, recording what it gave and did.
#[derive(Default)]
struct Scripted {
    setup: Duration,
    peer: Mutex<Option<Peer>>,
    unavailable: Option<String>,
    opens: AtomicUsize,
    abandoned: AtomicUsize,
    spawned: AtomicUsize,
    allowances: Mutex<Vec<Duration>>,
}

impl std::fmt::Debug for Scripted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Scripted")
    }
}

/// Counts an open abandoned before its setup finished (a Stop or the deadline dropped it).
struct Abandon<'a>(Option<&'a AtomicUsize>);

impl Drop for Abandon<'_> {
    fn drop(&mut self) {
        if let Some(counter) = self.0 {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Door for Arc<Scripted> {
    fn one_shot(&self) -> Option<OneShot> {
        Some(claude())
    }

    fn open(
        &self,
        request: HarnessRequest,
        deadline: Deadline,
        progress: Progress,
    ) -> Pin<Box<dyn Future<Output = Result<Opened, HarnessError>> + Send + '_>> {
        Box::pin(async move {
            self.opens.fetch_add(1, Ordering::SeqCst);
            let mut abandon = Abandon(Some(&self.abandoned));
            tokio::time::sleep(self.setup).await;
            abandon.0 = None;
            if let Some(reason) = &self.unavailable {
                return Err(HarnessError::Unavailable {
                    reason: reason.clone(),
                });
            }
            let allowance = Some(deadline.remaining())
                .filter(|left| !left.is_zero())
                .ok_or_else(|| HarnessError::Session {
                    reason: "expired before the spawn".into(),
                })?;
            let peer = self.peer.lock().unwrap().take().expect("one peer per door");
            self.spawned.fetch_add(1, Ordering::SeqCst);
            self.allowances.lock().unwrap().push(allowance);
            let (ours, theirs) = tokio::io::duplex(64 * 1024);
            tokio::spawn(peer(theirs));
            let (r, w) = tokio::io::split(ours);
            let marks = Some(progress);
            let stream =
                crate::client::drive_profile(r, w, request, allowance, Some(claude()), marks);
            Ok(Opened { stream, allowance })
        })
    }
}

fn door(setup: Duration, peer: Peer) -> Arc<Scripted> {
    Arc::new(Scripted {
        setup,
        peer: Mutex::new(Some(peer)),
        ..Scripted::default()
    })
}

fn authoring(door: &Arc<Scripted>) -> HarnessAuthoring {
    HarnessAuthoring::with_test_door("claude-code", Box::new(Arc::clone(door)))
}

fn request(cancel: Option<CancelCtx>) -> InferRequest {
    let mut request = InferRequest::new("session", vec![Message::text(Role::User, "draft")]);
    request.timeout = Some(DEADLINE);
    request.cancel = cancel;
    request
}

/// The receipt's observed records; no adapter text (the secret marker) anywhere in it.
fn observed(seat: &HarnessAuthoring) -> Vec<Value> {
    let receipt = seat.descriptor().expect("descriptor");
    assert!(!receipt.to_string().contains(SECRET), "{receipt}");
    receipt["observed"].as_array().cloned().expect("observed")
}

async fn read(r: &mut BufReader<tokio::io::ReadHalf<DuplexStream>>) -> Value {
    let mut line = String::new();
    r.read_line(&mut line).await.unwrap();
    serde_json::from_str(&line).unwrap_or(Value::Null)
}

async fn write(w: &mut tokio::io::WriteHalf<DuplexStream>, value: &Value) -> bool {
    w.write_all(format!("{value}\n").as_bytes()).await.is_ok()
}

/// The admitted identity and a plain session, then the prompt request the client sent.
async fn handshake(
    r: &mut BufReader<tokio::io::ReadHalf<DuplexStream>>,
    w: &mut tokio::io::WriteHalf<DuplexStream>,
) -> Value {
    let init = read(r).await;
    let identity = json!({"protocolVersion":1,"agentInfo":{"name":NAME,"version":VERSION}});
    write(
        w,
        &json!({"jsonrpc":"2.0","id":init["id"],"result":identity}),
    )
    .await;
    let new = read(r).await;
    let session = json!({"jsonrpc":"2.0","id":new["id"],"result":{"sessionId":"s"}});
    write(w, &session).await;
    read(r).await
}

/// The whole answer `{}`, then the prompt's end with `stop`.
async fn answer(w: &mut tokio::io::WriteHalf<DuplexStream>, prompt: &Value, stop: &str) -> bool {
    let chunk =
        json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"{}"}});
    let update = json!({"jsonrpc":"2.0","method":"session/update",
        "params":{"sessionId":"s","update":chunk}});
    let done = json!({"jsonrpc":"2.0","id":prompt["id"],"result":{"stopReason":stop}});
    write(w, &update).await && write(w, &done).await
}

/// A peer that admits, opens the session, stays silent for `silence`, then answers.
fn silent(silence: Duration) -> Peer {
    Box::new(move |theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let mut r = BufReader::new(r);
            let prompt = handshake(&mut r, &mut w).await;
            tokio::time::sleep(silence).await;
            answer(&mut w, &prompt, ACCEPTED_STOP).await;
        })
    })
}

/// A peer that reads the first request it is sent (`initialize`) and holds the transport
/// without a reply.
fn holds_initialize() -> Peer {
    Box::new(|theirs| {
        Box::pin(async move {
            let (r, _w) = tokio::io::split(theirs);
            let _ = read(&mut BufReader::new(r)).await;
            std::future::pending::<()>().await;
        })
    })
}

/// A peer that admits `initialize`, then reads `session/new` and holds it without a reply.
fn holds_session_new() -> Peer {
    Box::new(|theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let mut r = BufReader::new(r);
            let init = read(&mut r).await;
            let identity = json!({"protocolVersion":1,"agentInfo":{"name":NAME,"version":VERSION}});
            write(
                &mut w,
                &json!({"jsonrpc":"2.0","id":init["id"],"result":identity}),
            )
            .await;
            let _ = read(&mut r).await;
            std::future::pending::<()>().await;
        })
    })
}

/// A peer that admits, opens the session, reads the prompt (counting it) and stays silent.
fn holds_prompt(prompts: Arc<AtomicUsize>) -> Peer {
    Box::new(move |theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let prompt = handshake(&mut BufReader::new(r), &mut w).await;
            if prompt["method"] == "session/prompt" {
                prompts.fetch_add(1, Ordering::SeqCst);
            }
            std::future::pending::<()>().await;
        })
    })
}

/// A call that times out names the last protocol milestone this side closed, never more: a peer
/// that never answers `initialize` leaves it at the opened stream; a peer silent after the prompt
/// leaves it at the written prompt, the accepted initialize, the created session and the selection
/// checks behind it, with exactly one prompt sent. Either way nothing is accepted.
#[tokio::test(start_paused = true)]
async fn a_timed_out_call_names_the_last_protocol_milestone_it_closed() {
    let door_a = door(Duration::ZERO, holds_initialize());
    let seat = authoring(&door_a);
    assert!(seat.infer(request(None)).await.is_err());
    let stalled = observed(&seat)[1].clone();
    assert_eq!(
        (
            &stalled["status"],
            &stalled["phase"],
            &stalled["answer_accepted"]
        ),
        (&json!("timed_out"), &json!("session"), &json!(false))
    );
    assert_eq!(stalled["last_milestone"], "stream_opened");

    let door_i = door(Duration::ZERO, holds_session_new());
    let seat = authoring(&door_i);
    assert!(seat.infer(request(None)).await.is_err());
    assert_eq!(observed(&seat)[1]["last_milestone"], "initialize_accepted");

    let prompts = Arc::new(AtomicUsize::new(0));
    let door_b = door(Duration::ZERO, holds_prompt(Arc::clone(&prompts)));
    let seat = authoring(&door_b);
    assert!(seat.infer(request(None)).await.is_err());
    let stalled = observed(&seat)[1].clone();
    assert_eq!(
        (
            &stalled["status"],
            &stalled["phase"],
            &stalled["answer_accepted"]
        ),
        (&json!("timed_out"), &json!("session"), &json!(false))
    );
    assert_eq!(stalled["last_milestone"], "prompt_written");
    assert_eq!(prompts.load(Ordering::SeqCst), 1, "one prompt sent");
}

/// A selection the session refuses closes no milestone after the session: the call fails before
/// any prompt, never marked as checked or prompted.
#[tokio::test(start_paused = true)]
async fn a_refused_selection_is_never_marked_checked_or_prompted() {
    let door = door(Duration::ZERO, silent(Duration::ZERO));
    let seat = authoring(&door);
    // An explicit effort the plain session advertises no option for: refused before the prompt.
    let mut asked = request(None);
    asked.reasoning_effort = Some(nika_kernel::ai::provider::ReasoningEffort::High);
    assert!(seat.infer(asked).await.is_err());
    let refused = observed(&seat)[1].clone();
    assert_eq!(
        (&refused["status"], &refused["failure"]["class"]),
        (&json!("failed"), &json!("selection"))
    );
    assert_eq!(refused["last_milestone"], "session_created");
}

/// A peer that admits, opens the session and answers the prompt with a JSON-RPC `error`.
fn erring(error: Value) -> Peer {
    Box::new(move |theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let mut r = BufReader::new(r);
            let prompt = handshake(&mut r, &mut w).await;
            write(
                &mut w,
                &json!({"jsonrpc":"2.0","id":prompt["id"],"error":error}),
            )
            .await;
        })
    })
}

/// The whole answer of one stream, or the first error it reports.
async fn whole(mut stream: HarnessEventStream) -> Result<String, HarnessError> {
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        if let HarnessEvent::Completed { outcome } = event? {
            return Ok(outcome.output);
        }
    }
    Err(HarnessError::Session {
        reason: "the stream ended without a completed turn".into(),
    })
}

fn within(elapsed: Duration, expected: Duration) -> bool {
    elapsed >= expected && elapsed < expected + Duration::from_secs(1)
}

/// The incident's shape, discriminated: a valid answer silent for 450 s inside its 600 s deadline.
/// The former wiring handed every completion the fixed 300 s bound, which ends it at 300 s with
/// the pinned safe words; the authoring call now gives its transport what the deadline leaves.
#[tokio::test(start_paused = true)]
async fn a_valid_completion_silent_past_the_former_bound_completes_within_its_deadline() {
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (r, w) = tokio::io::split(ours);
    let former = tokio::spawn(silent(Duration::from_secs(450))(theirs));
    let started = tokio::time::Instant::now();
    let bound = Duration::from_secs(crate::IDLE_TIMEOUT_SECS);
    let request_once = HarnessRequest::new("p", "/tmp");
    let stream = crate::client::drive_profile(r, w, request_once, bound, Some(claude()), None);
    let ended = whole(stream).await;
    assert!(
        within(started.elapsed(), bound),
        "the fixed bound, unchanged"
    );
    assert_eq!(
        ended.as_ref().err().map(safe_error).as_deref(),
        Some("ACP authoring transport ended before a complete answer; no answer accepted")
    );
    former.abort();

    let door = door(Duration::ZERO, silent(Duration::from_secs(450)));
    let seat = authoring(&door);
    let started = tokio::time::Instant::now();
    let response = seat
        .infer(request(None))
        .await
        .expect("completes in its deadline");
    assert!(within(started.elapsed(), Duration::from_secs(450)));
    assert!(matches!(&response.content[..], [ContentBlock::Text { text }] if text == "{}"));
    assert_eq!(*door.allowances.lock().unwrap(), vec![DEADLINE]);
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "{observed:?}");
    let returned = &observed[1];
    assert_eq!(returned["status"], "returned");
    assert_eq!(returned["stop_reason"], json!(StopReason::EndTurn));
    assert_eq!(json!(StopReason::EndTurn), json!(ACCEPTED_STOP));
    assert_eq!(returned["phase"], "completion");
    assert_eq!(
        returned["bounds"],
        json!({"deadline_ms": 600_000, "transport_allowance_ms": 600_000})
    );
    assert_eq!(door.opens.load(Ordering::SeqCst), 1);
}

/// Probes and the spawn spend the same deadline: 100 s of setup leaves 500 s to the transport,
/// and the receipt names the value actually passed.
#[tokio::test(start_paused = true)]
async fn setup_time_reduces_the_allowance_the_transport_is_given() {
    let door = door(Duration::from_secs(100), silent(Duration::from_secs(450)));
    let seat = authoring(&door);
    seat.infer(request(None))
        .await
        .expect("100 + 450 s fit in 600 s");
    assert_eq!(
        *door.allowances.lock().unwrap(),
        vec![Duration::from_secs(500)]
    );
    let returned = &observed(&seat)[1];
    assert_eq!(returned["bounds"]["transport_allowance_ms"], 500_000);
    let elapsed = returned["elapsed_ms"].as_u64().expect("ms");
    assert!((550_000..551_000).contains(&elapsed), "{elapsed}");
}

/// Beyond the total deadline the call stays timed out: the late answer is never accepted, no
/// second call is made, and the record says where the call stood.
#[tokio::test(start_paused = true)]
async fn an_answer_beyond_the_deadline_stays_timed_out() {
    let door = door(Duration::from_secs(100), silent(Duration::from_secs(520)));
    let seat = authoring(&door);
    let refused = seat.infer(request(None)).await;
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("timed out; no answer accepted")),
        "{refused:?}"
    );
    assert!(
        matches!(
            &refused,
            Err(nika_kernel::ai::provider::ProviderError::Api { status: 408, .. })
        ),
        "{refused:?}"
    );
    // The established timeout form under its code: NIKA-330, where the untyped refusal said 339.
    let code = (refused.as_ref().err()).map(nika_error::traits::NikaErrorCode::nika_code);
    assert_eq!(code, Some(nika_kernel::ai::errors::NIKA_330));
    tokio::time::sleep(DEADLINE).await;
    let observed = observed(&seat);
    assert_eq!(observed.len(), 2, "no late record: {observed:?}");
    let timed_out = &observed[1];
    assert_eq!(timed_out["status"], "timed_out");
    assert_eq!(timed_out["answer_accepted"], false);
    assert_eq!(timed_out["phase"], "session");
    let elapsed = timed_out["elapsed_ms"].as_u64().expect("ms");
    assert!((600_000..601_000).contains(&elapsed), "{elapsed}");
    assert_eq!(timed_out["bounds"]["transport_allowance_ms"], 500_000);
    assert!(timed_out.get("stop_reason").is_none());
    assert_eq!(door.opens.load(Ordering::SeqCst), 1);
}

/// A deadline that passes during setup opens nothing: the setup is abandoned, no peer starts.
#[tokio::test(start_paused = true)]
async fn a_deadline_passed_during_setup_spawns_nothing() {
    let door = door(Duration::from_secs(700), silent(Duration::ZERO));
    let seat = authoring(&door);
    let refused = seat.infer(request(None)).await;
    assert!(
        matches!(
            &refused,
            Err(nika_kernel::ai::provider::ProviderError::Api { status: 408, .. })
        ),
        "{refused:?}"
    );
    let timed_out = &observed(&seat)[1];
    assert_eq!(timed_out["status"], "timed_out");
    assert_eq!(timed_out["phase"], "open");
    assert!(timed_out["bounds"]["transport_allowance_ms"].is_null());
    assert_eq!(door.abandoned.load(Ordering::SeqCst), 1);
    assert_eq!(door.spawned.load(Ordering::SeqCst), 0);
}

/// Stop during setup abandons the open: nothing is spawned and nothing is accepted.
#[tokio::test(start_paused = true)]
async fn stop_during_setup_spawns_nothing_and_accepts_nothing() {
    let door = door(Duration::from_secs(10_000), silent(Duration::ZERO));
    let seat = authoring(&door);
    let cancel = CancelCtx::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        stop.cancel();
    });
    let refused = seat.infer(request(Some(cancel))).await;
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("cancelled; no answer accepted")),
        "{refused:?}"
    );
    let cancelled = &observed(&seat)[1];
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(cancelled["answer_accepted"], false);
    assert_eq!(cancelled["phase"], "open");
    let elapsed = cancelled["elapsed_ms"].as_u64().expect("ms");
    assert!((5_000..=5_010).contains(&elapsed), "{elapsed}");
    assert_eq!(door.abandoned.load(Ordering::SeqCst), 1);
    assert_eq!(door.spawned.load(Ordering::SeqCst), 0);
}

/// Stop during a pending completion: the answer the peer sends afterwards is never accepted, and
/// the driver gives its transport up within the finite allowance it was given.
#[tokio::test(start_paused = true)]
async fn stop_during_a_pending_completion_accepts_no_late_answer() {
    let released = Arc::new(Mutex::new(None));
    let seen = Arc::clone(&released);
    let peer: Peer = Box::new(move |theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let mut r = BufReader::new(r);
            let prompt = handshake(&mut r, &mut w).await;
            tokio::time::sleep(Duration::from_secs(200)).await;
            answer(&mut w, &prompt, ACCEPTED_STOP).await;
            let mut rest = String::new();
            let eof = !matches!(r.read_line(&mut rest).await, Ok(n) if n > 0);
            *seen.lock().unwrap() = Some((eof, tokio::time::Instant::now()));
        })
    });
    let door = door(Duration::ZERO, peer);
    let seat = authoring(&door);
    let cancel = CancelCtx::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(100)).await;
        stop.cancel();
    });
    let started = tokio::time::Instant::now();
    assert!(seat.infer(request(Some(cancel))).await.is_err());
    tokio::time::sleep(Duration::from_secs(900)).await;
    let observed = observed(&seat);
    assert_eq!(
        observed.len(),
        2,
        "the late answer left no record: {observed:?}"
    );
    assert_eq!(observed[1]["status"], "cancelled");
    assert_eq!(observed[1]["phase"], "session");
    let (eof, at) = released.lock().unwrap().expect("the peer finished");
    assert!(eof, "the driver released the transport");
    assert!(
        at.duration_since(started) <= DEADLINE,
        "within its allowance"
    );
    assert_eq!(door.opens.load(Ordering::SeqCst), 1);
}

/// A completion door whose one turn is complete at the stream's first poll, which first sets a
/// Stop or holds the thread past the deadline: the answer is ready in the very poll that would
/// admit it.
struct Ready {
    cancel: Option<CancelCtx>,
    hold: Option<Duration>,
}

impl std::fmt::Debug for Ready {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ready")
    }
}

/// The one completed turn, yielded at the first poll after its side effect.
struct Turn {
    cancel: Option<CancelCtx>,
    hold: Option<Duration>,
    done: bool,
}

impl Stream for Turn {
    type Item = Result<HarnessEvent, HarnessError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        if self.done {
            return std::task::Poll::Ready(None);
        }
        self.done = true;
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        if let Some(hold) = self.hold {
            std::thread::sleep(hold);
        }
        let outcome = Box::new(HarnessOutcome::new("{}"));
        std::task::Poll::Ready(Some(Ok(HarnessEvent::Completed { outcome })))
    }
}

impl Door for Ready {
    fn one_shot(&self) -> Option<OneShot> {
        Some(claude())
    }

    fn open(
        &self,
        _: HarnessRequest,
        deadline: Deadline,
        _: Progress,
    ) -> Pin<Box<dyn Future<Output = Result<Opened, HarnessError>> + Send + '_>> {
        let stream: HarnessEventStream = Box::pin(Turn {
            cancel: self.cancel.clone(),
            hold: self.hold,
            done: false,
        });
        let allowance = deadline.remaining();
        Box::pin(async move { Ok(Opened { stream, allowance }) })
    }
}

/// Stop set while the completed answer is ready in the same poll: Stop wins.
#[tokio::test(start_paused = true)]
async fn a_stop_set_as_the_answer_becomes_ready_wins() {
    let cancel = CancelCtx::new();
    let ready = Ready {
        cancel: Some(cancel.clone()),
        hold: None,
    };
    let seat = HarnessAuthoring::with_test_door("claude-code", Box::new(ready));
    let refused = seat.infer(request(Some(cancel))).await;
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("cancelled; no answer accepted")),
        "{refused:?}"
    );
    assert_eq!(observed(&seat)[1]["status"], "cancelled");
}

/// The deadline passed while the completed answer became ready (its timer not yet fired, the
/// clock past it): the deadline wins. Real time: the answer's poll holds the thread 80 ms past a
/// 50 ms deadline.
#[tokio::test]
async fn a_deadline_passed_as_the_answer_becomes_ready_wins() {
    let ready = Ready {
        cancel: None,
        hold: Some(Duration::from_millis(80)),
    };
    let seat = HarnessAuthoring::with_test_door("claude-code", Box::new(ready));
    let mut late = request(None);
    late.timeout = Some(Duration::from_millis(50));
    let refused = seat.infer(late).await;
    assert!(
        matches!(&refused, Err(e) if e.to_string().contains("timed out; no answer accepted")),
        "{refused:?}"
    );
    assert!(
        matches!(
            &refused,
            Err(nika_kernel::ai::provider::ProviderError::Api { status: 408, .. })
        ),
        "{refused:?}"
    );
    assert_eq!(observed(&seat)[1]["status"], "timed_out");
}

/// Each failure keeps its closed typed identity before display flattens it, under its unchanged
/// safe words; the adapter's text (here a secret marker) never reaches the receipt.
#[tokio::test(start_paused = true)]
async fn a_failed_call_keeps_its_typed_identity_and_no_adapter_text() {
    let session = json!({"code":-32603,"message":format!("internal {SECRET}")});
    let signed_out = json!({"code":-32603,"message":format!("expired for {SECRET}"),
        "data":{"errorKind":"authentication_failed"}});
    let cases = [
        (
            session,
            "transport ended before a complete answer",
            json!({"class":"session","code":"NIKA-1804","transient":true,"cause":"unknown"}),
        ),
        (
            signed_out,
            "sign-in has expired or was revoked",
            json!({"class":"unavailable","code":"NIKA-1803","transient":false,
                "cause":"sign_in_expired"}),
        ),
    ];
    for (error, words, identity) in cases {
        let door = door(Duration::ZERO, erring(error));
        let seat = authoring(&door);
        let refused = seat.infer(request(None)).await.expect_err("no answer");
        assert!(refused.to_string().contains(words), "{refused}");
        assert!(!refused.to_string().contains(SECRET), "{refused}");
        let failed = &observed(&seat)[1];
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["answer_accepted"], false);
        assert_eq!(failed["failure"], identity);
        assert_eq!(failed["phase"], "session");
        assert!(failed.get("stop_reason").is_none(), "{failed}");
        assert_eq!(
            door.opens.load(Ordering::SeqCst),
            1,
            "no retry, no fallback"
        );
    }
}

/// A refusal during the handshake and an adapter that never opens keep their classes too.
#[tokio::test(start_paused = true)]
async fn a_refused_session_and_an_unopened_adapter_keep_their_classes() {
    let peer: Peer = Box::new(|theirs| {
        Box::pin(async move {
            let (r, mut w) = tokio::io::split(theirs);
            let mut r = BufReader::new(r);
            let init = read(&mut r).await;
            let identity = json!({"protocolVersion":1,"agentInfo":{"name":NAME,"version":VERSION}});
            write(
                &mut w,
                &json!({"jsonrpc":"2.0","id":init["id"],"result":identity}),
            )
            .await;
            let new = read(&mut r).await;
            let error = json!({"code":-32600,"message":format!("denied {SECRET}")});
            write(
                &mut w,
                &json!({"jsonrpc":"2.0","id":new["id"],"error":error}),
            )
            .await;
        })
    });
    let refusing = door(Duration::ZERO, peer);
    let seat = authoring(&refusing);
    assert!(seat.infer(request(None)).await.is_err());
    let failed = &observed(&seat)[1];
    let refused = json!({"class":"refused","code":"NIKA-1805","transient":false,"cause":"unknown"});
    assert_eq!(failed["failure"], refused);
    assert_eq!(failed["phase"], "session");

    let unopened = Arc::new(Scripted {
        unavailable: Some(format!("adapter said {SECRET}")),
        ..Scripted::default()
    });
    let seat = authoring(&unopened);
    assert!(seat.infer(request(None)).await.is_err());
    let failed = &observed(&seat)[1];
    let unavailable =
        json!({"class":"unavailable","code":"NIKA-1803","transient":false,"cause":"unknown"});
    assert_eq!(failed["failure"], unavailable);
    assert_eq!(failed["phase"], "open");
    assert!(failed["bounds"]["transport_allowance_ms"].is_null());
}

/// Only an accepted completion carries a stop reason: an unknown wire stop, or a truncation, is
/// refused with none, and the wire's own word never reaches the receipt.
#[tokio::test(start_paused = true)]
async fn success_and_an_unknown_stop_stay_distinct() {
    for stop in [format!("weird_{SECRET}"), "max_tokens".to_owned()] {
        let peer: Peer = Box::new(move |theirs| {
            Box::pin(async move {
                let (r, mut w) = tokio::io::split(theirs);
                let mut r = BufReader::new(r);
                let prompt = handshake(&mut r, &mut w).await;
                answer(&mut w, &prompt, &stop).await;
            })
        });
        let door = door(Duration::ZERO, peer);
        let seat = authoring(&door);
        assert!(seat.infer(request(None)).await.is_err());
        let failed = &observed(&seat)[1];
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["failure"]["class"], "refused");
        assert!(failed.get("stop_reason").is_none(), "{failed}");
        assert!(!failed.to_string().contains("max_tokens"), "{failed}");
        assert_eq!(failed["phase"], "answer");
    }
}
