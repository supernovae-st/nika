// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The monetary decision made by Session before cognition. An explicit
//! ceiling blocks unmetered inference; it never grants consent or execution.

use std::path::PathBuf;

use crate::outcome::ProposalId;

/// Where the effective execution amount came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MonetarySource {
    /// Session's documented fallback when the project declares none.
    SessionDefault,
    /// The observed project file's default.
    ProjectDefault,
    /// Money explicitly supplied by the human.
    Explicit,
    /// Explicit money replaces an observed project default.
    Override,
    /// Admission refused; there is no effective amount.
    Rejected,
}

/// Independent cap knowledge, including its provenance. Unknown is not absent.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CapKnowledge {
    /// Session has no authoritative observation of this cap.
    Unknown {
        /// Why the cap cannot be reported as known or absent.
        reason: String,
    },
}

/// What Session can enforce before inference for this decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InferenceEnforcement {
    /// One shared catalog allowance covers qualified calls; provider billing
    /// remains unknown. This is separate from any later execution ceiling.
    CatalogAdmission,
    /// Explicit one-time unknown cost; finite request/token/time bounds, no USD guarantee.
    ExplicitUnknown,
    /// No explicit inference budget was given: no aggregate USD allowance or
    /// cap applies, and the default applies to a later `RunRequest`. A call on
    /// a qualified priced route is still observed (catalog estimate, possible
    /// exposure), never admitted; other routes stay unobserved.
    NotMetered,
    /// Cognition that could charge, or whose cost is unknown, is blocked.
    CallsBlocked,
}

/// Actual admission, visible even when a Prepare cannot produce a workflow.
/// Invalid input is retained with a refusal and no effective monetary value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MonetaryDecision {
    /// Original work request, unchanged by a subsequent monetary amendment.
    pub original_intent: String,
    /// Exact submitted line, including decimal comma and whitespace.
    pub input: String,
    /// Chosen finite nonnegative USD ceiling, or none after invalid admission.
    pub effective_usd: Option<f64>,
    /// Default versus explicit invocation provenance.
    pub source: MonetarySource,
    /// Exact explicit amount token, when one was admitted.
    pub explicit_amount: Option<String>,
    /// Observed project default (never treated as an independent cap).
    pub project_default_usd: Option<f64>,
    /// Project-file provenance of the default, when observed.
    pub project_file: Option<PathBuf>,
    /// Independent policy cap knowledge; Session does not invent one.
    pub policy_cap: CapKnowledge,
    /// Independent machine cap knowledge; Session does not invent one.
    pub machine_cap: CapKnowledge,
    /// Actual inference enforcement, distinct from execution admission.
    pub inference: InferenceEnforcement,
    /// Admission observations at the last proposal binding; the runtime's
    /// `inference_receipt()` returns the live account without changing consent.
    pub admission: Option<nika_providers::InferenceReceipt>,
    /// Billed cost is unknown without a receipt, including subscriptions.
    pub observed_cost_usd: Option<f64>,
    /// Proposal whose exact preview carries this decision, if prepared.
    pub proposal: Option<ProposalId>,
    /// Monetary admission failure, if any. No stale amount is effective.
    pub refusal: Option<String>,
}

impl MonetaryDecision {
    pub(crate) fn line(&self) -> String {
        if let Some(reason) = &self.refusal {
            return format!("money: refused · {reason}");
        }
        if self.inference == InferenceEnforcement::ExplicitUnknown {
            return "money: explicitly accepted unknown inference cost for one invocation; numeric defaults overridden for this call only; billed cost unknown; Run requires independent review".into();
        }
        let scope = if self.inference == InferenceEnforcement::CatalogAdmission {
            " · catalog-backed admission ceiling: conservative token reservations at pinned prices, not a provider invoice or hard billing cap; authoring and Run have separate scopes"
        } else {
            ""
        };
        format!(
            "money: ${} USD · {:?} · project default {:?} · policy cap unknown · machine cap unknown · inference {:?} · billed cost unknown · execution requires a separate Run and downstream admission{scope}",
            self.effective_usd.unwrap_or_default(),
            self.source,
            self.project_default_usd,
            self.inference,
        )
    }
}
