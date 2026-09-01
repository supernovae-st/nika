// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

mod auth;
mod cancel;
mod config;
mod coordinator;
mod error;
mod listen;
mod model;
mod openapi;
mod production;
mod registry;
mod route;
mod schedule_http;
mod scheduler;
mod sse;
mod store;

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::net::SocketAddr;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use nika_execution::{ExecutionContext, ExecutionService, SnapshotLimits};
use nika_fs::OwnedDir;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, Semaphore, mpsc};
use tokio::task::JoinSet;

use crate::{
    EventPageLimit, JobId, JobOrigin, JobReceipt, JobStatus, JobStore, MAX_EVENT_PAGE_LEN,
    ScheduleStore,
};

use auth::BearerToken;
use cancel::{ActiveCancellations, CancellationRegistration};
pub use config::{ResidentClock, ResidentConfig, ServerConfig, ServerLimits, SystemResidentClock};
pub use coordinator::{PreparedScheduledRun, ResidentExecutionCoordinator};
use error::diagnose_capture;
pub use error::{CredentialRefuse, ServerError};
use listen::listen_line;
pub use production::{
    ResidentExecutionBackend, ServerLaunchRefuse, launch_operator_message, optional_server_config,
    process_shutdown, serve_resident, serve_resident_process, server_operator_message,
};
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
    /// The operator cancelled the admitted workflow.
    Cancelled,
}

/// Adapter result: disposition plus optional redacted diagnosis, declared
/// workflow outputs, and trace chain head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutcome {
    disposition: ExecutionDisposition,
    error_code: Option<String>,
    error_message: Option<String>,
    outputs: Option<BTreeMap<String, serde_json::Value>>,
    chain_head: Option<String>,
}

impl From<ExecutionDisposition> for ExecutionOutcome {
    fn from(disposition: ExecutionDisposition) -> Self {
        Self {
            disposition,
            error_code: None,
            error_message: None,
            outputs: None,
            chain_head: None,
        }
    }
}

impl ExecutionOutcome {
    /// Failed execution with a redacted operator-visible diagnosis.
    #[must_use]
    pub fn failed(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            disposition: ExecutionDisposition::Failed,
            error_code: Some(code.into()),
            error_message: Some(message.into()),
            outputs: None,
            chain_head: None,
        }
    }

    /// Terminal class.
    #[must_use]
    pub const fn disposition(&self) -> ExecutionDisposition {
        self.disposition
    }

    /// Redacted `(code, message)` when the backend supplied one.
    #[must_use]
    pub fn error(&self) -> Option<(&str, &str)> {
        Some((self.error_code.as_deref()?, self.error_message.as_deref()?))
    }

    /// Attach the runtime's redacted declared workflow outputs.
    ///
    /// A present empty map is preserved and differs from an adapter that did
    /// not supply outputs.
    #[must_use]
    pub fn with_outputs(mut self, outputs: BTreeMap<String, serde_json::Value>) -> Self {
        self.outputs = Some(outputs);
        self
    }

    /// Attach the trace chain head when the execution lane exposes one.
    #[must_use]
    pub fn with_chain_head(mut self, chain_head: impl Into<String>) -> Self {
        self.chain_head = Some(chain_head.into());
        self
    }

    /// Declared workflow outputs supplied by the execution adapter.
    #[must_use]
    pub fn outputs(&self) -> Option<&BTreeMap<String, serde_json::Value>> {
        self.outputs.as_ref()
    }

    /// Trace chain head supplied by the execution adapter.
    #[must_use]
    pub fn chain_head(&self) -> Option<&str> {
        self.chain_head.as_deref()
    }

    /// Attach a diagnosis. Ignored unless this outcome is
    /// [`ExecutionDisposition::Failed`].
    #[must_use]
    pub fn with_error(mut self, code: impl Into<String>, message: impl Into<String>) -> Self {
        if self.disposition == ExecutionDisposition::Failed {
            self.error_code = Some(code.into());
            self.error_message = Some(message.into());
        }
        self
    }
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
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>>;

    /// Execute with an optional schedule-owned per-fire spend ceiling.
    fn execute_with_max_cost<'a>(
        &'a self,
        context: ExecutionContext<'a>,
        _max_cost_usd: Option<f64>,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.execute(context)
    }

    /// Execute with a run-scoped cooperative cancellation token.
    ///
    /// The default preserves existing adapters; the resident owner also drops
    /// this future when the token fires, so an adapter cannot make HTTP
    /// cancellation depend on polling the token itself.
    fn execute_with_cancel<'a>(
        &'a self,
        context: ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        _cancel: nika_types::cancel::CancelCtx,
    ) -> Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.execute_with_max_cost(context, max_cost_usd)
    }
}

struct AppState {
    token: BearerToken,
    store: StoreHandle,
    project: Arc<OwnedDir>,
    service: ExecutionService,
    coordinator: ResidentExecutionCoordinator,
    limits: ServerLimits,
    snapshot_limits: SnapshotLimits,
    sse_slots: Arc<Semaphore>,
    event_page_limit: EventPageLimit,
    schedules: Arc<ScheduleStore>,
    schedule_wake: Arc<Notify>,
    project_refusals: scheduler::ProjectRefusals,
    clock: Arc<dyn ResidentClock>,
    cancellations: Arc<ActiveCancellations>,
}

struct AuthorityState {
    store: StoreHandle,
    service: ExecutionService,
    backend: Arc<dyn ExecutionBackend>,
    limits: ServerLimits,
    snapshot_limits: SnapshotLimits,
    project: Arc<OnceLock<Arc<OwnedDir>>>,
    schedules: Arc<ScheduleStore>,
    schedule_wake: Arc<Notify>,
    project_refusals: scheduler::ProjectRefusals,
    coordinator: ResidentExecutionCoordinator,
    clock: Arc<dyn ResidentClock>,
    cancellations: Arc<ActiveCancellations>,
}

#[derive(Debug)]
struct ExecutionTask {
    id: JobId,
    admitted: Option<nika_execution::AdmittedExecution>,
    prestarted: bool,
    origin: JobOrigin,
    max_cost_usd: Option<f64>,
}

impl ExecutionTask {
    fn new(id: JobId) -> Self {
        Self {
            id,
            admitted: None,
            prestarted: false,
            origin: JobOrigin::Manual,
            max_cost_usd: None,
        }
    }

    fn scheduled(
        id: JobId,
        admitted: nika_execution::AdmittedExecution,
        origin: JobOrigin,
        max_cost_usd: Option<f64>,
    ) -> Self {
        Self {
            id,
            admitted: Some(admitted),
            prestarted: true,
            origin,
            max_cost_usd,
        }
    }
}

/// Resident durable execution authority, independent of any HTTP listener.
pub struct ResidentAuthority {
    state: Arc<AuthorityState>,
    coordinator: ResidentExecutionCoordinator,
    jobs: mpsc::Receiver<ExecutionTask>,
    recovered: VecDeque<ExecutionTask>,
    store_actor: Option<StoreActor>,
}

impl ResidentAuthority {
    /// Recover durable state and open the one store, queue, execution, and
    /// admission authority. This performs no socket or credential I/O.
    ///
    /// # Errors
    /// Refuses invalid ceilings, inaccessible state, corrupt recovery, a
    /// competing incarnation, or store-actor startup failure.
    pub async fn open(
        config: ResidentConfig,
        backend: Arc<dyn ExecutionBackend>,
    ) -> Result<Self, ServerError> {
        validate_resident_config(&config)?;
        let prepared = prepare_authority(&config).await?;
        let control_capacity = store_control_capacity(config.limits());
        let store_actor = StoreActor::start(
            prepared.store,
            prepared.incarnation,
            config.limits().max_connections(),
            control_capacity,
        )?;
        let (jobs, receiver) = mpsc::channel(config.limits().queue_capacity());
        let recovered = store_actor
            .handle()
            .queued_jobs()
            .await?
            .into_iter()
            .map(|(id, _workflow)| ExecutionTask::new(id))
            .collect();
        let coordinator =
            ResidentExecutionCoordinator::new(store_actor.handle(), jobs, config.limits());
        let project = Arc::new(OnceLock::new());
        if let Some(held) = prepared.project {
            let _set = project.set(held);
        }
        let schedules = Arc::new(prepared.schedules);
        let schedule_wake = Arc::new(Notify::new());
        let project_refusals = scheduler::ProjectRefusals::default();
        let cancellations = Arc::new(ActiveCancellations::default());
        let state = Arc::new(AuthorityState {
            store: store_actor.handle(),
            service: ExecutionService::new(config.snapshot_limits()),
            backend,
            limits: config.limits(),
            snapshot_limits: config.snapshot_limits(),
            project,
            schedules,
            schedule_wake,
            project_refusals,
            coordinator: coordinator.clone(),
            clock: Arc::clone(config.clock()),
            cancellations,
        });
        Ok(Self {
            state,
            coordinator,
            jobs: receiver,
            recovered,
            store_actor: Some(store_actor),
        })
    }

    /// Clone the one admission, queue, and terminal-observation capability.
    #[must_use]
    pub fn execution_coordinator(&self) -> ResidentExecutionCoordinator {
        self.coordinator.clone()
    }

    /// Drive queue executions without opening a network listener.
    ///
    /// # Errors
    /// Returns a typed execution, store, or shutdown failure.
    pub async fn serve_until<F>(self, shutdown: F) -> Result<(), ServerError>
    where
        F: Future<Output = ()>,
    {
        run_authority_until(self, shutdown).await
    }

    /// Drive queue executions and one already-attached HTTP listener under a
    /// single shutdown and join boundary.
    ///
    /// # Errors
    /// Returns a typed listener, execution, store, or shutdown failure.
    pub async fn serve_with_http<F>(
        self,
        server: BoundServer,
        shutdown: F,
    ) -> Result<(), ServerError>
    where
        F: Future<Output = ()>,
    {
        run_authority_with_http(self, server, shutdown).await
    }
}

fn store_control_capacity(limits: ServerLimits) -> usize {
    // HTTP mutations remain bounded and may fail fast when this ingress queue
    // is full. Internal lifecycle controls use reliable backpressure instead.
    limits.max_connections()
}

/// Bound authenticated HTTP listener attached to a resident authority.
pub struct BoundServer {
    listener: TcpListener,
    state: Arc<AppState>,
}

impl BoundServer {
    /// Acquire HTTP credentials and registry, then attach a listener to an
    /// already recovered resident authority.
    ///
    /// # Errors
    /// Refuses unacknowledged remote binds, unsafe credential sources,
    /// inaccessible held roots, or bind failure. No listener is created until
    /// the supplied authority has already recovered.
    pub async fn attach(
        config: ServerConfig,
        authority: &ResidentAuthority,
    ) -> Result<Self, ServerError> {
        validate_server_config(&config)?;
        let event_page_limit = EventPageLimit::new(MAX_EVENT_PAGE_LEN).map_err(|_| {
            ServerError::InvalidConfig("SSE event page limit must be within the store cap")
        })?;
        let prepared = prepare_http(&config).await?;
        let project = if let Some(project) = authority.state.project.get() {
            Arc::clone(project)
        } else {
            let _set = authority.state.project.set(Arc::clone(&prepared.project));
            Arc::clone(&prepared.project)
        };
        let listener = TcpListener::bind(config.bind())
            .await
            .map_err(|error| ServerError::Listener(error.kind()))?;
        let state = Arc::new(AppState {
            token: prepared.token,
            store: authority.state.store.clone(),
            project,
            service: authority.state.service,
            coordinator: authority.coordinator.clone(),
            limits: authority.state.limits,
            snapshot_limits: authority.state.snapshot_limits,
            sse_slots: Arc::new(Semaphore::new(authority.state.limits.max_sse_clients())),
            event_page_limit,
            schedules: Arc::clone(&authority.state.schedules),
            schedule_wake: Arc::clone(&authority.state.schedule_wake),
            project_refusals: Arc::clone(&authority.state.project_refusals),
            clock: Arc::clone(&authority.state.clock),
            cancellations: Arc::clone(&authority.state.cancellations),
        });
        Ok(Self { listener, state })
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

    /// Render the operator-facing readiness line for this bound listener.
    ///
    /// # Errors
    /// Returns a listener error when the socket cannot report its address.
    pub fn listen_line(&self) -> Result<String, ServerError> {
        self.local_addr().map(listen_line)
    }
}

/// Parse a CLI bind string and serve until `shutdown` resolves.
///
/// # Errors
/// Returns the same typed failures as [`ResidentAuthority::open`],
/// [`BoundServer::attach`], and [`ResidentAuthority::serve_with_http`].
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
    let config = ServerConfig::new(bind, workflow_root.as_ref(), token_file.as_ref())
        .with_allow_remote(allow_remote);
    serve_resident(
        ResidentConfig::new(state_root.as_ref()),
        Some(config),
        backend,
        shutdown,
    )
    .await
}

struct PreparedAuthority {
    store: Arc<JobStore>,
    incarnation: crate::ServerIncarnation,
    schedules: ScheduleStore,
    project: Option<Arc<OwnedDir>>,
}

struct PreparedHttp {
    token: BearerToken,
    project: Arc<OwnedDir>,
}

async fn prepare_authority(config: &ResidentConfig) -> Result<PreparedAuthority, ServerError> {
    let state_root = config.state_root().to_owned();
    let workflow_root = config.workflow_root().map(Path::to_owned);
    tokio::task::spawn_blocking(move || {
        ensure_state_root(&state_root)?;
        let store = Arc::new(JobStore::open_fail_fast(&state_root)?);
        let schedules = ScheduleStore::open(&state_root).map_err(ServerError::ScheduleStore)?;
        let project = workflow_root
            .map(|root| {
                OwnedDir::open(&root)
                    .map(Arc::new)
                    .map_err(|error| ServerError::WorkflowRoot(error.kind()))
            })
            .transpose()?;
        let incarnation = store.claim_server_incarnation()?;
        store.settle_interrupted_jobs(&incarnation)?;
        Ok(PreparedAuthority {
            store,
            incarnation,
            schedules,
            project,
        })
    })
    .await
    .map_err(|_| ServerError::BlockingTask)?
}

async fn prepare_http(config: &ServerConfig) -> Result<PreparedHttp, ServerError> {
    let token_file = config.token_file().to_owned();
    let workflow_root = config.workflow_root().to_owned();
    tokio::task::spawn_blocking(move || {
        let token = BearerToken::from_file(&token_file)?;
        let project = Arc::new(
            OwnedDir::open(&workflow_root)
                .map_err(|error| ServerError::WorkflowRoot(error.kind()))?,
        );
        Ok(PreparedHttp { token, project })
    })
    .await
    .map_err(|_| ServerError::BlockingTask)?
}

fn validate_resident_config(config: &ResidentConfig) -> Result<(), ServerError> {
    if !config.limits().valid() {
        return Err(ServerError::InvalidConfig(
            "all size, timeout, concurrency, queue, connection, sse, and header ceilings must be non-zero",
        ));
    }
    if config.limits().max_body_bytes() > crate::MAX_ENCODED_EXECUTION_SNAPSHOT_BYTES {
        return Err(ServerError::InvalidConfig(
            "request body ceiling exceeds the durable encoded-snapshot ceiling",
        ));
    }
    Ok(())
}

fn validate_server_config(config: &ServerConfig) -> Result<(), ServerError> {
    if !config.bind().ip().is_loopback() && !config.allow_remote() {
        return Err(ServerError::InvalidConfig(
            "a non-loopback bind requires explicit remote acknowledgement",
        ));
    }
    Ok(())
}

fn ensure_state_root(path: &Path) -> Result<(), ServerError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        match builder.create(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(crate::JobStoreError::Io(error.kind()).into()),
        }
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path).map_err(|error| crate::JobStoreError::Io(error.kind()).into())
    }
}

async fn run_authority_until<F>(
    mut authority: ResidentAuthority,
    shutdown: F,
) -> Result<(), ServerError>
where
    F: Future<Output = ()>,
{
    let (schedule_stop, schedule_receiver) = tokio::sync::watch::channel(false);
    let mut schedule_task = tokio::spawn(scheduler::run(
        Arc::clone(&authority.state),
        schedule_receiver,
    ));
    let mut schedule_done = false;
    let mut executions = JoinSet::new();
    let mut fatal = None;
    tokio::pin!(shutdown);

    loop {
        if executions.len() < authority.state.limits.max_concurrent_jobs()
            && let Some(task) = authority.recovered.pop_front()
        {
            executions.spawn(run_job(Arc::clone(&authority.state), task));
            continue;
        }
        tokio::select! {
            () = &mut shutdown => break,
            task = authority.jobs.recv(),
                if executions.len() < authority.state.limits.max_concurrent_jobs() => {
                if let Some(task) = task {
                    executions.spawn(run_job(Arc::clone(&authority.state), task));
                }
            }
            joined = executions.join_next(), if !executions.is_empty() => {
                if let Some(joined) = joined
                    && let Err(error) = execution_result(joined) {
                    fatal = Some(error);
                    break;
                }
            }
            schedule = &mut schedule_task => {
                fatal = Some(schedule_failure(schedule));
                schedule_done = true;
                break;
            }
        }
    }

    if !schedule_done {
        let _ = schedule_stop.send(true);
    }
    let schedule_task = (!schedule_done).then_some(schedule_task);
    finish_authority(authority, executions, fatal, schedule_task).await
}

async fn run_authority_with_http<F>(
    mut authority: ResidentAuthority,
    server: BoundServer,
    shutdown: F,
) -> Result<(), ServerError>
where
    F: Future<Output = ()>,
{
    let (schedule_stop, schedule_receiver) = tokio::sync::watch::channel(false);
    let mut schedule_task = tokio::spawn(scheduler::run(
        Arc::clone(&authority.state),
        schedule_receiver,
    ));
    let mut schedule_done = false;
    let listener = server.listener;
    let http_state = server.state;
    let mut connections = JoinSet::new();
    let mut executions = JoinSet::new();
    let mut fatal = None;
    tokio::pin!(shutdown);

    loop {
        if executions.len() < authority.state.limits.max_concurrent_jobs()
            && let Some(task) = authority.recovered.pop_front()
        {
            executions.spawn(run_job(Arc::clone(&authority.state), task));
            continue;
        }
        tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept(),
                if connections.len() < http_state.limits.max_connections() => {
                match accepted {
                    Ok((stream, _)) => {
                        connections.spawn(serve_connection(stream, Arc::clone(&http_state)));
                    }
                    Err(error) => {
                        fatal = Some(ServerError::Listener(error.kind()));
                        break;
                    }
                }
            }
            task = authority.jobs.recv(),
                if executions.len() < authority.state.limits.max_concurrent_jobs() => {
                if let Some(task) = task {
                    executions.spawn(run_job(Arc::clone(&authority.state), task));
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
            schedule = &mut schedule_task => {
                fatal = Some(schedule_failure(schedule));
                schedule_done = true;
                break;
            }
        }
    }

    drop(listener);
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    if !schedule_done {
        let _ = schedule_stop.send(true);
    }
    let schedule_task = (!schedule_done).then_some(schedule_task);
    finish_authority(authority, executions, fatal, schedule_task).await
}

fn schedule_failure(
    result: Result<Result<(), ServerError>, tokio::task::JoinError>,
) -> ServerError {
    match result {
        Ok(Err(error)) => error,
        Ok(Ok(())) | Err(_) => ServerError::ExecutionTask,
    }
}

fn combine_schedule_result(
    authority: Result<(), ServerError>,
    schedule: Result<Result<(), ServerError>, tokio::task::JoinError>,
) -> Result<(), ServerError> {
    match authority {
        Err(error) => Err(error),
        Ok(()) => schedule.map_err(|_| ServerError::ExecutionTask)?,
    }
}

async fn finish_authority(
    mut authority: ResidentAuthority,
    mut executions: JoinSet<Result<(), ServerError>>,
    fatal: Option<ServerError>,
    schedule_task: Option<tokio::task::JoinHandle<Result<(), ServerError>>>,
) -> Result<(), ServerError> {
    authority.jobs.close();
    if fatal.is_some() {
        executions.abort_all();
    }
    let grace = authority.state.limits.shutdown_grace();
    let result = if let Some(error) = fatal {
        while executions.join_next().await.is_some() {}
        match authority.state.store.settle_interrupted().await {
            Ok(_) => Err(error),
            Err(settlement) => Err(settlement),
        }
    } else if let Ok(result) =
        tokio::time::timeout(grace, drain_authority(&mut authority, &mut executions)).await
    {
        if let Some(error) = result.err() {
            match authority.state.store.settle_interrupted().await {
                Ok(_) => Err(error),
                Err(settlement) => Err(settlement),
            }
        } else {
            Ok(())
        }
    } else {
        executions.abort_all();
        while executions.join_next().await.is_some() {}
        match authority.state.store.settle_interrupted().await {
            Ok(_) => Err(ServerError::ShutdownTimeout),
            Err(settlement) => Err(settlement),
        }
    };
    let schedule_result = match schedule_task {
        Some(task) => {
            #[cfg(test)]
            authority
                .state
                .store
                .shutdown_test_probe()
                .mark_phase(store::ShutdownPhase::SchedulerJoin);
            Some(task.await)
        }
        None => None,
    };
    let actor_shutdown = match authority.store_actor.take() {
        Some(actor) => actor.shutdown().await,
        None => Err(ServerError::BlockingTask),
    };
    let authority_result = match result {
        Err(error) => Err(error),
        Ok(()) => actor_shutdown,
    };
    match schedule_result {
        Some(schedule) => combine_schedule_result(authority_result, schedule),
        None => authority_result,
    }
}

async fn drain_authority(
    authority: &mut ResidentAuthority,
    executions: &mut JoinSet<Result<(), ServerError>>,
) -> Result<(), ServerError> {
    let mut failure = None;
    let mut queue_drained = false;
    loop {
        if executions.len() < authority.state.limits.max_concurrent_jobs()
            && let Some(task) = authority.recovered.pop_front()
        {
            executions.spawn(run_job(Arc::clone(&authority.state), task));
            continue;
        }
        if executions.is_empty() && authority.recovered.is_empty() && queue_drained {
            return failure.map_or(Ok(()), Err);
        }
        tokio::select! {
            task = authority.jobs.recv(),
                if !queue_drained
                    && executions.len() < authority.state.limits.max_concurrent_jobs() => {
                match task {
                    Some(task) => {
                        executions.spawn(run_job(Arc::clone(&authority.state), task));
                    }
                    None => queue_drained = true,
                }
            }
            joined = executions.join_next(), if !executions.is_empty() => {
                if let Some(joined) = joined
                    && let Err(error) = execution_result(joined) {
                    failure.get_or_insert(error);
                }
            }
        }
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

async fn run_job(state: Arc<AuthorityState>, mut task: ExecutionTask) -> Result<(), ServerError> {
    let (_registration, cancel) =
        CancellationRegistration::new(Arc::clone(&state.cancellations), task.id.clone());
    let mut guard = RunningGuard::new(state.store.clone(), task.id.clone());
    let admitted = match admit_task(&state, &mut task).await {
        Ok(admitted) => admitted,
        Err(error) => {
            let (code, message) = match &error {
                Some(error) => diagnose_capture(error),
                None => (
                    "admission_refused".to_owned(),
                    "workflow world could not be readmitted".to_owned(),
                ),
            };
            guard
                .settle(
                    JobStatus::Failed,
                    json!({
                        "kind": "execution.refused",
                        "status": "failed",
                        "code": code,
                        "message": message
                    }),
                )
                .await?;
            return Ok(());
        }
    };
    if !task.prestarted && !start_running(&mut guard, &admitted).await? {
        return Ok(());
    }
    settle_disposition(
        &state,
        &mut guard,
        admitted,
        task.origin,
        task.max_cost_usd,
        cancel,
    )
    .await
}

async fn admit_task(
    state: &AuthorityState,
    task: &mut ExecutionTask,
) -> Result<nika_execution::AdmittedExecution, Option<nika_execution::ExecutionError>> {
    if let Some(admitted) = task.admitted.take() {
        return Ok(admitted);
    }
    admit_workflow(state, task).await
}

async fn start_running(
    guard: &mut RunningGuard,
    admitted: &nika_execution::AdmittedExecution,
) -> Result<bool, ServerError> {
    match guard
        .store
        .start_execution_reliable(
            guard.id.clone(),
            admitted.execution_id().to_string(),
            admitted.trace_id().to_string(),
            admitted.snapshot().digest().to_owned(),
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
    state: &AuthorityState,
    task: &ExecutionTask,
) -> Result<nika_execution::AdmittedExecution, Option<nika_execution::ExecutionError>> {
    let encoded = state
        .store
        .load_world(task.id.clone())
        .await
        .map_err(|_| None)?;
    let service = state.service;
    let limits = state.snapshot_limits;
    let admission = tokio::task::spawn_blocking(move || {
        let snapshot = nika_execution::ExecutionSnapshot::decode_with_limits(&encoded, limits)?;
        service.readmit_snapshot(snapshot)
    })
    .await
    .map_err(|_| None)?;
    admission.map_err(Some)
}

async fn settle_disposition(
    state: &AuthorityState,
    guard: &mut RunningGuard,
    admitted: nika_execution::AdmittedExecution,
    origin: JobOrigin,
    max_cost_usd: Option<f64>,
    cancel: nika_types::cancel::CancelCtx,
) -> Result<(), ServerError> {
    let session = state.service.begin(admitted);
    let execute =
        state
            .backend
            .execute_with_cancel(session.context(), max_cost_usd, cancel.clone());
    let outcome = tokio::time::timeout(state.limits.execution_timeout(), async {
        tokio::select! {
            outcome = execute => outcome,
            () = cancel::cancelled(cancel.clone()) => ExecutionDisposition::Cancelled.into(),
        }
    })
    .await;
    let Ok(outcome) = outcome else {
        guard.interrupt().await?;
        return Ok(());
    };
    let outcome = if cancel.is_cancelled() {
        ExecutionDisposition::Cancelled.into()
    } else {
        outcome
    };
    let verdict = session.complete(outcome.disposition());
    let status = match *verdict.outcome() {
        ExecutionDisposition::Succeeded => JobStatus::Succeeded,
        ExecutionDisposition::Paused => JobStatus::Paused,
        ExecutionDisposition::Failed => JobStatus::Failed,
        ExecutionDisposition::Cancelled => JobStatus::Cancelled,
    };
    let mut event = json!({"kind": "execution.settled", "status": status});
    if let Some((code, message)) = outcome.error() {
        event["code"] = json!(code);
        event["message"] = json!(message);
    }
    let receipt = status
        .is_settled()
        .then(|| {
            JobReceipt::with_origin(
                guard.id.clone(),
                verdict.execution_id().to_string(),
                verdict.trace_id().to_string(),
                verdict.snapshot_digest().to_owned(),
                outcome.chain_head.clone(),
                origin,
            )
        })
        .transpose()?;
    let outputs = if status.is_settled() {
        outcome.outputs.clone()
    } else {
        None
    };
    guard.settle_result(status, event, outputs, receipt).await?;
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

    async fn settle_result(
        &mut self,
        status: JobStatus,
        event: serde_json::Value,
        outputs: Option<BTreeMap<String, serde_json::Value>>,
        receipt: Option<JobReceipt>,
    ) -> Result<(), ServerError> {
        let result = self
            .store
            .settle_with_result_reliable(self.id.clone(), status, event, outputs, receipt)
            .await;
        if let Err(ServerError::JobStore(crate::JobStoreError::IllegalTransition { .. })) = &result
        {
            let already_cancelled = self
                .store
                .get(self.id.clone())
                .await?
                .is_some_and(|record| record.status() == JobStatus::Cancelled);
            if already_cancelled {
                self.disarm();
                return Ok(());
            }
        }
        result?;
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
mod coordinator_tests;
#[cfg(test)]
mod credential_tests;
#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod result_tests;
#[cfg(test)]
mod schedule_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
