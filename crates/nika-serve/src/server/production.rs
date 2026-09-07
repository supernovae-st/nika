// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production composition for the resident authority.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use nika_dap::journal::TraceFileSink;
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use nika_event::settlement::RunCause;
use nika_service_execution::{
    ServiceExecutionDriver, ServiceExecutionOptions, ServiceExecutionResult, ServiceExecutionStatus,
};
use nika_types::cancel::CancelCtx;
use nika_types::id::ExecutionId;

use super::{
    BoundServer, CredentialRefuse, ExecutionBackend, ExecutionDisposition, ExecutionOutcome,
    ResidentAuthority, ResidentConfig, ServerConfig, ServerError,
};

/// Production adapter from admitted execution snapshots to the shared service driver.
#[non_exhaustive]
pub struct ResidentExecutionBackend {
    display_root: PathBuf,
    /// The seal the run journal receives at settlement: the machine's key
    /// custody in production, an in-memory key under test.
    seal: Arc<dyn JournalSeal>,
}

impl ResidentExecutionBackend {
    /// Bind resident output rendering to the held workflow root.
    #[must_use]
    pub fn new(display_root: impl Into<PathBuf>) -> Self {
        Self {
            display_root: display_root.into(),
            seal: Arc::new(CustodySeal),
        }
    }

    /// Replace the seal's key custody — the tests prove the door's seal
    /// POINT with a key they hold; production never calls this.
    #[cfg(test)]
    pub(crate) fn with_journal_seal(mut self, seal: Arc<dyn JournalSeal>) -> Self {
        self.seal = seal;
        self
    }

    fn drive<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        cancel: Option<CancelCtx>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let display_root = self.display_root.clone();
        let seal = Arc::clone(&self.seal);
        Box::pin(async move {
            drive_resident_execution(display_root, seal, context, max_cost_usd, cancel).await
        })
    }
}

/// The run journal's seal at settlement — the custody seam as a trait, so
/// the door's seal point is provable without a run key on the machine.
pub(crate) trait JournalSeal: Send + Sync + 'static {
    /// Seal `trace` the way the CLI's `surface_trace` does
    /// (`nika_dap::journal::seal_journal_with`); `true` when the seal landed.
    fn seal(
        &self,
        trace: &mut TraceFileSink,
        workflow_hash: Option<&str>,
        teardown: Option<&nika_dap::seal::SealTeardown>,
    ) -> bool;
}

/// Production custody: the run-signing key this machine holds (the OS
/// keychain · `~/.nika/keys`) — exactly what `nika run` seals with.
struct CustodySeal;

impl JournalSeal for CustodySeal {
    fn seal(
        &self,
        trace: &mut TraceFileSink,
        workflow_hash: Option<&str>,
        teardown: Option<&nika_dap::seal::SealTeardown>,
    ) -> bool {
        nika_dap::journal::seal_journal_with(trace, workflow_hash, teardown)
    }
}

impl ExecutionBackend for ResidentExecutionBackend {
    fn execute<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(context, None, None)
    }

    fn execute_with_cancel<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        cancel: CancelCtx,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(context, max_cost_usd, Some(cancel))
    }

    fn execute_with_max_cost<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(context, max_cost_usd, None)
    }

    fn trace_journal_dir(&self) -> Option<PathBuf> {
        Some(self.display_root.join(nika_dap::store::TRACE_DIR))
    }
}

/// The driver's mirror lane onto the resident's journal: the resident keeps
/// the sink (to seal it · settle it · close it), the runtime writes through
/// a handle it drops with its future.
#[derive(Clone)]
struct JournalLane(Arc<Mutex<TraceFileSink>>);

impl nika_runtime::EventSink for JournalLane {
    fn emit(&mut self, event: nika_event::Event) {
        journal_guard(&self.0).emit(event);
    }
}

/// Lock the journal. A poisoned lock still holds a coherent sink (the sink
/// buffers its own error), so a panic elsewhere never loses the run's END.
fn journal_guard(journal: &Mutex<TraceFileSink>) -> MutexGuard<'_, TraceFileSink> {
    match journal.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Dropping an execution future stops its blocking worker.
struct CancelOnDrop(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}

async fn drive_resident_execution(
    display_root: PathBuf,
    seal: Arc<dyn JournalSeal>,
    context: nika_execution::ExecutionContext<'_>,
    max_cost_usd: Option<f64>,
    operator_cancel: Option<CancelCtx>,
) -> ExecutionOutcome {
    // The journal a `nika run` would leave, under the project the resident
    // serves: the trace the receipt names exists on disk (#1381). The sink
    // stays with the resident — the driver mirrors into a lane on it — so
    // the run's END is the resident's to write: the seal at settlement, the
    // terminal record when the resident interrupts the run.
    let journal_dir = display_root.join(nika_dap::store::TRACE_DIR);
    let (execution_id, trace_id) = (context.execution_id(), context.trace_id());
    let snapshot_digest = context.snapshot().digest().to_owned();
    let journal = Arc::new(Mutex::new(
        TraceFileSink::new(journal_dir).for_execution(execution_id, trace_id),
    ));
    let mirror: nika_service_execution::MirrorFactory = {
        let lane = JournalLane(Arc::clone(&journal));
        Arc::new(move || Box::new(lane.clone()))
    };
    let Some(driver) = ServiceExecutionDriver::new(context, display_root.clone()) else {
        return ExecutionOutcome::failed(
            "admission_refused",
            "workflow world could not be composed",
        );
    };
    // One Door · wave 1b: the resident resolves the SAME frozen plan the
    // CLI door does (no pin, no override on a resident job) and executes
    // it — a job with no ready path refuses before its first task.
    let plan = driver.resolve_access_plan(None, None);
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let _cancel = CancelOnDrop(Some(cancel_tx));
    let job = ResidentJob {
        driver,
        plan,
        mirror,
        journal,
        seal,
        snapshot_digest,
        display_root,
        operator_cancel,
        max_cost_usd,
    };
    match tokio::task::spawn_blocking(move || run_admitted_resident_job(job, cancel_rx)).await {
        Ok(outcome) => outcome,
        Err(_) => ExecutionOutcome::failed("NIKA-COMP-001", "execution worker did not finish"),
    }
}

/// Everything the blocking worker holds for one admitted job.
struct ResidentJob {
    driver: ServiceExecutionDriver,
    plan: nika_service_execution::ExecutionAccessPlan,
    mirror: nika_service_execution::MirrorFactory,
    journal: Arc<Mutex<TraceFileSink>>,
    seal: Arc<dyn JournalSeal>,
    snapshot_digest: String,
    display_root: PathBuf,
    operator_cancel: Option<CancelCtx>,
    max_cost_usd: Option<f64>,
}

fn run_admitted_resident_job(
    job: ResidentJob,
    cancel: tokio::sync::oneshot::Receiver<()>,
) -> ExecutionOutcome {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return ExecutionOutcome::failed("NIKA-COMP-001", "execution runtime could not start");
    };
    let ResidentJob {
        driver,
        plan,
        mirror,
        journal,
        seal,
        snapshot_digest,
        display_root,
        operator_cancel,
        max_cost_usd,
    } = job;
    let options = ServiceExecutionOptions::new()
        .with_max_cost_usd(max_cost_usd)
        .with_access_plan(plan)
        .with_mirror(mirror)
        .with_cancel_option(operator_cancel.clone());
    let result = rt.block_on(async {
        tokio::select! {
            result = driver.execute(options) => Some(result),
            _ = cancel => None,
        }
    });
    // The driver's lane left with its future; the resident holds the sink
    // and writes the run's END here, where the settlement is built once
    // (ADR-128): the seal rides it like the CLI's `surface_trace`, and an
    // interrupted run gets its terminal record before the sink drops.
    match result {
        Some(Ok(outcome)) => {
            let mut mapped = map_outcome(&outcome);
            let facts = SealFacts {
                driver: &driver,
                outcome: &outcome,
                snapshot_digest: &snapshot_digest,
                display_root: &display_root,
            };
            if let Some(head) = settle_journal(&journal, seal.as_ref(), &facts) {
                mapped = mapped.with_chain_head(head);
            }
            mapped
        }
        Some(Err(_)) => {
            ExecutionOutcome::failed("NIKA-COMP-001", "service runtime could not be composed")
        }
        None => {
            interrupt_journal(&journal, driver.execution_id(), operator_cancel.as_ref());
            ExecutionDisposition::Failed.into()
        }
    }
}

/// The service result projected onto the resident's outcome (the status,
/// the settlement whole, the outputs, the redacted error).
fn map_outcome(outcome: &ServiceExecutionResult) -> ExecutionOutcome {
    let disposition = match outcome.status() {
        ServiceExecutionStatus::Succeeded => ExecutionDisposition::Succeeded,
        ServiceExecutionStatus::Paused => ExecutionDisposition::Paused,
        ServiceExecutionStatus::Cancelled => ExecutionDisposition::Cancelled,
        _ => ExecutionDisposition::Failed,
    };
    let mut mapped = ExecutionOutcome::from(disposition);
    if let Some(settlement) = outcome.settlement() {
        mapped = mapped.with_settlement(settlement.clone());
    }
    if !outcome.outputs().is_empty() {
        mapped = mapped.with_outputs(outcome.outputs().clone());
    }
    if let Some((code, message)) = outcome.error() {
        mapped = mapped.with_error(code, message);
    }
    mapped
}

/// What the seal's teardown attests for a resident run — the facts this
/// door holds at settlement.
struct SealFacts<'a> {
    driver: &'a ServiceExecutionDriver,
    outcome: &'a ServiceExecutionResult,
    snapshot_digest: &'a str,
    display_root: &'a Path,
}

/// Settle the journal the way the CLI's `surface_trace` does — the seal
/// FIRST, then the durability point, so the seal's own bytes are covered by
/// the fsync — and hand back the chain head the receipt names. `None` when
/// no journal was opened (a refusal before any event) or the lane died.
fn settle_journal(
    journal: &Mutex<TraceFileSink>,
    seal: &dyn JournalSeal,
    facts: &SealFacts<'_>,
) -> Option<String> {
    let mut trace = journal_guard(journal);
    trace.path()?;
    // The workflow hash the CLI seals under (`seal_hash`): the per-task
    // Merkle root of the admitted workflow.
    let workflow_hash = nika_runtime::proof::ir::merkle_by_task(facts.driver.workflow())
        .map(|proof| proof.workflow.as_hex().to_owned());
    let teardown = resident_teardown(facts);
    seal.seal(&mut trace, workflow_hash.as_deref(), Some(&teardown));
    trace.finalize();
    if trace.error().is_some() {
        return None;
    }
    Some(trace.chain_head().to_owned())
}

/// The teardown facts the seal binds (spec 17 §the end of the run) — the
/// CLI's `attended_facts` routed through the service boundary: the receipt
/// inputs (proves · the certificate · the outcome word), the budgets ρ from
/// the settlement's own spend (ADR-128 · `spent_usd` only when metered), the
/// SDK receipt binding this door knows, the signed-memory fold under the
/// served root. The effects ε and the quarantine fold need the per-task
/// records the service boundary redacts — their keys stay OUT (absent is
/// honest, never a fabricated zero).
fn resident_teardown(facts: &SealFacts<'_>) -> nika_dap::seal::SealTeardown {
    let (workflow, report) = (facts.driver.workflow(), facts.driver.report());
    let mut teardown = nika_dap::seal::SealTeardown::new();
    teardown.proves =
        nika_runtime::proof::ir::semantic_ir_hash(workflow).map(|hash| hash.as_hex().to_owned());
    teardown.certificate = serde_json::to_value(&report.certificate).ok();
    teardown.outcome = Some(
        match facts.outcome.status() {
            ServiceExecutionStatus::Succeeded => "completed",
            ServiceExecutionStatus::Paused => "paused",
            _ => "failed",
        }
        .to_owned(),
    );
    if let Some(settlement) = facts.outcome.settlement() {
        let mut budgets = serde_json::Map::new();
        if let Some(spent) = settlement.spend.total_cost_usd {
            budgets.insert("spent_usd".to_owned(), serde_json::json!(spent));
        }
        budgets.insert(
            "priced_calls".to_owned(),
            settlement.spend.priced_calls.into(),
        );
        budgets.insert(
            "unpriced_calls".to_owned(),
            settlement.spend.unpriced_calls.into(),
        );
        budgets.insert(
            "budget_exceeded".to_owned(),
            (settlement.cause == RunCause::Budget).into(),
        );
        if let Some(ceiling) = &report.certificate.usd_micros
            && let Ok(value) = serde_json::to_value(ceiling)
        {
            budgets.insert("ceiling".to_owned(), value);
        }
        teardown.budgets = Some(serde_json::Value::Object(budgets));
    }
    let execution = facts.driver.execution_id();
    teardown.sdk_receipt = Some(serde_json::json!({
        "receipt_format": 1,
        "execution_id": execution.to_string(),
        "trace_id": nika_types::id::TraceId::from(execution).to_string(),
        "snapshot_digest": facts.snapshot_digest,
    }));
    let memory = nika_dap::memory::attend(Some(facts.display_root));
    teardown.memory = memory.fold;
    teardown.memory_rejected = memory.rejected;
    teardown
}

/// The run's END when the resident stops it (the cancel grace expired · the
/// execution ceiling · shutdown): the terminal settlement envelope the chain
/// walk reads as a lifecycle end (spec 17 · `run_settled`), written by the
/// LIVING writer about the run it interrupted — never a frame claiming the
/// runtime's own settlement (no invented `workflow_cancelled`). The `cause`
/// rides only when the operator asked (absent is honest). With the terminal
/// written, the lease sidecar leaves with the sink (ADR-129).
fn interrupt_journal(
    journal: &Mutex<TraceFileSink>,
    execution: ExecutionId,
    operator_cancel: Option<&CancelCtx>,
) {
    let mut trace = journal_guard(journal);
    if trace.path().is_none() {
        return;
    }
    let mut record = serde_json::json!({
        "kind": "run_settled",
        "status": "interrupted",
        "execution": execution,
    });
    if operator_cancel.is_some_and(CancelCtx::is_cancelled) {
        record["cause"] = serde_json::Value::from("operator");
    }
    // The sink contract is infallible: a write failure is the lane's own
    // buffered error, never the job's verdict.
    let _written = trace.write_record(&record);
    trace.finalize();
}

/// Why optional HTTP launch flags could not form one complete listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ServerLaunchRefuse {
    /// `--bind` and `--workflows` were not both supplied.
    #[error("serve · --bind and --workflows are an inseparable pair")]
    MissingBindOrWorkflows,
    /// A listener was requested without an owner-only token file.
    #[error("serve · --bind requires --token-file")]
    MissingTokenFile,
    /// Bounded rehearsal mode cannot own a network listener.
    #[error("serve · --once/--dry cannot bind a listener")]
    RehearsalWithListener,
    /// The injected rehearsal clock cannot drive a persistent listener.
    #[error("serve · scripted clock is the resident firer harness, not HTTP")]
    ScriptedClockWithListener,
    /// The bind string is not a socket address.
    #[error("serve · server configuration refused: bind address is invalid")]
    InvalidBind,
}

impl NikaErrorCode for ServerLaunchRefuse {
    fn nika_code(&self) -> NikaCode {
        codes::NIKA_001
    }
}

/// Validate the optional HTTP flag group without opening credentials or sockets.
///
/// `rehearsal` is true for `--once` or `--dry`; `scripted_clock` is true for
/// either injected clock bound. The validation order is part of the CLI contract.
///
/// # Errors
/// Returns a typed refusal for an incomplete or incompatible flag group.
pub fn optional_server_config(
    bind: Option<&str>,
    workflow_root: Option<&Path>,
    token_file: Option<&Path>,
    allow_remote: bool,
    rehearsal: bool,
    scripted_clock: bool,
) -> Result<Option<ServerConfig>, ServerLaunchRefuse> {
    if bind.is_none() && workflow_root.is_none() && token_file.is_none() && !allow_remote {
        return Ok(None);
    }
    let bind = bind.ok_or(ServerLaunchRefuse::MissingBindOrWorkflows)?;
    let workflow_root = workflow_root.ok_or(ServerLaunchRefuse::MissingBindOrWorkflows)?;
    let token_file = token_file.ok_or(ServerLaunchRefuse::MissingTokenFile)?;
    if rehearsal {
        return Err(ServerLaunchRefuse::RehearsalWithListener);
    }
    if scripted_clock {
        return Err(ServerLaunchRefuse::ScriptedClockWithListener);
    }
    let bind = bind.parse().map_err(|_| ServerLaunchRefuse::InvalidBind)?;
    Ok(Some(
        ServerConfig::new(bind, workflow_root, token_file).with_allow_remote(allow_remote),
    ))
}

/// Open the resident authority, optionally attach HTTP, print readiness, and
/// drive both surfaces under one shutdown boundary.
///
/// # Errors
/// Returns the typed authority, listener, execution, or shutdown refusal.
#[allow(clippy::disallowed_macros, clippy::print_stderr)]
pub async fn serve_resident(
    resident: ResidentConfig,
    server: Option<ServerConfig>,
    backend: Arc<dyn ExecutionBackend>,
    shutdown: impl Future<Output = ()>,
) -> Result<(), ServerError> {
    let authority = ResidentAuthority::open(resident, backend).await?;
    let Some(config) = server else {
        return authority.serve_until(shutdown).await;
    };
    let server = BoundServer::attach(config, &authority).await?;
    let line = match server.listen_line() {
        Ok(line) => line,
        Err(error) => {
            drop(server);
            return authority.serve_until(async {}).await.and(Err(error));
        }
    };
    eprintln!("{line}");
    authority.serve_with_http(server, shutdown).await
}

/// Compose and run the production resident process on a current-thread runtime.
///
/// # Errors
/// Returns a bounded operator-facing startup or lifecycle refusal.
pub fn serve_resident_process(
    workflow_root: &Path,
    state_root: PathBuf,
    server: Option<ServerConfig>,
) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("serve · the signal runtime refused: {error}"))?;
    let backend = Arc::new(ResidentExecutionBackend::new(workflow_root));
    let resident = ResidentConfig::new(state_root).with_workflow_root(workflow_root.to_path_buf());
    runtime
        .block_on(serve_resident(
            resident,
            server,
            backend,
            process_shutdown(),
        ))
        .map_err(server_operator_message)
}

const TOKEN_FILE_MINT: &str =
    "umask 077 && openssl rand -hex 24 > .nika/serve.token && chmod 600 .nika/serve.token";
const TOKEN_FILE_RULE: &str = "32–512 visible ASCII bytes, mode 0600, never argv";

fn token_file_refused(prefix: &str) -> String {
    format!("serve · {prefix} ({TOKEN_FILE_RULE})\n  {TOKEN_FILE_MINT}")
}

fn credential_prefix(kind: CredentialRefuse) -> &'static str {
    match kind {
        CredentialRefuse::Unreadable => "token file unreadable",
        CredentialRefuse::FollowRefused => "token file must be a regular file, not a symlink",
        CredentialRefuse::InsecureMode => "token file must be mode 0600",
        CredentialRefuse::InvalidMaterial => "token file must be 32–512 visible ASCII",
    }
}

/// Render a launch-flag refusal with the same token-file mint guidance used
/// for credential acquisition failures.
#[must_use]
pub fn launch_operator_message(error: ServerLaunchRefuse) -> String {
    match error {
        ServerLaunchRefuse::MissingTokenFile => token_file_refused("--bind requires --token-file"),
        other => other.to_string(),
    }
}

/// Render one bounded operator-facing refusal without paths or secret bytes.
#[must_use]
pub fn server_operator_message(error: ServerError) -> String {
    match error {
        ServerError::Credential(kind) => token_file_refused(credential_prefix(kind)),
        other => format!("serve · {other}"),
    }
}

/// Resolve on Ctrl-C or SIGTERM for a production resident process.
pub async fn process_shutdown() {
    #[cfg(unix)]
    {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            () = async {
                if let Some(signal) = term.as_mut() {
                    signal.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use nika_error::prelude::{NikaErrorCode as _, codes};

    use super::ServerLaunchRefuse;

    #[test]
    fn launch_refusals_share_the_validation_wire_code() {
        let refusals = [
            ServerLaunchRefuse::MissingBindOrWorkflows,
            ServerLaunchRefuse::MissingTokenFile,
            ServerLaunchRefuse::RehearsalWithListener,
            ServerLaunchRefuse::ScriptedClockWithListener,
            ServerLaunchRefuse::InvalidBind,
        ];
        assert!(
            refusals
                .into_iter()
                .all(|refusal| refusal.nika_code() == codes::NIKA_001)
        );
    }
}
