// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Versioned observation/response only. The consuming review stays in its host.
use super::{CostReview, CostRoute};
use crate::InferenceAdmission;
use serde::{Deserialize, Serialize};

/// An observation for one fresh human decision, never callable authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CostChallenge {
    pub schema: String,
    pub nonce: String,
    pub candidate: String,
    pub invocation: String,
    pub route: CostRoute,
    pub source_sha256: String,
    pub inputs_sha256: String,
    pub question: String,
    pub native_price: String,
    pub review_details: String,
}
impl CostChallenge {
    /// Strict version and nonempty identity; the receiver cannot expand bounds.
    /// # Errors
    /// Malformed, duplicate, unknown-version or incomplete observation.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if value.schema != "nika/run-cost-challenge@1"
            || [
                &value.nonce,
                &value.candidate,
                &value.invocation,
                &value.source_sha256,
                &value.inputs_sha256,
                &value.question,
            ]
            .iter()
            .any(|s| s.is_empty())
        {
            return Err("invalid Run cost challenge".into());
        }
        Ok(value)
    }
    /// Echo only the exact observation, never an admission handle.
    #[must_use]
    pub fn response(&self, yes: bool) -> CostResponse {
        CostResponse {
            schema: "nika/run-cost-response@1".into(),
            challenge: self.clone(),
            yes,
        }
    }
    /// The first screen of a fresh Run decision: the exact route and where it
    /// goes, the money truth, the review's bounds and defaults, the three
    /// choices. Hashes, identities and host evidence stay on `details`.
    #[must_use]
    pub fn display(&self) -> String {
        format!(
            "Fresh Run cost decision · {}/{} at {}\n{}\nNative currency: {}\nApproves this Run once; an authoring or Save approval never approves a Run. The review expires after five minutes.\n{CHOICES}",
            self.route.provider,
            self.route.model,
            origin(&self.route.endpoint),
            question_body(&self.question),
            self.native_price,
        )
    }
    /// The complete evidence of this same challenge. Reading it answers
    /// nothing: the nonce, the bounds and the reply channel stay unchanged.
    #[must_use]
    pub fn details(&self) -> String {
        format!(
            "Run cost decision details · challenge {} · reading them approves nothing\nRoute: {}/{}\nEndpoint: {}\nSource SHA-256: {}\nInput SHA-256: {}\nCandidate: {}\nInvocation: {}\nNative currency: {}\nHost and cap evidence: {}\n{REVIEW_CHOICE}",
            self.nonce,
            self.route.provider,
            self.route.model,
            self.route.endpoint,
            self.source_sha256,
            self.inputs_sha256,
            self.candidate,
            self.invocation,
            self.native_price,
            self.review_details,
        )
    }
}

/// The review's own closing line, and the first screen's line that also names
/// `details` (which reads the evidence and answers nothing).
const REVIEW_CHOICE: &str = "Continue once? yes / no";
const CHOICES: &str = "Continue once? yes / no / details";

/// The review's sentences without its closing choice line, which the first
/// screen replaces; any other wording is kept whole.
fn question_body(question: &str) -> &str {
    question
        .strip_suffix(REVIEW_CHOICE)
        .map_or(question, str::trim_end)
}

/// Scheme and host of an endpoint, without path, query or credentials: where
/// the request goes, at a glance. `details` keeps the endpoint whole.
fn origin(endpoint: &str) -> String {
    let (scheme, rest) = endpoint.split_once("://").unwrap_or(("", endpoint));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if scheme.is_empty() {
        host.to_owned()
    } else {
        format!("{scheme}://{host}")
    }
}

/// A single strict response document. Duplicate keys/frames are refused by serde.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CostResponse {
    schema: String,
    challenge: CostChallenge,
    yes: bool,
}

/// Intentionally neither Clone nor serializable: one live pending review.
#[derive(Debug)]
#[non_exhaustive]
pub struct PendingCostReview {
    review: CostReview,
    challenge: CostChallenge,
}
impl PendingCostReview {
    #[must_use]
    pub fn new(review: CostReview, source_sha256: String, inputs_sha256: String) -> Self {
        let challenge = CostChallenge {
            schema: "nika/run-cost-challenge@1".into(),
            nonce: nika_types::id::EventId::generate().to_string(),
            candidate: review.candidate.clone(),
            invocation: review.invocation.clone(),
            route: review.route.clone(),
            source_sha256,
            inputs_sha256,
            question: review.question(),
            review_details: review.details(),
            native_price: nika_catalog::admission::InferenceTariff::new(&review.route.provider, &review.route.model, &review.route.endpoint)
                .map_or_else(|| "price and invoice unknown".into(), |t|
                    format!("{} catalog tariff, {} as of {}; final charge/invoice unknown; no currency conversion", t.currency, t.source, t.as_of)),
        };
        Self { review, challenge }
    }
    #[must_use]
    pub fn challenge(&self) -> &CostChallenge {
        &self.challenge
    }
    /// Consume exactly once, after the host independently re-observes its evidence.
    /// # Errors
    /// Decline, mismatched response, expiry or changed candidate/route/policy.
    pub fn confirm(
        self,
        response: &CostResponse,
        candidate: &str,
        route: &CostRoute,
    ) -> Result<InferenceAdmission, String> {
        if response.schema != "nika/run-cost-response@1" || response.challenge != self.challenge {
            return Err("Run response does not match this live review".into());
        }
        if !response.yes {
            return Err("unknown-cost Run declined; nothing sent".into());
        }
        self.review.confirm(candidate, route)
    }
}

#[cfg(test)]
mod tests;
