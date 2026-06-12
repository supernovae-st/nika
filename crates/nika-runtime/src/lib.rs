// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-runtime` — the L3 orchestrator (the first L3 crate).
//!
//! Executes one **checked** workflow wave-by-wave through the four verb
//! crates, emitting the canonical event stream. This crate owns what
//! `crates/nika-cli/tests/e2e_pipeline.rs` rehearsed: the harness PLAYED
//! the missing layer over the real shipped verbs · the runtime IS that
//! layer · the rehearsal's assertions are its conformance floor (same
//! YAML in · same event stream out). Spec:
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
//! - **Loud expressions** · unresolved `${{ }}` is `NIKA-1702` · a
//!   `when:` outside the v0 subset is `NIKA-1703` (see the private
//!   `expr` module · never a silent literal · never a silently-closed
//!   gate).
//!
//! ## Seams (why 5 generics)
//!
//! The agent's tool-definition impl lives in `nika-builtin` (WIP · not
//! admitted) — an admitted crate never depends on a WIP crate, so the
//! runtime stays seam-generic exactly like `AgentVerb` and the composer
//! (nika-cli · L4) injects. The four verbs arrive PRE-CONSTRUCTED:
//! their defaults (model · seams) are envelope concerns the composer
//! resolves before the run.

#![forbid(unsafe_code)]

mod errors;
mod expr;
mod stamp;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nika_error::traits::NikaErrorCode;
use nika_event::{Event, EventKind};
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::check::CheckReport;
use nika_schema::raw::{RawAction, RawCommand, RawWorkflow};
use nika_schema::types::{OutputDecl, VarDecl, WhenGate};
use nika_types::resource::{KeyValue, Value};
use nika_verb_agent::{AgentInput, AgentValue, AgentVerb};
use nika_verb_exec::{ExecInput, ExecValue, ExecVerb};
use nika_verb_infer::{InferInput, InferValue, InferVerb};
use nika_verb_invoke::{InvokeInput, InvokeVerb};

pub use errors::RuntimeError;
pub use stamp::{DeterministicStamper, EventSink, Stamper, VecSink};

use expr::Scope;

/// One task's terminal outcome inside the wave loop.
enum Outcome {
    Ok {
        output: String,
        note: String,
        tokens: Option<i64>,
    },
    Failed {
        detail: String,
        note: String,
    },
}

impl Outcome {
    /// The dispatch note (`invoke · <tool>` · `exec · <argv0>` · …) ·
    /// identical on both arms · stamped on `TaskStarted`.
    fn note(&self) -> &str {
        match self {
            Self::Ok { note, .. } | Self::Failed { note, .. } => note,
        }
    }
}

/// The run's verdict + resolved dataflow (spec §2).
#[derive(Debug)]
#[non_exhaustive]
pub struct RunOutcome {
    /// Terminal `WorkflowCompleted` (true) vs `WorkflowFailed`.
    pub ok: bool,
    /// `tasks.<id>.output` for every task that completed.
    pub bindings: BTreeMap<String, String>,
    /// Workflow `outputs:` resolved from the final bindings (an output
    /// whose reference no longer resolves is omitted · the verdict is
    /// unchanged · spec §3).
    pub outputs: BTreeMap<String, String>,
}

impl RunOutcome {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(
        ok: bool,
        bindings: BTreeMap<String, String>,
        outputs: BTreeMap<String, String>,
    ) -> Self {
        Self {
            ok,
            bindings,
            outputs,
        }
    }
}

/// The L3 executor over the four pre-constructed verbs.
pub struct Runtime<S, T, H, P, D> {
    shell: ExecVerb<S>,
    invoke: Arc<InvokeVerb<T>>,
    infer: InferVerb<H>,
    agent: AgentVerb<P, T, D>,
}

impl<S, T, H, P, D> Runtime<S, T, H, P, D> {
    /// Assemble the runtime from its four verbs (the composer wires
    /// seams + envelope defaults · spec §2).
    #[must_use]
    pub fn new(
        shell: ExecVerb<S>,
        invoke: Arc<InvokeVerb<T>>,
        infer: InferVerb<H>,
        agent: AgentVerb<P, T, D>,
    ) -> Self {
        Self {
            shell,
            invoke,
            infer,
            agent,
        }
    }
}

/// Emit one stamped event with the given fields.
fn emit(
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
    kind: EventKind,
    fields: &[(&str, Value)],
) {
    let (id, ts) = stamper.next();
    let mut event = Event::new(id, ts, kind);
    for (key, value) in fields {
        event = event.with_field(KeyValue::new(*key, value.clone()));
    }
    sink.emit(event);
}

fn s(v: &str) -> Value {
    Value::String(v.to_owned())
}

fn i(v: i64) -> Value {
    Value::Int(v)
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

/// Evaluate a task's optional `when:` gate (v0 subset · spec §3) ·
/// `Ok(true)` when absent or open.
fn gate_is_open(
    gate: Option<&nika_schema::Spanned<WhenGate>>,
    scope: &Scope<'_>,
) -> Result<bool, RuntimeError> {
    let Some(gate) = gate else { return Ok(true) };
    match &gate.value {
        WhenGate::Literal(b) => Ok(*b),
        WhenGate::Expr(body) => expr::eval_when(body, scope),
        // #[non_exhaustive] · a future gate form is out of the v0
        // subset by definition · loud, never silently closed.
        other => Err(RuntimeError::WhenUnsupported {
            expr: format!("{other:?}"),
        }),
    }
}

/// Mutable run state threaded through the wave loop · completed
/// outputs (`bindings`) · the failure-cascade set (`dead`) · the
/// terminal verdict (`ok`).
struct RunState {
    bindings: BTreeMap<String, String>,
    dead: BTreeSet<String>,
    ok: bool,
}

/// The envelope's string view · `vars` defaults + the workflow name.
fn envelope_strings(wf: &RawWorkflow) -> (BTreeMap<String, String>, String) {
    let vars = wf
        .vars
        .iter()
        .filter_map(|(key, decl)| {
            let value = match decl {
                VarDecl::Untyped(v) => v.as_str().map(str::to_owned),
                VarDecl::Typed { default, .. } => {
                    default.as_ref().and_then(|d| d.as_str()).map(str::to_owned)
                }
                // #[non_exhaustive] future forms carry no v0 string value.
                _ => None,
            }?;
            Some((key.value.clone(), value))
        })
        .collect();
    let name = wf
        .workflow
        .as_ref()
        .map_or_else(|| "workflow".to_owned(), |w| w.value.clone());
    (vars, name)
}

impl<S, T, H, P, D> Runtime<S, T, H, P, D>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn,
    D: ToolDefinitionProviderDyn,
{
    /// Execute the workflow per the report's wave schedule (spec §3).
    ///
    /// # Errors
    ///
    /// [`RuntimeError::DirtyReport`] (NIKA-1700) · audit-before-run ·
    /// [`RuntimeError::WaveOutOfBounds`] (NIKA-1701) · schedule breach ·
    /// [`RuntimeError::UnresolvedTemplate`] / [`RuntimeError::WhenUnsupported`]
    /// (NIKA-1702/1703) from a gate expression. Template failures INSIDE
    /// a task body fail THAT task (cascade) · they do not abort the run.
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
        let (vars, workflow_name) = envelope_strings(wf);
        emit_prologue(wf, &workflow_name, stamper, sink);

        let mut state = RunState {
            bindings: BTreeMap::new(),
            dead: BTreeSet::new(),
            ok: true,
        };

        for wave in &report.waves {
            for &index in wave {
                let task = &wf
                    .tasks
                    .get(index)
                    .ok_or(RuntimeError::WaveOutOfBounds {
                        index,
                        task_count: wf.tasks.len(),
                    })?
                    .value;
                let id = task.id.value.clone();

                // Upstream-failure cascade · any dead dependency skips
                // this task and propagates death downstream.
                if task
                    .depends_on
                    .iter()
                    .any(|d| state.dead.contains(&d.value))
                {
                    emit(
                        stamper,
                        sink,
                        EventKind::TaskSkipped,
                        &[("task", s(&id)), ("note", s("upstream failed"))],
                    );
                    state.dead.insert(id);
                    continue;
                }

                // `when:` gate (v0 subset · spec §3). A closed gate is a
                // pure skip · NOT a cascade (downstream with live deps
                // still runs · floor semantics).
                let gate = Scope {
                    bindings: &state.bindings,
                    vars: &vars,
                };
                if !gate_is_open(task.when.as_ref(), &gate)? {
                    emit(
                        stamper,
                        sink,
                        EventKind::TaskSkipped,
                        &[("task", s(&id)), ("note", s("when: gate closed"))],
                    );
                    continue;
                }

                self.run_task(id, &task.action, &vars, stamper, sink, &mut state)
                    .await;
            }
        }

        let terminal = if state.ok {
            EventKind::WorkflowCompleted
        } else {
            EventKind::WorkflowFailed
        };
        emit(stamper, sink, terminal, &[("workflow", s(&workflow_name))]);

        let outputs = resolve_outputs(wf, &state.bindings, &vars);
        Ok(RunOutcome::new(state.ok, state.bindings, outputs))
    }

    /// Execute one scheduled task · dispatch through its verb and
    /// settle the outcome (events + bindings on success · the dead
    /// set + verdict on failure).
    async fn run_task(
        &self,
        id: String,
        action: &RawAction,
        vars: &BTreeMap<String, String>,
        stamper: &mut dyn Stamper,
        sink: &mut dyn EventSink,
        state: &mut RunState,
    ) {
        let scope = Scope {
            bindings: &state.bindings,
            vars,
        };
        let outcome = self.dispatch(action, &scope).await;
        emit(
            stamper,
            sink,
            EventKind::TaskStarted,
            &[("task", s(&id)), ("note", s(outcome.note()))],
        );
        match outcome {
            Outcome::Ok {
                output,
                note,
                tokens,
            } => {
                let mut fields = vec![("task", s(&id)), ("note", s(&note))];
                if let Some(n) = tokens {
                    fields.push(("tokens", i(n)));
                }
                emit(stamper, sink, EventKind::TaskCompleted, &fields);
                state.bindings.insert(id, output);
            }
            Outcome::Failed { detail, note } => {
                emit(
                    stamper,
                    sink,
                    EventKind::TaskFailed,
                    &[("task", s(&id)), ("note", s(&note)), ("detail", s(&detail))],
                );
                state.dead.insert(id);
                state.ok = false;
            }
        }
    }

    /// Dispatch one action through its verb · template failures inside
    /// the body fail the task (never abort the run).
    async fn dispatch(&self, action: &RawAction, scope: &Scope<'_>) -> Outcome {
        match action {
            RawAction::Invoke(inner) => self.dispatch_invoke(inner, scope).await,
            RawAction::Exec(inner) => self.dispatch_shell(inner, scope).await,
            RawAction::Infer(inner) => self.dispatch_infer(inner, scope).await,
            RawAction::Agent(inner) => self.dispatch_agent(inner, scope).await,
            // #[non_exhaustive] · a future verb must land HERE loudly ·
            // the runtime refuses rather than silently no-ops.
            other => Outcome::Failed {
                detail: format!("verb not wired in the runtime yet: {other:?}"),
                note: "unknown verb".to_owned(),
            },
        }
    }

    async fn dispatch_invoke(
        &self,
        action: &nika_schema::raw::RawInvokeAction,
        scope: &Scope<'_>,
    ) -> Outcome {
        let tool = action.tool.value.clone();
        let note = format!("invoke · {tool}");
        let args = match &action.args {
            None => serde_json::Value::Object(serde_json::Map::new()),
            Some(a) => match expr::render_json(&a.value, scope) {
                Ok(v) => v,
                Err(err) => return template_failure(&err, &note),
            },
        };
        let mut input = InvokeInput::new(tool);
        input.args = args;
        match self.invoke.run(input).await {
            Ok(out) => Outcome::Ok {
                output: out.content,
                note,
                tokens: None,
            },
            Err(err) => Outcome::Failed {
                detail: format!("{} · {err}", err.nika_code()),
                note,
            },
        }
    }

    async fn dispatch_shell(
        &self,
        action: &nika_schema::raw::RawExecAction,
        scope: &Scope<'_>,
    ) -> Outcome {
        let rendered = match &action.command {
            RawCommand::Shell(text) => expr::render(&text.value, scope),
            // v0 joins argv for the shell seam (the argv runner path is
            // an engine roadmap item) · render each part first.
            RawCommand::Argv(parts) => parts
                .iter()
                .map(|p| expr::render(&p.value, scope))
                .collect::<Result<Vec<_>, _>>()
                .map(|v| v.join(" ")),
            // #[non_exhaustive] · refuse loudly · never guess a shape.
            other => {
                return Outcome::Failed {
                    detail: format!("command form not wired in the runtime yet: {other:?}"),
                    note: "exec · ?".to_owned(),
                };
            }
        };
        let command = match rendered {
            Ok(c) => c,
            Err(err) => return template_failure(&err, "exec · ?"),
        };
        let program = command.split_whitespace().next().unwrap_or("?").to_owned();
        let note = format!("exec · {program}");
        match self.shell.run(ExecInput::new(command)).await {
            Ok(out) => {
                let text = match out.output {
                    ExecValue::Text(text) => text,
                    ExecValue::Structured { stdout, .. } => stdout,
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Outcome::Failed {
                            detail: format!("exec value form not wired yet: {other:?}"),
                            note,
                        };
                    }
                };
                Outcome::Ok {
                    output: text.trim_end().to_owned(),
                    note,
                    tokens: None,
                }
            }
            Err(err) => Outcome::Failed {
                detail: format!("{} · {err}", err.nika_code()),
                note,
            },
        }
    }

    async fn dispatch_infer(
        &self,
        action: &nika_schema::raw::RawInferAction,
        scope: &Scope<'_>,
    ) -> Outcome {
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return template_failure(&err, "infer · ?"),
        };
        let mut input = InferInput::new(prompt);
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return template_failure(&err, "infer · ?"),
        };
        input.model = action.model.as_ref().map(|m| m.value.clone());
        #[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
        {
            input.temperature = action.temperature.as_ref().map(|t| t.value as f32);
        }
        input.max_tokens = action.max_tokens.as_ref().map(|t| t.value);
        input.schema = action.schema.as_ref().map(|v| v.value.clone());
        match self.infer.run(input).await {
            Ok(out) => {
                let note = format!("infer · {}", out.model_resolved);
                let text = match out.output {
                    InferValue::Text(text) => text,
                    InferValue::Structured(value) => value.to_string(),
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Outcome::Failed {
                            detail: format!("infer value form not wired yet: {other:?}"),
                            note,
                        };
                    }
                };
                Outcome::Ok {
                    output: text,
                    note,
                    tokens: Some(i64::try_from(out.usage.output_tokens).unwrap_or(i64::MAX)),
                }
            }
            Err(err) => Outcome::Failed {
                detail: format!("{} · {err}", err.nika_code()),
                note: "infer · ?".to_owned(),
            },
        }
    }

    async fn dispatch_agent(
        &self,
        action: &nika_schema::raw::RawAgentAction,
        scope: &Scope<'_>,
    ) -> Outcome {
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return template_failure(&err, "agent · ?"),
        };
        let mut input = AgentInput::new(prompt);
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return template_failure(&err, "agent · ?"),
        };
        input.model = action.model.as_ref().map(|m| m.value.clone());
        input.tools = action.tools.iter().map(|t| t.value.clone()).collect();
        input.max_turns = action.max_turns.as_ref().map(|t| t.value);
        input.max_tokens_total = action.max_tokens_total.as_ref().map(|t| t.value);
        #[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
        {
            input.temperature = action.temperature.as_ref().map(|t| t.value as f32);
        }
        input.schema = action.schema.as_ref().map(|v| v.value.clone());
        match self.agent.run(input).await {
            Ok(out) => {
                let note = format!("agent · {} turns", out.turns);
                let text = match out.output {
                    AgentValue::Text(text) => text,
                    AgentValue::Structured(value) => value.to_string(),
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Outcome::Failed {
                            detail: format!("agent value form not wired yet: {other:?}"),
                            note,
                        };
                    }
                };
                Outcome::Ok {
                    output: text,
                    note,
                    tokens: Some(i64::try_from(out.total_tokens).unwrap_or(i64::MAX)),
                }
            }
            Err(err) => Outcome::Failed {
                detail: format!("{} · {err}", err.nika_code()),
                note: "agent · ?".to_owned(),
            },
        }
    }
}

/// Render an optional spanned string field.
fn render_opt(
    field: Option<&nika_schema::Spanned<String>>,
    scope: &Scope<'_>,
) -> Result<Option<String>, RuntimeError> {
    field.map(|f| expr::render(&f.value, scope)).transpose()
}

/// A template failure inside a task body · the task fails (cascade) ·
/// the run continues (spec §3 · run-abort is reserved for gate/schedule
/// contract breaches).
fn template_failure(err: &RuntimeError, note: &str) -> Outcome {
    Outcome::Failed {
        detail: format!("{} · {err}", err.nika_code()),
        note: note.to_owned(),
    }
}

/// Resolve workflow `outputs:` from the final bindings · an output whose
/// reference no longer resolves is omitted (spec §3).
fn resolve_outputs(
    wf: &RawWorkflow,
    bindings: &BTreeMap<String, String>,
    vars: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let scope = Scope { bindings, vars };
    wf.outputs
        .iter()
        .filter_map(|(key, decl)| {
            let template = match decl {
                OutputDecl::Untyped(v) => &v.value,
                OutputDecl::Typed { value, .. } => &value.value,
                // #[non_exhaustive] future forms carry no v0 value.
                _ => return None,
            };
            let rendered = expr::render(template, &scope).ok()?;
            Some((key.value.clone(), rendered))
        })
        .collect()
}
