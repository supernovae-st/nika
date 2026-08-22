// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typed access-route receipts and the cancellation-safe observer.

use std::sync::{Arc, Mutex};

use nika_types::access::{AccessClass, AccessPlan, BillingClass};

use crate::AccessRefusal;

/// Per-attempt route witness owned outside a cancellable dispatch future.
#[derive(Default)]
pub struct AccessObserver {
    receipt: Arc<Mutex<Option<AccessReceipt>>>,
    parent: Option<Arc<Mutex<Option<AccessReceipt>>>>,
}

impl AccessObserver {
    /// Record a selected route before its effect starts.
    pub fn record(&self, receipt: &AccessReceipt) {
        Self::retain(&self.receipt, receipt);
        if let Some(parent) = &self.parent {
            Self::retain(parent, receipt);
        }
    }

    /// Create a child observer that also stamps this observer.
    #[must_use]
    pub fn child(&self) -> Self {
        Self {
            receipt: Arc::new(Mutex::new(None)),
            parent: Some(Arc::clone(&self.receipt)),
        }
    }

    fn retain(slot: &Mutex<Option<AccessReceipt>>, receipt: &AccessReceipt) {
        let mut retained = match slot.lock() {
            Ok(retained) => retained,
            Err(poisoned) => poisoned.into_inner(),
        };
        let replace = retained.is_none()
            || (!retained
                .as_ref()
                .is_some_and(AccessReceipt::selected_harness)
                && receipt.selected_harness());
        if replace {
            *retained = Some(receipt.clone());
        }
    }

    /// Return the strongest replay guard observed so far.
    #[must_use]
    pub fn receipt(&self) -> Option<AccessReceipt> {
        let retained = match self.receipt.lock() {
            Ok(retained) => retained,
            Err(poisoned) => poisoned.into_inner(),
        };
        retained.clone()
    }
}

/// Requested, selected, and observed execution-route identity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AccessReceipt {
    requested_model: String,
    observed_model: Option<String>,
    provider: String,
    access: Option<AccessClass>,
    billing: Option<BillingClass>,
    adapter: Option<String>,
    representative: bool,
}

impl AccessReceipt {
    /// Construct a receipt for a nested run that selected a harness.
    #[must_use]
    pub fn harness(
        requested_model: impl Into<String>,
        provider: impl Into<String>,
        adapter: impl Into<String>,
    ) -> Self {
        Self {
            requested_model: requested_model.into(),
            observed_model: None,
            provider: provider.into(),
            access: Some(AccessClass::Harness),
            billing: Some(BillingClass::Unknown),
            adapter: Some(adapter.into()),
            representative: false,
        }
    }

    /// Construct the receipt for a selected access plan.
    #[must_use]
    pub fn planned(plan: &AccessPlan) -> Self {
        Self {
            requested_model: plan.model.clone(),
            observed_model: None,
            provider: plan.provider.clone(),
            access: Some(plan.chosen),
            billing: Some(plan.billing),
            adapter: (plan.chosen == AccessClass::Harness).then(|| plan.access.clone()),
            representative: false,
        }
    }

    /// Construct a receipt for a total resolver refusal.
    #[must_use]
    pub fn refused(refusal: &AccessRefusal) -> Self {
        Self {
            requested_model: refusal.model.clone(),
            observed_model: None,
            provider: refusal.provider.clone(),
            access: None,
            billing: None,
            adapter: None,
            representative: false,
        }
    }

    /// Attach the model identity reported by the selected executor.
    #[must_use]
    pub fn with_observed_model(mut self, observed_model: impl Into<String>) -> Self {
        self.observed_model = Some(observed_model.into());
        self
    }

    /// Attach an executor-reported model when one was observable.
    #[must_use]
    pub fn with_optional_observed_model(mut self, observed_model: Option<String>) -> Self {
        self.observed_model = observed_model;
        self
    }

    /// Requested workflow model.
    #[must_use]
    pub fn requested_model(&self) -> &str {
        &self.requested_model
    }

    /// Executor-reported model, when observable.
    #[must_use]
    pub fn observed_model(&self) -> Option<&str> {
        self.observed_model.as_deref()
    }

    /// Requested model's provider namespace.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Selected access class, absent on total refusal.
    #[must_use]
    pub const fn access(&self) -> Option<AccessClass> {
        self.access
    }

    /// Selected billing class, absent on total refusal.
    #[must_use]
    pub const fn billing(&self) -> Option<BillingClass> {
        self.billing
    }

    /// Selected harness adapter id.
    #[must_use]
    pub fn adapter(&self) -> Option<&str> {
        self.adapter.as_deref()
    }

    /// Whether the receipt is one replay guard from an aggregate run.
    #[must_use]
    pub const fn is_representative(&self) -> bool {
        self.representative
    }

    /// Mark this receipt as representative of an aggregate run.
    #[must_use]
    pub fn into_representative(mut self) -> Self {
        self.representative = true;
        self
    }

    /// Whether replay could repeat a selected harness effect.
    #[must_use]
    pub fn selected_harness(&self) -> bool {
        self.access == Some(AccessClass::Harness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_child_retains_harness_route_on_parent() -> Result<(), String> {
        let parent = AccessObserver::default();
        let child = parent.child();
        child.record(&AccessReceipt::harness(
            "anthropic/claude-sonnet-4-6",
            "anthropic",
            "claude-agent-acp",
        ));
        drop(child);

        let receipt = parent
            .receipt()
            .ok_or_else(|| "the aggregate lost a cancelled child's route".to_owned())?;
        assert!(receipt.selected_harness());
        assert_eq!(receipt.adapter(), Some("claude-agent-acp"));
        Ok(())
    }
}
