// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authored access selection on the `agent` verb: the native effort
//! reaches the harness session request verbatim (the session applies and
//! reads it back), a session refusal keeps its typed access code, every
//! native-loop turn carries the exact level, and a word the request cannot
//! carry refuses before any model call.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use nika_error::traits::NikaErrorCode as _;
use nika_kernel::ai::harness::{
    DynAgentBackend, HarnessError, HarnessEvent, HarnessEventStream, HarnessOutcome,
    HarnessRequest, HarnessSelection, ModelProvenance,
};
use nika_kernel::ai::provider::ReasoningEffort;
use nika_kernel_mock::{MockProvider, MockToolDefinitionProvider, MockToolExecutor};
use nika_types::access::{AccessFallback, AccessProtocol, AccessRequirement};
use nika_verb_invoke::InvokeVerb;
use serde_json::json;

use crate::{AgentInput, AgentVerb, VerbAgentError, harness_path::HarnessSeat};

const MODEL: &str = "openai/gpt-6-astra";

/// A seat that captures its request and answers as the ACP client would
/// after configuring the session (or refuses, as it would before a prompt).
struct ConfiguredSeat {
    requests: Mutex<Vec<HarnessRequest>>,
    refusal: Option<String>,
    /// What the agent moved during the turn (`model=<v>` · `effort=<v>`).
    moved: Vec<String>,
}

impl DynAgentBackend for ConfiguredSeat {
    fn run_agent_boxed(
        &self,
        request: HarnessRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HarnessEventStream, HarnessError>> + Send + '_>> {
        let effort = request.requested_effort.clone();
        self.requests.lock().expect("requests").push(request);
        Box::pin(async move {
            if let Some(reason) = self.refusal.clone() {
                return Err(HarnessError::Refused { reason });
            }
            let mut outcome = HarnessOutcome::new("configured answer");
            outcome.observed_model = Some("gpt-6-astra".into());
            outcome.observed_model_source = Some(ModelProvenance::ConfirmedSelection);
            let mut selection = HarnessSelection::default();
            selection.model_option = Some("model".into());
            selection.transmitted_model = Some("gpt-6-astra".into());
            selection.effort_option = effort.as_ref().map(|_| "reasoning_effort".to_owned());
            selection.transmitted_effort.clone_from(&effort);
            selection.configured_effort = effort;
            selection.configured_effort_source = Some(ModelProvenance::ConfirmedSelection);
            selection.changed_mid_turn.clone_from(&self.moved);
            outcome.selection = selection;
            Ok(
                Box::pin(futures_util::stream::iter([Ok(HarnessEvent::Completed {
                    outcome: Box::new(outcome),
                })])) as HarnessEventStream,
            )
        })
    }
}

fn seated(
    refusal: Option<&str>,
) -> (
    AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider>,
    Arc<ConfiguredSeat>,
) {
    seated_moving(refusal, &[])
}

fn seated_moving(
    refusal: Option<&str>,
    moved: &[&str],
) -> (
    AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider>,
    Arc<ConfiguredSeat>,
) {
    let seat = Arc::new(ConfiguredSeat {
        requests: Mutex::new(Vec::new()),
        refusal: refusal.map(str::to_owned),
        moved: moved.iter().map(|m| (*m).to_owned()).collect(),
    });
    let verb = AgentVerb::new(
        Arc::new(MockProvider::new("mock")),
        Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new()))),
        Arc::new(MockToolDefinitionProvider::new()),
        MODEL,
    )
    .with_harness_seat(HarnessSeat::new(
        Arc::clone(&seat) as Arc<dyn DynAgentBackend>,
        "/tmp",
    ));
    (verb, seat)
}

fn codex(effort: &str) -> AccessRequirement {
    AccessRequirement::new()
        .with_via(Some("codex".into()))
        .with_protocol(Some(AccessProtocol::Acp))
        .with_fallback(Some(AccessFallback::None))
        .with_effort(Some(effort.into()))
}

#[tokio::test]
async fn the_native_effort_reaches_the_session_request_and_the_receipt() {
    let (verb, seat) = seated(None);
    let out = verb
        .run(AgentInput::new("summarise").with_requirement(Some(&codex("xhigh"))))
        .await
        .expect("configured session answers");
    let requests = seat.requests.lock().expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].requested_effort.as_deref(),
        Some("xhigh"),
        "verbatim"
    );
    assert_eq!(requests[0].requested_model.as_deref(), Some(MODEL));
    let receipt = out
        .selection
        .expect("a requirement yields a receipt")
        .to_json();
    assert_eq!(
        receipt,
        json!({
            "schema": "nika/access-selection@1",
            "protocol": "acp",
            "model": {"requested": MODEL, "option": "model", "transmitted": "gpt-6-astra",
                "configured": "gpt-6-astra", "configured_source": "confirmed_selection"},
            "effort": {"requested": "xhigh", "option": "reasoning_effort", "transmitted": "xhigh",
                "configured": "xhigh", "configured_source": "confirmed_selection"},
            "responder": {"model": null, "evidence": "unknown"}
        })
    );
}

#[tokio::test]
async fn a_session_refusal_keeps_its_typed_access_code_under_a_declaration() {
    let why = "the harness offers no reasoning effort `xhigh` for model `gpt-6-astra` — it offers: low · medium";
    let (verb, _) = seated(Some(why));
    let err = verb
        .run(AgentInput::new("summarise").with_requirement(Some(&codex("xhigh"))))
        .await
        .expect_err("refused before the prompt");
    assert!(matches!(err, VerbAgentError::Harness { .. }), "{err:?}");
    assert_eq!(err.spec_code(), "NIKA-1805");
    assert!(err.to_string().contains("low · medium"), "{err}");
    let (verb, _) = seated(Some(why));
    let legacy = verb
        .run(AgentInput::new("summarise"))
        .await
        .expect_err("refused");
    assert_eq!(
        legacy.spec_code(),
        "NIKA-INFER-001",
        "undeclared runs keep their class"
    );
}

#[tokio::test]
async fn without_a_requirement_no_effort_is_asked_and_no_receipt_rides() {
    let (verb, seat) = seated(None);
    let out = verb
        .run(AgentInput::new("summarise"))
        .await
        .expect("answers");
    assert_eq!(
        seat.requests.lock().expect("requests")[0].requested_effort,
        None
    );
    assert!(out.selection.is_none());
}

/// `fallback: none` holds through the turn: a model the agent moved after
/// it was applied refuses the answer (NIKA-1805); without a declaration the
/// same move is no refusal.
#[tokio::test]
async fn a_model_moved_mid_turn_refuses_under_a_declaration() {
    let (verb, _) = seated_moving(None, &["model=gpt-5.4"]);
    let err = verb
        .run(AgentInput::new("summarise").with_requirement(Some(&codex("xhigh"))))
        .await
        .expect_err("not produced under the selection");
    assert!(matches!(err, VerbAgentError::Harness { .. }), "{err:?}");
    assert_eq!(err.spec_code(), "NIKA-1805");
    let text = err.to_string();
    assert!(text.contains("model=gpt-5.4"), "{text}");
    assert!(text.contains("an explicit selection is exact"), "{text}");
    let (verb, _) = seated_moving(None, &["model=gpt-5.4"]);
    verb.run(AgentInput::new("summarise"))
        .await
        .expect("an undeclared run keeps its answer");
}

fn native(
    provider: Arc<MockProvider>,
) -> AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider> {
    AgentVerb::new(
        provider,
        Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new()))),
        Arc::new(MockToolDefinitionProvider::new()),
        "deepseek/deepseek-flash",
    )
}

#[tokio::test]
async fn every_native_turn_carries_the_exact_declared_level() {
    let provider = Arc::new(MockProvider::new("mock").enqueue_text("done"));
    let requirement = AccessRequirement::new().with_effort(Some("high".into()));
    let out = native(Arc::clone(&provider))
        .run(AgentInput::new("plan").with_requirement(Some(&requirement)))
        .await
        .expect("one turn");
    let sent = provider.captured_requests();
    assert!(!sent.is_empty());
    assert!(
        sent.iter()
            .all(|r| r.reasoning_effort == Some(ReasoningEffort::High)),
        "{:?}",
        sent.iter().map(|r| r.reasoning_effort).collect::<Vec<_>>()
    );
    let receipt = out.selection.expect("receipt").to_json();
    assert_eq!(receipt["protocol"], "api");
    assert_eq!(receipt["effort"]["transmitted"], "high");
    assert_eq!(receipt["effort"]["option"], "reasoning_effort");
}

#[tokio::test]
async fn a_word_the_request_cannot_carry_refuses_before_any_model_call() {
    let provider = Arc::new(MockProvider::new("mock").enqueue_text("never"));
    let requirement = AccessRequirement::new().with_effort(Some("xhigh".into()));
    let err = native(Arc::clone(&provider))
        .run(AgentInput::new("plan").with_requirement(Some(&requirement)))
        .await
        .expect_err("no alias");
    assert!(
        matches!(
            err,
            VerbAgentError::InvalidParam {
                param: "reasoning_effort",
                ..
            }
        ),
        "{err:?}"
    );
    assert!(provider.captured_requests().is_empty(), "zero model calls");
}
