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
use nika_schema::raw::{RawAction, RawCommand, VisionInput};
use nika_types::cost::UnpricedReason;
use nika_verb_agent::AgentInput;
use nika_verb_exec::{CaptureMode, ExecCommand, ExecValue};
use nika_verb_infer::{InferInput, VisionPart};
use nika_verb_invoke::InvokeInput;
use serde_json::Value;

use crate::Runtime;
use crate::errors::RuntimeError;

pub(crate) mod commit;
mod exec_io;
mod permits;
mod regate;
mod spend;
mod verb_outcome;
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;
use exec_io::{build_exec_input, capture_mode, render_exec_io};
use spend::{failed_usage_split, price_failed_spend, spend_for_model};

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
    /// The resolved call forbids replay even when `on_codes` matches.
    pub retry_forbidden: bool,
    /// Metered spend of the attempt that failed (absent = nothing billed).
    pub cost_usd: Option<f64>,
    /// The by-source attribution key for the ledger.
    pub cost_source: Option<String>,
    /// Why (part of) the incurred spend is NOT in `cost_usd`.
    pub cost_unpriced: Option<UnpricedReason>,
    /// F-P6 · the commit gate's binding evidence — `Fired` when the
    /// failure fired AFTER a passed gate (the judged bytes DID fire) ·
    /// `Refused` when the gate refused the fire (the finding). One field,
    /// so the two states can never both ride.
    pub evidence: Option<commit::CommitEvidence>,
    /// The admitted lane the failed dispatch rode (One Door · wave 2b) —
    /// the terminal frame names the path that FAILED, not `?`.
    pub access: Option<Box<nika_types::access::AccessPlan>>,
    /// Q01 · the usage split of a BILLED-then-failed round-trip — the
    /// receipt rides `task_failed` beside the spend it explains. `None`
    /// when the attempt never reached a provider. Boxed: cold.
    pub usage: Option<Box<crate::usage::UsageSplit>>,
}

impl FailedDispatch {
    /// A failure that spent nothing (template errors · security refusals
    /// · verbs without provider round-trips).
    fn unspent(record: TaskErrorRecord) -> Self {
        Self {
            record,
            retry_forbidden: false,
            cost_usd: None,
            cost_source: None,
            cost_unpriced: None,
            evidence: None,
            access: None,
            usage: None,
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
/// (the success-riding diagnostic channel — no current producer: the
/// OBS-E empty-answer class was promoted to the typed NIKA-INFER-004
/// failure at the verb · #651).
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
    /// F-P6 · the binding evidence of a FIRED exec/invoke step (the
    /// judged digest ≡ the fired digest) — `None` for the un-gated verbs
    /// (infer · agent) and for a `workflow:` call (the child's own trace
    /// carries its steps' attestations). Boxed: cold evidence, never
    /// widens the channel (the `child` precedent).
    pub commit: Option<Box<commit::CommitAttestation>>,
    /// One Door · wave 1: the admitted lane this dispatch actually
    /// rode (the frozen plan's `AccessPlan` for the task's model) —
    /// the task terminal stamps `access` · `billing` from it, never
    /// from a provider-prefix guess. `None` = no plan attached (a bare
    /// embedder · a templated model judged at dispatch). Boxed: cold.
    pub access: Option<Box<nika_types::access::AccessPlan>>,
    /// Q01 · the provider-reported usage SPLIT that priced `cost_usd`
    /// (input · cached input · cache writes · output · reasoning) plus
    /// the responder's own identity — the receipt a reader needs to
    /// recompute the number from the pinned catalog. `None` on the verbs
    /// that meter nothing. Boxed: cold, never widens the channel.
    pub usage: Option<Box<crate::usage::UsageSplit>>,
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
                commit: None,
                access: None,
                usage: None,
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
                commit: None,
                access: None,
                usage: None,
            }),
        }
    }

    /// Q01 · stamp the metered call's usage split (and the responder's
    /// identity) on a success — a failure keeps its own copy.
    fn with_usage(mut self, usage: Option<Box<crate::usage::UsageSplit>>) -> Self {
        if let Ok(ok) = &mut self.result {
            ok.usage = usage;
        }
        self
    }

    /// Stamp the admitted lane on a success (the plan's `AccessPlan`
    /// for the model that served) — a failure keeps its record.
    fn with_access(mut self, access: Option<nika_types::access::AccessPlan>) -> Self {
        if let Ok(ok) = &mut self.result {
            ok.access = access.map(Box::new);
        }
        self
    }

    fn verb_err(note: String, err: &dyn NikaErrorCode) -> Self {
        Self {
            note,
            result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
                // The USER-FACING spec code (`NIKA-EXEC-001` · not the engine
                // `NIKA-440`) — the identifier the author is forced (by `nika
                // check`) to write in `on_codes:`, and the one `tasks.X.error
                // .code` exposes (spec 05 §error structure). Selective
                // recovery/retry compares against THIS (BUG-C).
                err.spec_code(),
                err.to_string(),
                err.is_transient(),
            ))),
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
                record: TaskErrorRecord::new(err.spec_code(), err.to_string(), err.is_transient()),
                retry_forbidden: false,
                cost_usd,
                cost_source,
                cost_unpriced,
                evidence: None,
                access: None,
                usage: None,
            }),
        }
    }

    /// Carry a resolved replay veto without changing the error or its evidence.
    fn with_retry_forbidden(mut self, retry_forbidden: bool) -> Self {
        if let Err(failed) = &mut self.result {
            failed.retry_forbidden = retry_forbidden;
        }
        self
    }

    /// Q01 · the usage split of a billed-then-failed attempt — the
    /// receipt rides `task_failed` beside the spend it explains.
    fn with_failed_usage(mut self, usage: Option<Box<crate::usage::UsageSplit>>) -> Self {
        if let Err(failed) = &mut self.result {
            failed.usage = usage;
        }
        self
    }

    /// The lane a FAILED dispatch rode (wave 2b): the terminal frame
    /// stamps the path that failed instead of `?`.
    fn with_failed_access(mut self, access: Option<nika_types::access::AccessPlan>) -> Self {
        if let Err(failed) = &mut self.result {
            failed.access = access.map(Box::new);
        }
        self
    }

    pub(crate) fn template_err(note: &str, err: &RuntimeError) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
                // The SPEC-PLANE wire code (`NIKA-VAR-001` unresolved ·
                // `NIKA-VAR-005` out-of-subset) the author filters on — never
                // the engine-internal `nika_code()` (spec 05 §142 · the
                // `tasks.X.error.code` leak this closed). The message is
                // code-less (`wire_message`) · the code rides its own field.
                err.spec_code(),
                err.wire_message(),
                false, // static expression class · retry never helps
            ))),
        }
    }

    fn unwired(note: &str, detail: String) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
                nika_error::codes::NIKA_1703.to_string(),
                detail,
                false,
            ))),
        }
    }

    /// An effect refused by the declared `permits:` capability boundary
    /// (spec 01 §permits · `NIKA-SEC-004`). A security boundary is never
    /// retryable, and (for `agent:` loops) never fed back to the model.
    fn security_err(note: &str, reason: impl Into<String>) -> Self {
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
                "NIKA-SEC-004",
                reason.into(),
                false,
            ))),
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
            result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
                code, reason, false,
            ))),
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
            result: Err(FailedDispatch::unspent(TaskErrorRecord::new(
                code, reason, false, // a static composition defect · retry never helps
            ))),
        }
    }
}

/// Settle a successful exec output into the task value (spec 02 · 09 ·
/// the fit) — split for the fn ratchet · semantics unchanged.
fn settle_exec_out(
    note: &str,
    out: nika_verb_exec::ExecOutput,
    decode: nika_schema::DecodeMode,
    contract: Option<&crate::contract::TaskContract<'_>>,
) -> Dispatched {
    // Text modes trim to a STRING · structured yields the
    // `{stdout, stderr, exit_code}` object raw (spec 02 §exec).
    let value = match out.output {
        ExecValue::Text(text) => Value::String(text.trim_end().to_owned()),
        // The decode pipeline (spec 09 §decode) — the exact captured
        // octets become the value; a stream that does not decode settles
        // the task failure (NIKA-1705 · inside `on_error:` scope).
        ExecValue::Raw(bytes) => match crate::contract::decode_bytes(decode, &bytes) {
            Ok(value) => value,
            Err(err) => return Dispatched::template_err(note, &err),
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
        // #[non_exhaustive] · a future value form fails loudly rather
        // than dropping fields (the loud doctrine).
        other => {
            return Dispatched::unwired(note, format!("exec value form not wired yet: {other:?}"));
        }
    };
    // The run-time fit (spec 09 · `Type(decoded) ⊑ returns`): the
    // DECODED value under the text modes · the `{stdout, stderr,
    // exit_code}` object under structured (« a returns: on such a task
    // types that object directly »). Violation = NIKA-TYPE-101.
    if let Some(c) = contract
        && let Err(err) = c.check_fit(note, &value)
    {
        return Dispatched::template_err(note, &err);
    }
    Dispatched::ok(note.to_owned(), value, None)
}

/// `temperature:` lowering — f64 grammar to the provider's f32 seat.
#[allow(clippy::cast_possible_truncation)] // 0-2 range · checker-validated
fn temp_f32(t: Option<&nika_schema::Spanned<f64>>) -> Option<f32> {
    t.map(|t| t.value as f32)
}

/// The per-attempt task context the dispatch threads to its arms — the
/// task-level knobs that are not the action itself (bundled at the
/// 8-argument clippy wall · an additive knob lands HERE, never as a new
/// parameter).
pub(crate) struct DispatchCtx<'a> {
    /// The task's ONE `timeout:` (03-dag) — enforced at dispatch too.
    pub deadline: Option<std::time::Duration>,
    /// Law 6 · the ledger's remaining budget AT CALL TIME (a child
    /// workflow's ceiling).
    pub child_budget: Option<f64>,
    /// NEP-0006 · the task's declared `inert:` door (the data-as-code
    /// sink's run twin honors it).
    pub inert: Option<&'a str>,
    /// NEP-0007 · the attempt's permit-decision collector (spec 17 §the
    /// permit witness) — every dispatch-boundary decision records here,
    /// the settle spine emits one `permit_checked` frame per entry.
    pub witness: &'a crate::witness::PermitWitness,
    /// P3 B5 · the operator's bound `--answer` for THIS task (the
    /// harness gate's human verdict on a resumed run).
    pub gate_answer: Option<serde_json::Value>,
    /// The run's immutable opening instant, shared by every retry/fan-out.
    pub run_start: nika_kernel::tool_executor::ToolRunStart,
}

impl<'a> DispatchCtx<'a> {
    /// The per-attempt context of a task (invariant #19 · the type owns
    /// its constructor): the ONE `timeout:`, the ledger's remaining
    /// budget AT CALL TIME (law 6 · computed by the caller each
    /// attempt), and the NEP-0006 `inert:` door.
    pub(crate) fn of_task(
        task: &'a nika_schema::raw::RawTask,
        deadline: Option<std::time::Duration>,
        child_budget: Option<f64>,
        witness: &'a crate::witness::PermitWitness,
    ) -> Self {
        Self {
            deadline,
            child_budget,
            inert: task.data_as_code_because().map(|s| s.value.as_str()),
            witness,
            gate_answer: None,
            run_start: nika_kernel::tool_executor::ToolRunStart::new(0),
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
    ///
    /// `taint` — the F-O1 PR-2 per-template oracle: the exec/mcp re-gates
    /// label each RAW template against it and match the RENDERED value
    /// against the step's permit (NEP-0004 law 2 · `dispatch/regate.rs`).
    pub(crate) async fn dispatch(
        &self,
        action: &RawAction,
        scope: &Scope<'_>,
        taint: &crate::integrity::ValueTaint<'_>,
        agent_buffer: &crate::agent_events::BufferingObserver,
        ctx: DispatchCtx<'_>,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        match action {
            RawAction::Invoke(inner) => {
                self.dispatch_invoke(inner, scope, taint, &ctx, contract)
                    .await
            }
            RawAction::Exec(inner) => {
                self.dispatch_shell(inner, scope, taint, &ctx, contract)
                    .await
            }
            RawAction::Infer(inner) => {
                self.dispatch_infer(inner, scope, ctx.deadline, contract)
                    .await
            }
            RawAction::Agent(inner) => {
                self.dispatch_agent(inner, scope, agent_buffer, &ctx, contract)
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
        taint: &crate::integrity::ValueTaint<'_>,
        ctx: &DispatchCtx<'_>,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        let (deadline, child_budget, inert) = (ctx.deadline, ctx.child_budget, ctx.inert);
        let witness = ctx.witness;
        let tool = match &action.target {
            nika_schema::raw::RawInvokeTarget::Tool(t) => t.value.clone(),
            nika_schema::raw::RawInvokeTarget::Workflow(w) => {
                let args = action.args.as_ref();
                return self
                    .dispatch_workflow_call(w, args, scope, (deadline, child_budget), contract)
                    .await;
            }
        };
        let note = format!("invoke · {tool}");
        // NIKA-SEC-004 BEFORE any arg rendering — the tool id is static,
        // so an out-of-boundary invoke is refused without touching the scope.
        let raw_args = action.args.as_ref().map(|s| &s.value);
        if let Some(denial) =
            permits::check_tool_permits(scope.permits(), &note, &tool, raw_args, witness)
        {
            return denial;
        }
        let args = match &action.args {
            None => Value::Object(serde_json::Map::new()),
            Some(a) => match expr::render_json(&a.value, scope) {
                Ok(v) => v,
                Err(err) => return Dispatched::template_err(&note, &RuntimeError::from(err)),
            },
        };
        // NEP-0006 law 3 · the data-as-code sink's run twin: the RESOLVED
        // fetch URL is classified against the one closed list, honoring
        // the task's declared inert: door — the dynamic case the static
        // classifier deferred (dispatch/permits.rs · check_fetch_sink).
        if let Some(denial) = permits::check_fetch_sink(inert, &note, &tool, &args, witness) {
            return denial;
        }
        // F-O1 PR-2 · the mcp border re-gate (NEP-0004 law 2): the grant
        // of the tool IS the boundary, and a tainted path/host in its args
        // slipped through — the resolved value is canonicalized then
        // matched against the step's fs/net permit. First-party builtins
        // are already re-gated at their own boundary (`boundary.enforce` ·
        // the one-hop net enforce) — never duplicated here.
        if tool.starts_with("mcp:")
            && let (Some(boundary), Some(raw)) = (scope.permits(), &action.args)
            && let Some(denial) = regate::regate_mcp_args(
                boundary,
                &note,
                &tool,
                &raw.value,
                &args,
                taint,
                scope.records(),
                witness,
            )
        {
            return denial;
        }
        let mut input = InvokeInput::new(tool);
        input.args = args;
        // F-P6 · the gated firing lane (PREVIEW → tamper seam → COMMIT
        // gate → run · `dispatch/commit.rs` owns the binding).
        self.run_invoke_gated(note, input, ctx.run_start).await
    }

    /// ADR-095 Layer 6 — derive the OS jail from the declared boundary. F-O8
    /// maps absent permits to empty fs + denied net, under `check_exec_permits`.
    /// Since NEP-0009 (LAW-AUTH-0330), an effective path escape refuses before
    /// spawn as `NIKA-SEC-004`; its witness records `fs.path_mismatch`, the
    /// judged prefix, and the resolved target. Judged = mounted, never rewritten.
    fn exec_sandbox_spec(
        &self,
        permits: Option<&nika_schema::types::Permits>,
        note: &str,
        witness: &crate::witness::PermitWitness,
    ) -> Result<nika_kernel::process::SandboxSpec, Box<Dispatched>> {
        let zero = nika_schema::types::Permits::new();
        let permits = permits.unwrap_or(&zero);
        let root = self
            .config
            .sandbox_root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        #[allow(clippy::disallowed_methods)] // dispatch-owned absolute HOME for issue 1025
        let home = std::env::var("HOME").ok().filter(|h| h.starts_with('/'));
        match nika_exec_runner::sandbox_spec::spec_of_with_home(permits, &root, home.as_deref()) {
            Ok(spec) => {
                for grant in spec.fs_read.iter().chain(spec.fs_write.iter()) {
                    witness.record(
                        "fs",
                        grant,
                        "allow",
                        "the effective path identity stays in the declared set (NEP-0009)",
                    );
                }
                Ok(spec)
            }
            Err(mismatch) => {
                witness.record(
                    "fs",
                    &mismatch.grant,
                    "deny",
                    format!(
                        "fs.path_mismatch · resolves to `{}` · outside the declared {} set (NEP-0009)",
                        mismatch.resolved, mismatch.access
                    ),
                );
                Err(Box::new(Dispatched::security_err(
                    note,
                    format!(
                        "fs.{} grant `{}` names another identity: it resolves to `{}`, \
                         outside the declared boundary — a path grant names an effective \
                         path identity (NEP-0009) · declare the effective path; the grant \
                         is refused, never rewritten",
                        mismatch.access, mismatch.grant, mismatch.resolved
                    ),
                )))
            }
        }
    }

    async fn dispatch_shell(
        &self,
        action: &nika_schema::raw::RawExecAction,
        scope: &Scope<'_>,
        taint: &crate::integrity::ValueTaint<'_>,
        ctx: &DispatchCtx<'_>,
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
        // (the subprocess ran in the engine cwd with a floor-only env).
        if let Err(err) = render_exec_io(&mut input, action, scope) {
            return Dispatched::template_err(&note, &err);
        }

        if let Some(denial) =
            permits::check_exec_permits(scope.permits(), &note, &program, is_argv, ctx.witness)
        {
            return denial;
        }

        // F-O1 PR-2 · the argv/cwd re-gate (NEP-0004 law 2): an untrusted
        // value reaching the PERMITTED verb's argument is matched against
        // the step's permit on its RESOLVED, canonical form — argv[1..]
        // (option-injection · traversal · host), then cwd. `argv[0]` is
        // the program, already matched on its resolved value above; the
        // shell form has no per-token canonical form (the OS jail owns
        // its fs — `dispatch/regate.rs` module docs).
        if let Some(boundary) = scope.permits() {
            if let (RawCommand::Argv(elements), ExecCommand::Argv(argv)) =
                (&action.command, &input.command)
                && let Some(denial) = regate::regate_exec_argv(
                    boundary,
                    &note,
                    elements,
                    argv,
                    taint,
                    scope.records(),
                    ctx.witness,
                )
            {
                return denial;
            }
            if let (Some(template), Some(cwd)) = (&action.cwd, &input.cwd)
                && let Some(denial) = regate::regate_exec_cwd(
                    boundary,
                    &note,
                    &template.value,
                    &cwd.to_string_lossy(),
                    taint,
                    scope.records(),
                    ctx.witness,
                )
            {
                return denial;
            }
        }

        // F-P6 · the gated firing lane (PREVIEW → jail/passthrough
        // derivation → tamper seam → COMMIT gate → run ·
        // `dispatch/commit.rs` owns the binding).
        self.run_exec_gated(note, input, scope.permits(), ctx.witness, decode, contract)
            .await
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
            Err(err) => return Dispatched::template_err("infer · ?", &RuntimeError::from(err)),
        };
        let mut input = InferInput::new(prompt);
        // The task `timeout:` flows to the provider transport deadline —
        // the outer attempt-loop select still enforces the total budget.
        input.timeout = deadline;
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("infer · ?", &err),
        };
        // `model:` renders through the SAME `${{ }}` seam as
        // prompt/system (#824 · check⇄run parity).
        input.model = match render_opt(action.model.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("infer · ?", &err),
        };
        input.temperature = temp_f32(action.temperature.as_ref());
        input.max_tokens = action.max_tokens.as_ref().map(|t| t.value);
        input.schema = task_schema(action.schema.as_ref(), contract);
        if action.thinking.as_ref().is_some_and(|t| t.value.enabled) {
            input.thinking_budget = action.thinking.as_ref().and_then(|t| t.value.budget_tokens);
        }
        match collect_vision(&action.vision, scope) {
            Ok(v) => input.vision = v,
            Err(VisionErr::Template(err)) => return Dispatched::template_err("infer · ?", &err),
            Err(VisionErr::Unwired(detail)) => return Dispatched::unwired("infer · ?", detail),
        }
        // One Door · wave 1: the lane decides the adapter — the frozen
        // plan's seat for THIS task's model (a pinned seat serves every
        // model), never a run-wide « a seat exists » flag.
        let lane_model = input
            .model
            .clone()
            .unwrap_or_else(|| self.infer.default_model().to_owned());
        let access = self.lane_plan(&lane_model);
        #[cfg(feature = "access-harness")]
        if let Some(seat_id) = self.seat_for(&lane_model) {
            return match self.infer.run_on_harness(seat_id, input).await {
                Ok(out) => verb_outcome::harness_infer_success(seat_id, out, access),
                Err(err) => Dispatched::verb_err_spent(
                    format!("infer · seat {seat_id}"),
                    &err,
                    (None, None, None),
                )
                .with_failed_access(access),
            };
        }
        match self.infer.run(input).await {
            Ok(out) => verb_outcome::infer_success(out, access),
            Err(err) => {
                let split = failed_usage_split(err.spend());
                let spend = price_failed_spend(err.spend());
                // The note names the model the lane was asked for (the
                // W1 gauntlet read `infer · ?` in a sealed trace).
                Dispatched::verb_err_spent(format!("infer · {lane_model}"), &err, spend)
                    .with_failed_usage(split)
                    .with_failed_access(access)
            }
        }
    }

    async fn dispatch_agent(
        &self,
        action: &nika_schema::raw::RawAgentAction,
        scope: &Scope<'_>,
        agent_buffer: &crate::agent_events::BufferingObserver,
        ctx: &DispatchCtx<'_>,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        // NIKA-SEC-004: the declared `tools:` universe must FIT
        // `permits.tools` — refused before any render or provider call,
        // one refusal for the whole task.
        if let Some(denial) =
            permits::check_agent_tools_permits(scope.permits(), &action.tools, ctx.witness)
        {
            return denial;
        }
        let prompt = match expr::render(&action.prompt.value, scope) {
            Ok(p) => p,
            Err(err) => return Dispatched::template_err("agent · ?", &RuntimeError::from(err)),
        };
        let mut input = AgentInput::new(prompt);
        input.system = match render_opt(action.system.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("agent · ?", &err),
        };
        // `skills:` — the composer-resolved Agent Skill texts join the
        // system context as ONE `## Skills` section (spec 02 §agent skills).
        if !action.skills.is_empty() {
            match self.skill_docs(action) {
                Ok(docs) => input.system = Some(system_with_skills(input.system.take(), &docs)),
                Err(refused) => return *refused,
            }
        }
        // `model:` — the SAME render seam as infer's (#824 · one law).
        input.model = match render_opt(action.model.as_ref(), scope) {
            Ok(v) => v,
            Err(err) => return Dispatched::template_err("agent · ?", &err),
        };
        input.tools = action.tools.iter().map(|t| t.value.clone()).collect();
        // One Door · wave 1: the lane decides — an agent whose model's
        // lane is a provider path runs the native loop even when a seat
        // is attached for another lane (the plan, never the seat's mere
        // presence, routes the task).
        let lane_model = input
            .model
            .clone()
            .unwrap_or_else(|| self.agent.default_model().to_owned());
        let access = self.lane_plan(&lane_model);
        input.native_only = self.access_plan.is_some() && self.seat_for(&lane_model).is_none();
        Self::bridge_inputs(&mut input, scope, ctx);
        input.max_turns = action.max_turns.as_ref().map(|t| t.value);
        input.max_tokens_total = action.max_tokens_total.as_ref().map(|t| t.value);
        input.temperature = temp_f32(action.temperature.as_ref());
        input.schema = task_schema(action.schema.as_ref(), contract);
        // The buffer is the CALLER's (per task-attempt-loop · still
        // per-dispatch-isolated since a wave's tasks each own one):
        // owning it here would put it inside the timeout-cancellable
        // region and lose a timed-out attempt's telemetry (review F1).
        let ran = self
            .agent
            .run_observed_at(input, agent_buffer, ctx.run_start)
            .await;
        match ran {
            Ok(out) => verb_outcome::agent_success(out, access),
            Err(err) => {
                let split = failed_usage_split(err.spend());
                let spend = price_failed_spend(err.spend());
                Dispatched::verb_err_spent(format!("agent · {lane_model}"), &err, spend)
                    .with_failed_usage(split)
                    .with_failed_access(access)
            }
        }
    }

    /// P3 B5 · the bridge's inputs: the workflow's declared boundary
    /// (judged in-verb for a harness) + the operator's bound `--answer`.
    fn bridge_inputs(input: &mut AgentInput, scope: &Scope<'_>, ctx: &DispatchCtx<'_>) {
        input.permits = scope.permits().cloned();
        input.gate_answer.clone_from(&ctx.gate_answer);
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
fn collect_vision(
    items: &[nika_schema::Spanned<VisionInput>],
    scope: &Scope<'_>,
) -> Result<Vec<VisionPart>, VisionErr> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(match &item.value {
            VisionInput::File { path } => VisionPart::file(
                expr::render(&path.value, scope)
                    .map_err(RuntimeError::from)
                    .map_err(VisionErr::Template)?,
            ),
            VisionInput::Url { url } => VisionPart::url(
                expr::render(&url.value, scope)
                    .map_err(RuntimeError::from)
                    .map_err(VisionErr::Template)?,
            ),
            other => {
                return Err(VisionErr::Unwired(format!(
                    "vision source form not wired yet: {other:?}"
                )));
            }
        });
    }
    Ok(out)
}

enum VisionErr {
    Template(RuntimeError),
    Unwired(String),
}

/// Render an optional spanned string field.
fn render_opt(
    field: Option<&nika_schema::Spanned<String>>,
    scope: &Scope<'_>,
) -> Result<Option<String>, RuntimeError> {
    field
        .map(|f| expr::render(&f.value, scope))
        .transpose()
        .map_err(RuntimeError::from)
}

/// `returns:` is the `schema:` lane (spec 09 · NIKA-INFER-002).
fn task_schema(
    schema: Option<&nika_schema::Spanned<Value>>,
    contract: Option<&crate::contract::TaskContract<'_>>,
) -> Option<Value> {
    schema
        .map(|v| v.value.clone())
        .or_else(|| contract.map(crate::contract::TaskContract::lowered))
}

/// Authored `system:` plus one `## Skills` section (spec 02 · source order).
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
    use nika_kernel::secret::Secret;
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

    const ANTHROPIC_OK: &str = r#"{"id":"msg_1","model":"claude-x","stop_reason":"end_turn",
        "content":[{"type":"text","text":"ok"}],
        "usage":{"input_tokens":1,"output_tokens":1}}"#;

    impl HttpPostDyn for CapturingHttp {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.captured
                .lock()
                .expect("test mutex")
                .push(request.clone());
            let body = if request.url.contains("anthropic") {
                ANTHROPIC_OK
            } else {
                OPENAI_OK
            };
            Ok(HttpResponse::new(
                200,
                BTreeMap::new(),
                Bytes::from_static(body.as_bytes()),
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

    /// `pub(super)`: the `model_template_tests` sibling runs the same rig.
    pub(super) async fn run_and_capture(yaml: &str) -> Vec<HttpRequest> {
        let (outcome, captured) = run_capture(yaml).await;
        assert!(outcome.ok, "the canned success settles green");
        captured
    }

    /// Same rig as [`run_and_capture`], but a failed task is data (the
    /// #1135 missing-file catching test).
    pub(super) async fn run_capture(yaml: &str) -> (crate::RunOutcome, Vec<HttpRequest>) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture passes the ladder: {report:?}");

        let http = Arc::new(CapturingHttp::default());
        // The B-5 liveness gate dials the local endpoint with a REAL
        // TcpStream before any wire call — point ollama at a speaking
        // loopback stub so the rig stays hermetic (a live/dead ollama on
        // the host must never decide this test · the localhost-is-shared
        // law).
        let stub = {
            #[allow(clippy::disallowed_methods)] // test seam — the probe's worker pattern
            fn spawn() -> u16 {
                let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
                let port = listener.local_addr().expect("addr").port();
                std::thread::spawn(move || {
                    while let Ok((mut stream, _)) = listener.accept() {
                        use std::io::Write as _;
                        let _ = stream.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
                    }
                });
                port
            }
            spawn()
        };
        let registry = Arc::new(ProviderRegistry::new(
            Arc::clone(&http),
            ProvidersConfig::new()
                .with_base_url("ollama", format!("http://127.0.0.1:{stub}"))
                .with_key("anthropic", Secret::new("sk-ant-test")),
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
            .expect("the run completes (a workflow failure is data)");
        (outcome, http.captured())
    }

    #[tokio::test]
    async fn task_timeout_governs_the_provider_http_deadline() {
        // The exact field repro: `timeout: "7m"` on a local-model infer.
        let captured = run_and_capture(
            "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    timeout: \"7m\"\n    infer: { prompt: \"hello\" }\n",
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
            "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    infer: { prompt: \"hello\" }\n",
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

#[cfg(feature = "access-harness")]
// The #824 model-template parity proofs (the house `tests.rs`
// convention — `run_and_capture` is `pub(super)` for that sibling).
#[cfg(test)]
mod tests;
/// #651 (OBS-E promoted) — an `infer` whose visible answer is BLANK while
/// the provider billed real tokens settles the task FAILED with the typed
/// `NIKA-INFER-004`, and the run verdict follows (no more « 7/7 done ·
/// exit 0 » over an empty `output`). Proven through the REAL parse →
/// check → run chain at the http seam (a scripted effect under a real
/// `ProviderRegistry` · `ollama` profile · zero network).
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod infer_empty_answer_tests {
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex};

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

    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};

    /// Serves the queued canned bodies (one per round-trip) · counts every
    /// provider request it saw.
    struct ScriptedHttp {
        bodies: Mutex<VecDeque<&'static str>>,
        calls: Mutex<usize>,
    }

    impl ScriptedHttp {
        fn serving(bodies: &[&'static str]) -> Arc<Self> {
            Arc::new(Self {
                bodies: Mutex::new(bodies.iter().copied().collect()),
                calls: Mutex::new(0),
            })
        }

        fn calls(&self) -> usize {
            *self.calls.lock().expect("test mutex")
        }
    }

    impl HttpPostDyn for ScriptedHttp {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            *self.calls.lock().expect("test mutex") += 1;
            let body = self
                .bodies
                .lock()
                .expect("test mutex")
                .pop_front()
                .ok_or_else(|| HttpError::Other {
                    reason: "ScriptedHttp: no canned response queued".to_owned(),
                })?;
            Ok(HttpResponse::new(
                200,
                BTreeMap::new(),
                Bytes::from_static(body.as_bytes()),
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

    /// A speaking loopback stub so the B-5 liveness gate passes (the
    /// localhost-is-shared law: a live/dead ollama on the host must never
    /// decide this test).
    #[allow(clippy::disallowed_methods)] // test seam — the probe's own worker pattern
    fn spawn_stub_server() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            while let Ok((mut stream, _)) = listener.accept() {
                use std::io::Write as _;
                let _ = stream.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
            }
        });
        port
    }

    /// The blank-answer repro body: empty visible content · real billed
    /// output tokens (the reasoning trace ate the budget).
    const EMPTY_WITH_SPEND: &str = r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}],"usage":{"prompt_tokens":7,"completion_tokens":512}}"#;

    async fn run_workflow(yaml: &str, http: Arc<ScriptedHttp>) -> RunOutcome {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture passes the ladder");
        let registry = Arc::new(ProviderRegistry::new(
            http,
            ProvidersConfig::new().with_base_url(
                "ollama",
                format!("http://127.0.0.1:{}", spawn_stub_server()),
            ),
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
        runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("the run completes (a workflow failure is data)")
    }

    /// The issue's repro: the blank answer fails the task TYPED, the run
    /// verdict goes red, and the declared `retry:` does NOT fire — the
    /// remedy is `max_tokens`, never a re-ask at the same budget.
    #[tokio::test]
    async fn empty_answer_settles_failed_typed_and_the_run_goes_red() {
        let http = ScriptedHttp::serving(&[EMPTY_WITH_SPEND]);
        let outcome = run_workflow(
            "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    retry: { max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false }\n    infer: { prompt: \"hello\" }\n",
            Arc::clone(&http),
        )
        .await;
        assert!(!outcome.ok, "an empty answer is no longer a green run");
        let rec = &outcome.records["ask"];
        assert_eq!(rec.status, TaskStatus::Failure, "the task settles failed");
        let err = rec.error.as_ref().expect("the failure carries its record");
        assert_eq!(err.code, "NIKA-INFER-004", "the typed wire code");
        assert!(
            err.message.contains("infer produced an empty answer"),
            "the warn's teaching survives the promotion: {}",
            err.message
        );
        assert!(
            err.message.contains("max_tokens"),
            "the likely fix is named: {}",
            err.message
        );
        assert!(!err.transient, "never retry-eligible by default");
        assert_eq!(
            rec.attempts,
            Some(1),
            "the declared retry: does NOT fire on a non-transient code"
        );
        assert_eq!(http.calls(), 1, "exactly one billed round-trip");
    }

    /// The authored escape hatch stays bounded: `on_codes: [NIKA-INFER-004]`
    /// opts into retries (same policy as every typed infer failure) — and
    /// the budget caps them, never a forever-loop.
    #[tokio::test]
    async fn empty_answer_retry_is_opt_in_and_bounded() {
        let http = ScriptedHttp::serving(&[EMPTY_WITH_SPEND, EMPTY_WITH_SPEND, EMPTY_WITH_SPEND]);
        let outcome = run_workflow(
            "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    retry: { max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-INFER-004] }\n    infer: { prompt: \"hello\" }\n",
            Arc::clone(&http),
        )
        .await;
        assert!(!outcome.ok);
        let rec = &outcome.records["ask"];
        assert_eq!(rec.status, TaskStatus::Failure);
        assert_eq!(
            rec.error.as_ref().expect("error record").code,
            "NIKA-INFER-004"
        );
        assert_eq!(rec.attempts, Some(3), "the authored retries ran");
        assert_eq!(
            http.calls(),
            3,
            "bounded at max_attempts — never retried forever"
        );
    }

    /// Non-regression: a real answer with the same wire shape settles
    /// green, no error attached.
    #[tokio::test]
    async fn a_real_answer_still_settles_green() {
        let http = ScriptedHttp::serving(&[
            r#"{"choices":[{"message":{"content":"Paris"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":50}}"#,
        ]);
        let outcome = run_workflow(
            "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    infer: { prompt: \"capital of France?\" }\n",
            Arc::clone(&http),
        )
        .await;
        assert!(outcome.ok, "a non-empty answer stays green");
        let rec = &outcome.records["ask"];
        assert_eq!(rec.status, TaskStatus::Success);
        assert!(rec.error.is_none(), "no failure rides a real answer");
        assert_eq!(http.calls(), 1);
    }
}
