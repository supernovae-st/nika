// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Helpers the compile suites share: one hermetic generative provider that returns a fixed
//! text and counts its calls, the bounded authoring policy, the question keys of an outcome.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileRequest};
use nika_compile_cognition::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};

pub(crate) struct Provider {
    pub(crate) text: String,
    pub(crate) calls: AtomicU32,
}
impl Provider {
    pub(crate) fn new(plan: impl std::fmt::Display) -> Self {
        Self {
            text: plan.to_string(),
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Provider {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(request.max_tokens, Some(1024));
        assert_eq!(request.timeout, Some(Duration::from_secs(2)));
        assert!(request.tools.is_empty());
        assert!(request.temperature.is_none());
        assert!(request.extra.params.is_empty());
        Ok(InferResponse::new(
            vec![ContentBlock::Text {
                text: self.text.clone(),
            }],
            TokenUsage::new(120, 90),
            StopReason::EndTurn,
        ))
    }
}
pub(crate) fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 1024, Duration::from_secs(2))
}
pub(crate) fn keys(out: &nika_compile::CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

/// A clause the deterministic reader cannot consume ("harmonise le ton") forces COLD.
pub(crate) const INTENT: &str = "Pour chaque demande, consulte le client, classe le problème, puis harmonise le ton de la réponse. Demande un accord humain avant le remboursement.";
pub(crate) fn plan() -> Value {
    json!({"steps":[{"op":"lookup","detail":"le client","evidence":"consulte le client"},{"op":"classify","detail":"le problème","evidence":"classe le problème"},{"op":"draft","detail":"la réponse","evidence":"harmonise le ton de la réponse"}],
           "effects":[{"verb":"refund","target":"le remboursement","policy":"human_first","evidence":"Demande un accord humain avant le remboursement"}],
           "obligations":[],"constraints":[],"unknowns":[]})
}
pub(crate) fn request() -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(policy())
}

/// A provider answering its plans in order, round-robin, counting its calls.
pub(crate) struct Rotating {
    pub(crate) plans: Vec<String>,
    pub(crate) calls: AtomicU32,
}
impl Rotating {
    pub(crate) fn new(plans: Vec<String>) -> Self {
        Self {
            plans,
            calls: AtomicU32::new(0),
        }
    }
}
impl ProviderInferDyn for Rotating {
    async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst) as usize;
        let text = self.plans[index % self.plans.len()].clone();
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(100, 50),
            StopReason::EndTurn,
        ))
    }
}

pub(crate) struct ChoosePlan {
    pub(crate) choice: &'static str,
    pub(crate) asked: Mutex<Vec<ChoiceQuestion>>,
}
impl DecisionSeat for ChoosePlan {
    fn name(&self) -> &'static str {
        "double/plans"
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            Ok(ChoiceAnswer::new(self.choice, "double-1.0"))
        })
    }
}
pub(crate) fn disagreeing_provider() -> Rotating {
    let mut with_compute = plan();
    with_compute["steps"]
        .as_array_mut()
        .unwrap()
        .push(json!({"op":"compute","detail":"le problème","evidence":"classe le problème","computation":{"present":true}}));
    Rotating::new(vec![
        plan().to_string(),
        with_compute.to_string(),
        plan().to_string(),
    ])
}
pub(crate) fn candidates(doc: &Value) -> Vec<Value> {
    doc["provenance"]["decision"]["candidates"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}
pub(crate) fn route(doc: &Value) -> String {
    doc["provenance"]["decision"]["route"].to_string()
}
