// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cost Intelligence (2026-07-08) — the run ledger + `--max-cost-usd`
//! through the REAL parse → check → run chain: totals on the terminal
//! frame, block-before budget semantics (the crossing call completes
//! and counts · unstarted work cancels · NIKA-1704 carries both
//! numbers), and the no-fake-zero law at every level.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

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

use nika_event::EventKind;
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_types::resource::Value as FieldValue;

/// A tool result whose structured value reports real spend — the
/// honest-spend channel (`nika:image_generate` on tick-billed
/// providers rides exactly this shape).
fn priced_result(id: &str, usd: f64) -> ToolResult {
    ToolResult::success(id, "done")
        .with_structured(serde_json::json!({ "cost_usd": usd, "ok": true }))
}

async fn run_yaml(
    yaml: &str,
    tools: MockToolExecutor,
    budget: Option<f64>,
) -> (RunOutcome, VecSink) {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the ladder: {}",
        serde_json::to_string(&report).unwrap_or_default()
    );

    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default().with_max_cost_usd(budget),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run");
    (outcome, sink)
}

fn terminal_field<'a>(sink: &'a VecSink, kind: EventKind, key: &str) -> Option<&'a FieldValue> {
    sink.events()
        .iter()
        .find(|e| e.kind == kind)
        .and_then(|e| e.fields.iter().find(|f| f.key == key))
        .map(|f| &f.value)
}

const ONE_TOOL: &str = "nika: w\ntasks:\n  render:\n    invoke: { tool: \"nika:jq\", args: { input: { x: 1 }, expression: \".\" } }\n";

#[tokio::test]
async fn tool_spend_totals_ride_the_terminal_frame() {
    let tools = MockToolExecutor::new().enqueue_ok(priced_result("c1", 0.02));
    let (outcome, sink) = run_yaml(ONE_TOOL, tools, None).await;
    assert!(outcome.ok);
    assert_eq!(outcome.total_cost_usd, Some(0.02));
    assert_eq!(outcome.priced_calls, 1);
    assert!(!outcome.budget_exceeded);

    let total = terminal_field(&sink, EventKind::WorkflowCompleted, "total_cost_usd")
        .expect("the metered total rides the terminal frame");
    assert!(matches!(total, FieldValue::Float(c) if (*c - 0.02).abs() < 1e-12));
    // The point-in-time honesty stamp: WHICH prices billed this run.
    let as_of = terminal_field(&sink, EventKind::WorkflowCompleted, "pricing_as_of")
        .expect("the snapshot date rides next to the total");
    assert!(
        matches!(as_of, FieldValue::String(s) if s == nika_catalog::pricing_snapshot().as_of),
        "as_of mirrors the vendored snapshot"
    );
    let by_source = terminal_field(&sink, EventKind::WorkflowCompleted, "cost_by_source")
        .expect("the by-source breakdown rides");
    assert!(
        matches!(by_source, FieldValue::String(s) if s.contains("nika:jq")),
        "tool spend attributes to the tool id"
    );
}

#[tokio::test]
async fn unmetered_run_terminal_frame_is_field_free() {
    // A run that spends nothing METERED carries NO cost fields at
    // all — a `total_cost_usd: 0.0` nobody metered would be the
    // fake zero at the run level.
    let tools = MockToolExecutor::new()
        .enqueue_ok(ToolResult::success("c1", "plain text · no spend report"));
    let (outcome, sink) = run_yaml(ONE_TOOL, tools, None).await;
    assert!(outcome.ok);
    assert_eq!(outcome.total_cost_usd, None);
    assert_eq!(outcome.priced_calls, 0);
    assert!(
        terminal_field(&sink, EventKind::WorkflowCompleted, "total_cost_usd").is_none(),
        "absent is honest — no total field on an unmetered run"
    );
}

#[tokio::test]
async fn budget_blocks_the_next_wave_and_names_both_numbers() {
    // Two waves (b reads a) · the first task's spend crosses the
    // budget → in-flight completed and counted, the NEXT wave never
    // dispatches: b cancels, the run fails NIKA-1704 with
    // spent-vs-budget (the LiteLLM error shape).
    let yaml = "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:jq\", args: { input: { x: 1 }, expression: \".\" } }\n  b:\n    with: { prev: \"${{ tasks.a.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.prev }}\", expression: \".\" } }\n";
    let tools = MockToolExecutor::new()
        .enqueue_ok(priced_result("c1", 0.06))
        .enqueue_ok(priced_result("c2", 0.06));
    let (outcome, sink) = run_yaml(yaml, tools, Some(0.05)).await;

    assert!(!outcome.ok, "a budget stop is a run failure");
    assert!(outcome.budget_exceeded);
    assert_eq!(
        outcome.total_cost_usd,
        Some(0.06),
        "the crossing call COUNTS"
    );
    assert_eq!(
        outcome.records["b"].status,
        nika_runtime::TaskStatus::Cancelled,
        "the unstarted task cancels"
    );
    let detail = terminal_field(&sink, EventKind::WorkflowFailed, "detail")
        .expect("the failed frame explains");
    let FieldValue::String(detail) = detail else {
        panic!("detail is text")
    };
    assert!(detail.contains("NIKA-1704"), "{detail}");
    assert!(detail.contains("0.06"), "spent rides the message: {detail}");
    assert!(
        detail.contains("0.05"),
        "budget rides the message: {detail}"
    );
    // The cancelled task's note names the budget.
    let cancel_note = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCancelled)
        .and_then(|e| e.fields.iter().find(|f| f.key == "note"))
        .expect("cancelled frame carries a note");
    assert!(
        matches!(&cancel_note.value, FieldValue::String(s) if s.contains("--max-cost-usd")),
        "the WHY rides the cancel"
    );
}

#[tokio::test]
async fn budget_starves_a_serial_fan_out_mid_flight() {
    // `max_parallel: 1` serializes admission: 0.03 + 0.03 crosses
    // the 0.05 budget after the second iteration — the third is
    // never admitted, the task fails NIKA-1704 naming the shortfall.
    let yaml = "nika: w\ntasks:\n  fan:\n    for_each: { items: [1, 2, 3], max_parallel: 1 }\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ item }}\", expression: \".\" } }\n";
    let tools = MockToolExecutor::new()
        .enqueue_ok(priced_result("c1", 0.03))
        .enqueue_ok(priced_result("c2", 0.03))
        .enqueue_ok(priced_result("c3", 0.03));
    let (outcome, _sink) = run_yaml(yaml, tools, Some(0.05)).await;

    assert!(!outcome.ok);
    assert!(outcome.budget_exceeded);
    assert_eq!(
        outcome.total_cost_usd,
        Some(0.06),
        "two admitted iterations counted · the third never spent"
    );
    let error = outcome.records["fan"]
        .error
        .as_ref()
        .expect("the starved fan-out fails");
    assert_eq!(error.code, "NIKA-1704");
    assert!(
        error.message.contains("1 iteration(s)"),
        "names the denied count: {}",
        error.message
    );
}

#[tokio::test]
async fn unpriced_spend_never_trips_a_budget() {
    // A mock-provider infer reports usage but prices to NOTHING —
    // the budget bounds METERED spend only (said loudly at the
    // preflight), so the run completes and the unpriced count rides.
    let yaml = "nika: w\nmodel: mock/echo\ntasks:\n  ask:\n    infer: { prompt: \"salut\" }\n";
    let (outcome, sink) = run_yaml(yaml, MockToolExecutor::new(), Some(0.0)).await;
    assert!(outcome.ok, "unmetered spend cannot cross a budget");
    assert!(!outcome.budget_exceeded);
    assert_eq!(outcome.total_cost_usd, None);
    assert_eq!(
        outcome.unpriced_calls, 1,
        "the mock infer is counted as unpriced"
    );
    let unpriced = terminal_field(&sink, EventKind::WorkflowCompleted, "unpriced_calls")
        .expect("the unpriced count rides the terminal frame");
    assert!(matches!(unpriced, FieldValue::Int(1)));
    // …and the task frame names the WHY.
    let reason = sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .and_then(|e| e.fields.iter().find(|f| f.key == "cost_unpriced"))
        .expect("the task frame carries the reason");
    assert!(
        matches!(&reason.value, FieldValue::String(s) if s == "mock_provider"),
        "mock classifies as mock_provider"
    );
}

/// Cost Intelligence follow-up (billed-then-failed): a loop that BURNS
/// real spend and then dies must surface that spend — on the
/// `task_failed` frame, in the run totals, and to the budget gate.
mod billed_then_failed {
    use std::sync::Arc;

    use nika_event::EventKind;
    use nika_kernel::ai::provider::{
        ContentBlock, InferResponse, ProviderError, StopReason, TokenUsage,
    };
    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
    use nika_types::resource::Value as FieldValue;
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;

    /// Turn 1: the model calls a tool that reports REAL spend ($0.06) ·
    /// turn 2: the provider dies — the loop fails after burning money.
    fn dying_priced_agent() -> (MockProvider, MockToolExecutor) {
        let tool_turn = InferResponse::new(
            vec![ContentBlock::ToolUse {
                id: "c1".to_owned(),
                name: "nika:jq".to_owned(),
                input: serde_json::json!({ "input": {"x": 1}, "expression": "." }),
            }],
            TokenUsage::new(40, 12),
            StopReason::ToolUse,
        );
        let provider = MockProvider::new("mock")
            .enqueue_response(tool_turn)
            .enqueue_error(ProviderError::Api {
                status: 500,
                message: "upstream 500".to_owned(),
            });
        let tools = MockToolExecutor::new().enqueue_ok(
            ToolResult::success("c1", "done")
                .with_structured(serde_json::json!({ "ok": true, "cost_usd": 0.06 })),
        );
        (provider, tools)
    }

    async fn run_agent_yaml(
        yaml: &str,
        provider: MockProvider,
        tools: MockToolExecutor,
        budget: Option<f64>,
    ) -> (nika_runtime::RunOutcome, VecSink) {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture passes the ladder");
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/agent",
            ),
            MockClock::new(),
            RuntimeConfig::default().with_max_cost_usd(budget),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        (outcome, sink)
    }

    const DYING_AGENT: &str = "nika: w\ntasks:\n  work:\n    agent:\n      prompt: \"burn a paid tool then die\"\n      tools: [\"nika:jq\"]\n";

    #[tokio::test]
    async fn failed_task_frame_carries_the_burned_spend() {
        let (provider, tools) = dying_priced_agent();
        let (outcome, sink) = run_agent_yaml(DYING_AGENT, provider, tools, None).await;
        assert!(!outcome.ok, "the loop died");
        let failed = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskFailed)
            .expect("a task_failed frame");
        let cost = failed
            .fields
            .iter()
            .find(|f| f.key == "cost_usd")
            .expect("the burned tool spend rides the FAILURE frame");
        assert!(
            matches!(&cost.value, FieldValue::Float(c) if (*c - 0.06).abs() < 1e-12),
            "the $0.06 the dying loop spent is on the frame"
        );
        // The per-call observation has incomplete meters and no billing route;
        // the requested mock name cannot replace that evidence with a price.
        let reason = failed
            .fields
            .iter()
            .find(|f| f.key == "cost_unpriced")
            .expect("the unpriced LLM leg is named");
        assert_eq!(
            &reason.value,
            &FieldValue::String("provider_did_not_report_usage".into())
        );
        let calls = failed
            .fields
            .iter()
            .find(|f| f.key == "inference_calls")
            .expect("the failed task preserves its observed calls");
        let FieldValue::String(encoded) = &calls.value else {
            panic!("inference_calls is a JSON string");
        };
        // The durable call projection (`nika/route-identity@1`), not the record.
        let calls: serde_json::Value =
            serde_json::from_str(encoded).expect("durable call observations");
        // The legacy API error reports no new call: do not invent its evidence.
        let [call] = calls.as_array().expect("an array").as_slice() else {
            panic!("one observed call: {calls}");
        };
        assert_eq!(call["usage"]["input_tokens"], 40, "reported meters");
        assert_eq!(call["usage_complete"], false);
        assert!(call["route"].is_null() && call["requested_origin"].is_null());
        assert!(call["pricing"].is_null());
        assert_eq!(call["estimate_known"], false);
        assert_eq!(call["withheld"], serde_json::json!([]));
        // The run totals see the failed task's spend too.
        assert_eq!(outcome.total_cost_usd, Some(0.06));
        assert_eq!(outcome.priced_calls, 1, "the failed attempt debited");
        assert_eq!(
            outcome.unpriced_calls, 1,
            "the observed LLM cost stays unknown"
        );
    }

    #[tokio::test]
    async fn budget_sees_billed_then_failed_spend() {
        // The refuter's exact scenario: without failure-spend threading,
        // a dying loop's dollars were invisible to `--max-cost-usd`.
        let yaml = "nika: w\ntasks:\n  work:\n    agent:\n      prompt: \"burn then die\"\n      tools: [\"nika:jq\"]\n  report:\n    with: { done: \"${{ tasks.work.output }}\" }\n    invoke: { tool: \"nika:jq\", args: { input: \"${{ with.done }}\", expression: \".\" } }\n";
        let (provider, tools) = dying_priced_agent();
        let (outcome, _sink) = run_agent_yaml(yaml, provider, tools, Some(0.05)).await;
        assert!(!outcome.ok);
        assert!(
            outcome.budget_exceeded,
            "the failed task's $0.06 crossed the $0.05 budget — the gate SAW it"
        );
        assert_eq!(outcome.total_cost_usd, Some(0.06));
    }
}

/// E39 10C case F7-A03, verbatim: an agent on a priced model, no token ceiling.
const F7_A03: &str = "nika: \"e10c-f7-a03\"\nmodel: \"mistral/mistral-small-latest\"\npermits: {}\ntasks:\n  a:\n    agent:\n      prompt: \"Say hi\"\n      max_turns: 1\n";
/// E39 10C case F7-A08, verbatim: an infer on a priced model without `max_tokens`.
const F7_A08: &str = "nika: \"e10c-f7-a08\"\nmodel: \"mistral/mistral-small-latest\"\npermits: {}\ntasks:\n  ask:\n    infer:\n      prompt: \"Say hi\"\n";

/// The launch gate of a run under `budget` (the CLI preflight's floor law, no bindings).
fn launch_refusal(yaml: &str, budget: f64) -> Option<nika_runtime::RuntimeError> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    nika_runtime::budget_floor_refusal(&wf, &report, Some(budget), None)
}

/// E39 10C N6 · a zero budget admits zero metered requests. A task that certainly calls a priced
/// model with no token ceiling has a floor above zero whose size nothing bounds, so it refuses
/// NIKA-1709 before the prologue; mock work stays exempt, and a positive budget keeps the warning.
#[test]
fn a_zero_budget_refuses_a_certain_unbounded_priced_call() {
    for (case, yaml, task) in [("F7-A03", F7_A03, "a"), ("F7-A08", F7_A08, "ask")] {
        let refusal = launch_refusal(yaml, 0.0)
            .unwrap_or_else(|| panic!("{case}: a zero budget must refuse before any request"));
        // The messages never print the refusal: it carries the parsed workflow values.
        assert!(
            matches!(refusal, nika_runtime::RuntimeError::BudgetFloor { .. }),
            "{case}: the refusal is not the budget floor"
        );
        assert!(
            refusal.to_string().contains(&format!("`{task}`")),
            "{case}: the refusal does not name its task"
        );
    }
    let mock = F7_A08.replace("mistral/mistral-small-latest", "mock/echo");
    assert!(
        launch_refusal(&mock, 0.0).is_none(),
        "mock work is never metered"
    );
    assert!(
        launch_refusal(F7_A08, 0.01).is_none(),
        "a positive budget keeps the documented warn-and-proceed"
    );
}

/// NIK-152 · the live case of 13 October, replayed through the real parse → check → run chain
/// on recorded Anthropic Messages replies: a `for_each` of three `infer` with a `schema:` on
/// Claude. `nika check` priced it while the run settled every call as « provider did not report
/// usage » (`cost_usd: null` · `cost_unknown_calls: 3`), so `--max-cost-usd` never counted them.
/// The `infer` verb dispatches buffered (never streamed), so the buffered Messages reply is the
/// one replayed here.
mod claude_calls {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use nika_event::EventKind;
    use nika_kernel::http::{
        HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse,
    };
    use nika_kernel::secret::Secret;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
    use nika_types::resource::Value as FieldValue;
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;

    /// Output tokens of every recorded reply.
    const OUTPUT: u64 = 38;

    /// Answers every Messages request with the recorded reply, echoing the model the request
    /// named as Anthropic does, and keeps the bodies it was sent. The `usage` object is the
    /// live reply's of 2026-10-15 (recorded beside the completeness rule in `nika-providers`):
    /// input, output, the cache read and both cache-write lifetimes, the standard tier, global
    /// inference and the standard speed, at `input` input tokens.
    struct Anthropic {
        input: u64,
        sent: Mutex<Vec<serde_json::Value>>,
    }

    impl HttpPostDyn for Anthropic {
        async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
            let sent: serde_json::Value =
                serde_json::from_slice(req.body.as_ref().expect("a body")).expect("json");
            let reply = serde_json::json!({
                "id": "msg_01NIK152", "type": "message", "role": "assistant",
                "model": sent["model"],
                "content": [{"type": "text", "text": "{\"title\":\"Satin\"}"}],
                "stop_reason": "end_turn", "stop_sequence": null,
                "usage": {
                    "input_tokens": self.input,
                    "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0,
                    "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 0},
                    "output_tokens": OUTPUT, "output_tokens_details": {"thinking_tokens": 0},
                    "service_tier": "standard", "inference_geo": "global", "speed": "standard"
                }
            });
            self.sent.lock().expect("sent").push(sent);
            Ok(HttpResponse::new(
                200,
                BTreeMap::new(),
                reply.to_string().into(),
                req.url.clone(),
            ))
        }
        async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
            panic!("the infer verb dispatches buffered, never streamed");
        }
    }

    /// The `proposals` task of the live run: three Claude calls, one at a time, each with a
    /// `schema:` sent as native structured output.
    fn proposals(model: &str) -> String {
        format!(
            "nika: qr-satin\nmodel: anthropic/{model}\npermits: {{}}\ntasks:\n  proposals:\n    for_each: {{ items: [1, 2, 3], max_parallel: 1 }}\n    infer:\n      prompt: \"Propose a title for satin QR code ${{{{ item }}}}\"\n      max_tokens: 256\n      schema:\n        type: object\n        properties: {{ title: {{ type: string }} }}\n        required: [title]\n"
        )
    }

    /// The run of [`proposals`] on `model`, each reply reporting `input` input tokens, under
    /// `budget`. The envelope model is the infer verb's default, as the production composer
    /// hands it (`compose.rs`).
    async fn run(
        model: &str,
        input: u64,
        budget: Option<f64>,
    ) -> (RunOutcome, VecSink, Arc<Anthropic>) {
        let wf = nika_schema::parse(
            &proposals(model),
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "{:?}", report.findings);
        let wire = Arc::new(Anthropic {
            input,
            sent: Mutex::default(),
        });
        let config = ProvidersConfig::new().with_key("anthropic", Secret::new("sk-ant-fixture"));
        let registry = ProviderRegistry::new(Arc::clone(&wire), config);
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(Arc::new(registry), format!("anthropic/{model}")),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default().with_max_cost_usd(budget),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("the run settles");
        (outcome, sink, wire)
    }

    /// One call's price at the model's admission tariff (Anthropic's pricing pages of
    /// 2026-10-15/16, nano-USD per input and output token).
    fn per_call_usd(input: u64, (input_rate, output_rate): (u32, u32)) -> f64 {
        let nano = input * u64::from(input_rate) + OUTPUT * u64::from(output_rate);
        f64::from(u32::try_from(nano).expect("a small fixture amount")) / 1e9
    }

    /// Each call is priced from the usage Anthropic reported, on Claude Haiku 4.5 (the live
    /// case, $1 and $5 per million) and Claude Sonnet 5.5 (its comment, $2 and $10 per
    /// million): the run total is the three calls, and no call stays unknown.
    #[tokio::test]
    async fn a_claude_fan_out_is_priced_from_the_usage_anthropic_reported() {
        for (model, rates) in [
            ("claude-haiku-4-5-20251001", (1_000, 5_000)),
            ("claude-sonnet-5-5", (2_000, 10_000)),
        ] {
            let (outcome, sink, wire) = run(model, 412, None).await;
            assert!(outcome.ok, "{model}: {outcome:?}");
            let sent = wire.sent.lock().expect("sent").clone();
            assert_eq!(sent.len(), 3, "{model}: one request per item");
            for body in &sent {
                assert_eq!(body["model"], model);
                assert_eq!(body["stream"], false, "{model}: a buffered dispatch");
                assert_eq!(
                    body["output_config"]["format"]["type"], "json_schema",
                    "{model}: the schema rides as native structured output"
                );
            }
            assert_eq!(
                (outcome.priced_calls, outcome.unpriced_calls),
                (3, 0),
                "{model}"
            );
            let total = outcome
                .total_cost_usd
                .expect("the Claude calls are metered");
            let expected = 3.0 * per_call_usd(412, rates);
            assert!(
                (total - expected).abs() < 1e-12,
                "{model}: {total} vs {expected}"
            );
            let frame = sink
                .events()
                .iter()
                .find(|e| {
                    e.kind == EventKind::TaskCompleted && e.str_field("task") == Some("proposals")
                })
                .expect("the proposals frame");
            let field = |key: &str| frame.fields.iter().find(|f| f.key == key).map(|f| &f.value);
            assert_eq!(
                field("cost_unknown_calls"),
                Some(&FieldValue::Int(0)),
                "{model}"
            );
            let Some(FieldValue::String(calls)) = field("inference_calls") else {
                panic!("{model}: the frame carries its calls");
            };
            let calls: serde_json::Value = serde_json::from_str(calls).expect("JSON");
            let calls = calls.as_array().expect("an array");
            assert_eq!(calls.len(), 3, "{model}");
            for call in calls {
                assert_eq!(call["usage"]["input_tokens"], 412, "{model}: {call}");
                assert_eq!(call["usage"]["output_tokens"], OUTPUT, "{model}: {call}");
                assert_eq!(call["usage_complete"], true, "{model}: {call}");
                assert_eq!(call["estimate_known"], true, "{model}: {call}");
                assert_eq!(
                    call["pricing"]["kind"], "catalog_estimate_not_invoice",
                    "{model}"
                );
                assert_eq!(
                    call["route"]["origin"], "https://api.anthropic.com:443",
                    "{model}"
                );
            }
        }
    }

    /// `--max-cost-usd` counts the Claude calls. Each reply reports a 100,000-token prompt
    /// ($0.10019 on Claude Haiku 4.5), far above the static floor the run is admitted on
    /// (three `max_tokens` outputs, $0.00384): under a ceiling of one and a half calls, the
    /// second call crosses it and counts, the third is never sent, and the fan-out fails
    /// NIKA-1704 naming the denied iteration.
    #[tokio::test]
    async fn the_run_ceiling_counts_claude_calls() {
        let per_call = per_call_usd(100_000, (1_000, 5_000));
        let (outcome, _sink, wire) =
            run("claude-haiku-4-5-20251001", 100_000, Some(1.5 * per_call)).await;
        assert!(!outcome.ok, "a budget stop is a run failure");
        assert!(outcome.budget_exceeded, "{outcome:?}");
        assert_eq!(
            wire.sent.lock().expect("sent").len(),
            2,
            "the third call never left"
        );
        assert_eq!((outcome.priced_calls, outcome.unpriced_calls), (2, 0));
        let total = outcome.total_cost_usd.expect("metered");
        assert!((total - 2.0 * per_call).abs() < 1e-12, "{total}");
        let error = outcome.records["proposals"]
            .error
            .as_ref()
            .expect("the starved fan-out fails");
        assert_eq!(error.code, "NIKA-1704");
        assert!(
            error.message.contains("1 iteration(s)"),
            "{}",
            error.message
        );
    }
}
