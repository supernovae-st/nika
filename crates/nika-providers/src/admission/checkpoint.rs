// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Complete numeric accounting, restored closed; old observations remain observations.
use super::{
    AdmissionState, AttemptReceipt, Cost, InferenceAdmission, InferenceTariff, ProviderError,
    TokenUsage, add, denied, scope,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const SCHEMA: &str = "nika/inference-admission-checkpoint@1";
const MAX_BYTES: usize = 512 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    project: String,
    identity: String,
    limit: String,
    estimated: String,
    active: String,
    held: String,
    uncertain: bool,
    refusal: Option<String>,
    attempts: Vec<KeptAttempt>,
    observation: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    digest: String,
    account: Checkpoint,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Active,
    Held,
    Settled,
    Released,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeptAttempt {
    id: usize,
    route_tariff: String,
    reserved: String,
    max_output_tokens: u32,
    sent: bool,
    usage: Option<TokenUsage>,
    estimated: Option<String>,
    response_model: Option<String>,
    request_id: Option<String>,
    reported_estimate: Option<String>,
    note: String,
    phase: Phase,
}
fn hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
fn route_tariff(t: InferenceTariff, endpoint: &str) -> String {
    // Debug contains every pinned rate and limit. A changed representation also
    // refuses; it cannot silently reinterpret or reprice a previous settlement.
    hash(format!("{t:?}\0{endpoint}").as_bytes())
}
fn cost(raw: &str) -> Result<Cost, ProviderError> {
    raw.parse::<i128>()
        .ok()
        .filter(|n| *n >= 0 && n.to_string() == raw)
        .map(Cost::new)
        .ok_or_else(|| denied("checkpoint cost is invalid"))
}
fn bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ProviderError> {
    let bytes = serde_json::to_vec(value).map_err(|_| denied("checkpoint cannot be encoded"))?;
    if bytes.len() > MAX_BYTES {
        return Err(denied("checkpoint exceeds 512 KiB"));
    }
    Ok(bytes)
}
impl InferenceAdmission {
    /// Keep the complete strict numeric account, bound to a host's canonical project identity.
    /// This is not consent. Endpoints remain private; their exact tariff binding is hashed.
    /// # Errors
    /// Non-numeric accounts, unsafe metadata and oversized or unavailable accounts refuse.
    pub fn checkpoint(&self, project: &[u8]) -> Result<Value, ProviderError> {
        let s = self.lock()?;
        if project.is_empty()
            || s.unbudgeted
            || s.unknown.is_some()
            || s.unknown_in_flight != 0
            || !s.unknown_attempts.is_empty()
            || s.overridden_defaults != [None, None]
            || !matches!(self.1.routes, scope::RouteSelection::All)
            || !self.1.bound
        {
            return Err(denied(
                "only a complete strict numeric account can be checkpointed",
            ));
        }
        let account = Checkpoint {
            schema: SCHEMA.into(),
            project: hash(project),
            identity: s.identity.clone(),
            limit: s.limit.nano_usd.to_string(),
            estimated: s.estimated.nano_usd.to_string(),
            active: s.active.nano_usd.to_string(),
            held: s.held.nano_usd.to_string(),
            uncertain: s.status == AdmissionState::Uncertain
                || s.attempts
                    .iter()
                    .any(|a| a.note == "reserved" || (a.sent && a.estimated.is_none())),
            refusal: s.refusal.clone(),
            observation: self.snapshot_locked(&s)?.durable_observation(),
            attempts: s
                .attempts
                .iter()
                .map(KeptAttempt::capture)
                .collect::<Result<_, _>>()?,
        };
        if s.attempts.iter().any(|a| {
            s.refusal
                .as_deref()
                .is_some_and(|text| !crate::route_identity::checkpoint_text_safe(&a.endpoint, text))
        }) {
            return Err(denied("checkpoint would expose private route metadata"));
        }
        let envelope = Envelope {
            digest: hash(&bytes(&account)?),
            account,
        };
        serde_json::from_slice(&bytes(&envelope)?).map_err(|_| denied("checkpoint encoding failed"))
    }

    /// Reconstruct accounting only. The caller must own the project's exclusive lease and
    /// independently prove that its durable boundary is complete and concordant. The result
    /// is CLOSED (or uncertain), so only a new explicit total allowance can admit more work.
    /// The second value identifies the exact observation replaced by the restored account.
    /// # Errors
    /// Corruption, old/unknown formats, changed tariffs, invalid sums and other projects refuse.
    pub fn from_checkpoint(value: &Value, project: &[u8]) -> Result<(Self, Value), ProviderError> {
        let envelope: Envelope = serde_json::from_slice(&bytes(value)?)
            .map_err(|_| denied("unreadable admission checkpoint"))?;
        let c = envelope.account;
        if c.schema != SCHEMA
            || c.project != hash(project)
            || project.is_empty()
            || c.identity.is_empty()
            || c.identity.len() > 80
            || envelope.digest != hash(&bytes(&c)?)
        {
            return Err(denied("checkpoint identity or integrity does not match"));
        }
        let attempts = c
            .attempts
            .iter()
            .enumerate()
            .map(|(i, a)| a.restore(i))
            .collect::<Result<Vec<_>, _>>()?;
        let (mut estimated, mut active, mut held) = (Cost::zero(), Cost::zero(), Cost::zero());
        for (kept, a) in c.attempts.iter().zip(&attempts) {
            match kept.phase {
                Phase::Settled => {
                    estimated = add(
                        estimated,
                        a.estimated.ok_or_else(|| denied("settlement missing"))?,
                    )?;
                }
                Phase::Active => active = add(active, a.reserved)?,
                Phase::Held => held = add(held, a.reserved)?,
                Phase::Released => {}
            }
        }
        if (estimated, active, held) != (cost(&c.estimated)?, cost(&c.active)?, cost(&c.held)?)
            || (!c.uncertain
                && c.attempts
                    .iter()
                    .any(|a| matches!(a.phase, Phase::Active | Phase::Held)))
        {
            return Err(denied("checkpoint aggregate exposure is inconsistent"));
        }
        let account = Self::new(cost(&c.limit)?)?;
        {
            let mut s = account.lock()?;
            s.identity = c.identity;
            s.estimated = estimated;
            s.active = active;
            s.held = held;
            s.status = if c.uncertain {
                AdmissionState::Uncertain
            } else {
                AdmissionState::Closed
            };
            s.refusal = c.refusal;
            s.attempts = attempts;
            // The total may exceed an explicitly lowered ceiling; preserve both.
            s.committed()?;
        }
        let mut expected = account.snapshot()?.durable_observation();
        let observed_state = c.observation["state"].as_str();
        if !matches!(observed_state, Some("Open" | "Closed" | "Uncertain"))
            || (observed_state == Some("Uncertain") && !c.uncertain)
        {
            return Err(denied("checkpoint observation state is inconsistent"));
        }
        expected["state"] = c.observation["state"].clone();
        if expected != c.observation {
            return Err(denied("checkpoint observation does not match its ledger"));
        }
        Ok((account, c.observation))
    }
}
impl KeptAttempt {
    fn capture(a: &AttemptReceipt) -> Result<Self, ProviderError> {
        if [
            a.note.as_str(),
            a.response_model.as_deref().unwrap_or_default(),
            a.request_id.as_deref().unwrap_or_default(),
        ]
        .iter()
        .any(|text| !crate::route_identity::checkpoint_text_safe(&a.endpoint, text))
        {
            return Err(denied("checkpoint would expose private route metadata"));
        }
        let phase = if a.estimated.is_some() {
            Phase::Settled
        } else if a.note == "reserved" {
            Phase::Active
        } else if a.sent {
            Phase::Held
        } else {
            Phase::Released
        };
        Ok(Self {
            id: a.id,
            route_tariff: route_tariff(a.tariff, &a.endpoint),
            reserved: a.reserved.nano_usd.to_string(),
            max_output_tokens: a.max_output_tokens,
            sent: a.sent,
            usage: a.usage.clone(),
            estimated: a.estimated.map(|v| v.nano_usd.to_string()),
            response_model: a.response_model.clone(),
            request_id: a.request_id.clone(),
            reported_estimate: a.reported_estimate.map(|v| v.nano_usd.to_string()),
            note: a.note.clone(),
            phase,
        })
    }
    fn restore(&self, index: usize) -> Result<AttemptReceipt, ProviderError> {
        let (tariff, endpoint) = nika_catalog::admission::tariffs()
            .find_map(|t| {
                t.endpoints
                    .iter()
                    .find(|endpoint| route_tariff(*t, endpoint) == self.route_tariff)
                    .map(|endpoint| (*t, *endpoint))
            })
            .ok_or_else(|| denied("checkpoint's exact tariff/route is unavailable"))?;
        let reserved = cost(&self.reserved)?;
        let estimated = self.estimated.as_deref().map(cost).transpose()?;
        let reported = self.reported_estimate.as_deref().map(cost).transpose()?;
        let price = self.usage.as_ref().and_then(|u| {
            tariff.price(
                u.input_tokens,
                u.output_tokens,
                u.cache_read_tokens.unwrap_or(0),
            )
        });
        let valid_phase = match self.phase {
            Phase::Settled => {
                self.sent
                    && estimated.is_some()
                    && estimated == price
                    && reported == price
                    && estimated.is_some_and(|c| c.nano_usd <= reserved.nano_usd)
                    && self.response_model.as_deref() == Some(tariff.model)
                    && self.usage.as_ref().is_some_and(|u| {
                        u.input_tokens <= tariff.context_tokens
                            && u.output_tokens <= u64::from(self.max_output_tokens)
                    })
            }
            Phase::Released => {
                !self.sent
                    && self.usage.is_none()
                    && estimated.is_none()
                    && reported.is_none()
                    && self.response_model.is_none()
                    && self.request_id.is_none()
            }
            Phase::Active | Phase::Held => {
                estimated.is_none()
                    && (!matches!(self.phase, Phase::Held) || self.sent)
                    && reported == price
            }
        };
        if self.id != index
            || tariff.currency != "USD"
            || tariff.reserve(self.max_output_tokens) != Some(reserved)
            || !valid_phase
        {
            return Err(denied("checkpoint attempt contradicts its accounting"));
        }
        let a = AttemptReceipt {
            id: self.id,
            model: tariff.model.into(),
            endpoint: endpoint.into(),
            tariff,
            reserved,
            max_output_tokens: self.max_output_tokens,
            sent: self.sent,
            usage: self.usage.clone(),
            estimated,
            response_model: self.response_model.clone(),
            request_id: self.request_id.clone(),
            reported_estimate: reported,
            note: self.note.clone(),
        };
        if Self::capture(&a)?.phase != self.phase {
            return Err(denied("checkpoint attempt phase contradicts its outcome"));
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests;
