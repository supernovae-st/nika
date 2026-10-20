// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Operator control is distinct from authored recovery, including on resume.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nika_clock::SystemClock;
use nika_event::EventKind;
use nika_kernel::tool_executor::{StopOutcome, ToolCall, ToolErrorMeta, ToolResult};
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::resume::{PriorSuccess, ResumePlan, fields};
use nika_runtime::{
    DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, TerminalCause, VecSink,
};
use nika_types::cancel::CancelCtx;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::{InvokeVerb, McpPlane};

#[derive(Clone, Copy)]
enum Reply {
    OperatorStopped,
    CompletedAfterStop,
    OrdinaryErrorAfterStop,
    CancelCodeAfterStop,
    StoppedWithoutOperator,
    Timeout,
    Success,
}

fn source(on_error: &str, timeout: &str) -> String {
    format!(
        "nika: w\npermits:\n  tools: [\"mcp:s/work\"]\ntasks:\n  a:\n{timeout}{on_error}    invoke: {{ tool: \"mcp:s/work\" }}\n"
    )
}

async fn run_leg(
    source: &str,
    reply: Reply,
    cancel: CancelCtx,
    calls: Arc<Mutex<Vec<String>>>,
    plan: ResumePlan,
) -> (RunOutcome, VecSink) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{:?}", report.findings);
    let operator = cancel.clone();
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    invoke.install_mcp_plane(McpPlane::stoppable(
        Arc::new(move |call: ToolCall, stop: &CancelCtx| {
            calls.lock().expect("calls").push(call.id.to_string());
            let _ = stop.begin();
            match reply {
                Reply::OperatorStopped => {
                    operator.cancel();
                    assert!(stop.is_cancelled());
                    Ok(ToolResult::stopped(&call, StopOutcome::Unknown))
                }
                Reply::CompletedAfterStop => {
                    operator.cancel();
                    Ok(ToolResult::success(call.id.to_string(), "finished"))
                }
                Reply::OrdinaryErrorAfterStop => {
                    operator.cancel();
                    Ok(ToolResult::error(call.id.to_string(), "ordinary error"))
                }
                Reply::CancelCodeAfterStop => {
                    operator.cancel();
                    Ok(
                        ToolResult::error(call.id.to_string(), "NIKA-CANCEL-001").with_error_meta(
                            ToolErrorMeta::new(Some("NIKA-CANCEL-001".to_owned()), false),
                        ),
                    )
                }
                Reply::StoppedWithoutOperator => {
                    assert!(!operator.is_cancelled());
                    Ok(ToolResult::stopped(&call, StopOutcome::Unknown))
                }
                Reply::Timeout => {
                    let began = Instant::now();
                    while !stop.is_cancelled() && began.elapsed() < Duration::from_secs(5) {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(ToolResult::success(call.id.to_string(), "too late"))
                }
                Reply::Success => Ok(ToolResult::success(call.id.to_string(), "fresh")),
            }
        }),
        Arc::new(Vec::new),
    ));
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
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
        SystemClock,
        RuntimeConfig::default(),
    )
    .with_cancel(cancel)
    .with_resume_plan(plan);
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut DeterministicStamper::new(), &mut sink)
        .await
        .expect("the leg settles");
    (outcome, sink)
}

fn plan_from(sink: &VecSink) -> ResumePlan {
    sink.events()
        .iter()
        .filter(|event| event.kind == EventKind::TaskCompleted)
        .filter_map(|event| {
            Some((
                event.str_field("task")?.to_owned(),
                PriorSuccess::new(
                    event.str_field(fields::DEF_HASH)?.to_owned(),
                    event.str_field(fields::INPUT_HASH)?.to_owned(),
                    serde_json::from_str(event.str_field(fields::OUTPUT)?).expect("output JSON"),
                ),
            ))
        })
        .collect()
}

const RECOVER: &str = "    on_error: { recover: \"fallback\" }\n";

#[tokio::test]
async fn genuine_operator_control_bypasses_error_armor_without_a_success_trio() {
    for on_error in ["", "    on_error: { skip: true }\n", RECOVER] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (run, sink) = run_leg(
            &source(on_error, ""),
            Reply::OperatorStopped,
            CancelCtx::new(),
            Arc::clone(&calls),
            ResumePlan::new(),
        )
        .await;
        assert!(run.cancelled);
        assert!(!run.ok);
        let record = &run.records["a"];
        assert_eq!(record.status, TaskStatus::Cancelled);
        assert_eq!(record.cause, TerminalCause::Operator);
        assert_eq!(record.output, serde_json::Value::Null);
        assert!(record.error.is_none());
        assert!(record.recovered_from.is_none());
        assert!(record.attempts.is_none());
        let terminal = sink
            .events()
            .iter()
            .filter(|event| {
                event.str_field("task") == Some("a")
                    && matches!(
                        event.kind,
                        EventKind::TaskCompleted
                            | EventKind::TaskFailed
                            | EventKind::TaskSkipped
                            | EventKind::TaskCancelled
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].kind, EventKind::TaskCancelled);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                terminal[0].str_field("outcome").expect("outcome")
            )
            .expect("JSON"),
            serde_json::json!({"class": "cancelled", "cause": "operator", "payload": {"reason": "operator"}}),
        );
        for key in [fields::DEF_HASH, fields::INPUT_HASH, fields::OUTPUT] {
            assert!(terminal[0].str_field(key).is_none(), "{key}");
        }
        assert_eq!(
            run.settlement.stopped_calls,
            vec![nika_event::settlement::StoppedCall::new(
                "a",
                "mcp:s/work",
                calls.lock().expect("calls")[0].clone(),
                nika_event::settlement::StopOutcome::Unknown,
            )]
        );
        assert!(plan_from(&sink).is_empty());
    }
}

#[tokio::test]
async fn an_operator_stopped_leg_executes_again_instead_of_cache_hitting() {
    let wf = source(RECOVER, "");
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (_, first) = run_leg(
        &wf,
        Reply::OperatorStopped,
        CancelCtx::new(),
        Arc::clone(&calls),
        ResumePlan::new(),
    )
    .await;
    let plan = plan_from(&first);
    assert!(plan.is_empty());
    let (resumed, _) = run_leg(
        &wf,
        Reply::Success,
        CancelCtx::new(),
        Arc::clone(&calls),
        plan,
    )
    .await;
    assert!(resumed.ok);
    assert!(resumed.cache_hits.is_empty());
    assert_eq!(resumed.records["a"].output, serde_json::json!("fresh"));
    assert_eq!(calls.lock().expect("calls").len(), 2);
}

#[tokio::test]
async fn ordinary_errors_and_code_strings_remain_recoverable_after_stop() {
    for reply in [Reply::OrdinaryErrorAfterStop, Reply::CancelCodeAfterStop] {
        let (run, sink) = run_leg(
            &source(RECOVER, ""),
            reply,
            CancelCtx::new(),
            Arc::new(Mutex::new(Vec::new())),
            ResumePlan::new(),
        )
        .await;
        assert!(run.cancelled);
        assert_eq!(run.records["a"].status, TaskStatus::Success);
        assert_eq!(run.records["a"].cause, TerminalCause::Recovered);
        assert_eq!(run.records["a"].output, serde_json::json!("fallback"));
        assert_eq!(plan_from(&sink).len(), 1);
    }
}

#[tokio::test]
async fn ordinary_error_skip_remains_valid_during_a_run_stop() {
    let (run, _) = run_leg(
        &source("    on_error: { skip: true }\n", ""),
        Reply::OrdinaryErrorAfterStop,
        CancelCtx::new(),
        Arc::new(Mutex::new(Vec::new())),
        ResumePlan::new(),
    )
    .await;
    assert!(run.cancelled);
    assert_eq!(run.records["a"].status, TaskStatus::Skipped);
    assert_eq!(run.records["a"].cause, TerminalCause::ErrorSkip);
    assert_eq!(
        run.records["a"].error.as_ref().expect("error").code,
        "NIKA-451"
    );
}

#[tokio::test]
async fn genuinely_completed_inflight_success_keeps_its_checkpoint() {
    let (run, sink) = run_leg(
        &source("", ""),
        Reply::CompletedAfterStop,
        CancelCtx::new(),
        Arc::new(Mutex::new(Vec::new())),
        ResumePlan::new(),
    )
    .await;
    assert!(run.cancelled);
    assert_eq!(run.records["a"].status, TaskStatus::Success);
    assert_eq!(run.records["a"].cause, TerminalCause::Normal);
    assert_eq!(run.records["a"].output, serde_json::json!("finished"));
    assert_eq!(plan_from(&sink).len(), 1);
}

#[tokio::test]
async fn timeout_and_nonoperator_recovered_successes_still_cache_hit() {
    for (reply, timeout, code) in [
        (
            Reply::Timeout,
            "    timeout: \"100ms\"\n",
            "NIKA-TIMEOUT-001",
        ),
        (Reply::StoppedWithoutOperator, "", "NIKA-CANCEL-001"),
    ] {
        let wf = source(RECOVER, timeout);
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (first, sink) = run_leg(
            &wf,
            reply,
            CancelCtx::new(),
            Arc::clone(&calls),
            ResumePlan::new(),
        )
        .await;
        assert!(first.ok);
        assert!(!first.cancelled);
        assert_eq!(first.records["a"].cause, TerminalCause::Recovered);
        assert_eq!(
            first.records["a"]
                .recovered_from
                .as_ref()
                .expect("original")
                .code,
            code
        );
        let plan = plan_from(&sink);
        assert_eq!(plan.len(), 1);
        let (resumed, _) = run_leg(
            &wf,
            Reply::Success,
            CancelCtx::new(),
            Arc::clone(&calls),
            plan,
        )
        .await;
        assert!(resumed.ok);
        assert_eq!(resumed.cache_hits, vec!["a".to_owned()]);
        assert_eq!(resumed.records["a"].output, serde_json::json!("fallback"));
        assert_eq!(calls.lock().expect("calls").len(), 1);
    }
}
