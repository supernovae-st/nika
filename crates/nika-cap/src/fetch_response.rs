// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure shape law for the single-fetch response observation policy.
//! Static checking may defer expressions, but still judges every literal
//! sibling. Runtime parsing accepts only the complete resolved exact set.

use nika_types::net::MAX_FETCH_RESPONSE_STATUSES;
use serde_json::Value;

/// Parse a resolved `nika:fetch response:` object into its exact accepted
/// final HTTP statuses. This grants no network permission and changes no
/// retry policy. Expressions, unknown fields and an empty set are errors.
///
/// # Errors
/// Returns a path-bearing shape finding for anything other than
/// `{ "accept": [<distinct integers in 200..=599>] }`.
pub fn fetch_response_statuses(value: &Value) -> Result<Vec<u16>, String> {
    statuses(value, false)
}

/// Judge the same law statically without granting unresolved statuses.
pub(crate) fn check(value: &Value) -> Result<(), String> {
    statuses(value, true).map(|_| ())
}

fn deferred(value: &Value, static_check: bool) -> bool {
    static_check && value.as_str().is_some_and(|s| s.contains("${{"))
}

fn statuses(value: &Value, static_check: bool) -> Result<Vec<u16>, String> {
    if deferred(value, static_check) {
        return Ok(Vec::new());
    }
    let object = value
        .as_object()
        .ok_or("`response:` must be an object with an `accept:` array")?;
    if let Some(unknown) = object.keys().find(|key| key.as_str() != "accept") {
        return Err(format!(
            "`response.{unknown}:` is not a response field — the shape is closed"
        ));
    }
    let accept = object
        .get("accept")
        .ok_or("`response.accept:` is required — list the exact accepted final HTTP statuses")?;
    if deferred(accept, static_check) {
        return Ok(Vec::new());
    }
    let items = accept
        .as_array()
        .ok_or("`response.accept:` must be a nonempty array of distinct integers 200..=599")?;
    if items.is_empty() || items.len() > MAX_FETCH_RESPONSE_STATUSES {
        return Err(format!(
            "`response.accept:` must contain 1..={MAX_FETCH_RESPONSE_STATUSES} distinct final HTTP statuses"
        ));
    }
    let mut accepted = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        if deferred(item, static_check) {
            continue;
        }
        let status = item
            .as_u64()
            .filter(|n| (200..=599).contains(n))
            .and_then(|n| u16::try_from(n).ok())
            .ok_or_else(|| format!("`response.accept[{index}]:` must be an integer 200..=599"))?;
        if accepted.contains(&status) {
            return Err(format!(
                "`response.accept[{index}]:` repeats status {status} — statuses must be distinct"
            ));
        }
        accepted.push(status);
    }
    Ok(accepted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_final_status_set_preserves_order_without_implicit_success() {
        assert_eq!(
            fetch_response_statuses(&json!({"accept": [404, 200, 599]})),
            Ok(vec![404, 200, 599])
        );
        assert_eq!(
            fetch_response_statuses(&json!({"accept": [404]})),
            Ok(vec![404])
        );
    }

    #[test]
    fn malformed_policy_never_grants_a_runtime_status() {
        assert!(
            fetch_response_statuses(&json!({"accept": (200..217).collect::<Vec<_>>()})).is_err()
        );
        for value in [
            Value::Null,
            json!(true),
            json!([]),
            json!("all"),
            json!({}),
            json!({"accept": []}),
            json!({"accept": "200"}),
            json!({"accept": [true]}),
            json!({"accept": [200.0]}),
            json!({"accept": [199]}),
            json!({"accept": [600]}),
            json!({"accept": [200, 200]}),
            json!({"accept": [200], "other": true}),
            json!({"accept": "${{ inputs.codes }}"}),
            json!({"accept": ["${{ inputs.code }}"]}),
            json!("${{ inputs.response }}"),
        ] {
            assert!(fetch_response_statuses(&value).is_err(), "{value}");
        }
    }

    #[test]
    fn static_expressions_do_not_hide_invalid_literal_siblings() {
        for value in [
            json!({"accept": ["${{ inputs.code }}", 99]}),
            json!({"accept": ["${{ inputs.code }}", 404, 404]}),
            json!({"accept": "${{ inputs.codes }}", "other": true}),
        ] {
            assert!(check(&value).is_err(), "{value}");
        }
        for value in [
            json!("${{ inputs.response }}"),
            json!({"accept": "${{ inputs.codes }}"}),
            json!({"accept": [200, "${{ inputs.code }}"]}),
        ] {
            assert_eq!(check(&value), Ok(()));
            assert!(fetch_response_statuses(&value).is_err());
        }
    }
}
