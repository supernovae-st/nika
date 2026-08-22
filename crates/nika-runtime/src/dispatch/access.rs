// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typed access receipt + the dynamic-model refusal at the dispatch seam.

use super::{Dispatched, FailedDispatch};
use crate::record::TaskErrorRecord;

/// Typed execution-route receipt. Cost attribution remains separate: this
/// records what was requested, which path was selected, and what model the
/// executor actually observed. A total resolver refusal has no selected
/// path, but still preserves the requested model and provider.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AccessReceipt {
    pub(crate) requested_model: String,
    pub(crate) observed_model: Option<String>,
    pub(crate) provider: String,
    pub(crate) access: Option<nika_types::access::AccessClass>,
    pub(crate) billing: Option<nika_types::access::BillingClass>,
    pub(crate) adapter: Option<String>,
}

impl AccessReceipt {
    /// Construct the route receipt a child runner returns when its nested
    /// execution selected a harness. The observed identity stays separate
    /// because an ACP failure often reports none.
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
            access: Some(nika_types::access::AccessClass::Harness),
            billing: Some(nika_types::access::BillingClass::Unknown),
            adapter: Some(adapter.into()),
        }
    }

    /// Attach the model identity the selected executor actually reported.
    #[must_use]
    pub fn with_observed_model(mut self, observed_model: impl Into<String>) -> Self {
        self.observed_model = Some(observed_model.into());
        self
    }

    /// The model identity requested by the workflow.
    #[must_use]
    pub fn requested_model(&self) -> &str {
        &self.requested_model
    }

    /// The executor-reported model identity, when observable.
    #[must_use]
    pub fn observed_model(&self) -> Option<&str> {
        self.observed_model.as_deref()
    }

    /// The requested model's provider namespace.
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// The selected access class, absent on total resolver refusal.
    #[must_use]
    pub const fn access(&self) -> Option<nika_types::access::AccessClass> {
        self.access
    }

    /// The selected billing class, absent on total resolver refusal.
    #[must_use]
    pub const fn billing(&self) -> Option<nika_types::access::BillingClass> {
        self.billing
    }

    /// The selected harness adapter id, when the route uses a harness.
    #[must_use]
    pub fn adapter(&self) -> Option<&str> {
        self.adapter.as_deref()
    }

    pub(super) fn planned(plan: &nika_types::access::AccessPlan) -> Self {
        Self {
            requested_model: plan.model.clone(),
            observed_model: None,
            provider: plan.provider.clone(),
            access: Some(plan.chosen),
            billing: Some(plan.billing),
            adapter: (plan.chosen == nika_types::access::AccessClass::Harness)
                .then(|| plan.access.clone()),
        }
    }

    fn refused(refusal: &nika_providers::AccessRefusal) -> Self {
        Self {
            requested_model: refusal.model.clone(),
            observed_model: None,
            provider: refusal.provider.clone(),
            access: None,
            billing: None,
            adapter: None,
        }
    }

    /// A selected harness may already have performed external effects
    /// before its terminal beat is lost. Replaying that route is unsafe;
    /// the runtime retry seam uses this typed fact instead of guessing
    /// from an inference-family error code.
    pub(crate) fn selected_harness(&self) -> bool {
        self.access == Some(nika_types::access::AccessClass::Harness)
    }
}

impl Dispatched {
    /// Backstop for a dynamically rendered model whose access meet has no
    /// survivor. Static models were already judged at admission; this value
    /// did not exist until rendering. No provider/harness effect has fired.
    pub(super) fn access_refused(note: &str, refusal: &nika_providers::AccessRefusal) -> Self {
        let witnesses = refusal
            .rejected
            .iter()
            .map(nika_types::access::AccessRejection::witness_line)
            .collect::<Vec<_>>();
        let suffix = if witnesses.is_empty() {
            String::new()
        } else {
            format!(" · {}", witnesses.join(" · "))
        };
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch {
                record: TaskErrorRecord {
                    code: nika_error::codes::NIKA_1800.to_string(),
                    message: format!(
                        "no access path survives admission for `{}`{suffix}",
                        refusal.model
                    ),
                    transient: false,
                },
                cost_usd: None,
                cost_source: None,
                cost_unpriced: None,
                access_receipt: Some(AccessReceipt::refused(refusal)),
                evidence: None,
            }),
        }
    }
}
