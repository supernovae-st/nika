// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-runtime` — the L3 orchestrator (the first L3 crate).
//!
//! Executes one **checked** workflow wave-by-wave through the four verb
//! crates, emitting the canonical event stream. v2 implements the v0.1
//! task pipeline of `nika-spec` 03/04/05 — gates · result records ·
//! `with:` · `retry:` · `timeout:` · `on_error:` · `for_each:` ·
//! `on_finally:` — with bounded intra-wave concurrency. Spec:
//! `docs/crate-specs/nika-runtime.md`.
//!
//! ## Invariants
//!
//! - **Audit-before-run** · a dirty [`CheckReport`] is `NIKA-1700` ·
//!   never executes (spec §3). Under a declared `permits:` block the
//!   report must also MATCH the workflow bytes — the run-start boundary
//!   re-derivation (permits-fit · trifecta) refuses a hand-built clean
//!   report with `NIKA-1707` (the fail-closed backstop for library
//!   embedders · the CLI re-checks right before run and never needs it).
//! - **INV-024** · the runtime is the ONE emission site per verb path ·
//!   the verbs stay event-free.
//! - **Schedule is the checker's** · [`CheckReport::waves`] is executed
//!   as given · the runtime never re-sorts (a bad index is `NIKA-1701`).
//! - **Ordered settlement** · tasks within a wave dispatch CONCURRENTLY
//!   (cap = [`RuntimeConfig::wave_parallelism`]) and settle — events ·
//!   records — sequentially in wave order. The event stream is
//!   byte-identical for any cap ≥ 1 under deterministic seams (the
//!   deterministic-reservations pattern · Blelloch et al. `PPoPP` 2012 ·
//!   Calvin SIGMOD 2012 · Kahn 1974).
//! - **Loud expressions** · unresolved `${{ }}` is `NIKA-1702` · a form
//!   outside the v0 subset is `NIKA-1703` (see the private `expr`
//!   module · never a silent literal · never a silently-closed gate).
//!
//! ## Seams (why 6 generics)
//!
//! The agent's tool-definition impl lives in `nika-builtin` and the
//! production clock in `nika-clock` (L1) — the runtime CORE stays
//! seam-generic (zero concrete-effect edge in the DAG executor: the
//! four verbs arrive PRE-CONSTRUCTED, their defaults — model · seams —
//! are envelope concerns the composer resolves before the run). The
//! production composition itself descended here 2026-07-22 (the
//! run-verb 15k wall · compute descends, render stays): [`compose`]
//! wires the real effects into the generic runtime for every embedder,
//! keeping the executor injected (the L4 shim owns it).

#![forbid(unsafe_code)]

mod abort;
mod admit;
#[cfg(test)]
mod admit_resolved_tests;
mod agent_events;
pub mod approval;
pub mod child;
pub mod compose;
pub mod config;
mod dispatch;
mod emit_task;
pub(crate) mod harness_seat;
mod ledger;
mod pause;
/// The proof layer (spec 15) — re-exported whole from the standalone
/// `nika-proof` crate (L0 · the 15k crate-size split), so every
/// `nika_runtime::proof::…` path holds. `ir` stays (the resume projection).
pub mod proof {
    pub use nika_proof::receipt;
    pub use nika_proof::{
        FORMAT_VERSION, HashDomain, SemanticHash, UnknownDomain, canonical, hash_in_domain,
        preimage, semantic_hash,
    };
    pub mod ir;
}
mod recover;
pub mod resume;
mod retry;
#[cfg(test)]
mod retry_safety_tests;
#[cfg(test)]
mod run_clock_tests;
/// The ONE OS-sandbox selection (ADR-095 Layer 6 · #888) — `pub` because
/// `nika-mcp`'s spawn confinement rides the same decision (L4→L3).
mod settle;
/// The effects-simulated seams (the `nika test` plane · P0-16) — `pub`
/// because [`SimRuntime`](compose::SimRuntime) names them in its spelling
/// (the `compose::StderrEmitter` precedent).
pub mod simulated;
mod task;
mod trust;
mod workflow_call;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use futures_util::StreamExt;
use nika_check::CheckReport;
use nika_event::{Event, EventKind};
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::clock::ClockDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::raw::RawWorkflow;
use nika_schema::types::{OutputDecl, VarDecl, type_expr_display};
use nika_types::cancel::CancelCtx;
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

// ADR-127 · the run's laws live in the member crate; every historical
// path holds through these re-exports.
pub(crate) use nika_runtime_laws::{
    compat_record, contract, errors, integrity, origins, secret, stamp, witness,
};
pub use nika_runtime_laws::{identity, sandbox_select};

pub use admit::{
    access_pin_refusal, budget_floor_refusal, floor_refusal, modelless_refusal, plan_refusal,
    required_inputs_refusal, scope_to_task, unbounded_breakdown,
};
pub use compose::{
    ProdRuntime, RunSeams, RuntimeCapabilities, SimRuntime, capabilities_of, production_runtime,
    simulated_runtime,
};
pub use config::{RuntimeConfig, project_root_fingerprint};
pub use errors::RuntimeError;
pub use identity::{
    EngineIdentity, MACHINE_PROTOCOL_VERSION, MACHINE_SNAPSHOT_FORMAT_VERSION, engine_identity,
};
pub use origins::{InputOrigin, input_origins};
pub use pause::WorkflowPause;
// Descended to `nika-dataflow` at the 15k wall — paths preserved. The three
// modules keep their historical `crate::{expr,jq,record}` names so every
// call site inside this crate reads exactly as it did before the move; the
// public types keep their `nika_runtime::…` paths below.
pub use compat_record::{TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause, legal};
pub use nika_dataflow::DataflowError;
use nika_dataflow::TaskRecord as DataflowTaskRecord;
pub(crate) use nika_dataflow::{expr, jq, record};
// The shared execution driver (`ServiceExecutionDriver` + `AuthorizedRuntime`
// + the `ChildTrace*`/`Service*` family) descended to `nika-service-execution`
// at the 15k wall; `compose::service_runtime` is the one seam it reads back.
// Descended to `nika-secret` at the 15k wall — paths preserved.
pub use nika_secret::{
    NoSecrets, SecretResolveError, WorkflowSecretResolver, source_is_runtime_resolvable,
};
pub use stamp::{DeterministicStamper, EventSink, Stamper, SystemStamper, VecSink};

use expr::Scope;

/// Composer-owned execution knobs (spec §2).
/// The run's verdict + the result records (spec §2).
#[derive(Debug)]
#[non_exhaustive]
pub struct RunOutcome {
    /// Terminal `WorkflowCompleted` (true) vs `WorkflowFailed`.
    pub ok: bool,
    /// `tasks.<id>` result records for every settled task (spec 04).
    pub records: BTreeMap<String, TaskRecord>,
    /// Workflow `outputs:` resolved from the final records (an output
    /// whose reference no longer resolves is omitted · the verdict is
    /// unchanged · spec §3).
    pub outputs: BTreeMap<String, Value>,
    /// The task ids that settled as ADR-099 cache hits, in settle order
    /// (empty on a fresh run — feeds the `--resume` summary line).
    pub cache_hits: Vec<String>,
    /// `Some` iff the run PAUSED on a blocking `nika:prompt` (ADR-099
    /// rider · run state `paused` · `ok` stays true — a pause is a
    /// decision point, never a failure). The payload the CLI surfaces.
    pub paused: Option<WorkflowPause>,
    /// Σ of METERED spend across the run (the same fold the terminal
    /// frame carries) · `None` when nothing was priced — absent is
    /// honest, a `0.0` nobody metered is not.
    pub total_cost_usd: Option<f64>,
    /// Leaf executions whose spend is in `total_cost_usd`.
    pub priced_calls: u32,
    /// Leaf executions that carried an unpriced reason (local · mock ·
    /// uncataloged · provider silent) — spend NOT in the total.
    pub unpriced_calls: u32,
    /// Whether the run stopped on `--max-cost-usd` (NIKA-1704).
    pub budget_exceeded: bool,
    /// Whether the operator cancelled the run at a wave boundary (#1353).
    pub cancelled: bool,
}

impl RunOutcome {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(
        ok: bool,
        records: BTreeMap<String, TaskRecord>,
        outputs: BTreeMap<String, Value>,
    ) -> Self {
        Self {
            ok,
            records,
            outputs,
            cache_hits: Vec::new(),
            paused: None,
            total_cost_usd: None,
            priced_calls: 0,
            unpriced_calls: 0,
            budget_exceeded: false,
            cancelled: false,
        }
    }

    fn from_dataflow(
        ok: bool,
        records: BTreeMap<String, DataflowTaskRecord>,
        outputs: BTreeMap<String, Value>,
    ) -> Self {
        Self::new(
            ok,
            records
                .into_iter()
                .map(|(id, record)| (id, record.into()))
                .collect(),
            outputs,
        )
    }

    /// Fold the run ledger's terminal snapshot onto the outcome (one
    /// site — normal · paused · budget-abort all flow through).
    fn with_ledger(mut self, snap: &ledger::LedgerSnapshot) -> Self {
        self.total_cost_usd = snap.any_priced.then_some(snap.spent_usd);
        self.priced_calls = snap.priced_calls;
        self.unpriced_calls = snap.unpriced_calls;
        self.budget_exceeded = snap.tripped;
        self
    }
}

/// The L3 executor over the four pre-constructed verbs + the clock seam.
pub struct Runtime<S, T, H, P, D, C> {
    shell: ExecVerb<S>,
    invoke: Arc<InvokeVerb<T>>,
    infer: InferVerb<H>,
    agent: AgentVerb<P, T, D>,
    clock: C,
    config: RuntimeConfig,
    /// Resolves the `secrets:` namespace at run start (MINOR-B). Defaults to
    /// [`NoSecrets`] (every `secrets.X` unbound → NIKA-1702 · fail-closed ·
    /// the prior behavior); the composer injects an env/file resolver via
    /// [`Self::with_secret_resolver`].
    secrets: Arc<dyn WorkflowSecretResolver>,
    /// Operator-supplied `vars:` values (`nika run --var key=value` · F4)
    /// — merged OVER the envelope defaults at run start, so an override
    /// wins and a `required: true` var without a default becomes
    /// runnable. Empty by default (envelope defaults only).
    var_overrides: BTreeMap<String, Value>,
    /// F-P13 (NEP-0014 law 2) — the ORIGIN of every input the run binds
    /// (cli-operator · ci-context · env · file), computed by the composer
    /// (it owns the caller context: CI detection · the declared `@env:`
    /// reads) and journaled on `workflow_started`. Empty = no claim (a
    /// run without inputs never speaks).
    input_origins: BTreeMap<String, InputOrigin>,
    /// ADR-099 `--resume` skip plan — task id → the journaled success it
    /// may match (BOTH hashes · §1). Empty by default (a fresh run
    /// executes every task — `task.cache_hit` fires only under resume).
    resume_plan: resume::ResumePlan,
    /// ADR-099 rider — pause (instead of failing) on a blocking
    /// `nika:prompt` with no usable `default:`. The composer enables it
    /// for non-interactive machine surfaces (`--json` · `--output json`
    /// · serve); default OFF (the stdlib PROMPT-001 contract unchanged).
    pause_on_prompt: bool,
    /// ADR-099 rider — operator-supplied prompt answers (`--answer
    /// task=value`): bound as the prompt's `default:` at dispatch (the
    /// answered branch), never part of the task's resume identity.
    prompt_answers: BTreeMap<String, Value>,
    /// The composer's `--model` override (#409 · ADR-099): the EFFECTIVE
    /// default a model-less infer/agent task runs on is `override ||
    /// envelope model:` — resume identity must cover it, or a model swap
    /// cache-hits the OLD model's output. `None` = envelope only.
    model_override: Option<String>,
    /// The operator's `--access` pin (D-2026-08-04-N1): unsatisfied
    /// refuses before the prologue, never substitutes (A-4).
    access_pin: Option<String>,
    /// The composer-collected access-probe rows (env presence only —
    /// the runtime stays env-free here). Empty by default.
    access_probes: Vec<nika_providers::probe::ProviderProbe>,
    /// R-2 (P3 B5) · the composer-COMPUTED access stamps (`access_pin` ·
    /// `access_plan`), merged into the boot manifest verbatim (the
    /// F-P13 `input_origins` posture: the composer derives, the runtime
    /// journals — the runtime never re-resolves). Empty = no claim.
    boot_access_fields: Vec<(&'static str, FieldValue)>,
    /// The seated harness adapter's id (B6) — journaled as
    /// `harness_seat` so the trace names the execution override beside
    /// the resolver's plan. `None` without a declaration.
    harness_seat_id: Option<String>,
    cancel: Option<CancelCtx>,
    /// The FROZEN execution-access plan (One Door · wave 1): resolved
    /// once by the composer, consumed here — the admission belt, the
    /// per-lane seat routing, the task terminal's access stamp. `None`
    /// = a bare embedder (the env-declared seat + the pin gate stand).
    access_plan: Option<nika_providers::ExecutionAccessPlan>,
    /// Live `nika:inspect` cell (ADR-088). Shared with the dispatcher.
    inspect: Option<Arc<nika_builtin::LiveInspect>>,
    /// The run's SOURCE identity — sha256 hex over the exact bytes the
    /// operator ran (computed by the composer that read the file; the
    /// runtime never re-reads). Stamped on `workflow_started` so every
    /// journal names the definition it recorded: replay, diff and
    /// fork surfaces can prove « the file changed since this run »
    /// instead of guessing. `None` (embedded/test callers) = no claim.
    source_sha256: Option<String>,
    /// The LF normal form's sha256 — present only when the source bytes
    /// were CRLF/BOM-encoded (the two hashes differ). Lets drift checks
    /// tell a re-encode from an edit.
    source_sha256_lf: Option<String>,
    /// Agent Skills resolved by the COMPOSER (spec 02 §agent skills):
    /// path-as-written in `skills:` → the SKILL.md file's raw text. The
    /// runtime stays fs-free — it composes from THESE texts (dispatch)
    /// and keys resume identity on them (a changed skill re-runs · the
    /// same law as an edited prompt · ADR-099). Empty by default: a
    /// workflow without `skills:` never looks here.
    skills: BTreeMap<String, String>,
    /// Child-workflow closure digests resolved by the COMPOSER (spec 14
    /// law 10 at the `def_hash` tier): `workflow:` target-as-written →
    /// the transitive source-closure digest. The runtime stays fs-free —
    /// it keys the calling task's resume identity on THESE digests (an
    /// edited child or grandchild re-runs the call · ADR-099 trap 6
    /// across the file boundary). Empty by default: a workflow without
    /// `workflow:` calls never looks here, and a bare embedder's calls
    /// are simply not resume-eligible (never a wrong skip).
    child_closures: BTreeMap<String, String>,
    /// The child-workflow execution seam (spec 14 · composition). `None`
    /// (the default) = no nested-run surface: an `invoke: workflow:`
    /// task fails loudly (`NIKA-COMP-001` · the run-side voice) instead
    /// of silently no-oping. The CLI composer injects its recursive
    /// production runner via [`Self::with_child_runner`].
    child_runner: Option<Arc<dyn child::ChildRunner>>,
    /// THIS run's nesting depth (root = 0). The composer sets `parent
    /// depth + 1` on every child runtime; the dispatch gate refuses a
    /// call that would exceed [`child::MAX_RUN_DEPTH`] fail-closed
    /// (`NIKA-SEC-003` · spec 14 §errors — the runtime backstop behind
    /// the static acyclicity proof).
    run_depth: u32,
    /// F-P4 · the approval book (NEP-0013): the per-run ticket state the
    /// `nika:prompt` gate mints/dedups/rate-limits against, plus the
    /// folded resume authority the composer injects. Per-runtime = per
    /// run (a child run keeps its own journal — and its own book).
    approvals: approval::ApprovalBook,
    /// F-P21 (NEP-0014 law 4) — the DECLARED cross-version compat: the
    /// recorded engine version the operator allowed this resume to
    /// cross from (`--resume-compat`), attested on the boot manifest.
    /// `None` = no crossing declared (an exact resume · a fresh run).
    resume_compat: Option<String>,
    /// ADR-099 trust amendment (2026-08-08) — the resume's trust posture
    /// when the chain precondition did NOT hold, attested on the boot
    /// manifest (`resume_unverified: <posture>` + the finding). `None` =
    /// the resume's chain verified (or no resume at all).
    resume_unverified: Option<resume::ResumeUnverified>,
}

/// One wave's read-only value scope — (`inputs` · `const` · `secrets` ·
/// `permits` · `types`), a single loan the pipeline fan-out threads
/// whole (the named-type map is the `returns:` contract environment ·
/// spec 09 · W3).
type WaveScope<'a> = (
    &'a BTreeMap<String, Value>,
    &'a BTreeMap<String, Value>,
    &'a BTreeMap<String, Value>,
    Option<&'a nika_schema::types::Permits>,
    &'a BTreeMap<String, nika_types::types::NikaType>,
);

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C> {
    /// Assemble the runtime from its four verbs + the kernel clock
    /// (the composer wires seams + envelope defaults · spec §2). Secrets are
    /// unresolved by default ([`NoSecrets`]) — inject a resolver with
    /// [`Self::with_secret_resolver`].
    #[must_use]
    pub fn new(
        shell: ExecVerb<S>,
        invoke: Arc<InvokeVerb<T>>,
        infer: InferVerb<H>,
        agent: AgentVerb<P, T, D>,
        clock: C,
        config: RuntimeConfig,
    ) -> Self {
        Self {
            shell,
            invoke,
            infer,
            agent,
            clock,
            config,
            secrets: Arc::new(nika_secret::NoSecrets),
            var_overrides: BTreeMap::new(),
            input_origins: BTreeMap::new(),
            resume_plan: resume::ResumePlan::new(),
            pause_on_prompt: false,
            prompt_answers: BTreeMap::new(),
            model_override: None,
            source_sha256: None,
            source_sha256_lf: None,
            skills: BTreeMap::new(),
            child_closures: BTreeMap::new(),
            child_runner: None,
            run_depth: 0,
            approvals: approval::ApprovalBook::new(),
            resume_compat: None,
            resume_unverified: None,
            access_pin: None,
            access_probes: Vec::new(),
            boot_access_fields: Vec::new(),
            harness_seat_id: None,
            cancel: None,
            access_plan: None,
            inspect: None,
        }
    }

    /// Inject the live inspect cell (same `Arc` the dispatcher holds).
    #[must_use]
    pub fn with_inspect(mut self, cell: Arc<nika_builtin::LiveInspect>) -> Self {
        self.inspect = Some(cell);
        self
    }

    /// Seed the static DAG before any task runs.
    fn seed_inspect(&self, wf: &nika_schema::raw::RawWorkflow, report: &nika_check::CheckReport) {
        let Some(cell) = &self.inspect else {
            return;
        };
        let nodes: Vec<String> = wf.tasks.iter().map(|t| t.value.id.value.clone()).collect();
        let edges: Vec<(String, String)> = wf
            .tasks
            .iter()
            .flat_map(|t| {
                let to = t.value.id.value.clone();
                t.value
                    .after
                    .iter()
                    .map(move |(from, _)| (from.value.clone(), to.clone()))
            })
            .collect();
        let waves: Vec<Vec<String>> = report
            .waves
            .iter()
            .map(|w| {
                w.iter()
                    .filter_map(|&i| wf.tasks.get(i).map(|t| t.value.id.value.clone()))
                    .collect()
            })
            .collect();
        cell.seed_dag(nodes, edges, waves);
    }

    /// Mirror settled records + spend after each wave merge.
    fn publish_inspect(
        &self,
        records: &BTreeMap<String, DataflowTaskRecord>,
        ledger: &ledger::RunLedger,
    ) {
        let Some(cell) = &self.inspect else {
            return;
        };
        cell.replace_records(
            records
                .iter()
                .map(|(id, r)| (id.clone(), r.status.as_str().to_owned(), r.duration_ms)),
        );
        let snap = ledger.snapshot();
        cell.set_spend(snap.spent_usd, snap.any_priced, snap.by_source);
    }

    /// Declare the composer's `--model` override (#409): it joins the
    /// resume identity of every model-less infer/agent task (the model
    /// those tasks actually run on), so `--resume` under a different
    /// override re-runs instead of serving the old model's output.
    /// Builder form — `None` (the default) keys against the envelope
    /// `model:` alone.
    #[must_use]
    pub fn with_model_override(mut self, model: Option<String>) -> Self {
        self.model_override = model;
        self
    }

    /// Declare the operator's `--access` pin (D-2026-08-04-N1) — the
    /// admission gate judges it; unsatisfied refuses, never substitutes.
    #[must_use]
    pub fn with_access_pin(mut self, pin: Option<String>) -> Self {
        self.access_pin = pin;
        self
    }

    /// Inject the composer-collected access probes the `--access`
    /// admission gate judges against (env presence only).
    #[must_use]
    pub fn with_access_probes(mut self, probes: Vec<nika_providers::probe::ProviderProbe>) -> Self {
        self.access_probes = probes;
        self
    }

    /// Access probes already attached by the composition layer.
    #[must_use]
    pub fn access_probes(&self) -> &[nika_providers::probe::ProviderProbe] {
        &self.access_probes
    }

    /// Inject the composer-COMPUTED access stamps (R-2 · B5) — journaled
    /// verbatim (the composer derives, the runtime journals).
    #[must_use]
    pub fn with_boot_access_fields(mut self, fields: Vec<(&'static str, FieldValue)>) -> Self {
        self.boot_access_fields = fields;
        self
    }

    /// Set the seated harness adapter's id (B6) — journaled as
    /// `harness_seat` on the boot manifest (the execution override
    /// named beside the resolver's plan).
    #[must_use]
    pub fn with_harness_seat_id(mut self, id: Option<String>) -> Self {
        self.harness_seat_id = id;
        self
    }

    /// The operator's cancellation (#1353), read at every wave boundary.
    #[must_use]
    pub fn with_cancel(mut self, cancel: CancelCtx) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Attach the FROZEN execution-access plan (One Door · wave 1) —
    /// the runtime executes exactly this plan or refuses at admission;
    /// nothing downstream re-resolves.
    #[must_use]
    pub fn with_access_plan(mut self, plan: nika_providers::ExecutionAccessPlan) -> Self {
        self.access_plan = Some(plan);
        self
    }

    /// The attached plan, when the composer froze one.
    #[must_use]
    pub fn access_plan(&self) -> Option<&nika_providers::ExecutionAccessPlan> {
        self.access_plan.as_ref()
    }

    /// The seat that serves `model` — the plan's lane when a plan is
    /// attached (a pinned seat serves every model · a resolved seat only
    /// its own lanes), else the env-declared seat (the bare embedder).
    pub(crate) fn seat_for(&self, model: &str) -> Option<&str> {
        match &self.access_plan {
            Some(plan) => plan.seat_for(model),
            None => self.harness_seat_id.as_deref(),
        }
    }

    /// The admitted lane's plan for `model` — the access stamp a task
    /// terminal carries (what actually served, never a prefix guess).
    pub(crate) fn lane_plan(&self, model: &str) -> Option<nika_types::access::AccessPlan> {
        self.access_plan
            .as_ref()
            .and_then(|plan| plan.lane(model))
            .map(|lane| lane.plan.clone())
    }

    /// Inject the workflow `secrets:` resolver (MINOR-B · the composer's
    /// env/file boundary). Builder form — the run binds the resolved values
    /// into the `secrets.X` namespace.
    #[must_use]
    pub fn with_secret_resolver(mut self, resolver: Arc<dyn WorkflowSecretResolver>) -> Self {
        self.secrets = resolver;
        self
    }

    /// Inject operator-supplied `vars:` values (`nika run --var key=value`
    /// · F4). Builder form — merged OVER the envelope defaults at run
    /// start: an override wins against a declared `default:`, and a
    /// `required: true` var without one becomes runnable from the CLI.
    /// Keys are the composer's concern (the CLI validates them against
    /// the workflow's declared `vars:` before composing).
    #[must_use]
    pub fn with_var_overrides(mut self, overrides: BTreeMap<String, Value>) -> Self {
        self.var_overrides = overrides;
        self
    }

    /// Stamp the input ORIGINS (F-P13 · NEP-0014 law 2): the composer
    /// computed each bound input's channel (cli-operator · ci-context ·
    /// env · file — [`input_origins`]); the boot manifest journals them
    /// so the run's attested record names where every value came from.
    /// Builder form — empty (the default) journals no claim.
    #[must_use]
    pub fn with_input_origins(mut self, origins: BTreeMap<String, InputOrigin>) -> Self {
        self.input_origins = origins;
        self
    }

    /// Attach the operator run budget (`--max-cost-usd` · builder):
    /// once METERED spend crosses it the run stops admitting new work —
    /// the crossing call completes and counts, unstarted tasks cancel,
    /// the run fails NIKA-1704 with spent-vs-budget.
    #[must_use]
    pub fn with_max_cost_usd(mut self, budget: Option<f64>) -> Self {
        self.config.max_cost_usd = budget;
        self
    }

    /// Inject the ADR-099 `--resume` skip plan (the composer folds a
    /// prior NDJSON trace into task id → [`resume::PriorSuccess`]).
    /// Builder form — a task skips iff BOTH its recomputed hashes match
    /// its journaled success; everything else runs live (§1). An entry
    /// the composer removed (`--from` + transitive downstream) simply
    /// re-runs.
    #[must_use]
    pub fn with_resume_plan(mut self, plan: resume::ResumePlan) -> Self {
        self.resume_plan = plan;
        self
    }

    /// Stamp the DECLARED cross-version compat (F-P21 · NEP-0014 law 4):
    /// the recorded engine version the operator allowed this resume to
    /// cross from (`--resume-compat` — judged by the composer BEFORE the
    /// fold; this stamps the attestation). The boot manifest journals
    /// `resumed_from_engine` + `resume_compat: declared`. Builder form —
    /// `None` (the default) journals no claim.
    #[must_use]
    pub fn with_resume_compat(mut self, compat: Option<String>) -> Self {
        self.resume_compat = compat;
        self
    }

    /// Stamp the resume's unverified-trust attestation (ADR-099 trust
    /// amendment · 2026-08-08): the run proceeded WITHOUT a verified
    /// chain — the declared opt-out past a finding, or the chainless
    /// compat — and the boot manifest journals the posture and the
    /// finding, so no unverified ancestor launders silently. `None` (the
    /// chain verified · no resume) journals no claim.
    #[must_use]
    pub fn with_resume_unverified(mut self, unverified: Option<resume::ResumeUnverified>) -> Self {
        self.resume_unverified = unverified;
        self
    }

    /// Enable the ADR-099 pause rider: a blocking `nika:prompt` with no
    /// usable `default:` PAUSES the run (journals `workflow_paused` ·
    /// exits cleanly with run state `paused`) instead of failing
    /// PROMPT-001. The composer turns this on for non-interactive
    /// machine surfaces only — everything else keeps today's contract.
    #[must_use]
    pub fn with_prompt_pause(mut self, pause: bool) -> Self {
        self.pause_on_prompt = pause;
        self
    }

    /// Stamp the run's source identity (sha256 hex of the exact bytes
    /// the operator ran) on `workflow_started` — the journal then names
    /// the definition it recorded (drift detection for replay/diff/fork
    /// surfaces). Absent by default: no source, no claim.
    #[must_use]
    pub fn with_source_sha256(mut self, hex: String) -> Self {
        self.source_sha256 = Some(hex);
        self
    }

    /// Stamp the LF-normal-form sibling (`workflow_sha256_lf`) — only
    /// meaningful when it differs from the raw sha (CRLF/BOM sources);
    /// the composer owns that comparison, the runtime never re-reads.
    #[must_use]
    pub fn with_source_sha256_lf(mut self, hex: String) -> Self {
        self.source_sha256_lf = Some(hex);
        self
    }

    /// Supply prompt answers (`--answer task=value` · ADR-099 rider):
    /// bound as the named task's prompt `default:` at dispatch — the
    /// answered branch of the stdlib contract, type-validated per mode
    /// by the builtin itself. An answered task never cache-hits (a fresh
    /// answer always re-asks).
    #[must_use]
    pub fn with_prompt_answers(mut self, answers: BTreeMap<String, Value>) -> Self {
        self.prompt_answers = answers;
        self
    }

    /// Inject the F-P4 resume authority (NEP-0013): the approval ticket
    /// folded from the paused trace (its `workflow_paused` frame) plus
    /// the trace's own run identity. At the answered task's dispatch the
    /// ticket validates BEFORE the answer binds — nonce (cross-run) ·
    /// TTL (re-prompt) · shown hash (`approval.content_mismatch`).
    /// `None` on a fresh run: every `--answer` then mints a fresh
    /// ticket and attests against the content computed now.
    #[must_use]
    pub fn with_paused_approval(self, paused: Option<approval::PausedApproval>) -> Self {
        self.approvals.set_paused(paused);
        self
    }

    /// Inject the COMPOSER-resolved Agent Skills (spec 02 §agent skills):
    /// each `skills:` path, exactly as written, mapped to its SKILL.md
    /// raw text. The composer (CLI) owns the file reads — `nika check`
    /// has already refused a missing/malformed skill (NIKA-AGENT-003/
    /// 004), so at dispatch the map is complete; an entry a bare
    /// embedder forgot fails the TASK with the same codes (check≡run).
    /// The texts also join the referencing tasks' resume identity
    /// (ADR-099 · an edited skill re-runs the task).
    #[must_use]
    pub fn with_skills(mut self, skills: BTreeMap<String, String>) -> Self {
        self.skills = skills;
        self
    }

    /// Inject the COMPOSER-resolved child-workflow closure digests
    /// (spec 14 law 10 · the `def_hash` tier): each static `workflow:`
    /// target, exactly as written, mapped to its transitive source-
    /// closure digest. The digests join the calling tasks' resume
    /// identity (ADR-099 · an edited child re-runs the call — the same
    /// trap-6 law as an edited task, carried across the file boundary).
    /// A target with no entry makes its task non-eligible: it re-runs,
    /// never wrong-skips.
    #[must_use]
    pub fn with_child_closures(mut self, closures: BTreeMap<String, String>) -> Self {
        self.child_closures = closures;
        self
    }
}

/// Emit one stamped event with the given fields.
pub(crate) fn emit(
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
    kind: EventKind,
    fields: &[(&str, FieldValue)],
) -> nika_types::timestamp::Timestamp {
    let (id, ts) = stamper.next();
    let mut event = Event::new(id, ts, kind);
    for (key, value) in fields {
        event = event.with_field(KeyValue::new(*key, value.clone()));
    }
    sink.emit(event);
    ts
}

pub(crate) fn s(v: &str) -> FieldValue {
    FieldValue::String(v.to_owned())
}

pub(crate) fn i(v: i64) -> FieldValue {
    FieldValue::Int(v)
}

/// The envelope's value view (the four-authority family · C2): `inputs`
/// defaults (operator `--var` overrides win — F4 · they also give a
/// `required: true` input without a default its value) + `config` declared
/// fallbacks + `const` constants + the workflow name.
struct EnvelopeValues {
    /// `inputs.<name>` — typed input defaults + the operator overrides.
    inputs: BTreeMap<String, Value>,
    /// `const.<name>` — the author-fixed constants.
    consts: BTreeMap<String, Value>,
    /// The workflow id (the run's display + ledger name).
    workflow_name: String,
}

/// The parked-recovery state for a run — the await table plus the
/// read-only scope every park/drain resolves against (spec 05 §recover
/// step 3 · parked on the settle spine, task-id ordered · a pause exit
/// drops them UNEMITTED, they re-run on `--resume`).
fn parked_scope<'a>(
    wf: &'a RawWorkflow,
    inputs: &'a BTreeMap<String, Value>,
    consts: &'a BTreeMap<String, Value>,
    secrets: &'a BTreeMap<String, Value>,
    resume_ctx: &'a resume::ResumeContext,
    jq_clock: nika_cap::JqClock,
    run_start: nika_kernel::tool_executor::ToolRunStart,
) -> (recover::ParkedRecoveries, recover::ResolveScope<'a>) {
    let parked = recover::ParkedRecoveries::new();
    let resolve_scope = recover::ResolveScope {
        wf,
        inputs,
        consts,
        secrets,
        resume_ctx,
        jq_clock,
        run_start,
    };
    (parked, resolve_scope)
}

/// Resolve the envelope's value authorities from the parsed workflow.
fn envelope_values(wf: &RawWorkflow, overrides: &BTreeMap<String, Value>) -> EnvelopeValues {
    // A declaration's static value: a bare literal (const) or a typed
    // `default:` (inputs · config).
    fn declared_value(decl: &VarDecl) -> Option<Value> {
        match decl {
            VarDecl::Untyped(v) => Some(v.clone()),
            VarDecl::Typed { default, .. } => default.clone(),
        }
    }
    let mut inputs: BTreeMap<String, Value> = wf
        .inputs
        .iter()
        .filter_map(|(key, decl)| Some((key.value.clone(), declared_value(decl)?)))
        .collect();
    inputs.extend(overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
    let consts = wf
        .consts
        .iter()
        .filter_map(|(key, decl)| Some((key.value.clone(), declared_value(decl)?)))
        .collect();
    let workflow_name = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    EnvelopeValues {
        inputs,
        consts,
        workflow_name,
    }
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn + ProviderMeta,
    D: ToolDefinitionProviderDyn,
    C: ClockDyn + Sync,
{
    /// The admitted lane per model of the frozen plan (`model → access
    /// id`) — the resume identity's chosen-access half (wave 1b). Empty
    /// for a planless embedder.
    fn lane_access(&self) -> BTreeMap<String, String> {
        self.access_plan
            .as_ref()
            .map(|plan| {
                plan.admitted()
                    .map(|(model, lane)| (model.to_owned(), lane.plan.access.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// This run's resume identity context (ADR-099 · spec 14 law 10 at the
    /// `def_hash` tier) — the secret markers + leak-guard set + the
    /// composer-injected skill texts and child-closure digests, folded once
    /// per run.
    fn resume_context(
        &self,
        wf: &RawWorkflow,
        secrets: &BTreeMap<String, Value>,
    ) -> resume::ResumeContext {
        resume::ResumeContext::of(
            wf,
            secrets,
            self.model_override.as_deref(),
            &self.skills,
            &self.child_closures,
            self.access_pin.as_deref(),
            &self.lane_access(),
        )
    }

    /// The prologue emission (extracted under the fn-length law — the
    /// boot section's arg marshalling, one line at the call site).
    fn emit_run_prologue(
        &self,
        wf: &RawWorkflow,
        workflow_name: &str,
        opening_stamp: (nika_types::id::EventId, nika_types::timestamp::Timestamp),
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) {
        prologue::emit_prologue(
            wf,
            workflow_name,
            self.source_sha256.as_deref(),
            self.source_sha256_lf.as_deref(),
            self.config.sandbox_backend.as_deref(),
            self.config.sandbox_policy.as_deref(),
            self.config.sandbox_waived,
            self.config.project_root_fingerprint.as_deref(),
            &self.input_origins,
            self.resume_compat.as_deref(),
            self.resume_unverified.as_ref(),
            self.config.max_cost_usd,
            self.model_override.as_deref(),
            self.boot_access_fields.clone(),
            self.harness_seat_id.as_deref(),
            &self.approvals,
            opening_stamp,
            stamper,
            sink,
        );
    }

    /// Execute the workflow per the report's wave schedule (spec §3).
    ///
    /// Tasks within a wave dispatch concurrently (capped) and settle in
    /// wave order — the event stream is deterministic for any cap.
    ///
    /// # Errors
    ///
    /// [`RuntimeError::DirtyReport`] (NIKA-1700) · audit-before-run ·
    /// [`RuntimeError::ReportMismatch`] (NIKA-1707) · the report's
    /// boundary lanes do not match the workflow bytes (re-derived at run
    /// start under a declared `permits:` block — a clean report over
    /// different bytes is not clean) ·
    /// [`RuntimeError::WaveOutOfBounds`] (NIKA-1701) · schedule breach ·
    /// [`RuntimeError::MissingRequiredInputs`] (NIKA-1708) · the admission
    /// preflight (issue #603): a `required: true` input with neither a
    /// declared `default:` nor a `--var` override refuses BEFORE the
    /// prologue — zero events, zero spend.
    /// Expression failures (NIKA-1702/1703) fail the TASK (cascade) ·
    /// they never abort the run.
    pub async fn run(
        &self,
        wf: &RawWorkflow,
        report: &CheckReport,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<RunOutcome, RuntimeError> {
        // A launch refusal precedes the prologue: zero events, zero spend.
        admit::gates(
            wf,
            report,
            &self.var_overrides,
            self.config.max_cost_usd,
            self.model_override.as_deref(),
            (
                self.access_pin.as_deref(),
                &self.access_probes,
                self.access_plan.as_ref(),
            ),
        )?;
        let EnvelopeValues {
            inputs,
            consts,
            workflow_name,
        } = envelope_values(wf, &self.var_overrides);
        // Secrets resolve once; misses stay unbound and fail closed.
        let secrets = secret::resolve_secrets(self.secrets.as_ref(), &wf.secrets);
        let mut scrub = secret::RedactingSink::new(sink, &secrets);
        let sink: &mut dyn EventSink = &mut scrub;
        // Resume identities and leak guards are derived once per run.
        let resume_ctx = self.resume_context(wf, &secrets);
        // The declared capability boundary flows to every dispatch scope.
        let permits = wf.permits.as_ref().map(|spanned| &spanned.value);
        // Type expressions are self-contained; the name environment is empty.
        let types = std::collections::BTreeMap::new();
        // Mint one execution-bound stamp for evidence and every jq evaluator.
        let opening_stamp = stamper.next();
        let jq_clock = nika_cap::JqClock::at(opening_stamp.1);
        let run_start = nika_kernel::tool_executor::ToolRunStart::new(opening_stamp.1.unix_ns);
        self.emit_run_prologue(wf, &workflow_name, opening_stamp, stamper, sink);
        self.seed_inspect(wf, report);

        let mut records: BTreeMap<String, DataflowTaskRecord> = BTreeMap::new();
        let mut ok = true;
        let mut cache_hits: Vec<String> = Vec::new();
        // The spend ledger carries leaf debits and the run cost gate.
        let run_ledger =
            ledger::RunLedger::new(self.config.max_cost_usd).started_at(self.clock.now());
        let (mut parked, resolve_scope) = parked_scope(
            wf,
            &inputs,
            &consts,
            &secrets,
            &resume_ctx,
            jq_clock,
            run_start,
        );

        for wave in &report.waves {
            let early = self
                .run_one_wave(
                    wave,
                    (&workflow_name, permits, &types),
                    &resolve_scope,
                    &run_ledger,
                    &mut parked,
                    (&mut records, &mut ok, &mut cache_hits),
                    stamper,
                    sink,
                )
                .await?;
            if let Some(outcome) = early {
                return Ok(outcome);
            }
        }

        // Mutual recovery cycles resolve against the final records.
        recover::resolve_at_end(
            &resolve_scope,
            &mut parked,
            &mut records,
            &mut ok,
            &mut cache_hits,
            stamper,
            sink,
        );

        // Resolve outputs before the terminal frame so type failures fail the run.
        let mut outputs = resolve_outputs(wf, &records, &inputs, &consts, &secrets);
        // The output map bypasses the event sink, so scrub it explicitly.
        crate::secret::scrub_outputs(&mut outputs, &secrets);
        let snapshot = run_ledger.snapshot_at(self.clock.now());
        let ok = finalize_outputs(
            wf,
            &outputs,
            &records,
            &workflow_name,
            ok,
            &snapshot,
            stamper,
            sink,
        );

        let mut outcome = RunOutcome::from_dataflow(ok, records, outputs).with_ledger(&snapshot);
        outcome.cache_hits = cache_hits;
        Ok(outcome)
    }

    /// One wave through the spine: dispatch + streamed settle, then the
    /// two early-exit boundaries — `Some(outcome)` returns the run NOW
    /// (a pause · the budget boundary), `None` continues to the next
    /// wave. The wave-frozen view moves OUT for the wave (same-wave
    /// tasks never reference each other — checker law — so the
    /// pipelines read upstream records only) and back in after: the
    /// settles stream into a side map while the frozen view stays
    /// borrowed.
    // The scope struct + accumulator trio ARE the settle surface —
    // mirrors `dispatch_settle_wave` itself.
    #[allow(clippy::too_many_arguments)]
    async fn run_one_wave(
        &self,
        wave: &[usize],
        (workflow_name, permits, types): (
            &str,
            Option<&nika_schema::types::Permits>,
            &BTreeMap<String, nika_types::types::NikaType>,
        ),
        resolve_scope: &recover::ResolveScope<'_>,
        run_ledger: &ledger::RunLedger,
        parked: &mut recover::ParkedRecoveries,
        (records, ok, cache_hits): (
            &mut BTreeMap<String, DataflowTaskRecord>,
            &mut bool,
            &mut Vec<String>,
        ),
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<Option<RunOutcome>, RuntimeError> {
        let (wf, inputs, consts, secrets) = (
            resolve_scope.wf,
            resolve_scope.inputs,
            resolve_scope.consts,
            resolve_scope.secrets,
        );
        let frozen = std::mem::take(records);
        let streamed = self
            .dispatch_settle_wave(
                wave,
                wf,
                &frozen,
                (inputs, consts, secrets, permits, types),
                resolve_scope.resume_ctx,
                (resolve_scope.jq_clock, resolve_scope.run_start),
                run_ledger,
                (ok, cache_hits),
                parked,
                stamper,
                sink,
            )
            .await;
        *records = frozen;
        let (wave_records, paused) = streamed?;
        records.extend(wave_records);
        self.publish_inspect(records, run_ledger);

        if let Some(p) = paused {
            emit_paused(workflow_name, &p, stamper, sink);
            let mut outcome =
                RunOutcome::from_dataflow(true, std::mem::take(records), BTreeMap::new());
            outcome.cache_hits = std::mem::take(cache_hits);
            outcome.paused = Some(p);
            return Ok(Some(outcome.with_ledger(&run_ledger.snapshot())));
        }

        // The budget's boundary and the operator's (#1353): the run's terminal.
        let cancelled = self.cancel.as_ref().is_some_and(CancelCtx::is_cancelled);
        if run_ledger.tripped() || cancelled {
            // Parked recoveries resolve against what RAN before the
            // abort cancels the rest — no task is left frameless.
            recover::resolve_at_end(
                resolve_scope,
                parked,
                records,
                ok,
                cache_hits,
                stamper,
                sink,
            );
            let outcome = abort::abort_unran(
                wf,
                workflow_name,
                std::mem::take(records),
                std::mem::take(cache_hits),
                &run_ledger.snapshot_at(self.clock.now()),
                cancelled,
                stamper,
                sink,
            );
            return Ok(Some(outcome));
        }
        Ok(None)
    }

    /// Resolve one wave's members, dispatch concurrently over the
    /// wave-frozen records (checker law), and settle each finish AS THE
    /// ORDERED STREAM YIELDS IT (#412): `buffered` yields in submission
    /// order as each front future completes, so a settled task's frames
    /// reach the sink — and the trace file — at ITS settle, not the wave
    /// join. A `kill -9` mid-wave now keeps the resume credit of every
    /// sibling that already settled. The total event order is unchanged
    /// (the same sequential wave-order spine); only the timing moves
    /// earlier — a slow EARLIER member still holds later settles (the
    /// ordered spine's head-of-line, the price of determinism).
    ///
    /// Settles land in a SIDE map (the caller merges after the wave) —
    /// the frozen view stays borrowed by the in-flight pipelines, and
    /// same-wave tasks never reference each other, so neither the
    /// pipelines nor the pause payload can miss a same-wave record.
    ///
    /// Returns the wave's settled records + `Some(pause)` iff the wave
    /// PAUSED on a blocked `nika:prompt` (ADR-099 rider): siblings that
    /// finished still settle (their work is journaled · they cache-hit
    /// on resume); the blocked prompt itself never settles; the caller
    /// emits the pause frame and stops dispatching later waves.
    ///
    /// Budget gate: `take_while` runs when `buffered` PULLS — a tripped
    /// ledger stops NEW tasks; in-flight complete and count. The default
    /// cap (= wave width) pulls the whole wave before any debit lands:
    /// the gate stops later waves + capped fan-outs, never already-
    /// admitted same-wave siblings. Errors: NIKA-1701 schedule breach.
    // The accumulator pair + the two pens ARE the settle surface —
    // mirrors `settle` itself.
    #[allow(clippy::too_many_arguments)]
    async fn dispatch_settle_wave(
        &self,
        wave: &[usize],
        wf: &RawWorkflow,
        frozen: &BTreeMap<String, DataflowTaskRecord>,
        scope: WaveScope<'_>,
        resume_ctx: &resume::ResumeContext,
        execution: (nika_cap::JqClock, nika_kernel::tool_executor::ToolRunStart),
        ledger: &ledger::RunLedger,
        (ok, cache_hits): (&mut bool, &mut Vec<String>),
        parked: &mut recover::ParkedRecoveries,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<(BTreeMap<String, DataflowTaskRecord>, Option<WorkflowPause>), RuntimeError> {
        let (inputs, consts, secrets, permits, types) = scope;
        let (jq_clock, run_start) = execution;
        let resolve_scope = recover::ResolveScope {
            wf,
            inputs,
            consts,
            secrets,
            resume_ctx,
            jq_clock,
            run_start,
        };
        // Resolve indices up front — a bad index is a schedule breach
        // (NIKA-1701 · run abort · the one system error).
        let mut members = Vec::with_capacity(wave.len());
        for &index in wave {
            let task = wf.tasks.get(index).ok_or(RuntimeError::WaveOutOfBounds {
                index,
                task_count: wf.tasks.len(),
            })?;
            members.push(&task.value);
        }
        let cap = self
            .config
            .wave_parallelism
            .map_or_else(|| members.len().max(1), NonZeroUsize::get);
        let mut wave_records: BTreeMap<String, DataflowTaskRecord> = BTreeMap::new();
        let mut paused: Option<WorkflowPause> = None;
        let mut finishes = std::pin::pin!(
            futures_util::stream::iter(members.iter().take_while(|_| !ledger.tripped()).map(
                |&task| {
                    self.run_task_pipeline(
                        task, wf, frozen, inputs, consts, secrets, permits, types, resume_ctx,
                        ledger, jq_clock, run_start,
                    )
                },
            ))
            .buffered(cap)
        );
        while let Some(finish) = finishes.next().await {
            if self.pause_on_prompt
                && paused.is_none()
                && let Some(p) = pause::prompt_block(
                    &finish,
                    wf,
                    frozen,
                    inputs,
                    consts,
                    resume_ctx.markers(),
                    &self.approvals,
                )
                .or_else(|| pause::harness_gate_block(&finish, wf))
            {
                paused = Some(p);
                continue;
            }
            // A finish whose recovery AWAITS a not-yet-terminal referent
            // parks instead of settling (spec 05 §recover); everything
            // else settles into the side map, then covered parks drain —
            // the wave's terminal truth is `frozen ∪ wave_records`.
            recover::settle_or_park(
                finish,
                &resolve_scope,
                parked,
                frozen,
                &mut wave_records,
                ok,
                cache_hits,
                stamper,
                sink,
            );
        }
        Ok((wave_records, paused))
    }
}

/// Emit the `workflow_paused` terminal frame (ADR-099 rider) — the
/// prompt payload rides as fields (`task` · `mode` · `message` ·
/// `choices` as compact JSON text), secret-masked by construction (the
/// payload renders over the marker scope, never resolved values). The
/// F-P4 approval ticket rides additively (`approval_*` · NEP-0013): the
/// pause serializes the capability the resumed run validates the
/// `--answer` against — shown-hash · digest · nonce · mint · TTL.
fn emit_paused(
    workflow_name: &str,
    pause: &WorkflowPause,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let mut fields = vec![
        ("workflow", s(workflow_name)),
        ("task", s(&pause.task)),
        ("mode", s(&pause.mode)),
        (
            "note",
            s(
                "awaiting a `nika:prompt` answer — resume with `--resume <trace> --answer <task>=<value>`",
            ),
        ),
    ];
    if let Some(message) = pause.message.as_deref() {
        fields.push(("message", s(message)));
    }
    let choices_text = (!pause.choices.is_empty())
        .then(|| serde_json::to_string(&pause.choices).unwrap_or_else(|_| "[]".to_owned()));
    if let Some(text) = choices_text.as_deref() {
        fields.push(("choices", s(text)));
    }
    if let Some(ticket) = pause.approval.as_ref() {
        fields.push(("approval_shown_hash", s(&ticket.content_hash)));
        if let Some(digest) = ticket.digest() {
            fields.push(("approval_digest", s(&digest)));
        }
        fields.push(("approval_nonce", s(&ticket.run_nonce)));
        fields.push(("approval_minted_at_ms", i(ticket.minted_at_ms)));
        fields.push(("approval_ttl_seconds", i(i64::from(ticket.ttl_seconds))));
    }
    emit(stamper, sink, EventKind::WorkflowPaused, &fields);
}

/// Resolve workflow `outputs:` from the final records · an output whose
/// reference no longer resolves is omitted (spec §3) · single-island
/// templates preserve the referenced value's type.
fn resolve_outputs(
    wf: &RawWorkflow,
    records: &BTreeMap<String, DataflowTaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let scope = Scope::workflow_with_value_authorities(records, inputs, consts, secrets);
    wf.outputs
        .iter()
        .filter_map(|(key, decl)| {
            // CLOSED vocabulary (nika-vocab) — both forms named.
            let template = match decl {
                OutputDecl::Untyped(v) => &v.value,
                OutputDecl::Typed { value, .. } => &value.value,
            };
            let rendered = expr::render_json(&Value::String(template.clone()), &scope).ok()?;
            Some((key.value.clone(), rendered))
        })
        .collect()
}

/// Resolve the run's verdict over the typed `outputs:` and emit the terminal
/// frame. Returns the final `ok` (false if a typed output broke its declared
/// `type:` · spec 01 rule 6 · NIKA-VAR-009). The reason rides the
/// `WorkflowFailed` frame as a WORKFLOW-level `detail` (not a phantom
/// `task_failed`) — the event model stays consistent (no orphan task event, no
/// spurious row) and `--json`/journal consumers see a valid terminal with the
/// code. Split out of `run()` to keep that function within its line budget.
#[allow(clippy::too_many_arguments)] // the terminal frame's parts
fn finalize_outputs(
    wf: &RawWorkflow,
    outputs: &BTreeMap<String, Value>,
    records: &BTreeMap<String, DataflowTaskRecord>,
    workflow_name: &str,
    mut ok: bool,
    snapshot: &ledger::LedgerSnapshot,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> bool {
    // A typed output is checked only when every task otherwise settled (a task
    // failure is already the verdict · the output reference is then omitted).
    let violation = if ok {
        first_output_type_violation(wf, outputs)
    } else {
        None
    };
    if violation.is_some() {
        ok = false;
    }
    let kind = if ok {
        EventKind::WorkflowCompleted
    } else {
        EventKind::WorkflowFailed
    };
    let mut fields = vec![("workflow", s(workflow_name))];
    if let Some(v) = &violation {
        fields.push((
            "detail",
            s(&format!(
                "NIKA-VAR-009 · output `{}` is {}, declared type: {}",
                v.name, v.actual, v.expected
            )),
        ));
    }
    fields.extend(terminal_cost_fields(snapshot));
    // What the human card computes rides the frame (#1247): the status
    // and the task tally; `elapsed_ms` came with the cost fields.
    fields.extend(abort::terminal_summary_fields(
        records,
        if ok { "completed" } else { "failed" },
    ));
    emit(stamper, sink, kind, &fields);
    ok
}

/// The run-level cost summary the terminal frame carries — ONLY when
/// there is something honest to say: totals ride iff at least one leaf
/// METERED real spend (a mock/local run stays field-free — a
/// `total_cost_usd: 0.0` nobody metered would be the fake zero at the
/// run level); the unpriced count rides independently so `≥ $X` renders
/// can say what the total does NOT cover.
fn terminal_cost_fields(snap: &ledger::LedgerSnapshot) -> Vec<(&'static str, FieldValue)> {
    let mut fields = Vec::new();
    if snap.any_priced {
        fields.push(("total_cost_usd", FieldValue::Float(snap.spent_usd)));
        fields.push(("priced_calls", i(i64::from(snap.priced_calls))));
        // The snapshot identity the total was priced against — the
        // point-in-time honesty stamp (prices move; the trace says
        // WHICH prices billed this run).
        fields.push(("pricing_as_of", s(nika_catalog::pricing_snapshot().as_of)));
        // Micro-USD rounding at the serialization edge: consumers must
        // never see f64 accumulation dust (`0.030000000000000002`).
        let by_source: std::collections::BTreeMap<&String, f64> = snap
            .by_source
            .iter()
            .map(|(k, v)| (k, (v * 1e6).round() / 1e6))
            .collect();
        if let Ok(json) = serde_json::to_string(&by_source) {
            fields.push(("cost_by_source", s(&json)));
        }
    }
    if snap.unpriced_calls > 0 {
        fields.push(("unpriced_calls", i(i64::from(snap.unpriced_calls))));
    }
    if let Some(elapsed) = snap.elapsed {
        let ms = i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX);
        fields.push(("elapsed_ms", i(ms)));
    }
    fields
}

/// One typed-`outputs:` contract violation (spec 01 rule 6 · NIKA-VAR-009).
struct OutputTypeViolation {
    name: String,
    expected: String,
    actual: &'static str,
}

/// The FIRST typed `outputs:` value whose resolved JSON value does not
/// inhabit its declared `type:` (spec 01 §engine-MUST rule 6 · NIKA-VAR-009
/// · the output half of the callable contract). Post-R3b the declared
/// type speaks the full `TypeExpr` and the judgment is the ONE type core's
/// `fits` (lenient exactly where the core is: a whole float like `42.0`
/// inhabits `integer` · a genuine cross-type mismatch refuses). An output
/// whose `${{ }}` reference no longer resolves is OMITTED by
/// [`resolve_outputs`] (spec §3 · not a type error); a `type:` that does
/// not parse is the check's refusal (`NIKA-TYPE-001` · audit-before-run
/// keeps it out of the run), never the run's. `None` ⇒ every typed output
/// honours its declared type.
fn first_output_type_violation(
    wf: &RawWorkflow,
    resolved: &BTreeMap<String, Value>,
) -> Option<OutputTypeViolation> {
    let named = std::collections::BTreeMap::new();
    let type_names = std::collections::BTreeSet::new();
    for (key, decl) in &wf.outputs {
        let OutputDecl::Typed {
            r#type: Some(ty), ..
        } = decl
        else {
            continue; // untyped output OR no declared type → nothing to check
        };
        let Some(value) = resolved.get(key.value.as_str()) else {
            continue; // unresolved → omitted upstream, not a type error
        };
        let Ok(declared) = nika_types::types::parse_type(&ty.value, &type_names, "outputs") else {
            continue; // the check owns this refusal (NIKA-TYPE-001)
        };
        if !nika_types::types::fits(value, &declared, &named) {
            return Some(OutputTypeViolation {
                name: key.value.clone(),
                expected: type_expr_display(&ty.value),
                actual: json_type_name(value),
            });
        }
    }
    None
}

/// The JSON type name for a NIKA-VAR-009 diagnostic.
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

mod prologue;
#[cfg(test)]
mod tests;

/// The deterministic adversarial suite (B8) — attack fixtures F1..F5 with a
/// scripted mock-model hijack, asserted against the real static + runtime
/// boundaries. Test-only: no public surface, no production code.
#[cfg(test)]
mod adversarial;
