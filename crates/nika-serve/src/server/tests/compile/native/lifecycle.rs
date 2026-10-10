// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A native round's lifetime (S19 · S18 P1-1, P1-2 and the attempt limit): a round admitted but
//! started after its deadline never reaches the seat, a round its caller gave up on never starts
//! later, a stopping server cancels and joins its rounds before it returns, and one logical call
//! the transport resends after a 503 is one call in the receipt and two requests on the wire.
//! The rounds author under the shared default strategy: the private plan, then the sketch door.

use super::super::super::super::test_support::CaptureAction;
use super::refusals::{admitted_again, one_slot, operator, wait_entered};
use super::*;

/// A park inside the first compile's blocking section: entered, then held until released.
fn park() -> (
    CaptureAction,
    oneshot::Receiver<()>,
    std::sync::mpsc::Sender<()>,
) {
    let (entered, entered_signal) = oneshot::channel::<()>();
    let (release, parked_until) = std::sync::mpsc::channel::<()>();
    let park: CaptureAction = Box::new(move || {
        let _sent = entered.send(());
        let _resumed = parked_until.recv();
    });
    (park, entered_signal, release)
}

fn kept(state: &AppState) -> (usize, usize) {
    state.native.as_ref().expect("a native seat").replays.held()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_admitted_but_started_after_its_deadline_never_reaches_the_seat() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        RUN_MODEL, false,
    )))]);
    let authoring = operator(&seat).with_deadline(Duration::from_millis(200));
    let (hold, entered, release) = park();
    let (server, _backend, state) =
        start_native_observed(&world, one_slot(), authoring, Some(hold)).await;
    let address = server.address;
    let pending =
        tokio::spawn(
            async move { wire_request(address, &compile_request(&fresh(&json!({})))).await },
        );
    entered.await.expect("the round was admitted and parked");
    // The deadline passes while the admitted round waits to start, well within the handoff.
    tokio::time::sleep(Duration::from_millis(400)).await;
    release.send(()).expect("the parked round is alive");
    let answered = pending.await.expect("the round answered");
    assert_eq!(answered.status, 408, "{}", answered.body);
    assert_eq!(
        answered.json()["error"]["code"],
        "compile_deadline_exceeded"
    );
    assert!(answered.header("nika-compile-replay").is_none());
    assert_eq!(
        seat.calls(),
        0,
        "a round that starts after its deadline sends nothing"
    );
    assert!(admitted_again(&server).await, "its slot returns");
    assert_eq!(kept(&state), (0, 0), "nothing kept, no place held");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_round_its_caller_gave_up_on_never_starts_later() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        RUN_MODEL, false,
    )))]);
    let authoring = operator(&seat).with_deadline(Duration::from_millis(100));
    let (hold, entered, release) = park();
    let (server, _backend, state) =
        start_native_observed(&world, one_slot(), authoring, Some(hold)).await;
    let address = server.address;
    let pending =
        tokio::spawn(
            async move { wire_request(address, &compile_request(&fresh(&json!({})))).await },
        );
    entered.await.expect("the round was admitted and parked");
    // The handler gives up past the deadline and its handoff while the round is still parked.
    let answered = pending.await.expect("the handler answered");
    assert_eq!(answered.status, 408, "{}", answered.body);
    assert_eq!(
        answered.json()["error"]["code"],
        "compile_deadline_exceeded"
    );
    assert_eq!(seat.calls(), 0);
    // Released after its caller heard « stopped »: it must not start now.
    release.send(()).expect("the parked round is alive");
    assert!(
        admitted_again(&server).await,
        "its slot returns once it ends"
    );
    assert_eq!(seat.calls(), 0, "no late spending after a terminal answer");
    assert_eq!(kept(&state), (0, 0), "no late outcome kept");
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stopping_server_cancels_and_joins_its_native_rounds_before_it_returns() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Parked(native_answer(&candidate(RUN_MODEL, false))),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ]);
    // A whole document with its model: an un-cancelled round would send its judgment once
    // released.
    let limits = compile_limits();
    let slots = limits.max_compile_requests();
    let (server, _backend, state) = start_native_observed(
        &world,
        limits,
        NativeAuthoring::new(SEAT, seat.providers()),
        None,
    )
    .await;
    let mut caller = tokio::net::TcpStream::connect(server.address)
        .await
        .expect("connect");
    caller
        .write_all(compile_request(&fresh(&json!({}))).as_bytes())
        .await
        .expect("request");
    tokio::task::block_in_place(|| wait_entered(&seat));
    let started = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(20), server.stop())
        .await
        .expect("shutdown settles in bounds")
        .expect("clean stop");
    assert!(started.elapsed() < Duration::from_secs(10));
    // The server returned while the seat still holds the first call: it cancelled, not waited.
    assert_eq!(
        state.compile_slots.available_permits(),
        slots,
        "every slot is back"
    );
    assert_eq!(kept(&state), (0, 0), "no round kept, no place held");
    seat.release.send(()).expect("the parked call is alive");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(seat.calls(), 1, "no further call after the server stopped");
    drop(caller);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_answer_ends_the_round_and_the_callers_retry_is_a_new_counted_round() {
    let world = TestWorld::new();
    let script = vec![
        Reply::Busy,
        Reply::Text(document_answer(DRAFT)),
        Reply::Text(JUDGE_APPROVES.to_owned()),
    ];
    let seat = Seat::start(script);
    // A three-request grant, stated for each round alone: the busy answer uses one of them and
    // ends the round; nothing re-sends it.
    let (server, _backend) =
        start_native(&world, compile_limits(), operator(&seat).with_max_calls(3)).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    assert_eq!(
        seat.bodies().len(),
        1,
        "one physical request, never re-sent"
    );
    let account = &document["provenance"]["authoring"]["backend"]["authority"];
    assert_eq!(account["http_requests"]["sent"], 1);
    assert_eq!(account["invocations"]["sent"], 1);
    assert_eq!(account["max_calls"], 3);
    let failure = provider_failure(&document);
    assert!(
        failure.contains("HTTP 503"),
        "the busy answer is reported: {failure}"
    );
    // The caller's retry is a new round under its own grant: the document and its judgment,
    // each counted where it was sent.
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    assert_eq!(document["provenance"]["authoring"]["calls"], 2);
    let account = &document["provenance"]["authoring"]["backend"]["authority"];
    assert_eq!(account["http_requests"]["sent"], 2);
    assert_eq!(account["invocations"]["sent"], 2);
    assert_eq!(
        seat.bodies().len(),
        3,
        "every physical request reached the seat once"
    );
    server.stop().await.expect("clean stop");
}
