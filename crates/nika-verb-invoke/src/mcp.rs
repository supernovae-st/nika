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

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

use nika_kernel::ai::provider::ToolDef;
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolResult};

/// The call half of the plane.
pub type McpCallFn = dyn Fn(ToolCall) -> Result<ToolResult, ToolExecError> + Send + Sync;

/// The definitions half of the plane (the approved `mcp:` tool defs an
/// agent universe may offer).
pub type McpDefsFn = dyn Fn() -> Vec<ToolDef> + Send + Sync;

/// The installed MCP plane: a call and a definitions source.
#[derive(Clone)]
pub struct McpPlane {
    call: Arc<McpCallFn>,
    defs: Arc<McpDefsFn>,
}

impl std::fmt::Debug for McpPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("McpPlane")
    }
}

impl McpPlane {
    /// Build a plane from its two halves.
    #[must_use]
    pub fn new(call: Arc<McpCallFn>, defs: Arc<McpDefsFn>) -> Self {
        Self { call, defs }
    }

    /// Dispatch one `mcp:` call (sync · the caller offloads).
    pub(crate) fn call(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        (self.call)(call)
    }

    /// The approved `mcp:` tool definitions.
    #[must_use]
    pub fn tool_defs(&self) -> Vec<ToolDef> {
        (self.defs)()
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
/// CANCEL SAFETY: dropping the future does NOT stop the worker — the
/// in-flight `tools/call` completes on its thread and its result is
/// discarded (the `spawn_blocking` class the builtin plane already has).
pub(crate) struct Offloaded {
    shared: Arc<Mutex<Slot>>,
}

impl Future for Offloaded {
    type Output = Outcome;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Outcome> {
        let mut slot = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(value) = slot.value.take() {
            return Poll::Ready(value);
        }
        slot.waker = Some(cx.waker().clone());
        Poll::Pending
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
    Offloaded { shared }
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
            .call(ToolCall::new("tc-9", "mcp:s/t", serde_json::json!({})))
            .expect("ok");
        assert_eq!(out.tool_use_id.as_str(), "tc-9");
        assert_eq!(out.content, "mcp:s/t");
        assert_eq!(plane.tool_defs()[0].name, "mcp:s/t");
        assert_eq!(format!("{plane:?}"), "McpPlane");
    }
}
