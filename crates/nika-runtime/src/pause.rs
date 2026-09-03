// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The durable human gate (ADR-099 rider) — `nika:prompt` pause.
//!
//! Today's normative non-interactive contract (stdlib §`nika:prompt`):
//! with `default:` the engine MUST answer with it; without, it MUST
//! fail `NIKA-BUILTIN-PROMPT-001` — never hang. The rider occupies
//! **exactly that failure branch**, changing nothing else: under a
//! non-interactive machine surface the blocked prompt journals a
//! `workflow_paused` event carrying the prompt payload and the run
//! exits cleanly with state `paused` instead of failing. `--resume`
//! re-arms the prompt (completed upstream work cache-hits); resumed
//! with an answer supplied (`--answer task=value`) it binds; resumed
//! without one it pauses again — idempotent, a paused trace can be
//! resumed any number of times.
//!
//! The payload is rendered over the SECRET-MARKER scope (never a
//! resolved secret value — the journal surface stays masked); a render
//! miss falls back to the RAW template text (authored text · still
//! useful · never secret material).

use std::collections::BTreeMap;

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use serde_json::Value;

use crate::expr::{self, Scope};
use crate::record::TaskRecord;
use crate::task::{Finish, RunResult, SettleAs};

/// The blocking-prompt wire code this rider occupies (stdlib §prompt ·
/// the ONLY branch that pauses — config errors like PROMPT-002 still
/// fail hard).
const PROMPT_BLOCKED_CODE: &str = "NIKA-BUILTIN-PROMPT-001";

/// The harness gate's wire code (P3 B5 · NIKA-1806) — the agent-task
/// twin of [`PROMPT_BLOCKED_CODE`]: the permission bridge judged an
/// ask outside every `permits:` grant and no operator verdict was
/// bound, so the run pauses for a human instead of answering (a gate
/// question is never auto-answered · A-5).
const HARNESS_GATE_CODE: &str = "NIKA-1806";

/// A paused run's prompt payload (ADR-099 rider) — what `workflow_paused`
/// journals and what [`crate::RunOutcome::paused`] carries to the CLI.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct WorkflowPause {
    /// The blocked task's id.
    pub task: String,
    /// The prompt mode (`confirm` · `input` · `choice` — stdlib §prompt).
    pub mode: String,
    /// The prompt message (rendered when it renders · raw template text
    /// otherwise · absent when unreadable).
    pub message: Option<String>,
    /// The `choice` mode's options (empty for the other modes).
    pub choices: Vec<String>,
    /// The F-P4 approval ticket minted BEFORE the ask (NEP-0013) — the
    /// pause serializes the capability the resumed run validates the
    /// `--answer` against (nonce · TTL · shown hash). `None` only where
    /// the ticket layer never ran (a pause synthesized outside a run —
    /// the resume then binds unvalidated, the ADR-099 contract).
    pub approval: Option<crate::approval::ApprovalTicket>,
}

impl WorkflowPause {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(task: String, mode: String, message: Option<String>, choices: Vec<String>) -> Self {
        Self {
            task,
            mode,
            message,
            choices,
            approval: None,
        }
    }

    /// Attach the minted approval ticket (consuming builder · F-P4).
    #[must_use]
    pub fn with_approval(mut self, ticket: crate::approval::ApprovalTicket) -> Self {
        self.approval = Some(ticket);
        self
    }
}

/// Does this finish pause the run? `Some(payload)` iff the task failed
/// on the blocking-prompt branch (PROMPT-001) of a direct
/// `invoke: nika:prompt` AND the author did not claim that code with an
/// `on_error:` policy (an explicit route/filter wins — the
/// author's routing is never hijacked). The F-P4 ticket the gate minted
/// for the step rides the payload (NEP-0013 · the resumed run validates
/// the `--answer` against it).
#[allow(clippy::too_many_arguments)] // the pause scope + the approval book
pub(crate) fn prompt_block(
    finish: &Finish,
    wf: &RawWorkflow,
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    markers: &BTreeMap<String, Value>,
    approvals: &crate::approval::ApprovalBook,
) -> Option<WorkflowPause> {
    let SettleAs::Ran(ran) = &finish.settle else {
        return None;
    };
    let RunResult::Failed { error, .. } = &ran.result else {
        return None;
    };
    if error.code != PROMPT_BLOCKED_CODE {
        return None;
    }
    let task = wf
        .tasks
        .iter()
        .map(|t| &t.value)
        .find(|t| t.id.value == finish.id)?;
    // The author explicitly routed this code (`on_error:` applies to it,
    // whatever the action) — the rider never overrides authored policy.
    // (An applying `recover`/`skip` never reaches here — the result is
    // no longer a failure; this guards the explicit authored route.)
    if let Some(on_error) = task.on_error.as_ref()
        && crate::task::on_error_applies(&on_error.value, error)
    {
        return None;
    }
    let RawAction::Invoke(invoke) = &task.action else {
        return None; // the rider binds the direct builtin invocation only
    };
    if invoke.tool().map(|t| t.value.as_str()) != Some("nika:prompt") {
        return None;
    }
    // Secret values NEVER reach the journal payload — markers only.
    // (`env:` is non-sensitive by construction — real values render.)
    let scope = Scope::workflow_with_value_authorities(records, inputs, consts, markers);
    let mut pause = payload_of(task, invoke.args.as_ref().map(|a| &a.value), &scope);
    // F-P4 — the ticket minted at the gate rides the pause: the resumed
    // run validates the `--answer` against THIS shown hash · nonce · TTL.
    if let Some(ticket) = approvals.ticket_for(&finish.id) {
        pause = pause.with_approval(ticket);
    }
    Some(pause)
}

/// Does this finish pause the run on the HARNESS gate? (P3 B5 · the
/// prompt rider's agent-task twin.) `Some(payload)` iff an `agent:` task
/// failed with the bridge's gate code (NIKA-1806 — an ask outside every
/// `permits:` grant, no operator verdict bound) AND no authored
/// `on_error:` claims it. The question rides the verb's Display
/// (`harness gate: <question>`) and surfaces VERBATIM; mode `confirm`:
/// `--answer <task>=true` grants the re-asked action ONCE (consumed — a
/// second ask pauses again), `false` denies. No F-P4 ticket rides this
/// pause (the question only exists mid-run, where the ticket layer
/// never mints): the resume binds unvalidated per the ADR-099
/// ticketless contract, narrowed by the consume-once law; the
/// question-hash binding (F-P4's harness twin) defers.
pub(crate) fn harness_gate_block(finish: &Finish, wf: &RawWorkflow) -> Option<WorkflowPause> {
    let SettleAs::Ran(ran) = &finish.settle else {
        return None;
    };
    let RunResult::Failed { error, .. } = &ran.result else {
        return None;
    };
    if error.code != HARNESS_GATE_CODE {
        return None;
    }
    let task = wf
        .tasks
        .iter()
        .map(|t| &t.value)
        .find(|t| t.id.value == finish.id)?;
    // The author explicitly routed this code (`on_error:` wins) — the
    // rider never overrides authored policy (the prompt rider's honor).
    if let Some(on_error) = task.on_error.as_ref()
        && crate::task::on_error_applies(&on_error.value, error)
    {
        return None;
    }
    if !matches!(task.action, RawAction::Agent(_)) {
        return None; // the gate binds an agent task only
    }
    // The verb's Display carries the question after ONE fixed prefix —
    // strip it so the pause shows the harness's own words, untouched.
    let question = error
        .message
        .strip_prefix("harness gate: ")
        .map(str::to_owned)
        .or(Some(error.message.clone()));
    Some(WorkflowPause::new(
        finish.id.clone(),
        "confirm".to_owned(),
        question,
        Vec::new(),
    ))
}

/// Build the pause payload from the prompt's `args:` — rendered when the
/// scope resolves them, the RAW authored values otherwise (best-effort ·
/// a payload never blocks the pause itself).
fn payload_of(task: &RawTask, args: Option<&Value>, scope: &Scope<'_>) -> WorkflowPause {
    let raw = args.cloned().unwrap_or(Value::Null);
    let rendered = expr::render_json(&raw, scope).unwrap_or(raw);
    let mode = rendered
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("confirm")
        .to_owned();
    let message = rendered
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let choices = rendered
        .get("choices")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    WorkflowPause::new(task.id.value.clone(), mode, message, choices)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::record::TaskErrorRecord;
    use crate::task::RanTask;

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn failed_finish(id: &str, code: &str) -> Finish {
        failed_finish_msg(id, code, "non-interactive and no `default:`")
    }

    const PROMPT_WF: &str = "nika: gate\ninputs:\n  q: { type: string, required: false, default: \"deploy?\" }\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"choice\", message: \"${{ inputs.q }}\", choices: [\"yes\", \"no\"] }\n";

    /// A Finish failing with a caller-chosen message (the harness gate's
    /// question rides the verb's Display).
    fn failed_finish_msg(id: &str, code: &str, message: &str) -> Finish {
        Finish {
            id: id.to_owned(),
            settle: SettleAs::Ran(Box::new(RanTask {
                decisions: Vec::new(),
                note: "invoke · nika:prompt".to_owned(),
                retries: Vec::new(),
                agent_events: Vec::new(),
                evidence: None,
                duration_ms: 0,
                result: RunResult::Failed {
                    error: TaskErrorRecord::new(code, message, false),
                    cost_usd: None,
                    cost_unpriced: None,
                    access: None,
                },
            })),
            named: BTreeMap::new(),
            resume: None,
            integrity: nika_cap::Integrity::trusted(),
            declassified: Vec::new(),
            approval: None,
        }
    }

    const AGENT_WF: &str = "nika: g\npermits: { exec: [\"git\"] }\ntasks:\n  fix:\n    agent: { prompt: \"fix it\" }\n";

    #[test]
    fn the_harness_gate_pauses_an_agent_task_with_the_question_verbatim() {
        let wf = parse(AGENT_WF);
        let finish = failed_finish_msg("fix", "NIKA-1806", "harness gate: run `rm -rf /`");
        let pause = harness_gate_block(&finish, &wf).expect("the gate branch pauses");
        assert_eq!(pause.task, "fix");
        assert_eq!(pause.mode, "confirm");
        assert_eq!(
            pause.message.as_deref(),
            Some("run `rm -rf /`"),
            "the question surfaces VERBATIM — the prefix stripped, never a paraphrase"
        );
        assert!(pause.choices.is_empty(), "a confirm carries no choices");
        assert!(
            pause.approval.is_none(),
            "no F-P4 ticket exists mid-run — the ticketless ADR-099 posture, documented"
        );
    }

    #[test]
    fn the_harness_gate_never_pauses_other_branches() {
        let wf = parse(AGENT_WF);
        // Another code on an agent task: a hard failure, never a pause.
        assert!(
            harness_gate_block(
                &failed_finish_msg("fix", "NIKA-1804", "harness session failed: x"),
                &wf
            )
            .is_none()
        );
        // The gate code on a NON-agent task (an exec failing with a
        // lifted code string): the gate binds agent tasks only.
        let exec_wf = parse("nika: t\ntasks:\n  ask:\n    exec: { command: [\"true\"] }\n");
        assert!(
            harness_gate_block(
                &failed_finish_msg("ask", "NIKA-1806", "harness gate: x"),
                &exec_wf
            )
            .is_none()
        );
        // An authored on_error claiming the failure wins over the pause
        // (a bare catch-all route — `on_codes` takes only the namespaced
        // NIKA-<NS>-<NNN> form, which the numeric gate code never wears).
        let routed = parse(
            "nika: t\ntasks:\n  fix:\n    agent: { prompt: \"x\" }\n    on_error:\n      skip: true\n",
        );
        assert!(
            harness_gate_block(
                &failed_finish_msg("fix", "NIKA-1806", "harness gate: x"),
                &routed
            )
            .is_none(),
            "authored policy is never hijacked"
        );
    }

    #[test]
    #[allow(clippy::unreachable)]
    fn the_gate_message_without_the_prefix_still_surfaces() {
        // A message that lost the prefix (a future constructor) still
        // surfaces WHOLE — the question is never swallowed.
        let wf = parse(AGENT_WF);
        let finish = failed_finish_msg("fix", "NIKA-1806", "a bare question");
        let pause = harness_gate_block(&finish, &wf).expect("pauses");
        assert_eq!(pause.message.as_deref(), Some("a bare question"));
    }

    #[test]
    fn prompt_001_on_a_direct_invoke_pauses_with_the_rendered_payload() {
        let wf = parse(PROMPT_WF);
        let records = BTreeMap::new();
        let vars = BTreeMap::from([("q".to_owned(), serde_json::json!("deploy?"))]);
        let markers = BTreeMap::new();
        let pause = prompt_block(
            &failed_finish("ask", PROMPT_BLOCKED_CODE),
            &wf,
            &records,
            &vars,
            &BTreeMap::new(),
            &markers,
            &crate::approval::ApprovalBook::new(),
        )
        .expect("the blocking branch pauses");
        assert_eq!(pause.task, "ask");
        assert_eq!(pause.mode, "choice");
        assert_eq!(pause.message.as_deref(), Some("deploy?"), "rendered");
        assert_eq!(pause.choices, vec!["yes".to_owned(), "no".to_owned()]);
    }

    #[test]
    fn other_codes_and_other_tools_never_pause() {
        let wf = parse(PROMPT_WF);
        let (records, vars, markers) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        // PROMPT-002 (config error) stays a hard failure.
        assert!(
            prompt_block(
                &failed_finish("ask", "NIKA-BUILTIN-PROMPT-002"),
                &wf,
                &records,
                &vars,
                &BTreeMap::new(),
                &markers,
                &crate::approval::ApprovalBook::new(),
            )
            .is_none()
        );
        // A non-prompt task failing with the same code text never pauses.
        let exec_wf = parse("nika: t\ntasks:\n  ask:\n    exec: { command: [\"true\"] }\n");
        assert!(
            prompt_block(
                &failed_finish("ask", PROMPT_BLOCKED_CODE),
                &exec_wf,
                &records,
                &vars,
                &BTreeMap::new(),
                &markers,
                &crate::approval::ApprovalBook::new(),
            )
            .is_none()
        );
    }

    #[test]
    fn an_authored_on_error_route_wins_over_the_pause() {
        // The author explicitly claimed the code — the
        // rider never hijacks authored routing.
        let wf = parse(
            "nika: t\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { message: \"go?\" }\n    on_error:\n      skip: true\n      on_codes: [\"NIKA-BUILTIN-PROMPT-001\"]\n",
        );
        let (records, vars, markers) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        assert!(
            prompt_block(
                &failed_finish("ask", PROMPT_BLOCKED_CODE),
                &wf,
                &records,
                &vars,
                &BTreeMap::new(),
                &markers,
                &crate::approval::ApprovalBook::new(),
            )
            .is_none()
        );
    }

    /// The rider end-to-end over mock seams: a blocked prompt PAUSES
    /// (`workflow_paused` · no `task_failed` · later waves never dispatch);
    /// the resumed run cache-hits upstream, binds the `--answer`, and
    /// completes; resumed with NO answer it pauses again (idempotent).
    #[tokio::test]
    async fn pause_then_answered_resume_completes_then_no_answer_repauses() {
        use nika_event::EventKind;
        use nika_kernel::tool_executor::{ToolErrorMeta, ToolResult};

        const GATED: &str = "nika: gated\npermits: { exec: [\"echo\"], tools: [\"nika:prompt\"] }\ntasks:\n  prep:\n    exec: { command: [\"echo\", \"ready\"] }\n  ask:\n    after: { prep: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"input\", message: \"proceed?\" }\n  finish:\n    after: { ask: success }\n    exec: { command: [\"echo\", \"done\"] }\n";

        let blocked_prompt = || {
            let mut result = ToolResult::error("tc1", "non-interactive and no `default:`");
            result.error_meta = Some(ToolErrorMeta::new(
                Some(PROMPT_BLOCKED_CODE.to_owned()),
                false,
            ));
            result
        };

        // Run 1 — the prompt blocks: paused, prep journaled, after untouched.
        let (paused_outcome, sink) = harness::run_gated(
            GATED,
            harness::Seams {
                shell: vec!["ready\n"],
                tool: vec![blocked_prompt()],
                pause: true,
                plan: None,
                answers: BTreeMap::new(),
            },
        )
        .await;
        let pause = paused_outcome.paused.as_ref().expect("the run paused");
        assert_eq!(pause.task, "ask");
        assert!(paused_outcome.ok, "a pause is never a failure");
        let kinds: Vec<_> = sink.events().iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&EventKind::WorkflowPaused));
        assert!(!kinds.contains(&EventKind::TaskFailed), "no failure frame");
        assert!(!kinds.contains(&EventKind::WorkflowFailed));
        assert!(
            !sink
                .events()
                .iter()
                .any(|e| harness::str_field(e, "task") == Some("after")
                    && e.kind == EventKind::TaskStarted),
            "downstream never dispatched"
        );
        let plan = harness::plan_from(&sink);
        assert!(plan.contains_key("prep"), "prep's success is resumable");

        // Run 2 — resumed WITH an answer: prep cache-hits, ask binds, done.
        let (answered, sink) = harness::run_gated(
            GATED,
            harness::Seams {
                shell: vec!["done\n"],
                tool: vec![ToolResult::success("tc1", "yes")],
                pause: true,
                plan: Some(plan.clone()),
                answers: BTreeMap::from([("ask".to_owned(), serde_json::json!("yes"))]),
            },
        )
        .await;
        assert!(answered.paused.is_none(), "the answer unblocked the run");
        assert!(answered.ok);
        assert_eq!(answered.cache_hits, vec!["prep".to_owned()]);
        assert!(
            sink.events()
                .iter()
                .any(|e| e.kind == EventKind::WorkflowCompleted),
            "the resumed run completes"
        );

        // Run 3 — resumed with NO answer: pauses again (idempotent).
        let (repaused, _) = harness::run_gated(
            GATED,
            harness::Seams {
                shell: vec![],
                tool: vec![blocked_prompt()],
                pause: true,
                plan: Some(plan),
                answers: BTreeMap::new(),
            },
        )
        .await;
        assert!(repaused.paused.is_some(), "a paused trace re-pauses");
        assert_eq!(repaused.cache_hits, vec!["prep".to_owned()]);
    }

    /// Mock-seam harness for the pause tests (the builtin dispatcher is
    /// an L1.5 sideways dep — the mock executor plays the prompt).
    mod harness {
        use std::collections::BTreeMap;
        use std::sync::Arc;

        use nika_kernel::tool_executor::ToolResult;
        use nika_kernel_mock::{
            MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
        };
        use nika_providers::{ProviderRegistry, ProvidersConfig};
        use nika_verb_agent::AgentVerb;
        use nika_verb_exec::ExecVerb;
        use nika_verb_infer::InferVerb;
        use nika_verb_invoke::InvokeVerb;
        use serde_json::Value;

        use crate::resume::{PriorSuccess, ResumePlan, fields};
        use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

        pub(super) struct Seams {
            pub shell: Vec<&'static str>,
            /// Queued tool results — a PROMPT-001 error result plays the
            /// blocked prompt (tool-reported failures ride the Ok lane ·
            /// the MCP `isError` split).
            pub tool: Vec<ToolResult>,
            pub pause: bool,
            pub plan: Option<ResumePlan>,
            pub answers: BTreeMap<String, Value>,
        }

        pub(super) fn str_field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
            event
                .fields
                .iter()
                .find(|kv| kv.key == key)
                .and_then(|kv| match &kv.value {
                    nika_types::resource::Value::String(s) => Some(s.as_str()),
                    _ => None,
                })
        }

        /// Fold a sink's stream back into a resume plan (what the CLI does).
        pub(super) fn plan_from(sink: &VecSink) -> ResumePlan {
            let mut plan = ResumePlan::new();
            for event in sink.events() {
                if event.kind != nika_event::EventKind::TaskCompleted {
                    continue;
                }
                let (Some(task), Some(def), Some(input), Some(output)) = (
                    str_field(event, "task"),
                    str_field(event, fields::DEF_HASH),
                    str_field(event, fields::INPUT_HASH),
                    str_field(event, fields::OUTPUT),
                ) else {
                    continue;
                };
                let Ok(output) = serde_json::from_str(output) else {
                    continue;
                };
                plan.insert(
                    task.to_owned(),
                    PriorSuccess::new(def.to_owned(), input.to_owned(), output),
                );
            }
            plan
        }

        #[allow(clippy::expect_used)]
        pub(super) async fn run_gated(yaml: &str, seams: Seams) -> (RunOutcome, VecSink) {
            let wf = nika_schema::parse(
                yaml,
                nika_schema::FileId::new(0),
                nika_schema::ParseMode::Strict,
            )
            .expect("fixture parses");
            let report = nika_check::check(&wf);
            assert!(report.is_clean());
            let mut shell = MockShell::new();
            for out in seams.shell {
                shell = shell.enqueue_ok(out);
            }
            let mut tools = MockToolExecutor::new();
            for result in seams.tool {
                tools = tools.enqueue_ok(result);
            }
            let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
            let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
            let mut runtime = Runtime::new(
                ExecVerb::new(Arc::new(shell)),
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
            .with_prompt_pause(seams.pause)
            .with_prompt_answers(seams.answers);
            if let Some(plan) = seams.plan {
                runtime = runtime.with_resume_plan(plan);
            }
            let mut stamper = DeterministicStamper::new();
            let mut sink = VecSink::new();
            let outcome = runtime
                .run(&wf, &report, &mut stamper, &mut sink)
                .await
                .expect("clean run");
            (outcome, sink)
        }
    }

    #[test]
    fn unrenderable_payload_falls_back_to_the_raw_template_text() {
        // `${{ tasks.missing.output }}` cannot render (no record) — the
        // payload carries the AUTHORED text instead of blocking the pause.
        let wf = parse(
            "nika: t\ntasks:\n  up:\n    exec: { command: [\"true\"] }\n  ask:\n    with: { upstream: \"${{ tasks.up.output }}\" }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: \"input\", message: \"about ${{ with.upstream }}\" }\n",
        );
        let (records, vars, markers) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
        let pause = prompt_block(
            &failed_finish("ask", PROMPT_BLOCKED_CODE),
            &wf,
            &records,
            &vars,
            &BTreeMap::new(),
            &markers,
            &crate::approval::ApprovalBook::new(),
        )
        .expect("pauses with the raw fallback");
        assert_eq!(pause.mode, "input");
        assert_eq!(
            pause.message.as_deref(),
            Some("about ${{ with.upstream }}"),
            "raw authored text · never silence, never secret material"
        );
    }
}
