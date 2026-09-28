// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Compatibility surface: the review contract is owned by provider admission.
pub use nika_providers::admission::{
    CapEvidence, CostHostEvidence, CostReview, CostRoute, monetary_default,
    native_catalog_price_known,
};

/// Stamps the host account's receipt on the run's terminal frame: the one
/// evidence trail every door shares. A declared-free observer's receipt says
/// `scoped_to_declared_free`, so its subtotal never reads as the whole Run's.
pub(crate) struct ObservedSink<'a>(
    pub(crate) &'a mut dyn crate::EventSink,
    pub(crate) Option<&'a nika_providers::InferenceAdmission>,
);
impl crate::EventSink for ObservedSink<'_> {
    fn emit(&mut self, mut event: nika_event::Event) {
        if let Some(account) = self.1.filter(|_| event.is_terminal()) {
            // An unreadable account is said on the frame, never omitted.
            let observed = account.snapshot().map_or_else(
                |e| serde_json::json!({ "unreadable": e.to_string() }),
                |receipt| receipt.observation(),
            );
            let value = crate::FieldValue::String(observed.to_string());
            event = event.with_field(crate::KeyValue::new("inference_admission", value));
        }
        self.0.emit(event);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_providers::ProvidersConfig;
    use nika_types::cost::Cost;
    fn route() -> CostRoute {
        CostRoute::observe("deepseek/deepseek-v4-pro", ProvidersConfig::new())
            .expect("native route")
    }
    fn review() -> CostReview {
        CostReview::new(
            "candidate-a".into(),
            "invocation-a".into(),
            route(),
            CostHostEvidence::unmanaged_interactive_local(),
            Some(Cost::new(20_000_000)),
            Some(Cost::new(10_000_000)),
        )
        .expect("review")
    }
    #[test]
    fn new_review_and_runtime_scope_bindings_do_not_restore_authority() {
        let account = review()
            .confirm("candidate-a", &route())
            .expect("fresh choice");
        let config = crate::RuntimeConfig::new(None, 0)
            .with_inference_admission(&account, "candidate-a", "invocation-a")
            .expect("exact scope");
        assert!(
            crate::RuntimeConfig::new(None, 0)
                .with_inference_admission(&account, "candidate-b", "invocation-a")
                .is_err()
        );
        assert!(
            crate::RuntimeConfig::new(None, 0)
                .with_inference_admission(&account, "candidate-a", "invocation-b")
                .is_err()
        );
        let receipt = account.snapshot().expect("receipt");
        assert_eq!(
            receipt.overridden_defaults,
            [Some(Cost::new(20_000_000)), Some(Cost::new(10_000_000))]
        );
        assert!(receipt.observation()["limit_nano_usd"].is_null());
        account.close("cancel").expect("close shared account");
        assert_eq!(
            config
                .inference_admission
                .expect("same account")
                .snapshot()
                .expect("closed")
                .state,
            nika_providers::AdmissionState::Closed
        );
    }
}
