// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One public page a conversation's intelligence looks at while it prepares (`observe`): a GET
//! through the fetch client's guard on every hop (no private, local or credentialed address),
//! a short deadline and a small body cap. The reply is what a person would see of the page,
//! never its whole text: its status, final address, content type, title and size. A trial keeps
//! the page whole instead ([`capture`]), through the same guard and bounds.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nika_http::{HttpConfig, ReqwestHttp};
use nika_kernel::HttpRequest;
use nika_kernel::http::{HttpGetDyn, HttpResponse};
use nika_service_execution::replay::Capture;
use serde_json::{Value, json};

use crate::reasoner::block_on;

/// The bytes a page may answer before the observation is refused.
const OBSERVED_BYTES: u64 = 512 * 1024;
/// How long a page may take.
const OBSERVED_FOR: Duration = Duration::from_secs(15);

/// Fetch `url` once with GET and say what a person would see of it.
///
/// # Errors
///
/// The address is not a public `http(s)` address without credentials, the guard refused a hop,
/// the page is too large or too slow, or the preparation was stopped.
pub fn observe(url: &str) -> Result<Value, String> {
    let response = guarded(url)?;
    let text = String::from_utf8_lossy(&response.body);
    Ok(json!({
        "status": response.status,
        "final_url": response.final_url,
        "content_type": response.headers.get("content-type"),
        "title": title(&text),
        "bytes": response.body.len(),
    }))
}

/// Fetch `url` once with GET, as [`observe`] does, and keep the page whole for a trial: its
/// exact bytes, its status and content type, and when it was taken, at the address asked.
///
/// # Errors
///
/// As [`observe`].
pub fn capture(url: &str) -> Result<Capture, String> {
    let response = guarded(url)?;
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let at = u64::try_from(since.as_millis()).unwrap_or(u64::MAX);
    let kind = response.headers.get("content-type").cloned();
    let body = response.body.to_vec();
    Ok(Capture::new(url.trim(), response.status, kind, body, at))
}

/// One guarded GET of a public `http(s)` address without credentials, within the bounds.
fn guarded(url: &str) -> Result<HttpResponse, String> {
    let url = url.trim();
    let rest = (url.strip_prefix("https://"))
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(|| format!("`{url}` is not an http(s) address"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    if authority.contains('@') {
        return Err(format!(
            "`{url}` carries credentials; only public pages are observed"
        ));
    }
    let mut config = HttpConfig::new();
    config.timeout = OBSERVED_FOR;
    config.max_response_bytes = OBSERVED_BYTES;
    let http = ReqwestHttp::with_config(config).map_err(|e| e.to_string())?;
    block_on(async { http.get(HttpRequest::get(url)).await })
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("`{url}` was not observed: {e}"))
}

/// The page's `<title>`, its spacing folded, when it has one.
fn title(page: &str) -> Option<String> {
    let lower = page.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let words: Vec<&str> = page.get(start..end)?.split_whitespace().collect();
    (!words.is_empty()).then(|| words.join(" ").chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_public_address_without_credentials_is_observed() {
        for refused in [
            "ftp://example.org",
            "example.org",
            "https://user:pw@example.org/x",
        ] {
            assert!(observe(refused).is_err(), "{refused}");
            assert!(capture(refused).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_title_is_read_with_its_spacing_folded() {
        let page = "<html><head><TITLE>\n  Hacker   News\n</TITLE></head>";
        assert_eq!(title(page).as_deref(), Some("Hacker News"));
        assert_eq!(title("<html>no title</html>"), None);
        assert_eq!(title("<title></title>"), None);
    }
}
