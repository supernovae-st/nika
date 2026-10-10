// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One rehearsal run of the admitted pair through the existing runtime. The tool plane is the
//! production builtin dispatcher over the caller's filesystem (a rehearsal room), judged by the
//! workflow's own `permits.fs`, behind a gate that lets only [`ADMITTED_TOOLS`] through. Every
//! other capability is denied, and counted when the runtime reaches for it: the shell spawns
//! nothing, the fetch plane opens no socket, the provider registry has no transport, a secret
//! never resolves and a nested run never starts. A replay trial lends the fetch plane captures
//! ([`Captures`]): a plain `nika:fetch` GET of exactly a captured address is answered with that
//! capture, and every other request is refused as before, with no socket opened and no name
//! resolved. The runtime, the workflow and its report are the admitted ones: nothing here
//! interprets the candidate.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta, ToolDef};
use nika_kernel::ai::tool_defs::{ToolDefinitionProviderDyn, ToolDefsError};
use nika_kernel::fs::{FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nika_kernel::http::{
    HttpError, HttpGetDyn, HttpMethod, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse,
};
use nika_kernel::process::{ShellCommand, ShellError, ShellResult, ShellRunDyn};
use nika_kernel::provider::{InferRequest, InferResponse, ProviderError};
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_providers::{ExecutionAccessPlan, ProviderRegistry, ProvidersConfig};
use nika_runtime::child::{ChildCall, ChildOutcome, ChildRunRefusal, ChildRunner};
use nika_runtime::compose::fs_boundary_of_permits;
use nika_runtime::{
    RunOutcome, RunSeams, Runtime, RuntimeConfig, RuntimeError, SecretResolveError, TaskStatus,
    WorkflowSecretResolver,
};
use nika_schema::raw::{RawAction, RawInvokeTarget, RawTask};
use nika_schema::types::SecretRef;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use super::isolated_jq::IsolatedJq;
use super::replay::{self, Capture, Captures};
use crate::{ServiceExecutionDriver, SilentSink};

/// The builtins a rehearsal runs: the room's file reads and writes, the run's clock, its logs
/// and pure computation. A host screens a candidate against this surface before any room
/// exists; at run time the gate refuses every other tool.
pub const ADMITTED_TOOLS: &[&str] = &[
    "nika:read",
    "nika:write",
    "nika:wait",
    "nika:log",
    "nika:emit",
    "nika:hash",
    "nika:assert",
    "nika:done",
    "nika:date",
    "nika:uuid",
];

/// The sentence every denied capability of a rehearsal speaks.
pub(super) const OUTSIDE: &str = "a rehearsal runs its room's files and pure tools only";

/// What a rehearsal run's denied capabilities saw attempted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeniedEffects {
    /// Network requests: the fetch plane, `nika:fetch` or `nika:notify`, an `mcp:` call.
    pub network: u32,
    /// Provider calls: a model turn, `nika:image_generate` or `nika:tts_generate`.
    pub provider: u32,
    /// Process spawns.
    pub spawn: u32,
    /// Prompts to a person (`nika:prompt`).
    pub prompt: u32,
    /// Secret resolutions.
    pub secret: u32,
    /// Nested workflow runs.
    pub child: u32,
}

/// The counter the denied capabilities of one rehearsal run share. The host keeps it, so the
/// counts survive a run stopped at its bound.
#[derive(Debug, Default)]
pub struct DeniedTally {
    network: AtomicU32,
    provider: AtomicU32,
    spawn: AtomicU32,
    prompt: AtomicU32,
    secret: AtomicU32,
    child: AtomicU32,
}

impl DeniedTally {
    /// The attempts counted so far.
    #[must_use]
    pub fn counted(&self) -> DeniedEffects {
        let read = |counter: &AtomicU32| counter.load(Ordering::SeqCst);
        DeniedEffects {
            network: read(&self.network),
            provider: read(&self.provider),
            spawn: read(&self.spawn),
            prompt: read(&self.prompt),
            secret: read(&self.secret),
            child: read(&self.child),
        }
    }

    /// The counter of the capability a refused tool reaches for, when it reaches for one.
    fn reached(&self, tool: &str) -> Option<&AtomicU32> {
        match tool {
            "nika:fetch" | "nika:notify" => Some(&self.network),
            "nika:image_generate" | "nika:tts_generate" => Some(&self.provider),
            "nika:prompt" => Some(&self.prompt),
            mcp if mcp.starts_with("mcp:") => Some(&self.network),
            _ => None,
        }
    }

    /// The refusal of a step the trial names not run taken back, never below none: the trial's
    /// view excuses `task`, so its step's one attempt is no effect the room denied.
    fn excuse(&self, task: &RawTask) {
        let counter = match &task.action {
            RawAction::Infer(_) | RawAction::Agent(_) => Some(&self.provider),
            RawAction::Exec(_) => Some(&self.spawn),
            RawAction::Invoke(invoke) => match &invoke.target {
                RawInvokeTarget::Tool(tool) => self.reached(tool.value.as_str()),
                RawInvokeTarget::Workflow(_) => None,
            },
            _ => None,
        };
        if let Some(counter) = counter {
            let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
        }
    }
}

/// One more attempt on `counter`.
fn count(counter: &AtomicU32) {
    counter.fetch_add(1, Ordering::SeqCst);
}

impl ServiceExecutionDriver {
    /// Run the admitted workflow and report once, as a rehearsal: through the runtime over `fs`
    /// (the room), judged by the workflow's own `permits.fs`, under `plan` (resolved by
    /// [`Self::rehearsal_plan`]), every denied attempt counted in `tally`.
    ///
    /// The future is not `Send`, a run's never is: drive it on a current-thread executor. A host
    /// that stops it at a bound drops it where it stands, then drains what it had started before
    /// it reads the room.
    ///
    /// # Errors
    /// The runtime's own launch refusal, before any task. A failed task is a settled outcome.
    pub async fn rehearse_over<F>(
        &self,
        fs: Arc<F>,
        plan: ExecutionAccessPlan,
        tally: Arc<DeniedTally>,
    ) -> Result<RunOutcome, RuntimeError>
    where
        F: FsReadDyn + FsWriteDyn + FsListDyn + FsMetaDyn + Send + Sync + 'static,
    {
        self.rehearse_over_with(fs, plan, tally, None).await
    }

    /// [`Self::rehearse_over`], its `nika:jq` calls evaluated by `jq` when one is given (a
    /// bounded process of the host's own), refused as outside the surface otherwise.
    ///
    /// # Errors
    /// As [`Self::rehearse_over`].
    pub async fn rehearse_over_with<F>(
        &self,
        fs: Arc<F>,
        plan: ExecutionAccessPlan,
        tally: Arc<DeniedTally>,
        jq: Option<Arc<IsolatedJq>>,
    ) -> Result<RunOutcome, RuntimeError>
    where
        F: FsReadDyn + FsWriteDyn + FsListDyn + FsMetaDyn + Send + Sync + 'static,
    {
        self.rehearse_over_replaying(fs, plan, tally, jq, None)
            .await
    }

    /// [`Self::rehearse_over_with`] as a replay trial when `captures` are lent: a plain
    /// `nika:fetch` GET (no crawl, no jq extraction) reaches a fetch plane that answers exactly a
    /// captured address with its capture and refuses every other request, counted, with no
    /// socket opened and no name resolved. Under [`Self::replay_plan`] a model step fails where
    /// it stands: the registry has no transport and no key, the agent's seat refuses every turn.
    /// The outcome is the trial's view of the run ([`replay::settle_into`]), screened as
    /// [`replay::screen`] names it.
    ///
    /// # Errors
    /// As [`Self::rehearse_over`].
    pub async fn rehearse_over_replaying<F>(
        &self,
        fs: Arc<F>,
        plan: ExecutionAccessPlan,
        tally: Arc<DeniedTally>,
        jq: Option<Arc<IsolatedJq>>,
        captures: Option<Arc<Captures>>,
    ) -> Result<RunOutcome, RuntimeError>
    where
        F: FsReadDyn + FsWriteDyn + FsListDyn + FsMetaDyn + Send + Sync + 'static,
    {
        let seams = RunSeams::of(self.workflow.run.as_ref().map(|run| &run.value));
        let held = Arc::clone(&tally);
        let model = self
            .workflow
            .model
            .as_ref()
            .map_or("", |model| model.value.as_str());
        let permits = self.workflow.permits.as_ref().map(|permits| &permits.value);
        let http = RoomHttp {
            tally: Arc::clone(&tally),
            captures: captures.clone(),
        };
        let plane = BuiltinDispatcher::new(
            fs,
            Arc::new(http),
            Arc::new(seams.clock.clone()),
            Arc::new(NullEmitter::default()),
            Arc::new(NonInteractive::default()),
            Arc::new(NoWorkflow::default()),
        )
        .with_fs_boundary(fs_boundary_of_permits(permits));
        let gate = Arc::new(Gate {
            inner: Arc::new(plane),
            tally: Arc::clone(&tally),
            jq,
            replays: captures.is_some(),
        });
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&gate)));
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::new()));
        let seat = Arc::new(DeniedProvider(Arc::clone(&tally)));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(DeniedShell(Arc::clone(&tally)))),
            Arc::clone(&invoke),
            InferVerb::new(registry, model),
            AgentVerb::new(seat, invoke, gate, model),
            seams.clock.clone(),
            RuntimeConfig::new(None, seams.jitter_seed),
        )
        .with_secret_resolver(Arc::new(DeniedSecrets(Arc::clone(&tally))))
        .with_child_runner(Arc::new(DeniedChild(tally)))
        .with_access_plan(plan);
        let (mut stamper, mut sink) = (seams.stamper(), SilentSink);
        let mut outcome = runtime
            .run(&self.workflow, &self.report, stamper.as_mut(), &mut sink)
            .await?;
        if let Some(captures) = &captures {
            let screen = replay::screen(&self.workflow, captures);
            let order: Vec<String> = (self.workflow.tasks.iter())
                .map(|task| task.value.id.value.clone())
                .collect();
            let failed: Vec<String> = (outcome.records.iter())
                .filter(|(_, record)| record.status == TaskStatus::Failure)
                .map(|(task, _)| task.clone())
                .collect();
            replay::settle_into(&mut outcome, &order, &screen);
            for task in &self.workflow.tasks {
                let id = &task.value.id.value;
                let excused = failed.contains(id)
                    && (outcome.records.get(id)).is_some_and(|r| r.status == TaskStatus::Skipped);
                if excused {
                    held.excuse(&task.value);
                }
            }
        }
        Ok(outcome)
    }
}

/// The tool plane of a rehearsal: the admitted surface reaches the builtin dispatcher, `nika:jq`
/// the isolated evaluator when the host gave one, a plain `nika:fetch` GET the replaying fetch
/// plane when the trial replays, every other tool is refused before it, and one that reaches
/// beyond the room is counted.
struct Gate<D> {
    inner: Arc<D>,
    tally: Arc<DeniedTally>,
    jq: Option<Arc<IsolatedJq>>,
    replays: bool,
}

/// Whether a `nika:fetch` call is a plain GET a replay may answer, as its arguments read at run
/// time: no crawl, no jq extraction (it would run in this process, unbounded), no other method.
fn replayable(input: &serde_json::Value) -> bool {
    let text = |field: &str| input.get(field).and_then(serde_json::Value::as_str);
    let get = text("method").is_none_or(|method| method.eq_ignore_ascii_case("GET"));
    let extraction = input.get("jq").is_some() || text("mode") == Some("jq");
    get && !extraction && input.get("traverse").is_none()
}

impl<D> ToolExecuteDyn for Gate<D>
where
    D: ToolExecuteDyn,
{
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        if let Some(jq) = self.jq.as_ref().filter(|_| call.name == "nika:jq") {
            return jq.evaluate(&call).await;
        }
        if ADMITTED_TOOLS.contains(&call.name.as_str()) {
            return self.inner.execute(call).await;
        }
        if self.replays && call.name == "nika:fetch" && replayable(&call.input) {
            return self.inner.execute(call).await;
        }
        if let Some(counter) = self.tally.reached(&call.name) {
            count(counter);
        }
        Err(ToolExecError::NotAvailable {
            reason: format!("{OUTSIDE} · tool `{}` refused", call.name),
        })
    }
}

impl<D> ToolDefinitionProviderDyn for Gate<D>
where
    D: ToolDefinitionProviderDyn,
{
    async fn tool_defs(&self) -> Result<Vec<ToolDef>, ToolDefsError> {
        self.inner.tool_defs().await
    }
}

/// The shell of a rehearsal: every command is refused before a process could spawn.
struct DeniedShell(Arc<DeniedTally>);

impl ShellRunDyn for DeniedShell {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        count(&self.0.spawn);
        Err(ShellError::Blocked {
            reason: format!("{OUTSIDE} · exec `{}` refused", command.program),
        })
    }
}

/// The fetch plane of a rehearsal: every request is refused before a socket opens, but a GET of
/// exactly an address a replay trial lent a capture of, answered with that capture.
struct RoomHttp {
    tally: Arc<DeniedTally>,
    captures: Option<Arc<Captures>>,
}

impl RoomHttp {
    fn refuse(&self) -> HttpError {
        count(&self.tally.network);
        HttpError::Unsupported {
            reason: format!("{OUTSIDE} · a request was refused"),
        }
    }
}

/// The response a capture replays: its status, its content type and its exact bytes, at the
/// address it was taken at.
fn replayed(capture: &Capture) -> HttpResponse {
    let mut headers = std::collections::BTreeMap::new();
    if let Some(kind) = capture.content_type() {
        headers.insert("content-type".to_owned(), kind.to_owned());
    }
    HttpResponse::new(capture.status(), headers, capture.served(), capture.url())
}

impl HttpGetDyn for RoomHttp {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let held = (self.captures.as_ref())
            .filter(|_| request.method == HttpMethod::Get)
            .and_then(|captures| captures.get(&request.url));
        held.map(replayed).ok_or_else(|| self.refuse())
    }
}

impl HttpPostDyn for RoomHttp {
    async fn post(&self, _request: HttpRequest) -> Result<HttpResponse, HttpError> {
        Err(self.refuse())
    }

    async fn send_streaming(&self, _request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        Err(self.refuse())
    }
}

/// The model seat of a rehearsal: every turn is refused before any provider is reached.
struct DeniedProvider(Arc<DeniedTally>);

impl ProviderInferDyn for DeniedProvider {
    async fn infer(&self, _request: InferRequest) -> Result<InferResponse, ProviderError> {
        count(&self.0.provider);
        Err(ProviderError::AdmissionDenied {
            reason: format!("{OUTSIDE} · a model turn was refused"),
        })
    }
}

impl ProviderMeta for DeniedProvider {
    // The trait ties the return to `&self`; the seat's name is a fixed literal.
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "rehearsal"
    }
}

/// The secret store of a rehearsal: no reference resolves.
struct DeniedSecrets(Arc<DeniedTally>);

impl WorkflowSecretResolver for DeniedSecrets {
    fn resolve(&self, name: &str, _reference: &SecretRef) -> Result<String, SecretResolveError> {
        count(&self.0.secret);
        Err(SecretResolveError {
            name: name.to_owned(),
            reason: format!("{OUTSIDE} · no secret resolves"),
        })
    }
}

/// The nested runs of a rehearsal: none starts.
struct DeniedChild(Arc<DeniedTally>);

impl ChildRunner for DeniedChild {
    fn run_child<'a>(
        &'a self,
        _call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        count(&self.0.child);
        Box::pin(std::future::ready(Err(ChildRunRefusal {
            code: "NIKA-COMP-001".to_owned(),
            message: format!("{OUTSIDE} · a nested run was refused"),
        })))
    }
}
