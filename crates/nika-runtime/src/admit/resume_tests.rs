// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIK-245 · a run paused at a human gate resumes on the work it still has
//! to do. The paused leg spent part of its ceiling on a priced step; the
//! resumed leg runs under what that leaves, and its admission used to price
//! the WHOLE workflow again, the finished step included (it is a cache hit
//! and spends nothing). « non » and « oui » were both refused NIKA-1709,
//! the answer was never recorded and the run stayed paused.
//!
//! Both legs run the real runtime over a priced cloud seat
//! (`openai/gpt-4o-mini`) whose wire is a mock: the spend is the catalog's
//! price of the usage the mock reports, exactly as a live call is priced.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nika_event::{Event, EventKind};
use nika_kernel::secret::Secret;
use nika_kernel::tool_executor::{ToolErrorMeta, ToolResult};
use nika_kernel_mock::{
    MockClock, MockHttp, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};

use super::{budget_floor_refusal_bound_over, budget_floor_refusal_remaining, remaining_cost};
use crate::approval::{ApprovalTicket, PausedApproval};
use crate::resume::{PriorSuccess, ResumePlan, fields};
use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, RuntimeError, VecSink};

/// A priced step before the gate (`draft`, its floor $0.0096), the gate,
/// a branch the answer opens (`send`, gated: the cheapest path never pays
/// it) and a step that runs whatever the answer (`log`, floor $0.0006).
const GATED: &str = r#"
nika: resume-floor
model: openai/gpt-4o-mini
permits: { tools: ["nika:prompt"] }
tasks:
  draft:
    infer: { prompt: "Draft the note.", max_tokens: 16000 }
  review:
    with: { note: "${{ tasks.draft.output }}" }
    invoke: { tool: "nika:prompt", args: { mode: "confirm", message: "Send ${{ with.note }}?" } }
  send:
    with: { approved: "${{ tasks.review.output }}", note: "${{ tasks.draft.output }}" }
    when: "${{ with.approved == true }}"
    infer: { prompt: "Polish ${{ with.note }}", max_tokens: 1000 }
  log:
    with: { approved: "${{ tasks.review.output }}" }
    infer: { prompt: "Log the decision ${{ with.approved }}.", max_tokens: 1000 }
"#;

/// The endpoint the catalog's `gpt-4o-mini` tariff names.
const OPENAI_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

/// The ceiling the first leg starts under: it admits the whole static floor.
const CEILING: f64 = 0.0105;

/// One priced answer of the mock wire: 10 prompt tokens, `completion` out
/// (complete meters: the total is their sum, so the catalog prices the call).
fn answer(completion: u32) -> String {
    json!({
        "id": "cc",
        "model": "gpt-4o-mini-2024-07-18",
        "choices": [{"message": {"content": "the note"}, "finish_reason": "stop"}],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": completion,
            "total_tokens": 10 + completion
        }
    })
    .to_string()
}

fn blocked_prompt() -> ToolResult {
    let mut result = ToolResult::error("review", "non-interactive and no `default:`");
    result.error_meta = Some(ToolErrorMeta::new(
        Some("NIKA-BUILTIN-PROMPT-001".to_owned()),
        false,
    ));
    result
}

/// What one leg is given: the workflow, the wire's answers, the tool's
/// answers, and the resume (plan · ticket · answer) of a continued leg.
struct Leg<'s> {
    source: &'s str,
    wire: Vec<String>,
    tools: Vec<ToolResult>,
    ceiling: f64,
    resume: Option<(ResumePlan, PausedApproval, Value)>,
}

async fn run_leg(leg: Leg<'_>) -> Result<(RunOutcome, Vec<Event>), RuntimeError> {
    let wf = nika_schema::parse(
        leg.source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "the fixture checks clean: {report:?}");
    let mut http = MockHttp::new();
    for body in leg.wire {
        // The route a tariff prices is the endpoint the call landed on.
        http = http.enqueue_ok_final_url(200, body, OPENAI_ENDPOINT);
    }
    let mut tools = MockToolExecutor::new();
    for result in leg.tools {
        tools = tools.enqueue_ok(result);
    }
    let registry = Arc::new(ProviderRegistry::new(
        Arc::new(http),
        ProvidersConfig::new().with_key("openai", Secret::new("sk-test")),
    ));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let mut runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "openai/gpt-4o-mini"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default()
            .with_approval_operator("fixture".to_owned())
            .with_max_cost_usd(Some(leg.ceiling)),
    )
    .with_prompt_pause(true);
    if let Some((plan, paused, answer)) = leg.resume {
        runtime = runtime
            .with_resume_plan(plan)
            .with_paused_approval(Some(paused))
            .with_prompt_answers(BTreeMap::from([("review".to_owned(), answer)]));
    }
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime.run(&wf, &report, &mut stamper, &mut sink).await?;
    Ok((outcome, sink.into_events()))
}

fn text<'e>(event: &'e Event, key: &str) -> Option<&'e str> {
    event
        .fields
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| match &f.value {
            FieldValue::String(s) => Some(s.as_str()),
            _ => None,
        })
}

/// The journaled successes, folded back into a skip plan (what `--resume` does).
fn plan_from(events: &[Event]) -> ResumePlan {
    let mut plan = ResumePlan::new();
    for event in events.iter().filter(|e| e.kind == EventKind::TaskCompleted) {
        let (Some(task), Some(def), Some(input), Some(output)) = (
            text(event, "task"),
            text(event, fields::DEF_HASH),
            text(event, fields::INPUT_HASH),
            text(event, fields::OUTPUT),
        ) else {
            continue;
        };
        let output = serde_json::from_str(output).expect("a journaled output parses");
        plan.insert(
            task.to_owned(),
            PriorSuccess::new(def.to_owned(), input.to_owned(), output),
        );
    }
    plan
}

/// The ticket the pause minted, with the paused run's identity.
fn paused_from(events: &[Event]) -> PausedApproval {
    let pause = (events.iter().rev())
        .find(|e| e.kind == EventKind::WorkflowPaused)
        .expect("the first leg paused");
    let int = |key: &str| {
        (pause.fields.iter())
            .find(|f| f.key == key)
            .and_then(|f| match f.value {
                FieldValue::Int(v) => Some(v),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the pause frame carries {key}"))
    };
    let nonce = (events.iter())
        .find(|e| e.kind == EventKind::WorkflowStarted)
        .map(|e| e.id.uuid.to_string())
        .expect("the first leg started");
    PausedApproval::new(
        ApprovalTicket::new(
            text(pause, "approval_shown_hash")
                .expect("shown hash")
                .to_owned(),
            text(pause, "approval_nonce").expect("nonce").to_owned(),
            text(pause, "task").expect("task").to_owned(),
            int("approval_minted_at_ms"),
            u32::try_from(int("approval_ttl_seconds")).expect("ttl"),
        ),
        nonce,
    )
}

/// The first leg: `draft` is priced and paid, the gate pauses the run.
async fn paused_leg() -> (f64, Vec<Event>) {
    let (outcome, events) = run_leg(Leg {
        source: GATED,
        wire: vec![answer(1000)],
        tools: vec![blocked_prompt()],
        ceiling: CEILING,
        resume: None,
    })
    .await
    .expect("the whole static floor fits the first leg's ceiling");
    assert!(
        outcome.paused.is_some(),
        "the gate parks the run: {outcome:?}"
    );
    // What the pause journals as spent is what a resume subtracts from the ceiling.
    let spent = (events.iter())
        .find(|e| e.kind == EventKind::WorkflowPaused)
        .and_then(|e| e.fields.iter().find(|f| f.key == "total_cost_usd"))
        .and_then(|f| match f.value {
            FieldValue::Float(usd) => Some(usd),
            _ => None,
        })
        .expect("the pause journals the priced step's spend");
    (spent, events)
}

/// The tasks that settled with `kind`, in stream order.
fn settled(events: &[Event], kind: EventKind) -> Vec<&str> {
    (events.iter().filter(|e| e.kind == kind))
        .filter_map(|e| text(e, "task"))
        .collect()
}

/// The answer the gate recorded (`allow` · `deny`).
fn decision(events: &[Event]) -> Option<&str> {
    (events.iter())
        .find(|e| e.kind == EventKind::ApprovalDecided)
        .and_then(|e| text(e, "decision"))
}

/// The resumed leg, answered `answer`, under what the paused leg left.
async fn resumed_leg(answer_value: bool) -> (RunOutcome, Vec<Event>) {
    let (spent, first) = paused_leg().await;
    let left = CEILING - spent;
    // The scenario NIK-245 measured: what was spent before the pause is more
    // than the ceiling minus the whole static floor, while the work still to
    // run fits what is left.
    let wf = nika_schema::parse(
        GATED,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let cost = nika_check::check(&wf).cost;
    let log_floor = (cost.tasks.iter())
        .find(|t| t.task == "log")
        .and_then(|t| t.min_path_usd)
        .expect("log is priced");
    assert!(
        cost.min_path_total_usd > left,
        "the whole floor ${} must exceed the ${left} left",
        cost.min_path_total_usd
    );
    assert!(
        log_floor <= left,
        "the remaining floor ${log_floor} fits ${left}"
    );
    let wire = if answer_value {
        vec![answer(5), answer(5)]
    } else {
        vec![answer(5)]
    };
    let tools = vec![
        ToolResult::success("review", answer_value.to_string())
            .with_structured(json!(answer_value)),
    ];
    let resume = (plan_from(&first), paused_from(&first), json!(answer_value));
    run_leg(Leg {
        source: GATED,
        wire,
        tools,
        ceiling: left,
        resume: Some(resume),
    })
    .await
    .unwrap_or_else(|e| panic!("the resumed leg is admitted on the work it still runs: {e}"))
}

#[tokio::test]
async fn a_no_at_the_gate_is_recorded_on_resume_under_what_the_pause_left() {
    let (outcome, events) = resumed_leg(false).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.cache_hits, vec!["draft".to_owned()]);
    assert_eq!(decision(&events), Some("deny"), "the « non » is recorded");
    assert_eq!(settled(&events, EventKind::TaskSkipped), vec!["send"]);
    assert_eq!(
        settled(&events, EventKind::TaskCompleted),
        vec!["review", "log"]
    );
}

#[tokio::test]
async fn a_yes_at_the_gate_is_recorded_on_resume_under_what_the_pause_left() {
    let (outcome, events) = resumed_leg(true).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.cache_hits, vec!["draft".to_owned()]);
    assert_eq!(decision(&events), Some("allow"), "the « oui » is recorded");
    let mut completed = settled(&events, EventKind::TaskCompleted);
    completed.sort_unstable();
    assert_eq!(completed, vec!["log", "review", "send"]);
    assert!(settled(&events, EventKind::TaskSkipped).is_empty());
}

/// A step edited since the pause is no cache hit: it runs again, so its cost
/// stays in the resumed leg's floor, which refuses what the pause left.
#[tokio::test]
async fn a_step_edited_since_the_pause_is_priced_again() {
    let (spent, first) = paused_leg().await;
    let edited = GATED.replace("Draft the note.", "Draft the memo.");
    let resume = (plan_from(&first), paused_from(&first), json!(false));
    let refused = run_leg(Leg {
        source: &edited,
        wire: Vec::new(),
        tools: Vec::new(),
        ceiling: CEILING - spent,
        resume: Some(resume),
    })
    .await;
    let Err(RuntimeError::BudgetFloor { message }) = refused else {
        panic!("the edited step is priced again and refused: {refused:?}");
    };
    assert!(message.contains("floor $0.010200"), "{message}");
}

fn parsed(yaml: &str) -> (nika_schema::raw::RawWorkflow, nika_check::CheckReport) {
    let mode = nika_schema::ParseMode::Strict;
    let wf = nika_schema::parse(yaml, nika_schema::FileId::new(0), mode).expect("fixture parses");
    let report = nika_check::check(&wf);
    (wf, report)
}

/// The budget refusal's words, `None` when the gate admits.
fn floor_words(refusal: Option<RuntimeError>) -> Option<String> {
    refusal.map(|error| match error {
        RuntimeError::BudgetFloor { message } => message,
        other => panic!("a budget refusal, not {other}"),
    })
}

/// The pure gate: under what the pause left, the whole static floor refuses
/// and the floor of the work still to run (`log` · $0.0006) admits.
#[test]
fn a_resumed_floor_prices_only_the_work_still_to_run() {
    let (wf, report) = parsed(GATED);
    let none = BTreeMap::new();
    let probes = Vec::new();
    let settled = BTreeSet::from(["draft".to_owned()]);
    let whole =
        budget_floor_refusal_bound_over(&wf, &report, Some(0.0099), None, &none, false, &probes);
    let whole = floor_words(whole).expect("the whole floor still refuses a fresh run");
    assert!(whole.contains("floor $0.010200"), "{whole}");
    let rest = remaining_cost(&report.cost, &settled);
    assert!((rest.min_path_total_usd - 0.0006).abs() < 1e-12, "{rest:?}");
    let left: Vec<&str> = rest.tasks.iter().map(|t| t.task.as_str()).collect();
    assert_eq!(left, vec!["send", "log"]);
    let resumed = (None, &none);
    let admitted = budget_floor_refusal_remaining(
        &wf,
        &report,
        Some(0.0099),
        resumed,
        false,
        &probes,
        &settled,
    );
    assert_eq!(floor_words(admitted), None);
    // What still runs is still judged: a budget below `log`'s floor refuses.
    let short = budget_floor_refusal_remaining(
        &wf,
        &report,
        Some(0.0005),
        resumed,
        false,
        &probes,
        &settled,
    );
    let short = floor_words(short).expect("the remaining floor exceeds $0.0005");
    assert!(short.contains("floor $0.000600"), "{short}");
}

/// A paused leg that spent its whole ceiling leaves $0: a step it finished
/// with no token ceiling no longer refuses the zero budget, a step still to
/// run does.
#[test]
fn a_zero_budget_judges_only_the_work_still_to_run() {
    let (wf, report) = parsed(&GATED.replace(", max_tokens: 16000", ""));
    let none = BTreeMap::new();
    let probes = Vec::new();
    let fresh =
        budget_floor_refusal_bound_over(&wf, &report, Some(0.0), None, &none, false, &probes);
    let fresh = floor_words(fresh).expect("a fresh run refuses the unbounded step");
    assert!(fresh.contains("`draft`"), "{fresh}");
    let resumed = (None, &none);
    let draft = BTreeSet::from(["draft".to_owned()]);
    let left =
        budget_floor_refusal_remaining(&wf, &report, Some(0.0), resumed, false, &probes, &draft);
    let left = floor_words(left).expect("`log` still runs and still costs");
    assert!(
        left.contains("floor $0.000600") && !left.contains("`draft`"),
        "{left}"
    );
    let done = BTreeSet::from(["draft".to_owned(), "log".to_owned()]);
    let none_left =
        budget_floor_refusal_remaining(&wf, &report, Some(0.0), resumed, false, &probes, &done);
    assert_eq!(
        floor_words(none_left),
        None,
        "nothing priced is left to run"
    );
}
