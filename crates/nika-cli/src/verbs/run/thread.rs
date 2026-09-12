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
    let _listener = listen(runtime, cancel.clone());
    runtime.block_on(future)
}

/// A run owns its listener, including while the listener waits for the
/// second signal. Finishing or unwinding the run cancels that wait and joins
/// the thread before another run can start in the same terminal session.
struct Listener {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            eprintln!("nika run: signal listener ended unexpectedly");
        }
    }
}

/// Hear the operator on a thread of its own: the first signal flips the
/// context and says what happens next; the second ends the process with
/// the cancelled class. A listener that cannot start says so once and the
/// run then ends the way the platform ends it, never a silent hang.
fn listen(runtime: &tokio::runtime::Runtime, cancel: CancelCtx) -> Option<Listener> {
    // Register before dispatch, and retain both Unix subscriptions between
    // signals. Recreating a receiver after "cancelling" could lose the next
    // signal while another runtime broadcasts it with no receiver present.
    #[cfg(unix)]
    let mut receiver = {
        let _entered = runtime.enter();
        match OperatorSignals::new() {
            Ok(receiver) => receiver,
            Err(error) => {
                eprintln!("nika run: cannot listen for Ctrl-C: {error}");
                return None;
            }
        }
    };
    #[cfg(not(unix))]
    let _ = runtime;
    let signals = async move {
        #[cfg(unix)]
        receiver.recv().await;
        #[cfg(not(unix))]
        operator_signal().await;
        cancel.cancel();
        eprintln!(
            "nika run: cancelling · in-flight work completes and is counted · \
             unstarted tasks are cancelled · Ctrl-C again to abort"
        );
        #[cfg(unix)]
        receiver.recv().await;
        #[cfg(not(unix))]
        operator_signal().await;
        eprintln!("nika run: aborted · the trace is incomplete (the run was cut mid-flight)");
        std::process::exit(i32::from(crate::verbs::exit::CANCELLED));
    };
    match spawn_listener(signals) {
        Ok(listener) => Some(listener),
        Err(error) => {
            eprintln!("nika run: cannot listen for Ctrl-C: {error}");
            None
        }
    }
}

fn spawn_listener(
    signals: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<Listener> {
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let thread = std::thread::Builder::new()
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
                tokio::select! {
                    biased;
                    _ = stopped => {}
                    () = signals => {}
                }
            });
        })?;
    Ok(Listener {
        stop: Some(stop),
        thread: Some(thread),
    })
}

/// Persistent subscriptions retain a signal delivered between the two waits.
#[cfg(unix)]
struct OperatorSignals {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
}

#[cfg(unix)]
impl OperatorSignals {
    fn new() -> std::io::Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
        })
    }

    async fn recv(&mut self) {
        tokio::select! {
            _ = self.interrupt.recv() => {}
            _ = self.terminate.recv() => {}
        }
    }
}

/// Resolves on the operator's next Ctrl-C. A listener that cannot be
/// installed says so once and never resolves.
#[cfg(not(unix))]
async fn operator_signal() {
    match tokio::signal::ctrl_c().await {
        Ok(()) => {}
        Err(error) => {
            eprintln!("nika run: cannot listen for Ctrl-C: {error}");
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests;
