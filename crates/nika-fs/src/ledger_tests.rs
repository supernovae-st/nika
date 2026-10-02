// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The room ledger proved on its own seam.
//!
//! A seal refuses every later registration and counts it, even one whose
//! budget was authorized before the seal (that budget comes back); refused
//! work is destroyed outside the lock, reservation included; a drain waits for
//! every operation any seal of its phase took, a second empty seal never
//! completes the first one's drain, a drain of an earlier phase never drains
//! the current one, a dropped join leaves the phase sealed for good, and a
//! panic is waited for, reported and latched on its phase and on the room; a
//! phase advances only after a completed drain; a write registers only in the
//! phase that authorized it; budgets are taken atomically; a claimed name is
//! evidence and keeps its usage, an unclaimed one returns its budget only once
//! cleaned, and a leftover is counted; the written snapshot never holds the
//! ledger.
//!
//! Every held operation goes through a bounded rendezvous, is released before
//! the assertions about it, and reports how its wait ended: a hold that its own
//! bound ended reads `HARNESS_INVALID`. Every witness thread is joined once it
//! has answered.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Waker};

use nika_kernel::fs::FsError;

use crate::room_harness::{Hold, answer_within, next_phase, roomy};
use crate::{EffectLedger, LedgerRefusal, Phase, RoomLimits};

#[tokio::test]
async fn a_seal_refuses_a_registration_that_arrives_after_it() {
    let ledger = roomy();
    ledger.seal_and_drain().await;
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let refused = ledger.run_blocking(move || flag.store(true, Ordering::SeqCst));
    tokio::task::yield_now().await;
    assert!(
        matches!(
            refused,
            Err(LedgerRefusal::Sealed {
                phase: Phase::Preparation
            })
        ),
        "{refused:?}"
    );
    assert!(
        !ran.load(Ordering::SeqCst),
        "a refused operation never runs"
    );
    assert_eq!(ledger.late_refusals(), 1);
}

#[tokio::test]
async fn a_drain_waits_for_an_operation_already_blocking_whose_caller_gave_up() {
    let ledger = roomy();
    let (mut hold, operation) = Hold::new();
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    let receiver = ledger
        .run_blocking(move || {
            operation();
            flag.store(true, Ordering::SeqCst);
        })
        .unwrap();
    drop(receiver);
    hold.entered();
    let mut joining = Box::pin(ledger.seal().join());
    let early = joining
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()));
    hold.release();
    hold.check();
    let held_back = early.is_pending();
    let drained = match early {
        Poll::Ready(drained) => drained,
        Poll::Pending => joining.await,
    };
    assert!(
        held_back,
        "the drain completed while its operation was held"
    );
    assert!(
        finished.load(Ordering::SeqCst),
        "the drain returned before the operation ended"
    );
    assert_eq!((drained.joined, drained.panicked), (1, 0));
}

#[tokio::test]
async fn a_registration_during_the_drain_is_refused_and_counted() {
    let ledger = roomy();
    let (mut hold, operation) = Hold::new();
    let pending = ledger.run_blocking(operation).unwrap();
    hold.entered();
    let drain = ledger.seal();
    let late = ledger.run_blocking(|| ());
    hold.release();
    hold.check();
    let drained = drain.join().await;
    assert!(
        matches!(
            late,
            Err(LedgerRefusal::Sealed {
                phase: Phase::Preparation
            })
        ),
        "{late:?}"
    );
    assert_eq!((drained.joined, ledger.late_refusals()), (1, 1));
    assert!(pending.await.is_ok());
}

#[tokio::test]
async fn a_second_seal_never_completes_a_drain_the_first_still_owes() {
    let ledger = roomy();
    let (mut hold, operation) = Hold::new();
    let pending = ledger.run_blocking(operation).unwrap();
    hold.entered();
    let first = ledger.seal();
    let second = ledger.seal().join().await;
    let early = ledger.advance();
    hold.release();
    hold.check();
    let owed = first.join().await;
    let after = ledger.advance();
    assert_eq!(second.joined, 0, "the second seal took nothing");
    assert!(
        matches!(
            early,
            Err(LedgerRefusal::Order {
                phase: Phase::Preparation
            })
        ),
        "an empty second drain completed the first one's: {early:?}"
    );
    assert_eq!((owed.joined, owed.panicked), (1, 0));
    assert_eq!(after, Ok(Phase::Run));
    assert!(pending.await.is_ok());
}

#[tokio::test]
async fn a_join_dropped_before_it_completes_leaves_the_phase_sealed_for_good() {
    let ledger = roomy();
    let (mut hold, operation) = Hold::new();
    let pending = ledger.run_blocking(operation).unwrap();
    hold.entered();
    let mut joining = Box::pin(ledger.seal().join());
    let early = joining
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()));
    drop(joining);
    hold.release();
    hold.check();
    let finished = pending.await;
    let after = ledger.advance();
    let again = ledger.seal_and_drain().await;
    let still = ledger.advance();
    let late = ledger.run_blocking(|| ());
    assert!(early.is_pending());
    assert!(finished.is_ok());
    assert!(
        matches!(after, Err(LedgerRefusal::Order { .. })),
        "{after:?}"
    );
    assert_eq!(again.joined, 0);
    assert!(
        matches!(still, Err(LedgerRefusal::Order { .. })),
        "{still:?}"
    );
    assert!(
        matches!(late, Err(LedgerRefusal::Sealed { .. })),
        "the phase stays sealed: {late:?}"
    );
}

#[tokio::test]
async fn a_drain_waits_for_every_operation_and_reports_a_panic() {
    let ledger = roomy();
    let (mut hold, operation) = Hold::new();
    let pending = ledger.run_blocking(operation).unwrap();
    let panicking = ledger
        .run_blocking::<(), _>(|| panic!("a blocking operation that panics"))
        .unwrap();
    hold.entered();
    let drain = ledger.seal();
    hold.release();
    hold.check();
    let drained = drain.join().await;
    assert_eq!(
        (drained.joined, drained.panicked),
        (2, 1),
        "both were waited for and the panic was reported"
    );
    assert!(pending.await.is_ok());
    assert!(
        panicking.await.is_err(),
        "a panicked operation yields no result"
    );
}

#[tokio::test]
async fn a_panic_stays_on_its_phase_and_the_room_after_a_later_empty_drain() {
    // A panic is latched: a later, empty drain of the same phase still reports it, the next
    // phase starts its own total, and the room's total keeps it for good.
    let ledger = roomy();
    let panicking = ledger
        .run_blocking::<(), _>(|| panic!("a blocking operation that panics"))
        .unwrap();
    let first = ledger.seal_and_drain().await;
    let second = ledger.seal_and_drain().await;
    let advanced = ledger.advance();
    let next = ledger.seal_and_drain().await;
    assert!(
        panicking.await.is_err(),
        "a panicked operation yields no result"
    );
    assert_eq!((first.joined, first.panicked), (1, 1));
    assert_eq!(
        (second.joined, second.panicked),
        (0, 1),
        "a later empty drain reported the phase clean"
    );
    assert_eq!(
        advanced,
        Ok(Phase::Run),
        "a drained phase advances: its panic is reported, never hidden"
    );
    assert_eq!((next.phase, next.joined, next.panicked), (Phase::Run, 0, 0));
    assert_eq!(
        ledger.panicked(),
        1,
        "the room's total never forgets the panic"
    );
}

#[tokio::test]
async fn a_drain_of_an_earlier_phase_never_drains_the_current_one() {
    // A seal taken in preparation and joined only once the run is sealed: its drain belongs
    // to its own phase and never marks the run drained.
    let ledger = roomy();
    let stale = ledger.seal();
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    let current = ledger.seal();
    let late = stale.join().await;
    let early = ledger.advance();
    let drained = current.join().await;
    let after = ledger.advance();
    assert_eq!(late.phase, Phase::Preparation);
    assert!(
        matches!(early, Err(LedgerRefusal::Order { phase: Phase::Run })),
        "a drain of an earlier phase drained the run: {early:?}"
    );
    assert_eq!(drained.phase, Phase::Run);
    assert_eq!(after, Ok(Phase::ReadBack));
}

#[tokio::test]
async fn a_phase_advances_only_after_a_completed_drain_and_never_back() {
    let ledger = roomy();
    let early = ledger.advance();
    assert!(
        matches!(early, Err(LedgerRefusal::Order { .. })),
        "{early:?}"
    );
    let drain = ledger.seal();
    let sealed = ledger.advance();
    assert!(
        matches!(sealed, Err(LedgerRefusal::Order { .. })),
        "sealed, not drained: {sealed:?}"
    );
    drain.join().await;
    assert_eq!(ledger.advance(), Ok(Phase::Run));
    assert_eq!(next_phase(&ledger).await, Phase::ReadBack);
    assert_eq!(next_phase(&ledger).await, Phase::Closed);
    ledger.seal_and_drain().await;
    let past = ledger.advance();
    assert!(matches!(past, Err(LedgerRefusal::Order { .. })), "{past:?}");
    let closed = ledger.run_blocking(|| ());
    assert!(
        matches!(
            closed,
            Err(LedgerRefusal::Sealed {
                phase: Phase::Closed
            })
        ),
        "{closed:?}"
    );
}

#[tokio::test]
async fn a_write_authorized_before_the_seal_is_refused_at_registration_and_returns_its_budget() {
    // The budget is taken (the write is authorized), then the phase seals before the write
    // registers: the registration is refused, nothing runs, and the dropped reservation
    // returns every byte it held to the next phase.
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    let authorized = ledger.reserve(10, 1).unwrap();
    let drain = ledger.seal();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let late = ledger.run_blocking(move || flag.store(true, Ordering::SeqCst));
    drop(authorized);
    let drained = drain.join().await;
    tokio::task::yield_now().await;
    assert!(
        matches!(
            late,
            Err(LedgerRefusal::Sealed {
                phase: Phase::Preparation
            })
        ),
        "{late:?}"
    );
    assert_eq!((drained.joined, ledger.late_refusals()), (0, 1));
    assert!(
        !ran.load(Ordering::SeqCst),
        "a refused operation never runs"
    );
    assert_eq!(ledger.advance(), Ok(Phase::Run));
    assert!(
        ledger.reserve(10, 1).is_ok(),
        "the refused write's budget came back"
    );
}

#[test]
fn a_refused_operation_that_owns_its_reservation_is_destroyed_outside_the_lock() {
    // The refused work owns the reservation, whose drop takes the ledger's lock again. The
    // refusal runs on a thread of its own and answers within a bound, so a ledger that
    // destroyed the work under its lock fails here instead of hanging the suite.
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    let owned = ledger.reserve(10, 1).unwrap();
    let sealed = ledger.seal();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let worker = Arc::clone(&ledger);
    let refused = answer_within(move || {
        worker
            .register(Some(Phase::Preparation), move || {
                drop(owned);
                flag.store(true, Ordering::SeqCst);
            })
            .map(|_| ())
    });
    drop(sealed);
    let refused = refused.expect("the refusal answered within the bound: no drop under the lock");
    assert!(
        matches!(
            refused,
            Err(LedgerRefusal::Sealed {
                phase: Phase::Preparation
            })
        ),
        "{refused:?}"
    );
    assert!(
        !ran.load(Ordering::SeqCst),
        "a refused operation never runs"
    );
    assert_eq!(ledger.late_refusals(), 1);
    assert!(
        ledger.reserve(10, 1).is_ok(),
        "the owned reservation came back"
    );
}

#[tokio::test]
async fn a_preparation_authorization_never_registers_in_the_run() {
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    let copy_in = ledger.reserve(10, 1).unwrap();
    let authorized = copy_in.phase();
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let late = ledger.register(Some(authorized), move || {
        drop(copy_in);
        flag.store(true, Ordering::SeqCst);
    });
    tokio::task::yield_now().await;
    assert_eq!(authorized, Phase::Preparation);
    assert!(
        matches!(
            late,
            Err(LedgerRefusal::Sealed {
                phase: Phase::Preparation
            })
        ),
        "{late:?}"
    );
    assert!(!ran.load(Ordering::SeqCst), "a stale write never runs");
    assert_eq!(ledger.late_refusals(), 1);
    assert!(ledger.reserve(10, 1).is_ok(), "its budget came back");
}

#[tokio::test]
async fn a_run_authorization_never_registers_after_the_run() {
    let ledger = roomy();
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    let run_write = ledger.reserve(1, 1).unwrap();
    let authorized = run_write.phase();
    assert_eq!(next_phase(&ledger).await, Phase::ReadBack);
    let late = ledger.register(Some(authorized), move || drop(run_write));
    assert!(
        matches!(late, Err(LedgerRefusal::Sealed { phase: Phase::Run })),
        "{late:?}"
    );
    assert_eq!(ledger.late_refusals(), 1);
}

#[test]
fn a_registration_outside_a_runtime_is_refused_without_running() {
    let ledger = roomy();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let refused = ledger.run_blocking(move || flag.store(true, Ordering::SeqCst));
    assert!(
        matches!(
            refused,
            Err(LedgerRefusal::NoRuntime {
                phase: Phase::Preparation
            })
        ),
        "{refused:?}"
    );
    assert!(!ran.load(Ordering::SeqCst));
    assert_eq!(
        ledger.late_refusals(),
        0,
        "a missing runtime is no late producer"
    );
}

#[test]
fn the_last_byte_goes_to_one_reservation_and_returns_when_dropped() {
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    let first = ledger.reserve(10, 1).unwrap();
    let second = ledger.reserve(1, 1);
    assert!(
        matches!(second, Err(LedgerRefusal::Budget { .. })),
        "{second:?}"
    );
    drop(first);
    let again = ledger.reserve(10, 1);
    assert!(
        again.is_ok(),
        "a dropped reservation returns its bytes: {again:?}"
    );
}

#[test]
fn a_committed_reservation_keeps_only_what_it_used() {
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    ledger.reserve(10, 2).unwrap().commit(4, 1);
    assert!(
        ledger.reserve(6, 9).is_ok(),
        "six bytes and nine files are left"
    );
    let over = ledger.reserve(7, 1);
    assert!(
        matches!(over, Err(LedgerRefusal::Budget { .. })),
        "{over:?}"
    );
}

#[test]
fn concurrent_reservations_never_overdraw_the_room() {
    let ledger = EffectLedger::new(RoomLimits::new(8, 64));
    let outcomes: Vec<_> = std::thread::scope(|scope| {
        let racers: Vec<_> = (0..16)
            .map(|_| scope.spawn(|| ledger.reserve(1, 1)))
            .collect();
        racers
            .into_iter()
            .map(|racer| racer.join().unwrap())
            .collect()
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 8);
}

#[tokio::test]
async fn only_a_run_phase_publish_is_write_evidence() {
    let ledger = roomy();
    ledger.published(Path::new("out/copied.txt"));
    assert!(
        ledger.written().next().is_none(),
        "a copy-in write is never evidence"
    );
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    ledger.published(Path::new("out/top.json"));
    assert_eq!(
        ledger.written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/top.json")]
    );
    assert_eq!(next_phase(&ledger).await, Phase::ReadBack);
    ledger.published(Path::new("out/late.json"));
    assert_eq!(
        ledger.written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/top.json")]
    );
}

#[tokio::test]
async fn a_claimed_name_keeps_its_evidence_and_usage_even_when_cleanup_fails() {
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    let reservation = ledger.reserve(10, 1).unwrap();
    let settled = ledger.settle(reservation, Path::new("out/top.json"), 10, Ok(()), false);
    let over = ledger.reserve(1, 1);
    assert!(settled.is_ok(), "{settled:?}");
    assert_eq!(
        ledger.written().collect::<Vec<_>>(),
        vec![PathBuf::from("out/top.json")]
    );
    assert!(
        matches!(over, Err(LedgerRefusal::Budget { bytes_left: 0, .. })),
        "the published usage stays committed: {over:?}"
    );
    assert_eq!(
        ledger.leftovers(),
        1,
        "a leftover temporary name is never clean"
    );
}

#[tokio::test]
async fn an_unclaimed_write_returns_its_budget_only_once_cleaned() {
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    let reservation = ledger.reserve(10, 1).unwrap();
    let failed = Err(FsError::Io {
        reason: "the claim failed".to_owned(),
    });
    let settled = ledger.settle(reservation, Path::new("out/top.json"), 10, failed, true);
    assert!(matches!(settled, Err(FsError::Io { .. })), "{settled:?}");
    assert!(
        ledger.written().next().is_none(),
        "an unclaimed name is no evidence"
    );
    assert!(ledger.reserve(10, 1).is_ok(), "the whole budget came back");
    assert_eq!(ledger.leftovers(), 0);
}

#[tokio::test]
async fn an_unclaimed_write_whose_cleanup_failed_keeps_its_usage_and_is_never_clean() {
    let ledger = EffectLedger::new(RoomLimits::new(10, 10));
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    let reservation = ledger.reserve(10, 1).unwrap();
    let failed = Err(FsError::Io {
        reason: "the claim failed".to_owned(),
    });
    let settled = ledger.settle(reservation, Path::new("out/top.json"), 10, failed, false);
    let over = ledger.reserve(1, 1);
    assert!(settled.is_err(), "{settled:?}");
    assert!(
        ledger.written().next().is_none(),
        "an unclaimed name is no evidence"
    );
    assert!(
        matches!(over, Err(LedgerRefusal::Budget { bytes_left: 0, .. })),
        "the bytes a leftover holds stay charged: {over:?}"
    );
    assert_eq!(ledger.leftovers(), 1);
}

#[tokio::test]
async fn the_written_snapshot_never_holds_the_ledger() {
    let ledger = roomy();
    assert_eq!(next_phase(&ledger).await, Phase::Run);
    ledger.published(Path::new("a.json"));
    let snapshot = ledger.written();
    let writer = Arc::clone(&ledger);
    let published = answer_within(move || writer.published(Path::new("b.json")));
    let seen: Vec<PathBuf> = snapshot.collect();
    assert!(
        published.is_ok(),
        "a publish waited on a live snapshot: {published:?}"
    );
    assert_eq!(seen, vec![PathBuf::from("a.json")]);
    assert_eq!(
        ledger.written().collect::<Vec<_>>(),
        vec![PathBuf::from("a.json"), PathBuf::from("b.json")]
    );
}
