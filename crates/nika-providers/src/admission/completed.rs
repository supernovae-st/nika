// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Completed unknown-cost observations, never a deserialized admission or invoice.
use super::{AdmissionState, Cost, observation_consistent, observation_readable};
use serde_json::{Value, json};

const SCHEMA: &str = "nika/completed-cost-report@1";

/// Prior one-time invocations whose responses completed, with their unknown prices intact.
/// This report cannot create, amend or reopen an account. A host still needs a fresh review.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct CompletedCostReport {
    digest: String,
    summary: String,
}
impl CompletedCostReport {
    pub(super) fn into_display(self) -> (String, String) {
        (self.digest, self.summary)
    }

    /// Read only the owner's durable `@2` form, all scopes closed and all attempts completed.
    /// Numeric, active, interrupted, duplicate and contradictory records remain refused.
    /// # Errors
    /// Any observation is absent, unsupported, incomplete or inconsistent.
    pub fn read(observations: &[Value]) -> Result<Self, String> {
        let bad =
            || "prior unknown-cost invocations are not complete closed observations".to_owned();
        if observations.is_empty() || observations.len() > 256 {
            return Err(bad());
        }
        let bytes = serde_json::to_vec(observations).map_err(|e| e.to_string())?;
        if bytes.len() > 512 * 1024 {
            return Err(bad());
        }
        let mut scopes = std::collections::BTreeSet::new();
        let (mut known, mut unknown, mut requests) = (0_i128, 0_u64, 0_usize);
        for observation in observations {
            validate(observation).ok_or_else(bad)?;
            let choice = &observation["unknown_cost"];
            if !scopes.insert(choice["invocation"].as_str().ok_or_else(bad)?) {
                return Err(bad());
            }
            let amount = observation["known_subtotal_nano_usd"]
                .as_str()
                .and_then(|v| v.parse::<i128>().ok())
                .ok_or_else(bad)?;
            known = known.checked_add(amount).ok_or_else(bad)?;
            unknown = unknown
                .checked_add(observation["unknown_calls"].as_u64().ok_or_else(bad)?)
                .ok_or_else(bad)?;
            requests += observation["unknown_attempts"]
                .as_array()
                .ok_or_else(bad)?
                .len();
        }
        Ok(Self {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            summary: format!(
                "completed invocation exposure retained: {} scope(s), {requests} request(s), known estimate {}, {unknown} unpriced call(s); invoice unknown; no earlier allowance or consent restored",
                scopes.len(),
                Cost::new(known)
            ),
        })
    }
    /// Display retained complete scopes even while a distinct fresh invocation is active.
    /// This projection grants no authority and never summarizes unreadable evidence as zero.
    #[must_use]
    pub fn summary_of(observations: &[Value]) -> Option<String> {
        Self::read(observations).ok().map(|report| report.summary)
    }
    /// Digest of the original, unchanged observations, including their distinct scope IDs.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
    /// Prior exposure, kept visible beside a new review, never an invoice.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
    /// A versioned project-bound witness to a report; no callable authority is serialized.
    #[must_use]
    pub fn checkpoint(&self, project: &[u8]) -> Value {
        json!({"schema":SCHEMA,"project":blake3::hash(project).to_hex().to_string(),
            "observations":self.digest})
    }
    /// Require exact report/project identity. The sole compatibility form is the old numeric
    /// codec's exact refusal string, and only after `read` has validated every closed scope.
    /// Hosts must also require concordant exclusive history and no interrupted operation.
    #[must_use]
    pub fn matches_checkpoint(&self, raw: &Value, project: &[u8]) -> bool {
        !project.is_empty()
            && (raw == &self.checkpoint(project)
                || raw.as_str()
                    == Some(
                        super::denied("only a complete strict numeric account can be checkpointed")
                            .to_string()
                            .as_str(),
                    ))
    }
}

fn validate(observation: &Value) -> Option<()> {
    if !observation_readable(observation)
        || observation["schema"] != "nika/inference-cost-observation@2"
        || observation_consistent(observation).ok()?.0 != AdmissionState::Closed
        || observation["unbudgeted"] == true
        || !observation["limit_nano_usd"].is_null()
        || !observation["attempts"].as_array()?.is_empty()
    {
        return None;
    }
    let choice = &observation["unknown_cost"];
    let requests = choice["max_requests"].as_u64().filter(|n| *n > 0)?;
    let output = choice["max_output_tokens"].as_u64().filter(|n| *n > 0)?;
    choice["timeout_ms"].as_u64().filter(|n| *n > 0)?;
    if !choice["max_in_flight"].is_null() {
        choice["max_in_flight"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= requests)?;
    }
    for key in ["candidate", "invocation", "provider", "model", "origin"] {
        choice[key].as_str().filter(|s| !s.is_empty())?;
    }
    let attempts = observation["unknown_attempts"].as_array()?;
    if attempts.len() as u64 > requests {
        return None;
    }
    for (id, attempt) in attempts.iter().enumerate() {
        if attempt["id"].as_u64() != u64::try_from(id).ok()
            || attempt["choice"] != *choice
            || attempt["sent"] != true
            || attempt["response_model"] != choice["model"]
            || attempt["usage"]["output_tokens"].as_u64()? > output
            || !matches!(
                attempt["note"].as_str(),
                Some("complete usage priced; invoice unknown" | "completed; USD cost unknown")
            )
        {
            return None;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests;
