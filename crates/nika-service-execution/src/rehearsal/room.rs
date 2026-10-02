// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One rehearsal run of the admitted pair through the existing runtime. The tool plane is the
//! production builtin dispatcher over the caller's filesystem (a rehearsal room), judged by the
//! workflow's own `permits.fs`, behind a gate that lets only [`ADMITTED_TOOLS`] through. Every
//! other capability is denied, and counted when the runtime reaches for it: the shell spawns
//! nothing, the fetch plane opens no socket, the provider registry has no transport, a secret
//! never resolves and a nested run never starts. The runtime, the workflow and its report are
//! the admitted ones: nothing here interprets the candidate.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta, ToolDef};
use nika_kernel::ai::tool_defs::{ToolDefinitionProviderDyn, ToolDefsError};
use nika_kernel::fs::{FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};
use nika_kernel::http::{
    HttpError, HttpGetDyn, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse,
};
use nika_kernel::process::{ShellCommand, ShellError, ShellResult, ShellRunDyn};
use nika_kernel::provider::{InferRequest, InferResponse, ProviderError};
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_providers::{ExecutionAccessPlan, ProviderRegistry, ProvidersConfig};
use nika_runtime::child::{ChildCall, ChildOutcome, ChildRunRefusal, ChildRunner};
use nika_runtime::compose::fs_boundary_of_permits;
use nika_runtime::{
    RunOutcome, RunSeams, Runtime, RuntimeConfig, RuntimeError, SecretResolveError,
    WorkflowSecretResolver,
};
use nika_schema::types::SecretRef;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

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
const OUTSIDE: &str = "a rehearsal runs its room's files and pure tools only";

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
        let seams = RunSeams::of(self.workflow.run.as_ref().map(|run| &run.value));
        let model = self
            .workflow
            .model
            .as_ref()
            .map_or("", |model| model.value.as_str());
        let permits = self.workflow.permits.as_ref().map(|permits| &permits.value);
        let plane = BuiltinDispatcher::new(
            fs,
            Arc::new(DeniedHttp(Arc::clone(&tally))),
            Arc::new(seams.clock.clone()),
            Arc::new(NullEmitter::default()),
            Arc::new(NonInteractive::default()),
            Arc::new(NoWorkflow::default()),
        )
        .with_fs_boundary(fs_boundary_of_permits(permits));
        let gate = Arc::new(Gate {
            inner: Arc::new(plane),
            tally: Arc::clone(&tally),
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
        runtime
            .run(&self.workflow, &self.report, stamper.as_mut(), &mut sink)
            .await
    }
}

/// The tool plane of a rehearsal: the admitted surface reaches the builtin dispatcher, every
/// other tool is refused before it, and one that reaches beyond the room is counted.
struct Gate<D> {
    inner: Arc<D>,
    tally: Arc<DeniedTally>,
}

impl<D> ToolExecuteDyn for Gate<D>
where
    D: ToolExecuteDyn,
{
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        if ADMITTED_TOOLS.contains(&call.name.as_str()) {
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

/// The fetch plane of a rehearsal: every request is refused before a socket opens.
struct DeniedHttp(Arc<DeniedTally>);

impl DeniedHttp {
    fn refuse(&self) -> HttpError {
        count(&self.0.network);
        HttpError::Unsupported {
            reason: format!("{OUTSIDE} · a request was refused"),
        }
    }
}

impl HttpGetDyn for DeniedHttp {
    async fn get(&self, _request: HttpRequest) -> Result<HttpResponse, HttpError> {
        Err(self.refuse())
    }
}

impl HttpPostDyn for DeniedHttp {
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
