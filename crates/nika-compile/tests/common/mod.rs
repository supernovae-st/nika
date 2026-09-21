// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Helpers the compile suites share: one hermetic generative provider that returns a fixed
//! text and counts its calls, the bounded authoring policy, the question keys of an outcome.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]
use nika_compile::AuthoringPolicy;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use std::{
    sync::atomic::{AtomicU32, Ordering},
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
