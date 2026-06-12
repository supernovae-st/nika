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
mod record;
mod retry;
mod stamp;
mod task;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use futures_util::StreamExt;
use nika_event::{Event, EventKind};
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::clock::ClockDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_schema::types::{OutputDecl, VarDecl};
use nika_types::resource::{KeyValue, Value as FieldValue};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

pub use errors::RuntimeError;
pub use record::{TaskErrorRecord, TaskRecord, TaskStatus};
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
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C> {
    /// Assemble the runtime from its four verbs + the kernel clock
    /// (the composer wires seams + envelope defaults · spec §2).
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
        }
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
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    emit(
        stamper,
        sink,
        EventKind::WorkflowStarted,
        &[
            ("workflow", s(workflow_name)),
            ("permits", s("engine floor (no boundary declared)")),
        ],
    );
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
    P: ProviderInferDyn,
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
        let (vars, workflow_name) = envelope_values(wf);
        // The declared capability boundary (spec 01 §permits) flows to every
        // task's dispatch scope so the exec sink can enforce it (NIKA-SEC-004).
        let permits = wf.permits.as_ref().map(|spanned| &spanned.value);
        emit_prologue(wf, &workflow_name, stamper, sink);

        let mut records: BTreeMap<String, TaskRecord> = BTreeMap::new();
        let mut ok = true;

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
            let finishes: Vec<Finish> = futures_util::stream::iter(
                members
                    .iter()
                    .map(|&task| self.run_task_pipeline(task, &records, &vars, permits)),
            )
            .buffered(cap)
            .collect()
            .await;

            for finish in finishes {
                settle(finish, &mut records, &mut ok, stamper, sink);
            }
        }

        let terminal = if ok {
            EventKind::WorkflowCompleted
        } else {
            EventKind::WorkflowFailed
        };
        emit(stamper, sink, terminal, &[("workflow", s(&workflow_name))]);

        let outputs = resolve_outputs(wf, &records, &vars);
        Ok(RunOutcome::new(ok, records, outputs))
    }
}

/// Settle one task in wave order · owns the pens (stamper + sink) ·
/// inserts the result record.
fn settle(
    finish: Finish,
    records: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    let id = finish.id;
    match finish.settle {
        SettleAs::Cancelled { note } => {
            emit(
                stamper,
                sink,
                EventKind::TaskCancelled,
                &[("task", s(&id)), ("note", s(note))],
            );
            records.insert(id, TaskRecord::unran(TaskStatus::Cancelled));
        }
        SettleAs::SkippedGate { note } => {
            emit(
                stamper,
                sink,
                EventKind::TaskSkipped,
                &[("task", s(&id)), ("note", s(note))],
            );
            records.insert(id, TaskRecord::unran(TaskStatus::Skipped));
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
            records.insert(id, record);
            *ok = false;
        }
        SettleAs::Ran(run) => {
            let record = settle_ran(&id, run, ok, stamper, sink);
            records.insert(id, record);
        }
    }
}

/// Settle a task that RAN — the started frame · the retry history ·
/// the terminal frame · the result record (spec §3.9).
fn settle_ran(
    id: &str,
    run: task::RanTask,
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
    // The agent loop's decisions (ADR-093 · buffered per dispatch · in
    // order across attempts) land between the attempt history and the
    // terminal frame — readers reconstruct per-attempt interleaving
    // from the `turn` field.
    agent_events::emit_agent_events(id, &run.agent_events, stamper, sink);
    let duration = i64::try_from(run.duration_ms).unwrap_or(i64::MAX);
    let mut record = TaskRecord::unran(TaskStatus::Success);
    record.started_at = Some(started_at);
    record.duration_ms = Some(run.duration_ms);
    match run.result {
        task::RunResult::Success { value, tokens } => {
            let mut fields = vec![
                ("task", s(id)),
                ("note", s(&run.note)),
                ("duration_ms", i(duration)),
            ];
            if let Some(n) = tokens {
                fields.push(("tokens", i(n)));
            }
            let ended = emit(stamper, sink, EventKind::TaskCompleted, &fields);
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

/// Resolve workflow `outputs:` from the final records · an output whose
/// reference no longer resolves is omitted (spec §3) · single-island
/// templates preserve the referenced value's type.
fn resolve_outputs(
    wf: &RawWorkflow,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let scope = Scope::workflow(records, vars);
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
}
