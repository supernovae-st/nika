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
//!   never executes (spec §3).
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
//! The agent's tool-definition impl lives in `nika-builtin` (WIP · not
//! admitted) and the production clock in `nika-clock` (L1) — an
//! admitted crate never depends on a sideways impl, so the runtime
//! stays seam-generic and the composer (nika-cli · L4) injects. The
//! four verbs arrive PRE-CONSTRUCTED: their defaults (model · seams)
//! are envelope concerns the composer resolves before the run.

#![forbid(unsafe_code)]

mod agent_events;
pub mod child;
mod contract;
mod dispatch;
mod emit_task;
mod errors;
mod expr;
mod jq;
mod ledger;
mod pause;
pub mod proof;
mod record;
mod recover;
pub mod resume;
mod retry;
mod secret;
mod stamp;
mod task;
mod workflow_call;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use futures_util::StreamExt;
use nika_event::{Event, EventKind};
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::clock::ClockDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::types::{OutputDecl, VarDecl, VarType};
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

pub use errors::RuntimeError;
pub use pause::WorkflowPause;
pub use record::{TaskErrorRecord, TaskRecord, TaskStatus, TerminalCause, legal};
pub use secret::{
    NoSecrets, SecretResolveError, WorkflowSecretResolver, source_is_runtime_resolvable,
};
pub use stamp::{DeterministicStamper, EventSink, Stamper, VecSink};

use expr::Scope;
use task::{Finish, SettleAs};

/// Composer-owned execution knobs (spec §2).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RuntimeConfig {
    /// Per-wave in-flight cap (`for_each` has its own `max_parallel`).
    /// `None` = wave-width (every wave member in flight at once).
    pub wave_parallelism: Option<NonZeroUsize>,
    /// Seed for the retry full-jitter PRNG — pure splitmix64 over
    /// `(seed, task, attempt)` · replay-stable by construction.
    pub jitter_seed: u64,
    /// Operator run budget (`--max-cost-usd`) over METERED spend. Once
    /// crossed, the run stops admitting new work: in-flight tasks
    /// complete and count, unstarted ones cancel, the run fails with
    /// NIKA-1704. `None` = no budget (the default). Unmetered work
    /// (local · mock · unpriced) can never trip it — the budget bounds
    /// what the ledger can SEE, said loudly at the preflight.
    pub max_cost_usd: Option<f64>,
}

impl RuntimeConfig {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(wave_parallelism: Option<NonZeroUsize>, jitter_seed: u64) -> Self {
        Self {
            wave_parallelism,
            jitter_seed,
            max_cost_usd: None,
        }
    }

    /// Attach an operator run budget (builder — `new()` stays stable).
    #[must_use]
    pub fn with_max_cost_usd(mut self, budget: Option<f64>) -> Self {
        self.max_cost_usd = budget;
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(None, 0)
    }
}

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
        }
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
}

/// One wave's read-only value scope — (`vars` · `env` · `secrets` ·
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
            secrets: Arc::new(secret::NoSecrets),
            var_overrides: BTreeMap::new(),
            resume_plan: resume::ResumePlan::new(),
            pause_on_prompt: false,
            prompt_answers: BTreeMap::new(),
            model_override: None,
            source_sha256: None,
            source_sha256_lf: None,
            skills: BTreeMap::new(),
            child_runner: None,
            run_depth: 0,
        }
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

/// Emit the run's opening frames · `WorkflowStarted` + one
/// `TaskScheduled` per task (the storyboard's fixed prologue).
fn emit_prologue(
    wf: &RawWorkflow,
    workflow_name: &str,
    source_sha256: Option<&str>,
    source_sha256_lf: Option<&str>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    // The run banner reflects the ACTUAL boundary: a declared `permits:` block
    // is a default-deny boundary, so the banner must not keep saying "no
    // boundary declared" once one is present (it misled operators into thinking
    // permits were inert). We state only what is unconditionally true — the
    // boundary is declared and default-deny — and DO NOT claim "(enforced)":
    // runtime enforcement is axis-dependent (fs+exec gate at dispatch; tools+net
    // are validated by `nika check`), so a blanket enforcement claim would
    // over-state for a tools/net-only block (NIKA-SEC-004 · spn-nika review).
    let permits_desc = if wf.permits.is_some() {
        "declared boundary · default-deny"
    } else {
        "engine floor (no boundary declared)"
    };
    let mut opening = vec![("workflow", s(workflow_name)), ("permits", s(permits_desc))];
    if let Some(hex) = source_sha256 {
        opening.push(("workflow_sha256", s(hex)));
    }
    if let Some(hex) = source_sha256_lf {
        opening.push(("workflow_sha256_lf", s(hex)));
    }
    // The trace-format marker (spec 13 §trace · the graph_format: 2
    // precedent): format-2 lines carry `outcome: {class, cause}` on
    // every terminal task event, so the run's opening frame — the
    // trace's header — names the format it speaks. ONE source:
    // `TraceFormatVersion::CURRENT` (pack-parity-pinned).
    opening.push((
        "trace_format",
        i(i64::from(nika_types::TraceFormatVersion::CURRENT.version)),
    ));
    // Environment attestation (Q11): reproducing a failure needs to know
    // WHICH engine on WHICH platform wrote the journal. Compile-time
    // constants only — the workspace releases in lockstep, so the
    // runtime crate's version IS the engine version; no clock, no I/O,
    // determinism intact.
    opening.push(("engine_version", s(env!("CARGO_PKG_VERSION"))));
    let platform = format!("{}/{}", std::env::consts::OS, std::env::consts::ARCH);
    opening.push(("platform", s(&platform)));
    emit(stamper, sink, EventKind::WorkflowStarted, &opening);
    for task in &wf.tasks {
        emit(
            stamper,
            sink,
            EventKind::TaskScheduled,
            &[("task", s(&task.value.id.value))],
        );
    }
}

/// The envelope's value view · `vars` defaults (operator `--var`
/// overrides win — F4 · they also give a `required: true` var without a
/// default its value) + `env` config + the workflow name.
fn envelope_values(
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
) -> (BTreeMap<String, Value>, BTreeMap<String, Value>, String) {
    let mut vars: BTreeMap<String, Value> = wf
        .vars
        .iter()
        .filter_map(|(key, decl)| {
            // CLOSED vocabulary (nika-vocab) — both forms named.
            let value = match decl {
                VarDecl::Untyped(v) => v.clone(),
                VarDecl::Typed { default, .. } => default.clone()?,
            };
            Some((key.value.clone(), value))
        })
        .collect();
    vars.extend(overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
    let env = wf
        .env
        .iter()
        .map(|(key, value)| (key.value.clone(), Value::String(value.value.clone())))
        .collect();
    let name = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    (vars, env, name)
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
    /// Execute the workflow per the report's wave schedule (spec §3).
    ///
    /// Tasks within a wave dispatch concurrently (capped) and settle in
    /// wave order — the event stream is deterministic for any cap.
    ///
    /// # Errors
    ///
    /// [`RuntimeError::DirtyReport`] (NIKA-1700) · audit-before-run ·
    /// [`RuntimeError::WaveOutOfBounds`] (NIKA-1701) · schedule breach.
    /// Expression failures (NIKA-1702/1703) fail the TASK (cascade) ·
    /// they never abort the run.
    pub async fn run(
        &self,
        wf: &RawWorkflow,
        report: &CheckReport,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<RunOutcome, RuntimeError> {
        if !report.is_clean() {
            return Err(RuntimeError::DirtyReport);
        }
        let (vars, env, workflow_name) = envelope_values(wf, &self.var_overrides);
        // Secrets resolve ONCE at run start (MINOR-B · a miss stays
        // unbound → NIKA-1702, fail-closed); the sink gets the redaction
        // scrub (secret.rs · S1) for every emitted event.
        let secrets = secret::resolve_secrets(self.secrets.as_ref(), &wf.secrets);
        let mut scrub = secret::RedactingSink::new(sink, &secrets);
        let sink: &mut dyn EventSink = &mut scrub;
        // ADR-099 resume identities — secret markers + the leak-guard set,
        // derived once per run (keys are stamped on every success so any
        // `--json` trace is later resumable).
        let resume_ctx =
            resume::ResumeContext::of(wf, &secrets, self.model_override.as_deref(), &self.skills);
        // The declared capability boundary (spec 01 §permits) flows to every
        // task's dispatch scope so the exec sink can enforce it (NIKA-SEC-004).
        let permits = wf.permits.as_ref().map(|spanned| &spanned.value);
        // The acyclic named types (spec 09 · `types:`) — resolved ONCE
        // per run through the schema's one projection; every task's
        // `returns:` contract parses against THIS environment (W3).
        let types = nika_schema::named_types(wf);
        emit_prologue(
            wf,
            &workflow_name,
            self.source_sha256.as_deref(),
            self.source_sha256_lf.as_deref(),
            stamper,
            sink,
        );

        let mut records: BTreeMap<String, TaskRecord> = BTreeMap::new();
        let mut ok = true;
        let mut cache_hits: Vec<String> = Vec::new();
        // The spend ledger (leaf debits) + the `--max-cost-usd` gate.
        let run_ledger = ledger::RunLedger::new(self.config.max_cost_usd);
        // Recoveries awaiting a not-yet-terminal referent (spec 05
        // §recover step 3) — parked on the settle spine, task-id ordered.
        // A pause exit drops them UNEMITTED (like the blocked prompt,
        // they simply have not happened yet · they re-run on `--resume`).
        let mut parked = recover::ParkedRecoveries::new();
        let resolve_scope = recover::ResolveScope {
            wf,
            vars: &vars,
            env: &env,
            secrets: &secrets,
            resume_ctx: &resume_ctx,
        };

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

        // Recoveries whose referents never settled on the spine (mutual
        // recovery cycles) resolve against the FINAL records — each
        // still-parked task reads as its PRE-recovery failed record
        // (spec 05 · recovery never rewrites the referent's history).
        recover::resolve_at_end(
            &resolve_scope,
            &mut parked,
            &mut records,
            &mut ok,
            &mut cache_hits,
            stamper,
            sink,
        );

        // Resolve the `outputs:` BEFORE the terminal frame so a typed output
        // that breaks its declared `type:` can fail the run — the output half
        // of the callable contract (spec 01 §engine-MUST rule 6 · NIKA-VAR-009 ·
        // symmetric with the typed-`vars:` input validation).
        let outputs = resolve_outputs(wf, &records, &vars, &env, &secrets);
        let snapshot = run_ledger.snapshot();
        let ok = finalize_outputs(wf, &outputs, &workflow_name, ok, &snapshot, stamper, sink);

        let mut outcome = RunOutcome::new(ok, records, outputs).with_ledger(&snapshot);
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
            &mut BTreeMap<String, TaskRecord>,
            &mut bool,
            &mut Vec<String>,
        ),
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<Option<RunOutcome>, RuntimeError> {
        let (wf, vars, env, secrets) = (
            resolve_scope.wf,
            resolve_scope.vars,
            resolve_scope.env,
            resolve_scope.secrets,
        );
        let frozen = std::mem::take(records);
        let streamed = self
            .dispatch_settle_wave(
                wave,
                wf,
                &frozen,
                (vars, env, secrets, permits, types),
                resolve_scope.resume_ctx,
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

        if let Some(p) = paused {
            emit_paused(workflow_name, &p, stamper, sink);
            let mut outcome = RunOutcome::new(true, std::mem::take(records), BTreeMap::new());
            outcome.cache_hits = std::mem::take(cache_hits);
            outcome.paused = Some(p);
            return Ok(Some(outcome.with_ledger(&run_ledger.snapshot())));
        }

        // The budget boundary: settle what ran, cancel what never
        // started, fail the run with spent-vs-budget (NIKA-1704).
        if run_ledger.tripped() {
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
            let outcome = abort_on_budget(
                wf,
                workflow_name,
                std::mem::take(records),
                std::mem::take(cache_hits),
                &run_ledger.snapshot(),
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
        frozen: &BTreeMap<String, TaskRecord>,
        scope: WaveScope<'_>,
        resume_ctx: &resume::ResumeContext,
        ledger: &ledger::RunLedger,
        (ok, cache_hits): (&mut bool, &mut Vec<String>),
        parked: &mut recover::ParkedRecoveries,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Result<(BTreeMap<String, TaskRecord>, Option<WorkflowPause>), RuntimeError> {
        let (vars, env, secrets, permits, types) = scope;
        let resolve_scope = recover::ResolveScope {
            wf,
            vars,
            env,
            secrets,
            resume_ctx,
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
        let mut wave_records: BTreeMap<String, TaskRecord> = BTreeMap::new();
        let mut paused: Option<WorkflowPause> = None;
        let mut finishes = std::pin::pin!(
            futures_util::stream::iter(members.iter().take_while(|_| !ledger.tripped()).map(
                |&task| {
                    self.run_task_pipeline(
                        task, frozen, vars, env, secrets, permits, types, resume_ctx, ledger,
                    )
                },
            ))
            .buffered(cap)
        );
        while let Some(finish) = finishes.next().await {
            if self.pause_on_prompt
                && paused.is_none()
                && let Some(p) =
                    pause::prompt_block(&finish, wf, frozen, vars, env, resume_ctx.markers())
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
/// payload renders over the marker scope, never resolved values).
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
    emit(stamper, sink, EventKind::WorkflowPaused, &fields);
}

/// Settle one task in wave order · owns the pens (stamper + sink) ·
/// inserts the result record. `pub(crate)`: the recover-await spine
/// (`recover::settle_or_park` + the drain passes) settles through THIS
/// one site — parked stories keep the same frames as live ones.
pub(crate) fn settle(
    finish: Finish,
    records: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    cache_hits: &mut Vec<String>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let id = finish.id;
    // `output:` named bindings (spec 04) — the same map rides every
    // outcome: the evaluated values on success · all-`Null` on a
    // non-success (defined-null reads · empty when no `output:`).
    let named = finish.named;
    let resume = finish.resume;
    match finish.settle {
        SettleAs::Cancelled { note, blocked_by } => {
            // The WHY rides along: which upstream kept the gate closed —
            // the outcome names the cause (spec 13 · cancelled/upstream).
            let record = with_named(
                TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Upstream),
                named,
            );
            let mut fields = vec![("task", s(&id)), ("note", s(note))];
            if let Some(culprit) = &blocked_by {
                fields.push(("blocked_by", s(culprit)));
            }
            fields.push(("outcome", s(&record::outcome_json(&record))));
            emit(stamper, sink, EventKind::TaskCancelled, &fields);
            records.insert(id, record);
        }
        SettleAs::SkippedGate { note, expr } => {
            // The gate's own CEL text — « why did this not run » verbatim.
            // Outcome: skipped/gate — a decision, `.error` defined-null.
            let record = with_named(
                TaskRecord::unran(TaskStatus::Skipped, TerminalCause::Gate),
                named,
            );
            let mut fields = vec![("task", s(&id)), ("note", s(note))];
            if let Some(cel) = &expr {
                fields.push(("when", s(cel)));
            }
            fields.push(("outcome", s(&record::outcome_json(&record))));
            emit(stamper, sink, EventKind::TaskSkipped, &fields);
            records.insert(id, record);
        }
        SettleAs::FailedBeforeStart { stage, error } => {
            // A pre-dispatch failure (gate eval · with · for_each
            // collection) — the task never started: no TaskStarted ·
            // no on_finally (spec 03) · the failure cascades. The one
            // boundary evaluation IS the settling attempt (spec 13 ·
            // failure/verb_error · attempts = 1).
            let mut record = TaskRecord::unran(TaskStatus::Failure, TerminalCause::VerbError);
            record.attempts = Some(1);
            let detail = format!("{} · {}", error.code, error.message);
            record.error = Some(error);
            let record = with_named(record, named);
            emit(
                stamper,
                sink,
                EventKind::TaskFailed,
                &[
                    ("task", s(&id)),
                    ("note", s(stage)),
                    ("detail", s(&detail)),
                    ("outcome", s(&record::outcome_json(&record))),
                ],
            );
            records.insert(id, record);
            *ok = false;
        }
        SettleAs::CacheHit { output } => {
            let record = settle_cache_hit(&id, output, named, resume.as_ref(), stamper, sink);
            records.insert(id.clone(), record);
            cache_hits.push(id);
        }
        SettleAs::Ran(run) => {
            let mut record = settle_ran(&id, run, resume.as_ref(), ok, stamper, sink);
            record.named = named;
            records.insert(id, record);
        }
    }
}

/// ADR-099 §2 — the skip is VISIBLE: one `task_cache_hit` frame carrying
/// the matched identity + the rehydrated output (so a resumed run's own
/// trace stays resumable). No `TaskStarted` · no duration — the task
/// never ran here. Downstream observes a plain success (spec
/// vocabulary), so the outcome reads `success/normal`; the rehydration
/// is the settling attempt (attempts = 1).
fn settle_cache_hit(
    id: &str,
    output: Value,
    named: BTreeMap<String, Value>,
    resume: Option<&resume::ResumeStamp>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> TaskRecord {
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
    record.attempts = Some(1);
    record.output = output;
    record.named = named;
    let mut fields = vec![("task", s(id)), ("note", s("cache hit"))];
    let output_text = serde_json::to_string(&record.output).unwrap_or_else(|_| "null".to_owned());
    if let Some(stamp) = resume {
        fields.push((resume::fields::DEF_HASH, s(&stamp.def_hash)));
        fields.push((resume::fields::INPUT_HASH, s(&stamp.input_hash)));
        fields.push((resume::fields::OUTPUT, s(&output_text)));
    }
    fields.push(("outcome", s(&record::outcome_json(&record))));
    let ended = emit(stamper, sink, EventKind::TaskCacheHit, &fields);
    record.ended_at = Some(ended);
    record
}

/// Attach the `output:` named bindings to a record (spec 04 · the bindings
/// ride every outcome · null on a non-success).
fn with_named(mut record: TaskRecord, named: BTreeMap<String, Value>) -> TaskRecord {
    record.named = named;
    record
}

/// Settle a task that RAN — the started frame · the retry history ·
/// the terminal frame · the result record (spec §3.9). A SUCCESS with a
/// resume stamp carries the ADR-099 identity + output on its
/// `task_completed` frame (additive trace fields · the checkpoint).
/// The attempt history, one `TaskRetrying` frame per retry — split from
/// [`settle_ran`] at the 100-line cap (the block is self-contained: it
/// reads only the retry ledger and touches no record state).
fn emit_retry_history(
    id: &str,
    retries: &[task::RetryStamp],
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    for r in retries {
        emit(
            stamper,
            sink,
            EventKind::TaskRetrying,
            &[
                ("task", s(id)),
                ("attempt", i(i64::from(r.attempt))),
                ("max_attempts", i(i64::from(r.max_attempts))),
                ("delay_ms", i(i64::try_from(r.delay_ms).unwrap_or(i64::MAX))),
            ],
        );
    }
}

fn settle_ran(
    id: &str,
    run: task::RanTask,
    resume: Option<&resume::ResumeStamp>,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> TaskRecord {
    let started_at = emit(
        stamper,
        sink,
        EventKind::TaskStarted,
        &[("task", s(id)), ("note", s(&run.note))],
    );
    emit_retry_history(id, &run.retries, stamper, sink);
    // The agent loop's decisions (ADR-096 · buffered per dispatch · in
    // order across attempts) land between the attempt history and the
    // terminal frame — readers reconstruct per-attempt interleaving
    // from the `turn` field.
    agent_events::emit_agent_events(id, &run.agent_events, stamper, sink);
    let duration = i64::try_from(run.duration_ms).unwrap_or(i64::MAX);
    // Every attempt including the settling one (spec 13 §payload).
    let attempts = run.attempts();
    let mut record = TaskRecord::unran(TaskStatus::Success, TerminalCause::Normal);
    record.started_at = Some(started_at);
    record.duration_ms = Some(run.duration_ms);
    match run.result {
        task::RunResult::Success {
            value,
            tokens,
            recovered_from,
            warning,
            child,
            cost_usd,
            cost_unpriced,
        } => settle_success_terminal(
            id,
            &run.note,
            duration,
            (value, tokens, recovered_from, warning),
            child.as_deref(),
            (cost_usd, cost_unpriced),
            attempts,
            resume,
            &mut record,
            stamper,
            sink,
        ),
        task::RunResult::SkippedWithError {
            error,
            cost_usd,
            cost_unpriced,
        } => settle_skip_with_error(
            id,
            error,
            (cost_usd, cost_unpriced),
            &mut record,
            stamper,
            sink,
        ),
        task::RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
        } => settle_failed_terminal(
            id,
            &run.note,
            duration,
            error,
            (cost_usd, cost_unpriced),
            attempts,
            &mut record,
            ok,
            stamper,
            sink,
        ),
        // Backstop: a pending recovery parks BEFORE settle (the
        // `recover::settle_or_park` spine) — one reaching this site
        // settles its classification-time failure (total · no panic).
        task::RunResult::PendingRecovery(pending) => settle_failed_terminal(
            id,
            &run.note,
            duration,
            pending.render_error,
            (pending.failed.cost_usd, pending.failed.cost_unpriced),
            attempts,
            &mut record,
            ok,
            stamper,
            sink,
        ),
    }
    record
}

/// The failure terminal — the ONE site for a settled failure's frame +
/// record (the `Failed` arm and the pending-recovery backstop share it).
/// Billed-then-failed spend rides the frame — the dollars a dying task
/// burned must never vanish with it (already ledger-debited per attempt;
/// this is the frame's per-task truth).
/// The success terminal — `task_recovered` (when the success was
/// repaired · D-2026-07-08-N4 sequence) then `task_completed`, with the
/// spec-13 outcome derived from the settled record: `success/normal` or
/// `success/recovered` (+ the ORIGINAL error as `recovered_from`).
// REASON: the terminal frame's field surface + the settle pens — the
// same shape as `settle_ran` itself.
#[allow(clippy::too_many_arguments)]
fn settle_success_terminal(
    id: &str,
    note: &str,
    duration: i64,
    (value, tokens, recovered_from, warning): (
        Value,
        Option<i64>,
        Option<TaskErrorRecord>,
        Option<String>,
    ),
    child: Option<&child::ChildRunSummary>,
    (cost_usd, cost_unpriced): (Option<f64>, Option<nika_types::cost::UnpricedReason>),
    attempts: u32,
    resume: Option<&resume::ResumeStamp>,
    record: &mut TaskRecord,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    if let Some(original) = &recovered_from {
        emit_task::emit_recovered(id, &original.code, stamper, sink);
    }
    record.cause = if recovered_from.is_some() {
        TerminalCause::Recovered
    } else {
        TerminalCause::Normal
    };
    record.attempts = Some(attempts);
    record.recovered_from = recovered_from;
    record.output = value;
    let ended = emit_task::emit_completed(
        id,
        note,
        duration,
        tokens,
        cost_usd,
        cost_unpriced,
        warning.as_deref(),
        child,
        resume,
        record,
        stamper,
        sink,
    );
    record.ended_at = Some(ended);
}

// REASON: the terminal frame's field surface + the settle pens — the
// same shape as `settle_ran` itself.
#[allow(clippy::too_many_arguments)]
fn settle_failed_terminal(
    id: &str,
    note: &str,
    duration: i64,
    error: TaskErrorRecord,
    spend: (Option<f64>, Option<nika_types::cost::UnpricedReason>),
    attempts: u32,
    record: &mut TaskRecord,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    // The failure-cause triage (spec 13): timeout budget · retries
    // exhausted · plain verb error — assigned at the ONE failure site.
    record.status = TaskStatus::Failure;
    record.cause = record::failure_cause(&error, attempts);
    record.attempts = Some(attempts);
    let detail = format!("{} · {}", error.code, error.message);
    record.error = Some(error);
    let mut fields = vec![
        ("task", s(id)),
        ("note", s(note)),
        ("detail", s(&detail)),
        ("duration_ms", i(duration)),
    ];
    push_spend_fields(&mut fields, spend.0, spend.1);
    fields.push(("outcome", s(&record::outcome_json(record))));
    let ended = emit(stamper, sink, EventKind::TaskFailed, &fields);
    record.ended_at = Some(ended);
    *ok = false;
}

/// `on_error: skip` — the ONE state where status is skipped AND the
/// error stays readable (spec 05). The billed spend of the skipped
/// attempts rides the frame (skip ≠ refund). Outcome: `skipped/error_skip`
/// with the PRESERVED error (spec 13 · the skipped payload law carries
/// the error only — attempts stay off the record, per the closed table).
fn settle_skip_with_error(
    id: &str,
    error: TaskErrorRecord,
    spend: (Option<f64>, Option<nika_types::cost::UnpricedReason>),
    record: &mut TaskRecord,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    record.status = TaskStatus::Skipped;
    record.cause = TerminalCause::ErrorSkip;
    let detail = format!("{} · {}", error.code, error.message);
    record.error = Some(error);
    let mut fields = vec![
        ("task", s(id)),
        ("note", s("on_error · skip")),
        ("detail", s(&detail)),
    ];
    push_spend_fields(&mut fields, spend.0, spend.1);
    fields.push(("outcome", s(&record::outcome_json(record))));
    let ended = emit(stamper, sink, EventKind::TaskSkipped, &fields);
    record.ended_at = Some(ended);
}

/// Push the spend pair onto a frame's fields — absent stays absent
/// (never a fake zero), the WHY rides when named.
fn push_spend_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    cost_usd: Option<f64>,
    cost_unpriced: Option<nika_types::cost::UnpricedReason>,
) {
    if let Some(c) = cost_usd {
        fields.push(("cost_usd", FieldValue::Float(c)));
    }
    if let Some(reason) = cost_unpriced {
        fields.push(("cost_unpriced", s(reason.as_str())));
    }
}

/// Resolve workflow `outputs:` from the final records · an output whose
/// reference no longer resolves is omitted (spec §3) · single-island
/// templates preserve the referenced value's type.
fn resolve_outputs(
    wf: &RawWorkflow,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let scope = Scope::workflow_with_env_and_secrets(records, vars, env, secrets);
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
fn finalize_outputs(
    wf: &RawWorkflow,
    outputs: &BTreeMap<String, Value>,
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
    fields
}

/// The `--max-cost-usd` stop: settle-what-ran is already done (the wave
/// settled before the check); every task that never STARTED cancels
/// with the budget note, then the run fails with spent-vs-budget — the
/// LiteLLM-shaped error message (both numbers, always).
fn abort_on_budget(
    wf: &RawWorkflow,
    workflow_name: &str,
    mut records: BTreeMap<String, TaskRecord>,
    cache_hits: Vec<String>,
    snapshot: &ledger::LedgerSnapshot,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> RunOutcome {
    for task in &wf.tasks {
        let id = &task.value.id.value;
        if !records.contains_key(id) {
            // Spec 13 · cancelled/budget: this task was UNSTARTED when
            // the cap hit (in-flight work completed and was counted).
            let record = TaskRecord::unran(TaskStatus::Cancelled, TerminalCause::Budget);
            emit(
                stamper,
                sink,
                EventKind::TaskCancelled,
                &[
                    ("task", s(id)),
                    ("note", s("budget · --max-cost-usd reached")),
                    ("outcome", s(&record::outcome_json(&record))),
                ],
            );
            records.insert(id.clone(), record);
        }
    }
    let detail = format!(
        "NIKA-1704 · run budget exceeded — spent ${:.6} of ${:.6} (--max-cost-usd) · \
         in-flight work completed and was counted · unstarted tasks were cancelled",
        snapshot.spent_usd,
        snapshot.budget.unwrap_or(0.0),
    );
    let mut fields = vec![("workflow", s(workflow_name)), ("detail", s(&detail))];
    fields.extend(terminal_cost_fields(snapshot));
    emit(stamper, sink, EventKind::WorkflowFailed, &fields);

    let mut outcome = RunOutcome::new(false, records, BTreeMap::new()).with_ledger(snapshot);
    outcome.cache_hits = cache_hits;
    outcome
}

/// One typed-`outputs:` contract violation (spec 01 rule 6 · NIKA-VAR-009).
struct OutputTypeViolation {
    name: String,
    expected: String,
    actual: &'static str,
}

/// The FIRST typed `outputs:` value whose resolved JSON type does not match
/// its declared `type:` (spec 01 §engine-MUST rule 6 · NIKA-VAR-009 · the
/// output half of the callable contract). An output whose `${{ }}` reference
/// no longer resolves is OMITTED by [`resolve_outputs`] (spec §3 · not a type
/// error). `None` ⇒ every typed output honours its declared type.
fn first_output_type_violation(
    wf: &RawWorkflow,
    resolved: &BTreeMap<String, Value>,
) -> Option<OutputTypeViolation> {
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
        if !value_matches_vartype(value, *ty) {
            return Some(OutputTypeViolation {
                name: key.value.clone(),
                expected: ty.to_string(),
                actual: json_type_name(value),
            });
        }
    }
    None
}

/// Whether a resolved JSON value satisfies a declared [`VarType`]. Lenient
/// where the spec is silent: any JSON number satisfies `number`, and an
/// integer-valued number (incl. a whole float like `42.0`) satisfies
/// `integer` — only a genuine cross-type mismatch (a string where a number
/// is declared, an object where an array is, …) is a NIKA-VAR-009.
fn value_matches_vartype(value: &Value, ty: VarType) -> bool {
    match ty {
        VarType::String => value.is_string(),
        VarType::Number => value.is_number(),
        VarType::Integer => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        VarType::Boolean => value.is_boolean(),
        VarType::Array => value.is_array(),
        // CLOSED vocabulary (nika-vocab) — a new type is a spec change
        // that must land HERE explicitly (never leniently waved through).
        VarType::Object => value.is_object(),
    }
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

#[cfg(test)]
mod tests;
