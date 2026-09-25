// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Per-dispatch monetary evidence. No tariff lookup or execution authority here.
use super::Cost;
use crate::token_usage::TokenUsage;
use alloc::string::String;

/// Selected adapter/model and the full endpoint observed on the HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct InferenceRoute {
    /// Adapter namespace; not necessarily the biller.
    pub provider: String,
    /// Exact selected wire model; independent of the returned model.
    pub model: String,
    /// Full final URL, including path. Never a normalized origin.
    pub endpoint: String,
}
impl InferenceRoute {
    /// Construct observed identity. The producer must omit unsafe URL secrets.
    #[must_use]
    pub fn new(provider: String, model: String, endpoint: String) -> Self {
        Self {
            provider,
            model,
            endpoint,
        }
    }
}

/// One actual provider dispatch, including a dispatch that returned no usage.
/// An empty collection means no observations, not evidence of a free call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct InferenceCall {
    /// Safe full endpoint handed to HTTP. It does not prove the final route.
    pub requested_endpoint: Option<String>,
    /// Actual response endpoint and selected wire identity, when observed.
    pub route: Option<InferenceRoute>,
    /// Reported meters, absent on missing usage or interrupted responses.
    pub usage: Option<TokenUsage>,
    /// All tariff-relevant meters were validated by the adapter.
    pub usage_complete: bool,
    /// Model actually returned by the provider.
    pub response_model: Option<String>,
    /// Provider request identifier when available.
    pub request_id: Option<String>,
    /// Immutable serialized pricing provenance, including estimate kind/currency.
    /// This observation is not a grant and must never be used to reprice history.
    pub pricing: Option<String>,
    /// Qualified USD estimate produced at the provider boundary, never an invoice.
    pub estimated_usd: Option<Cost>,
}
impl InferenceCall {
    /// A dispatched call whose monetary result is not yet known.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return an estimate only with matching observed identities and complete
    /// meters. Unknown usage/routes remain unknown even if a producer set a cost.
    #[must_use]
    pub fn known_estimate(&self) -> Option<Cost> {
        let route = self.route.as_ref()?;
        let usage = self.usage.as_ref()?;
        if !self.usage_complete
            || self.response_model.as_deref() != Some(route.model.as_str())
            || self.pricing.is_none()
            || usage.cache_read_tokens.unwrap_or(0) > usage.input_tokens
        {
            return None;
        }
        self.estimated_usd.filter(|c| c.nano_usd >= 0)
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::*;
    use alloc::vec;
    #[test]
    fn old_failure_defaults_and_observation_recovery_preserve_unknown() {
        let old = r#"{"usage":{"input_tokens":0,"output_tokens":0},"tools_cost_usd":null,"model_resolved":null}"#;
        let failure: super::super::SpendOnFailure = serde_json::from_str(old).expect("legacy");
        assert!(failure.inference_calls.is_empty());
        let mut call = InferenceCall::new();
        call.requested_endpoint = Some("https://gateway.example/v1/chat/completions".into());
        let failure = failure.with_inference_calls(vec![call]);
        assert!(failure.has_signal());
        let encoded = serde_json::to_string(&failure).expect("serialize");
        let restored: super::super::SpendOnFailure =
            serde_json::from_str(&encoded).expect("recover");
        assert_eq!(failure, restored);
        assert_eq!(restored.inference_calls[0].known_estimate(), None);
    }
}
