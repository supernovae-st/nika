// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A native round's lifetime (S19 · S18 P1-1, P1-2 and the attempt limit): a round admitted but
//! started after its deadline never reaches the seat, a round its caller gave up on never starts
//! later, a stopping server cancels and joins its rounds before it returns, and one logical call
//! the transport resends after a 503 is one call in the receipt and two requests on the wire.

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
    let broken = native_answer("nika: broken\ntasks: {}\n");
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Parked(broken.clone()), Reply::Text(broken)]);
    // One repair allowed: an un-cancelled round would call again once released.
    let limits = compile_limits();
    let slots = limits.max_compile_requests();
    let (server, _backend, state) =
        start_native_observed(&world, limits, operator(&seat), None).await;
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
    assert_eq!(seat.calls(), 1, "no repair call after the server stopped");
    drop(caller);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_logical_call_the_transport_resends_after_a_503_is_one_call_in_the_receipt() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![
        Reply::Busy,
        Reply::Text(native_answer(&candidate(RUN_MODEL, false))),
    ]);
    let (server, _backend) =
        start_native(&world, compile_limits(), operator(&seat).with_repairs(0)).await;
    let response = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(response.status, 200, "{}", response.body);
    let document = response.json();
    assert_eq!(document["status"], "ready", "{document:#}");
    // One logical call (the gate's budget, the receipt's count) · two HTTP requests on the wire,
    // the same request resent by the provider transport after the 503.
    assert_eq!(document["provenance"]["authoring"]["calls"], 1);
    let bodies = seat.bodies();
    assert_eq!(bodies.len(), 2, "503 then 200");
    assert_eq!(bodies[0], bodies[1], "the same request resent");
    server.stop().await.expect("clean stop");
}
