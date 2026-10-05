// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A legacy report is evidence to display, never a recovered numeric account.
use super::{AdmissionState, Cost, observation_consistent, observation_readable};
use serde_json::Value;

/// Readable old numeric exposure followed only by completed, freshly reviewed invocations.
/// The original observations stay with the host unchanged. This projection cannot create or
/// amend an account; its digest belongs in the host's project/input witness before confirmation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct LegacyCostReport {
    digest: String,
    known: Cost,
    reserved_unknown: Cost,
    previous_limit: Cost,
    later_known: Cost,
    later_unknown: u64,
}
impl LegacyCostReport {
    /// Read the deliberately narrow legacy form: one strict, uncertain numeric observation,
    /// followed only by closed unknown-cost invocations. Active, malformed, other schemas and
    /// newly uncertain scopes refuse. No endpoint, output bound or missing receipt is inferred.
    /// # Errors
    /// Evidence is absent, contradictory, active, unbudgeted or outside this legacy form.
    pub fn read(observations: &[Value]) -> Result<Self, String> {
        let bad = || {
            "legacy exposure is unreadable or still active; no new invocation reviewed".to_owned()
        };
        let Some(first) = observations.first() else {
            return Err(bad());
        };
        if observations.len() > 256
            || !observation_readable(first)
            || first["schema"] != "nika/inference-cost-observation@2"
            || first["unbudgeted"] == true
            || !first["unknown_cost"].is_null()
            || observation_consistent(first) != Ok((AdmissionState::Uncertain, true))
            || first["unknown_calls"].as_u64().is_none_or(|n| n == 0)
            || first["unknown_attempts"]
                .as_array()
                .is_none_or(|a| !a.is_empty())
        {
            return Err(bad());
        }
        let previous_limit = amount(&first["limit_nano_usd"])?;
        if previous_limit.nano_usd == 0 {
            return Err(bad());
        }
        let known = amount(&first["known_subtotal_nano_usd"])?;
        let attempts = first["attempts"].as_array().ok_or_else(bad)?;
        let mut reserved_unknown = 0_i128;
        for (id, attempt) in attempts.iter().enumerate() {
            if attempt["id"].as_u64() != u64::try_from(id).ok() || attempt["sent"] != true {
                return Err(bad());
            }
            let reserved = amount(&attempt["reserved_nano_usd"])?;
            if attempt["estimated_nano_usd"].is_null() {
                if !attempt["usage"].is_null()
                    || attempt["note"] != "charge unknown; reservation retained"
                {
                    return Err(bad());
                }
                reserved_unknown = reserved_unknown
                    .checked_add(reserved.nano_usd)
                    .ok_or_else(bad)?;
            } else if amount(&attempt["estimated_nano_usd"])?.nano_usd > reserved.nano_usd {
                return Err(bad());
            }
        }
        // A retained quote is neither the final charge nor a proven historical wire bound.
        if known
            .nano_usd
            .checked_add(reserved_unknown)
            .is_none_or(|n| n > previous_limit.nano_usd)
        {
            return Err(bad());
        }
        let (later_known, later_unknown) = later_costs(&observations[1..])?;
        let bytes = serde_json::to_vec(observations).map_err(|e| e.to_string())?;
        if bytes.len() > 512 * 1024 {
            return Err(bad());
        }
        Ok(Self {
            digest: blake3::hash(&bytes).to_hex().to_string(),
            known,
            reserved_unknown: Cost::new(reserved_unknown),
            previous_limit,
            later_known: Cost::new(later_known),
            later_unknown,
        })
    }
    /// Keep the original exposure visible even when a later report blocks another review.
    /// An unreadable original report is not projected as a known amount.
    #[must_use]
    pub fn summary_of(observations: &[Value]) -> Option<String> {
        match Self::read(observations) {
            Ok(report) => Some(report.summary()),
            Err(_) => Self::read(observations.get(..1)?).ok().map(|report| {
                format!("{} · later cost reports are unreadable, active or uncertain; another invocation is blocked", report.summary())
            }),
        }
    }
    /// Digest of all unchanged observations, not proof of a provider invoice.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
    /// Past exposure, separate from the next invocation's authorization.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "legacy exposure retained: known estimate {} · reservation for unknown charge {} (not a final charge or a proven bound) · old allowance {} unchanged · later reviewed known subtotal {} and {} unknown call(s) · invoice unknown; no guaranteed TOTAL ceiling",
            self.known,
            self.reserved_unknown,
            self.previous_limit,
            self.later_known,
            self.later_unknown
        )
    }
}
fn later_costs(observations: &[Value]) -> Result<(i128, u64), String> {
    let bad = || "later reviewed exposure is unreadable or not closed".to_owned();
    let mut invocations = std::collections::BTreeSet::new();
    let (mut later_known, mut later_unknown) = (0_i128, 0_u64);
    for later in observations {
        if !observation_readable(later)
            || later["schema"] != "nika/inference-cost-observation@2"
            || later["unbudgeted"] == true
            || !later["unknown_cost"].is_object()
            || observation_consistent(later).is_err()
            || later["state"] != "Closed"
            || later["attempts"].as_array().is_none_or(|a| !a.is_empty())
            || later["unknown_attempts"].as_array().is_none_or(|a| {
                a.iter().any(|r| {
                    r["sent"] != true
                        || !matches!(
                            r["note"].as_str(),
                            Some(
                                "complete usage priced; invoice unknown"
                                    | "completed; USD cost unknown"
                            )
                        )
                })
            })
        {
            return Err(bad());
        }
        let invocation = later["unknown_cost"]["invocation"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(bad)?;
        if !invocations.insert(invocation) {
            return Err(bad());
        }
        later_known = later_known
            .checked_add(amount(&later["known_subtotal_nano_usd"])?.nano_usd)
            .ok_or_else(bad)?;
        later_unknown = later_unknown
            .checked_add(later["unknown_calls"].as_u64().ok_or_else(bad)?)
            .ok_or_else(bad)?;
    }
    Ok((later_known, later_unknown))
}
fn amount(value: &Value) -> Result<Cost, String> {
    value
        .as_str()
        .and_then(|s| s.parse::<i128>().ok())
        .filter(|n| *n >= 0)
        .map(Cost::new)
        .ok_or_else(|| "legacy amount is unreadable".to_owned())
}

#[cfg(test)]
mod tests;
