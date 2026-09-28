// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Opt-in OpenAI-compatible model-list observations, over the existing HTTP seam.
use nika_kernel::http::{HttpGetDyn, HttpRequest};
use std::{collections::BTreeSet, time::Duration};

/// A model-list response, not evidence that any advertised model can infer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ModelListing {
    /// None when no HTTP response could be judged.
    pub protocol_compatible: Option<bool>,
    /// Observed status, absent on transport failure.
    pub http_status: Option<u16>,
    /// Validated advertised identifiers; never downloaded or invoked.
    pub models: Vec<String>,
    /// Bounded diagnostic without endpoint credentials or response contents.
    pub failure: Option<String>,
}
impl ModelListing {
    /// An unobserved capability with a fixed, non-secret reason.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            protocol_compatible: None,
            http_status: None,
            models: Vec::new(),
            failure: Some(reason.into()),
        }
    }
    /// Whether a compatible listing advertises any model. Incompatible/unobserved
    /// data cannot prove that no model exists.
    #[must_use]
    pub fn available(&self) -> Option<bool> {
        (self.protocol_compatible == Some(true)).then_some(!self.models.is_empty())
    }
    fn response(status: u16, body: &[u8]) -> Self {
        let mut out = Self::new("model-list response is not compatible");
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
            ModelListing::response(response.status, &response.body)
        }
        Ok(_) => ModelListing::new("model-list transport changed the endpoint"),
        Err(_) => ModelListing::new("model-list transport did not return a usable response"),
    }
}

#[cfg(test)]
mod tests;
