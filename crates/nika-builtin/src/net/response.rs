// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Single-fetch output policy. A named status is observed data; transport
//! and extraction failures keep their existing failure and retry laws.

use nika_kernel::io::http::HttpResponse;
use serde_json::Value;

use crate::{Args, BuiltinFailure};

pub(super) struct Policy {
    accepted: Option<Vec<u16>>,
}

impl Policy {
    pub(super) fn parse(args: &Args, code: &'static str) -> Result<Self, BuiltinFailure> {
        let accepted = args
            .get("response")
            .map(nika_cap::fetch_response_statuses)
            .transpose()
            .map_err(|finding| BuiltinFailure::new(code, finding))?;
        Ok(Self { accepted })
    }

    pub(super) fn admits(&self, status: u16) -> bool {
        self.accepted
            .as_ref()
            .map_or_else(|| (200..300).contains(&status), |set| set.contains(&status))
    }

    pub(super) fn finish(self, response: &HttpResponse, body: Value) -> Value {
        if self.accepted.is_none() {
            return body;
        }
        serde_json::json!({
            "status_code": response.status,
            "url": observation_url(&response.final_url),
            "body": body,
        })
    }

    /// Preserve existing diagnostics while retaining the known status
    /// when extraction fails. The default fetch failure stays byte-identical.
    pub(super) fn annotate(&self, mut failure: BuiltinFailure, status: u16) -> BuiltinFailure {
        let Some(accepted) = &self.accepted else {
            return failure;
        };
        let mut details = match failure.details.take() {
            Some(Value::Object(map)) => map,
            Some(cause) => serde_json::Map::from_iter([("cause".to_owned(), cause)]),
            None => serde_json::Map::new(),
        };
        details.insert("status_code".to_owned(), status.into());
        details.insert("accepted".to_owned(), serde_json::json!(accepted));
        failure.with_details(Value::Object(details))
    }
}

/// A final route identity, never a credential-bearing URL. No fallback
/// to the authored URL: an unreported redirect destination is unknown.
fn observation_url(raw: &str) -> Option<String> {
    let mut url = url::Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
        return None;
    }
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_query(None);
    url.set_fragment(None);
    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_route_strips_credentials_query_and_fragment() {
        assert_eq!(
            observation_url("https://user:pass@EXAMPLE.test/login?api_key=hidden#private")
                .expect("URL"),
            "https://example.test/login"
        );
        for missing in [
            "",
            "not a URL",
            "data:text/plain,secret",
            "file:///private/file",
        ] {
            assert_eq!(observation_url(missing), None);
        }
    }
}
