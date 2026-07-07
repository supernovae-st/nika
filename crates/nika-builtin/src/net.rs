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

use bytes::Bytes;
use nika_extract::{ExtractMode, ExtractOptions};
use nika_kernel::io::fs::FsReadDyn;
use nika_kernel::io::http::{HttpError, HttpGetDyn, HttpMethod, HttpPostDyn, HttpRequest};

use crate::permits::{FsAccess, FsBoundary};
use crate::wire::{self, Part};
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

/// `nika:fetch` — HTTP request + content extraction (stdlib §fetch).
/// Non-2xx is failure (`transient: true` for 5xx/408/429, `false` for
/// other 4xx — normative). The body is decoded then run through the
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
pub(crate) async fn fetch<H: HttpGetDyn + HttpPostDyn, F: FsReadDyn>(
    http: &H,
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
) -> BuiltinOutcome {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let url = req_str(args, "url", C)?;
    // `traverse:` is the bounded-crawl family — it owns its whole arg
    // surface (exclusivity · GET-only · closed spec) and its own output
    // shape, so it branches before any single-fetch vetting.
    if args.contains_key("traverse") {
        return crate::net_traverse::traverse(http, url, args).await;
    }
    let method = opt_str(args, "method", C)?.unwrap_or("GET").to_uppercase();
    // Vet the mode + arg pairings BEFORE the network call — a bad
    // `mode:` or a silently-droppable `selector:`/`jq:` should fail
    // without spending a request (the static checker catches literals;
    // this is the runtime defense for hand-built ToolCalls and
    // TEMPLATED modes the static ladder can't see · review lens 3 P2).
    let mode = parse_mode(args, C)?;
    let selector = opt_str(args, "selector", C)?.map(str::to_owned);
    if selector.is_some() && mode != ExtractMode::Selector {
        return Err(BuiltinFailure::new(
            C,
            "`selector:` pairs with `mode: selector` only (extract-modes-v0.1.md §selector)",
        ));
    }
    if args.contains_key("jq") && mode != ExtractMode::Jq {
        return Err(BuiltinFailure::new(
            C,
            "`jq:` is «a jq expression · only with mode: jq» (builtins-v0.1.md §nika:fetch)",
        ));
    }

    // ONE method parse — routing + the wire request both derive from
    // the enum (no string re-match to desync).
    let http_method = parse_method(&method).map_err(|m| BuiltinFailure::new(C, m))?;
    let request = prepare_request(http_method, url, fs, boundary, args).await?;
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
        net_security_failure(&e).unwrap_or_else(|| {
            let transient = matches!(e, HttpError::Timeout { .. } | HttpError::Connection { .. });
            BuiltinFailure::new(C, format!("request failed: {e}")).with_transient(transient)
        })
    })?;

    if !(200..300).contains(&response.status) {
        // `details.status_code` carries the status (stdlib §fetch ·
        // normative) — branching on 403 vs 429 must never mean parsing
        // the human message.
        return Err(
            BuiltinFailure::new(C, format!("HTTP {} from {url}", response.status))
                .with_transient(is_transient_status(response.status))
                .with_details(serde_json::json!({ "status_code": response.status })),
        );
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
    };
    tokio::task::spawn_blocking(move || plan.run(C))
        .await
        .map_err(|e| BuiltinFailure::new(C, format!("extraction task failed: {e}")))?
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
                crate::data::jq(&jq_args)
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

/// Assemble the outgoing request: the base (`build_request` · url ·
/// headers · raw `body:`) plus the vNext payload families — `form:`
/// (urlencoded) and `multipart:` (RFC 7578 · file parts read under the
/// `permits.fs` READ boundary). At most ONE of `body:`/`form:`/
/// `multipart:`; the payload families own their `content-type` (a
/// user-supplied one is a conflict, refused loudly) and need a
/// body-bearing method.
async fn prepare_request<F: FsReadDyn>(
    method: HttpMethod,
    url: &str,
    fs: &F,
    boundary: &FsBoundary,
    args: &Args,
) -> Result<HttpRequest, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let declared = [
        args.contains_key("body"),
        args.contains_key("form"),
        args.contains_key("multipart"),
    ];
    if declared.iter().filter(|present| **present).count() > 1 {
        return Err(BuiltinFailure::new(
            C,
            "at most one of `body:` · `form:` · `multipart:` (builtins-v0.1.md §nika:fetch)",
        ));
    }
    let wants_payload = args.contains_key("form") || args.contains_key("multipart");
    if wants_payload
        && !matches!(
            method,
            HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
        )
    {
        return Err(BuiltinFailure::new(
            C,
            "`form:`/`multipart:` need a body-bearing method — set `method: POST` (or PUT/PATCH)",
        ));
    }
    let mut request = build_request(method, url, args).map_err(|m| BuiltinFailure::new(C, m))?;
    if wants_payload
        && request
            .headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("content-type"))
    {
        return Err(BuiltinFailure::new(
            C,
            "`form:`/`multipart:` set their own content-type — drop the `headers:` entry",
        ));
    }
    if let Some(form) = args.get("form") {
        let encoded = form_body(form).map_err(|m| BuiltinFailure::new(C, m))?;
        request.headers.insert(
            "content-type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        );
        request.body = Some(encoded.into_bytes().into());
    }
    if let Some(spec) = args.get("multipart") {
        let parts = resolve_multipart(fs, boundary, spec).await?;
        let borrowed: Vec<Part<'_>> = parts.iter().map(OwnedPart::as_part).collect();
        let (body, content_type) =
            wire::multipart(&borrowed).map_err(|m| BuiltinFailure::new(C, m))?;
        request
            .headers
            .insert("content-type".to_owned(), content_type);
        request.body = Some(body.into());
    }
    Ok(request)
}

/// `form:` → `application/x-www-form-urlencoded` pairs. Scalars only —
/// nesting has no canonical urlencoded shape, so it is refused rather
/// than guessed (reshape upstream with `nika:jq`).
fn form_body(form: &serde_json::Value) -> Result<String, String> {
    let Some(map) = form.as_object() else {
        return Err("`form:` must be an object of scalar fields".to_owned());
    };
    let mut pairs = Vec::with_capacity(map.len());
    for (key, value) in map {
        let text = match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => {
                return Err(format!(
                    "`form.{key}:` must be a string, number or boolean — reshape nested \
                     data with nika:jq or send `body:` JSON instead"
                ));
            }
        };
        pairs.push((key.as_str(), text));
    }
    Ok(wire::form_urlencode(&pairs))
}

/// One resolved multipart part — file bytes already read (and permit-
/// gated); borrows are handed to the wire encoder.
struct OwnedPart {
    name: String,
    filename: Option<String>,
    mime: Option<String>,
    payload: PartPayload,
}

enum PartPayload {
    Text(String),
    File(Bytes),
}

impl OwnedPart {
    fn as_part(&self) -> Part<'_> {
        match &self.payload {
            PartPayload::Text(value) => Part::Text {
                name: &self.name,
                value,
            },
            PartPayload::File(bytes) => Part::File {
                name: &self.name,
                filename: self.filename.as_deref().unwrap_or("part"),
                mime: self.mime.as_deref().unwrap_or("application/octet-stream"),
                bytes,
            },
        }
    }
}

/// The closed part shape: `{name, value}` text XOR `{name, path,
/// filename?, content_type?}` file. Every `path:` is enforced against
/// the `permits.fs` READ boundary BEFORE any byte is read
/// (`NIKA-SEC-004` on escape — the image edit-input precedent).
const MULTIPART_PART_KEYS: [&str; 5] = ["name", "value", "path", "filename", "content_type"];
/// Upload ceiling — assets-not-blobs: bigger payloads ship by URL.
const MAX_MULTIPART_BYTES: usize = 32 * 1024 * 1024;
const MAX_MULTIPART_PARTS: usize = 64;

async fn resolve_multipart<F: FsReadDyn>(
    fs: &F,
    boundary: &FsBoundary,
    spec: &serde_json::Value,
) -> Result<Vec<OwnedPart>, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let fail = |m: String| BuiltinFailure::new(C, m);
    let Some(items) = spec.as_array() else {
        return Err(fail("`multipart:` must be an array of parts".to_owned()));
    };
    if items.is_empty() {
        return Err(fail("`multipart:` needs at least one part".to_owned()));
    }
    if items.len() > MAX_MULTIPART_PARTS {
        return Err(fail(format!(
            "`multipart:` caps at {MAX_MULTIPART_PARTS} parts (got {})",
            items.len()
        )));
    }
    let mut total = 0usize;
    let mut parts = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let Some(map) = item.as_object() else {
            return Err(fail(format!("multipart part {i} must be an object")));
        };
        if let Some(unknown) = map
            .keys()
            .find(|k| !MULTIPART_PART_KEYS.contains(&k.as_str()))
        {
            return Err(fail(format!(
                "multipart part {i}: unknown key `{unknown}` — the shape is \
                 {{name, value}} or {{name, path, filename?, content_type?}}"
            )));
        }
        let Some(name) = map.get("name").and_then(serde_json::Value::as_str) else {
            return Err(fail(format!("multipart part {i} needs a string `name:`")));
        };
        parts.push(resolve_part(fs, boundary, name, map, &mut total).await?);
        if total > MAX_MULTIPART_BYTES {
            return Err(fail(format!(
                "multipart payload exceeds {MAX_MULTIPART_BYTES} bytes (~32 MiB) — \
                 ship large assets by URL instead"
            )));
        }
    }
    Ok(parts)
}

/// Resolve one part (text value XOR permit-gated file read).
async fn resolve_part<F: FsReadDyn>(
    fs: &F,
    boundary: &FsBoundary,
    name: &str,
    map: &serde_json::Map<String, serde_json::Value>,
    total: &mut usize,
) -> Result<OwnedPart, BuiltinFailure> {
    const C: &str = "NIKA-BUILTIN-FETCH-001";
    let fail = |m: String| BuiltinFailure::new(C, m);
    match (map.get("value"), map.get("path")) {
        (Some(_), Some(_)) | (None, None) => Err(fail(format!(
            "multipart part `{name}`: exactly one of `value:` (text) | `path:` (file)"
        ))),
        (Some(value), None) => {
            let Some(text) = value.as_str() else {
                return Err(fail(format!(
                    "multipart part `{name}`: `value:` must be a string — stringify \
                     upstream with nika:jq"
                )));
            };
            if map.contains_key("filename") || map.contains_key("content_type") {
                return Err(fail(format!(
                    "multipart part `{name}`: `filename:`/`content_type:` belong to \
                     file parts (`path:`)"
                )));
            }
            *total += text.len();
            Ok(OwnedPart {
                name: name.to_owned(),
                filename: None,
                mime: None,
                payload: PartPayload::Text(text.to_owned()),
            })
        }
        (None, Some(path)) => {
            let Some(path) = path.as_str() else {
                return Err(fail(format!(
                    "multipart part `{name}`: `path:` must be a string"
                )));
            };
            boundary.enforce(fs, path, FsAccess::Read).await?;
            let bytes = fs.read(std::path::Path::new(path)).await.map_err(|e| {
                fail(format!(
                    "multipart part `{name}`: `{path}` could not be read: {e}"
                ))
            })?;
            *total += bytes.len();
            let filename = match map.get("filename") {
                Some(serde_json::Value::String(f)) => Some(f.clone()),
                Some(_) => {
                    return Err(fail(format!(
                        "multipart part `{name}`: `filename:` must be a string"
                    )));
                }
                None => std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned()),
            };
            let mime = match map.get("content_type") {
                Some(serde_json::Value::String(m)) => Some(m.clone()),
                Some(_) => {
                    return Err(fail(format!(
                        "multipart part `{name}`: `content_type:` must be a string"
                    )));
                }
                None => None,
            };
            Ok(OwnedPart {
                name: name.to_owned(),
                filename,
                mime,
                payload: PartPayload::File(bytes),
            })
        }
    }
}

fn build_request(method: HttpMethod, url: &str, args: &Args) -> Result<HttpRequest, String> {
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
        net_security_failure(&e)
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
mod tests {
    use super::*;
    use nika_kernel_mock::{MockFs, MockHttp};

    fn args(v: serde_json::Value) -> Args {
        match v {
            serde_json::Value::Object(map) => map,
            other => panic!("test arg must be an object, got {other}"),
        }
    }

    /// Test adapter — shadows `super::fetch` with the no-fs/no-boundary
    /// shape most cases need (multipart file-part tests call
    /// `super::fetch` with a real `MockFs` + declared boundary).
    async fn fetch(http: &MockHttp, args: &Args) -> BuiltinOutcome {
        super::fetch(http, &MockFs::new(), &FsBoundary::unbounded(), args).await
    }

    // ─── fetch vNext · form: + multipart: (stdlib §fetch) ──────────────

    #[tokio::test]
    async fn multipart_uploads_file_part_bytes_under_the_boundary() {
        let fs = MockFs::new().with_file("out/bg.png", b"\x89PNG-fake-image-bytes".to_vec());
        let boundary = FsBoundary::declared(vec!["out/**".to_owned()], Vec::new());
        let http =
            MockHttp::new().enqueue_ok(200, br#"{"url":"https://cdn.test/bg.png"}"#.to_vec());
        let out = super::fetch(
            &http,
            &fs,
            &boundary,
            &args(serde_json::json!({
                "url": "https://api.test/upload", "method": "POST", "mode": "jq", "jq": ".url",
                "multipart": [
                    { "name": "directory", "value": "assets" },
                    { "name": "file", "path": "out/bg.png", "content_type": "image/png" }
                ]
            })),
        )
        .await
        .expect("upload succeeds");
        assert_eq!(out, serde_json::json!("https://cdn.test/bg.png"));
        let sent = http.sent_requests();
        let req = sent.first().expect("one request");
        let body = req.body.as_ref().expect("multipart body");
        assert!(
            body.windows(21).any(|w| w == b"\x89PNG-fake-image-bytes"),
            "file bytes ride verbatim"
        );
        let text = String::from_utf8_lossy(body);
        assert!(text.contains("name=\"directory\"\r\n\r\nassets"));
        assert!(text.contains("filename=\"bg.png\"") && text.contains("Content-Type: image/png"));
        let ct = req.headers.get("content-type").expect("content-type set");
        assert!(ct.starts_with("multipart/form-data; boundary="), "{ct}");
    }

    #[tokio::test]
    async fn multipart_path_outside_the_boundary_is_sec_004() {
        let fs = MockFs::new().with_file("secrets/key.pem", b"private".to_vec());
        let boundary = FsBoundary::declared(vec!["out/**".to_owned()], Vec::new());
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = super::fetch(
            &http,
            &fs,
            &boundary,
            &args(serde_json::json!({
                "url": "https://api.test/upload", "method": "POST",
                "multipart": [ { "name": "file", "path": "secrets/key.pem" } ]
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == crate::permits::SEC_DENIED),
            "escape must be NIKA-SEC-004: {fail:?}"
        );
        assert!(
            http.sent_requests().is_empty(),
            "no byte leaves the machine"
        );
    }

    #[tokio::test]
    async fn multipart_part_shape_violations_are_loud() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        for (spec, needle) in [
            (serde_json::json!([]), "at least one part"),
            (serde_json::json!([{ "name": "f" }]), "exactly one of"),
            (
                serde_json::json!([{ "name": "f", "value": "v", "path": "p" }]),
                "exactly one of",
            ),
            (
                serde_json::json!([{ "name": "f", "value": "v", "surprise": 1 }]),
                "unknown key",
            ),
            (
                serde_json::json!([{ "name": "f", "value": "v", "filename": "x" }]),
                "file parts",
            ),
        ] {
            let fail = fetch(
                &http,
                &args(serde_json::json!({
                    "url": "https://api.test", "method": "POST", "multipart": spec
                })),
            )
            .await;
            assert!(
                matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                    && f.message.contains(needle)),
                "wanted `{needle}` in {fail:?}"
            );
        }
    }

    #[tokio::test]
    async fn multipart_conflicting_user_content_type_is_refused() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "method": "POST",
                "headers": { "Content-Type": "application/json" },
                "multipart": [ { "name": "f", "value": "v" } ]
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                && f.message.contains("content-type")),
            "{fail:?}"
        );
    }

    #[tokio::test]
    async fn form_encodes_urlencoded_body_and_content_type() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let out = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test/submit", "method": "POST", "mode": "raw",
                "form": { "a": "b c", "n": 5, "flag": true }
            })),
        )
        .await
        .expect("form post succeeds");
        assert_eq!(out, serde_json::Value::String("ok".to_owned()));
        let sent = http.sent_requests();
        let req = sent.first().expect("one request");
        let body = req.body.as_ref().expect("form body present");
        assert_eq!(
            std::str::from_utf8(body).expect("utf8"),
            "a=b+c&flag=true&n=5",
            "urlencoded · space→+ · BTreeMap key order"
        );
        assert_eq!(
            req.headers.get("content-type").map(String::as_str),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[tokio::test]
    async fn form_on_get_is_a_clear_error() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "form": { "a": "b" }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                && f.message.contains("POST")),
            "{fail:?}"
        );
        assert!(http.sent_requests().is_empty(), "no request spent");
    }

    #[tokio::test]
    async fn body_form_multipart_are_mutually_exclusive() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        let fail = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "method": "POST", "body": "x",
                "form": { "a": "b" }
            })),
        )
        .await;
        assert!(
            matches!(&fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"
                && f.message.contains("at most one")),
            "{fail:?}"
        );
    }

    #[tokio::test]
    async fn fetch_raw_returns_body_verbatim_fails_on_4xx() {
        // mode: raw is the transport-passthrough (no extraction).
        let http = MockHttp::new().enqueue_ok(200, "hello world".as_bytes().to_vec());
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "raw" })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::Value::String("hello world".to_owned()));

        let http = MockHttp::new().enqueue_ok(404, Vec::new());
        let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" }))).await;
        assert!(
            matches!(fail, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001" && f.message.contains("404"))
        );
    }

    #[tokio::test]
    async fn fetch_default_mode_is_markdown_extraction() {
        // No mode: → markdown (the spec default · extract-modes-v0.1.md).
        let html = b"<html><body><h1>Title</h1><p>Body text.</p>\
                     <script>evil()</script></body></html>"
            .to_vec();
        let http = MockHttp::new().enqueue_ok(200, html);
        let out = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
            .await
            .expect("ok");
        let md = out.as_str().expect("string");
        assert!(md.contains("# Title"), "heading→markdown: {md}");
        assert!(md.contains("Body text."), "prose survives");
        assert!(!md.contains("evil()"), "script stripped: {md}");
    }

    #[tokio::test]
    async fn fetch_mode_jq_composes_the_one_jq_engine() {
        let json = br#"{"items":[{"name":"a"},{"name":"b"}]}"#.to_vec();
        let http = MockHttp::new().enqueue_ok(200, json);
        let out = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "mode": "jq", "jq": "[.items[].name]"
            })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::json!(["a", "b"]));

        // The one-output law is the jq engine's (a bare stream is rejected
        // with the [ … ]-collect advice · NOT re-implemented here).
        let json = br#"{"items":[{"name":"a"},{"name":"b"}]}"#.to_vec();
        let http = MockHttp::new().enqueue_ok(200, json);
        let stream = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://api.test", "mode": "jq", "jq": ".items[].name"
            })),
        )
        .await
        .expect_err("stream is a single-output violation");
        assert!(stream.message.contains("[ … ]"), "{}", stream.message);
    }

    #[tokio::test]
    async fn fetch_mode_jq_on_non_json_is_a_clear_error() {
        let http = MockHttp::new().enqueue_ok(200, b"<html>not json</html>".to_vec());
        let err = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "mode": "jq", "jq": "."
            })),
        )
        .await
        .expect_err("html is not json");
        assert!(err.code == "NIKA-BUILTIN-FETCH-001" && err.message.contains("not JSON"));
    }

    #[tokio::test]
    async fn fetch_mode_selector_extracts_matches() {
        let html = b"<div class=\"x\"><p>one</p></div><div class=\"x\"><p>two</p></div>".to_vec();
        let http = MockHttp::new().enqueue_ok(200, html);
        let out = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "mode": "selector", "selector": "div.x"
            })),
        )
        .await
        .expect("ok");
        let s = out.as_str().expect("string");
        assert!(s.contains("<p>one</p>") && s.contains("<p>two</p>"), "{s}");
    }

    #[tokio::test]
    async fn fetch_unknown_mode_fails_before_the_network() {
        let http = MockHttp::new(); // no response enqueued
        let err = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "html" })),
        )
        .await
        .expect_err("html is not a mode");
        assert_eq!(err.code, "NIKA-BUILTIN-FETCH-001");
        assert!(err.message.contains("closed"), "{}", err.message);
        assert!(http.sent_requests().is_empty(), "no request was spent");
    }

    #[tokio::test]
    async fn fetch_raw_rejects_non_utf8_body() {
        // 0xFF is never valid UTF-8 — raw is the spec's text contract.
        let http = MockHttp::new().enqueue_ok(200, vec![0xff, 0xfe, 0x00]);
        let err = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "raw" })),
        )
        .await
        .expect_err("non-utf8 raw");
        assert!(err.message.contains("not valid UTF-8"), "{}", err.message);
    }

    #[tokio::test]
    async fn fetch_runtime_mirrors_the_pairing_rules() {
        // A templated mode bypasses the STATIC checker — the runtime
        // must reject the same pairings loud, never silently drop args.
        let http = MockHttp::new(); // nothing enqueued — must fail pre-network
        let err = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "mode": "text", "selector": "div"
            })),
        )
        .await
        .expect_err("selector with non-selector mode");
        assert!(err.message.contains("mode: selector"), "{}", err.message);

        let err = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "jq": ".x"
            })),
        )
        .await
        .expect_err("jq with the default markdown mode");
        assert!(err.message.contains("mode: jq"), "{}", err.message);
        assert!(http.sent_requests().is_empty(), "no request spent");
    }

    #[tokio::test]
    async fn fetch_metadata_merges_link_header_alternates() {
        let html = br#"<html lang="en"><head><title>T</title></head><body></body></html>"#.to_vec();
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [(
                "Link",
                r#"<https://x.test/fr/>; rel="alternate"; hreflang="fr", <https://x.test/de/>; rel="alternate"; hreflang="de""#,
            )],
            html,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "metadata" })),
        )
        .await
        .expect("ok");
        assert_eq!(out["alternates"][0]["lang"], "fr");
        assert_eq!(out["alternates"][1]["href"], "https://x.test/de/");
        assert_eq!(out["title"], "T", "the HTML head still mined");
    }

    #[tokio::test]
    async fn fetch_decodes_declared_charset_for_extraction() {
        // "Café" in ISO-8859-1: 'é' = 0xE9. UTF-8-lossy would corrupt it;
        // charset-aware decode recovers it.
        let body = vec![
            b'<', b'p', b'>', b'C', b'a', b'f', 0xe9, b'<', b'/', b'p', b'>',
        ];
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=iso-8859-1")],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert_eq!(out.as_str(), Some("Café"), "ISO-8859-1 é recovered");
    }

    #[tokio::test]
    async fn fetch_bom_overrides_header_charset() {
        // WHATWG: the BOM is more authoritative than the header. A
        // UTF-16LE body with a misleading `charset=utf-8` header must
        // decode via the BOM, not mojibake as UTF-8 (the audit's P3).
        // "Hi" in UTF-16LE with BOM: FF FE 48 00 69 00.
        let body = vec![0xff, 0xfe, b'H', 0x00, b'i', 0x00];
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=utf-8")],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert_eq!(
            out.as_str(),
            Some("Hi"),
            "UTF-16LE BOM beat the lying header"
        );
    }

    #[tokio::test]
    async fn fetch_meta_charset_prescan_when_header_absent() {
        // No charset in the header → the <meta charset> prescan recovers
        // it (the legacy-page gap). 'é' = 0xE9 in ISO-8859-1.
        let mut body = br#"<html><head><meta charset="iso-8859-1"></head><body><p>Caf"#.to_vec();
        body.push(0xe9);
        body.extend_from_slice(b"</p></body></html>");
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html")], // NO charset param
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert!(
            out.as_str().is_some_and(|s| s.contains("Café")),
            "meta-charset prescan recovered ISO-8859-1: {out:?}"
        );
    }

    #[test]
    fn meta_charset_ignores_a_meta_inside_a_comment() {
        // A `<meta charset>` living in a comment is NOT a declaration (the
        // WHATWG prescan steps over comments) — the real one downstream wins.
        assert_eq!(
            meta_charset(b"<!-- <meta charset=koi8-r> --><meta charset=shift_jis>"),
            Some(encoding_rs::SHIFT_JIS),
            "the commented <meta> must be skipped; the real one wins"
        );
        // Comment-only → no declaration (decode falls back to UTF-8 upstream).
        assert_eq!(
            meta_charset(b"<!-- <meta charset=koi8-r> -->"),
            None,
            "a <meta> seen only inside a comment yields no charset"
        );
    }

    #[tokio::test]
    async fn fetch_header_charset_beats_meta_prescan() {
        // Precedence: a Content-Type charset OUTRANKS a (conflicting)
        // <meta> declaration. Body is ISO-8859-1 (0xE9 = é); meta lies
        // "utf-8", header says "iso-8859-1" → header wins, é recovered.
        let mut body = br#"<html><head><meta charset="utf-8"></head><body><p>Caf"#.to_vec();
        body.push(0xe9);
        body.extend_from_slice(b"</p></body></html>");
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=iso-8859-1")],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert!(out.as_str().is_some_and(|s| s.contains("Café")), "{out:?}");
    }

    #[tokio::test]
    async fn fetch_charset_matrix_windows1252_shiftjis_quoted() {
        // windows-1252: € = 0x80 (the byte ISO-8859-1 maps to a C1
        // control — the label distinction is real).
        let body = vec![b'<', b'p', b'>', 0x80, b'5', b'<', b'/', b'p', b'>'];
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=windows-1252")],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert_eq!(out.as_str(), Some("€5"), "windows-1252 euro sign");

        // Shift_JIS: 日本 = 93 FA 96 7B.
        let body = vec![
            b'<', b'p', b'>', 0x93, 0xfa, 0x96, 0x7b, b'<', b'/', b'p', b'>',
        ];
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=Shift_JIS")],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert_eq!(out.as_str(), Some("日本"), "Shift_JIS kanji");

        // Quote-aware param split: a `;` inside a QUOTED earlier param
        // must not hide the real charset (review lens 2 · P3-3).
        let body = vec![
            b'<', b'p', b'>', b'C', b'a', b'f', 0xe9, b'<', b'/', b'p', b'>',
        ];
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [(
                "Content-Type",
                r#"text/html; title="a;charset=koi8-r"; charset=iso-8859-1"#,
            )],
            body,
        );
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "mode": "text" })),
        )
        .await
        .expect("ok");
        assert_eq!(
            out.as_str(),
            Some("Café"),
            "quoted ; did not derail the scan"
        );
    }

    #[tokio::test]
    async fn fetch_huge_body_extracts_without_distortion() {
        // ~600 KB of repeated paragraphs: the blocking-pool handoff +
        // markdown pipeline must hold shape (no truncation · no panic).
        let para = "<p>Sixty kilobyte stress paragraph with stable words.</p>";
        let html = format!("<html><body>{}</body></html>", para.repeat(10_000));
        let http = MockHttp::new().enqueue_ok(200, html.into_bytes());
        let out = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
            .await
            .expect("ok");
        let md = out.as_str().expect("string");
        assert_eq!(
            md.matches("Sixty kilobyte stress paragraph").count(),
            10_000,
            "every paragraph survived"
        );
    }

    #[tokio::test]
    async fn fetch_non_string_header_value_is_loud() {
        let http = MockHttp::new();
        let err = fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "headers": { "x-count": 1 }
            })),
        )
        .await
        .expect_err("non-string header");
        assert!(err.message.contains("x-count"), "{}", err.message);
        assert!(http.sent_requests().is_empty(), "failed before the wire");
    }

    #[tokio::test]
    async fn fetch_transient_follows_the_spec_status_table() {
        // 5xx · 408 · 429 are transient; other 4xx are not (stdlib §fetch).
        for (status, expect) in [
            (503, true),
            (500, true),
            (408, true),
            (429, true),
            (404, false),
        ] {
            let http = MockHttp::new().enqueue_ok(status, Vec::new());
            let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
                .await
                .expect_err("non-2xx fails");
            assert_eq!(fail.transient, expect, "HTTP {status}");
        }
        // The boundary neighbours of the 5xx range stay non-transient.
        assert!(!is_transient_status(499));
        assert!(is_transient_status(599));
        assert!(!is_transient_status(600));
        assert!(!is_transient_status(200));
    }

    #[tokio::test]
    async fn fetch_transport_failures_are_transient() {
        // BUG-D: connection + timeout transport errors are the textbook
        // transient case (DNS/connection-refused/reset surface as
        // HttpError::Connection) — they must be retryable so `retry:` works.
        use nika_kernel::io::http::HttpError;
        for err in [
            HttpError::Timeout { duration_ms: 5000 },
            HttpError::Connection {
                reason: "dns resolution failed".to_owned(),
            },
        ] {
            let http = MockHttp::new().enqueue_err(err);
            let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
                .await
                .expect_err("transport failure");
            assert_eq!(fail.code, "NIKA-BUILTIN-FETCH-001");
            assert!(fail.transient, "a transport failure is retryable");
        }
        // An SSRF/scheme rejection (a deterministic refusal) is NOT transient,
        // and speaks the security-plane NIKA-SEC-005 (not the generic
        // FETCH-001) so it derives `security_error` + never reaches an agent.
        let http = MockHttp::new().enqueue_err(HttpError::SsrfBlocked {
            url: "http://127.0.0.1".to_owned(),
        });
        let fail = fetch(
            &http,
            &args(serde_json::json!({ "url": "http://127.0.0.1" })),
        )
        .await
        .expect_err("ssrf blocked");
        assert!(!fail.transient, "an SSRF block is a deterministic refusal");
        assert_eq!(fail.code, "NIKA-SEC-005", "SSRF is the security-plane code");
    }

    #[tokio::test]
    async fn host_not_allowed_surfaces_as_nika_sec_004() {
        // A permits.net.http escape (the kernel HostNotAllowed) is the
        // spec-plane NIKA-SEC-004 capability denial — NOT a transport
        // failure, NEVER retryable, distinct from the SSRF floor's
        // NIKA-SEC-005. This is the user-facing half of the runtime boundary.
        let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
            host: "evil.com".to_owned(),
        });
        let fail = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://evil.com" })),
        )
        .await
        .expect_err("host outside permits.net.http");
        assert_eq!(
            fail.code, "NIKA-SEC-004",
            "a declared-boundary escape is the security code"
        );
        assert!(!fail.transient, "a capability denial is never retryable");
        assert!(fail.message.contains("net.http"), "{}", fail.message);

        // notify's webhook `target:` rides the very same boundary.
        let http = MockHttp::new().enqueue_err(HttpError::HostNotAllowed {
            host: "evil.com".to_owned(),
        });
        let fail = notify(
            &http,
            &args(serde_json::json!({ "target": "https://evil.com", "message": "x" })),
        )
        .await
        .expect_err("notify target outside permits.net.http");
        assert_eq!(fail.code, "NIKA-SEC-004", "notify honors the same boundary");
    }

    #[tokio::test]
    async fn notify_ssrf_blocked_surfaces_as_nika_sec_005() {
        // The SSRF floor on the webhook `target:` is the security-plane
        // NIKA-SEC-005 (non-transient · never agent-fed) — same as fetch.
        let http = MockHttp::new().enqueue_err(HttpError::SsrfBlocked {
            url: "http://169.254.169.254".to_owned(),
        });
        let fail = notify(
            &http,
            &args(serde_json::json!({ "target": "http://169.254.169.254", "message": "x" })),
        )
        .await
        .expect_err("notify target is an SSRF block");
        assert_eq!(fail.code, "NIKA-SEC-005", "SSRF is the security floor code");
        assert!(!fail.transient, "an SSRF block is a deterministic refusal");
    }

    #[tokio::test]
    async fn fetch_failure_details_carry_the_status_code() {
        // `details.status_code` is normative (stdlib §fetch) — branching
        // on 403 vs 429 must never mean parsing the human message.
        let http = MockHttp::new().enqueue_ok(429, Vec::new());
        let fail = fetch(&http, &args(serde_json::json!({ "url": "https://x.test" })))
            .await
            .expect_err("non-2xx fails");
        assert_eq!(
            fail.details,
            Some(serde_json::json!({ "status_code": 429 })),
            "machine-readable status in details"
        );
        // Transport-plane failures carry no status (no response existed).
        let empty = MockHttp::new();
        let transport = fetch(
            &empty,
            &args(serde_json::json!({ "url": "https://x.test" })),
        )
        .await
        .expect_err("no canned response = transport error");
        assert_eq!(transport.details, None);
    }

    #[tokio::test]
    async fn fetch_head_succeeds_with_an_empty_body() {
        // HEAD carries no body per HTTP — a 200 HEAD is an empty-string
        // success, not an error.
        let http = MockHttp::new().enqueue_ok(200, Vec::new());
        let out = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "method": "HEAD" })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::Value::String(String::new()));
    }

    #[tokio::test]
    async fn fetch_post_carries_the_body() {
        let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
        fetch(
            &http,
            &args(serde_json::json!({
                "url": "https://x.test", "method": "POST", "body": {"a": 1}
            })),
        )
        .await
        .expect("ok");
        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        assert!(matches!(sent[0].method, HttpMethod::Post));
        assert!(sent[0].body.is_some());
    }

    #[tokio::test]
    async fn fetch_maps_every_method_to_the_request() {
        for (name, expected) in [
            ("GET", HttpMethod::Get),
            ("POST", HttpMethod::Post),
            ("PUT", HttpMethod::Put),
            ("DELETE", HttpMethod::Delete),
            ("PATCH", HttpMethod::Patch),
            ("HEAD", HttpMethod::Head),
        ] {
            let http = MockHttp::new().enqueue_ok(200, b"ok".to_vec());
            fetch(
                &http,
                &args(serde_json::json!({ "url": "https://x.test", "method": name })),
            )
            .await
            .expect("ok");
            let sent = http.sent_requests();
            assert_eq!(sent.len(), 1, "{name} sent one request");
            assert_eq!(
                std::mem::discriminant(&sent[0].method),
                std::mem::discriminant(&expected),
                "{name} maps to its HttpMethod"
            );
        }
        // An unsupported method is a build-request failure.
        let http = MockHttp::new();
        let bad = fetch(
            &http,
            &args(serde_json::json!({ "url": "https://x.test", "method": "TRACE" })),
        )
        .await;
        assert!(matches!(bad, Err(f) if f.code == "NIKA-BUILTIN-FETCH-001"));
        assert!(
            http.sent_requests().is_empty(),
            "TRACE never reached the wire"
        );
    }

    #[tokio::test]
    async fn notify_webhook_only_at_v0_1() {
        let http = MockHttp::new().enqueue_ok(200, Vec::new());
        let out = notify(
            &http,
            &args(serde_json::json!({ "target": "https://hooks.x", "message": "done" })),
        )
        .await
        .expect("ok");
        assert_eq!(out, serde_json::Value::Null);

        let http = MockHttp::new();
        let unconfigured = notify(
            &http,
            &args(serde_json::json!({ "channel": "slack", "target": "x", "message": "y" })),
        )
        .await;
        assert!(matches!(unconfigured, Err(f) if f.code == "NIKA-BUILTIN-NOTIFY-001"));

        // A 5xx webhook delivery is transient (same status table as fetch).
        let http = MockHttp::new().enqueue_ok(503, Vec::new());
        let retry = notify(
            &http,
            &args(serde_json::json!({ "target": "https://hooks.x", "message": "m" })),
        )
        .await
        .expect_err("5xx fails");
        assert!(retry.code == "NIKA-BUILTIN-NOTIFY-002" && retry.transient);
    }

    #[tokio::test]
    async fn notify_data_rides_the_payload_and_is_absent_when_not_given() {
        // With data: the payload is { message, severity, data } —
        // receivers branch on machine fields, never parse the message.
        let http = MockHttp::new().enqueue_ok(200, Vec::new());
        notify(
            &http,
            &args(serde_json::json!({
                "target": "https://hooks.x", "message": "done",
                "data": { "run": "r-42", "count": 7 }
            })),
        )
        .await
        .expect("ok");
        let sent = http.sent_requests();
        let body: serde_json::Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(
            body,
            serde_json::json!({
                "message": "done", "severity": "info",
                "data": { "run": "r-42", "count": 7 }
            })
        );

        // Without data: the key is ABSENT (not null — spec §notify).
        let http = MockHttp::new().enqueue_ok(200, Vec::new());
        notify(
            &http,
            &args(serde_json::json!({ "target": "https://hooks.x", "message": "m" })),
        )
        .await
        .expect("ok");
        let sent = http.sent_requests();
        let body: serde_json::Value =
            serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
        assert_eq!(
            body,
            serde_json::json!({ "message": "m", "severity": "info" })
        );
        assert!(body.get("data").is_none(), "absent, never null");
    }
}
