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
mod dispatch;
mod errors;
mod expr;
mod jq;
mod pause;
mod record;
pub mod resume;
mod retry;
mod secret;
mod stamp;
mod task;

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
pub use record::{TaskErrorRecord, TaskRecord, TaskStatus};
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
}

impl RuntimeConfig {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(wave_parallelism: Option<NonZeroUsize>, jitter_seed: u64) -> Self {
        Self {
            wave_parallelism,
            jitter_seed,
        }
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
        }
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
    /// The run's SOURCE identity — sha256 hex over the exact bytes the
    /// operator ran (computed by the composer that read the file; the
    /// runtime never re-reads). Stamped on `workflow_started` so every
    /// journal names the definition it recorded: replay, diff and
    /// fork surfaces can prove « the file changed since this run »
    /// instead of guessing. `None` (embedded/test callers) = no claim.
    source_sha256: Option<String>,
}

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
            source_sha256: None,
        }
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
}

/// Emit one stamped event with the given fields.
fn emit(
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

fn s(v: &str) -> FieldValue {
    FieldValue::String(v.to_owned())
}

fn i(v: i64) -> FieldValue {
    FieldValue::Int(v)
}

/// Emit the run's opening frames · `WorkflowStarted` + one
/// `TaskScheduled` per task (the storyboard's fixed prologue).
fn emit_prologue(
    wf: &RawWorkflow,
    workflow_name: &str,
    source_sha256: Option<&str>,
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

/// The envelope's value view · `vars` defaults + the workflow name.
fn envelope_values(wf: &RawWorkflow) -> (BTreeMap<String, Value>, String) {
    let vars = wf
        .vars
        .iter()
        .filter_map(|(key, decl)| {
            let value = match decl {
                VarDecl::Untyped(v) => v.clone(),
                VarDecl::Typed { default, .. } => default.clone()?,
                // #[non_exhaustive] future forms carry no v0 value.
                _ => return None,
            };
            Some((key.value.clone(), value))
        })
        .collect();
    let name = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    (vars, name)
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
        let (mut vars, workflow_name) = envelope_values(wf);
        // Operator `--var` overrides win over the envelope defaults (F4) —
        // and give a `required: true` var without a default its value.
        for (key, value) in &self.var_overrides {
            vars.insert(key.clone(), value.clone());
        }
        // Resolve the `secrets:` namespace ONCE at run start (MINOR-B · the
        // injected composer resolver reads env/file). A miss leaves that
        // secret unbound → its `${{ secrets.X }}` reference raises NIKA-1702
        // (fail-closed · clean typed error · no token spent on a broken
        // secret). The resolved values flow ONLY where the IFC sanctioned
        // them (the clean check) and are never emitted to the event stream.
        let secrets = secret::resolve_secrets(self.secrets.as_ref(), &wf.secrets);
        // ADR-099 resume identities — secret markers + the leak-guard set,
        // derived once per run (keys are stamped on every success so any
        // `--json` trace is later resumable).
        let resume_ctx = resume::ResumeContext::of(wf, &secrets);
        // The declared capability boundary (spec 01 §permits) flows to every
        // task's dispatch scope so the exec sink can enforce it (NIKA-SEC-004).
        let permits = wf.permits.as_ref().map(|spanned| &spanned.value);
        emit_prologue(
            wf,
            &workflow_name,
            self.source_sha256.as_deref(),
            stamper,
            sink,
        );

        let mut records: BTreeMap<String, TaskRecord> = BTreeMap::new();
        let mut ok = true;
        let mut cache_hits: Vec<String> = Vec::new();

        for wave in &report.waves {
            // Resolve indices up front — a bad index is a schedule
            // breach (NIKA-1701 · run abort · the one system error).
            let mut members = Vec::with_capacity(wave.len());
            for &index in wave {
                let task = wf.tasks.get(index).ok_or(RuntimeError::WaveOutOfBounds {
                    index,
                    task_count: wf.tasks.len(),
                })?;
                members.push(&task.value);
            }

            // Dispatch concurrently over the wave-frozen records (same-
            // wave tasks never reference each other — checker law) ·
            // collect in submission order · settle sequentially below.
            let cap = self
                .config
                .wave_parallelism
                .map_or_else(|| members.len().max(1), NonZeroUsize::get);
            let finishes: Vec<Finish> = futures_util::stream::iter(members.iter().map(|&task| {
                self.run_task_pipeline(task, &records, &vars, &secrets, permits, &resume_ctx)
            }))
            .buffered(cap)
            .collect()
            .await;

            if let Some(outcome) = self.settle_wave(
                finishes,
                wf,
                &vars,
                &resume_ctx,
                &workflow_name,
                &mut records,
                &mut ok,
                &mut cache_hits,
                stamper,
                sink,
            ) {
                return Ok(outcome);
            }
        }

        // Resolve the `outputs:` BEFORE the terminal frame so a typed output
        // that breaks its declared `type:` can fail the run — the output half
        // of the callable contract (spec 01 §engine-MUST rule 6 · NIKA-VAR-009 ·
        // symmetric with the typed-`vars:` input validation).
        let outputs = resolve_outputs(wf, &records, &vars, &secrets);
        let ok = finalize_outputs(wf, &outputs, &workflow_name, ok, stamper, sink);

        let mut outcome = RunOutcome::new(ok, records, outputs);
        outcome.cache_hits = cache_hits;
        Ok(outcome)
    }

    /// Settle one wave's finishes in order — `Some(outcome)` iff the wave
    /// PAUSED on a blocked `nika:prompt` (ADR-099 rider · PROMPT-001
    /// under a non-interactive surface): siblings that finished still
    /// settle (their work is journaled · they cache-hit on resume); the
    /// blocked prompt itself never settles (no `task_failed` — it simply
    /// has not happened yet); later waves never dispatch.
    // The pens + the three run accumulators ARE the settle surface —
    // mirrors `settle` itself.
    #[allow(clippy::too_many_arguments)]
    fn settle_wave(
        &self,
        finishes: Vec<Finish>,
        wf: &RawWorkflow,
        vars: &BTreeMap<String, Value>,
        resume_ctx: &resume::ResumeContext,
        workflow_name: &str,
        records: &mut BTreeMap<String, TaskRecord>,
        ok: &mut bool,
        cache_hits: &mut Vec<String>,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
    ) -> Option<RunOutcome> {
        let mut paused: Option<WorkflowPause> = None;
        for finish in finishes {
            if self.pause_on_prompt
                && paused.is_none()
                && let Some(p) =
                    pause::prompt_block(&finish, wf, records, vars, resume_ctx.markers())
            {
                paused = Some(p);
                continue;
            }
            settle(finish, records, ok, cache_hits, stamper, sink);
        }
        let p = paused?;
        emit_paused(workflow_name, &p, stamper, sink);
        let mut outcome = RunOutcome::new(true, std::mem::take(records), BTreeMap::new());
        outcome.cache_hits = std::mem::take(cache_hits);
        outcome.paused = Some(p);
        Some(outcome)
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
/// inserts the result record.
fn settle(
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
            // The WHY rides along: which upstream kept the gate closed.
            let mut fields = vec![("task", s(&id)), ("note", s(note))];
            if let Some(culprit) = &blocked_by {
                fields.push(("blocked_by", s(culprit)));
            }
            emit(stamper, sink, EventKind::TaskCancelled, &fields);
            records.insert(
                id,
                with_named(TaskRecord::unran(TaskStatus::Cancelled), named),
            );
        }
        SettleAs::SkippedGate { note, expr } => {
            // The gate's own CEL text — « why did this not run » verbatim.
            let mut fields = vec![("task", s(&id)), ("note", s(note))];
            if let Some(cel) = &expr {
                fields.push(("when", s(cel)));
            }
            emit(stamper, sink, EventKind::TaskSkipped, &fields);
            records.insert(
                id,
                with_named(TaskRecord::unran(TaskStatus::Skipped), named),
            );
        }
        SettleAs::FailedBeforeStart { stage, error } => {
            // A pre-dispatch failure (gate eval · with · for_each
            // collection) — the task never started: no TaskStarted ·
            // no on_finally (spec 03) · the failure cascades.
            emit(
                stamper,
                sink,
                EventKind::TaskFailed,
                &[
                    ("task", s(&id)),
                    ("note", s(stage)),
                    ("detail", s(&format!("{} · {}", error.code, error.message))),
                ],
            );
            let mut record = TaskRecord::unran(TaskStatus::Failure);
            record.error = Some(error);
            records.insert(id, with_named(record, named));
            *ok = false;
        }
        SettleAs::CacheHit { output } => {
            // ADR-099 §2 — the skip is VISIBLE: one `task_cache_hit`
            // frame carrying the matched identity + the rehydrated
            // output (so a resumed run's own trace stays resumable).
            // No `TaskStarted` · no duration — the task never ran here.
            let mut fields = vec![("task", s(&id)), ("note", s("cache hit"))];
            let output_text = serde_json::to_string(&output).unwrap_or_else(|_| "null".to_owned());
            if let Some(stamp) = resume.as_ref() {
                fields.push((resume::fields::DEF_HASH, s(&stamp.def_hash)));
                fields.push((resume::fields::INPUT_HASH, s(&stamp.input_hash)));
                fields.push((resume::fields::OUTPUT, s(&output_text)));
            }
            let ended = emit(stamper, sink, EventKind::TaskCacheHit, &fields);
            let mut record = TaskRecord::unran(TaskStatus::Success);
            record.output = output;
            record.ended_at = Some(ended);
            record.named = named;
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
    for r in &run.retries {
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
    // The agent loop's decisions (ADR-096 · buffered per dispatch · in
    // order across attempts) land between the attempt history and the
    // terminal frame — readers reconstruct per-attempt interleaving
    // from the `turn` field.
    agent_events::emit_agent_events(id, &run.agent_events, stamper, sink);
    let duration = i64::try_from(run.duration_ms).unwrap_or(i64::MAX);
    let mut record = TaskRecord::unran(TaskStatus::Success);
    record.started_at = Some(started_at);
    record.duration_ms = Some(run.duration_ms);
    match run.result {
        task::RunResult::Success {
            value,
            tokens,
            warning,
            cost_usd,
        } => {
            let ended = emit_completed(
                id,
                &run.note,
                duration,
                tokens,
                cost_usd,
                warning.as_deref(),
                resume,
                &value,
                stamper,
                sink,
            );
            record.ended_at = Some(ended);
            record.output = value;
        }
        task::RunResult::SkippedWithError { error } => {
            // `on_error: skip` — the ONE state where status is skipped
            // AND the error stays readable (spec 05).
            let ended = emit(
                stamper,
                sink,
                EventKind::TaskSkipped,
                &[
                    ("task", s(id)),
                    ("note", s("on_error · skip")),
                    ("detail", s(&format!("{} · {}", error.code, error.message))),
                ],
            );
            record.status = TaskStatus::Skipped;
            record.ended_at = Some(ended);
            record.error = Some(error);
        }
        task::RunResult::Failed { error } => {
            let ended = emit(
                stamper,
                sink,
                EventKind::TaskFailed,
                &[
                    ("task", s(id)),
                    ("note", s(&run.note)),
                    ("detail", s(&format!("{} · {}", error.code, error.message))),
                    ("duration_ms", i(duration)),
                ],
            );
            record.status = TaskStatus::Failure;
            record.ended_at = Some(ended);
            record.error = Some(error);
            *ok = false;
        }
    }
    record
}

/// Emit one `task_completed` frame — the base fields (`note` ·
/// `duration_ms`) + spend (`tokens`) + the OBS-E `warning` diagnostic
/// when present + the ADR-099 checkpoint trio (`def_hash` · `input_hash`
/// · `output` as ONE compact JSON text) when the task carries a resume
/// stamp. Returns the terminal timestamp.
// The 7 payload knobs mirror the frame's field surface — a builder
// struct would just restate them.
#[allow(clippy::too_many_arguments)]
fn emit_completed(
    id: &str,
    note: &str,
    duration: i64,
    tokens: Option<i64>,
    cost_usd: Option<f64>,
    warning: Option<&str>,
    resume: Option<&resume::ResumeStamp>,
    value: &Value,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) -> nika_types::timestamp::Timestamp {
    let mut fields = vec![
        ("task", s(id)),
        ("note", s(note)),
        ("duration_ms", i(duration)),
    ];
    if let Some(n) = tokens {
        fields.push(("tokens", i(n)));
    }
    // Real spend rides next to the tokens it prices · absent = unpriced
    // (mock · local) — the render layer already treats absent as honest.
    if let Some(c) = cost_usd {
        fields.push(("cost_usd", FieldValue::Float(c)));
    }
    // OBS-E · a non-fatal diagnostic rides the success frame as a
    // `warning` field (the reasoning-model blank-answer footgun) · the
    // task still completes.
    if let Some(msg) = warning {
        fields.push(("warning", s(msg)));
    }
    // ADR-099 · the checkpoint fields — only a stamped success carries
    // them (additive trace fields).
    let output_text =
        resume.map(|_| serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned()));
    if let (Some(stamp), Some(text)) = (resume, output_text.as_deref()) {
        fields.push((resume::fields::DEF_HASH, s(&stamp.def_hash)));
        fields.push((resume::fields::INPUT_HASH, s(&stamp.input_hash)));
        fields.push((resume::fields::OUTPUT, s(text)));
    }
    emit(stamper, sink, EventKind::TaskCompleted, &fields)
}

/// Resolve workflow `outputs:` from the final records · an output whose
/// reference no longer resolves is omitted (spec §3) · single-island
/// templates preserve the referenced value's type.
fn resolve_outputs(
    wf: &RawWorkflow,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let scope = Scope::workflow_with_secrets(records, vars, secrets);
    wf.outputs
        .iter()
        .filter_map(|(key, decl)| {
            let template = match decl {
                OutputDecl::Untyped(v) => &v.value,
                OutputDecl::Typed { value, .. } => &value.value,
                // #[non_exhaustive] future forms carry no v0 value.
                _ => return None,
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
    if let Some(v) = &violation {
        emit(
            stamper,
            sink,
            kind,
            &[
                ("workflow", s(workflow_name)),
                (
                    "detail",
                    s(&format!(
                        "NIKA-VAR-009 · output `{}` is {}, declared type: {}",
                        v.name, v.actual, v.expected
                    )),
                ),
            ],
        );
    } else {
        emit(stamper, sink, kind, &[("workflow", s(workflow_name))]);
    }
    ok
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
        VarType::Object => value.is_object(),
        // `VarType` is `#[non_exhaustive]`: a future type this engine version
        // does not yet model is treated leniently (no NIKA-VAR-009) rather
        // than failing a run on a contract it cannot evaluate.
        _ => true,
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
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_default_is_wave_width_seed_zero() {
        let cfg = RuntimeConfig::default();
        assert!(cfg.wave_parallelism.is_none());
        assert_eq!(cfg.jitter_seed, 0);
    }

    #[test]
    fn envelope_values_carries_typed_defaults_and_containers() {
        // The v1 string-only view dropped typed list defaults — the
        // value model must carry them (for_each collections · spec 03).
        let yaml = r#"
nika: v1
workflow: vals
vars:
  plain: "text"
  urls: ["a", "b"]
  topic: { type: string, default: "news" }
tasks:
  - id: t
    exec: { command: "true" }
"#;
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        let (vars, name) = envelope_values(&wf);
        assert_eq!(name, "vals");
        assert_eq!(vars["plain"], Value::String("text".into()));
        assert_eq!(vars["urls"], serde_json::json!(["a", "b"]));
        assert_eq!(vars["topic"], Value::String("news".into()));
    }

    #[test]
    fn typed_output_type_mismatch_is_a_var009() {
        // `outputs.n: { type: string }` — when the resolved value is a number
        // the callable contract is broken (spec 01 §engine-MUST rule 6).
        let yaml = r#"
nika: v1
workflow: typed-out
tasks:
  - id: t
    invoke: { tool: "nika:jq", args: { input: { x: 42 }, expression: ".x" } }
outputs:
  n:
    value: ${{ tasks.t.output }}
    type: string
"#;
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("parses");
        // A number where `string` is declared → NIKA-VAR-009.
        let bad = BTreeMap::from([("n".to_owned(), serde_json::json!(42))]);
        let v = first_output_type_violation(&wf, &bad)
            .expect("number vs declared string is a violation");
        assert_eq!(v.name, "n");
        assert_eq!(v.expected, "string");
        assert_eq!(v.actual, "number");
        // The declared type → no violation.
        let good = BTreeMap::from([("n".to_owned(), serde_json::json!("hello"))]);
        assert!(first_output_type_violation(&wf, &good).is_none());
        // An unresolved output (omitted upstream) is NOT a type error.
        assert!(first_output_type_violation(&wf, &BTreeMap::new()).is_none());
    }

    #[test]
    fn value_matches_vartype_lenient_floats_strict_cross_type() {
        use serde_json::json;
        // integer: whole floats OK, fractional rejected, numeric STRING rejected.
        assert!(value_matches_vartype(&json!(42), VarType::Integer));
        assert!(value_matches_vartype(&json!(42.0), VarType::Integer));
        assert!(!value_matches_vartype(&json!(42.5), VarType::Integer));
        // number: any JSON number, but NOT a numeric string.
        assert!(value_matches_vartype(&json!(42), VarType::Number));
        assert!(!value_matches_vartype(&json!("42"), VarType::Number));
        // array vs object are distinct.
        assert!(value_matches_vartype(&json!([1, 2]), VarType::Array));
        assert!(!value_matches_vartype(&json!({}), VarType::Array));
        assert!(value_matches_vartype(&json!({ "k": 1 }), VarType::Object));
        assert!(value_matches_vartype(&json!("x"), VarType::String));
        assert!(value_matches_vartype(&json!(true), VarType::Boolean));
    }

    /// A settled success carrying an OBS-E `warning` puts it on the
    /// `TaskCompleted` frame as a `warning` field — the wiring proof that
    /// the dispatch's diagnostic actually reaches the event stream.
    #[test]
    fn obs_e_warning_rides_task_completed() {
        let ran = task::RanTask {
            note: "infer · gemini/flash".to_owned(),
            retries: Vec::new(),
            agent_events: Vec::new(),
            duration_ms: 0,
            result: task::RunResult::Success {
                value: Value::String(String::new()),
                tokens: Some(84),
                warning: Some("infer produced an empty answer · …".to_owned()),
                cost_usd: Some(0.0125),
            },
        };
        let mut ok = true;
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        settle_ran("think", ran, None, &mut ok, &mut stamper, &mut sink);

        let completed = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted)
            .expect("a TaskCompleted frame");
        let warning = completed
            .fields
            .iter()
            .find(|f| f.key == "warning")
            .expect("the warning field rides the success frame");
        assert!(
            matches!(&warning.value, FieldValue::String(s) if s.contains("empty answer")),
            "the diagnostic text is carried verbatim"
        );
        // Real spend rides the same frame · absent-when-unpriced is pinned
        // by the sibling test below (its cost_usd is None · no field).
        let cost = completed
            .fields
            .iter()
            .find(|f| f.key == "cost_usd")
            .expect("the cost_usd field rides the priced success frame");
        assert!(
            matches!(&cost.value, FieldValue::Float(c) if (*c - 0.0125).abs() < f64::EPSILON),
            "the priced spend is carried verbatim"
        );
    }

    /// The common path · a success with no OBS-E diagnostic emits NO
    /// `warning` field (zero false-alarm noise on the happy stream).
    #[test]
    fn no_warning_field_on_a_clean_success() {
        let ran = task::RanTask {
            note: "exec · true".to_owned(),
            retries: Vec::new(),
            agent_events: Vec::new(),
            duration_ms: 0,
            result: task::RunResult::Success {
                value: Value::String("ok".to_owned()),
                tokens: None,
                warning: None,
                cost_usd: None,
            },
        };
        let mut ok = true;
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        settle_ran("t", ran, None, &mut ok, &mut stamper, &mut sink);

        let completed = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted)
            .expect("a TaskCompleted frame");
        assert!(
            !completed.fields.iter().any(|f| f.key == "warning"),
            "no warning on a clean success"
        );
        assert!(
            !completed.fields.iter().any(|f| f.key == "cost_usd"),
            "an unpriced success carries NO cost field — absent is honest, never a fake zero"
        );
    }
}

/// F4 — operator `--var` overrides through the REAL parse → check → run
/// chain: an override wins over a declared `default:`, and a
/// `required: true` var without one becomes runnable (before: the run
/// could only die NIKA-VAR-001 at first reference).
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod var_override_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;
    use serde_json::Value;

    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

    const WORKFLOW: &str = r#"
nika: v1
workflow: var-override
vars:
  topic:
    type: string
    required: true
  lang: { type: string, default: "en" }
tasks:
  - id: say
    exec: { command: "echo ${{ vars.topic }}" }
outputs:
  topic_out: ${{ vars.topic }}
  lang_out: ${{ vars.lang }}
"#;

    async fn run_with(overrides: BTreeMap<String, Value>) -> RunOutcome {
        let wf = nika_schema::parse(
            WORKFLOW,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        assert!(report.is_clean(), "fixture passes the ladder");

        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("said\n"))),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_var_overrides(overrides);
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run")
    }

    #[tokio::test]
    async fn override_satisfies_a_required_var_and_beats_the_default() {
        let overrides = BTreeMap::from([
            ("topic".to_owned(), Value::String("rust".to_owned())),
            ("lang".to_owned(), Value::String("fr".to_owned())),
        ]);
        let outcome = run_with(overrides).await;
        assert!(outcome.ok, "the required var is satisfied → green run");
        assert_eq!(outcome.outputs["topic_out"], "rust");
        assert_eq!(
            outcome.outputs["lang_out"], "fr",
            "an override wins over the declared default"
        );
    }

    #[tokio::test]
    async fn missing_required_var_still_fails_var001_at_reference() {
        // No override → the pre-F4 behavior is intact: the task's
        // `${{ vars.topic }}` fails NIKA-VAR-001 (with the --var hint).
        let outcome = run_with(BTreeMap::new()).await;
        assert!(!outcome.ok, "unbound required var fails the task");
        let record = &outcome.records["say"];
        let error = record.error.as_ref().expect("task carries its error");
        assert_eq!(error.code, "NIKA-VAR-001");
        assert!(
            error.message.contains("--var"),
            "the message teaches the CLI fix: {}",
            error.message
        );
    }
}
