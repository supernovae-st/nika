// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared production execution driver for admitted, byte-owned worlds.
//!
//! The driver is intentionally filesystem-blind. An L4 adapter readmits an
//! owned snapshot through `nika-execution`, then this L3 crate composes both
//! the root runtime and every nested `workflow:` call through the same
//! production composition root.
//!
//! Descended out of `nika-runtime` at the 15k prod-LOC wall (the
//! `nika-proof` / `nika-secret` precedent). It sits at the SAME layer as
//! `nika-runtime` and `nika-execution` and joins them: the runtime owns
//! generic wave-ordered execution, `nika-execution` owns byte admission, and
//! this crate is the ONE driver both the local CLI/ARM surface and the
//! service surface share.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use nika_event::Event;
use nika_event::source_id::{lf_normal_form, sha256_hex};
use nika_execution::{ExecutionContext, ExecutionSnapshot};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::types::Permits;
use nika_schema::{FileId, ParseMode, ResolvedSkills};
use nika_types::id::{CorrelationId, EventId, ExecutionId, RunId};
use nika_types::timestamp::Timestamp;
use serde_json::Value;

use nika_runtime::child::{
    ChildCall, ChildOutcome, ChildRunRefusal, ChildRunSummary, ChildRunner, MAX_RUN_DEPTH,
};
use nika_runtime::compose::{
    ComposeError, ProdRuntime, RuntimeCapabilities, fs_boundary_of_permits,
    net_boundary_of_permits, production_runtime, service_runtime,
};
use nika_runtime::{EventSink, InputOrigin, RunOutcome, RunSeams, RuntimeError, Stamper};

/// Metadata a child trace lane commits into its parent's trace-forest row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildTraceMetadata {
    trace_id: Option<String>,
    chain_head: Option<String>,
}

impl ChildTraceMetadata {
    /// Construct metadata from a trace identity and chain head.
    #[must_use]
    pub fn new(trace_id: Option<String>, chain_head: Option<String>) -> Self {
        Self {
            trace_id,
            chain_head,
        }
    }

    /// Child trace identity, when the injected trace lane created one.
    #[must_use]
    pub fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    /// Child trace chain head, when the injected lane maintains one.
    #[must_use]
    pub fn chain_head(&self) -> Option<&str> {
        self.chain_head.as_deref()
    }
}

/// One child run's injected trace lane.
pub trait ChildTrace {
    /// Event sink driven by the child runtime.
    fn sink(&mut self) -> &mut dyn EventSink;

    /// Metadata committed into the parent after the child settles.
    fn metadata(&self) -> ChildTraceMetadata;
}

/// Factory for child trace lanes.
///
/// CLI and ARM inject the existing `TraceFileSink` adapter. Service execution
/// defaults to a silent lane, so no path or raw event payload is invented at
/// the remote boundary.
pub trait ChildTraceFactory: Send + Sync {
    /// Create one independent trace lane for one nested run.
    fn create(&self) -> Box<dyn ChildTrace>;
}

#[derive(Default)]
struct SilentSink;

impl EventSink for SilentSink {
    fn emit(&mut self, _event: Event) {}
}

struct SilentChildTrace {
    sink: SilentSink,
}

impl ChildTrace for SilentChildTrace {
    fn sink(&mut self) -> &mut dyn EventSink {
        &mut self.sink
    }

    fn metadata(&self) -> ChildTraceMetadata {
        ChildTraceMetadata::default()
    }
}

struct SilentChildTraceFactory;

impl ChildTraceFactory for SilentChildTraceFactory {
    fn create(&self) -> Box<dyn ChildTrace> {
        Box::new(SilentChildTrace { sink: SilentSink })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriverSurface {
    Service,
    Local,
}

/// Production execution over one readmitted immutable world.
///
/// `new` selects the service-safe surface: builtin log/emit payloads are not
/// copied to stderr, child traces are silent unless explicitly injected, and
/// [`Self::execute`] returns metadata-only events. CLI/ARM use
/// [`Self::for_local_interface`] plus their existing trace factory while
/// sharing every production effect and child-composition decision.
#[non_exhaustive]
pub struct ServiceExecutionDriver {
    snapshot: ExecutionSnapshot,
    root_logical: String,
    root_source: String,
    workflow: RawWorkflow,
    report: nika_check::CheckReport,
    skills: ResolvedSkills,
    execution_id: ExecutionId,
    display_root: PathBuf,
    child_traces: Arc<dyn ChildTraceFactory>,
    surface: DriverSurface,
}

impl std::fmt::Debug for ServiceExecutionDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceExecutionDriver")
            .field("root", &self.root_logical)
            .field("root_bytes", &self.root_source.len())
            .field("surface", &self.surface)
            .finish_non_exhaustive()
    }
}

impl Clone for ServiceExecutionDriver {
    fn clone(&self) -> Self {
        Self {
            snapshot: self.snapshot.clone(),
            root_logical: self.root_logical.clone(),
            root_source: self.root_source.clone(),
            workflow: self.workflow.clone(),
            report: self.report.clone(),
            skills: self.skills.clone(),
            execution_id: self.execution_id,
            display_root: self.display_root.clone(),
            child_traces: Arc::clone(&self.child_traces),
            surface: self.surface,
        }
    }
}

impl ServiceExecutionDriver {
    /// Construct the service-safe driver from a readmitted execution context.
    ///
    /// The context is the unforgeable authority joining snapshot bytes, parsed
    /// workflow, check report, resolved skills, and execution identity. No
    /// independently parsed workflow or report is accepted by this driver.
    /// Returns `None` only if an internal admission invariant lost the root
    /// unit; no fallback reader is accepted.
    #[must_use]
    pub fn new(context: ExecutionContext<'_>, display_root: impl Into<PathBuf>) -> Option<Self> {
        Self::with_surface(context, display_root.into(), DriverSurface::Service)
    }

    /// Construct the local CLI/ARM surface over the same L3 driver.
    ///
    /// Local production keeps the existing stderr `nika:log`/`nika:emit`
    /// behavior. All definition reads and child composition remain identical
    /// to [`Self::new`].
    #[must_use]
    pub fn for_local_interface(
        context: ExecutionContext<'_>,
        display_root: impl Into<PathBuf>,
    ) -> Option<Self> {
        Self::with_surface(context, display_root.into(), DriverSurface::Local)
    }

    fn with_surface(
        context: ExecutionContext<'_>,
        display_root: PathBuf,
        surface: DriverSurface,
    ) -> Option<Self> {
        let snapshot = context.snapshot().clone();
        let root_logical = snapshot.root().to_owned();
        let root_source = snapshot.text(&root_logical)?.to_owned();
        let workflow = context.workflow().clone();
        let mut report = context.check().clone();
        stamp_workflow_semantic(&workflow, &mut report);
        Some(Self {
            snapshot,
            root_logical,
            root_source,
            workflow,
            report,
            skills: context.skills().clone(),
            execution_id: context.execution_id(),
            display_root,
            child_traces: Arc::new(SilentChildTraceFactory),
            surface,
        })
    }

    /// Inject the local interface's child trace implementation.
    #[must_use]
    pub fn with_child_trace_factory(mut self, factory: Arc<dyn ChildTraceFactory>) -> Self {
        self.child_traces = factory;
        self
    }

    /// Restrict the admitted program to one task's ancestor cone.
    ///
    /// The cone and its fresh report are derived internally from the workflow
    /// admitted with this driver's snapshot. Callers can select a task but
    /// cannot inject a replacement graph or report.
    ///
    /// # Errors
    ///
    /// Returns the existing task-scope refusal when `target` is unknown.
    pub fn with_task_scope(mut self, target: Option<&str>) -> Result<Self, String> {
        let Some(target) = target else {
            return Ok(self);
        };
        self.workflow = nika_runtime::scope_to_task(self.workflow, target)?;
        self.report = nika_check::check(&self.workflow);
        stamp_workflow_semantic(&self.workflow, &mut self.report);
        Ok(self)
    }

    /// Parsed workflow derived from the owned readmitted root bytes.
    #[must_use]
    pub const fn workflow(&self) -> &RawWorkflow {
        &self.workflow
    }

    /// Check report derived from the same admitted workflow.
    #[must_use]
    pub const fn report(&self) -> &nika_check::CheckReport {
        &self.report
    }

    /// Skills resolved from the same readmitted snapshot.
    #[must_use]
    pub const fn skills(&self) -> &ResolvedSkills {
        &self.skills
    }

    /// Exact root source owned by the readmitted snapshot.
    #[must_use]
    pub fn root_source(&self) -> &str {
        &self.root_source
    }

    /// Execution identity minted by snapshot readmission.
    #[must_use]
    pub const fn execution_id(&self) -> ExecutionId {
        self.execution_id
    }

    /// Compose the real production runtime plus snapshot-backed child runner.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] when the production HTTP or sandbox seams
    /// cannot be built under the workflow's declared capabilities.
    pub fn compose(&self, default_model: &str) -> Result<AuthorizedRuntime, ComposeError> {
        let caps = nika_runtime::compose::capabilities_of(&self.workflow);
        let runtime = self.base_runtime(
            default_model,
            caps,
            self.workflow.run.as_ref().map(|run| &run.value),
        )?;
        let raw_sha = sha256_hex(self.root_source.as_bytes());
        let lf_sha = sha256_hex(lf_normal_form(&self.root_source).as_bytes());
        let runtime = runtime
            .with_child_runner(Arc::new(self.clone()))
            .with_child_closures(admitted_closure_digests(
                &self.workflow,
                &self.snapshot,
                &self.root_logical,
            ))
            .with_source_sha256(raw_sha.clone())
            .with_skills(self.skills.texts.clone());
        let runtime = if lf_sha == raw_sha {
            runtime
        } else {
            runtime.with_source_sha256_lf(lf_sha)
        };
        Ok(AuthorizedRuntime::new(
            runtime,
            self.workflow.clone(),
            self.report.clone(),
        ))
    }

    fn base_runtime(
        &self,
        default_model: &str,
        caps: RuntimeCapabilities,
        run: Option<&nika_schema::types::RunDecl>,
    ) -> Result<ProdRuntime, ComposeError> {
        match self.surface {
            DriverSurface::Service => {
                service_runtime(default_model, caps, run, self.display_root.clone())
            }
            DriverSurface::Local => production_runtime(default_model, caps, run),
        }
    }

    /// Execute through the service-safe composition and metadata projection.
    ///
    /// A runtime refusal is projected to the result's redacted `Refused`
    /// status, while construction failures return [`ComposeError`] before any
    /// event or workflow effect.
    ///
    /// # Errors
    ///
    /// Returns [`ComposeError`] when production composition fails.
    pub async fn execute(
        &self,
        options: ServiceExecutionOptions,
    ) -> Result<ServiceExecutionResult, ComposeError> {
        let envelope_model = self
            .workflow
            .model
            .as_ref()
            .map_or("", |model| model.value.as_str());
        let default_model = options.model_override.as_deref().unwrap_or(envelope_model);
        let runtime = self
            .compose(default_model)?
            .with_var_overrides(options.inputs)
            .with_input_origins(options.input_origins)
            .with_max_cost_usd(options.max_cost_usd)
            .with_prompt_pause(true)
            .with_model_override(options.model_override);
        let mut stamper = RunSeams::of(self.workflow.run.as_ref().map(|run| &run.value)).stamper();
        let mut sink = ServiceEventSink::new(Some(self.execution_id));
        let outcome = runtime.run(stamper.as_mut(), &mut sink).await;
        Ok(ServiceExecutionResult::from_runtime(
            &outcome,
            sink.into_events(),
        ))
    }

    #[allow(clippy::type_complexity)] // one child admission tuple is the seam
    fn load_child(
        &self,
        call: &ChildCall,
    ) -> Result<(String, String, RawWorkflow, nika_check::CheckReport), ChildRunRefusal> {
        if call.target.starts_with("registry:") {
            return Err(refusal(
                "NIKA-COMP-001",
                format!(
                    "`{}` — registry child execution lands with the registry lane; run a filesystem child today (spec 14 §the form)",
                    call.target
                ),
            ));
        }
        let logical = resolve_logical(&self.root_logical, &call.target).map_err(|detail| {
            refusal(
                "NIKA-COMP-001",
                format!("cannot resolve child `{}`: {detail}", call.target),
            )
        })?;
        let path = self.display_root.join(&logical);
        let source = self.snapshot.text(&logical).ok_or_else(|| {
            refusal(
                "NIKA-COMP-001",
                format!("captured world has no child `{}`", path.display()),
            )
        })?;
        let source = source.to_owned();
        let workflow =
            nika_schema::parse(&source, FileId::new(0), ParseMode::Strict).map_err(|error| {
                refusal(
                    "NIKA-COMP-001",
                    format!("child `{}` does not parse: {error}", path.display()),
                )
            })?;
        let report = nika_check::check_composed(&workflow, &logical, &mut |path| {
            self.snapshot
                .text(path)
                .map(str::to_owned)
                .ok_or_else(|| format!("captured world has no unit `{path}`"))
        });
        if !report.is_clean() {
            let (code, detail) = first_finding(&report);
            return Err(refusal(
                &code,
                format!("child `{}` fails its own check: {detail}", path.display()),
            ));
        }
        Ok((logical, source, workflow, report))
    }
}

/// Production runtime whose workflow and report are sealed to one readmitted
/// snapshot.
///
/// The wrapper deliberately exposes no constructor and no way to replace its
/// workflow, report, skills, closure digests, or source hashes. Composition
/// knobs may refine a run, while [`Self::run`] always executes the internally
/// bound pair.
#[non_exhaustive]
pub struct AuthorizedRuntime {
    runtime: ProdRuntime,
    workflow: RawWorkflow,
    report: nika_check::CheckReport,
}

impl std::fmt::Debug for AuthorizedRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedRuntime")
            .field("task_count", &self.workflow.tasks.len())
            .field("report_clean", &self.report.is_clean())
            .finish_non_exhaustive()
    }
}

impl AuthorizedRuntime {
    fn new(runtime: ProdRuntime, workflow: RawWorkflow, report: nika_check::CheckReport) -> Self {
        Self {
            runtime,
            workflow,
            report,
        }
    }

    /// Execute the internally bound workflow/report pair.
    ///
    /// # Errors
    ///
    /// Returns the runtime's typed admission or execution refusal.
    pub async fn run(
        &self,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<RunOutcome, RuntimeError> {
        self.runtime
            .run(&self.workflow, &self.report, stamper, sink)
            .await
    }

    /// Attach validated workflow input overrides.
    #[must_use]
    pub fn with_var_overrides(mut self, overrides: BTreeMap<String, Value>) -> Self {
        self.runtime = self.runtime.with_var_overrides(overrides);
        self
    }

    /// Attach input-origin attestations.
    #[must_use]
    pub fn with_input_origins(mut self, origins: BTreeMap<String, InputOrigin>) -> Self {
        self.runtime = self.runtime.with_input_origins(origins);
        self
    }

    /// Attach the run's spend ceiling.
    #[must_use]
    pub fn with_max_cost_usd(mut self, budget: Option<f64>) -> Self {
        self.runtime = self.runtime.with_max_cost_usd(budget);
        self
    }

    /// Enable durable prompt pauses.
    #[must_use]
    pub fn with_prompt_pause(mut self, pause: bool) -> Self {
        self.runtime = self.runtime.with_prompt_pause(pause);
        self
    }

    /// Attach validated prompt answers.
    #[must_use]
    pub fn with_prompt_answers(mut self, answers: BTreeMap<String, Value>) -> Self {
        self.runtime = self.runtime.with_prompt_answers(answers);
        self
    }

    /// Attach the folded approval authority for a resumed pause.
    #[must_use]
    pub fn with_paused_approval(
        mut self,
        paused: Option<nika_runtime::approval::PausedApproval>,
    ) -> Self {
        self.runtime = self.runtime.with_paused_approval(paused);
        self
    }

    /// Attach the declared cross-version resume compatibility.
    #[must_use]
    pub fn with_resume_compat(mut self, compat: Option<String>) -> Self {
        self.runtime = self.runtime.with_resume_compat(compat);
        self
    }

    /// Attach the resume's unverified-trust attestation.
    #[must_use]
    pub fn with_resume_unverified(
        mut self,
        unverified: Option<nika_runtime::resume::ResumeUnverified>,
    ) -> Self {
        self.runtime = self.runtime.with_resume_unverified(unverified);
        self
    }

    /// Attach the effective model override.
    #[must_use]
    pub fn with_model_override(mut self, model: Option<String>) -> Self {
        self.runtime = self.runtime.with_model_override(model);
        self
    }

    /// Attach the operator's access-path pin.
    #[must_use]
    pub fn with_access_pin(mut self, pin: Option<String>) -> Self {
        self.runtime = self.runtime.with_access_pin(pin);
        self
    }

    /// Attach the access candidates judged by the run gate.
    #[must_use]
    pub fn with_access_probes(mut self, probes: Vec<nika_providers::probe::ProviderProbe>) -> Self {
        self.runtime = self.runtime.with_access_probes(probes);
        self
    }

    /// Attach composer-derived boot access fields.
    #[must_use]
    pub fn with_boot_access_fields(
        mut self,
        fields: Vec<(&'static str, nika_types::resource::Value)>,
    ) -> Self {
        self.runtime = self.runtime.with_boot_access_fields(fields);
        self
    }

    /// Seat an agentic CLI from `--access` (pin wins over env).
    ///
    /// # Errors
    ///
    /// The registry row cannot be built.
    pub fn with_harness_from_pin(
        mut self,
        pin: Option<&str>,
    ) -> Result<Self, nika_runtime::compose::ComposeError> {
        self.runtime = self.runtime.with_harness_from_pin(pin)?;
        Ok(self)
    }

    /// Attach the verified resume plan.
    #[must_use]
    pub fn with_resume_plan(mut self, plan: nika_runtime::resume::ResumePlan) -> Self {
        self.runtime = self.runtime.with_resume_plan(plan);
        self
    }
}

fn stamp_workflow_semantic(workflow: &RawWorkflow, report: &mut nika_check::CheckReport) {
    report.workflow_semantic =
        nika_runtime::proof::ir::semantic_ir_hash(workflow).map(|hash| hash.as_hex().to_owned());
}

/// Service-owned execution inputs and composition controls.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct ServiceExecutionOptions {
    inputs: BTreeMap<String, Value>,
    input_origins: BTreeMap<String, InputOrigin>,
    model_override: Option<String>,
    max_cost_usd: Option<f64>,
}

impl std::fmt::Debug for ServiceExecutionOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceExecutionOptions")
            .field("input_count", &self.inputs.len())
            .field("origin_count", &self.input_origins.len())
            .field("model_override", &self.model_override)
            .field("max_cost_usd", &self.max_cost_usd)
            .finish_non_exhaustive()
    }
}

impl ServiceExecutionOptions {
    /// Construct service options with no caller overrides.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Supply already validated workflow input overrides.
    #[must_use]
    pub fn with_inputs(mut self, inputs: BTreeMap<String, Value>) -> Self {
        self.inputs = inputs;
        self
    }

    /// Supply the provenance labels paired with input overrides.
    #[must_use]
    pub fn with_input_origins(mut self, origins: BTreeMap<String, InputOrigin>) -> Self {
        self.input_origins = origins;
        self
    }

    /// Override the workflow's default model.
    #[must_use]
    pub fn with_model_override(mut self, model: Option<String>) -> Self {
        self.model_override = model;
        self
    }

    /// Apply the service admission's spend ceiling.
    #[must_use]
    pub fn with_max_cost_usd(mut self, max_cost_usd: Option<f64>) -> Self {
        self.max_cost_usd = max_cost_usd;
        self
    }
}

/// One runtime event projected for a remote service boundary.
///
/// Runtime fields are deliberately absent. This preserves ordering and typed
/// identity while refusing task output, provider/tool payloads, prompt text,
/// failure detail, filesystem paths, and any future unreviewed field by
/// default.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServiceEvent {
    id: EventId,
    timestamp: Timestamp,
    kind: &'static str,
    run: Option<RunId>,
    execution: Option<ExecutionId>,
    correlation: Option<CorrelationId>,
}

impl ServiceEvent {
    /// Construct an event from conservative service-boundary metadata.
    #[must_use]
    pub const fn new(
        id: EventId,
        timestamp: Timestamp,
        kind: &'static str,
        run: Option<RunId>,
        execution: Option<ExecutionId>,
        correlation: Option<CorrelationId>,
    ) -> Self {
        Self {
            id,
            timestamp,
            kind,
            run,
            execution,
            correlation,
        }
    }

    /// Event identity.
    #[must_use]
    pub const fn id(&self) -> EventId {
        self.id
    }

    /// Event timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Stable event-kind slug.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        self.kind
    }

    /// Runtime run identity, when emitted.
    #[must_use]
    pub const fn run(&self) -> Option<RunId> {
        self.run
    }

    /// Service execution identity.
    #[must_use]
    pub const fn execution(&self) -> Option<ExecutionId> {
        self.execution
    }

    /// Runtime correlation identity, when emitted.
    #[must_use]
    pub const fn correlation(&self) -> Option<CorrelationId> {
        self.correlation
    }
}

struct ServiceEventSink {
    execution: Option<ExecutionId>,
    events: Vec<ServiceEvent>,
}

impl ServiceEventSink {
    fn new(execution: Option<ExecutionId>) -> Self {
        Self {
            execution,
            events: Vec::new(),
        }
    }

    fn into_events(self) -> Vec<ServiceEvent> {
        self.events
    }
}

impl EventSink for ServiceEventSink {
    fn emit(&mut self, event: Event) {
        self.events.push(ServiceEvent::new(
            event.id,
            event.timestamp,
            event.kind.as_str(),
            event.run,
            self.execution.or(event.execution),
            event.correlation,
        ));
    }
}

/// Conservative service-boundary execution status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceExecutionStatus {
    /// The workflow completed successfully.
    Succeeded,
    /// The workflow settled unsuccessfully without exposing task failures.
    Failed,
    /// The workflow paused without exposing the pause payload.
    Paused,
    /// The runtime refused execution without exposing its typed error.
    Refused,
}

/// Service execution's redacted status and conservative event projection.
#[non_exhaustive]
pub struct ServiceExecutionResult {
    status: ServiceExecutionStatus,
    events: Vec<ServiceEvent>,
}

impl std::fmt::Debug for ServiceExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceExecutionResult")
            .field("status", &self.status)
            .field("event_count", &self.events.len())
            .finish_non_exhaustive()
    }
}

impl ServiceExecutionResult {
    /// Construct a redacted status and metadata-only event projection.
    #[must_use]
    pub fn new(status: ServiceExecutionStatus, events: Vec<ServiceEvent>) -> Self {
        Self { status, events }
    }

    fn from_runtime(outcome: &Result<RunOutcome, RuntimeError>, events: Vec<ServiceEvent>) -> Self {
        let status = match outcome {
            Err(_) => ServiceExecutionStatus::Refused,
            Ok(outcome) if outcome.paused.is_some() => ServiceExecutionStatus::Paused,
            Ok(outcome) if outcome.ok && !outcome.budget_exceeded => {
                ServiceExecutionStatus::Succeeded
            }
            Ok(_) => ServiceExecutionStatus::Failed,
        };
        Self::new(status, events)
    }

    /// Redacted terminal status.
    #[must_use]
    pub const fn status(&self) -> ServiceExecutionStatus {
        self.status
    }

    /// Borrow the ordered metadata-only event projection.
    #[must_use]
    pub fn events(&self) -> &[ServiceEvent] {
        &self.events
    }

    /// Consume the result into its redacted status and event projection.
    #[must_use]
    pub fn into_parts(self) -> (ServiceExecutionStatus, Vec<ServiceEvent>) {
        (self.status, self.events)
    }
}

impl ChildRunner for ServiceExecutionDriver {
    fn run_child<'a>(
        &'a self,
        call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        Box::pin(async move {
            let (logical, source, workflow, report) = self.load_child(&call)?;
            let child_permits = effective_permits(
                workflow.permits.as_ref().map(|permits| &permits.value),
                call.parent_permits.as_ref(),
            );
            let caps = RuntimeCapabilities {
                fs: fs_boundary_of_permits(Some(&child_permits)),
                net: net_boundary_of_permits(Some(&child_permits)),
                exec_tasks: workflow
                    .tasks
                    .iter()
                    .any(|task| matches!(task.value.action, RawAction::Exec(_))),
                permits_declared: workflow.permits.is_some() || call.parent_permits.is_some(),
            };
            let model = workflow
                .model
                .as_ref()
                .map_or("", |model| model.value.as_str());
            let runtime = self
                .base_runtime(model, caps, workflow.run.as_ref().map(|run| &run.value))
                .map_err(|error| refusal("NIKA-COMP-001", format!("child runtime: {error}")))?
                .with_var_overrides(call.args.clone().into_iter().collect())
                .with_max_cost_usd(call.remaining_budget_usd)
                .with_source_sha256(sha256_hex(source.as_bytes()))
                .with_run_depth(call.depth);
            let mut reader = |authored: &str| {
                let skill = resolve_logical(&logical, authored)?;
                self.snapshot
                    .text(&skill)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("captured world has no unit `{skill}`"))
            };
            let mut child_driver = self.clone();
            child_driver.root_logical.clone_from(&logical);
            child_driver.root_source.clone_from(&source);
            let runtime = runtime
                .with_skills(nika_schema::resolve_skills(&workflow, &mut reader).texts)
                .with_child_runner(Arc::new(child_driver))
                .with_child_closures(admitted_closure_digests(
                    &workflow,
                    &self.snapshot,
                    &logical,
                ));
            let mut trace = self.child_traces.create();
            let def_hash = sha256_hex(source.as_bytes());
            let mut stamper = RunSeams::of(workflow.run.as_ref().map(|run| &run.value)).stamper();
            let run = runtime
                .run(&workflow, &report, stamper.as_mut(), trace.sink())
                .await;
            let (ok, outputs, cost, failure) = child_run_parts(&run);
            let metadata = trace.metadata();
            let trace = Some(ChildRunSummary::new(
                call.target.clone(),
                ok,
                (
                    metadata.trace_id().map(str::to_owned),
                    metadata.chain_head().map(str::to_owned),
                    Some(def_hash),
                ),
            ));
            Ok(ChildOutcome {
                ok,
                outputs,
                cost_usd: cost,
                trace,
                failure,
            })
        })
    }
}

type ChildRunParts = (
    bool,
    BTreeMap<String, Value>,
    Option<f64>,
    Option<(String, String)>,
);

fn child_run_parts(run: &Result<RunOutcome, RuntimeError>) -> ChildRunParts {
    match run {
        Ok(outcome) => {
            let ok = outcome.ok && outcome.paused.is_none() && !outcome.budget_exceeded;
            let failure = (!ok).then(|| first_failure(outcome));
            (ok, outcome.outputs.clone(), outcome.total_cost_usd, failure)
        }
        Err(error) => (
            false,
            BTreeMap::new(),
            None,
            Some((error.spec_code(), error.to_string())),
        ),
    }
}

fn effective_permits(child: Option<&Permits>, parent: Option<&Permits>) -> Permits {
    let zero = Permits::new();
    let parent = parent.unwrap_or(&zero);
    child.map_or_else(|| parent.clone(), |child| child.intersect(parent))
}

fn resolve_logical(parent: &str, target: &str) -> Result<String, String> {
    let target = Path::new(target);
    if target.is_absolute() {
        return Err("absolute paths are outside the admitted project".to_owned());
    }
    let mut parts = Path::new(parent)
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>();
    for component in target.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| "path is not valid UTF-8".to_owned())?;
                parts.push(part.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                parts
                    .pop()
                    .ok_or_else(|| "path escapes the admitted project".to_owned())?;
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("path escapes the admitted project".to_owned());
            }
        }
    }
    if parts.is_empty() {
        return Err("path resolves to the project root".to_owned());
    }
    Ok(parts.join("/"))
}

fn first_finding(report: &nika_check::CheckReport) -> (String, String) {
    report.findings.first().map_or_else(
        || ("NIKA-COMP-001".to_owned(), "unnamed finding".to_owned()),
        |finding| {
            (
                finding
                    .code
                    .clone()
                    .unwrap_or_else(|| "NIKA-COMP-001".to_owned()),
                finding.message.clone(),
            )
        },
    )
}

fn refusal(code: &str, message: String) -> ChildRunRefusal {
    ChildRunRefusal {
        code: code.to_owned(),
        message,
    }
}

fn first_failure(outcome: &RunOutcome) -> (String, String) {
    for (id, record) in &outcome.records {
        if let Some(error) = &record.error {
            return (
                error.code.clone(),
                format!("task `{id}`: {}", error.message),
            );
        }
    }
    if outcome.budget_exceeded {
        return (
            "NIKA-1704".to_owned(),
            "the inherited cost budget was exceeded (spec 14 law 6 · min(parent remaining, child declared))"
                .to_owned(),
        );
    }
    (
        "NIKA-COMP-001".to_owned(),
        "child run failed without a task error".to_owned(),
    )
}

fn admitted_closure_digests(
    workflow: &RawWorkflow,
    snapshot: &ExecutionSnapshot,
    parent_logical: &str,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for target in workflow_targets_of(workflow) {
        let Ok(resolved) = resolve_logical(parent_logical, &target) else {
            continue;
        };
        let mut stack = Vec::new();
        if let Some(digest) = closure_digest(snapshot, &resolved, &mut stack, 1) {
            out.insert(target, digest);
        }
    }
    out
}

fn workflow_targets_of(workflow: &RawWorkflow) -> Vec<String> {
    let target_of = |action: &RawAction| match action {
        RawAction::Invoke(action) => match &action.target {
            RawInvokeTarget::Workflow(workflow) => Some(workflow.value.clone()),
            RawInvokeTarget::Tool(_) => None,
        },
        _ => None,
    };
    workflow
        .tasks
        .iter()
        .filter_map(|task| target_of(&task.value.action))
        .collect()
}

fn closure_digest(
    snapshot: &ExecutionSnapshot,
    logical: &str,
    stack: &mut Vec<String>,
    depth: u32,
) -> Option<String> {
    if depth > MAX_RUN_DEPTH || stack.iter().any(|identity| identity == logical) {
        return None;
    }
    let source = snapshot.text(logical)?;
    let workflow = nika_schema::parse(source, FileId::new(0), ParseMode::Strict).ok()?;
    stack.push(logical.to_owned());
    let mut children = BTreeMap::new();
    for target in workflow_targets_of(&workflow) {
        if target.starts_with("registry:") {
            stack.pop();
            return None;
        }
        let Ok(resolved) = resolve_logical(logical, &target) else {
            stack.pop();
            return None;
        };
        let Some(digest) = closure_digest(snapshot, &resolved, stack, depth + 1) else {
            stack.pop();
            return None;
        };
        children.insert(target, digest);
    }
    stack.pop();
    let mut fold = String::from("nika-child-closure:v1\0");
    fold.push_str(&sha256_hex(source.as_bytes()));
    for (target, digest) in &children {
        fold.push('\0');
        fold.push_str(target);
        fold.push('\0');
        fold.push_str(digest);
    }
    Some(sha256_hex(fold.as_bytes()))
}

#[cfg(test)]
mod tests;
