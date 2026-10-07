// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Effective model inheritance must reach the delegated consumer, not only routing.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome, HarnessRequest,
};
use nika_kernel_mock::{MockProvider, MockToolDefinitionProvider, MockToolExecutor};
use nika_verb_invoke::InvokeVerb;

use crate::{AgentInput, AgentVerb, harness_path::HarnessSeat};

const DEFAULT: &str = "openai/gpt-6-astra";

#[derive(Default)]
struct CapturingBackend {
    requests: Mutex<Vec<HarnessRequest>>,
    refuse: bool,
}

impl DynAgentBackend for CapturingBackend {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        self.requests.lock().expect("requests").push(request);
        Box::pin(async move {
            if self.refuse {
                return Err(HarnessError::Refused {
                    reason: "the simulated seat does not offer the requested model".to_owned(),
                });
            }
            Ok(
                Box::pin(futures_util::stream::iter([Ok(HarnessEvent::Completed {
                    outcome: Box::new(HarnessOutcome::new("simulated answer")),
                })])) as HarnessEventStream,
            )
        })
    }
}

fn verb(
    provider: Arc<MockProvider>,
    backend: Arc<CapturingBackend>,
) -> AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider> {
    AgentVerb::new(
        provider,
        Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new()))),
        Arc::new(MockToolDefinitionProvider::new()),
        DEFAULT,
    )
    .with_harness_seat(HarnessSeat::new(backend, "/tmp"))
}

#[tokio::test]
async fn the_harness_receives_the_effective_default_or_the_complete_task_override() {
    for (task_model, expected) in [
        (None, DEFAULT),
        (Some("openai/gpt-6-sol"), "openai/gpt-6-sol"),
        (Some("openai/gpt-6-astra[high]"), "openai/gpt-6-astra[high]"),
        (Some("openai/default"), "openai/default"),
    ] {
        let provider = Arc::new(MockProvider::new("mock"));
        let backend = Arc::new(CapturingBackend::default());
        let agent = verb(Arc::clone(&provider), Arc::clone(&backend));
        let mut input = AgentInput::new("test inherited model");
        input.model = task_model.map(str::to_owned);
        let output = agent
            .run(input)
            .await
            .expect("simulated delegated completion");
        let requests = backend.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].requested_model.as_deref(), Some(expected));
        assert_eq!(requests[0].prompt, "test inherited model");
        assert!(
            provider.captured_requests().is_empty(),
            "no native fallback"
        );
        assert!(
            output.model_resolved.is_none(),
            "no invented pricing identity"
        );
        assert!(
            output.model_reported.is_none(),
            "request is not a peer report"
        );
        assert!(output.model_reported_source.is_none());
    }
}

#[tokio::test]
async fn a_refused_inherited_model_never_falls_back_to_the_native_provider() {
    let provider = Arc::new(MockProvider::new("mock"));
    let backend = Arc::new(CapturingBackend {
        refuse: true,
        ..CapturingBackend::default()
    });
    let agent = verb(Arc::clone(&provider), Arc::clone(&backend));
    let error = agent
        .run(AgentInput::new("refuse inherited model"))
        .await
        .expect_err("seat refuses");
    assert!(error.to_string().contains("requested model"));
    let requests = backend.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].requested_model.as_deref(), Some(DEFAULT));
    assert!(
        provider.captured_requests().is_empty(),
        "no silent API fallback"
    );
}
