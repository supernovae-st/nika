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
                |receipt| {
                    nika_providers::project_observation(&receipt.observation()).unwrap_or_else(|| {
                        serde_json::json!({ "unreadable": "cost observation cannot be projected" })
                    })
                },
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
    #[test]
    fn durable_cost_writers_hide_private_routes_in_terminal_frames() {
        let route = CostRoute::observe(
            "deepseek/deepseek-v4-pro",
            ProvidersConfig::new()
                .with_base_url("deepseek", "https://example.test/private-route-canary/v1"),
        )
        .expect("canonical private route");
        let account = CostReview::new(
            "candidate".into(),
            "run".into(),
            route.clone(),
            CostHostEvidence::unmanaged_interactive_local(),
            None,
            None,
        )
        .expect("review")
        .confirm("candidate", &route)
        .expect("fresh authority");
        let exact = account.snapshot().expect("account").observation();
        assert!(exact.to_string().contains("private-route-canary"));
        let mut events = crate::VecSink::new();
        let mut stamp = crate::DeterministicStamper::new();
        let mut sink = ObservedSink(&mut events, Some(&account));
        crate::emit(
            &mut stamp,
            &mut sink,
            nika_event::EventKind::WorkflowStarted,
            &[],
        );
        crate::emit(
            &mut stamp,
            &mut sink,
            nika_event::EventKind::WorkflowCompleted,
            &[],
        );
        let events = events.into_events();
        assert!(events[0].field("inference_admission").is_none());
        let text = events[1]
            .str_field("inference_admission")
            .expect("terminal receipt");
        let durable: serde_json::Value = serde_json::from_str(text).expect("JSON");
        assert_eq!(durable["schema"], "nika/inference-cost-observation@2");
        assert!(!text.contains("private-route-canary"));
        assert_eq!(
            durable["unknown_cost"]["origin"],
            "https://example.test:443"
        );
        assert_eq!(durable["unknown_calls"], exact["unknown_calls"]);
        assert_eq!(
            account.snapshot().expect("unchanged account").observation(),
            exact
        );
    }

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
