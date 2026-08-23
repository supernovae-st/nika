// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

mod auth;
mod config;
mod error;
mod model;
mod openapi;
mod registry;
mod route;
mod sse;
mod store;

use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use nika_execution::{ExecutionContext, ExecutionService, SnapshotLimits};
use nika_fs::OwnedDir;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

use crate::{EventPageLimit, JobId, JobStatus, JobStore, MAX_EVENT_PAGE_LEN};

use auth::BearerToken;
pub use config::{ServerConfig, ServerLimits};
pub use error::ServerError;
use store::{StoreActor, StoreHandle};

/// Backend-owned terminal class projected onto the durable job lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecutionDisposition {
    /// The admitted workflow completed successfully.
    Succeeded,
    /// The admitted workflow paused at a typed decision point.
    Paused,
    /// The admitted workflow settled unsuccessfully.
    Failed,
}

/// Effecting execution seam used after [`ExecutionService`] admission.
pub trait ExecutionBackend: Send + Sync + 'static {
    /// Execute the exact immutable world in `context`.
    ///
    /// The returned future is the cancellation boundary. An implementation
    /// must not detach effecting work that can outlive that future: execution
    /// timeout and server shutdown cancel by dropping it before the durable
    /// running guard settles `interrupted`.
    fn execute<'a>(
        &'a self,
        context: ExecutionContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ExecutionDisposition> + Send + 'a>>;
}

struct AppState {
    token: BearerToken,
    store: StoreHandle,
    project: Arc<OwnedDir>,
    service: ExecutionService,
    backend: Arc<dyn ExecutionBackend>,
    jobs: mpsc::Sender<ExecutionTask>,
    limits: ServerLimits,
    snapshot_limits: SnapshotLimits,
    sse_slots: Arc<Semaphore>,
    event_page_limit: EventPageLimit,
}

#[derive(Debug)]
struct ExecutionTask {
    id: JobId,
    workflow: PathBuf,
}

impl ExecutionTask {
    fn new(id: JobId, workflow: String) -> Self {
        Self {
            id,
            workflow: PathBuf::from(workflow),
        }
    }
}

/// Bound HTTP listener holding the exclusive durable-server incarnation.
pub struct BoundServer {
    listener: TcpListener,
    state: Arc<AppState>,
    jobs: mpsc::Receiver<ExecutionTask>,
    store_actor: StoreActor,
}

impl BoundServer {
    /// Validate authority, recover durable state, then bind the listener.
    ///
    /// # Errors
    /// Refuses invalid limits, unacknowledged remote binds, unsafe credential
    /// sources, inaccessible held roots, a competing incarnation, or bind
    /// failure. Recovery completes before the socket becomes reachable.
    pub async fn bind(
        config: ServerConfig,
        backend: Arc<dyn ExecutionBackend>,
    ) -> Result<Self, ServerError> {
        validate_config(&config)?;
        let event_page_limit = EventPageLimit::new(MAX_EVENT_PAGE_LEN).map_err(|_| {
            ServerError::InvalidConfig("SSE event page limit must be within the store cap")
        })?;
        let prepared = prepare_authority(&config).await?;
        let listener = TcpListener::bind(config.bind())
            .await
            .map_err(|error| ServerError::Listener(error.kind()))?;
        let store_actor = StoreActor::start(
            prepared.store,
            prepared.incarnation,
            config.limits().max_connections(),
            config
                .limits()
                .max_concurrent_jobs()
                .saturating_mul(2)
                .saturating_add(4),
        )?;
        let (jobs, receiver) = mpsc::channel(config.limits().queue_capacity());
        let state = Arc::new(AppState {
            token: prepared.token,
            store: store_actor.handle(),
            project: prepared.project,
            service: ExecutionService::new(config.snapshot_limits()),
            backend,
            jobs,
            limits: config.limits(),
            snapshot_limits: config.snapshot_limits(),
            sse_slots: Arc::new(Semaphore::new(config.limits().max_sse_clients())),
            event_page_limit,
        });
        Ok(Self {
            listener,
            state,
            jobs: receiver,
            store_actor,
        })
    }

    /// Return the actual socket address, including an ephemeral port.
    ///
    /// # Errors
    /// Returns a listener error when the bound socket cannot report its
    /// address.
    pub fn local_addr(&self) -> Result<SocketAddr, ServerError> {
        self.listener
            .local_addr()
            .map_err(|error| ServerError::Listener(error.kind()))
    }

    /// Serve until the supplied shutdown future resolves.
    ///
    /// # Errors
    /// Returns a typed listener or shutdown-timeout failure.
    pub async fn serve_until<F>(self, shutdown: F) -> Result<(), ServerError>
    where
        F: Future<Output = ()>,
    {
        run_until(self, shutdown).await
    }
}

/// Parse a CLI bind string and serve until `shutdown` resolves.
///
/// # Errors
/// Returns the same typed failures as [`BoundServer::bind`] and
/// [`BoundServer::serve_until`].
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
pub async fn serve_http(
    bind: &str,
    workflow_root: impl AsRef<Path>,
    state_root: impl AsRef<Path>,
    token_file: impl AsRef<Path>,
    allow_remote: bool,
    backend: Arc<dyn ExecutionBackend>,
    shutdown: impl Future<Output = ()>,
) -> Result<(), ServerError> {
    let bind = bind
        .parse()
        .map_err(|_| ServerError::InvalidConfig("bind address is invalid"))?;
    let config = ServerConfig::new(
        bind,
        workflow_root.as_ref(),
        state_root.as_ref(),
        token_file.as_ref(),
    )
    .with_allow_remote(allow_remote);
    let server = BoundServer::bind(config, backend).await?;
    let addr = server.local_addr()?;
    // Operator-facing: `--bind …:0` is useless without the chosen port.
    eprintln!("nika serve · listening http://{addr} · GET /health");
    server.serve_until(shutdown).await
}

struct PreparedAuthority {
    token: BearerToken,
    project: Arc<OwnedDir>,
    store: Arc<JobStore>,
    incarnation: crate::ServerIncarnation,
}

async fn prepare_authority(config: &ServerConfig) -> Result<PreparedAuthority, ServerError> {
    let token_file = config.token_file().to_owned();
    let workflow_root = config.workflow_root().to_owned();
    let state_root = config.state_root().to_owned();
    tokio::task::spawn_blocking(move || {
        let token = BearerToken::from_file(&token_file)?;
        let project = Arc::new(
            OwnedDir::open(&workflow_root)
                .map_err(|error| ServerError::WorkflowRoot(error.kind()))?,
        );
        let store = Arc::new(JobStore::open_fail_fast(&state_root)?);
        let incarnation = store.claim_server_incarnation()?;
        store.settle_interrupted_jobs(&incarnation)?;
        Ok(PreparedAuthority {
            token,
            project,
            store,
            incarnation,
        })
    })
    .await
    .map_err(|_| ServerError::BlockingTask)?
}

fn validate_config(config: &ServerConfig) -> Result<(), ServerError> {
    if !config.limits().valid() {
        return Err(ServerError::InvalidConfig(
            "all size, timeout, concurrency, queue, connection, sse, and header ceilings must be non-zero",
        ));
    }
    if !config.bind().ip().is_loopback() && !config.allow_remote() {
        return Err(ServerError::InvalidConfig(
            "a non-loopback bind requires explicit remote acknowledgement",
        ));
    }
    Ok(())
}

async fn run_until<F>(mut server: BoundServer, shutdown: F) -> Result<(), ServerError>
where
    F: Future<Output = ()>,
{
    let mut connections = JoinSet::new();
    let mut executions = JoinSet::new();
    let mut fatal = None;
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            () = &mut shutdown => break,
            accepted = server.listener.accept(),
                if connections.len() < server.state.limits.max_connections() => {
                match accepted {
                    Ok((stream, _)) => {
                        connections.spawn(serve_connection(stream, Arc::clone(&server.state)));
                    }
                    Err(error) => {
                        fatal = Some(ServerError::Listener(error.kind()));
                        break;
                    }
                }
            }
            task = server.jobs.recv(),
                if executions.len() < server.state.limits.max_concurrent_jobs() => {
                if let Some(task) = task {
                    executions.spawn(run_job(Arc::clone(&server.state), task));
                }
            }
            joined = executions.join_next(), if !executions.is_empty() => {
                if let Some(joined) = joined
                    && let Err(error) = execution_result(joined) {
                    fatal = Some(error);
                    break;
                }
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }

    finish_serve(server, connections, executions, fatal).await
}

async fn finish_serve(
    mut server: BoundServer,
    mut connections: JoinSet<()>,
    mut executions: JoinSet<Result<(), ServerError>>,
    fatal: Option<ServerError>,
) -> Result<(), ServerError> {
    server.jobs.close();
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    if fatal.is_some() {
        executions.abort_all();
    }
    let grace = server.state.limits.shutdown_grace();
    let result = if let Ok(result) = tokio::time::timeout(grace, drain(&mut executions)).await {
        if let Some(error) = fatal.or_else(|| result.err()) {
            match server.state.store.settle_interrupted().await {
                Ok(_) => Err(error),
                Err(settlement) => Err(settlement),
            }
        } else {
            Ok(())
        }
    } else {
        executions.abort_all();
        while executions.join_next().await.is_some() {}
        match server.state.store.settle_interrupted().await {
            Ok(_) => Err(ServerError::ShutdownTimeout),
            Err(settlement) => Err(settlement),
        }
    };
    let actor_shutdown = server.store_actor.shutdown().await;
    match result {
        Err(error) => Err(error),
        Ok(()) => actor_shutdown,
    }
}

async fn serve_connection(stream: TcpStream, state: Arc<AppState>) {
    let timeout = state.limits.request_timeout();
    let max_headers = state.limits.max_headers();
    let service = service_fn(move |request| route::handle(request, Arc::clone(&state)));
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .header_read_timeout(timeout)
        .max_headers(max_headers)
        .keep_alive(false);
    let _result = builder
        .serve_connection(TokioIo::new(stream), service)
        .await;
}

async fn run_job(state: Arc<AppState>, task: ExecutionTask) -> Result<(), ServerError> {
    let mut guard = RunningGuard::new(state.store.clone(), task.id);
    if !start_running(&mut guard).await? {
        return Ok(());
    }
    let admitted = admit_workflow(&state, &task.workflow).await;
    let Ok(admitted) = admitted else {
        guard
            .settle(
                JobStatus::Failed,
                json!({"kind": "execution.refused", "status": "failed"}),
            )
            .await?;
        return Ok(());
    };
    settle_disposition(&state, &mut guard, admitted).await
}

async fn start_running(guard: &mut RunningGuard) -> Result<bool, ServerError> {
    match guard
        .store
        .transition_with_events(
            guard.id.clone(),
            JobStatus::Running,
            json!({"kind": "execution.started", "status": "running"}),
        )
        .await
    {
        Ok(_) => Ok(true),
        Err(ServerError::JobStore(crate::JobStoreError::IllegalTransition { .. })) => {
            guard.disarm();
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

async fn admit_workflow(
    state: &AppState,
    workflow: &Path,
) -> Result<nika_execution::AdmittedExecution, ()> {
    let service = state.service;
    let project = Arc::clone(&state.project);
    let workflow = workflow.to_owned();
    let admission = tokio::task::spawn_blocking(move || service.admit(&project, &workflow)).await;
    match admission {
        Ok(Ok(admitted)) => Ok(admitted),
        _ => Err(()),
    }
}

async fn settle_disposition(
    state: &AppState,
    guard: &mut RunningGuard,
    admitted: nika_execution::AdmittedExecution,
) -> Result<(), ServerError> {
    let session = state.service.begin(admitted);
    let disposition = tokio::time::timeout(
        state.limits.execution_timeout(),
        state.backend.execute(session.context()),
    )
    .await;
    let Ok(disposition) = disposition else {
        guard.interrupt().await?;
        return Ok(());
    };
    let verdict = session.complete(disposition);
    let status = match *verdict.outcome() {
        ExecutionDisposition::Succeeded => JobStatus::Succeeded,
        ExecutionDisposition::Paused => JobStatus::Paused,
        ExecutionDisposition::Failed => JobStatus::Failed,
    };
    guard
        .settle(
            status,
            json!({"kind": "execution.settled", "status": status}),
        )
        .await?;
    Ok(())
}

struct RunningGuard {
    store: StoreHandle,
    id: JobId,
    armed: bool,
}

impl RunningGuard {
    fn new(store: StoreHandle, id: JobId) -> Self {
        Self {
            store,
            id,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    async fn settle(
        &mut self,
        status: JobStatus,
        event: serde_json::Value,
    ) -> Result<(), ServerError> {
        self.store
            .transition_with_events(self.id.clone(), status, event)
            .await?;
        self.disarm();
        Ok(())
    }

    async fn interrupt(&mut self) -> Result<(), ServerError> {
        self.store.interrupt(self.id.clone()).await?;
        self.disarm();
        Ok(())
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if self.armed {
            self.store.interrupt_detached(self.id.clone());
        }
    }
}

async fn drain(set: &mut JoinSet<Result<(), ServerError>>) -> Result<(), ServerError> {
    let mut failure = None;
    while let Some(joined) = set.join_next().await {
        if let Err(error) = execution_result(joined) {
            failure.get_or_insert(error);
        }
    }
    if let Some(error) = failure {
        Err(error)
    } else {
        Ok(())
    }
}

fn execution_result(
    joined: Result<Result<(), ServerError>, tokio::task::JoinError>,
) -> Result<(), ServerError> {
    match joined {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(error),
        Err(_) => Err(ServerError::ExecutionTask),
    }
}

#[cfg(test)]
mod tests;
