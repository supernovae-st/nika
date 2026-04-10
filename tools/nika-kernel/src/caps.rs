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

impl<'a> ExecCaps<'a> {
    /// Construct from individual references.
    ///
    /// Only `VerbCapabilities` (in nika-runtime) should call this.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shell: &'a dyn ShellExecutor,
        policy: &'a dyn PolicyChecker,
        clock: &'a dyn Clock,
        fs_read: &'a dyn FsRead,
        cancel: &'a CancellationToken,
        workflow_base_dir: &'a Path,
        working_dir_mode: Option<&'a str>,
        project_root: Option<&'a Path>,
    ) -> Self {
        Self {
            shell,
            policy,
            clock,
            fs_read,
            cancel,
            workflow_base_dir,
            working_dir_mode,
            project_root,
        }
    }
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

impl<'a> FetchCaps<'a> {
    /// Construct from individual references.
    pub fn new(
        http: &'a dyn HttpClient,
        policy: &'a dyn PolicyChecker,
        blobs: &'a dyn BlobStore,
        clock: &'a dyn Clock,
        cancel: &'a CancellationToken,
    ) -> Self {
        Self {
            http,
            policy,
            blobs,
            clock,
            cancel,
        }
    }
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

impl<'a> InferCaps<'a> {
    /// Construct from individual references.
    ///
    /// Only `VerbCapabilities` (in nika-runtime) and engine bridge
    /// `TaskExecutor::run_infer` should call this.
    pub fn new(
        provider: Arc<dyn Provider>,
        fs_read: &'a dyn FsRead,
        policy: &'a dyn PolicyChecker,
        clock: &'a dyn Clock,
        cancel: &'a CancellationToken,
        workflow_base_dir: &'a Path,
    ) -> Self {
        Self {
            provider,
            fs_read,
            policy,
            clock,
            cancel,
            workflow_base_dir,
        }
    }
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

impl<'a> InvokeCaps<'a> {
    /// Construct from individual references.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        builtin_router: &'a dyn BuiltinRouter,
        mcp_pool: &'a dyn McpPool,
        fs_read: &'a dyn FsRead,
        fs_write: &'a dyn FsWrite,
        http: &'a dyn HttpClient,
        blobs: &'a dyn BlobStore,
        policy: &'a dyn PolicyChecker,
        clock: &'a dyn Clock,
        cancel: &'a CancellationToken,
    ) -> Self {
        Self {
            builtin_router,
            mcp_pool,
            fs_read,
            fs_write,
            http,
            blobs,
            policy,
            clock,
            cancel,
        }
    }
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

impl<'a> AgentCaps<'a> {
    /// Construct from individual references.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<dyn Provider>,
        builtin_router: &'a dyn BuiltinRouter,
        mcp_pool: &'a dyn McpPool,
        fs_read: &'a dyn FsRead,
        fs_write: &'a dyn FsWrite,
        http: &'a dyn HttpClient,
        blobs: &'a dyn BlobStore,
        policy: &'a dyn PolicyChecker,
        clock: &'a dyn Clock,
        cancel: &'a CancellationToken,
        workflow_base_dir: &'a Path,
    ) -> Self {
        Self {
            provider,
            builtin_router,
            mcp_pool,
            fs_read,
            fs_write,
            http,
            blobs,
            policy,
            clock,
            cancel,
            workflow_base_dir,
        }
    }
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
