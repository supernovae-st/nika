// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
struct Dropped(Arc<AtomicBool>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
#[test]
fn stop_drops_the_inflight_future_and_keeps_the_request_unknown() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut costs = PreparationCosts::default();
    let cancel = costs.begin_turn();
    let dropped = Arc::new(AtomicBool::new(false));
    let started = Arc::new(tokio::sync::Notify::new());
    {
        let _scope = costs.enter();
        runtime.block_on(async {
            let work = async {
                let _dropped = Dropped(dropped.clone());
                let _entry = crate::dispatch_journal::open().unwrap();
                crate::dispatch_journal::sent(Some(&call()));
                started.notify_one();
                std::future::pending::<u8>().await
            };
            let stop = async {
                started.notified().await;
                cancel.cancel();
            };
            let (result, ()) = tokio::join!(PreparationCosts::while_active(work), stop);
            assert_eq!(result, None);
        });
    }
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(costs.observation().unwrap()["state"], "Uncertain");
    assert_eq!(costs.uncertain_requests(), 1);
    let fresh = costs.begin_turn();
    assert!(!fresh.is_cancelled());
    let _scope = costs.enter();
    assert_eq!(
        runtime.block_on(PreparationCosts::while_active(async { 7 })),
        Some(7)
    );
    assert_eq!(
        costs.uncertain_requests(),
        1,
        "a fresh turn must not erase the old charge"
    );
}
#[test]
fn a_stop_received_before_poll_sends_nothing_and_a_late_result_is_discarded() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut costs = PreparationCosts::default();
    let token = costs.begin_turn();
    token.cancel();
    let polled = AtomicBool::new(false);
    {
        let _scope = costs.enter();
        assert_eq!(
            runtime.block_on(PreparationCosts::while_active(async {
                polled.store(true, Ordering::SeqCst);
                3
            })),
            None
        );
    }
    assert!(!polled.load(Ordering::SeqCst));
    assert!(costs.observation().is_none());
    let late = costs.begin_turn();
    let _scope = costs.enter();
    assert_eq!(
        runtime.block_on(PreparationCosts::while_active(async {
            late.cancel();
            5
        })),
        None,
        "Stop wins before the result is accepted"
    );
}
