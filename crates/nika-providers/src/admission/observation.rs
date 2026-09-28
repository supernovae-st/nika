// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a written observation says about the account that wrote it: the reading
//! law of [`InferenceReceipt::observation`](super::InferenceReceipt::observation),
//! kept beside its serializer. The cost journal judges its rows with it; the
//! journal's own law (phases, leases, digests, transitions) stays with the journal.
use super::AdmissionState;
use serde_json::Value;

/// Whether an observation reads as one this serializer writes: its schema, a
/// known subtotal that parses, an unknown-call count and one of the three states.
#[must_use]
pub fn observation_readable(observation: &Value) -> bool {
    observation["schema"] == "nika/inference-cost-observation@1"
        && observation["known_subtotal_nano_usd"]
            .as_str()
            .and_then(|v| v.parse::<i128>().ok())
            .is_some()
        && observation["unknown_calls"].as_u64().is_some()
        && matches!(
            observation["state"].as_str(),
            Some("Open" | "Closed" | "Uncertain")
        )
}

/// Whether a readable observation is one its account could have written, and
/// then its state and whether any attempt moved it. The account counts as
/// unknown exactly the sent attempts it could not price and adds only
/// nonnegative estimates of sent attempts to its known subtotal. A state other
/// than `Open` or `Closed` reads as `Uncertain`, never as cleared.
///
/// # Errors
/// Why the observation contradicts the account that would have written it.
pub fn observation_consistent(observation: &Value) -> Result<(AdmissionState, bool), &'static str> {
    let (Some(priced), Some(unpriced)) = (
        observation["attempts"].as_array(),
        observation["unknown_attempts"].as_array(),
    ) else {
        return Err("records its account without both attempt lists");
    };
    let (mut unknown_calls, mut known) = (0_u64, 0_i128);
    for attempt in priced.iter().chain(unpriced) {
        match (attempt["sent"].as_bool(), &attempt["estimated_nano_usd"]) {
            (Some(true), Value::Null) => unknown_calls += 1,
            (Some(false), Value::Null) => {}
            (Some(true), Value::String(nano)) => {
                let estimate = nano.parse::<i128>().ok().filter(|value| *value >= 0);
                known = estimate
                    .and_then(|value| known.checked_add(value))
                    .ok_or("records an attempt its account cannot write")?;
            }
            _ => return Err("records an attempt its account cannot write"),
        }
    }
    let subtotal = observation["known_subtotal_nano_usd"]
        .as_str()
        .and_then(|nano| nano.parse::<i128>().ok());
    match subtotal {
        Some(nano) if nano < 0 => return Err("reports a negative known subtotal"),
        Some(nano) if nano == known => {}
        _ => return Err("reports a known subtotal its attempts do not add up to"),
    }
    if observation["unknown_calls"].as_u64() != Some(unknown_calls) {
        return Err("counts unknown calls its sent attempts do not record");
    }
    let state = match observation["state"].as_str() {
        Some("Open") => AdmissionState::Open,
        Some("Closed") => AdmissionState::Closed,
        _ => AdmissionState::Uncertain,
    };
    Ok((state, !(priced.is_empty() && unpriced.is_empty())))
}

/// The route an observation's unknown-cost choice names, as it was written
/// (`provider`, `model`, `endpoint`), or null when it names none.
#[must_use]
pub fn observation_route(observation: &Value) -> Value {
    let choice = &observation["unknown_cost"];
    if choice.is_object() {
        serde_json::json!({"provider": choice["provider"], "model": choice["model"],
            "endpoint": choice["endpoint"]})
    } else {
        Value::Null
    }
}

/// Every provider request id the observation's attempts record, catalog
/// attempts first, in the order they were written.
#[must_use]
pub fn observation_request_ids(observation: &Value) -> Vec<&str> {
    ["attempts", "unknown_attempts"]
        .iter()
        .filter_map(|list| observation[*list].as_array())
        .flatten()
        .filter_map(|a| a["request_id"].as_str())
        .collect()
}
