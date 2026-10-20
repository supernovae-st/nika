// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The **MCP plane** seam — how an `mcp:<server>/<tool>` call leaves the
//! verb (#1575).
//!
//! The builtin dispatcher behind [`ToolExecuteDyn`](nika_kernel::tool_executor::ToolExecuteDyn)
//! resolves `nika:*` only; the MCP client lives above this crate (an L4
//! interface crate that spawns confined servers), so the verb cannot name
//! it. The seam is therefore two closures over KERNEL types — a call and a
//! tool-definition source — installed once by the composition root
//! ([`InvokeVerb::install_mcp_plane`](crate::InvokeVerb::install_mcp_plane))
//! and consulted for every `mcp:` name from then on. No plane installed =
//! every `mcp:` name still reaches the executor, which resolves none
//! (the pre-plane behavior · the `nika test` rehearsal keeps it).
//!
//! The plane is SYNC (the MCP client is tokio-free by dependency law); the
//! verb offloads a call to a worker thread (`offload`) and awaits it.
//! This crate carries zero runtime tokio by the same law, so the bridge is
//! a std thread + a [`Waker`] — runtime-agnostic.
//!
//! A call can be STOPPED: each one carries a [`CancelCtx`], a child of the
//! run's stop when the runtime installed one
//! ([`InvokeVerb::install_run_stop`](crate::InvokeVerb::install_run_stop)),
//! that the verb also cancels when the run stops waiting for the call (the
//! awaiting future is dropped: a task `timeout:`, an aborted attempt). A
//! plane built with [`McpPlane::stoppable`] watches it, writes the call only
//! through [`CancelCtx::begin`] and tells the server to cancel; one built
//! with [`McpPlane::new`] cannot stop a call in flight but never starts one
//! already stopped. A dropped call leaves what became of it in the
//! [`StoppedCallSlot`] its input carried.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

use nika_kernel::CancelCtx;
use nika_kernel::ai::provider::ToolDef;
use nika_kernel::tool_executor::{StopOutcome, StoppedCall, ToolCall, ToolExecError, ToolResult};

/// The call half of the plane.
pub type McpCallFn = dyn Fn(ToolCall) -> Result<ToolResult, ToolExecError> + Send + Sync;

/// The call half of a plane that can stop a call: the same call, watching
/// the token the verb cancels when the run stops waiting for it.
pub type McpStoppableCallFn =
    dyn Fn(ToolCall, &CancelCtx) -> Result<ToolResult, ToolExecError> + Send + Sync;

/// The definitions half of the plane (the approved `mcp:` tool defs an
/// agent universe may offer).
pub type McpDefsFn = dyn Fn() -> Vec<ToolDef> + Send + Sync;

/// The installed MCP plane: a call and a definitions source.
#[derive(Clone)]
pub struct McpPlane {
    call: Arc<McpStoppableCallFn>,
    defs: Arc<McpDefsFn>,
}

impl std::fmt::Debug for McpPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("McpPlane")
    }
}

impl McpPlane {
    /// Build a plane from its two halves. Its calls cannot be stopped once
    /// started: one the run stops waiting for runs to its own end. A call
    /// the run stopped before it started is never started.
    #[must_use]
    pub fn new(call: Arc<McpCallFn>, defs: Arc<McpDefsFn>) -> Self {
        Self::stoppable(
            Arc::new(move |tool_call: ToolCall, stop: &CancelCtx| {
                if !stop.begin() {
                    return Ok(ToolResult::stopped(&tool_call, StopOutcome::NotSent));
                }
                call(tool_call)
            }),
            defs,
        )
    }

    /// Build a plane whose call watches its stop token: cancelled, the call
    /// is not sent, or the server is told to cancel it.
    #[must_use]
    pub fn stoppable(call: Arc<McpStoppableCallFn>, defs: Arc<McpDefsFn>) -> Self {
        Self { call, defs }
    }

    /// Dispatch one `mcp:` call (sync · the caller offloads), watching
    /// `stop`.
    pub(crate) fn call(
        &self,
        call: ToolCall,
        stop: &CancelCtx,
    ) -> Result<ToolResult, ToolExecError> {
        (self.call)(call, stop)
    }

    /// The approved `mcp:` tool definitions.
    #[must_use]
    pub fn tool_defs(&self) -> Vec<ToolDef> {
        (self.defs)()
    }
}

/// Where the verb leaves the call a dropped attempt stopped (a task
/// `timeout:` cut it): the runtime reads it to name the stopped call on the
/// task's terminal frame. Clones share one slot. A child also forwards every
/// typed witness to its parent, which outlives a cancelled attempt or fan-out.
#[derive(Clone, Debug, Default)]
pub struct StoppedCallSlot(Arc<StoppedCalls>);

#[derive(Debug, Default)]
struct StoppedCalls {
    facts: Mutex<CallFacts>,
    parent: Option<StoppedCallSlot>,
}

/// Minimal physical receipts only: never copy tool arguments or result content.
#[derive(Debug, Default)]
struct CallFacts {
    stopped: Vec<StoppedCall>,
    reported_cost_usd: f64,
}

impl StoppedCallSlot {
    /// An empty slot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The stopped call, once: a second read finds nothing.
    #[must_use]
    pub fn take(&self) -> Option<StoppedCall> {
        self.0
            .facts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stopped
            .drain(..)
            .next_back()
    }

    /// A local attempt slot whose witnesses also survive in this parent.
    #[must_use]
    pub fn child(&self) -> Self {
        Self(Arc::new(StoppedCalls {
            facts: Mutex::new(CallFacts::default()),
            parent: Some(self.clone()),
        }))
    }

    /// Every recorded witness, without consuming the run's evidence.
    #[must_use]
    pub fn snapshot(&self) -> Vec<StoppedCall> {
        self.0
            .facts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stopped
            .clone()
    }

    /// Successful physical calls' finite non-negative cost reports, once per
    /// call. This subtotal survives a ready result whose future is dropped;
    /// it does not imply an unread task or iteration succeeded.
    #[must_use]
    pub fn reported_cost_usd(&self) -> Option<f64> {
        let cost = self
            .0
            .facts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reported_cost_usd;
        (cost > 0.0).then_some(cost)
    }

    /// Leave `call` for the runtime.
    fn record(&self, call: StoppedCall) {
        self.0
            .facts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stopped
            .push(call.clone());
        if let Some(parent) = &self.0.parent {
            parent.record(call);
        }
    }

    /// The physical completion owner calls this exactly once, before giving
    /// away or discarding its result. Keep only the existing monetary fact.
    pub(crate) fn record_success_cost(&self, result: &ToolResult) {
        if !result.is_error
            && let Some(cost) = result
                .structured
                .as_ref()
                .and_then(|value| value.get("cost_usd"))
                .and_then(serde_json::Value::as_f64)
                .filter(|cost| cost.is_finite() && *cost >= 0.0)
        {
            self.record_cost(cost);
        }
    }

    fn record_cost(&self, cost: f64) {
        self.0
            .facts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reported_cost_usd += cost;
        if let Some(parent) = &self.0.parent {
            parent.record_cost(cost);
        }
    }
}

/// The result of one offloaded call.
type Outcome = Result<ToolResult, ToolExecError>;

/// The worker ↔ future handoff: the value lands under the lock, the waker
/// registered by the last poll fires under the same lock — no lost wakeup.
struct Slot {
    value: Option<Outcome>,
    waker: Option<Waker>,
}

/// A call running on a worker thread, awaited by the verb.
///
/// CANCEL SAFETY: dropping the future does not kill the worker thread.
/// Poll and drop arbitrate through the same worker handoff lock. A result
/// already available there keeps its exact physical Stop/cost facts even if
/// it was never polled. Only an unresolved drop cancels the stop token
/// ([`Offloaded::cancel_on_drop`]) and classifies the call from its actual
/// begun state: `unknown` or `not_sent`. No waiting, tool-result copy, task
/// success credit or remote rollback is implied.
pub(crate) struct Offloaded {
    shared: Arc<Mutex<Slot>>,
    stop: Option<CancelCtx>,
    witness: Option<(StoppedCallSlot, String, String)>,
}

impl Offloaded {
    /// Cancel `stop` when this future is dropped before it resolves.
    pub(crate) fn cancel_on_drop(mut self, stop: CancelCtx) -> Self {
        self.stop = Some(stop);
        self
    }

    /// Dropped before it resolves, leave the call (`tool`, `call_id`) and
    /// what became of it in `slot`.
    pub(crate) fn recording_in(
        mut self,
        slot: StoppedCallSlot,
        tool: String,
        call_id: String,
    ) -> Self {
        self.witness = Some((slot, tool, call_id));
        self
    }
}

impl Future for Offloaded {
    type Output = Outcome;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Outcome> {
        let ready = {
            let mut slot = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
            let value = slot.value.take();
            if value.is_none() {
                slot.waker = Some(cx.waker().clone());
            }
            value
        };
        match ready {
            Some(value) => {
                // Taking the handoff result owns this physical completion.
                // Drop cannot classify or debit it again after this poll.
                record_returned(self.witness.take(), &value);
                self.stop = None;
                Poll::Ready(value)
            }
            None => Poll::Pending,
        }
    }
}

impl Drop for Offloaded {
    fn drop(&mut self) {
        // The worker stores its result under this same lock. Prefer any
        // available physical result to inferred abandonment, even without
        // a final poll; only an unresolved handoff may become Unknown.
        let mut handoff = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(value) = handoff.value.take() {
            record_returned(self.witness.take(), &value);
            self.stop = None;
            return;
        }
        let Some(stop) = self.stop.take() else {
            return;
        };
        let began = stop.cancel_begun();
        if let Some((slot, tool, call_id)) = self.witness.take() {
            let outcome = if began {
                StopOutcome::Unknown
            } else {
                StopOutcome::NotSent
            };
            slot.record(StoppedCall::new(tool, call_id, outcome));
        }
    }
}

/// The sole returned-result receipt path for both polling and dropping.
/// Only matching typed metadata can name a stopped physical call; arbitrary
/// content and ordinary failures grant no control or success-cost credit.
fn record_returned(witness: Option<(StoppedCallSlot, String, String)>, value: &Outcome) {
    let (Some((slot, tool, call_id)), Ok(result)) = (witness, value) else {
        return;
    };
    slot.record_success_cost(result);
    if result.is_error
        && let Some(call) = result
            .error_meta
            .as_ref()
            .and_then(|meta| meta.details.as_ref())
            .and_then(|details| details.stopped_call.as_deref())
        && call.tool == tool
        && call.call_id == call_id
    {
        slot.record(call.clone());
    }
}

/// Run `f` on a named worker thread and hand its outcome to the awaiting
/// task. A thread that cannot be spawned resolves immediately as
/// [`ToolExecError::NotAvailable`] — never a panic, never a hang.
#[allow(clippy::disallowed_methods)] // a std thread IS the bridge: this crate carries zero runtime tokio by dependency law
pub(crate) fn offload<F>(f: F) -> Offloaded
where
    F: FnOnce() -> Outcome + Send + 'static,
{
    let shared = Arc::new(Mutex::new(Slot {
        value: None,
        waker: None,
    }));
    let worker = Arc::clone(&shared);
    let spawned = std::thread::Builder::new()
        .name("nika-mcp-call".to_owned())
        .spawn(move || {
            let value = f();
            let mut slot = worker.lock().unwrap_or_else(PoisonError::into_inner);
            slot.value = Some(value);
            if let Some(waker) = slot.waker.take() {
                waker.wake();
            }
        });
    if let Err(e) = spawned {
        let mut slot = shared.lock().unwrap_or_else(PoisonError::into_inner);
        slot.value = Some(Err(ToolExecError::NotAvailable {
            reason: format!("cannot start the MCP call worker: {e}"),
        }));
    }
    Offloaded {
        shared,
        stop: None,
        witness: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_offloaded_call_completes_on_a_worker_and_wakes_the_task() {
        let out = offload(|| {
            std::thread::sleep(std::time::Duration::from_millis(20));
            Ok(ToolResult::success("tc", "from the worker"))
        })
        .await
        .expect("ok");
        assert_eq!(out.content, "from the worker");
        // A call that finishes BEFORE the first poll is picked up too.
        let done = offload(|| Ok(ToolResult::success("tc", "already"))).await;
        assert_eq!(done.expect("ok").content, "already");
    }

    #[test]
    fn the_plane_routes_to_its_halves() {
        let plane = McpPlane::new(
            Arc::new(|call: ToolCall| Ok(ToolResult::success(call.id.to_string(), call.name))),
            Arc::new(|| vec![ToolDef::new("mcp:s/t", "d", serde_json::json!({}))]),
        );
        let out = plane
            .call(
                ToolCall::new("tc-9", "mcp:s/t", serde_json::json!({})),
                &CancelCtx::new(),
            )
            .expect("ok");
        assert_eq!(out.tool_use_id.as_str(), "tc-9");
        assert_eq!(out.content, "mcp:s/t");
        assert_eq!(plane.tool_defs()[0].name, "mcp:s/t");
        assert_eq!(format!("{plane:?}"), "McpPlane");
    }

    /// The run stops waiting (the awaiting future is dropped
    /// before the call answered), so the call's stop token is cancelled and
    /// a stoppable plane sees it.
    #[tokio::test]
    async fn dropping_an_unresolved_call_cancels_its_stop() {
        let stop = CancelCtx::new();
        let watched = stop.clone();
        let (seen_tx, seen_rx) = std::sync::mpsc::channel();
        let call = offload(move || {
            let started = std::time::Instant::now();
            while !watched.is_cancelled() && started.elapsed() < std::time::Duration::from_secs(10)
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let _ = seen_tx.send(watched.is_cancelled());
            Ok(ToolResult::success("tc", "stopped"))
        })
        .cancel_on_drop(stop);
        let cut = tokio::time::timeout(std::time::Duration::from_millis(50), call).await;
        assert!(
            cut.is_err(),
            "the call was still running when the run stopped waiting"
        );
        let seen = seen_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the worker ended");
        assert!(seen, "the worker saw its stop, well before its own 10 s");
    }

    /// A dropped call names what became of it: `unknown` once the plane
    /// began it, `not_sent` when it never did (it still queued, or its
    /// server was still starting).
    #[tokio::test]
    async fn a_dropped_call_names_what_became_of_it() {
        for (begins, outcome) in [(true, StopOutcome::Unknown), (false, StopOutcome::NotSent)] {
            let stop = CancelCtx::new();
            let watched = stop.clone();
            let slot = StoppedCallSlot::new();
            let call = offload(move || {
                if begins {
                    let _ = watched.begin();
                }
                let started = std::time::Instant::now();
                while !watched.is_cancelled()
                    && started.elapsed() < std::time::Duration::from_secs(10)
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Ok(ToolResult::success("tc", "late"))
            })
            .cancel_on_drop(stop)
            .recording_in(slot.clone(), "mcp:s/slow".to_owned(), "c-9".to_owned());
            let cut = tokio::time::timeout(std::time::Duration::from_millis(50), call).await;
            assert!(cut.is_err(), "the run stopped waiting");
            assert_eq!(
                slot.take(),
                Some(StoppedCall::new("mcp:s/slow", "c-9", outcome)),
                "began: {begins}"
            );
            assert_eq!(slot.take(), None, "read once");
        }
    }

    /// A plane that cannot stop a call in flight never starts one the run
    /// already stopped, and begins the ones it starts.
    #[test]
    fn a_plane_that_cannot_stop_never_starts_a_stopped_call() {
        let started = Arc::new(Mutex::new(0_usize));
        let seen = Arc::clone(&started);
        let plane = McpPlane::new(
            Arc::new(move |call: ToolCall| {
                *seen.lock().unwrap() += 1;
                Ok(ToolResult::success(call.id.to_string(), "ran"))
            }),
            Arc::new(Vec::new),
        );
        let call = || ToolCall::new("tc-2", "mcp:s/t", serde_json::json!({}));
        let stopped = CancelCtx::new();
        stopped.cancel();
        let out = plane
            .call(call(), &stopped)
            .expect("a stopped call is a result");
        assert!(out.is_error);
        assert_eq!(
            out.error_meta.and_then(|meta| meta.spec_code).as_deref(),
            Some("NIKA-CANCEL-001")
        );
        assert_eq!(*started.lock().unwrap(), 0, "never started");
        let fresh = CancelCtx::new();
        assert_eq!(plane.call(call(), &fresh).expect("ok").content, "ran");
        assert!(fresh.cancel_begun(), "a started call is begun");
    }

    /// A call that answered leaves its stop alone, even when its future is
    /// dropped afterwards.
    #[tokio::test]
    async fn a_resolved_call_leaves_its_stop_alone() {
        let stop = CancelCtx::new();
        let slot = StoppedCallSlot::new();
        let out = offload(|| Ok(ToolResult::success("tc", "done")))
            .cancel_on_drop(stop.clone())
            .recording_in(slot.clone(), "mcp:s/t".to_owned(), "c".to_owned())
            .await;
        assert_eq!(out.expect("ok").content, "done");
        assert!(!stop.is_cancelled());
        assert_eq!(slot.take(), None, "nothing was stopped");
    }

    /// A stoppable plane hands each call the token it was given.
    #[test]
    fn a_stoppable_plane_passes_the_stop_to_its_call() {
        let plane = McpPlane::stoppable(
            Arc::new(|call: ToolCall, stop: &CancelCtx| {
                let seen = if stop.is_cancelled() {
                    "stopped"
                } else {
                    "running"
                };
                Ok(ToolResult::success(call.id.to_string(), seen))
            }),
            Arc::new(Vec::new),
        );
        let stop = CancelCtx::new();
        let call = || ToolCall::new("tc-1", "mcp:s/t", serde_json::json!({}));
        assert_eq!(plane.call(call(), &stop).expect("ok").content, "running");
        stop.cancel();
        assert_eq!(plane.call(call(), &stop).expect("ok").content, "stopped");
    }

    #[tokio::test]
    async fn returned_and_dropped_calls_survive_consumed_local_slots() {
        let parent = StoppedCallSlot::new();
        let returned = parent.child();
        let result = offload(|| {
            let call = ToolCall::new("returned-1", "mcp:s/slow", serde_json::json!({}));
            Ok(ToolResult::stopped(&call, StopOutcome::NotSent))
        })
        .cancel_on_drop(CancelCtx::new())
        .recording_in(returned.clone(), "mcp:s/slow".into(), "returned-1".into())
        .await;
        assert!(result.expect("returned witness").is_error);
        assert_eq!(
            returned.take(),
            Some(StoppedCall::new(
                "mcp:s/slow",
                "returned-1",
                StopOutcome::NotSent
            ))
        );
        let dropped = parent.child();
        let stop = CancelCtx::new();
        let watched = stop.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let future = offload(move || {
            assert!(watched.begin());
            ready_tx.send(()).expect("started");
            while !watched.is_cancelled() {
                std::thread::yield_now();
            }
            Ok(ToolResult::success("dropped-1", "late"))
        })
        .cancel_on_drop(stop)
        .recording_in(dropped.clone(), "mcp:s/slow".into(), "dropped-1".into());
        ready_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("physical start");
        drop(future);
        assert_eq!(
            dropped.take(),
            Some(StoppedCall::new(
                "mcp:s/slow",
                "dropped-1",
                StopOutcome::Unknown
            ))
        );
        assert_eq!(returned.take(), None);
        assert_eq!(dropped.take(), None);
        let expected = vec![
            StoppedCall::new("mcp:s/slow", "returned-1", StopOutcome::NotSent),
            StoppedCall::new("mcp:s/slow", "dropped-1", StopOutcome::Unknown),
        ];
        assert_eq!(parent.snapshot(), expected);
        assert_eq!(
            parent.snapshot(),
            expected,
            "a cost/terminal snapshot consumes no evidence"
        );
    }

    /// Model the actual worker handoff state directly: a closure's earlier
    /// signal or an outer coalesced wake is not proof of Slot publication.
    fn handed_off(
        value: Option<Outcome>,
        began: bool,
        receipts: &StoppedCallSlot,
        tool: &str,
        id: &str,
    ) -> (Offloaded, CancelCtx) {
        let stop = CancelCtx::new();
        if began {
            assert!(stop.begin());
        }
        let pending = Offloaded {
            shared: Arc::new(Mutex::new(Slot { value, waker: None })),
            stop: Some(stop.clone()),
            witness: Some((receipts.clone(), tool.to_owned(), id.to_owned())),
        };
        (pending, stop)
    }

    #[test]
    fn dropping_a_ready_success_keeps_cost_without_fabricating_a_stop() {
        let parent = StoppedCallSlot::new();
        let local = parent.child();
        let (pending, stop) = handed_off(
            Some(Ok(ToolResult::success(
                "paid",
                "private result stays in its own value",
            )
            .with_structured(serde_json::json!({"cost_usd":2.5})))),
            true,
            &local,
            "mcp:s/paid",
            "paid",
        );
        drop(pending); // No poll consumes the physically available result.
        assert!(
            !stop.is_cancelled(),
            "a completed call has nothing left to cancel"
        );
        assert_eq!(local.snapshot(), Vec::<StoppedCall>::new());
        assert_eq!(parent.snapshot(), Vec::<StoppedCall>::new());
        assert_eq!(local.reported_cost_usd(), Some(2.5));
        assert_eq!(parent.reported_cost_usd(), Some(2.5));
        assert_eq!(local.take(), None);
        assert_eq!(
            parent.reported_cost_usd(),
            Some(2.5),
            "reading witnesses cannot consume spend"
        );
    }

    #[tokio::test]
    async fn first_stop_then_dropped_ready_paid_sibling_keeps_only_the_actual_stop() {
        let batch = StoppedCallSlot::new();
        let call = ToolCall::new("stop-0", "mcp:s/slow", serde_json::json!({}));
        let (first, _) = handed_off(
            Some(Ok(ToolResult::stopped(&call, StopOutcome::Unknown))),
            true,
            &batch,
            "mcp:s/slow",
            "stop-0",
        );
        let (second, _) = handed_off(
            Some(Ok(ToolResult::success("paid-1", "done")
                .with_structured(serde_json::json!({"cost_usd":2.5})))),
            true,
            &batch,
            "mcp:s/paid",
            "paid-1",
        );
        // The first request-index's Stop returns before the ready sibling's
        // final poll. The batch short-circuits and drops that sibling.
        assert!(first.await.expect("typed stop result").is_error);
        drop(second);
        assert_eq!(
            batch.snapshot(),
            vec![StoppedCall::new(
                "mcp:s/slow",
                "stop-0",
                StopOutcome::Unknown,
            )]
        );
        assert_eq!(batch.reported_cost_usd(), Some(2.5));
        assert_eq!(
            batch.reported_cost_usd(),
            Some(2.5),
            "snapshot does not debit twice"
        );
    }

    #[test]
    fn dropping_a_ready_typed_not_sent_preserves_its_exact_returned_outcome() {
        let batch = StoppedCallSlot::new();
        let call = ToolCall::new("not-sent", "mcp:s/t", serde_json::json!({}));
        let (pending, stop) = handed_off(
            Some(Ok(ToolResult::stopped(&call, StopOutcome::NotSent))),
            true,
            &batch,
            "mcp:s/t",
            "not-sent",
        );
        drop(pending);
        assert!(!stop.is_cancelled());
        assert_eq!(
            batch.snapshot(),
            vec![StoppedCall::new(
                "mcp:s/t",
                "not-sent",
                StopOutcome::NotSent,
            )]
        );
        assert_eq!(batch.reported_cost_usd(), None);
    }

    #[test]
    fn ready_ordinary_failures_and_foreign_stop_metadata_do_not_become_abandoned_calls() {
        let cases = [
            Ok(ToolResult::error("own", "NIKA-CANCEL-001 ordinary failure")
                .with_structured(serde_json::json!({"cost_usd":99.0}))),
            Err(ToolExecError::NotAvailable {
                reason: "ordinary refusal".into(),
            }),
            Ok(ToolResult::stopped(
                &ToolCall::new("foreign", "mcp:other/t", serde_json::json!({})),
                StopOutcome::Unknown,
            )),
        ];
        for value in cases {
            let batch = StoppedCallSlot::new();
            let (pending, stop) = handed_off(Some(value), true, &batch, "mcp:s/t", "own");
            drop(pending);
            assert!(!stop.is_cancelled());
            assert_eq!(batch.snapshot(), Vec::<StoppedCall>::new());
            assert_eq!(batch.reported_cost_usd(), None);
        }
    }

    #[test]
    fn unresolved_drop_keeps_unknown_or_not_sent_even_if_a_late_success_arrives() {
        for (began, outcome) in [(true, StopOutcome::Unknown), (false, StopOutcome::NotSent)] {
            let batch = StoppedCallSlot::new();
            let (pending, stop) = handed_off(None, began, &batch, "mcp:s/t", "pending");
            let handoff = Arc::clone(&pending.shared);
            drop(pending);
            assert!(stop.is_cancelled());
            // This is after the abandonment ownership decision, not a ready
            // result at that decision. The worker is not awaited or drained.
            handoff.lock().expect("handoff").value =
                Some(Ok(ToolResult::success("pending", "late")
                    .with_structured(serde_json::json!({"cost_usd":9.0}))));
            assert_eq!(
                batch.snapshot(),
                vec![StoppedCall::new("mcp:s/t", "pending", outcome)]
            );
            assert_eq!(batch.reported_cost_usd(), None);
        }
    }

    #[tokio::test]
    async fn returned_success_and_later_drop_record_cost_only_once_per_physical_call() {
        let batch = StoppedCallSlot::new();
        for _ in 0..2 {
            // A textual ID may be reused on another physical turn. The
            // handoff owns each completion, not a global ID-based debit set.
            let (pending, _) = handed_off(
                Some(Ok(ToolResult::success("reused", "done")
                    .with_structured(serde_json::json!({"cost_usd":2.5})))),
                true,
                &batch,
                "mcp:s/paid",
                "reused",
            );
            assert_eq!(
                pending
                    .await
                    .expect("returned success")
                    .tool_use_id
                    .as_str(),
                "reused"
            );
        }
        assert_eq!(batch.snapshot(), Vec::<StoppedCall>::new());
        assert_eq!(batch.reported_cost_usd(), Some(5.0));
    }

    #[test]
    fn physical_cost_receipts_keep_the_existing_finite_non_negative_success_filter() {
        let batch = StoppedCallSlot::new();
        for cost in [
            serde_json::json!(2.5),
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!("3"),
            serde_json::Value::Null,
        ] {
            let (pending, _) = handed_off(
                Some(Ok(ToolResult::success("c", "done")
                    .with_structured(serde_json::json!({"cost_usd":cost})))),
                true,
                &batch,
                "mcp:s/paid",
                "c",
            );
            drop(pending);
        }
        assert_eq!(batch.reported_cost_usd(), Some(2.5));
        assert_eq!(batch.snapshot(), Vec::<StoppedCall>::new());
    }
}
