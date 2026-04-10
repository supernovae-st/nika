// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-verb capability slices.
//!
//! In Session 13, `nika-runtime` will build a run-scoped `VerbCapabilities`
//! bundle once per workflow invocation. For each task, it borrows slices
//! of that bundle into one of the five `*Caps<'a>` structs below and passes
//! it to the relevant `pub async fn run()` in the verb crate.
//!
//! These structs are intentionally NOT wired in Session 12 — they exist
//! so Session 13 is mechanical: every field here already has a home.
//!
//! **Design note (AMEND-2):** fields are trait objects (`&dyn Trait`)
//! rather than concrete types, so verb crates depend only on `nika-kernel`
//! and remain decoupled from the concrete impls (`nika-policy`,
//! `nika-exec-runner`, `nika-http`, etc.).
//!
//! Session 13 may extend these structs with run-scoped fields like
//! `shield: &'a nika_shield::ShieldContext`, `events: &'a nika_event::EventLog`,
//! `cancel: &'a CancellationToken`, and `workflow_base_dir: &'a Path` once
//! the corresponding crates are extracted.

use std::sync::Arc;

use crate::clock::Clock;
use crate::filesystem::{FsRead, FsWrite};
use crate::http::HttpClient;
use crate::policy::PolicyChecker;
use crate::provider::Provider;
use crate::shell::ShellExecutor;
use crate::store::BlobStore;

/// Capabilities available to an `exec:` task.
#[non_exhaustive]
pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub fs_read: &'a dyn FsRead,
}

/// Capabilities available to a `fetch:` task.
#[non_exhaustive]
pub struct FetchCaps<'a> {
    pub http: &'a dyn HttpClient,
    pub policy: &'a dyn PolicyChecker,
    pub blobs: &'a dyn BlobStore,
    pub clock: &'a dyn Clock,
}

/// Capabilities available to an `infer:` task.
#[non_exhaustive]
pub struct InferCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub fs_read: &'a dyn FsRead,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

/// Capabilities available to an `invoke:` task (MCP or builtin).
#[non_exhaustive]
pub struct InvokeCaps<'a> {
    pub fs_read: &'a dyn FsRead,
    pub fs_write: &'a dyn FsWrite,
    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

/// Capabilities available to an `agent:` task (multi-turn loop).
#[non_exhaustive]
pub struct AgentCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub invoke: InvokeCaps<'a>,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn exec_caps_is_send_sync() {
        assert_send::<&ExecCaps<'_>>();
        assert_sync::<&ExecCaps<'_>>();
    }

    #[test]
    fn fetch_caps_is_send_sync() {
        assert_send::<&FetchCaps<'_>>();
        assert_sync::<&FetchCaps<'_>>();
    }

    #[test]
    fn infer_caps_is_send_sync() {
        assert_send::<&InferCaps<'_>>();
        assert_sync::<&InferCaps<'_>>();
    }

    #[test]
    fn invoke_caps_is_send_sync() {
        assert_send::<&InvokeCaps<'_>>();
        assert_sync::<&InvokeCaps<'_>>();
    }

    #[test]
    fn agent_caps_is_send_sync() {
        assert_send::<&AgentCaps<'_>>();
        assert_sync::<&AgentCaps<'_>>();
    }
}
