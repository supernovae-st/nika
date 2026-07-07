// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:fetch` `traverse:` — the bounded same-origin crawl
//! (stdlib §fetch · traverse). What site-brief helpers hand-roll as a
//! Node script becomes one declarative arg:
//!
//! ```yaml
//! invoke:
//!   tool: "nika:fetch"
//!   args:
//!     url: "${{ vars.site_url }}"
//!     traverse: { max_pages: 5 }
//! ```
//!
//! Semantics (all bounds are hard):
//!
//! - **Same-origin BFS from `url:`** — links come from each page's
//!   digest ([`nika_extract::page_digest`]), only the ROOT's origin is
//!   enqueued, fragments are stripped for dedup, and the frontier caps
//!   at `max_pages × 8` (a link farm cannot balloon the queue).
//! - **`max_pages` (1..=[`MAX_TRAVERSE_PAGES`])** bounds page REQUESTS —
//!   the effect certificate counts exactly `max_pages` page calls
//!   (+1 robots probe unless `respect_robots: false`), one definition
//!   with `nika-schema`'s certificate via `nika_types::net`.
//! - **`respect_robots` (default `true`)** — one GET of
//!   `{origin}/robots.txt`; `User-agent: *` `Disallow:` prefixes are
//!   honored (a fetch error or non-2xx = no robots = allow-all, the
//!   crawler-standard fallback). A disallowed ROOT is a loud failure;
//!   disallowed descendants are silently not enqueued.
//! - **Per-page SSRF re-vetting** — every hop goes through the SAME
//!   injected guarded client as a single fetch (the L1 3-layer
//!   defense); this module never re-implements the guard.
//! - **Failures degrade per page** — the root failing is the builtin
//!   failing (nothing was crawled · loud beats empty, the single-fetch
//!   contract); a descendant failing becomes an honest
//!   `{url, status|error}` page entry and the crawl continues.
//!
//! Output (the fixed crawl shape · stdlib §fetch):
//!
//! ```json
//! { "url": "<root>", "page_count": N,
//!   "pages": [ { "url", "status", ...page-digest } | { "url", "status"|"error" } ],
//!   "assets": { "images": [..≤40], "colors": [..≤30] } }
//! ```

use std::collections::VecDeque;

use nika_extract::page_digest;
use nika_kernel::io::http::{HttpGetDyn, HttpRequest};
use nika_types::net::MAX_TRAVERSE_PAGES;

use crate::{Args, BuiltinFailure, BuiltinOutcome};

const C: &str = "NIKA-BUILTIN-FETCH-001";

/// Aggregate-asset caps (across the whole crawl · first-seen order).
const MAX_ASSET_IMAGES: usize = 40;
const MAX_ASSET_COLORS: usize = 30;
/// Frontier multiplier — pending links per page budget.
const FRONTIER_PER_PAGE: u64 = 8;

/// The parsed `traverse:` spec (closed shape · unknown keys rejected).
struct TraverseSpec {
    max_pages: u64,
    respect_robots: bool,
}

/// Run the bounded crawl. Reached from `net::fetch` when `traverse:`
/// is present — arg exclusivity is vetted HERE (before any request).
pub(crate) async fn traverse<H: HttpGetDyn>(http: &H, root: &str, args: &Args) -> BuiltinOutcome {
    let spec = parse_traverse_args(args)?;
    let root_url = url::Url::parse(root)
        .map_err(|e| BuiltinFailure::new(C, format!("`url: {root}` does not parse: {e}")))?;
    if !matches!(root_url.scheme(), "http" | "https") {
        return Err(BuiltinFailure::new(
            C,
            format!(
                "`traverse:` crawls http(s) only — got `{}`",
                root_url.scheme()
            ),
        ));
    }
    let origin = root_url.origin();

    let disallows = if spec.respect_robots {
        fetch_robots_disallows(http, &root_url).await
    } else {
        Vec::new()
    };
    if robots_blocks(&disallows, root_url.path()) {
        return Err(BuiltinFailure::new(
            C,
            format!(
                "robots.txt disallows the root `{root}` — nothing to crawl (set `respect_robots: false` only if you own the site)"
            ),
        ));
    }

    let mut queue: VecDeque<String> = VecDeque::new();
    let mut visited: Vec<String> = Vec::new();
    let mut pages: Vec<serde_json::Value> = Vec::new();
    let mut asset_images: Vec<String> = Vec::new();
    let mut asset_colors: Vec<String> = Vec::new();
    queue.push_back(normalize(root_url.clone()));

    while let Some(page_url) = queue.pop_front() {
        if pages.len() as u64 >= spec.max_pages {
            break;
        }
        if visited.contains(&page_url) {
            continue;
        }
        visited.push(page_url.clone());
        let is_root = pages.is_empty();

        let response = match http.get(HttpRequest::get(&page_url)).await {
            Ok(response) => response,
            Err(e) => {
                // The root failing IS the builtin failing (single-fetch
                // contract); a descendant records an honest error entry —
                // security blocks included (the request was BLOCKED,
                // nothing left the machine · recording is honest).
                if is_root {
                    return Err(crate::net::net_security_failure(&e).unwrap_or_else(|| {
                        BuiltinFailure::new(C, format!("request failed: {e}"))
                    }));
                }
                pages.push(serde_json::json!({ "url": page_url, "error": e.to_string() }));
                continue;
            }
        };
        if !(200..300).contains(&response.status) {
            if is_root {
                return Err(BuiltinFailure::new(
                    C,
                    format!("HTTP {} from {page_url}", response.status),
                )
                .with_transient(crate::net::is_transient_status(response.status))
                .with_details(serde_json::json!({ "status_code": response.status })));
            }
            pages.push(serde_json::json!({ "url": page_url, "status": response.status }));
            continue;
        }

        let text = crate::net::decode_charset(
            &response.body,
            response.headers.get("content-type").map(String::as_str),
        )
        .into_owned();
        let base = if response.final_url.is_empty() {
            page_url.clone()
        } else {
            response.final_url.clone()
        };
        let status = response.status;
        // CPU-heavy DOM parse rides the blocking pool (the single-fetch
        // extraction precedent — same bounded-orphan cancel contract).
        let digest = tokio::task::spawn_blocking(move || page_digest(&text, Some(&base)))
            .await
            .map_err(|e| BuiltinFailure::new(C, format!("extraction task failed: {e}")))?;

        enqueue_links(&digest, &origin, &disallows, &visited, &mut queue, &spec);
        collect_assets(&digest, &mut asset_images, &mut asset_colors);
        pages.push(page_entry(&page_url, status, digest));
    }

    Ok(serde_json::json!({
        "url": root,
        "page_count": pages.len(),
        "pages": pages,
        "assets": { "images": asset_images, "colors": asset_colors },
    }))
}

/// Vet the whole-arg surface + parse the spec: `traverse:` excludes the
/// single-fetch extraction/payload families, forces GET, and its own
/// shape is closed. All of it fails BEFORE any request is spent.
fn parse_traverse_args(args: &Args) -> Result<TraverseSpec, BuiltinFailure> {
    for key in ["mode", "selector", "jq", "body", "form", "multipart"] {
        if args.contains_key(key) {
            return Err(BuiltinFailure::new(
                C,
                format!(
                    "`traverse:` excludes `{key}:` — the crawl emits the fixed \
                     page-digest shape (builtins-v0.1.md §nika:fetch · traverse)"
                ),
            ));
        }
    }
    if let Some(method) = args.get("method").and_then(serde_json::Value::as_str)
        && !method.eq_ignore_ascii_case("GET")
    {
        return Err(BuiltinFailure::new(
            C,
            format!("`traverse:` crawls with GET only — drop `method: {method}`"),
        ));
    }
    let spec = args
        .get("traverse")
        .ok_or_else(|| BuiltinFailure::new(C, "internal: traverse arg vanished"))?;
    let Some(map) = spec.as_object() else {
        return Err(BuiltinFailure::new(
            C,
            "`traverse:` must be an object — `{ max_pages: N, respect_robots?: bool }`",
        ));
    };
    if let Some(unknown) = map
        .keys()
        .find(|k| !matches!(k.as_str(), "max_pages" | "respect_robots"))
    {
        return Err(BuiltinFailure::new(
            C,
            format!("`traverse.{unknown}:` is not a traverse field — the shape is closed"),
        ));
    }
    let max_pages = map
        .get("max_pages")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            BuiltinFailure::new(
                C,
                "`traverse.max_pages:` is required — an integer 1..=25 (the crawl bound)",
            )
        })?;
    if !(1..=MAX_TRAVERSE_PAGES).contains(&max_pages) {
        return Err(BuiltinFailure::new(
            C,
            format!("`traverse.max_pages: {max_pages}` out of range — 1..={MAX_TRAVERSE_PAGES}"),
        ));
    }
    let respect_robots = match map.get("respect_robots") {
        None => true,
        Some(serde_json::Value::Bool(flag)) => *flag,
        Some(_) => {
            return Err(BuiltinFailure::new(
                C,
                "`traverse.respect_robots:` must be a boolean",
            ));
        }
    };
    Ok(TraverseSpec {
        max_pages,
        respect_robots,
    })
}

/// Push a successful page's same-origin links onto the frontier —
/// normalized, robots-filtered, dedup'd, frontier-capped.
fn enqueue_links(
    digest: &serde_json::Value,
    origin: &url::Origin,
    disallows: &[String],
    visited: &[String],
    queue: &mut VecDeque<String>,
    spec: &TraverseSpec,
) {
    let Some(links) = digest.get("links").and_then(serde_json::Value::as_array) else {
        return;
    };
    // max_pages ≤ 25 and the multiplier is 8 — the product fits every
    // pointer width (belt: saturate, never truncate).
    let frontier_cap = usize::try_from(spec.max_pages * FRONTIER_PER_PAGE).unwrap_or(usize::MAX);
    for link in links.iter().filter_map(serde_json::Value::as_str) {
        if queue.len() >= frontier_cap {
            break;
        }
        let Ok(parsed) = url::Url::parse(link) else {
            continue;
        };
        if parsed.origin() != *origin || robots_blocks(disallows, parsed.path()) {
            continue;
        }
        let normalized = normalize(parsed);
        if !visited.contains(&normalized) && !queue.contains(&normalized) {
            queue.push_back(normalized);
        }
    }
}

/// Fold a page's digest images/colors into the crawl-level asset pools
/// (first-seen dedup · capped).
fn collect_assets(digest: &serde_json::Value, images: &mut Vec<String>, colors: &mut Vec<String>) {
    let fold = |key: &str, pool: &mut Vec<String>, cap: usize| {
        if let Some(items) = digest.get(key).and_then(serde_json::Value::as_array) {
            for item in items.iter().filter_map(serde_json::Value::as_str) {
                if pool.len() >= cap {
                    break;
                }
                if !pool.iter().any(|seen| seen == item) {
                    pool.push(item.to_owned());
                }
            }
        }
    };
    fold("images", images, MAX_ASSET_IMAGES);
    fold("colors", colors, MAX_ASSET_COLORS);
}

/// One page entry: `{url, status}` + every digest facet.
fn page_entry(page_url: &str, status: u16, digest: serde_json::Value) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert("url".to_owned(), page_url.into());
    entry.insert("status".to_owned(), status.into());
    if let serde_json::Value::Object(fields) = digest {
        entry.extend(fields);
    }
    serde_json::Value::Object(entry)
}

/// Dedup key: fragment stripped (the anchor never changes the resource).
fn normalize(mut parsed: url::Url) -> String {
    parsed.set_fragment(None);
    parsed.to_string()
}

/// GET `{origin}/robots.txt` — errors and non-2xx mean "no robots"
/// (allow-all · the crawler-standard fallback, never a crawl failure).
async fn fetch_robots_disallows<H: HttpGetDyn>(http: &H, root: &url::Url) -> Vec<String> {
    let Ok(robots_url) = root.join("/robots.txt") else {
        return Vec::new();
    };
    match http.get(HttpRequest::get(robots_url.as_str())).await {
        Ok(response) if (200..300).contains(&response.status) => {
            robots_disallows(&String::from_utf8_lossy(&response.body))
        }
        _ => Vec::new(),
    }
}

/// `User-agent: *` group `Disallow:` path prefixes (the 1994 de-facto
/// grammar — groups separated by blank lines; a group applies when any
/// of its `User-agent` lines is `*`). Comments (`#…`) stripped. An
/// EMPTY `Disallow:` means allow-all (skipped).
fn robots_disallows(body: &str) -> Vec<String> {
    let mut disallows = Vec::new();
    let mut group_is_star = false;
    let mut in_agent_run = false;
    for raw in body.lines() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            group_is_star = false;
            in_agent_run = false;
            continue;
        }
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if field.eq_ignore_ascii_case("user-agent") {
            // Consecutive User-agent lines share one group; a User-agent
            // AFTER rules starts a new group.
            if !in_agent_run {
                group_is_star = false;
                in_agent_run = true;
            }
            group_is_star |= value == "*";
        } else {
            in_agent_run = false;
            if field.eq_ignore_ascii_case("disallow") && group_is_star && !value.is_empty() {
                disallows.push(value.to_owned());
            }
        }
    }
    disallows
}

/// Path-prefix match (the de-facto Disallow semantics).
fn robots_blocks(disallows: &[String], path: &str) -> bool {
    disallows.iter().any(|prefix| path.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel_mock::MockHttp;

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    fn page(title: &str, links: &[&str]) -> Vec<u8> {
        use std::fmt::Write as _;
        let mut anchors = String::new();
        for l in links {
            // Writing to a String is infallible.
            let _ = write!(anchors, "<a href=\"{l}\">x</a>");
        }
        format!(
            "<html><head><title>{title}</title></head><body>\
             <h1>{title}</h1><img src=\"/img/{title}.png\"> #0400ff {anchors}</body></html>"
        )
        .into_bytes()
    }

    #[tokio::test]
    async fn traverse_crawls_same_origin_bfs_with_dedup() {
        // robots 404 (allow-all) · root links to /a twice, /b, and an
        // offsite URL · /a links back to root (visited — skipped).
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(
                200,
                page("root", &["/a", "/a#frag", "/b", "https://other.test/x"]),
            )
            .enqueue_ok(200, page("a", &["/"]))
            .enqueue_ok(200, page("b", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await
        .expect("crawl succeeds");
        assert_eq!(out["page_count"], 3, "{out}");
        let urls: Vec<&str> = out["pages"]
            .as_array()
            .expect("pages")
            .iter()
            .filter_map(|p| p["url"].as_str())
            .collect();
        assert_eq!(
            urls,
            vec![
                "https://acme.test/",
                "https://acme.test/a",
                "https://acme.test/b"
            ],
            "BFS order · fragment-dedup'd · offsite excluded"
        );
        // 4 requests total: robots + 3 pages (root revisit skipped).
        assert_eq!(http.sent_requests().len(), 4);
        let images = out["assets"]["images"].as_array().expect("images");
        assert_eq!(images.len(), 3, "one per page, dedup'd: {images:?}");
        assert_eq!(out["assets"]["colors"], serde_json::json!(["#0400ff"]));
        assert_eq!(out["pages"][0]["title"], "root", "digest fields ride");
    }

    #[tokio::test]
    async fn traverse_max_pages_bounds_the_crawl() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(200, page("root", &["/a", "/b", "/c", "/d"]))
            .enqueue_ok(200, page("a", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 2 }
            })),
        )
        .await
        .expect("bounded crawl");
        assert_eq!(out["page_count"], 2);
        assert_eq!(http.sent_requests().len(), 3, "robots + exactly 2 pages");
    }

    #[tokio::test]
    async fn traverse_respects_robots_disallow() {
        let http = MockHttp::new()
            .enqueue_ok(200, b"User-agent: *\nDisallow: /private\n".to_vec())
            .enqueue_ok(200, page("root", &["/private/x", "/ok"]))
            .enqueue_ok(200, page("ok", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await
        .expect("crawl succeeds");
        assert_eq!(out["page_count"], 2);
        assert!(
            !http
                .sent_requests()
                .iter()
                .any(|r| r.url.contains("/private")),
            "disallowed path never requested"
        );
    }

    #[tokio::test]
    async fn traverse_root_disallowed_is_loud() {
        let http = MockHttp::new().enqueue_ok(200, b"User-agent: *\nDisallow: /\n".to_vec());
        let fail = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == C && f.message.contains("robots.txt")),
            "{fail:?}"
        );
        assert_eq!(http.sent_requests().len(), 1, "only robots was fetched");
    }

    #[tokio::test]
    async fn traverse_respect_robots_false_skips_the_probe() {
        let http = MockHttp::new().enqueue_ok(200, page("root", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/",
                "traverse": { "max_pages": 1, "respect_robots": false }
            })),
        )
        .await
        .expect("crawl succeeds");
        assert_eq!(out["page_count"], 1);
        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        assert!(!sent[0].url.contains("robots"), "no robots probe");
    }

    #[tokio::test]
    async fn traverse_records_failed_descendants_and_continues() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(200, page("root", &["/broken", "/ok"]))
            .enqueue_ok(500, Vec::new())
            .enqueue_ok(200, page("ok", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await
        .expect("crawl survives a broken page");
        let pages = out["pages"].as_array().expect("pages");
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[1]["status"], 500, "honest failure entry");
        assert!(pages[1].get("title").is_none(), "no digest on a failure");
        assert_eq!(pages[2]["title"], "ok", "crawl continued");
    }

    #[tokio::test]
    async fn traverse_root_failure_propagates_like_single_fetch() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(503, Vec::new());
        let fail = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 3 }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == C && f.message.contains("503") && f.transient),
            "{fail:?}"
        );
    }

    #[tokio::test]
    async fn traverse_excludes_extraction_and_payload_args() {
        let http = MockHttp::new();
        for extra in [
            serde_json::json!({ "mode": "raw" }),
            serde_json::json!({ "jq": ".x" }),
            serde_json::json!({ "body": "x" }),
            serde_json::json!({ "form": { "a": "b" } }),
            serde_json::json!({ "method": "POST" }),
        ] {
            let mut base = args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 2 }
            }));
            if let serde_json::Value::Object(extra) = extra {
                base.extend(extra);
            }
            let fail = traverse(&http, "https://acme.test/", &base).await;
            assert!(
                matches!(&fail, Err(f) if f.code == C),
                "conflicting arg must refuse: {fail:?}"
            );
        }
        assert!(http.sent_requests().is_empty(), "no request spent");
    }

    #[tokio::test]
    async fn traverse_spec_shape_violations_are_loud() {
        let http = MockHttp::new();
        for (spec, needle) in [
            (serde_json::json!("later"), "must be an object"),
            (serde_json::json!({}), "required"),
            (serde_json::json!({ "max_pages": 0 }), "out of range"),
            (serde_json::json!({ "max_pages": 26 }), "out of range"),
            (
                serde_json::json!({ "max_pages": 2, "depth": 3 }),
                "not a traverse field",
            ),
            (
                serde_json::json!({ "max_pages": 2, "respect_robots": "yes" }),
                "boolean",
            ),
        ] {
            let fail = traverse(
                &http,
                "https://acme.test/",
                &args(serde_json::json!({ "url": "https://acme.test/", "traverse": spec })),
            )
            .await;
            assert!(
                matches!(&fail, Err(f) if f.code == C && f.message.contains(needle)),
                "wanted `{needle}` in {fail:?}"
            );
        }
    }

    #[test]
    fn robots_grammar_parses_star_groups_only() {
        let body = "User-agent: googlebot\nDisallow: /g\n\n\
                    User-agent: *\nUser-agent: extra\nDisallow: /private # comment\n\
                    Disallow:\nAllow: /public\nDisallow: /tmp\n\n\
                    User-agent: bing\nDisallow: /b\n";
        let disallows = robots_disallows(body);
        assert_eq!(disallows, vec!["/private".to_owned(), "/tmp".to_owned()]);
        assert!(robots_blocks(&disallows, "/private/deep"));
        assert!(!robots_blocks(&disallows, "/public"));
        assert!(!robots_blocks(&[], "/anything"), "no robots = allow-all");
    }
}
