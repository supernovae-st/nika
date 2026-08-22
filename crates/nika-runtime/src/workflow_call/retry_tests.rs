// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Parent retry boundary for nested runs that selected an ACP harness.

#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_event::{Event, EventKind};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::child::{ChildCall, ChildOutcome, ChildRunRefusal, ChildRunner};
use crate::{AccessReceipt, DeterministicStamper, Runtime, RuntimeConfig, VecSink};

struct CountingChildRunner {
    calls: Arc<AtomicUsize>,
    access_receipt: Option<AccessReceipt>,
}

impl ChildRunner for CountingChildRunner {
    fn run_child<'a>(
        &'a self,
        _call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let access_receipt = self.access_receipt.clone();
        Box::pin(async move {
            Ok(ChildOutcome::new(
                false,
                BTreeMap::new(),
                None,
                None,
                Some((
                    "NIKA-INFER-001".to_owned(),
                    "harness session failed: AUTH_REQUIRED".to_owned(),
                )),
                access_receipt,
            ))
        })
    }
}

fn runtime(
    runner: Arc<dyn ChildRunner>,
) -> Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
> {
    runtime_with_shell(runner, MockShell::new())
}

fn runtime_with_shell(
    runner: Arc<dyn ChildRunner>,
    shell: MockShell,
) -> Runtime<
    MockShell,
    MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
> {
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    Runtime::new(
        ExecVerb::new(Arc::new(shell)),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(
            Arc::new(nika_providers::ProviderRegistry::without_http(
                nika_providers::ProvidersConfig::new(),
            )),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    )
    .with_child_runner(runner)
}

fn field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .and_then(|field| {
            if let nika_types::resource::Value::String(value) = &field.value {
                Some(value.as_str())
            } else {
                None
            }
        })
}

async fn run(access_receipt: Option<AccessReceipt>) -> (usize, Vec<Event>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = Arc::new(CountingChildRunner {
        calls: Arc::clone(&calls),
        access_receipt,
    });
    let wf = nika_schema::parse(
        "nika: parent\ntasks:\n  nested:\n    invoke: { workflow: ./child.nika.yaml }\n    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-INFER-001] }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks clean: {report:?}");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime(runner)
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("parent run settles");
    assert!(!outcome.ok);
    (calls.load(Ordering::Relaxed), sink.into_events())
}

#[tokio::test]
async fn parent_on_codes_cannot_replay_a_child_harness_effect() {
    let receipt = AccessReceipt::harness(
        "anthropic/claude-sonnet-4-6",
        "anthropic",
        "claude-agent-acp",
    )
    .with_observed_model("anthropic/claude-observed");
    let (calls, events) = run(Some(receipt)).await;
    assert_eq!(calls, 1, "the parent must not replay the child ACP effect");
    assert!(
        events
            .iter()
            .all(|event| event.kind != EventKind::TaskRetrying),
        "the parent emits no retry frame"
    );
    let failed = events
        .iter()
        .find(|event| event.kind == EventKind::TaskFailed)
        .expect("parent terminal failure");
    assert_eq!(
        field(failed, "requested_model"),
        Some("anthropic/claude-sonnet-4-6")
    );
    assert_eq!(
        field(failed, "observed_model"),
        Some("anthropic/claude-observed")
    );
    assert_eq!(field(failed, "access"), Some("harness"));
    assert_eq!(field(failed, "adapter"), Some("claude-agent-acp"));
    assert_eq!(
        field(failed, "access_receipt_scope"),
        Some("representative")
    );
}

#[tokio::test]
async fn parent_on_codes_still_retries_an_ordinary_child_failure() {
    let (calls, events) = run(None).await;
    assert_eq!(calls, 2, "non-ACP child retry behavior stays authored");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == EventKind::TaskRetrying)
            .count(),
        1
    );
}

struct SuccessfulReceiptRunner {
    calls: Arc<AtomicUsize>,
}

impl ChildRunner for SuccessfulReceiptRunner {
    fn run_child<'a>(
        &'a self,
        _call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {
            Ok(ChildOutcome::new(
                true,
                BTreeMap::from([("answer".to_owned(), serde_json::json!(42))]),
                None,
                None,
                None,
                Some(
                    AccessReceipt::harness(
                        "anthropic/claude-sonnet-4-6",
                        "anthropic",
                        "claude-agent-acp",
                    )
                    .with_observed_model("anthropic/claude-observed"),
                ),
            ))
        })
    }
}

#[tokio::test]
async fn successful_child_receipt_survives_returns_failure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = Arc::new(SuccessfulReceiptRunner {
        calls: Arc::clone(&calls),
    });
    let wf = nika_schema::parse(
        "nika: returns-boundary\ntasks:\n  nested:\n    invoke: { workflow: child.nika.yaml }\n    returns: string\n    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-TYPE-101] }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks clean: {report:?}");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime(runner)
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("parent run settles");

    assert!(!outcome.ok);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(
        sink.events()
            .iter()
            .all(|event| event.kind != EventKind::TaskRetrying)
    );
    let failed = sink
        .events()
        .iter()
        .find(|event| event.kind == EventKind::TaskFailed)
        .expect("parent terminal failure");
    assert_eq!(field(failed, "access"), Some("harness"));
    assert_eq!(field(failed, "adapter"), Some("claude-agent-acp"));
    assert_eq!(
        field(failed, "access_receipt_scope"),
        Some("representative")
    );
}

#[tokio::test]
async fn successful_workflow_cleanup_receipt_guards_the_failed_task() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runner = Arc::new(SuccessfulReceiptRunner {
        calls: Arc::clone(&calls),
    });
    let wf = nika_schema::parse(
        "nika: cleanup-boundary\npermits: { exec: [false] }\ntasks:\n  main:\n    exec: { command: [false] }\n  cleanup:\n    after: { main: unwind }\n    invoke: { workflow: cleanup.nika.yaml }\n",
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks clean: {report:?}");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime_with_shell(runner, MockShell::new().enqueue_fail(7, "main failed"))
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("parent run settles");

    assert!(!outcome.ok);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let failed = sink
        .events()
        .iter()
        .find(|event| event.kind == EventKind::TaskFailed && field(event, "task") == Some("main"))
        .expect("producer terminal failure");
    assert_eq!(field(failed, "access"), Some("harness"));
    assert_eq!(field(failed, "adapter"), Some("claude-agent-acp"));
}
