// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Opt-in OpenAI-compatible model-list observations, over the existing HTTP seam.
use nika_kernel::http::{HttpGetDyn, HttpRequest};
use std::{collections::BTreeSet, time::Duration};

use nika_kernel::secret::Secret;
use nika_providers_access::probe::{KeyAuth, ModelListing, classify_http_status};

/// The listing one HTTP answer states: a compatible list, or why not.
fn listing_of(status: u16, body: &[u8]) -> ModelListing {
    let mut out = ModelListing::new("model-list response is not compatible");
    out.http_status = Some(status);
    out.protocol_compatible = Some(false);
    if status != 200 {
        out.failure = Some(format!(
            "model-list HTTP status {status}; redirects are not followed"
        ));
        return out;
    }
    if body.len() > 262_144 {
        out.failure = Some("model-list response exceeds 256 KiB".into());
        return out;
    }
    let Some(models) = model_ids(body) else {
        return out;
    };
    out.models = models;
    out.protocol_compatible = Some(true);
    out.failure = None;
    out
}

fn model_ids(body: &[u8]) -> Option<Vec<String>> {
    let json = crate::wire::bounded_json::parse(body).ok()?;
    if json
        .get("object")
        .is_some_and(|v| v.as_str() != Some("list"))
    {
        return None;
    }
    let rows = json.get("data")?.as_array()?;
    if rows.len() > 1024 {
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut ids = Vec::with_capacity(rows.len());
    for row in rows {
        if row
            .get("object")
            .is_some_and(|v| v.as_str() != Some("model"))
        {
            return None;
        }
        let id = row.get("id")?.as_str()?;
        if id.trim().is_empty()
            || id.len() > 512
            || id.chars().any(char::is_control)
            || !seen.insert(id)
        {
            return None;
        }
        ids.push(id.to_owned());
    }
    Some(ids)
}

/// Send one bounded, bodyless GET to an already validated model-list endpoint.
/// The host opts in, selects local-protocol profiles, strips URL credentials,
/// and supplies a transport with retries disabled and a 256-KiB body limit.
/// No inference, download, redirect, auth header, or fallback is requested.
pub async fn probe_model_listing<H: HttpGetDyn>(http: &H, endpoint: &str) -> ModelListing {
    let mut request = HttpRequest::get(endpoint);
    request.follow_redirects = false;
    request.timeout = Some(Duration::from_millis(1000));
    match http.get(request).await {
        Ok(response) if response.final_url == endpoint => {
            listing_of(response.status, &response.body)
        }
        Ok(_) => ModelListing::new("model-list transport changed the endpoint"),
        Err(_) => ModelListing::new("model-list transport did not return a usable response"),
    }
}

/// Send one bounded, authenticated GET to a keyed cloud route's own listing endpoint
/// ([`crate::ProviderRegistry::cloud_listing_target`]), at census time: the route's key rides one
/// `authorization` header of this one request and nothing records it; no redirect is followed and
/// nothing is re-sent, inferred or downloaded. The answer states the key's rung and the listing:
/// a 200 lists the route's models; a 401 or 403 refuses the key, with the provider's own words
/// reduced as every provider error is (the key withheld); `None` when no HTTP answer could be
/// judged, so the key stays configured, not validated.
pub async fn probe_cloud_listing<H: HttpGetDyn>(
    http: &H,
    endpoint: &str,
    key: &Secret,
    deadline: Duration,
) -> (Option<KeyAuth>, ModelListing) {
    let mut request = HttpRequest::get(endpoint);
    request.follow_redirects = false;
    request.timeout = Some(deadline);
    request.headers.insert(
        "authorization".to_owned(),
        format!("Bearer {}", key.expose()),
    );
    match http.get(request).await {
        Ok(response) if response.final_url == endpoint => {
            let auth = classify_http_status(response.status);
            let mut listing = listing_of(response.status, &response.body);
            if matches!(auth, KeyAuth::Unauthorized | KeyAuth::Forbidden) {
                let said =
                    crate::wire::status_error(response.status, &response.body, None, Some(key));
                listing.failure = Some(format!("the provider refused the key: {said}"));
            }
            (Some(auth), listing)
        }
        Ok(_) => (
            None,
            ModelListing::new("model-list transport changed the endpoint"),
        ),
        Err(_) => (
            None,
            ModelListing::new("model-list transport did not return a usable response"),
        ),
    }
}

#[cfg(test)]
mod tests;
