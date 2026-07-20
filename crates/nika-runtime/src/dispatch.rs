// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-verb dispatch — render the action's fields over the scope, run
//! the verb, normalize the outcome to the value model (spec 04).
//!
//! Pen-free (INV-024 stays in the settle pass): a dispatch returns
//! [`Dispatched`] · the note (`invoke · <tool>` · `exec · <argv0>` ·
//! `infer · <model>` · `agent · N turns`) + a value-or-error. Verb
//! failures carry the verb's own `nika_code()` wire form · template
//! failures inside a body carry NIKA-1702/1703 — both fail the TASK
//! (cascade) · never the run.

use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::raw::{RawAction, RawCommand};
use nika_schema::types::CaptureMode as SpecCaptureMode;
use nika_types::cost::UnpricedReason;
use nika_verb_agent::{AgentInput, AgentValue};
use nika_verb_exec::{CaptureMode, ExecInput, ExecValue};
use nika_verb_infer::{InferInput, InferValue};
use nika_verb_invoke::InvokeInput;
use serde_json::Value;

use crate::Runtime;
use crate::errors::RuntimeError;

mod permits;
mod sandbox;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

/// One dispatch's outcome — the display note + value-or-error.
/// (Agent decisions do NOT ride here: the buffer lives in the CALLER,
/// outside the timeout-cancellable region, so a timed-out attempt's
/// telemetry survives — the wiring review's F1.)
pub(crate) struct Dispatched {
    /// `<verb> · <subject>` (the display contract's `TaskStarted` note).
    pub note: String,
    /// The verb's value (spec-04 typed) or the task error.
    pub result: Result<DispatchOk, FailedDispatch>,
}

/// A failed dispatch — the task error PLUS the spend the verb had
/// already incurred before dying (billed-then-failed round-trips are
/// real money: the ledger debits it, `task_failed` carries it, the
/// `--max-cost-usd` gate sees it — Cost Intelligence follow-up).
pub(crate) struct FailedDispatch {
    pub record: TaskErrorRecord,
    /// Metered spend of the attempt that failed (absent = nothing billed).
    pub cost_usd: Option<f64>,
    /// The by-source attribution key for the ledger.
    pub cost_source: Option<String>,
    /// Why (part of) the incurred spend is NOT in `cost_usd`.
    pub cost_unpriced: Option<UnpricedReason>,
}

impl FailedDispatch {
    /// A failure that spent nothing (template errors · security refusals
    /// · verbs without provider round-trips).
    fn unspent(record: TaskErrorRecord) -> Self {
        Self {
            record,
            cost_usd: None,
            cost_source: None,
            cost_unpriced: None,
        }
    }

    /// Debit this attempt's incurred spend to the run ledger and fold
    /// it onto the task-level accumulators — returns the bare error
    /// record for the retry policy. One call per failed attempt.
    pub(crate) fn debit_and_fold(
        self,
        ledger: &crate::ledger::RunLedger,
        failed_cost: &mut Option<f64>,
        failed_unpriced: &mut Option<UnpricedReason>,
    ) -> TaskErrorRecord {
        ledger.debit(
            self.cost_source.as_deref(),
            self.cost_usd,
            self.cost_unpriced.is_some(),
        );
        if let Some(c) = self.cost_usd {
            *failed_cost = Some(failed_cost.unwrap_or(0.0) + c);
        }
        *failed_unpriced = failed_unpriced.or(self.cost_unpriced);
        self.record
    }
}

/// A successful dispatch — the output value + token spend when the
/// verb reports it (infer · agent) + an optional non-fatal WARNING
/// (OBS-E: a thinking model that ate its budget on reasoning and
/// returned a blank answer · the task still succeeded).
pub(crate) struct DispatchOk {
    pub value: Value,
    pub tokens: Option<i64>,
    pub warning: Option<String>,
    /// The child-run summary when this dispatch was an `invoke:
    /// workflow:` call (spec 14 law 8 · the trace-forest row the parent
    /// records) — `None` for every other verb. Boxed: the row is cold
    /// and must not widen every dispatch result.
    pub child: Option<Box<crate::child::ChildRunSummary>>,
    /// Real spend in USD (catalog pricing × the provider's reported
    /// usage split · plus tool-reported spend) · None for unpriced
    /// models (mock · local · unknown): absent is honest — never a
    /// fake zero.
    pub cost_usd: Option<f64>,
    /// The by-source attribution key (`provider/model` · tool id) the
    /// run ledger folds `cost_usd` under.
    pub cost_source: Option<String>,
    /// Why (part of) this leaf's spend is NOT in `cost_usd` — the
    /// honest-absence WHY channel (`cost_unpriced` on the trace frame).
    pub cost_unpriced: Option<UnpricedReason>,
}

impl DispatchOk {
    /// Fold the FAILED attempts' spend onto this success so the frame
    /// reports what the whole task cost (the ledger already debited
    /// those attempts individually — this is event-surface only).
    pub(crate) fn fold_failed_spend(
        &mut self,
        failed_cost: Option<f64>,
        failed_unpriced: Option<UnpricedReason>,
    ) {
        if failed_cost.is_some() {
            self.cost_usd = Some(self.cost_usd.unwrap_or(0.0) + failed_cost.unwrap_or(0.0));
        }
        self.cost_unpriced = self.cost_unpriced.or(failed_unpriced);
    }
}

impl Dispatched {
    fn ok(note: String, value: Value, tokens: Option<i64>) -> Self {
        Self {
            note,
            result: Ok(DispatchOk {
                value,
                tokens,
                warning: None,
                child: None,
                cost_usd: None,
                cost_source: None,
                cost_unpriced: None,
            }),
        }
    }

    /// `ok` + spend telemetry — the metered verbs (infer · agent ·
    /// invoke's tool-reported spend channel).
    fn ok_metered(
        note: String,
        value: Value,
        tokens: Option<i64>,
        warning: Option<String>,
        cost_usd: Option<f64>,
        cost_source: Option<String>,
        cost_unpriced: Option<UnpricedReason>,
    ) -> Self {
        Self {
            note,
            result: Ok(DispatchOk {
                value,
                tokens,
                warning,
                child: None,
                cost_usd,
                cost_source,
                cost_unpriced,
            }),
        }
    }

    fn verb_err(note: String, err: &dyn NikaErrorCode) -> Self {
        Self {
            note,
            result: Err(FailedDispatch::unspent(TaskErrorRecord {
                // The USER-FACING spec code (`NIKA-EXEC-001` · not the engine
                // `NIKA-440`) — the identifier the author is forced (by `nika
                // check`) to write in `on_codes:`, and the one `tasks.X.error
                // .code` exposes (spec 05 §error structure). Selective
                // recovery/retry compares against THIS (BUG-C).
                code: err.spec_code(),
                message: err.to_string(),
                transient: err.is_transient(),
            })),
        }
    }

    /// `verb_err` + the spend the failed verb had already incurred —
    /// the infer/agent error path (their typed errors carry a
    /// [`nika_types::cost::SpendOnFailure`] decorated at the verb seam).
    fn verb_err_spent(
        note: String,
        err: &dyn NikaErrorCode,
        spend: (Option<f64>, Option<String>, Option<UnpricedReason>),
    ) -> Self {
        let (cost_usd, cost_source, cost_unpriced) = spend;
        Self {
            note,
            result: Err(FailedDispatch {
                record: TaskErrorRecord {
                    code: err.spec_code(),
                    message: err.to_string(),
                    transient: err.is_transient(),
                },
                cost_usd,
                cost_source,
                cost_unpriced,
            }),
        }
    }

    pub(crate) fn template_err(note: &str, err: &RuntimeError) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord {
                // The SPEC-PLANE wire code (`NIKA-VAR-001` unresolved ·
                // `NIKA-VAR-005` out-of-subset) the author filters on — never
                // the engine-internal `nika_code()` (spec 05 §142 · the
                // `tasks.X.error.code` leak this closed). The message is
                // code-less (`wire_message`) · the code rides its own field.
                code: err.spec_code(),
                message: err.wire_message(),
                transient: false, // static expression class · retry never helps
            })),
        }
    }

    fn unwired(note: &str, detail: String) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord {
                code: nika_error::codes::NIKA_1703.to_string(),
                message: detail,
                transient: false,
            })),
        }
    }

    /// An effect refused by the declared `permits:` capability boundary
    /// (spec 01 §permits · `NIKA-SEC-004`). A security boundary is never
    /// retryable, and (for `agent:` loops) never fed back to the model.
    fn security_err(note: &str, reason: impl Into<String>) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord {
                code: "NIKA-SEC-004".to_owned(),
                message: reason.into(),
                transient: false,
            })),
        }
    }

    /// A composition refusal (spec 14 · the `NIKA-COMP` namespace + the
    /// `NIKA-SEC-003` depth backstop) — the run-side voice of the
    /// check-time findings (the skills dual-surface precedent): `nika
    /// check` refuses these BEFORE any run; this path fires for an
    /// embedder that skipped the contract, or for the depth/containment
    /// backstops a static checker cannot draw. Never transient — a
    /// composition defect is structural; retry never helps.
    pub(crate) fn comp_refusal(note: &str, code: &str, reason: String) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord {
                code: code.to_owned(),
                message: reason,
                transient: false,
            })),
        }
    }

    /// A `skills:` reference that cannot compose (spec 02 §agent skills) —
    /// the run-side voice of the check-time findings: `NIKA-AGENT-003`
    /// (text never resolved · the composer did not read it) or
    /// `NIKA-AGENT-004` (the text is not a valid Agent Skill). `nika
    /// check` refuses both BEFORE any run reaches here (check≡run); this
    /// path fires only for an embedder that skipped the composition
    /// contract — fail the TASK loudly, never inject half a context.
    fn skill_err(note: &str, code: &str, reason: String) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord {
                code: code.to_owned(),
                message: reason,
                transient: false, // a static composition defect · retry never helps
            })),
        }
    }
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn + ProviderMeta,
    D: ToolDefinitionProviderDyn,
{
    /// Dispatch one action through its verb (see module docs).
    ///
    /// `deadline` — the task-level `timeout:` budget (spec 03). The
    /// attempt loop enforces it as the TOTAL budget; it is ALSO handed
    /// to the infer path so the provider transport deadline cannot
    /// undercut it (F1 · a 30s HTTP default killed every `timeout: 7m`
    /// local-model task at 30s).
    ///
    /// `contract` — the task's parsed `returns:` (spec 09 · W3): the
    /// exec path decodes + fits against it (`NIKA-TYPE-101`); the
    /// infer/agent paths compile `lower(returns)` onto the EXISTING
    /// structured-output lane (violations stay `NIKA-INFER-002`);
    /// invoke stays `Unknown` in W3 (tool contracts land later).
    /// `child_budget` — the run ledger's remaining USD at call time
    /// (spec 14 law 6): an `invoke: workflow:` child runs under
    /// `min(this, its declared budget)`. `None` = no budget to inherit.
    pub(crate) async fn dispatch(
        &self,
        action: &RawAction,
        scope: &Scope<'_>,
        agent_buffer: &crate::agent_events::BufferingObserver,
        deadline: Option<std::time::Duration>,
        contract: Option<&crate::contract::TaskContract<'_>>,
        child_budget: Option<f64>,
    ) -> Dispatched {
        match action {
            RawAction::Invoke(inner) => {
                self.dispatch_invoke(inner, scope, (deadline, child_budget), contract)
                    .await
            }
            RawAction::Exec(inner) => self.dispatch_shell(inner, scope, contract).await,
            RawAction::Infer(inner) => self.dispatch_infer(inner, scope, deadline, contract).await,
            RawAction::Agent(inner) => {
                self.dispatch_agent(inner, scope, agent_buffer, contract)
                    .await
            }
            // #[non_exhaustive] · a future verb must land HERE loudly ·
            // the runtime refuses rather than silently no-ops.
            other => Dispatched::unwired(
                "unknown verb",
                format!("verb not wired in the runtime yet: {other:?}"),
            ),
        }
    }

    async fn dispatch_invoke(
        &self,
        action: &nika_schema::raw::RawInvokeAction,
        scope: &Scope<'_>,
        (deadline, child_budget): (Option<std::time::Duration>, Option<f64>),
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        let tool = match &action.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => t.value.clone(),
            nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                return self
                    .dispatch_workflow_call(
                        w,
                        action.args.as_ref(),
                        scope,
                        (deadline, child_budget),
                        contract,
                    )
                    .await;
            }
        };
        let note = format!("invoke · {tool}");
        // NIKA-SEC-004 BEFORE any arg rendering — the tool id is static,
        // so an out-of-boundary invoke is refused without touching the scope.
        if let Some(denial) = permits::check_tool_permits(scope.permits, &note, &tool) {
            return denial;
        }
        let args = match &action.args {
            None => Value::Object(serde_json::Map::new()),
            Some(a) => match expr::render_json(&a.value, scope) {
                Ok(v) => v,
                Err(err) => return Dispatched::template_err(&note, &err),
            },
        };
        let mut input = InvokeInput::new(tool);
        input.args = args;
        match self.invoke.run(input).await {
            // A tool's typed value (builtins · MCP `structuredContent`) flows
            // to tasks.X.output AS ITSELF — an array stays an array so
            // `for_each` / CEL navigation works (spec 04 §tasks.X.output ·
            // "string · object · or bytes · per verb"). A text-only tool
            // (no structured value) stays a String — never silently
            // JSON-coerced from text.
            Ok(out) => {
                // Move the typed value out when present; otherwise wrap the
                // text view — no clone, and `out.content` is only consumed on
                // the None arm (clippy: not a lazy-eval candidate).
                let value = match out.structured {
                    Some(value) => value,
                    None => Value::String(out.content),
                };
                // A tool that reports its REAL spend as a top-level numeric
                // `cost_usd` in its structured output is metered into the
                // run ledger (`nika:image_generate` on tick-billed
                // providers · future paid tools/MCP) — the same honest-
                // spend channel infer rides. Absent/invalid → unmetered,
                // never a guess (and never an unpriced REASON either: most
                // tools legitimately cost nothing — tagging every free
                // invoke would drown the honest signal).
                let cost_usd = value
                    .get("cost_usd")
                    .and_then(Value::as_f64)
                    .filter(|c| c.is_finite() && *c >= 0.0);
                let cost_source = cost_usd
                    .is_some()
                    .then(|| note.trim_start_matches("invoke · ").to_owned());
                Dispatched::ok_metered(note, value, None, None, cost_usd, cost_source, None)
            }
            Err(err) => Dispatched::verb_err(note, &err),
        }
    }

    /// ADR-095 Layer 6 — the OS-confinement spec from the declared
    /// boundary (the `dispatch/sandbox.rs` derivation · `None` = the
    /// unconfined floor: no `permits:` block).
    fn exec_sandbox_spec(
        &self,
        permits: Option<&nika_schema::types::Permits>,
    ) -> Option<nika_kernel::process::SandboxSpec> {
        let permits = permits?;
        let root = self
            .config
            .sandbox_root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        Some(sandbox::spec_of(permits, &root))
    }

    async fn dispatch_shell(
        &self,
        action: &nika_schema::raw::RawExecAction,
        scope: &Scope<'_>,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        let (mut input, program, is_argv) = match build_exec_input(action, scope) {
            Ok(built) => built,
            Err(refusal) => return *refusal,
        };
        // The authored `capture:` mode flows to the verb (spec 02 §exec ·
        // default `stdout`). It selects which streams come back AND the
        // one-obvious-way split: under `structured` a non-zero exit is
        // DATA (the task succeeds · `exit_code` is the branch), under the
        // text modes it fails the task — the verb owns that decision, so
        // it MUST see the mode (omitting this ran every exec in stdout
        // mode · `tasks.X.output.exit_code` was unresolvable).
        input.capture = capture_mode(action.capture.as_ref().map(|c| c.value));
        // The RAW-bytes pipeline (spec 09 §decode · W3) activates when a
        // `decode:` is declared OR a `returns:` contract types the
        // stream — never under `capture: structured` (already an
        // object). Without either, the lossy-text path is UNTOUCHED.
        let structured = input.capture == CaptureMode::Structured;
        let decode = action
            .decode
            .as_ref()
            .map_or(nika_schema::DecodeMode::Text, |d| d.value);
        input.raw_capture = !structured && (action.decode.is_some() || contract.is_some());
        let note = format!("exec · {program}");

        // cwd · env · stdin flow to the subprocess (spec 02 §exec). All
        // three may carry `${{ }}` and are rendered against the scope; the
        // parser captured them but the dispatch dropped them before this
        // (the subprocess ran in the engine cwd with the inherited env).
        if let Err(err) = render_exec_io(&mut input, action, scope) {
            return Dispatched::template_err(&note, &err);
        }

        if let Some(denial) = check_exec_permits(scope.permits, &note, &program, is_argv) {
            return denial;
        }

        // ADR-095 Layer 6 · the OS jail rides the SAME declared boundary
        // (no `permits:` block = today's unconfined floor).
        input.sandbox = self.exec_sandbox_spec(scope.permits);

        match self.shell.run(input).await {
            Ok(out) => {
                // A text mode (`stdout`/`stderr`/`combined`) yields a
                // trailing-newline-trimmed STRING (the `tasks.X.output ==
                // '42'` ergonomic). `capture: structured` yields the
                // `{ stdout, stderr, exit_code }` OBJECT verbatim — so
                // `tasks.X.output.exit_code` resolves via CEL (spec 02
                // §exec · same class as BUG#3's invoke value). The
                // structured streams are NOT trimmed (fidelity is the
                // whole point of the mode · the verb keeps them raw).
                let value = match out.output {
                    ExecValue::Text(text) => Value::String(text.trim_end().to_owned()),
                    // The decode pipeline (spec 09 §decode) — the exact
                    // captured octets become the value; a stream that
                    // does not decode settles the task failure
                    // (NIKA-1705 · inside `on_error:` scope).
                    ExecValue::Raw(bytes) => match crate::contract::decode_bytes(decode, &bytes) {
                        Ok(value) => value,
                        Err(err) => return Dispatched::template_err(&note, &err),
                    },
                    ExecValue::Structured {
                        stdout,
                        stderr,
                        exit_code,
                    } => serde_json::json!({
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": exit_code,
                    }),
                    // #[non_exhaustive] · a future value form fails loudly
                    // rather than dropping fields (the loud doctrine).
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("exec value form not wired yet: {other:?}"),
                        );
                    }
                };
                // The run-time fit (spec 09 · `Type(decoded) ⊑ returns`):
                // the DECODED value under the text modes · the
                // `{stdout, stderr, exit_code}` object under structured
                // (« a returns: on such a task types that object
                // directly »). Violation = NIKA-TYPE-101.
                if let Some(c) = contract
                    && let Err(err) = c.check_fit(&note, &value)
                {
                    return Dispatched::template_err(&note, &err);
                }
                Dispatched::ok(note, value, None)
            }
            Err(err) => Dispatched::verb_err(note, &err),
        }
    }

    async fn dispatch_infer(
        &self,
        action: &nika_schema::raw::RawInferAction,
        scope: &Scope<'_>,
        deadline: Option<std::time::Duration>,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return Dispatched::template_err("infer · ?", &err),
        };
        let mut input = InferInput::new(prompt);
        // The task `timeout:` flows to the provider transport deadline —
        // the outer attempt-loop select still enforces the total budget.
        input.timeout = deadline;
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("infer · ?", &err),
        };
        input.model = action.model.as_ref().map(|m| m.value.clone());
        #[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
        {
            input.temperature = action.temperature.as_ref().map(|t| t.value as f32);
        }
        input.max_tokens = action.max_tokens.as_ref().map(|t| t.value);
        // `returns:` compiles `lower(returns)` as the structured-output
        // contract — EXACTLY the `schema:` lane (spec 09 §returns · one
        // enforcement path · violations stay NIKA-INFER-002). The two
        // never coexist (NIKA-TYPE-003); `schema:` wins the or_else
        // only in the impossible-past-check overlap.
        input.schema = action
            .schema
            .as_ref()
            .map(|v| v.value.clone())
            .or_else(|| contract.map(crate::contract::TaskContract::lowered));
        match self.infer.run(input).await {
            Ok(out) => {
                let note = format!("infer · {}", out.model_resolved);
                let value = match out.output {
                    InferValue::Text(text) => Value::String(text),
                    // Structured output IS a JSON value (spec 04 typed
                    // dataflow — downstream templates render it
                    // canonically · for_each can fan over arrays).
                    InferValue::Structured(value) => value,
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("infer value form not wired yet: {other:?}"),
                        );
                    }
                };
                let tokens = Some(i64::try_from(out.usage.output_tokens).unwrap_or(i64::MAX));
                // OBS-E: surface the silent empty-answer footgun (a
                // reasoning model that ate its budget on thinking) as a
                // non-fatal warning · the task still succeeds.
                let warning = empty_thinking_warning(&value, &out.usage);
                // Real spend: catalog pricing × the provider's FULL usage
                // split (cache subsets at their own rates) · the SAME
                // resolver as the check-time floor (they can never disagree
                // on which row prices a model) · unpriced models emit
                // nothing PLUS the honest WHY (local · mock · uncataloged ·
                // provider silent).
                let (cost_usd, cost_unpriced) = spend_for_model(&out.model_resolved, &out.usage);
                let cost_source = Some(out.model_resolved.clone());
                Dispatched::ok_metered(
                    note,
                    value,
                    tokens,
                    warning,
                    cost_usd,
                    cost_source,
                    cost_unpriced,
                )
            }
            Err(err) => {
                let spend = price_failed_spend(err.spend());
                Dispatched::verb_err_spent("infer · ?".to_owned(), &err, spend)
            }
        }
    }

    async fn dispatch_agent(
        &self,
        action: &nika_schema::raw::RawAgentAction,
        scope: &Scope<'_>,
        agent_buffer: &crate::agent_events::BufferingObserver,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        // NIKA-SEC-004: the declared `tools:` universe must FIT
        // `permits.tools` — refused before any render or provider call,
        // one refusal for the whole task.
        if let Some(denial) = permits::check_agent_tools_permits(scope.permits, &action.tools) {
            return denial;
        }
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return Dispatched::template_err("agent · ?", &err),
        };
        let mut input = AgentInput::new(prompt);
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("agent · ?", &err),
        };
        // `skills:` — the composer-resolved Agent Skill texts join the
        // system context as ONE deterministic `## Skills` section (spec
        // 02 §agent skills · source order · name + description + body).
        if !action.skills.is_empty() {
            match self.skill_docs(action) {
                Ok(docs) => input.system = Some(system_with_skills(input.system.take(), &docs)),
                Err(refused) => return *refused,
            }
        }
        input.model = action.model.as_ref().map(|m| m.value.clone());
        input.tools = action.tools.iter().map(|t| t.value.clone()).collect();
        input.max_turns = action.max_turns.as_ref().map(|t| t.value);
        input.max_tokens_total = action.max_tokens_total.as_ref().map(|t| t.value);
        #[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
        {
            input.temperature = action.temperature.as_ref().map(|t| t.value as f32);
        }
        // `returns:` — same as infer: `lower(returns)` rides the
        // structured-output lane over the loop's FINAL message (spec 09
        // §returns · violations stay NIKA-INFER-002 · one voice).
        input.schema = action
            .schema
            .as_ref()
            .map(|v| v.value.clone())
            .or_else(|| contract.map(crate::contract::TaskContract::lowered));
        // The buffer is the CALLER's (per task-attempt-loop · still
        // per-dispatch-isolated since a wave's tasks each own one):
        // owning it here would put it inside the timeout-cancellable
        // region and lose a timed-out attempt's telemetry (review F1).
        let ran = self.agent.run_observed(input, agent_buffer).await;
        match ran {
            Ok(out) => {
                let note = format!("agent · {} turns", out.turns);
                let value = match out.output {
                    AgentValue::Text(text) => Value::String(text),
                    AgentValue::Structured(value) => value,
                    // #[non_exhaustive] · a future value form fails loudly.
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("agent value form not wired yet: {other:?}"),
                        );
                    }
                };
                let tokens = Some(i64::try_from(out.total_tokens).unwrap_or(i64::MAX));
                // BOTH spend channels: the loop's TOOL spend (exact ·
                // tool-reported — an agent-driven $0.02 render must never
                // show as $0.00) PLUS the LLM turns priced from the loop's
                // absorbed usage split via the same resolver `infer` uses
                // (the seam closed 2026-07-08). Either alone still rides;
                // an unpriced LLM leg names its reason next to whatever
                // tool spend DID meter.
                let (llm_cost, llm_unpriced) = match out.model_resolved.as_deref() {
                    Some(model) => spend_for_model(model, &out.usage),
                    // Harness-built outputs that never touched a provider.
                    None => (None, None),
                };
                let cost_usd = match (llm_cost, out.tools_cost_usd) {
                    (None, None) => None,
                    (llm, tools) => Some(llm.unwrap_or(0.0) + tools.unwrap_or(0.0)),
                };
                Dispatched::ok_metered(
                    note,
                    value,
                    tokens,
                    None,
                    cost_usd,
                    out.model_resolved.clone(),
                    llm_unpriced,
                )
            }
            Err(err) => {
                let spend = price_failed_spend(err.spend());
                Dispatched::verb_err_spent("agent · ?".to_owned(), &err, spend)
            }
        }
    }

    /// Parse the composer-resolved skill texts one agent action names
    /// (spec 02 §agent skills) — `Err` is the ready-made task refusal
    /// (BOXED · clippy `result_large_err` · unboxed at the one caller):
    /// `NIKA-AGENT-003` (text never resolved · the composition root
    /// skipped `Runtime::with_skills` — `nika check` refuses a missing
    /// FILE before any run) or `NIKA-AGENT-004` (not a valid Agent
    /// Skill). Fails BEFORE any provider call — never half a context.
    fn skill_docs(
        &self,
        action: &nika_schema::raw::RawAgentAction,
    ) -> Result<Vec<nika_schema::SkillDoc>, Box<Dispatched>> {
        let mut docs = Vec::with_capacity(action.skills.len());
        for path in &action.skills {
            let Some(raw) = self.skills.get(&path.value) else {
                return Err(Box::new(Dispatched::skill_err(
                    "agent · ?",
                    "NIKA-AGENT-003",
                    format!(
                        "skill `{}` was never resolved at compose time — the \
                         composition root must read every `skills:` file and \
                         inject it via `Runtime::with_skills` (nika check \
                         refuses a missing file before any run)",
                        path.value
                    ),
                )));
            };
            match nika_schema::parse_skill(raw) {
                Ok(doc) => docs.push(doc),
                Err(defect) => {
                    return Err(Box::new(Dispatched::skill_err(
                        "agent · ?",
                        "NIKA-AGENT-004",
                        format!(
                            "skill `{}` is not a valid Agent Skill: {defect}",
                            path.value
                        ),
                    )));
                }
            }
        }
        Ok(docs)
    }
}

/// Price the spend a FAILED verb had already incurred (decorated on
/// its error at the verb seam): the LLM leg through the same resolver
/// successes use, plus any tool-reported spend — either alone still
/// rides. `None`-everything when the failure preceded any billed call.
fn price_failed_spend(
    spend: Option<&nika_types::cost::SpendOnFailure>,
) -> (Option<f64>, Option<String>, Option<UnpricedReason>) {
    let Some(incurred) = spend else {
        return (None, None, None);
    };
    let (llm, unpriced) = match incurred.model_resolved.as_deref() {
        Some(model) if usage_has_signal(&incurred.usage) => spend_for_model(model, &incurred.usage),
        _ => (None, None),
    };
    let cost_usd = match (llm, incurred.tools_cost_usd) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    };
    (cost_usd, incurred.model_resolved.clone(), unpriced)
}

/// The ONE model-spend computation — catalog price × the FULL usage
/// split, with the honest WHY when no number can exist.
///
/// Order matters: an unpriced model class (mock · local · uncataloged)
/// outranks a silent provider — « local compute · not priced » is the
/// actionable truth for a local model even when it also reported no
/// usage. A PRICED model with a degenerate split (all meters zero —
/// e.g. a stream that never carried usage) must NOT price to $0.00:
/// the spend is real but unknowable, so it stays absent + named
/// (`provider_did_not_report_usage`) — the fake-zero gate.
fn spend_for_model(
    model: &str,
    usage: &nika_kernel::provider::TokenUsage,
) -> (Option<f64>, Option<UnpricedReason>) {
    if nika_catalog::find_pricing_for(model).is_none() {
        return (None, Some(unpriced_reason_for(model)));
    }
    if !usage_has_signal(usage) {
        return (None, Some(UnpricedReason::ProviderDidNotReportUsage));
    }
    let cache_write = usage
        .cache_write_tokens
        .unwrap_or(0)
        .saturating_add(usage.cache_creation_tokens.unwrap_or(0));
    let cost = nika_catalog::estimate_cost_usage_for(
        model,
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens.unwrap_or(0),
        cache_write,
    )
    .map(|e| e.usd);
    (cost, None)
}

/// Whether the provider reported ANY billable meter — zero-everything is
/// « did not report », never a $0.00.
fn usage_has_signal(usage: &nika_kernel::provider::TokenUsage) -> bool {
    usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.cache_read_tokens.is_some_and(|n| n > 0)
        || usage.cache_write_tokens.is_some_and(|n| n > 0)
        || usage.cache_creation_tokens.is_some_and(|n| n > 0)
}

/// Classify WHY a model string has no catalog price. The provider
/// prefix is the discriminator: `mock` = the test backend · a keyless
/// catalog provider = a local server (sovereign path — not priced,
/// never « free ») · anything else = not in the vendored catalog.
fn unpriced_reason_for(model: &str) -> UnpricedReason {
    match model.split_once('/').map(|(provider, _)| provider) {
        Some("mock") => UnpricedReason::MockProvider,
        Some(prefix) => match nika_catalog::find_provider(prefix) {
            Some(row) if !row.requires_key => UnpricedReason::LocalModel,
            _ => UnpricedReason::MissingCatalogPrice,
        },
        None => UnpricedReason::MissingCatalogPrice,
    }
}

/// Render an optional spanned string field.
fn render_opt(
    field: Option<&nika_schema::Spanned<String>>,
    scope: &Scope<'_>,
) -> Result<Option<String>, RuntimeError> {
    field.map(|f| expr::render(&f.value, scope)).transpose()
}

/// The effective system prompt of a `skills:`-carrying agent (spec 02
/// §agent skills · normative injection shape): the authored `system:`
/// (already rendered · absent = the section stands alone), then ONE
/// `## Skills` section — per skill, in `skills:` source order,
/// `### <name>` + the description + the body (trimmed). Deterministic
/// bytes: same inputs, same prompt, provider-cache-friendly.
pub(crate) fn system_with_skills(system: Option<String>, docs: &[nika_schema::SkillDoc]) -> String {
    let mut out = match system {
        Some(s) if !s.is_empty() => {
            let mut s = s;
            s.push_str("\n\n");
            s
        }
        _ => String::new(),
    };
    out.push_str("## Skills");
    for doc in docs {
        out.push_str("\n\n### ");
        out.push_str(&doc.name);
        out.push_str("\n\n");
        out.push_str(&doc.description);
        let body = doc.body.trim();
        if !body.is_empty() {
            out.push_str("\n\n");
            out.push_str(body);
        }
    }
    out
}

/// Build the [`ExecInput`] from the authored command form — each form
/// maps to its OWN variant: the argv form is rendered PER ELEMENT and
/// passed as a vector (NO join, NO shell), so an interpolated value can
/// never break out of its argv token; the shell form keeps `/bin/sh -c`
/// for genuine pipelines. Returns `(input, program, is_argv)` — a
/// refusal comes back boxed (clippy: the error path stays thin).
fn build_exec_input(
    action: &nika_schema::raw::RawExecAction,
    scope: &Scope<'_>,
) -> Result<(ExecInput, String, bool), Box<Dispatched>> {
    match &action.command {
        RawCommand::Shell(text) => match expr::render(&text.value, scope) {
            Ok(line) => {
                let program = shell_leading_program(&line);
                Ok((ExecInput::shell(line), program, false))
            }
            Err(err) => Err(Box::new(Dispatched::template_err("exec · ?", &err))),
        },
        RawCommand::Argv(parts) => {
            let rendered: Result<Vec<_>, _> = parts
                .iter()
                .map(|p| expr::render(&p.value, scope))
                .collect();
            match rendered {
                Ok(argv) => {
                    let program = argv.first().cloned().unwrap_or_else(|| "?".to_owned());
                    Ok((ExecInput::argv(argv), program, true))
                }
                Err(err) => Err(Box::new(Dispatched::template_err("exec · ?", &err))),
            }
        }
        // #[non_exhaustive] · refuse loudly · never guess a shape.
        other => Err(Box::new(Dispatched::unwired(
            "exec · ?",
            format!("command form not wired in the runtime yet: {other:?}"),
        ))),
    }
}

/// Render the exec subprocess I/O (`cwd` · `env` · `stdin`) onto `input`.
///
/// Each field may carry `${{ }}` and is resolved against the scope. `env`
/// keys AND values are both rendered (a value commonly forwards an envelope
/// var · `env: { API_BASE: "${{ env.API_BASE }}" }` per spec 02 §exec).
fn render_exec_io(
    input: &mut ExecInput,
    action: &nika_schema::raw::RawExecAction,
    scope: &Scope<'_>,
) -> Result<(), RuntimeError> {
    input.cwd = render_opt(action.cwd.as_ref(), scope)?.map(std::path::PathBuf::from);
    input.stdin = render_opt(action.stdin.as_ref(), scope)?;
    for (key, value) in &action.env {
        let k = expr::render(&key.value, scope)?;
        let v = expr::render(&value.value, scope)?;
        input.env.insert(k, v);
    }
    Ok(())
}

/// The leading program token of a shell command line (for the display note +
/// identity), skipping leading `NAME=value` env assignments — `FOO=bar git`
/// → `git`, matching the static `permits_fit::leading_program` so the note
/// names the real program (not the assignment).
fn shell_leading_program(line: &str) -> String {
    line.split_whitespace()
        .find(|token| !is_env_assignment(token))
        .unwrap_or("?")
        .to_owned()
}

/// Whether a shell token is a `NAME=value` environment assignment (a valid
/// env name before the `=`), not the program — `FOO=bar` runs `git`, not `FOO`.
fn is_env_assignment(token: &str) -> bool {
    match token.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

/// Map the authored `capture:` (spec 02 §exec · schema enum) to the verb's
/// own `CaptureMode`. Two parallel closed enums (the schema layer vs the
/// verb layer) bridged here. `None` (omitted) is the spec default
/// (`stdout`); a future `#[non_exhaustive]` schema variant also falls to
/// `stdout` (the safe spec default) — the named arms below are the wired
/// set, so when a new mode lands in BOTH enums it must be added here to
/// stop riding the default.
fn capture_mode(spec: Option<SpecCaptureMode>) -> CaptureMode {
    match spec {
        Some(SpecCaptureMode::Stderr) => CaptureMode::Stderr,
        Some(SpecCaptureMode::Combined) => CaptureMode::Combined,
        Some(SpecCaptureMode::Structured) => CaptureMode::Structured,
        // `stdout`, omitted, OR an unknown future variant → the default.
        None | Some(SpecCaptureMode::Stdout | _) => CaptureMode::Stdout,
    }
}

/// The exec capability boundary (spec 01 §permits · NIKA-SEC-004): once a
/// workflow declares `permits`, the exec sink enforces it. Returns `Some(error)`
/// when the command is refused, `None` when permitted (or no `permits` declared
/// — today's behavior, where the runner floor is the only gate; operator policy
/// is nika-policy's job, s8).
///
/// A program allowlist (`Programs`) governs `argv[0]` of the ARRAY form (the
/// unambiguous program); the SHELL form is REFUSED under an allowlist because a
/// pipeline can launch any program, so a single leading token cannot verify it
/// (use the array form). This is STRICTER than the static `nika check` for
/// shell-under-allowlist — the safe direction.
fn check_exec_permits(
    permits: Option<&nika_schema::types::Permits>,
    note: &str,
    program: &str,
    is_argv: bool,
) -> Option<Dispatched> {
    use nika_schema::types::ExecPermit;
    let permits = permits?;
    match &permits.exec {
        // Omitted or `false` → this workflow runs zero processes.
        None | Some(ExecPermit::No) => Some(Dispatched::security_err(
            note,
            "exec is not permitted by the workflow `permits` boundary",
        )),
        // `true` → any process (still blocklist-gated at the floor).
        Some(ExecPermit::Any) => None,
        // A program allowlist → ARRAY form only (argv[0] must be listed); the
        // SHELL form cannot be verified (a pipeline can launch any program), so
        // it is refused — use the array form.
        Some(ExecPermit::Programs(allowed)) => {
            if !is_argv {
                return Some(Dispatched::security_err(
                    note,
                    "a shell-string command cannot be verified against a \
                     `permits.exec` program allowlist (a pipeline can launch \
                     any program) — use the array form",
                ));
            }
            if !allowed.iter().any(|p| p == program) {
                return Some(Dispatched::security_err(
                    note,
                    format!("program {program:?} is not in the `permits.exec` allowlist"),
                ));
            }
            None
        }
        // #[non_exhaustive] · a future permit form fails CLOSED.
        Some(_) => Some(Dispatched::security_err(
            note,
            "exec permit form not understood by this engine version",
        )),
    }
}

/// OBS-E · the silent-empty-answer footgun for reasoning models.
///
/// A thinking model (gemini-2.5-flash · openai o-series · anthropic
/// extended-thinking) under a tight `max_tokens` can spend the whole
/// budget on its reasoning trace and conclude with a BLANK visible
/// answer — the task completes rc=0 with `output == ""` and every
/// downstream `${{ tasks.X.output }}` silently resolves to nothing.
///
/// The signal is the **empty visible answer paired with token spend** —
/// under either report shape:
///
/// - a REPORTED reasoning split (`thinking_tokens` · anthropic
///   extended-thinking; `reasoning_tokens` · `OpenAI` o-series; the
///   gemini wire folds `thoughtsTokenCount` INTO `output_tokens`, so its
///   heavy-think empty answer reports `output_tokens == thoughts > 0` —
///   see `wire::gemini::usage_from`), or
/// - NO split at all with `output_tokens > 0` (#410 · the ollama path
///   strips the think block upstream and reports one undifferentiated
///   count: 512 tokens consumed, 2 bytes visible — the exact silent
///   footgun, previously unguarded).
///
/// A blank answer with ZERO tokens of any kind stays silent — nothing
/// was spent, so this is a plain empty completion, not the thinking
/// footgun.
///
/// Non-fatal: returns the diagnostic STRING when the footgun is detected
/// (the caller rides it on `TaskCompleted` as a `warning` field) · the
/// task still succeeds. Pure (no I/O) so it is `--lib`-testable.
fn empty_thinking_warning(
    value: &Value,
    usage: &nika_kernel::provider::TokenUsage,
) -> Option<String> {
    if !value_is_blank(value) {
        return None;
    }
    let reasoning = usage
        .thinking_tokens
        .unwrap_or(0)
        .saturating_add(usage.reasoning_tokens.unwrap_or(0));
    if reasoning > 0 {
        return Some(format!(
            "infer produced an empty answer · max_tokens likely too low for a thinking model \
             (reasoning consumed {reasoning} tokens)"
        ));
    }
    if usage.output_tokens > 0 {
        return Some(format!(
            "infer consumed {} tokens yet the visible answer is empty — a thinking model may \
             have spent the budget inside its think block (raise max_tokens, or use a no-think \
             variant)",
            usage.output_tokens
        ));
    }
    None
}

/// A "blank" visible answer · an empty/whitespace string, JSON null, or
/// an empty container (`""`, `null`, `[]`, `{}`). Anything with content
/// (a non-empty string · a populated object/array · a number/bool) is a
/// real answer, never the footgun.
fn value_is_blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Bool(_) | Value::Number(_) => false,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;

    use nika_schema::raw::{RawCommand, RawExecAction};
    use nika_schema::{Span, Spanned};
    use nika_verb_exec::ExecInput;
    use serde_json::Value;

    use super::{Scope, render_exec_io};

    fn spanned(s: &str) -> Spanned<String> {
        Spanned::new(s.to_owned(), Span::default())
    }

    #[test]
    fn invoke_meters_a_top_level_cost_usd_from_structured_output() {
        // The honest-spend channel: a tool reporting real spend as a
        // top-level numeric `cost_usd` is metered; junk shapes never are.
        let extract = |v: serde_json::Value| {
            v.get("cost_usd")
                .and_then(Value::as_f64)
                .filter(|c| c.is_finite() && *c >= 0.0)
        };
        assert_eq!(
            extract(serde_json::json!({ "cost_usd": 0.02, "images": [] })),
            Some(0.02)
        );
        assert_eq!(extract(serde_json::json!({ "cost_usd": null })), None);
        assert_eq!(
            extract(serde_json::json!({ "cost_usd": -1.0 })),
            None,
            "negative refused"
        );
        assert_eq!(
            extract(serde_json::json!({ "cost_usd": "0.02" })),
            None,
            "strings refused"
        );
        assert_eq!(extract(serde_json::json!({ "other": 1 })), None);
        assert_eq!(extract(serde_json::json!("just text")), None);
        assert_eq!(
            extract(serde_json::json!({ "cost_usd": f64::NAN })),
            None,
            "non-finite refused"
        );
    }

    #[test]
    fn exec_cwd_env_stdin_render_onto_the_input() {
        // The Findings #3/#4 regression: the parser captured cwd/env/stdin
        // but dispatch dropped them. All three resolve `${{ }}` and land on
        // the ExecInput the verb spawns from.
        let records = BTreeMap::new();
        let vars = BTreeMap::from([
            ("dir".to_owned(), Value::String("./engine".to_owned())),
            ("base".to_owned(), Value::String("https://x".to_owned())),
            ("payload".to_owned(), Value::String("hello".to_owned())),
        ]);
        let scope = Scope::workflow(&records, &vars);

        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("printenv")));
        action.cwd = Some(spanned("${{ vars.dir }}"));
        action.env = vec![
            (spanned("API_BASE"), spanned("${{ vars.base }}")),
            (spanned("STATIC"), spanned("lit")),
        ];
        action.stdin = Some(spanned("${{ vars.payload }}"));

        let mut input = ExecInput::shell("printenv");
        render_exec_io(&mut input, &action, &scope).expect("renders");

        assert_eq!(input.cwd.as_deref(), Some(std::path::Path::new("./engine")));
        assert_eq!(input.stdin.as_deref(), Some("hello"));
        assert_eq!(
            input.env.get("API_BASE").map(String::as_str),
            Some("https://x")
        );
        assert_eq!(input.env.get("STATIC").map(String::as_str), Some("lit"));
    }

    #[test]
    fn exec_env_can_forward_envelope_env_config() {
        // The spec has two env layers: envelope `env:` config and
        // `exec.env` subprocess variables. The latter commonly forwards the
        // former (`QRCODE_AI_API_BASE: ${{ env.QRCODE_AI_API_BASE }}`); this
        // must render after a green `nika check`.
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let env = BTreeMap::from([(
            "QRCODE_AI_API_BASE".to_owned(),
            Value::String("https://odin.qrcode-ai.com".to_owned()),
        )]);
        let secrets = BTreeMap::new();
        let scope = Scope::workflow_with_env_and_secrets(&records, &vars, &env, &secrets);

        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("printenv")));
        action.env = vec![(
            spanned("QRCODE_AI_API_BASE"),
            spanned("${{ env.QRCODE_AI_API_BASE }}"),
        )];

        let mut input = ExecInput::shell("printenv");
        render_exec_io(&mut input, &action, &scope).expect("renders");
        assert_eq!(
            input.env.get("QRCODE_AI_API_BASE").map(String::as_str),
            Some("https://odin.qrcode-ai.com")
        );
    }

    #[test]
    fn absent_exec_io_leaves_the_input_at_its_defaults() {
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let action = RawExecAction::with_command(RawCommand::Shell(spanned("true")));
        let mut input = ExecInput::shell("true");
        render_exec_io(&mut input, &action, &scope).expect("renders");
        assert!(input.cwd.is_none(), "no cwd → inherited");
        assert!(input.stdin.is_none(), "no stdin");
        assert!(input.env.is_empty(), "no env → inherited only");
    }

    #[test]
    fn a_bad_template_in_exec_io_is_a_loud_error() {
        // An unresolvable reference in cwd/env/stdin must surface, not be
        // swallowed (the loud doctrine · same class as a bad command).
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let scope = Scope::workflow(&records, &vars);
        let mut action = RawExecAction::with_command(RawCommand::Shell(spanned("true")));
        action.cwd = Some(spanned("${{ vars.nope }}"));
        let mut input = ExecInput::shell("true");
        assert!(render_exec_io(&mut input, &action, &scope).is_err());
    }

    // ── OBS-E · the reasoning-model blank-answer footgun ────────────────

    use nika_kernel::provider::TokenUsage;

    use super::{empty_thinking_warning, value_is_blank};

    fn usage_thinking(output: u64, thinking: u64) -> TokenUsage {
        let mut u = TokenUsage::new(7, output);
        u.thinking_tokens = Some(thinking);
        u
    }

    #[test]
    fn obs_e_fires_on_empty_answer_with_thinking_spend() {
        // The task's footgun: a thinking model that ate its budget on
        // reasoning and concluded with a blank visible answer.
        let usage = usage_thinking(0, 84);
        let warn = empty_thinking_warning(&Value::String(String::new()), &usage)
            .expect("empty answer + thinking → warning");
        assert!(warn.contains("empty answer"), "names the footgun: {warn}");
        assert!(warn.contains("84"), "reports the reasoning spend: {warn}");
        assert!(warn.contains("max_tokens"), "names the likely fix: {warn}");
    }

    #[test]
    fn obs_e_silent_on_a_real_answer_without_thinking() {
        // The normal path · {output:50, thinking:0} with content → no warn.
        let usage = usage_thinking(50, 0);
        assert!(
            empty_thinking_warning(&Value::String("Paris".to_owned()), &usage).is_none(),
            "a real answer is never the footgun"
        );
    }

    #[test]
    fn obs_e_keys_off_the_empty_visible_answer_not_output_tokens() {
        // The gemini wire folds `thoughtsTokenCount` INTO `output_tokens`
        // (usage_from), so a heavy-think empty answer reports
        // output_tokens == thoughts > 0 · keying off output_tokens would
        // MISS it. The blank visible answer is the real signal.
        let usage = usage_thinking(84, 84); // gemini-shaped: candidates(0)+thoughts(84)
        assert!(
            empty_thinking_warning(&Value::String("   ".to_owned()), &usage).is_some(),
            "whitespace-only answer + thinking fires (the gemini reality)"
        );
    }

    #[test]
    fn obs_e_reasoning_tokens_count_like_thinking_tokens() {
        // OpenAI o-series reports the reasoning budget under
        // `reasoning_tokens` · the footgun is provider-agnostic.
        let mut usage = TokenUsage::new(7, 0);
        usage.reasoning_tokens = Some(40);
        assert!(
            empty_thinking_warning(&Value::Null, &usage).is_some(),
            "reasoning_tokens triggers the same diagnostic"
        );
    }

    #[test]
    fn obs_e_silent_when_nothing_was_spent() {
        // A blank answer with ZERO tokens of any kind is a plain empty
        // completion, not the thinking footgun — silence (no false alarm).
        let usage = TokenUsage::new(7, 0); // no output, no thinking, no reasoning
        assert!(empty_thinking_warning(&Value::String(String::new()), &usage).is_none());
        assert!(empty_thinking_warning(&Value::Null, &usage).is_none());
    }

    #[test]
    fn obs_e_fires_on_the_unreported_split_shape() {
        // #410 · the ollama path: the think block is stripped upstream and
        // the wire reports ONE undifferentiated count — 512 tokens
        // consumed, a blank visible answer, no thinking/reasoning split.
        // The run used to stay green and silent; the warning now names it.
        let usage = TokenUsage::new(7, 512); // output only — no split fields
        let warn = empty_thinking_warning(&Value::String(String::new()), &usage)
            .expect("blank answer + undifferentiated spend → warning");
        assert!(warn.contains("512"), "reports the spend: {warn}");
        assert!(warn.contains("max_tokens"), "names the likely fix: {warn}");
        assert!(warn.contains("no-think"), "names the alternative: {warn}");
        // A real answer with the same usage shape stays silent.
        assert!(
            empty_thinking_warning(&Value::String("Paris".to_owned()), &usage).is_none(),
            "content is never the footgun"
        );
    }

    #[test]
    fn value_is_blank_classifies_empty_forms() {
        assert!(value_is_blank(&Value::Null));
        assert!(value_is_blank(&Value::String(String::new())));
        assert!(value_is_blank(&Value::String("  \n ".to_owned())));
        assert!(value_is_blank(&serde_json::json!([])));
        assert!(value_is_blank(&serde_json::json!({})));
        // Content of any kind is NOT blank.
        assert!(!value_is_blank(&Value::String("x".to_owned())));
        assert!(!value_is_blank(&serde_json::json!({ "k": 1 })));
        assert!(!value_is_blank(&serde_json::json!([1])));
        assert!(!value_is_blank(&Value::Bool(false)));
        assert!(!value_is_blank(&serde_json::json!(0)));
    }
}

/// F1 (field report 2026-07-04) — the task `timeout:` must arrive on the
/// provider HTTP request through the REAL parse → check → run chain: the
/// 30s transport default killed every `timeout: "7m"` local-model task
/// with a 408 at 30s. Asserted at the http seam (a capturing effect
/// under a real `ProviderRegistry` · `ollama` profile · zero network).
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod infer_deadline_tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use bytes::Bytes;
    use nika_kernel::http::{
        HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse,
    };
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_invoke::InvokeVerb;

    use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

    /// Captures every provider request · answers a minimal
    /// openai-compat success so the run settles green.
    #[derive(Default)]
    struct CapturingHttp {
        captured: Mutex<Vec<HttpRequest>>,
    }

    impl CapturingHttp {
        fn captured(&self) -> Vec<HttpRequest> {
            self.captured.lock().expect("test mutex").clone()
        }
    }

    const OPENAI_OK: &str = r#"{"id":"cc","model":"m",
        "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1}}"#;

    impl HttpPostDyn for CapturingHttp {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.captured
                .lock()
                .expect("test mutex")
                .push(request.clone());
            Ok(HttpResponse::new(
                200,
                BTreeMap::new(),
                Bytes::from_static(OPENAI_OK.as_bytes()),
                request.url,
            ))
        }

        async fn send_streaming(
            &self,
            _request: HttpRequest,
        ) -> Result<HttpStreamResponse, HttpError> {
            Err(HttpError::Unsupported {
                reason: "streaming not exercised here".to_owned(),
            })
        }
    }

    async fn run_and_capture(yaml: &str) -> Vec<HttpRequest> {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        assert!(report.is_clean(), "fixture passes the ladder");

        let http = Arc::new(CapturingHttp::default());
        let registry = Arc::new(ProviderRegistry::new(
            Arc::clone(&http),
            ProvidersConfig::new(),
        ));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            nika_verb_infer::InferVerb::new(registry, "ollama/llama3.2"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        assert!(outcome.ok, "the canned success settles green");
        http.captured()
    }

    #[tokio::test]
    async fn task_timeout_governs_the_provider_http_deadline() {
        // The exact field repro: `timeout: "7m"` on a local-model infer.
        let captured = run_and_capture(
            "nika: v1\nworkflow:\n  id: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    timeout: \"7m\"\n    infer: { prompt: \"hello\" }\n",
        )
        .await;
        assert_eq!(captured.len(), 1, "one provider round-trip");
        assert_eq!(
            captured[0].timeout,
            Some(Duration::from_secs(420)),
            "the task budget rides the provider HTTP request"
        );
    }

    #[tokio::test]
    async fn local_provider_without_task_timeout_gets_the_generous_default() {
        let captured = run_and_capture(
            "nika: v1\nworkflow:\n  id: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    infer: { prompt: \"hello\" }\n",
        )
        .await;
        assert_eq!(captured.len(), 1, "one provider round-trip");
        // 300s — nika-providers' LOCAL_DEFAULT_TIMEOUT (pub(crate) there ·
        // the ≥300s F1 acceptance floor pinned at the consumer seam).
        assert_eq!(
            captured[0].timeout,
            Some(Duration::from_secs(300)),
            "a local provider defaults to minutes, never the 30s cloud default"
        );
    }
}
