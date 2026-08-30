// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use jiff::Zoned;
use nika_execution::SnapshotLimits;

use crate::MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES;

/// Injected time and wait authority for the resident schedule planner.
///
/// Production uses [`SystemResidentClock`]. Tests and embedders can advance a
/// deterministic clock without sleeping on wall time.
pub trait ResidentClock: fmt::Debug + Send + Sync {
    /// Fresh zoned time for one planning or pre-claim decision.
    fn now(&self) -> Zoned;

    /// Wait until the next clock or scheduler edge.
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// System-backed resident clock.
#[derive(Debug)]
pub struct SystemResidentClock;

impl ResidentClock for SystemResidentClock {
    fn now(&self) -> Zoned {
        Zoned::now()
    }

    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep(duration))
    }
}

/// Explicit ceilings for the remote HTTP and execution boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerLimits {
    max_body_bytes: usize,
    request_timeout: Duration,
    execution_timeout: Duration,
    shutdown_grace: Duration,
    max_concurrent_jobs: usize,
    queue_capacity: usize,
    max_connections: usize,
    max_headers: usize,
    max_jobs: usize,
    max_sse_clients: usize,
    sse_heartbeat: Duration,
    sse_reconnect: Duration,
}

impl ServerLimits {
    /// Construct every server ceiling explicitly.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        max_body_bytes: usize,
        request_timeout: Duration,
        execution_timeout: Duration,
        shutdown_grace: Duration,
        max_concurrent_jobs: usize,
        queue_capacity: usize,
        max_connections: usize,
        max_headers: usize,
    ) -> Self {
        Self {
            max_body_bytes,
            request_timeout,
            execution_timeout,
            shutdown_grace,
            max_concurrent_jobs,
            queue_capacity,
            max_connections,
            max_headers,
            max_jobs: 10_000,
            max_sse_clients: max_connections,
            sse_heartbeat: Duration::from_secs(15),
            sse_reconnect: Duration::from_secs(1),
        }
    }

    /// Replace the durable job-record ceiling.
    #[must_use]
    pub const fn with_max_jobs(mut self, max_jobs: usize) -> Self {
        self.max_jobs = max_jobs;
        self
    }

    /// Replace the concurrent SSE client ceiling.
    #[must_use]
    pub const fn with_max_sse_clients(mut self, max_sse_clients: usize) -> Self {
        self.max_sse_clients = max_sse_clients;
        self
    }

    /// Replace SSE heartbeat and client reconnect guidance.
    #[must_use]
    pub const fn with_sse_timing(mut self, heartbeat: Duration, reconnect: Duration) -> Self {
        self.sse_heartbeat = heartbeat;
        self.sse_reconnect = reconnect;
        self
    }

    pub(crate) const fn valid(self) -> bool {
        self.max_body_bytes != 0
            && !self.request_timeout.is_zero()
            && !self.execution_timeout.is_zero()
            && !self.shutdown_grace.is_zero()
            && self.max_concurrent_jobs != 0
            && self.queue_capacity != 0
            && self.max_connections != 0
            && self.max_headers != 0
            && self.max_jobs != 0
            && self.max_sse_clients != 0
            && !self.sse_heartbeat.is_zero()
            && self.sse_reconnect.as_millis() >= 100
            && self.sse_reconnect.as_millis() <= 30_000
    }

    pub(crate) const fn max_body_bytes(self) -> usize {
        self.max_body_bytes
    }

    pub(crate) const fn request_timeout(self) -> Duration {
        self.request_timeout
    }

    pub(crate) const fn execution_timeout(self) -> Duration {
        self.execution_timeout
    }

    pub(crate) const fn shutdown_grace(self) -> Duration {
        self.shutdown_grace
    }

    pub(crate) const fn max_concurrent_jobs(self) -> usize {
        self.max_concurrent_jobs
    }

    pub(crate) const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }

    pub(crate) const fn max_connections(self) -> usize {
        self.max_connections
    }

    pub(crate) const fn max_headers(self) -> usize {
        self.max_headers
    }

    pub(crate) const fn max_jobs(self) -> usize {
        self.max_jobs
    }

    pub(crate) const fn max_sse_clients(self) -> usize {
        self.max_sse_clients
    }

    pub(crate) const fn sse_heartbeat(self) -> Duration {
        self.sse_heartbeat
    }

    pub(crate) const fn sse_reconnect(self) -> Duration {
        self.sse_reconnect
    }
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self::new(
            MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES,
            Duration::from_secs(5),
            Duration::from_secs(15 * 60),
            Duration::from_secs(30),
            4,
            64,
            128,
            32,
        )
    }
}

/// Complete startup authority for resident durable execution.
#[derive(Clone)]
#[non_exhaustive]
pub struct ResidentConfig {
    state_root: PathBuf,
    workflow_root: Option<PathBuf>,
    limits: ServerLimits,
    snapshot_limits: SnapshotLimits,
    clock: Arc<dyn ResidentClock>,
}

impl ResidentConfig {
    /// Build resident authority rooted at one durable state directory.
    #[must_use]
    pub fn new(state_root: impl Into<PathBuf>) -> Self {
        Self {
            state_root: state_root.into(),
            workflow_root: None,
            limits: ServerLimits::default(),
            snapshot_limits: SnapshotLimits::default(),
            clock: Arc::new(SystemResidentClock),
        }
    }

    /// Replace execution, queue, store, and shutdown ceilings.
    #[must_use]
    pub const fn with_limits(mut self, limits: ServerLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Replace immutable workflow snapshot ceilings.
    #[must_use]
    pub const fn with_snapshot_limits(mut self, limits: SnapshotLimits) -> Self {
        self.snapshot_limits = limits;
        self
    }

    /// Attach the contained workflow root used by the resident scheduler.
    #[must_use]
    pub fn with_workflow_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.workflow_root = Some(root.into());
        self
    }

    /// Replace the schedule clock and wait authority.
    #[must_use]
    pub fn with_clock(mut self, clock: Arc<dyn ResidentClock>) -> Self {
        self.clock = clock;
        self
    }

    pub(crate) fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub(crate) fn workflow_root(&self) -> Option<&Path> {
        self.workflow_root.as_deref()
    }

    pub(crate) const fn limits(&self) -> ServerLimits {
        self.limits
    }

    pub(crate) const fn snapshot_limits(&self) -> SnapshotLimits {
        self.snapshot_limits
    }

    pub(crate) fn clock(&self) -> &Arc<dyn ResidentClock> {
        &self.clock
    }
}

impl fmt::Debug for ResidentConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResidentConfig")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

/// Complete authenticated HTTP-listener configuration.
#[derive(Clone)]
#[non_exhaustive]
pub struct ServerConfig {
    bind: SocketAddr,
    workflow_root: PathBuf,
    token_file: PathBuf,
    allow_remote: bool,
}

impl ServerConfig {
    /// Build a deny-by-default listener with explicit bind, registry, and
    /// secret-source paths. Durable state belongs to [`ResidentConfig`].
    #[must_use]
    pub fn new(
        bind: SocketAddr,
        workflow_root: impl Into<PathBuf>,
        token_file: impl Into<PathBuf>,
    ) -> Self {
        Self {
            bind,
            workflow_root: workflow_root.into(),
            token_file: token_file.into(),
            allow_remote: false,
        }
    }

    /// Acknowledge a non-loopback listener without weakening authentication.
    #[must_use]
    pub const fn with_allow_remote(mut self, allow: bool) -> Self {
        self.allow_remote = allow;
        self
    }

    pub(crate) const fn bind(&self) -> SocketAddr {
        self.bind
    }

    pub(crate) fn workflow_root(&self) -> &Path {
        &self.workflow_root
    }

    pub(crate) fn token_file(&self) -> &Path {
        &self.token_file
    }

    pub(crate) const fn allow_remote(&self) -> bool {
        self.allow_remote
    }
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("bind", &self.bind)
            .field("allow_remote", &self.allow_remote)
            .finish_non_exhaustive()
    }
}
