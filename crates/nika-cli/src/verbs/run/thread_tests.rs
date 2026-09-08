// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Deterministic listener lifetime checks, without signalling the test runner.
#![allow(clippy::expect_used)]

use std::sync::mpsc;
use std::time::Duration;

use super::spawn_listener;

struct Dropped(mpsc::Sender<()>);

impl Drop for Dropped {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[test]
fn ending_a_run_cancels_and_joins_its_waiting_listener() {
    for already_cancelled in [false, true] {
        let (ready, started) = mpsc::channel();
        let (dropped, ended) = mpsc::channel();
        let cancel = nika_types::cancel::CancelCtx::new();
        let observed = cancel.clone();
        let listener = spawn_listener(async move {
            let _lifetime = Dropped(dropped);
            if already_cancelled {
                cancel.cancel();
            }
            ready.send(()).expect("ready");
            std::future::pending::<()>().await;
        })
        .expect("listener");
        started
            .recv_timeout(Duration::from_secs(5))
            .expect("listener running");
        assert_eq!(observed.is_cancelled(), already_cancelled);
        drop(listener);
        // Drop must join: completion is already observable, no retry/sleep.
        ended
            .try_recv()
            .expect("listener future dropped before returning");
    }
}

#[test]
fn unwinding_a_run_also_releases_its_listener() {
    let (ready, started) = mpsc::channel();
    let (dropped, ended) = mpsc::channel();
    let result = std::panic::catch_unwind(|| {
        let _listener = spawn_listener(async move {
            let _lifetime = Dropped(dropped);
            ready.send(()).expect("ready");
            std::future::pending::<()>().await;
        })
        .expect("listener");
        started
            .recv_timeout(Duration::from_secs(5))
            .expect("listener running");
        std::panic::resume_unwind(Box::new("simulated run unwind"));
    });
    assert!(result.is_err());
    ended.try_recv().expect("listener joined on unwind");
}
