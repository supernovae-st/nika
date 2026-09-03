// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's blocking driver: the tokio executor and the operator's signals.
//!
//! A `nika run` always listens (#1438). The first SIGINT (Ctrl-C) or, on
//! unix, SIGTERM flips the [`CancelCtx`] the runtime reads at every wave
//! boundary: in-flight work completes and is counted, the unstarted tasks
//! settle as cancelled by the operator, the run ends with ONE
//! `workflow_cancelled` terminal and the trace seals. A second signal ends
//! the process at once: the trace stays incomplete · the operator's choice,
//! said on stderr.
//!
//! The listener is a THREAD of its own, never a branch beside the run's
//! future: the run's task can sit in a long synchronous stretch (a wave
//! settling, a builtin computing, the next wave dispatching) and a listener
//! polled by that task would flip the context only when it next yields ·
//! one wave too late, measured by the e2e twin of `cancel_gate`.

use nika_types::cancel::CancelCtx;

use super::RunVerdict;

pub(super) fn block_on_run<F>(
    runtime: &tokio::runtime::Runtime,
    future: F,
    cancel: &CancelCtx,
) -> RunVerdict
where
    F: std::future::Future<Output = RunVerdict>,
{
    listen(cancel.clone());
    runtime.block_on(future)
}

/// Hear the operator on a thread of its own: the first signal flips the
/// context and says what happens next; the second ends the process with
/// the cancelled class. A listener that cannot start says so once and the
/// run then ends the way the platform ends it, never a silent hang.
fn listen(cancel: CancelCtx) {
    let spawned = std::thread::Builder::new()
        .name("nika-signals".to_owned())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                eprintln!("nika run: cannot listen for Ctrl-C: no executor for the listener");
                return;
            };
            rt.block_on(async {
                operator_signal().await;
                cancel.cancel();
                eprintln!(
                    "nika run: cancelling · in-flight work completes and is counted · \
                     unstarted tasks are cancelled · Ctrl-C again to abort"
                );
                operator_signal().await;
                eprintln!(
                    "nika run: aborted · the trace is incomplete (the run was cut mid-flight)"
                );
                std::process::exit(i32::from(crate::verbs::exit::CANCELLED));
            });
        });
    if let Err(error) = spawned {
        eprintln!("nika run: cannot listen for Ctrl-C: {error}");
    }
}

/// Resolves on the operator's next SIGINT (Ctrl-C) or, on unix, SIGTERM. A
/// listener that cannot be installed says so once and never resolves.
async fn operator_signal() {
    let interrupt = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {}
            Err(error) => {
                eprintln!("nika run: cannot listen for Ctrl-C: {error}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).ok();
        let terminate = async {
            match term.as_mut() {
                Some(term) => {
                    term.recv().await;
                }
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            () = interrupt => {}
            () = terminate => {}
        }
    }
    #[cfg(not(unix))]
    interrupt.await;
}
