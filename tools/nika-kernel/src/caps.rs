// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-verb capability slices.
//!
//! `nika-runtime` builds a run-scoped `VerbCapabilities` bundle once per
//! workflow invocation. For each task, it borrows slices of that bundle
//! into one of the five `*Caps<'a>` structs below and passes it to the
//! relevant `pub async fn run()` in the verb crate.
//!
//! **Design note (AMEND-2):** fields are trait objects (`&dyn Trait`)
//! rather than concrete types, so verb crates depend only on `nika-kernel`
//! and remain decoupled from the concrete impls (`nika-policy`,
//! `nika-exec-runner`, `nika-http`, etc.).
//!
//! **EventLog is NOT included.** `nika-kernel` does not depend on
//! `nika-event` (layering: L0.5 vs L1). Verb crates that emit events
//! depend on `nika-event` directly and receive `&EventLog` as a separate
//! parameter to their `run()` function.

use std::path::Path;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::builtin::BuiltinRouter;
use crate::clock::Clock;
use crate::filesystem::{FsRead, FsWrite};
use crate::http::HttpClient;
use crate::mcp::McpPool;
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
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
    pub working_dir_mode: Option<&'a str>,
    pub project_root: Option<&'a Path>,
}

/// Capabilities available to a `fetch:` task.
#[non_exhaustive]
pub struct FetchCaps<'a> {
    pub http: &'a dyn HttpClient,
    pub policy: &'a dyn PolicyChecker,
    pub blobs: &'a dyn BlobStore,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
}

/// Capabilities available to an `infer:` task.
#[non_exhaustive]
pub struct InferCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub fs_read: &'a dyn FsRead,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
}

/// Capabilities available to an `invoke:` task (MCP or builtin).
#[non_exhaustive]
pub struct InvokeCaps<'a> {
    pub builtin_router: &'a dyn BuiltinRouter,
    pub mcp_pool: &'a dyn McpPool,
    pub fs_read: &'a dyn FsRead,
    pub fs_write: &'a dyn FsWrite,
    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
}

/// Capabilities available to an `agent:` task (multi-turn loop).
///
/// Flattened rather than embedding `InvokeCaps` to avoid lifetime pain
/// with the `AgentCapsOwned::borrow()` pattern for `tokio::spawn`.
#[non_exhaustive]
pub struct AgentCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub builtin_router: &'a dyn BuiltinRouter,
    pub mcp_pool: &'a dyn McpPool,
    pub fs_read: &'a dyn FsRead,
    pub fs_write: &'a dyn FsWrite,
    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn exec_caps_is_send_sync() {
        assert_send::<ExecCaps<'_>>();
        assert_sync::<ExecCaps<'_>>();
    }

    #[test]
    fn fetch_caps_is_send_sync() {
        assert_send::<FetchCaps<'_>>();
        assert_sync::<FetchCaps<'_>>();
    }

    #[test]
    fn infer_caps_is_send_sync() {
        assert_send::<InferCaps<'_>>();
        assert_sync::<InferCaps<'_>>();
    }

    #[test]
    fn invoke_caps_is_send_sync() {
        assert_send::<InvokeCaps<'_>>();
        assert_sync::<InvokeCaps<'_>>();
    }

    #[test]
    fn agent_caps_is_send_sync() {
        assert_send::<AgentCaps<'_>>();
        assert_sync::<AgentCaps<'_>>();
    }
}
