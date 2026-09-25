// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Compatibility surface: the review contract is owned by provider admission.
pub use nika_providers::admission::{
    CapEvidence, CostHostEvidence, CostReview, CostRoute, monetary_default,
    native_catalog_price_known,
};

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
