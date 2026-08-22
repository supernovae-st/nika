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

    use crate::{DeterministicStamper, EventKind, Runtime, RuntimeConfig, VecSink};

    struct CompletedBackend;

    impl DynAgentBackend for CompletedBackend {
        fn run_agent_boxed(
            &self,
            _request: HarnessRequest,
        ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>>
        {
            Box::pin(async {
                let event = HarnessEvent::Completed {
                    outcome: Box::new(HarnessOutcome::new("harness route")),
                };
                Ok(Box::pin(futures_util::stream::iter([Ok(event)])) as HarnessEventStream)
            })
        }
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
                "openai/gpt-5",
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
            "nika: access-route\ntasks:\n  api:\n    agent: { model: openai/gpt-5, prompt: native }\n  harness:\n    agent: { model: anthropic/claude-sonnet-4-6, prompt: delegated }\n",
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
        assert!(outcome.ok);
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
        assert_eq!(field(harness, "provider").as_deref(), Some("anthropic"));
        assert_eq!(field(harness, "access").as_deref(), Some("harness"));
        assert_eq!(field(harness, "billing").as_deref(), Some("unknown"));
        assert_eq!(
            field(harness, "adapter").as_deref(),
            Some("claude-agent-acp")
        );
        Ok(())
    }
}
