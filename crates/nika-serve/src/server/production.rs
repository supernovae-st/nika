// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production composition for the resident authority.

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use nika_dap::journal::TraceFileSink;
use nika_error::prelude::{NikaCode, NikaErrorCode, codes};
use nika_service_execution::{
    ServiceExecutionDriver, ServiceExecutionOptions, ServiceExecutionResult, ServiceExecutionStatus,
};
use nika_types::cancel::CancelCtx;

use super::{
    BoundServer, CostAuthority, CredentialRefuse, ExecutionBackend, ExecutionDisposition,
    ExecutionOutcome, ResidentAuthority, ResidentConfig, ServerConfig, ServerError,
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
        access_pin: Option<&str>,
        (inputs, cancel): (BTreeMap<String, serde_json::Value>, Option<CancelCtx>),
        authority: Option<CostAuthority>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let display_root = self.display_root.clone();
        let seal = Arc::clone(&self.seal);
        let access_pin = access_pin.map(str::to_owned);
        Box::pin(async move {
            drive_resident_execution(
                display_root,
                seal,
                context,
                max_cost_usd,
                access_pin.as_deref(),
                (inputs, cancel),
                authority,
            )
            .await
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
        self.drive(context, None, None, (BTreeMap::new(), None), None)
    }

    fn execute_with_cancel<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        cancel: CancelCtx,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(
            context,
            max_cost_usd,
            None,
            (BTreeMap::new(), Some(cancel)),
            None,
        )
    }

    fn execute_with_max_cost<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(context, max_cost_usd, None, (BTreeMap::new(), None), None)
    }

    fn execute_with_access<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        access_pin: Option<&str>,
        cancel: CancelCtx,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(
            context,
            max_cost_usd,
            access_pin,
            (BTreeMap::new(), Some(cancel)),
            None,
        )
    }

    fn execute_with_inputs<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        max_cost_usd: Option<f64>,
        access_pin: Option<&str>,
        inputs: &BTreeMap<String, serde_json::Value>,
        cancel: CancelCtx,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        self.drive(
            context,
            max_cost_usd,
            access_pin,
            (inputs.clone(), Some(cancel)),
            None,
        )
    }

    fn execute_reviewed<'a>(
        &'a self,
        context: nika_execution::ExecutionContext<'a>,
        inputs: &BTreeMap<String, serde_json::Value>,
        cancel: CancelCtx,
        authority: CostAuthority,
    ) -> std::pin::Pin<Box<dyn Future<Output = ExecutionOutcome> + Send + 'a>> {
        let pair = (inputs.clone(), Some(cancel));
        self.drive(context, None, None, pair, Some(authority))
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
    access_pin: Option<&str>,
    (inputs, operator_cancel): (BTreeMap<String, serde_json::Value>, Option<CancelCtx>),
    authority: Option<CostAuthority>,
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
    // CLI door does. A job body `access` is `--access` (a pin is a pin);
    // absent, the unpinned plan. No silent substitution after admission.
    // A reviewed job (C6) runs the plan its review judged, with its account.
    let (plan, cost) = if let Some(CostAuthority { plan, cost }) = authority {
        (plan, Some(cost))
    } else {
        let plan = driver.resolve_access_plan(None, access_pin);
        match unreviewed_cost(&display_root, &driver, &plan, &inputs) {
            Ok(cost) => (plan, cost),
            Err(why) => return ExecutionOutcome::failed("admission_refused", why),
        }
    };
    // Only a fresh unknown-cost choice replaces the run's spend ceiling, as
    // in `nika run`; an observer keeps it.
    let reviewed = cost
        .as_ref()
        .is_some_and(|c| !c.account.observes_declared_free_only());
    let max_cost_usd = max_cost_usd.filter(|_| !reviewed);
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let _cancel = CancelOnDrop(Some(cancel_tx));
    let job = ResidentJob {
        driver,
        cost,
        plan,
        mirror,
        journal,
        seal,
        snapshot_digest,
        display_root,
        operator_cancel,
        max_cost_usd,
        inputs,
    };
    match tokio::task::spawn_blocking(move || run_admitted_resident_job(job, cancel_rx)).await {
        Ok(outcome) => outcome,
        Err(_) => ExecutionOutcome::failed("NIKA-COMP-001", "execution worker did not finish"),
    }
}

/// A job with no reviewed authority (manual, snapshot or scheduled) goes
/// through the one host evaluator `nika run` uses: exact priced and local
/// routes keep their composition, declared-free or run-time routes bind the
/// per-Run observer, and an unknown-cost route refuses before the worker
/// starts, after the evaluator recorded what earlier Runs left.
fn unreviewed_cost(
    root: &Path,
    driver: &ServiceExecutionDriver,
    plan: &nika_service_execution::ExecutionAccessPlan,
    inputs: &BTreeMap<String, serde_json::Value>,
) -> Result<Option<nika_cli_host::run_cost::RunCost>, String> {
    use nika_cli_host::run_cost::{RunCostPlan, prepare};
    use nika_providers::admission::{CapEvidence, CostHostEvidence};
    let ask = || Err(UNREVIEWED.to_owned());
    let evidence = CostHostEvidence::new(
        false,
        CapEvidence::Unknown,
        CapEvidence::Unknown,
        CapEvidence::Unknown,
    );
    let execution = driver.execution_id().to_string();
    let source = driver.root_source();
    match prepare(
        root,
        Some(root),
        source,
        execution,
        driver.workflow(),
        None,
        plan,
        inputs,
        None,
        (evidence, &ask),
    )? {
        RunCostPlan::Unneeded => Ok(None),
        RunCostPlan::Observer(cost) | RunCostPlan::Zero(cost) => Ok(Some(*cost)),
        _ => Err(UNREVIEWED.to_owned()),
    }
}

const UNREVIEWED: &str = "price unknown: this Run's exact route needs a fresh one-time cost review, which a job without one never obtains (a server started with --cost-review admits POST /v1/cost-reviews, then one job carrying that review); scheduled occurrences stay refused";

/// Everything the blocking worker holds for one admitted job.
struct ResidentJob {
    driver: ServiceExecutionDriver,
    /// The Run's account (a reviewed choice or the observer), settled at its end.
    cost: Option<nika_cli_host::run_cost::RunCost>,
    plan: nika_service_execution::ExecutionAccessPlan,
    mirror: nika_service_execution::MirrorFactory,
    journal: Arc<Mutex<TraceFileSink>>,
    seal: Arc<dyn JournalSeal>,
    snapshot_digest: String,
    display_root: PathBuf,
    operator_cancel: Option<CancelCtx>,
    max_cost_usd: Option<f64>,
    inputs: BTreeMap<String, serde_json::Value>,
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
        cost,
        plan,
        mirror,
        journal,
        seal,
        snapshot_digest,
        display_root,
        operator_cancel,
        max_cost_usd,
        inputs,
    } = job;
    // Defaults retain file provenance; explicit HTTP bindings are the API
    // caller's, never a person, CI context or environment read from here.
    let origins = driver.caller_origins(&inputs, nika_types::InputOrigin::ApiCaller);
    let options = ServiceExecutionOptions::new()
        .with_inputs(inputs)
        .with_input_origins(origins)
        .with_max_cost_usd(max_cost_usd)
        .with_access_plan(plan)
        .with_mirror(mirror)
        .with_cancel_option(operator_cancel.clone());
    let options = match &cost {
        Some(cost) => options.with_runtime_config(cost.config.clone()),
        None => options,
    };
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
            match settle_journal(&journal, seal.as_ref(), &facts) {
                Ok(Some(head)) => mapped = mapped.with_chain_head(head),
                Err(evidence) => mapped = mapped.with_evidence(evidence),
                Ok(None) => {}
            }
            // The account's final row, uncertainty included; a failure is
            // possible billing the job must name (as `nika run` does).
            match cost.as_ref().map(nika_cli_host::run_cost::RunCost::finish) {
                Some(Err(why)) => mapped.with_error(
                    "cost_observation_failed",
                    format!("the Run may have been billed; its cost observation failed: {why}"),
                ),
                _ => mapped,
            }
        }
        Some(Err(_)) => {
            ExecutionOutcome::failed("NIKA-COMP-001", "service runtime could not be composed")
        }
        None => {
            // The resident stopped a run the runtime never settled (the cancel
            // grace · the execution ceiling · shutdown): its END, by DAP's law.
            let operator = operator_cancel
                .as_ref()
                .is_some_and(CancelCtx::is_cancelled);
            journal_guard(&journal).interrupt(driver.execution_id(), operator);
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
/// no journal was opened (a refusal before any event). A failed lane returns
/// a path-free loss record, including an error before the first file opened.
fn settle_journal(
    journal: &Mutex<TraceFileSink>,
    seal: &dyn JournalSeal,
    facts: &SealFacts<'_>,
) -> Result<Option<String>, crate::JournalEvidence> {
    let settled = journal_guard(journal).settle_sealed(|trace| {
        let workflow_hash = nika_dap::seal::workflow_hash(facts.driver.workflow());
        let teardown = resident_teardown(facts);
        seal.seal(trace, workflow_hash.as_deref(), Some(&teardown));
    });
    settled.map_err(|error| crate::JournalEvidence::from_error(&error))
}

/// The teardown facts the seal binds, folded by DAP
/// ([`nika_dap::seal::SealTeardown::served`], the CLI's `attended_facts` as far
/// as the service boundary carries them) from what this door holds at
/// settlement: the outcome word it maps from the service status and the SDK
/// receipt binding the local door signs too.
fn resident_teardown(facts: &SealFacts<'_>) -> nika_dap::seal::SealTeardown {
    let outcome = match facts.outcome.status() {
        ServiceExecutionStatus::Succeeded => "completed",
        ServiceExecutionStatus::Paused => "paused",
        _ => "failed",
    };
    let driver = facts.driver;
    let binding = nika_cli_host::run_settlement::local_receipt_binding(
        driver.execution_id(),
        facts.snapshot_digest,
    );
    nika_dap::seal::SealTeardown::served(
        driver.workflow(),
        driver.report(),
        outcome,
        facts.outcome.settlement(),
        binding,
        facts.display_root,
    )
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
    /// `--run-cost-ceiling` is neither a finite non-negative USD amount nor `none`.
    #[error(
        "serve · --run-cost-ceiling takes a finite non-negative USD amount (0 vetoes every priced run) or `none` (the only disarm)"
    )]
    InvalidRunCostCeiling,
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
    let limits = server.as_ref().and_then(ServerConfig::resident_limits);
    let resident = resident.with_limits(limits.unwrap_or_default());
    runtime
        .block_on(serve_resident(
            resident,
            server,
            backend,
            process_shutdown(),
        ))
        .map_err(server_operator_message)
}

/// Apply `nika serve --cost-review` and `--run-cost-ceiling <USD|none>` to the
/// optional listener: the door is seated only on request, and the per-run
/// ceiling changes only by an explicit amount or an explicit `none` (an
/// operator disarm for every manual job). Never implied by `--cost-review`.
///
/// # Errors
/// A ceiling that is not a finite non-negative USD amount or `none` (0 is a
/// binding ceiling, never a disarm), or either option without a listener.
pub fn seat_cost_review(
    server: Option<ServerConfig>,
    cost_review: bool,
    run_cost_ceiling: Option<&str>,
) -> Result<Option<ServerConfig>, ServerLaunchRefuse> {
    let Some(server) = server else {
        return if cost_review || run_cost_ceiling.is_some() {
            Err(ServerLaunchRefuse::MissingBindOrWorkflows)
        } else {
            Ok(None)
        };
    };
    let server = server.with_cost_review(cost_review);
    let ceiling = match run_cost_ceiling {
        None => return Ok(Some(server)),
        Some("none") => None,
        Some(amount) => Some(
            amount
                .parse::<f64>()
                .ok()
                .filter(|usd| usd.is_finite() && *usd >= 0.0)
                .ok_or(ServerLaunchRefuse::InvalidRunCostCeiling)?,
        ),
    };
    Ok(Some(server.with_run_cost_ceiling(ceiling)))
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
            ServerLaunchRefuse::InvalidRunCostCeiling,
        ];
        assert!(
            refusals
                .into_iter()
                .all(|refusal| refusal.nika_code() == codes::NIKA_001)
        );
    }
}

#[cfg(test)]
#[path = "production_cost_tests.rs"]
mod cost_tests;
