// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Composer-owned harness availability/probes; the verb owns routing.

/// Declared harness, or a zero-sized feature-off witness.
#[cfg(feature = "access-harness")]
pub(crate) type Seat = Option<nika_verb_agent::harness_path::HarnessSeat>;
/// The feature-off twin.
#[cfg(not(feature = "access-harness"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Seat;

/// The access-probe rows the `--access` gate judges (presence only).
pub(crate) fn access_probes() -> Vec<nika_providers::probe::ProviderProbe> {
    nika_providers::probe::collect_provider_probes(&nika_providers::ProviderRegistry::without_http(
        crate::compose::config_from_env(),
    ))
}

/// Read this machine's declaration into a seat — a declared-but-broken
/// adapter REFUSES rather than substitute the native loop (A-4).
#[cfg(feature = "access-harness")]
pub(crate) fn seat_from_env() -> Result<Seat, nika_kernel::HttpError> {
    let wrap = |why: String| nika_kernel::HttpError::Connection {
        reason: format!("harness seat: {why}"),
    };
    let Some(b) = nika_harness::seat_from_env().map_err(wrap)? else {
        return Ok(None);
    };
    let id = nika_harness::declared_adapter_id()
        .ok_or_else(|| wrap("adapter built without a declared id".to_owned()))?;
    nika_verb_agent::harness_path::HarnessSeat::from_backend(std::sync::Arc::new(b))
        .map(|seat| seat.with_access_id(id))
        .map(Some)
        .map_err(wrap)
}

/// Feature-off twin with the same composer-facing `Result` shape.
#[expect(clippy::unnecessary_wraps, reason = "the ON arm fails · shared shape")]
#[cfg(not(feature = "access-harness"))]
pub(crate) const fn seat_from_env() -> Result<Seat, nika_kernel::HttpError> {
    Ok(Seat)
}

/// Declared adapter id for the boot-manifest `harness_seat` stamp (B6).
#[cfg(feature = "access-harness")]
pub(crate) fn declared_id() -> Option<String> {
    nika_harness::declared_adapter_id()
}

/// The feature-off twin (always `None` — no seat exists to name).
#[cfg(not(feature = "access-harness"))]
pub(crate) const fn declared_id() -> Option<String> {
    None
}

#[cfg(all(test, feature = "access-harness"))]
mod tests {
    use std::collections::BTreeMap;
    use std::pin::Pin;
    use std::sync::Arc;

    use nika_kernel::ai::harness::{
        DynAgentBackend, HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome,
        HarnessRequest,
    };
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};
    use nika_types::access::AccessClass;
    use nika_verb_agent::{AgentVerb, harness_path::HarnessSeat};
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;

    use crate::child::{ChildCall, ChildOutcome, ChildRunRefusal, ChildRunner};
    use crate::{DeterministicStamper, EventKind, Runtime, RuntimeConfig, VecSink};

    struct CompletedBackend;

    impl DynAgentBackend for CompletedBackend {
        fn run_agent_boxed(
            &self,
            request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            assert_eq!(
                request.requested_model.as_deref(),
                Some("anthropic/claude-sonnet-4-6"),
                "the envelope default rides to the harness request"
            );
            Box::pin(async {
                let event = HarnessEvent::Completed {
                    outcome: Box::new(
                        HarnessOutcome::new("harness route")
                            .with_observed_model("anthropic/claude-observed"),
                    ),
                };
                Ok(Box::pin(futures_util::stream::iter([Ok(event)])) as HarnessEventStream)
            })
        }
    }

    struct CountingCompletedBackend {
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl DynAgentBackend for CountingCompletedBackend {
        fn run_agent_boxed(
            &self,
            _request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Box::pin(async {
                let event = HarnessEvent::Completed {
                    outcome: Box::new(
                        HarnessOutcome::new("harness route")
                            .with_observed_model("anthropic/claude-observed"),
                    ),
                };
                Ok(Box::pin(futures_util::stream::iter([Ok(event)])) as HarnessEventStream)
            })
        }
    }

    #[tokio::test]
    async fn fan_out_keeps_a_successful_harness_route_when_a_later_lane_fails() -> Result<(), String>
    {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let runtime = harness_runtime(
            MockShell::new(),
            Arc::new(CountingCompletedBackend {
                calls: Arc::clone(&calls),
            }),
            Arc::new(MockProvider::new("mock")),
        );
        let (outcome, events) = drive(
            &runtime,
            "nika: fanout-sibling\ntasks:\n  delegated:\n    for_each:\n      items: [{ prompt: ok }, {}]\n      max_parallel: 1\n      fail_fast: false\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: \"${{ item.prompt }}\" }\n",
        )
        .await?;

        assert!(!outcome.ok, "the second lane fails before dispatch");
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "only the first lane starts a harness effect"
        );
        let failed = events
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the fan-out has no terminal failure".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(
            field(failed, "observed_model").as_deref(),
            Some("anthropic/claude-observed")
        );
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        assert_eq!(
            field(failed, "access_receipt_scope").as_deref(),
            Some("representative"),
            "one nested receipt is a replay guard, not a complete effect list"
        );
        Ok(())
    }

    #[tokio::test]
    async fn failed_task_keeps_the_harness_route_selected_by_its_cleanup() -> Result<(), String> {
        let runtime = harness_runtime(
            MockShell::new().enqueue_fail(7, "main failed"),
            Arc::new(CompletedBackend),
            Arc::new(MockProvider::new("mock")),
        );
        let (outcome, events) = drive(
            &runtime,
            "nika: cleanup-route\npermits: { exec: [false] }\ntasks:\n  main:\n    exec: { command: [false] }\n  main_cleanup:\n    after: { main: unwind }\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: cleanup }\n",
        )
        .await?;

        assert!(!outcome.ok);
        let failed = events
            .iter()
            .find(|event| {
                event.kind == EventKind::TaskFailed
                    && field(event, "task").as_deref() == Some("main")
            })
            .ok_or_else(|| "the producer has no terminal failure".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(
            field(failed, "observed_model").as_deref(),
            Some("anthropic/claude-observed")
        );
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        Ok(())
    }

    fn probe(id: &str, class: AccessClass, serves: &[&str]) -> ProviderProbe {
        ProviderProbe::new(
            id,
            false,
            true,
            "",
            false,
            ProviderReadiness::new(
                true,
                true,
                None,
                None,
                false,
                ExecutionLocus::Loopback,
                class,
            ),
            "",
        )
        .with_serves(serves.iter().map(|s| (*s).to_owned()).collect())
    }

    type HarnessRuntime = Runtime<
        MockShell,
        MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    >;

    fn harness_runtime(
        shell: MockShell,
        backend: Arc<dyn DynAgentBackend>,
        provider: Arc<MockProvider>,
    ) -> HarnessRuntime {
        let tools = Arc::new(MockToolExecutor::new());
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let seat = HarnessSeat::new(backend, "/tmp").with_access_id("claude-agent-acp");
        Runtime::new(
            ExecVerb::new(Arc::new(shell)),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "anthropic/claude-sonnet-4-6",
            ),
            AgentVerb::new(
                provider,
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "anthropic/claude-sonnet-4-6",
            )
            .with_harness_seat(seat),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_access_probes(vec![probe(
            "claude-agent-acp",
            AccessClass::Harness,
            &["anthropic"],
        )])
    }

    async fn drive(
        runtime: &HarnessRuntime,
        yaml: &str,
    ) -> Result<(crate::RunOutcome, Vec<nika_event::Event>), String> {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .map_err(|err| err.to_string())?;
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture checks clean: {report:?}");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .map_err(|err| err.to_string())?;
        Ok((outcome, sink.into_events()))
    }

    fn field(event: &nika_event::Event, key: &str) -> Option<String> {
        event
            .fields
            .iter()
            .find(|field| field.key == key)
            .and_then(|field| match &field.value {
                crate::FieldValue::String(value) => Some(value.clone()),
                _ => None,
            })
    }

    #[tokio::test]
    async fn each_model_executes_its_planned_access_and_receipts_the_harness_adapter()
    -> Result<(), String> {
        let provider = Arc::new(MockProvider::new("mock").enqueue_text("native route"));
        let tools = Arc::new(MockToolExecutor::new());
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let seat =
            HarnessSeat::new(Arc::new(CompletedBackend), "/tmp").with_access_id("claude-agent-acp");
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "openai/gpt-5",
            ),
            AgentVerb::new(
                Arc::clone(&provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "anthropic/claude-sonnet-4-6",
            )
            .with_harness_seat(seat),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_access_probes(vec![
            probe("openai", AccessClass::Api, &[]),
            probe("claude-agent-acp", AccessClass::Harness, &["anthropic"]),
        ]);
        let wf = nika_schema::parse(
            "nika: access-route\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  api:\n    agent: { model: openai/gpt-5, prompt: native }\n  harness:\n    agent: { prompt: delegated }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .map_err(|err| err.to_string())?;
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture checks clean: {report:?}");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .map_err(|err| err.to_string())?;
        assert!(outcome.ok, "terminal events: {:#?}", sink.events());
        assert_eq!(provider.captured_requests().len(), 1, "only api is native");

        let completed = |task: &str| {
            sink.events().iter().find(|event| {
                event.kind == EventKind::TaskCompleted
                    && field(event, "task").as_deref() == Some(task)
            })
        };
        let api = completed("api").ok_or_else(|| "api task did not complete".to_owned())?;
        assert_eq!(field(api, "access").as_deref(), Some("api"));
        assert_eq!(field(api, "billing").as_deref(), Some("api_metered"));
        assert!(field(api, "adapter").is_none());

        let harness =
            completed("harness").ok_or_else(|| "harness task did not complete".to_owned())?;
        assert_eq!(
            field(harness, "model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(
            field(harness, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(
            field(harness, "observed_model").as_deref(),
            Some("anthropic/claude-observed"),
            "the harness observation never overwrites the requested model"
        );
        assert_eq!(field(harness, "provider").as_deref(), Some("anthropic"));
        assert_eq!(field(harness, "access").as_deref(), Some("harness"));
        assert_eq!(field(harness, "billing").as_deref(), Some("unknown"));
        assert_eq!(
            field(harness, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_templated_model_with_an_unsatisfied_pin_refuses_before_native_effects()
    -> Result<(), String> {
        let provider = Arc::new(MockProvider::new("mock").enqueue_text("must not run"));
        let tools = Arc::new(MockToolExecutor::new());
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "openai/gpt-5",
            ),
            AgentVerb::new(
                Arc::clone(&provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "openai/gpt-5",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_access_pin(Some("harness".to_owned()))
        .with_access_probes(vec![probe("openai", AccessClass::Api, &[])]);
        let wf = nika_schema::parse(
            "nika: dynamic-access\ninputs:\n  wanted: { type: string, default: openai/gpt-5 }\ntasks:\n  denied:\n    agent: { model: \"${{ inputs.wanted }}\", prompt: never-run }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .map_err(|err| err.to_string())?;
        let report = nika_check::check(&wf);
        assert!(
            report.is_clean(),
            "templated model is judged at dispatch: {report:?}"
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .map_err(|err| err.to_string())?;
        assert!(!outcome.ok, "the access refusal fails the task");
        assert_eq!(
            provider.captured_requests().len(),
            0,
            "a refused access plan performs zero native provider calls"
        );
        let failed = sink
            .events()
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the refused task has no terminal frame".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("openai/gpt-5")
        );
        assert_eq!(field(failed, "provider").as_deref(), Some("openai"));
        assert!(field(failed, "access").is_none(), "no path was selected");
        Ok(())
    }

    struct RefusingBackend;

    impl DynAgentBackend for RefusingBackend {
        fn run_agent_boxed(
            &self,
            _request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            Box::pin(async {
                Err(HarnessError::Refused {
                    reason: "scripted refusal".to_owned(),
                })
            })
        }
    }

    #[tokio::test]
    async fn a_failed_harness_call_keeps_its_route_receipt() -> Result<(), String> {
        let provider = Arc::new(MockProvider::new("mock"));
        let tools = Arc::new(MockToolExecutor::new());
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let seat =
            HarnessSeat::new(Arc::new(RefusingBackend), "/tmp").with_access_id("claude-agent-acp");
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "anthropic/claude-sonnet-4-6",
            ),
            AgentVerb::new(
                provider,
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "anthropic/claude-sonnet-4-6",
            )
            .with_harness_seat(seat),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_access_probes(vec![probe(
            "claude-agent-acp",
            AccessClass::Harness,
            &["anthropic"],
        )]);
        let wf = nika_schema::parse(
            "nika: failed-receipt\ntasks:\n  delegated:\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: fail }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .map_err(|err| err.to_string())?;
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture checks clean: {report:?}");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .map_err(|err| err.to_string())?;
        assert!(!outcome.ok);
        let failed = sink
            .events()
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the failed harness task has no terminal frame".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(field(failed, "billing").as_deref(), Some("unknown"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        Ok(())
    }

    struct CountingSessionBackend {
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl DynAgentBackend for CountingSessionBackend {
        fn run_agent_boxed(
            &self,
            _request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Box::pin(async {
                Err(HarnessError::Session {
                    reason: "AUTH_REQUIRED after session start".to_owned(),
                })
            })
        }
    }

    struct HangingBackend {
        calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl DynAgentBackend for HangingBackend {
        fn run_agent_boxed(
            &self,
            _request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            self.calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Box::pin(std::future::pending())
        }
    }

    #[tokio::test]
    async fn timeout_after_harness_start_keeps_the_selected_route() -> Result<(), String> {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let runtime = harness_runtime(
            MockShell::new(),
            Arc::new(HangingBackend {
                calls: Arc::clone(&calls),
            }),
            Arc::new(MockProvider::new("mock")),
        );
        let (outcome, events) = drive(
            &runtime,
            "nika: timeout-route\ntasks:\n  delegated:\n    timeout: \"50ms\"\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: hang }\n",
        )
        .await?;

        assert!(!outcome.ok);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "the harness effect started before the timeout won"
        );
        assert_eq!(
            outcome.records["delegated"]
                .error
                .as_ref()
                .map(|error| error.code.as_str()),
            Some("NIKA-TIMEOUT-001")
        );
        let failed = events
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the timed-out task has no terminal frame".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(field(failed, "provider").as_deref(), Some("anthropic"));
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(field(failed, "billing").as_deref(), Some("unknown"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        assert!(
            field(failed, "observed_model").is_none(),
            "the failed ACP session reported no observed identity"
        );
        Ok(())
    }

    struct HarnessFanoutChildRunner {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        provider: Arc<MockProvider>,
    }

    impl ChildRunner for HarnessFanoutChildRunner {
        fn run_child<'a>(
            &'a self,
            _call: ChildCall,
        ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
            Box::pin(async move {
                let runtime = harness_runtime(
                    MockShell::new(),
                    Arc::new(CountingSessionBackend {
                        calls: Arc::clone(&self.calls),
                    }),
                    Arc::clone(&self.provider),
                );
                let (outcome, _events) = drive(
                    &runtime,
                    "nika: child\ntasks:\n  delegated:\n    for_each: { items: [one], max_parallel: 1 }\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: fail }\n",
                )
                .await
                .map_err(|message| ChildRunRefusal {
                    code: "NIKA-COMP-001".to_owned(),
                    message,
                })?;
                let record = outcome
                    .records
                    .get("delegated")
                    .ok_or_else(|| ChildRunRefusal {
                        code: "NIKA-COMP-001".to_owned(),
                        message: "child fan-out produced no task record".to_owned(),
                    })?;
                let error = record.error.as_ref().ok_or_else(|| ChildRunRefusal {
                    code: "NIKA-COMP-001".to_owned(),
                    message: "child fan-out produced no failure".to_owned(),
                })?;
                Ok(ChildOutcome::new(
                    false,
                    BTreeMap::new(),
                    outcome.total_cost_usd,
                    None,
                    Some((error.code.clone(), error.message.clone())),
                    record.access_receipt().cloned(),
                ))
            })
        }
    }

    struct SuccessfulHarnessGrandchildRunner {
        calls: Arc<std::sync::atomic::AtomicUsize>,
        provider: Arc<MockProvider>,
    }

    impl ChildRunner for SuccessfulHarnessGrandchildRunner {
        fn run_child<'a>(
            &'a self,
            _call: ChildCall,
        ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
            Box::pin(async move {
                let runtime = harness_runtime(
                    MockShell::new(),
                    Arc::new(CountingCompletedBackend {
                        calls: Arc::clone(&self.calls),
                    }),
                    Arc::clone(&self.provider),
                );
                let (outcome, _events) = drive(
                    &runtime,
                    "nika: grandchild\ntasks:\n  delegated:\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: done }\n",
                )
                .await
                .map_err(|message| ChildRunRefusal {
                    code: "NIKA-COMP-001".to_owned(),
                    message,
                })?;
                let receipt = outcome
                    .records
                    .get("delegated")
                    .and_then(crate::TaskRecord::access_receipt)
                    .cloned();
                Ok(ChildOutcome::new(
                    outcome.ok,
                    outcome.outputs,
                    outcome.total_cost_usd,
                    None,
                    None,
                    receipt,
                ))
            })
        }
    }

    struct SameWaveNestedRunner {
        harness_calls: Arc<std::sync::atomic::AtomicUsize>,
        provider: Arc<MockProvider>,
    }

    impl ChildRunner for SameWaveNestedRunner {
        fn run_child<'a>(
            &'a self,
            _call: ChildCall,
        ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
            Box::pin(async move {
                let grandchild = Arc::new(SuccessfulHarnessGrandchildRunner {
                    calls: Arc::clone(&self.harness_calls),
                    provider: Arc::clone(&self.provider),
                });
                let runtime = harness_runtime(
                    MockShell::new().enqueue_fail(7, "ordinary failure"),
                    Arc::new(CompletedBackend),
                    Arc::new(MockProvider::new("mock")),
                )
                .with_child_runner(grandchild);
                let (outcome, _events) = drive(
                    &runtime,
                    "nika: child\npermits: { exec: [false] }\ntasks:\n  a_failure:\n    exec: { command: [false] }\n  z_nested:\n    invoke: { workflow: grandchild.nika.yaml }\n",
                )
                .await
                .map_err(|message| ChildRunRefusal {
                    code: "NIKA-COMP-001".to_owned(),
                    message,
                })?;
                let error = outcome
                    .records
                    .get("a_failure")
                    .and_then(|record| record.error.as_ref())
                    .ok_or_else(|| ChildRunRefusal {
                        code: "NIKA-COMP-001".to_owned(),
                        message: "nested child produced no ordinary failure".to_owned(),
                    })?;
                let receipt = outcome
                    .records
                    .values()
                    .filter_map(crate::TaskRecord::access_receipt)
                    .find(|receipt| receipt.selected_harness())
                    .cloned();
                Ok(ChildOutcome::new(
                    false,
                    BTreeMap::new(),
                    outcome.total_cost_usd,
                    None,
                    Some((error.code.clone(), error.message.clone())),
                    receipt,
                ))
            })
        }
    }

    #[tokio::test]
    async fn successful_grandchild_harness_is_not_replayed_by_parent_retry() -> Result<(), String> {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let provider = Arc::new(MockProvider::new("mock").enqueue_text("must not fall through"));
        let runtime = harness_runtime(
            MockShell::new(),
            Arc::new(CompletedBackend),
            Arc::new(MockProvider::new("mock")),
        )
        .with_child_runner(Arc::new(SameWaveNestedRunner {
            harness_calls: Arc::clone(&calls),
            provider: Arc::clone(&provider),
        }));
        let (outcome, events) = drive(
            &runtime,
            "nika: root\ntasks:\n  nested:\n    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-EXEC-001] }\n    invoke: { workflow: child.nika.yaml }\n",
        )
        .await?;

        assert!(!outcome.ok);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "the root must not replay a successful grandchild ACP effect"
        );
        assert!(
            provider.captured_requests().is_empty(),
            "the selected harness never falls through to the native provider"
        );
        assert!(
            events
                .iter()
                .all(|event| event.kind != EventKind::TaskRetrying),
            "the root emits no retry frame"
        );
        let failed = events
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the root has no terminal failure".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(
            field(failed, "observed_model").as_deref(),
            Some("anthropic/claude-observed")
        );
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        Ok(())
    }

    #[tokio::test]
    async fn parent_retry_replays_child_harness_fanout() -> Result<(), String> {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let provider = Arc::new(MockProvider::new("mock").enqueue_text("must not fall through"));
        let runtime = harness_runtime(
            MockShell::new(),
            Arc::new(CompletedBackend),
            Arc::new(MockProvider::new("mock")),
        )
        .with_child_runner(Arc::new(HarnessFanoutChildRunner {
            calls: Arc::clone(&calls),
            provider: Arc::clone(&provider),
        }));
        let (outcome, events) = drive(
            &runtime,
            "nika: parent\ntasks:\n  nested:\n    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-INFER-001] }\n    invoke: { workflow: child.nika.yaml }\n",
        )
        .await?;

        assert!(!outcome.ok);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "the parent must not replay a harness effect hidden by child fan-out"
        );
        assert!(
            provider.captured_requests().is_empty(),
            "the selected harness never falls through to the native provider"
        );
        assert!(
            events
                .iter()
                .all(|event| event.kind != EventKind::TaskRetrying),
            "the parent emits no retry frame"
        );
        let failed = events
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the parent has no terminal failure".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(field(failed, "provider").as_deref(), Some("anthropic"));
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(field(failed, "billing").as_deref(), Some("unknown"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        assert!(
            field(failed, "observed_model").is_none(),
            "the failed ACP session reported no observed identity"
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_started_harness_route_is_terminal_even_under_explicit_retry() -> Result<(), String> {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let provider = Arc::new(MockProvider::new("mock").enqueue_text("must not fall through"));
        let tools = Arc::new(MockToolExecutor::new());
        let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
        let seat = HarnessSeat::new(
            Arc::new(CountingSessionBackend {
                calls: Arc::clone(&calls),
            }),
            "/tmp",
        )
        .with_access_id("claude-agent-acp");
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new())),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "anthropic/claude-sonnet-4-6",
            ),
            AgentVerb::new(
                Arc::clone(&provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "anthropic/claude-sonnet-4-6",
            )
            .with_harness_seat(seat),
            MockClock::new(),
            RuntimeConfig::default(),
        )
        .with_access_probes(vec![probe(
            "claude-agent-acp",
            AccessClass::Harness,
            &["anthropic"],
        )]);
        let wf = nika_schema::parse(
            "nika: terminal-harness\ntasks:\n  delegated:\n    retry: { max_attempts: 2, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-INFER-001] }\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: fail-once }\n",
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .map_err(|err| err.to_string())?;
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "fixture checks clean: {report:?}");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .map_err(|err| err.to_string())?;

        assert!(!outcome.ok, "the ACP session failure is terminal");
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "workflow retry must not replay a started harness effect"
        );
        assert!(
            provider.captured_requests().is_empty(),
            "a selected harness route never falls through to the native provider"
        );
        assert!(
            sink.events()
                .iter()
                .all(|event| event.kind != EventKind::TaskRetrying),
            "a terminal harness failure emits no retry frame"
        );
        let failed = sink
            .events()
            .iter()
            .find(|event| event.kind == EventKind::TaskFailed)
            .ok_or_else(|| "the failed harness task has no terminal frame".to_owned())?;
        assert_eq!(
            field(failed, "requested_model").as_deref(),
            Some("anthropic/claude-sonnet-4-6")
        );
        assert_eq!(field(failed, "provider").as_deref(), Some("anthropic"));
        assert_eq!(field(failed, "access").as_deref(), Some("harness"));
        assert_eq!(
            field(failed, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        assert!(
            field(failed, "observed_model").is_none(),
            "an ACP failure must not forge an unreported observed identity"
        );
        assert!(
            field(failed, "outcome").is_some_and(|outcome| outcome.contains("\"transient\":false")),
            "the terminal receipt must not advertise the ACP effect as retryable"
        );
        Ok(())
    }
}
