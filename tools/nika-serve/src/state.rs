// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared application state injected into all Axum handlers.

use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinHandle;

use crate::config::ServeConfig;
use crate::events::EventBus;
use crate::executor::Executor;
use crate::token_store::AuthMode;

/// Handle for a running worker, storing the task handle and subprocess PID.
pub struct WorkerHandle {
    pub join: JoinHandle<()>,
    /// PID of the child process (set by worker after spawn, 0 if not yet spawned).
    pub child_pid: Arc<std::sync::atomic::AtomicU32>,
}

/// Shared state for the Nika HTTP API server.
///
/// Cloned into every Axum handler via `State<AppState>`.
#[derive(Clone)]
pub struct AppState {
    /// SQLite-backed job persistence.
    pub storage: nika_storage::Storage,

    /// Immutable server configuration.
    pub config: Arc<ServeConfig>,

    /// Workflow execution backend (subprocess or embedded).
    pub executor: Executor,

    /// Cross-workflow concurrency limiter for `nika serve`.
    /// (The Runner inside nika-engine has its own per-run semaphore for
    /// intra-workflow task concurrency -- this one is for *inter*-workflow
    /// concurrency, i.e. how many `nika run` subprocesses run at once.)
    pub semaphore: Arc<Semaphore>,

    /// Shutdown signal receiver. When `true`, the server is draining.
    pub shutdown: tokio::sync::watch::Receiver<bool>,

    /// Active worker handles keyed by job ID (ERRATA-9).
    /// Used for cancel + graceful drain at shutdown.
    pub workers: Arc<Mutex<HashMap<String, WorkerHandle>>>,

    /// Atomic counter of active jobs (pending + running) for race-free queue depth check.
    pub active_jobs: Arc<AtomicUsize>,

    /// Per-job event broadcast for SSE streaming.
    pub event_bus: EventBus,

    /// Webhook configuration, loaded once at startup (BUG-8).
    pub webhook_config: Option<crate::webhook::WebhookConfig>,

    /// Authentication mode (Legacy or MultiKey).
    pub auth_mode: Arc<AuthMode>,
}
