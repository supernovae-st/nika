// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Network builtins (2) — fetch · notify (stdlib §Network).
//!
//! Both compose the injected kernel http seam — SSRF defense lives in the
//! L1 http effect (3-layer · s5), this layer never re-implements it.
//!
//! `nika:fetch` is web-CONTENT acquisition: the kernel http GET/POST,
//! then the `mode:` extraction (`nika-extract`) — `mode: jq`
//! composes THIS crate's one jq engine (`data::jq`), never a second one.

mod response;

use bytes::Bytes;
use nika_cap::JqClock;
use nika_extract::{ExtractMode, ExtractOptions};
use nika_kernel::io::fs::{FsMetaDyn, FsReadDyn};
use nika_kernel::io::http::{HttpError, HttpGetDyn, HttpMethod, HttpPostDyn, HttpRequest};

use crate::permits::FsBoundary;
use crate::{Args, BuiltinFailure, BuiltinOutcome, opt_str, req_str};

/// Map the two NET SECURITY-BOUNDARY errors to their spec-plane codes, shared
/// by `fetch` + `notify` (one definition · no drift between the surfaces):
/// a declared `permits.net.http` escape → `NIKA-SEC-004`, and the always-on
/// SSRF floor (loopback/private/link-local/metadata) → `NIKA-SEC-005`. Both
/// are `security_error` · non-transient · never fed back to an `agent:` model
/// (a boundary is not negotiation material). `None` for transport-plane
/// errors — the caller maps those per its own retry contract.
pub(crate) fn net_security_failure(e: &HttpError) -> Option<BuiltinFailure> {
    match e {
        HttpError::HostNotAllowed { host } => Some(BuiltinFailure::new(
            crate::permits::SEC_DENIED,
            format!("`{host}` resolves outside the declared net.http boundary"),
        )),
        HttpError::SsrfBlocked { url } => Some(BuiltinFailure::new(
            crate::permits::SEC_SSRF,
            format!("SSRF blocked · `{url}` resolves to a loopback/private/metadata target"),
        )),
        _ => None,
    }
}

/// [`net_security_failure`] knowing the URL written in the file — the one
/// `nika check` audited. When the refused host is a DIFFERENT one, the
/// boundary was crossed by a redirect hop the static audit cannot see
/// (#1582 · check green ≠ run): the message then names the audited host
/// and the exact grant, so the run explains the green check instead of
/// contradicting it. `fetch` + `notify` (the two file-named URLs) map
/// through this door; the traverse family keeps the plain mapper.
pub(crate) fn net_security_failure_for(e: &HttpError, requested: &str) -> Option<BuiltinFailure> {
    let mut failure = net_security_failure(e)?;
    if let HttpError::HostNotAllowed { host } = e
        && let Some(audited) = redirect_origin(requested, host)
    {
        failure.message = format!(
            "{refusal} · reached by a redirect from `{audited}` — `nika check` audits the \
             URL written in the file, a hop is judged here at run · to admit it add \
             `{host}` to permits.net.http",
            refusal = failure.message
        );
    }
    Some(failure)
}

/// The connect host of a parsed URL, normalized the way `permits.net.http`
/// is written — the SAME extraction as the transport (`nika-http`'s
/// `host_of`: bracket-free IPv6 · FQDN trailing dot stripped) and the static
/// checker, pinned by [`nika_types::net::HOST_EXTRACTION_VECTORS`] in this
/// crate's tests too, so the three readers of one host can never drift.
fn url_host(url: &url::Url) -> Option<String> {
    match url.host()? {
        url::Host::Domain(d) => Some(d.trim_end_matches('.').to_owned()),
        url::Host::Ipv4(a) => Some(a.to_string()),
        url::Host::Ipv6(a) => Some(a.to_string()),
    }
}

/// The host `nika check` audited (the one written in the file) when the
/// refused host is a DIFFERENT one — a declared boundary refuses a host the
/// file never named only through a redirect hop (#1582). Same extractor as
/// the transport's per-hop vet, so a spelling difference (case · FQDN dot ·
/// IPv6 brackets · `\@` userinfo) is never mistaken for a hop.
fn redirect_origin(requested: &str, refused: &str) -> Option<String> {
    let audited = url_host(&url::Url::parse(requested).ok()?)?;
    (audited != refused).then_some(audited)
}

/// The #1371 effect-safe retry verdict for one prepared request (spec 05
/// §the effect-safe retry law): a keyless effect-capable call (POST ·
/// PUT · DELETE · PATCH without an `idempotency-key` header) types EVERY
/// failure non-transient — the failure may be ambiguous (the server may
/// have committed the effect before the socket dropped or the 500 was
/// emitted) and a blind replay doubles it. ONE predicate, shared with
/// the static `NIKA-SEC-016` refusal (`nika_types::net`), judged over
/// the headers that ACTUALLY ride the wire — check ≡ run.
fn effect_safe_retry(method: &str, request: &HttpRequest) -> bool {
    nika_types::net::retry_is_effect_safe(method, request.headers.keys().map(String::as_str))
}

/// `nika:fetch` — HTTP request + content extraction (stdlib §fetch).
/// Without `response.accept`, non-2xx is failure. An explicit exact set
/// returns accepted responses as status/body observations and fails every
/// unlisted status. Failures have `transient: true` for 5xx/408/429, `false` for
/// other 4xx — normative) — with the #1371 effect-safe carve-out: a
/// keyless effect-capable method (POST · PUT · DELETE · PATCH without an
/// `idempotency-key` header) is `transient: false` on EVERY failure
/// (the failure may be ambiguous — the server may have committed — and
/// a blind retry replays the effect; spec 05 §the effect-safe retry
/// law). GET/HEAD and keyed calls keep the status table. The body is
/// decoded then run through the
/// `mode:` extraction (default `markdown` · extract-modes-v0.1.md).
///
/// CANCEL SAFETY: dropping this future detaches — it does NOT stop an
/// in-flight extraction. A dropped `spawn_blocking` keeps running to
/// completion on its pool thread, so the L3 timeout path leaves a
/// bounded orphan, never unbounded work. The bound is two-sided:
/// MEMORY by the L1 http 64 MiB body cap, and TIME by the extractor's
/// own guarantees — the depth guard rejects pathological nesting in
/// O(cap) BEFORE any parse (so a hostile body can't spin or, via
/// htmd's recursive rcdom `Drop`, abort the whole process), and every
/// mode is otherwise linear in the (capped) body. A panic inside the
/// closure unwinds (workspace `panic = "unwind"`) to a `JoinError`
/// handled at the call site — it is never a process-wide abort.
pub(crate) async fn fetch_with_clock<H: HttpGetDyn + HttpPostDyn, F: FsReadDyn + FsMetaDyn>(
    http: &H,
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
    jq_clock: JqClock,
) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let url = req_str(args, "url", C)?;
    // The bounded-crawl family owns its arg surface and output shape.
    if args.contains_key("traverse") {
        return crate::net_traverse::traverse(http, url, args).await;
    }
    let response_policy = response::Policy::parse(args, C)?;
    let method = opt_str(args, "method", C)?.unwrap_or("GET").to_uppercase();
    let (mode, selector) = extraction_mode(args, C)?;

    // ONE method parse — routing + the wire request both derive from
    // the enum (no string re-match to desync).
    let http_method = parse_method(&method).map_err(|m| BuiltinFailure::new(C, m))?;
    let request = crate::net_payload::prepare_request(http_method, url, fs, boundary, args).await?;
    let retry_safe = effect_safe_retry(&method, &request);
    // MUTATION (equivalent under the mock): GET/HEAD route to .get(), all
    // else to .post() — but a test double serves both identically and the
    // recorded request carries its own method, so deleting this arm is
    // behaviorally invisible in tests. Real transports differ (GET has no
    // body); the per-method `request.method` mapping IS pinned below.
    let response = match http_method {
        HttpMethod::Get | HttpMethod::Head => http.get(request).await,
        _ => http.post(request).await,
    }
    .map_err(|e| {
        // A security-boundary error (permits.net.http → SEC-004 · SSRF floor
        // → SEC-005) takes its spec-plane code; otherwise it's a transport
        // failure whose retryability follows the spec status table.
        net_security_failure_for(&e, url).unwrap_or_else(|| {
            // #1371: a keyless effect-capable call never earns the
            // transport-transient classification (the ambiguous commit).
            let transient =
                retry_safe && matches!(e, HttpError::Timeout { .. } | HttpError::Connection { .. });
            BuiltinFailure::new(C, format!("request failed: {e}")).with_transient(transient)
        })
    })?;

    if !response_policy.admits(response.status) {
        // `details.status_code` carries the status (stdlib §fetch ·
        // normative) — branching on 403 vs 429 must never mean parsing
        // the human message.
        return Err(response_policy.annotate(
            BuiltinFailure::new(
                C,
                format!(
                    "HTTP {} from {}",
                    response.status,
                    crate::wire::redact_url(url)
                ),
            )
            .with_transient(retry_safe && is_transient_status(response.status))
            .with_details(serde_json::json!({ "status_code": response.status })),
            response.status,
        ));
    }

    // Gather the extraction inputs (owned) BEFORE handing off, so the
    // CPU-heavy parse runs on the blocking pool — a 64 MiB HTML parse or
    // a heavy jq must not starve the async executor (the data::jq /
    // nika-ocr precedent). `Bytes` is Arc-backed: the clone is cheap.
    let plan = ExtractPlan {
        mode,
        body: response.body.clone(),
        content_type: response.headers.get("content-type").cloned(),
        link_header: response.headers.get("link").cloned(),
        base_url: if response.final_url.is_empty() {
            url.to_owned()
        } else {
            response.final_url.clone()
        },
        selector,
        jq: if mode == ExtractMode::Jq {
            Some(req_str(args, "jq", C)?.to_owned())
        } else {
            None
        },
        jq_clock,
    };
    let body = tokio::task::spawn_blocking(move || plan.run(C))
        .await
        .map_err(|e| BuiltinFailure::new(C, format!("extraction task failed: {e}")))
        .and_then(|result| result)
        .map_err(|failure| response_policy.annotate(failure, response.status))?;
    Ok(response_policy.finish(&response, body))
}

/// Validate extraction pairings before any request, including resolved dynamic modes.
fn extraction_mode(
    args: &Args,
    code: &'static str,
) -> Result<(ExtractMode, Option<String>), BuiltinFailure> {
    // Vet extraction pairings before spending a request. This runtime
    // defense covers hand-built ToolCalls and templated modes.
    let mode = parse_mode(args, code)?;
    let selector = opt_str(args, "selector", code)?.map(str::to_owned);
    if selector.is_some() && mode != ExtractMode::Selector {
        return Err(BuiltinFailure::new(
            code,
            "`selector:` pairs with `mode: selector` only (extract-modes-v0.1.md §selector)",
        ));
    }
    if args.contains_key("jq") && mode != ExtractMode::Jq {
        return Err(BuiltinFailure::new(
            code,
            "`jq:` is «a jq expression · only with mode: jq» (builtins-v0.1.md §nika:fetch)",
        ));
    }

    Ok((mode, selector))
}

/// Resolve the `mode:` argument against the closed extract-mode set
/// (default `markdown` · extract-modes-v0.1.md). A templated value is
/// already CEL-resolved by the time the builtin runs — a non-canon
/// string here is a genuine error.
fn parse_mode(args: &Args, code: &'static str) -> Result<ExtractMode, BuiltinFailure> {
    match opt_str(args, "mode", code)? {
        None => Ok(ExtractMode::Markdown),
        Some(raw) => raw
            .parse::<ExtractMode>()
            .map_err(|e| BuiltinFailure::new(code, e.to_string())),
    }
}

/// The owned extraction inputs — runs on the blocking pool.
struct ExtractPlan {
    mode: ExtractMode,
    body: Bytes,
    content_type: Option<String>,
    /// The response `Link:` header (RFC 8288) — `mode: metadata` mines
    /// it for hreflang alternates.
    link_header: Option<String>,
    base_url: String,
    selector: Option<String>,
    jq: Option<String>,
    jq_clock: JqClock,
}

impl ExtractPlan {
    /// Decode per `mode` and run the extraction. `raw`/`jq` demand strict
    /// UTF-8 (raw is the spec's UTF-8 contract; jq input is JSON = UTF-8
    /// by RFC 8259); the HTML/feed/sitemap modes decode charset-aware
    /// from `Content-Type` (the web is not all UTF-8).
    fn run(self, code: &'static str) -> BuiltinOutcome {
        match self.mode {
            ExtractMode::Raw => Ok(serde_json::Value::String(decode_utf8_strict(
                &self.body, code,
            )?)),
            ExtractMode::Jq => {
                // The caller sets `jq` iff mode == Jq — reaching this arm
                // without it is an internal invariant break, not an empty
                // expression (a silent wrong answer · review lens 1 P1).
                let expression = self.jq.ok_or_else(|| {
                    BuiltinFailure::new(code, "internal: mode jq reached without a jq expression")
                })?;
                let text = decode_utf8_strict(&self.body, code)?;
                let input: serde_json::Value = serde_json::from_str(&text).map_err(|e| {
                    BuiltinFailure::new(code, format!("response is not JSON (mode: jq): {e}"))
                })?;
                // Compose THE jq engine (data::jq · one data language · the
                // exactly-one-output law + ceiling live there, not here).
                let mut jq_args = serde_json::Map::new();
                jq_args.insert(
                    "expression".to_owned(),
                    serde_json::Value::String(expression),
                );
                jq_args.insert("input".to_owned(), input);
                crate::data::jq_with_clock(&jq_args, self.jq_clock)
            }
            // feed gets RAW BYTES: feed-rs owns charset detection (XML
            // prolog + BOM) — pre-transcoding would leave a stale prolog
            // and mojibake non-ASCII (review lens 2 · P3-7).
            ExtractMode::Feed => nika_extract::feed_from_bytes(&self.body)
                .map_err(|e| BuiltinFailure::new(code, e.to_string())),
            other => {
                let mut opts = ExtractOptions::new();
                opts.base_url = Some(&self.base_url);
                opts.selector = self.selector.as_deref();
                // Cow: a UTF-8 body (the web's majority) borrows — no
                // 64 MiB copy before the DOM build (review lens 3 P3).
                let text = decode_charset(&self.body, self.content_type.as_deref());
                // Extraction is deterministic (parse failures don't get
                // better on retry).
                let mut value = nika_extract::extract(&text, other, &opts)
                    .map_err(|e| BuiltinFailure::new(code, e.to_string()))?;
                // metadata gains the RFC 8288 hreflang alternates when
                // the response carried a Link header (additive key).
                if other == ExtractMode::Metadata
                    && let Some(header) = self.link_header.as_deref()
                    && let Some(object) = value.as_object_mut()
                {
                    let entries = nika_extract::link_header::parse_link_header(header);
                    let alternates = nika_extract::link_header::alternates(&entries);
                    if !alternates.is_empty() {
                        object.insert(
                            "alternates".to_owned(),
                            serde_json::Value::Array(alternates),
                        );
                    }
                }
                Ok(value)
            }
        }
    }
}

#[cfg(test)]
pub(crate) async fn fetch<H: HttpGetDyn + HttpPostDyn, F: FsReadDyn + FsMetaDyn>(
    http: &H,
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
) -> BuiltinOutcome {
    fetch_with_clock(
        http,
        fs,
        boundary,
        args,
        JqClock::at(nika_types::timestamp::Timestamp::EPOCH),
    )
    .await
}

/// Strict UTF-8 decode (`raw`/`jq`): a non-UTF-8 body is
/// `NIKA-BUILTIN-FETCH-001` (stdlib §fetch raw contract · binary is
/// file-mediated, not fetch's job).
fn decode_utf8_strict(body: &[u8], code: &'static str) -> Result<String, BuiltinFailure> {
    std::str::from_utf8(body).map(str::to_owned).map_err(|e| {
        BuiltinFailure::new(
            code,
            format!(
                "response body is not valid UTF-8 ({e}) — `mode: raw`/`jq` need text; \
                 binary payloads are not a fetch concern"
            ),
        )
    })
}

/// Charset-aware decode for the extraction modes, in the WHATWG
/// encoding-sniffing precedence (HTML §13.2 · the order browsers use):
///
/// 1. **BOM** — a leading UTF-8/UTF-16 byte-order mark is MORE
///    authoritative than any header (WHATWG: "the BOM … is more
///    authoritative than anything else"). Without this a UTF-16 page
///    decodes as UTF-8-lossy → mojibake (the security-audit P3).
/// 2. **`Content-Type` charset** — the transport label.
/// 3. **`<meta charset>` prescan** of the first 1024 bytes — legacy
///    pages that declare their charset only in HTML (windows-1251 /
///    `Shift_JIS` / GBK …). Closes the prior "header-less → UTF-8" gap.
/// 4. **UTF-8** default.
///
/// Lossy by design — extraction is best-effort cleanup, a stray byte
/// must not sink the page (the strict path is `raw`/`jq` above). `Cow`:
/// clean UTF-8 (the web's majority) BORROWS — no copy.
pub(crate) fn decode_charset<'a>(
    body: &'a [u8],
    content_type: Option<&str>,
) -> std::borrow::Cow<'a, str> {
    let encoding = encoding_rs::Encoding::for_bom(body)
        .map(|(enc, _bom_len)| enc)
        .or_else(|| {
            content_type
                .and_then(charset_label)
                .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        })
        .or_else(|| meta_charset(body))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(body).0
}

/// Number of leading bytes scanned for a `<meta>` charset declaration —
/// the WHATWG prescan window (HTML §13.2 "prescan a byte stream").
const META_PRESCAN_LEN: usize = 1024;

/// Prescan the first [`META_PRESCAN_LEN`] bytes for an HTML-declared
/// charset: `<meta charset=…>` (HTML5) or `<meta http-equiv=…
/// content="…; charset=…">` (legacy). Returns the matched encoding, or
/// `None`. Byte-level + case-insensitive — runs before any decode, so
/// it must not assume the bytes are already UTF-8 (ASCII-subset match).
///
/// Anchored to `<meta` tags (per the WHATWG prescan): a `charset=`
/// substring living in a `<script>` string or a comment must NOT be
/// honored — only the charset declared inside an actual `<meta>` tag.
fn meta_charset(body: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    let window = &body[..body.len().min(META_PRESCAN_LEN)];
    // Lowercased ASCII view (non-ASCII bytes map to themselves — we only
    // match ASCII tokens, so this is sound on un-decoded bytes).
    let lower: Vec<u8> = window.iter().map(u8::to_ascii_lowercase).collect();
    let mut i = 0;
    while i < lower.len() {
        // Step OVER comments wholesale (the WHATWG prescan does too) — a
        // `<meta charset=…>` living inside `<!-- … -->` is NOT a real
        // declaration and must not set the encoding.
        if lower[i..].starts_with(b"<!--") {
            i = match find_subslice(&lower[i + 4..], b"-->") {
                Some(rel) => i + 4 + rel + 3,
                None => lower.len(),
            };
            continue;
        }
        // A real `<meta>` tag: search ONLY its own bytes (to the closing
        // `>`) for a `charset` label.
        if lower[i..].starts_with(b"<meta") {
            let tag_start = i + b"<meta".len();
            let tag_end = lower[tag_start..]
                .iter()
                .position(|&b| b == b'>')
                .map_or(lower.len(), |p| tag_start + p);
            if let Some(enc) = charset_in_tag(&lower[tag_start..tag_end]) {
                return Some(enc);
            }
            i = tag_end;
            continue;
        }
        i += 1;
    }
    None
}

/// Extract a `charset` label from one `<meta …>` tag's bytes (already
/// lowercased, sans the `<meta`/`>` delimiters).
fn charset_in_tag(tag: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    let rel = find_subslice(tag, b"charset")?;
    let after = rel + b"charset".len();
    // Skip `=` / whitespace / quotes between `charset` and the label.
    let mut k = after;
    while k < tag.len() && matches!(tag[k], b'=' | b' ' | b'\t' | b'"' | b'\'') {
        k += 1;
    }
    let start = k;
    while k < tag.len()
        && !matches!(
            tag[k],
            b'"' | b'\'' | b' ' | b'\t' | b';' | b'/' | b'\r' | b'\n'
        )
    {
        k += 1;
    }
    (start < k)
        .then(|| encoding_rs::Encoding::for_label(&tag[start..k]))
        .flatten()
}

/// First index of `needle` in `haystack` (tiny — no memchr dep needed
/// for a ≤1024-byte window scanned once).
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

/// Pull the `charset=` parameter out of a `Content-Type` (case-
/// insensitive key · quotes trimmed · quote-AWARE splitting: a `;`
/// inside a quoted param value must not cut the scan —
/// `title="a;charset=koi8-r"; charset=utf-8` is utf-8). `None` when
/// absent → [`decode_charset`] falls back to the `<meta>` prescan.
fn charset_label(content_type: &str) -> Option<&str> {
    split_params_quote_aware(content_type)
        .into_iter()
        .skip(1)
        .find_map(|param| {
            let (key, value) = param.split_once('=')?;
            key.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| value.trim().trim_matches('"').trim_matches('\''))
        })
}

/// Split a header value on `;` OUTSIDE double quotes (RFC 9110
/// parameter syntax — quoted-string values may carry `;`).
fn split_params_quote_aware(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut in_quotes = false;
    let mut start = 0usize;
    for (i, ch) in value.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => {
                parts.push(&value[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&value[start..]);
    parts
}

/// One method parse for the whole builtin (review lens 2 · P3-8 — the
/// string was matched three times with a silent `_ => Get` fallback a
/// future edit could desync into a GET downgrade).
fn parse_method(method: &str) -> Result<HttpMethod, String> {
    match method {
        "GET" => Ok(HttpMethod::Get),
        "HEAD" => Ok(HttpMethod::Head),
        "POST" => Ok(HttpMethod::Post),
        "PUT" => Ok(HttpMethod::Put),
        "DELETE" => Ok(HttpMethod::Delete),
        "PATCH" => Ok(HttpMethod::Patch),
        other => Err(format!("unsupported method `{other}`")),
    }
}

pub(super) fn build_request(
    method: HttpMethod,
    url: &str,
    args: &Args,
) -> Result<HttpRequest, String> {
    let mut request = HttpRequest::get(url);
    request.method = method;
    if let Some(headers) = args.get("headers").and_then(serde_json::Value::as_object) {
        for (key, value) in headers {
            // A non-string header value is LOUD (mirrors opt_str's
            // strictness three lines up the file — silent drops are the
            // anti-pattern this builtin exists to avoid).
            let Some(text) = value.as_str() else {
                return Err(format!("header `{key}:` must be a string"));
            };
            request.headers.insert(key.clone(), text.to_owned());
        }
    }
    if let Some(body) = args.get("body") {
        let bytes = match body {
            serde_json::Value::String(s) => s.clone().into_bytes(),
            other => serde_json::to_vec(other).map_err(|e| e.to_string())?,
        };
        request.body = Some(bytes.into());
    }
    Ok(request)
}

/// The spec's status→retryability table (stdlib §fetch · normative):
/// 5xx, 408 (request timeout) and 429 (rate limit) are transient.
/// Callers GATE this with the #1371 effect-safe law
/// (`nika_types::net::retry_is_effect_safe`): a keyless effect-capable
/// method is never transient, whatever the table says.
pub(crate) fn is_transient_status(status: u16) -> bool {
    matches!(status, 500..=599 | 408 | 429)
}

/// `nika:notify` — send an alert. `webhook` MUST work (POST the message);
/// other channels are feature-gated → `NIKA-BUILTIN-NOTIFY-001` when
/// unconfigured (stdlib §notify).
///
/// SECURITY: `target:` is workflow-controlled — callers MUST inject an
/// SSRF-guarding `H` (production = `ReqwestHttp` with
/// `SsrfMode::Enforce`); this layer never re-implements the guard.
pub(crate) async fn notify<H: HttpPostDyn>(http: &H, args: &Args) -> BuiltinOutcome {
    const C1: &str = "NIKA-BUILTIN-NOTIFY-001";
    const C2: &str = "NIKA-BUILTIN-NOTIFY-002";
    let channel = opt_str(args, "channel", C1)?.unwrap_or("webhook");
    if channel != "webhook" {
        return Err(BuiltinFailure::new(
            C1,
            format!("channel `{channel}` is not configured (v0.1 engines MUST support `webhook`)"),
        ));
    }
    let target = req_str(args, "target", C1)?;
    let message = req_str(args, "message", C1)?;
    let severity = opt_str(args, "severity", C1)?.unwrap_or("info");

    let mut request = HttpRequest::post(target);
    request
        .headers
        .insert("content-type".to_owned(), "application/json".to_owned());
    // `{ message, severity, data? }` — `data:` carries structured context
    // so receivers branch on machine fields, never parse the human
    // message (stdlib §notify · the key is ABSENT when not given).
    let mut payload = serde_json::json!({ "message": message, "severity": severity });
    if let (Some(map), Some(data)) = (payload.as_object_mut(), args.get("data")) {
        map.insert("data".to_owned(), data.clone());
    }
    let body = serde_json::to_vec(&payload)
        .map_err(|e| BuiltinFailure::new(C1, format!("payload serialization failed: {e}")))?;
    request.body = Some(body.into());

    let response = http.post(request).await.map_err(|e| {
        // The webhook `target:` rides the SAME net security boundary as
        // fetch (SEC-004 permits / SEC-005 SSRF · shared helper); anything
        // else is a delivery failure.
        net_security_failure_for(&e, target)
            .unwrap_or_else(|| BuiltinFailure::new(C2, format!("delivery failed: {e}")))
    })?;
    if (200..300).contains(&response.status) {
        Ok(serde_json::Value::Null)
    } else {
        Err(
            BuiltinFailure::new(C2, format!("webhook returned HTTP {}", response.status))
                .with_transient(is_transient_status(response.status)),
        )
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod observation_tests;
