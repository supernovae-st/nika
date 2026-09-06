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
//!     url: "${{ inputs.site_url }}"
//!     traverse: { max_pages: 5 }
//! ```
//!
//! Semantics (all bounds are hard):
//!
//! - **Same-origin BFS from `url:`** — links come from each page's
//!   DISCOVERY list ([`nika_extract::page_digest_discovering`] · every
//!   link the page carries, never the digest's ≤30 preview — T9-F08),
//!   only the ROOT's origin is enqueued, fragments are stripped for
//!   dedup, and the frontier caps at `max_pages × 8` (a link farm cannot
//!   balloon the queue).
//! - **`max_pages` (1..=[`MAX_TRAVERSE_PAGES`])** bounds page REQUESTS —
//!   the effect certificate counts exactly `max_pages` page calls
//!   (+1 robots probe unless `respect_robots: false`), one definition
//!   with `nika-schema`'s certificate via `nika_types::net`.
//! - **`respect_robots` (default `true`)** — one GET of
//!   `{origin}/robots.txt`; `User-agent: *` `Disallow:` prefixes are
//!   honored. The probe's outcome follows RFC 9309 §2.3.1: a 4xx
//!   (*unavailable*) is allow-all; a 5xx or a transport failure
//!   (*unreachable*) is a COMPLETE DISALLOW — the crawl refuses loudly
//!   before a page is spent (it used to read every non-2xx as
//!   allow-all · T9-F10). A root that LANDS on another origin
//!   (apex→www) re-reads THAT origin's robots before any descendant is
//!   enqueued — the seed's rules never govern the landed host. A
//!   disallowed ROOT is a loud failure; disallowed descendants are
//!   silently not enqueued.
//! - **Per-page SSRF re-vetting** — every hop goes through the SAME
//!   injected guarded client as a single fetch (the L1 3-layer
//!   defense); this module never re-implements the guard.
//! - **Failures degrade per page** — the root failing is the builtin
//!   failing (nothing was crawled · loud beats empty, the single-fetch
//!   contract); a descendant failing becomes an honest
//!   `{url, status|error}` page entry and the crawl continues. The
//!   digest passes the same depth admission every HTML mode passes
//!   (T9-F07): a depth-bomb root refuses loudly, a depth-bomb descendant
//!   is recorded `{url, status, error}` — never a parser teardown.
//!
//! Output (the fixed crawl shape · stdlib §fetch):
//!
//! ```json
//! { "url": "<root>", "page_count": N,
//!   "pages": [ { "url", "status", ...page-digest } | { "url", "status"|"error" } ],
//!   "assets": { "images": [..≤40], "colors": [..≤30] } }
//! ```

use std::collections::VecDeque;

use nika_extract::page_digest_discovering;
use nika_kernel::io::http::{HttpGetDyn, HttpRequest};
use nika_types::net::MAX_TRAVERSE_PAGES;

use crate::{Args, BuiltinFailure, BuiltinOutcome};

const C: &str = "NIKA-BUILTIN-FETCH-001";

/// Aggregate-asset caps (across the whole crawl · first-seen order).
const MAX_ASSET_IMAGES: usize = 40;
const MAX_ASSET_COLORS: usize = 30;
/// robots.txt is bounded by the L1 64 MiB response cap — far too much to
/// PARSE. A real robots.txt is a few KiB; cap the parse input + the
/// disallow set so a hostile crawl target can't turn a 64 MiB body of
/// `Disallow:` lines into hundreds of MB of `String` + billions of
/// prefix compares on the frontier (the rust-pro review's amplification).
const MAX_ROBOTS_BYTES: usize = 512 * 1024;
const MAX_ROBOTS_DISALLOWS: usize = 4096;
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
    // The filter origin starts at the seed but is RE-PINNED to wherever
    // the root actually LANDS (a cross-origin redirect: apex→www ·
    // http→https · both ubiquitous). Fixed to the pre-redirect seed, it
    // filtered out every descendant link (resolved against the landed
    // origin) → the crawl silently truncated to one page (the rust-pro
    // review's redirect-truncation finding). The landed origin was itself
    // vetted by the L1 guard on the redirect hop.
    let mut origin = root_url.origin();

    let mut disallows = robots_for(http, &root_url, &spec, "the root").await?;

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

        let response = match fetch_page(http, &page_url, is_root).await? {
            Fetched::Ok(response) => response,
            Fetched::Recorded(entry) => {
                pages.push(entry);
                continue;
            }
        };

        let text = crate::net::decode_charset(
            &response.body,
            response.headers.get("content-type").map(String::as_str),
        )
        .into_owned();
        // The link-resolution base: where the page LANDED (post-redirect),
        // userinfo-stripped so a crawled asset never carries the crawl's
        // own basic-auth into the digest.
        let base = resolution_base(&page_url, &response.final_url);
        // Re-pin the same-origin filter to where the ROOT landed, so
        // apex→www / http→https canonicalization does not empty the
        // frontier (descendants still match the landed origin).
        if is_root
            && let Ok(landed) = url::Url::parse(&base)
            && landed.origin() != origin
        {
            origin = landed.origin();
            // The seed's robots never govern the landed host: re-read
            // the rules where the root LANDED before any descendant is
            // enqueued (RFC 9309 rules are per origin). The root's bytes
            // were already received (the redirect is the transport's);
            // a landed root its own robots forbids stops the crawl here.
            disallows = robots_for(http, &landed, &spec, "the landed root").await?;
        }
        let status = response.status;
        // CPU-heavy DOM parse rides the blocking pool (the single-fetch
        // extraction precedent — same bounded-orphan cancel contract).
        let digested =
            tokio::task::spawn_blocking(move || page_digest_discovering(&text, Some(&base)))
                .await
                .map_err(|e| BuiltinFailure::new(C, format!("extraction task failed: {e}")))?;
        let (digest, discovered) = match digested {
            Ok(pair) => pair,
            // The depth admission refused the body before any parser
            // saw it. The ROOT refusing is the single-fetch contract
            // failing (loud beats empty); a descendant becomes the
            // honest `{url, status, error}` entry and the crawl continues.
            Err(refusal) => {
                if is_root {
                    return Err(BuiltinFailure::new(
                        C,
                        format!("root page refused before parsing: {refusal}"),
                    ));
                }
                pages.push(serde_json::json!({
                    "url": crate::wire::redact_url(&page_url),
                    "status": status,
                    "error": format!("page refused before parsing: {refusal}"),
                }));
                continue;
            }
        };

        enqueue_links(
            &discovered,
            &origin,
            &disallows,
            &visited,
            &mut queue,
            &spec,
        );
        collect_assets(&digest, &mut asset_images, &mut asset_colors);
        pages.push(page_entry(&page_url, status, digest));
    }

    Ok(serde_json::json!({
        "url": crate::wire::redact_url(root),
        "page_count": pages.len(),
        "pages": pages,
        "assets": { "images": asset_images, "colors": asset_colors },
    }))
}

/// The robots gate for one origin: fetch + parse the disallow prefixes
/// (empty when `respect_robots: false`), refuse an UNREACHABLE robots
/// (RFC 9309 §2.3.1.4 · complete disallow), and refuse a disallowed
/// root loudly — nothing was crawled, loud beats empty. `which` names
/// the root in the message (the seed · the landed root).
async fn robots_for<H: HttpGetDyn>(
    http: &H,
    root_url: &url::Url,
    spec: &TraverseSpec,
    which: &str,
) -> Result<Vec<String>, BuiltinFailure> {
    let disallows = if spec.respect_robots {
        fetch_robots_disallows(http, root_url).await?
    } else {
        Vec::new()
    };
    if robots_blocks(&disallows, root_url.path()) {
        return Err(BuiltinFailure::new(
            C,
            format!(
                "robots.txt disallows {which} `{}` — nothing to crawl \
                 (set `respect_robots: false` only if you own the site)",
                crate::wire::redact_url(root_url.as_str())
            ),
        ));
    }
    Ok(disallows)
}

/// One page's fetch outcome: a usable 2xx response, or an entry already
/// shaped for the `pages` list (a descendant's error/non-2xx — recorded,
/// not fatal). The ROOT failing propagates as the builtin failing.
enum Fetched {
    Ok(nika_kernel::http::HttpResponse),
    Recorded(serde_json::Value),
}

/// GET one crawl page. A descendant error/non-2xx is recorded (honest:
/// the request was made, or BLOCKED and nothing left the machine); the
/// root failing IS the single-fetch contract failing. Userinfo redacted
/// in every echo.
async fn fetch_page<H: HttpGetDyn>(
    http: &H,
    page_url: &str,
    is_root: bool,
) -> Result<Fetched, BuiltinFailure> {
    let response = match http.get(HttpRequest::get(page_url)).await {
        Ok(response) => response,
        Err(e) => {
            if is_root {
                return Err(crate::net::net_security_failure(&e)
                    .unwrap_or_else(|| BuiltinFailure::new(C, format!("request failed: {e}"))));
            }
            return Ok(Fetched::Recorded(serde_json::json!({
                "url": crate::wire::redact_url(page_url), "error": e.to_string()
            })));
        }
    };
    if !(200..300).contains(&response.status) {
        if is_root {
            return Err(BuiltinFailure::new(
                C,
                format!(
                    "HTTP {} from {}",
                    response.status,
                    crate::wire::redact_url(page_url)
                ),
            )
            .with_transient(crate::net::is_transient_status(response.status))
            .with_details(serde_json::json!({ "status_code": response.status })));
        }
        return Ok(Fetched::Recorded(serde_json::json!({
            "url": crate::wire::redact_url(page_url), "status": response.status
        })));
    }
    Ok(Fetched::Ok(response))
}

/// The base a page's links/images resolve against: the post-redirect
/// landing URL (else the request URL), with userinfo stripped — a
/// crawled asset URL must never echo the crawl's own basic-auth.
fn resolution_base(page_url: &str, final_url: &str) -> String {
    let landed = if final_url.is_empty() {
        page_url
    } else {
        final_url
    };
    crate::wire::redact_url(landed)
}

/// Vet the whole-arg surface + parse the spec: `traverse:` excludes the
/// single-fetch extraction/payload families, forces GET, and its own
/// shape is closed. All of it fails BEFORE any request is spent.
fn parse_traverse_args(args: &Args) -> Result<TraverseSpec, BuiltinFailure> {
    for key in [
        "mode",
        "selector",
        "jq",
        "body",
        "form",
        "multipart",
        "headers",
    ] {
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
    discovered: &[String],
    origin: &url::Origin,
    disallows: &[String],
    visited: &[String],
    queue: &mut VecDeque<String>,
    spec: &TraverseSpec,
) {
    // max_pages ≤ 25 and the multiplier is 8 — the product fits every
    // pointer width (belt: saturate, never truncate).
    let frontier_cap = usize::try_from(spec.max_pages * FRONTIER_PER_PAGE).unwrap_or(usize::MAX);
    for link in discovered.iter().map(String::as_str) {
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
    // Userinfo never persists into the crawl output (journal · logs ·
    // agent tool-result) — the single-fetch path already redacts; the
    // crawl output must match (the review's credential-leak finding).
    entry.insert("url".to_owned(), crate::wire::redact_url(page_url).into());
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

/// GET `{origin}/robots.txt` and read its outcome the way RFC 9309
/// §2.3.1 does: 2xx → the rules · 4xx (*unavailable*) → allow-all · 5xx
/// or a transport failure (*unreachable*) → complete disallow. It used
/// to read EVERY non-2xx and every error as allow-all, so an origin
/// whose robots was down for a minute was crawled as if it had none
/// (T9-F10). Robots never replaces the permits boundary — it only ever
/// narrows what an admitted crawl touches.
///
/// # Errors
///
/// The unreachable refusal (transient when the transport may recover).
async fn fetch_robots_disallows<H: HttpGetDyn>(
    http: &H,
    root: &url::Url,
) -> Result<Vec<String>, BuiltinFailure> {
    let Ok(robots_url) = root.join("/robots.txt") else {
        return Ok(Vec::new());
    };
    let origin = root.origin().ascii_serialization();
    match http.get(HttpRequest::get(robots_url.as_str())).await {
        Ok(response) if (200..300).contains(&response.status) => {
            // Parse only the first MAX_ROBOTS_BYTES — a real robots.txt is
            // KiB; anything past that is amplification fuel, not policy.
            let cap = response.body.len().min(MAX_ROBOTS_BYTES);
            Ok(robots_disallows(&String::from_utf8_lossy(
                &response.body[..cap],
            )))
        }
        // §2.3.1.3 · unavailable: the crawler MAY access any resource.
        Ok(response) if (400..500).contains(&response.status) => Ok(Vec::new()),
        // §2.3.1.4 · unreachable: the crawler MUST assume complete disallow.
        Ok(response) => Err(robots_unreachable(
            &origin,
            &format!("HTTP {}", response.status),
            crate::net::is_transient_status(response.status),
        )),
        Err(error) => {
            let transient = matches!(
                error,
                nika_kernel::io::http::HttpError::Timeout { .. }
                    | nika_kernel::io::http::HttpError::Connection { .. }
            );
            Err(robots_unreachable(&origin, &error.to_string(), transient))
        }
    }
}

/// The complete-disallow refusal — teaching, never silent.
fn robots_unreachable(origin: &str, why: &str, transient: bool) -> BuiltinFailure {
    BuiltinFailure::new(
        C,
        format!(
            "robots.txt for `{origin}` is unreachable ({why}) — RFC 9309 §2.3.1.4 makes \
             that a complete disallow, so nothing is crawled; retry later, or set \
             `respect_robots: false` only if you own the site"
        ),
    )
    .with_transient(transient)
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
            if field.eq_ignore_ascii_case("disallow")
                && group_is_star
                && !value.is_empty()
                && disallows.len() < MAX_ROBOTS_DISALLOWS
            {
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
    async fn traverse_follows_a_cross_origin_root_redirect() {
        // apex → www canonicalization (ubiquitous): the root LANDS on
        // www, its links resolve against www, and the same-origin filter
        // must re-pin to www or the frontier empties (a 1-page crawl).
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new()) // robots
            .enqueue_ok_final_url(200, page("home", &["/a"]), "https://www.acme.test/")
            .enqueue_ok(200, page("a", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await
        .expect("crawl succeeds");
        assert_eq!(
            out["page_count"], 2,
            "the www descendant is followed: {out}"
        );
    }

    #[tokio::test]
    async fn traverse_output_redacts_url_userinfo() {
        // The crawl output must not persist basic-auth into the journal —
        // the single-fetch path redacts; the crawl must match.
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(200, page("root", &[]));
        let out = traverse(
            &http,
            "https://user:s3cret@acme.test/",
            &args(serde_json::json!({
                "url": "https://user:s3cret@acme.test/", "traverse": { "max_pages": 3 }
            })),
        )
        .await
        .expect("crawl succeeds");
        let dump = out.to_string();
        assert!(
            !dump.contains("s3cret"),
            "no userinfo in the crawl output: {dump}"
        );
        assert!(dump.contains("acme.test"), "the host still shows: {dump}");
    }

    #[tokio::test]
    async fn traverse_excludes_headers_loudly() {
        // headers were silently dropped on the crawl (every hop went
        // unauthenticated) — now a loud exclusion, never a silent drop.
        let http = MockHttp::new().enqueue_ok(404, Vec::new());
        let err = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/",
                "headers": { "Authorization": "Bearer x" },
                "traverse": { "max_pages": 3 }
            })),
        )
        .await
        .expect_err("headers + traverse is refused");
        assert!(err.message.contains("headers"), "{}", err.message);
    }

    #[test]
    fn robots_disallows_are_capped() {
        // A hostile robots.txt of millions of Disallow lines can't turn
        // into an unbounded Vec (amplification cap).
        use std::fmt::Write as _;
        let mut body = String::from("User-agent: *\n");
        for i in 0..(MAX_ROBOTS_DISALLOWS + 500) {
            let _ = writeln!(body, "Disallow: /p{i}");
        }
        assert_eq!(robots_disallows(&body).len(), MAX_ROBOTS_DISALLOWS);
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

    /// T9-F10 · RFC 9309 §2.3.1.4: a robots.txt the server cannot serve
    /// (5xx) is UNREACHABLE, and unreachable is a complete disallow — the
    /// crawl refuses before a page is spent, transient (the server may
    /// recover), with the rule named. It used to read 503 as allow-all.
    #[tokio::test]
    async fn robots_5xx_is_a_complete_disallow() {
        let http = MockHttp::new().enqueue_ok(503, Vec::new());
        let fail = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == C && f.transient && f.message.contains("9309")),
            "{fail:?}"
        );
        assert_eq!(http.sent_requests().len(), 1, "no page was spent");
    }

    /// T9-F10 · a transport failure on the probe is unreachable too.
    #[tokio::test]
    async fn robots_transport_error_is_a_complete_disallow() {
        let http = MockHttp::new().enqueue_err(nika_kernel::io::http::HttpError::Connection {
            reason: "refused".to_owned(),
        });
        let fail = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == C && f.transient && f.message.contains("unreachable")),
            "{fail:?}"
        );
        assert_eq!(http.sent_requests().len(), 1);
    }

    /// RFC 9309 §2.3.1.3: an UNAVAILABLE robots (any 4xx, not only 404)
    /// lets the crawler access any resource.
    #[tokio::test]
    async fn robots_4xx_is_allow_all() {
        let http = MockHttp::new()
            .enqueue_ok(403, Vec::new())
            .enqueue_ok(200, page("root", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 2 }
            })),
        )
        .await
        .expect("403 robots is allow-all");
        assert_eq!(out["page_count"], 1);
    }

    /// T9-F10 · rules are per ORIGIN: a root that lands on www must obey
    /// www's robots, not the apex's. The seed's probe said allow-all; the
    /// landed host forbids /private — and /private is never requested.
    #[tokio::test]
    async fn a_cross_origin_root_redirect_re_reads_robots_where_it_landed() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new()) // apex robots · unavailable
            .enqueue_ok_final_url(
                200,
                page("home", &["/private/x", "/ok"]),
                "https://www.acme.test/",
            )
            .enqueue_ok(200, b"User-agent: *\nDisallow: /private\n".to_vec()) // www robots
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
        let sent: Vec<String> = http.sent_requests().into_iter().map(|r| r.url).collect();
        assert_eq!(
            sent,
            vec![
                "https://acme.test/robots.txt",
                "https://acme.test/",
                "https://www.acme.test/robots.txt",
                "https://www.acme.test/ok",
            ],
            "the landed origin's robots is read before any descendant"
        );
        assert_eq!(out["page_count"], 2, "{out}");
    }

    /// T9-F10 · the landed host's own robots forbidding the landed root
    /// stops the crawl loudly (the root's bytes were the transport's
    /// redirect; nothing further is spent).
    #[tokio::test]
    async fn a_landed_root_its_own_robots_forbids_is_loud() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok_final_url(200, page("home", &["/a"]), "https://www.acme.test/")
            .enqueue_ok(200, b"User-agent: *\nDisallow: /\n".to_vec());
        let fail = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == C && f.message.contains("landed root")),
            "{fail:?}"
        );
        assert_eq!(http.sent_requests().len(), 3, "no descendant was spent");
    }

    fn depth_bomb() -> Vec<u8> {
        // Past the extract crate's nesting cap (2048) — refused by the
        // byte-scan admission before any parser runs.
        "<div>".repeat(2100).into_bytes()
    }

    /// T9-F07 · a depth-bomb DESCENDANT is recorded `{url, status,
    /// error}` and the crawl continues — the same admission every HTML
    /// mode passes now guards the one path a crawl takes.
    #[tokio::test]
    async fn a_depth_bomb_descendant_is_recorded_not_fatal() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(200, page("root", &["/bomb", "/ok"]))
            .enqueue_ok(200, depth_bomb())
            .enqueue_ok(200, page("ok", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await
        .expect("crawl survives a depth bomb");
        let pages = out["pages"].as_array().expect("pages");
        assert_eq!(pages.len(), 3, "{out}");
        assert_eq!(pages[1]["status"], 200);
        assert!(
            pages[1]["error"]
                .as_str()
                .is_some_and(|e| e.contains("refused before parsing")),
            "the refusal is named: {}",
            pages[1]
        );
        assert!(pages[1].get("title").is_none(), "no digest on a refusal");
        assert_eq!(pages[2]["title"], "ok", "crawl continued");
    }

    /// T9-F07 · a depth-bomb ROOT is the single-fetch contract failing.
    #[tokio::test]
    async fn a_depth_bomb_root_fails_loudly() {
        let http = MockHttp::new()
            .enqueue_ok(404, Vec::new())
            .enqueue_ok(200, depth_bomb());
        let fail = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 3 }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == C && f.message.contains("refused before parsing")),
            "{fail:?}"
        );
    }

    /// T9-F08 · discovery is not the preview: the 31st link on a page is
    /// crawled even though the digest shows only 30. Thirty decoys the
    /// robots forbid come first; the one allowed link is the 31st.
    #[tokio::test]
    async fn the_thirty_first_link_is_discovered() {
        let decoys: Vec<String> = (0..30).map(|i| format!("/a{i:02}")).collect();
        let mut links: Vec<&str> = decoys.iter().map(String::as_str).collect();
        links.push("/z");
        let http = MockHttp::new()
            .enqueue_ok(200, b"User-agent: *\nDisallow: /a\n".to_vec())
            .enqueue_ok(200, page("root", &links))
            .enqueue_ok(200, page("z", &[]));
        let out = traverse(
            &http,
            "https://acme.test/",
            &args(serde_json::json!({
                "url": "https://acme.test/", "traverse": { "max_pages": 5 }
            })),
        )
        .await
        .expect("crawl succeeds");
        assert_eq!(out["page_count"], 2, "the 31st link was crawled: {out}");
        assert_eq!(out["pages"][1]["title"], "z");
        assert_eq!(
            out["pages"][0]["links"].as_array().expect("preview").len(),
            30,
            "the digest keeps its spec preview cap"
        );
        assert_eq!(http.sent_requests().len(), 3, "robots + root + /z");
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
