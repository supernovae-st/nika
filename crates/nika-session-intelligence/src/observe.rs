// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One public page a conversation's intelligence looks at while it prepares (`observe`): a GET
//! through the fetch client's guard on every hop (no private, local or credentialed address),
//! a short deadline and a small body cap. The reply is what a person would see of the page,
//! never its whole text: its status, final address, content type, title and size.

use std::time::Duration;

use nika_http::{HttpConfig, ReqwestHttp};
use nika_kernel::HttpRequest;
use nika_kernel::http::HttpGetDyn;
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
    let response = block_on(async { http.get(HttpRequest::get(url)).await })
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("`{url}` was not observed: {e}"))?;
    let text = String::from_utf8_lossy(&response.body);
    Ok(json!({
        "status": response.status,
        "final_url": response.final_url,
        "content_type": response.headers.get("content-type"),
        "title": title(&text),
        "bytes": response.body.len(),
    }))
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
