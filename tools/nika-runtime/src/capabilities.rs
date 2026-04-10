// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! VerbCapabilities — the run-scoped capability bundle.
//!
//! Constructed once per workflow run by the Runner, then passed into
//! `dispatch()` by reference. Each verb accessor returns only the subset
//! needed by that verb via the per-verb `*Caps<'a>` structs from
//! `nika-kernel::caps`.

use std::path::PathBuf;
use std::sync::Arc;

use nika_event::EventLog;
use nika_kernel::builtin::BuiltinRouter;
use nika_kernel::caps::{ExecCaps, FetchCaps, InferCaps, InvokeCaps};
use nika_kernel::clock::Clock;
use nika_kernel::filesystem::{FsRead, FsWrite};
use nika_kernel::http::HttpClient;
use nika_kernel::mcp::McpPool;
use nika_kernel::policy::PolicyChecker;
use nika_kernel::provider::Provider;
use nika_kernel::shell::ShellExecutor;
use nika_kernel::store::BlobStore;
use tokio_util::sync::CancellationToken;

/// All side-effect dependencies needed to execute any verb.
///
/// Constructed once per workflow run. Cheap to clone (each field is
/// `Arc` or a cheap-clone handle). Verb accessor methods return borrowed
/// `*Caps<'_>` slices tied to `&self`.
///
/// **EventLog** is stored here and passed separately to verb `run()`
/// functions (not inside Caps structs — see `nika-kernel::caps` module doc).
#[derive(Clone)]
pub struct VerbCapabilities {
    // ── Core side effects ────────────────────────────────────────────
    pub(crate) shell: Arc<dyn ShellExecutor>,
    pub(crate) http: Arc<dyn HttpClient>,
    pub(crate) blobs: Arc<dyn BlobStore>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) fs_read: Arc<dyn FsRead>,
    pub(crate) fs_write: Arc<dyn FsWrite>,

    // ── Security + observability ─────────────────────────────────────
    pub(crate) policy: Arc<dyn PolicyChecker>,
    pub(crate) event_log: EventLog,
    pub(crate) cancel_token: CancellationToken,

    // ── Invoke infrastructure ────────────────────────────────────────
    pub(crate) builtin_router: Arc<dyn BuiltinRouter>,
    pub(crate) mcp_pool: Arc<dyn McpPool>,

    // ── Infer infrastructure ─────────────────────────────────────────
    /// LLM provider for `infer:` tasks.
    ///
    /// This is the **workflow default** provider. Per-task provider
    /// chain resolution (with fallback) happens in the engine bridge
    /// during S14; dispatch() does not yet handle the chain. In S15
    /// when template resolution moves to nika-runtime, this field
    /// will be accompanied by a `Fn(&str) -> Arc<dyn Provider>`
    /// factory that the task-dispatch pipeline invokes per-task
    /// based on the resolved provider name.
    pub(crate) provider: Arc<dyn Provider>,

    // ── Run-scoped context ───────────────────────────────────────────
    pub(crate) workflow_base_dir: PathBuf,
    pub(crate) working_dir_mode: Option<String>,
    pub(crate) project_root: Option<PathBuf>,
}

impl VerbCapabilities {
    /// Borrow an `ExecCaps` slice from this bundle.
    ///
    /// Lifetime tied to `&self` — cannot cross `tokio::spawn`.
    /// For spawn paths, use `exec_caps_owned()`.
    pub fn exec_caps(&self) -> ExecCaps<'_> {
        ExecCaps::new(
            &*self.shell,
            &*self.policy,
            &*self.clock,
            &*self.fs_read,
            &self.cancel_token,
            &self.workflow_base_dir,
            self.working_dir_mode.as_deref(),
            self.project_root.as_deref(),
        )
    }

    /// Borrow a `FetchCaps` slice from this bundle.
    pub fn fetch_caps(&self) -> FetchCaps<'_> {
        FetchCaps::new(
            &*self.http,
            &*self.policy,
            &*self.blobs,
            &*self.clock,
            &self.cancel_token,
        )
    }

    /// Borrow an `InvokeCaps` slice from this bundle.
    pub fn invoke_caps(&self) -> InvokeCaps<'_> {
        InvokeCaps::new(
            &*self.builtin_router,
            &*self.mcp_pool,
            &*self.fs_read,
            &*self.fs_write,
            &*self.http,
            &*self.blobs,
            &*self.policy,
            &*self.clock,
            &self.cancel_token,
        )
    }

    /// Borrow an `InferCaps` slice from this bundle.
    ///
    /// Clones the provider `Arc` to satisfy `InferCaps::provider:
    /// Arc<dyn Provider>` — the only non-`&self`-tied field. All
    /// other fields borrow from `&self`.
    pub fn infer_caps(&self) -> InferCaps<'_> {
        InferCaps::new(
            Arc::clone(&self.provider),
            &*self.fs_read,
            &*self.policy,
            &*self.clock,
            &self.cancel_token,
            &self.workflow_base_dir,
        )
    }

    /// Access the run-scoped `EventLog`.
    ///
    /// Passed separately to verb `run()` functions, not inside Caps.
    pub fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    /// Access the cancellation token.
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verb_capabilities_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VerbCapabilities>();
    }

    #[test]
    fn verb_capabilities_is_clone() {
        fn assert_clone<T: Clone>() {}
        assert_clone::<VerbCapabilities>();
    }
}
