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
pub(crate) struct AccessReceipt {
    pub requested_model: String,
    pub observed_model: Option<String>,
    pub provider: String,
    pub access: Option<nika_types::access::AccessClass>,
    pub billing: Option<nika_types::access::BillingClass>,
    pub adapter: Option<String>,
}

impl AccessReceipt {
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
