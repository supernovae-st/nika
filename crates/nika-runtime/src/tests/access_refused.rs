// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A chosen seat that fails at the call is a TYPED refusal on the
//! terminal frame — and never a second path's call.

use std::collections::BTreeMap;
use std::sync::Arc;

use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::plan::{ExecutionAccessPlan, LaneVerdict, ResolvedLane};
use nika_types::access::{
    AccessClass, AccessPlan, AccessRejection, BillingClass, RejectionDimension, RejectionLayer,
};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

type SeatRuntime = Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

/// The verbs' default model is the composer's job in production (the
/// effective envelope `model:`); the fixture sets it by hand.
fn runtime(default_model: &str) -> SeatRuntime {
    runtime_with_agent(default_model, MockProvider::new("mock"))
}

fn runtime_with_agent(default_model: &str, agent: MockProvider) -> SeatRuntime {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            default_model,
        ),
        AgentVerb::new(
            Arc::new(agent),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            default_model,
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
}

fn field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            FieldValue::String(value) => Some(value.as_str()),
            _ => None,
        })
}

fn int_field(event: &Event, key: &str) -> Option<i64> {
    event
        .fields
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| match &kv.value {
            FieldValue::Int(value) => Some(*value),
            _ => None,
        })
}

/// The frozen plan seats `gemini-cli` for the lane and records the api
/// path it outranked — the shape a PROVEN seat would leave. The seat is
/// not infer-grade, so the call refuses before any spawn: a seat that
/// fails on its first call. The terminal frame carries `access_refused`
/// (the seat · the seat's own witness · the next ready path · the flag
/// that pins it), the note names the seat lane, no `task_completed`
/// exists and the run counted ZERO calls — the mock provider beside the
/// seat was never dialed. Revert `push_access_refused_field`'s push and
/// the field vanishes; make the harness arm fall through to
/// `self.infer.run` and `unpriced_calls` reads 1.
#[tokio::test]
async fn a_seat_that_fails_at_the_call_is_a_typed_refusal_and_never_a_second_call() {
    const MODEL: &str = "gemini/gemini-2.5-flash";
    let yaml = format!(
        "nika: seat-refusal\nmodel: {MODEL}\npermits: {{}}\ntasks:\n  ask:\n    infer:\n      \
         prompt: \"hi\"\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let lane = AccessPlan::new(
        MODEL,
        "gemini",
        "gemini-cli",
        AccessClass::Harness,
        BillingClass::Unknown,
        false,
        Vec::new(),
    )
    .with_outranked(vec![AccessRejection::new(
        "gemini",
        RejectionDimension::Outranked,
        RejectionLayer::Access,
        "ready · ranked below `gemini-cli` (harness outranks api)",
    )]);
    let plan = ExecutionAccessPlan::new(
        BTreeMap::from([(
            MODEL.to_owned(),
            LaneVerdict::Admitted(ResolvedLane::new(lane, 2)),
        )]),
        None,
        Some("gemini-cli".to_owned()),
        None,
    );
    let runtime = runtime(MODEL).with_access_plan(plan);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    assert!(!outcome.ok, "the seat's failure is the run's failure");
    let events = sink.events();
    assert!(
        !events.iter().any(|e| e.kind == EventKind::TaskCompleted),
        "no path answered: {events:?}"
    );
    let failed: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == EventKind::TaskFailed)
        .collect();
    assert_eq!(failed.len(), 1, "exactly one terminal: {events:?}");
    let failed = failed[0];
    assert_eq!(field(failed, "note"), Some("infer · seat gemini-cli"));
    assert_eq!(field(failed, "access"), Some("harness"));
    assert_eq!(field(failed, "access_id"), Some("gemini-cli"));
    let refused: serde_json::Value =
        serde_json::from_str(field(failed, "access_refused").expect("the typed refusal rides"))
            .expect("compact JSON");
    assert_eq!(refused["seat"], "gemini-cli");
    assert!(
        refused["witness"]
            .as_str()
            .is_some_and(|w| w.contains("not infer-grade")),
        "the seat's own words: {refused}"
    );
    assert_eq!(refused["next_ready"], "gemini");
    assert_eq!(refused["pin"], "--access gemini");
    let settled = events
        .iter()
        .find(|e| e.kind == EventKind::WorkflowFailed)
        .expect("workflow_failed");
    assert_eq!(int_field(settled, "priced_calls"), Some(0));
    assert_eq!(
        int_field(settled, "unpriced_calls"),
        Some(0),
        "the mock provider beside the seat was never dialed"
    );
}

fn seated_gemini_plan(model: &str) -> ExecutionAccessPlan {
    let lane = AccessPlan::new(
        model,
        "gemini",
        "gemini-cli",
        AccessClass::Harness,
        BillingClass::Unknown,
        false,
        Vec::new(),
    )
    .with_outranked(vec![AccessRejection::new(
        "gemini",
        RejectionDimension::Outranked,
        RejectionLayer::Access,
        "ready · ranked below `gemini-cli` (harness outranks api)",
    )]);
    ExecutionAccessPlan::new(
        BTreeMap::from([(
            model.to_owned(),
            LaneVerdict::Admitted(ResolvedLane::new(lane, 2)),
        )]),
        None,
        Some("gemini-cli".to_owned()),
        None,
    )
}

/// A plan-level seat does not make every later agent failure a pin.
/// Schema (and max-turns / tool) after the native loop ran must not
/// teach `--access gemini`.
#[tokio::test]
async fn an_agent_schema_failure_after_a_plan_seat_is_not_a_typed_refusal() {
    const MODEL: &str = "gemini/gemini-2.5-flash";
    let yaml = format!(
        "nika: seat-schema\nmodel: {MODEL}\npermits: {{}}\ntasks:\n  ask:\n    agent:\n      \
         prompt: \"hi\"\n      schema:\n        type: object\n        properties:\n          n:\n            type: integer\n        required: [n]\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "{report:?}");
    let runtime = runtime_with_agent(MODEL, MockProvider::new("mock").enqueue_text("not-json"))
        .with_access_plan(seated_gemini_plan(MODEL));
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    assert!(!outcome.ok, "schema failure is the run's failure");
    let events = sink.events();
    let failed: Vec<&Event> = events
        .iter()
        .filter(|e| e.kind == EventKind::TaskFailed)
        .collect();
    assert_eq!(failed.len(), 1, "exactly one terminal: {events:?}");
    let failed = failed[0];
    assert!(
        field(failed, "access_refused").is_none(),
        "schema after a plan seat is not a pin: {failed:?}"
    );
}
