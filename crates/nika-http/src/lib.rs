// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-http` — the production HTTP implementation for the Nika diamond.
//!
//! This crate sits at **L1** (effect crate): it implements the L0.5
//! `nika_kernel::http` traits (`HttpGet` · `HttpPost` — and the blanket
//! `HttpClient`) using `reqwest` (rustls — no openssl). Every crate that
//! talks HTTP injects the kernel traits and receives [`ReqwestHttp`] in
//! production, a mock in tests (Invariant #27).
//!
//! [`ReqwestHttp`] implements the **`*Dyn` trait-variant companions**
//! (`Send` futures — base traits via the `trait_variant` blanket impls),
//! same pattern as `nika-fs`/`nika-clock`.
//!
//! # SSRF defense (on by default · the Diamond upgrade vs brouillon)
//!
//! Workflows fetch attacker-influenced URLs, so the MECHANISM is safe
//! before `nika-policy` (L1.5) adds capability gating. Four layers:
//!
//! 1. **Static** (the `ssrf` module · pure): scheme allow-list (http/https),
//!    blocked hostnames (localhost · cloud metadata), literal-IP range
//!    checks against the single oracle (see the `ssrf` module docs for
//!    the full range list).
//! 2. **DNS-resolve (advisory · fast-fail)**: non-literal hosts are
//!    resolved via `tokio::net::lookup_host` and EVERY address is
//!    range-checked — kills decimal-IP tricks (`http://2130706433/`)
//!    and public-names-resolving-private with a TYPED error before any
//!    connection attempt.
//! 3. **Resolver-enforced connect path (the TOCTOU killer)**: the
//!    reqwest client's DNS resolver IS `GuardedResolver` (private) — every
//!    address the transport will actually connect to is range-checked
//!    inside the lookup that produces it. A DNS rebind between layer 2
//!    and connect time now fails at the resolver; there is no window
//!    in which an unchecked address reaches the socket. (Pooled
//!    connections skip resolution — an established socket cannot be
//!    rebound.) Ambient `HTTP(S)_PROXY` env vars are IGNORED
//!    (`no_proxy`): a proxy resolves names itself, which would bypass
//!    this layer — explicit proxy support is a future `HttpConfig`
//!    field, never ambient.
//! 4. **Per-hop re-check**: reqwest redirects are DISABLED
//!    (`Policy::none`); this crate follows redirects itself and re-runs
//!    layers 1+2 on every hop (layer 3 guards each hop's connect) —
//!    per-request `follow_redirects` works, and a public host can not
//!    bounce the client into private space.
//!
//! The ONE narrow carve-out (issue #395 · ADR-092 egress precedent): an
//! EXACT loopback literal in the declared `permits.net.http`
//! (`localhost` · `127.x.y.z` · `::1`/`[::1]` — never a glob, never
//! RFC1918/link-local/metadata) is the author's declassification and
//! clears the floor for THAT host only, across all four layers: the
//! static vet skips the block for the exact host, and the resolve layers
//! admit the LOOPBACK addresses of that permitted NAME only (a rebind to
//! any other blocked range, or a redirect to an un-permitted floor host,
//! still refuses).
//!
//! # Response size caps
//!
//! `max_response_bytes` (default 64 MiB) activates the kernel's
//! [`HttpError::TooLarge`]: a `Content-Length` above the cap fails
//! fast; bodies without one are capped while reading; streaming bodies
//! get a counting wrapper that yields `TooLarge` mid-stream.
//!
//! # Compression (transparent) + HTTP/2
//!
//! reqwest's `gzip`/`brotli`/`deflate` features are ON: the client
//! sends `Accept-Encoding` and decompresses transparently (pure-Rust
//! codecs). Cap semantics under compression: reqwest hides the
//! compressed `Content-Length` for encoded bodies (`content_length()`
//! is `None`), so the fail-fast branch is skipped and the COUNTING
//! reader caps the DECOMPRESSED stream — the cap holds against
//! decompression bombs, it just fires while reading instead of before.
//! `http2` is ON (ALPN over rustls); plain-`http://` stays HTTP/1.1
//! (h2c is not attempted — ALPN needs TLS).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod ssrf;

use std::collections::BTreeMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;
use nika_kernel::http::{HttpGetDyn, HttpPostDyn};
use nika_kernel::{HttpError, HttpMethod, HttpRequest, HttpResponse, HttpStreamResponse};

/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default maximum redirect hops.
const DEFAULT_MAX_REDIRECTS: u8 = 5;
/// Default maximum response body size (64 MiB).
const DEFAULT_MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
/// Upper bound on the body-buffer PRE-allocation (the Content-Length
/// hint is attacker-influenced — see `read_capped`).
const PREALLOC_FLOOR: u64 = 256 * 1024;

/// SSRF enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SsrfMode {
    /// Block private/internal targets (the default).
    #[default]
    Enforce,
    /// No SSRF checks — tests and trusted internal networks only.
    /// This is an explicit, auditable opt-out; the policy layer
    /// (`nika-policy` · L1.5) decides who may ever set it.
    Disabled,
}

/// The workflow's declared `permits.net.http` boundary (spec `01-envelope.md`
/// §permits) — the net analogue of `nika-builtin`'s `FsBoundary`, so the two
/// capability axes read symmetrically at the composition root.
///
/// The host the boundary checks is taken from the PARSED [`url::Url`] (what
/// the transport connects to), never a re-parse of the raw string — a string
/// parser disagrees with WHATWG normalization (`\`, userinfo, C0 bytes) and
/// that gap is a bypass. Enforced on EVERY redirect hop (see `check_net_allowlist`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetBoundary {
    /// No declared boundary — the always-on SSRF floor is the only net guard
    /// (today's default · a workflow with no `permits:` block).
    #[default]
    Unbounded,
    /// A declared allowlist of host globs (`*.github.com`). DEFAULT-DENY: a
    /// host outside it fails [`HttpError::HostNotAllowed`] (→ `NIKA-SEC-004`).
    /// An EMPTY list admits nothing (a `permits:` block present but omitting
    /// `net` = no outbound network).
    ///
    /// An EXACT loopback literal among the entries (`localhost` ·
    /// `127.x.y.z` · `::1`/`[::1]`) is ALSO the author's SSRF-floor
    /// declassification for that host (issue #395 · ADR-092 egress
    /// precedent) — never a glob, never RFC1918/link-local/metadata (those
    /// stay floor-blocked even when named).
    Declared(Vec<String>),
}

impl NetBoundary {
    /// The declared glob list — empty for [`NetBoundary::Unbounded`]. The
    /// ONE slice the floor declassification, the allowlist, and the
    /// guarded resolver all read.
    fn globs(&self) -> &[String] {
        match self {
            Self::Declared(globs) => globs,
            Self::Unbounded => &[],
        }
    }
}

/// Configuration for [`ReqwestHttp`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HttpConfig {
    /// Default request timeout (a per-request `timeout` wins).
    pub timeout: Duration,
    /// Maximum redirect hops followed (each hop re-checked). Counts
    /// REDIRECTS, not requests: `5` allows the initial request plus up
    /// to 5 follow-ups (6 wire requests total).
    pub max_redirects: u8,
    /// Maximum response body size in bytes ([`HttpError::TooLarge`]).
    pub max_response_bytes: u64,
    /// SSRF enforcement mode.
    pub ssrf: SsrfMode,
    /// The workflow's declared `permits.net.http` boundary (default
    /// [`NetBoundary::Unbounded`] · the SSRF floor is then the only net
    /// guard). Re-checked on EVERY redirect hop.
    pub net: NetBoundary,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            ssrf: SsrfMode::Enforce,
            net: NetBoundary::Unbounded,
        }
    }
}

impl HttpConfig {
    /// Create the default configuration (SSRF enforced · 30s timeout ·
    /// 5 redirects · 64 MiB cap).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Production HTTP client backed by `reqwest` (rustls).
///
/// Cheap to clone (`reqwest::Client` is an `Arc` internally). Redirects
/// are followed by THIS crate (never by reqwest) so SSRF re-checks run
/// on every hop — see the crate docs.
#[derive(Debug, Clone)]
pub struct ReqwestHttp {
    inner: reqwest::Client,
    config: HttpConfig,
}

/// The connect host of a parsed URL, normalized to how `permits.net.http`
/// is written: bracket-free for IPv6 (`::1`, never `[::1]` — `url::Host`'s
/// Display would add brackets, so take the inner `Ipv6Addr`) and with an
/// absolute-FQDN trailing dot stripped (`allowed.com.` ≡ `allowed.com` · the
/// SSRF layer normalizes this too). `None` when the URL has no host.
///
/// SHARED SEMANTICS with the static checker (`nika-schema`'s `url_host`):
/// both take the host from the SAME `url::Url` the transport connects with —
/// a hand-rolled string parser disagrees with WHATWG normalization (`\` is a
/// path separator for http/https; userinfo + C0 bytes are stripped), and
/// that gap is a bypass (`https://evil.com\@allowed.com` → string-host
/// `allowed.com` but connect-host `evil.com`). The two extractors are pinned
/// identical by [`nika_types::net::HOST_EXTRACTION_VECTORS`] (asserted in
/// BOTH crates' tests) so check and runtime can never drift.
fn host_of(url: &url::Url) -> Option<String> {
    match url.host()? {
        url::Host::Domain(d) => Some(d.trim_end_matches('.').to_owned()),
        url::Host::Ipv4(a) => Some(a.to_string()),
        url::Host::Ipv6(a) => Some(a.to_string()),
    }
}

/// Enforce a declared [`NetBoundary`] on ONE already-parsed URL (one hop).
///
/// [`NetBoundary::Unbounded`] → `Ok` (the caller's SSRF floor is the only net
/// guard, today's behavior). [`NetBoundary::Declared`] is DEFAULT-DENY (spec
/// `01-envelope.md` §permits): the host (via [`host_of`] · the parsed connect
/// host, not the raw string) must match a glob, else
/// [`HttpError::HostNotAllowed`]. A URL with no host under a declared boundary
/// is DENIED (it cannot be confirmed in-bounds).
fn check_net_allowlist(boundary: &NetBoundary, url: &url::Url) -> Result<(), HttpError> {
    let NetBoundary::Declared(globs) = boundary else {
        return Ok(());
    };
    let Some(host) = host_of(url) else {
        return Err(HttpError::HostNotAllowed {
            host: url.as_str().to_owned(),
        });
    };
    // A qualifying loopback literal admits its host too (#395): the glob
    // matcher knows nothing of the `[::1]` authority spelling, and an
    // entry that CLEARS the floor for a host but then failed the
    // allowlist would be check-green + run-refused — the drift class this
    // carve-out closes. Loopback-exact only; it widens nothing else.
    if nika_types::net::host_in_allowlist(globs, &host)
        || nika_types::net::loopback_declassified(globs, &host)
    {
        Ok(())
    } else {
        Err(HttpError::HostNotAllowed { host })
    }
}

impl ReqwestHttp {
    /// Create a client with default configuration.
    ///
    /// # Errors
    ///
    /// [`HttpError::Other`] when the TLS backend fails to initialize.
    pub fn new() -> Result<Self, HttpError> {
        Self::with_config(HttpConfig::default())
    }

    /// Create a client with explicit configuration.
    ///
    /// # Errors
    ///
    /// [`HttpError::Other`] when the TLS backend fails to initialize.
    pub fn with_config(config: HttpConfig) -> Result<Self, HttpError> {
        let mut builder = reqwest::Client::builder()
            .user_agent(concat!("nika/", env!("CARGO_PKG_VERSION")))
            // Redirects are followed manually with per-hop SSRF
            // re-checks — reqwest must never follow on its own.
            .redirect(reqwest::redirect::Policy::none())
            // Idle-read guard: a connection that stops DELIVERING for a
            // full timeout window dies — this (not a total deadline)
            // is what protects STREAMING responses, where wall-clock
            // legitimately exceeds any total budget (SSE generations).
            .read_timeout(config.timeout)
            // Ambient HTTP(S)_PROXY env vars are ignored: a proxy
            // resolves names itself, which would bypass the guarded
            // resolver below. Proxy support is a future explicit
            // HttpConfig field — never ambient (crate docs · layer 3).
            .no_proxy();
        if config.ssrf == SsrfMode::Enforce {
            // Layer 3: range-check INSIDE the connect path. The resolver
            // carries the declared `permits.net.http` so a permitted exact
            // loopback literal (#395) clears its OWN resolution at the
            // connect path too — per resolved NAME, never per address class.
            builder = builder.dns_resolver(std::sync::Arc::new(GuardedResolver {
                net_permits: std::sync::Arc::new(config.net.globs().to_vec()),
            }));
        }
        let inner = builder.build().map_err(|e| HttpError::Other {
            reason: format!("failed to build HTTP client: {e}"),
        })?;
        Ok(Self { inner, config })
    }

    /// Vet one absolute URL: parse + static SSRF, then the declared
    /// `permits.net.http` boundary, then the DNS-resolve guard. The ORDER is
    /// load-bearing — the boundary gates BEFORE any DNS so a host outside it
    /// is refused with zero network activity (see below).
    async fn vet(&self, raw: &str) -> Result<url::Url, HttpError> {
        // 1. Parse + STATIC SSRF (scheme · literal-IP ranges · blocked
        //    hostnames · NO DNS yet) so the permits check reads the SAME host
        //    the transport connects to (the bypass class is a string parser
        //    disagreeing with the url crate · see `check_net_allowlist`).
        let parsed = match self.config.ssrf {
            SsrfMode::Disabled => url::Url::parse(raw).map_err(|e| HttpError::Other {
                reason: format!("invalid URL: {e}"),
            })?,
            // The declared permits ride into the static floor check: an
            // exact loopback literal among them declassifies its host (#395).
            SsrfMode::Enforce => ssrf::check_url(raw, self.config.net.globs())?,
        };
        // 2. The workflow's DECLARED permits.net.http boundary (NIKA-SEC-004),
        //    on EVERY hop (this fn runs once per redirect), holding even under
        //    SsrfMode::Disabled. It gates BEFORE the DNS-resolve guard below:
        //    a host outside the boundary is refused with NO network activity
        //    AT ALL — not even a DNS lookup (the engine must not resolve, nor
        //    leak the resolvability of, a host the workflow may not reach; and
        //    the refusal is the precise NIKA-SEC-004, never a DNS error). This
        //    is the runtime half spec §permits demands — it catches the
        //    dynamic hosts (and redirect bounces) `nika check` cannot see.
        check_net_allowlist(&self.config.net, &parsed)?;
        // 3. DNS-resolve guard (Enforce only) — every resolved address
        //    range-checked, AFTER the host cleared the declared boundary.
        if self.config.ssrf == SsrfMode::Enforce {
            resolve_guard(&parsed, self.config.net.globs()).await?;
        }
        Ok(parsed)
    }

    /// Drive the request including the manual redirect loop. Every hop
    /// is re-vetted; the final `reqwest::Response` is returned together
    /// with the URL string that produced it.
    ///
    /// `total_deadline`: reqwest's per-request timeout spans "until the
    /// body has FINISHED" — correct for the buffered get/post paths,
    /// fatal for streaming (an SSE generation routinely outlives any
    /// total budget). Buffered callers pass `Some`; `send_streaming`
    /// passes the request's EXPLICIT timeout only (else `None`) and
    /// relies on the client-level idle-read guard.
    async fn execute(
        &self,
        request: HttpRequest,
        total_deadline: Option<Duration>,
    ) -> Result<(reqwest::Response, String), HttpError> {
        // The duration used in Timeout error MESSAGES (the idle guard
        // still bounds a deadline-less stream).
        let reported_timeout = total_deadline.unwrap_or(self.config.timeout);
        let mut method = to_reqwest_method(request.method)?;
        // Move the owned fields out (field moves are legal on a foreign
        // `#[non_exhaustive]` struct — full destructuring is not); the
        // redirect loop mutates headers/body in place. Zero clones.
        let mut body = request.body;
        let mut headers = request.headers;
        let mut url = request.url;

        // First attempt + up to max_redirects follow-ups.
        for _hop in 0..=u32::from(self.config.max_redirects) {
            let vetted = self.vet(&url).await?;

            let mut builder = self.inner.request(method.clone(), vetted.as_str());
            if let Some(deadline) = total_deadline {
                builder = builder.timeout(deadline);
            }
            for (key, value) in &headers {
                builder = builder.header(key.as_str(), value.as_str());
            }
            if let Some(bytes) = &body {
                builder = builder.body(bytes.clone());
            }

            let response = builder
                .send()
                .await
                .map_err(|e| map_send_error(&e, reported_timeout))?;

            let status = response.status();
            // Only the FOLLOWABLE 3xx codes drive the loop. 300 Multiple
            // Choices · 304 Not Modified · 305/306 carry no Location and
            // are returned to the caller verbatim (304 in particular is a
            // normal conditional-GET answer, not an error).
            let followable = matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308);
            if followable && request.follow_redirects {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or_else(|| HttpError::Other {
                        reason: format!("redirect {status} without a Location header"),
                    })?;
                let next = vetted.join(location).map_err(|e| HttpError::Other {
                    reason: format!("invalid redirect Location {location:?}: {e}"),
                })?;
                // Cross-origin hop: strip credential-bearing headers so a
                // public host can not bounce an Authorization/Cookie to a
                // DIFFERENT host (the SSRF layer only blocks PRIVATE
                // targets — public→public credential leak is its own
                // class). Matches curl / reqwest redirect-policy behaviour.
                if !same_origin(&vetted, &next) {
                    strip_sensitive_headers(&mut headers);
                }
                // Method/body demotion (RFC 9110 §15.4): 303 → GET for
                // every method EXCEPT HEAD (which stays HEAD); 301/302
                // demote POST → GET (de-facto standard); 307/308 preserve.
                let demote = (status == reqwest::StatusCode::SEE_OTHER
                    && method != reqwest::Method::HEAD)
                    || ((status == reqwest::StatusCode::MOVED_PERMANENTLY
                        || status == reqwest::StatusCode::FOUND)
                        && method == reqwest::Method::POST);
                if demote {
                    method = reqwest::Method::GET;
                    body = None;
                    // The body is gone — its describing headers must not
                    // ride along on the now-bodyless GET.
                    strip_body_headers(&mut headers);
                }
                url = next.to_string();
                continue;
            }

            return Ok((response, url));
        }

        Err(HttpError::Other {
            reason: format!("too many redirects (max {})", self.config.max_redirects),
        })
    }

    /// Read a full response under the size cap.
    async fn read_capped(
        &self,
        response: reqwest::Response,
        final_url: String,
    ) -> Result<HttpResponse, HttpError> {
        let max = self.config.max_response_bytes;
        if let Some(len) = response.content_length()
            && len > max
        {
            return Err(HttpError::TooLarge { size: len, max });
        }

        let status = response.status().as_u16();
        let headers = headers_to_btreemap(response.headers());

        // Pre-size from Content-Length, BOUNDED: the header is
        // attacker-influenced and already ≤ max here, but a lying value
        // must not buy a 64 MiB allocation up front — growth past the
        // floor is amortized doubling as usual.
        let hint = response
            .content_length()
            .unwrap_or(0)
            .min(max)
            .min(PREALLOC_FLOOR);
        let mut collected: Vec<u8> = Vec::with_capacity(usize::try_from(hint).unwrap_or(0));
        let mut response = response;
        while let Some(chunk) = response.chunk().await.map_err(|e| HttpError::Other {
            reason: format!("failed to read response body: {e}"),
        })? {
            let next_len = collected.len() as u64 + chunk.len() as u64;
            if next_len > max {
                return Err(HttpError::TooLarge {
                    size: next_len,
                    max,
                });
            }
            collected.extend_from_slice(&chunk);
        }

        Ok(HttpResponse::new(
            status,
            headers,
            Bytes::from(collected),
            final_url,
        ))
    }
}

impl HttpGetDyn for ReqwestHttp {
    /// Send a GET request (manual redirect loop · SSRF per hop · size cap).
    ///
    /// CANCEL SAFETY: cancel-safe — GET is idempotent by HTTP spec;
    /// dropping the future releases the connection and discards unread
    /// bytes.
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let deadline = request.timeout.unwrap_or(self.config.timeout);
        let (response, final_url) = self.execute(request, Some(deadline)).await?;
        self.read_capped(response, final_url).await
    }
}

impl HttpPostDyn for ReqwestHttp {
    /// Send a POST request (manual redirect loop · SSRF per hop · size cap).
    ///
    /// CANCEL SAFETY: NOT cancel-safe at the application layer — a
    /// dropped future may already have delivered the body to the remote
    /// (mutation committed) while the response read was cancelled.
    /// Callers on non-idempotent paths pair POST with an idempotency
    /// key or accept that retry-on-cancel may double-commit (kernel
    /// contract verbatim).
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let deadline = request.timeout.unwrap_or(self.config.timeout);
        let (response, final_url) = self.execute(request, Some(deadline)).await?;
        self.read_capped(response, final_url).await
    }

    /// Send a request and receive a streaming response. The body stream
    /// is wrapped in a byte counter that yields
    /// [`HttpError::TooLarge`] mid-stream past the cap.
    ///
    /// TIMEOUT SEMANTICS: the config's total deadline does NOT apply —
    /// reqwest's per-request timeout spans the whole body, and a
    /// long-lived stream (an LLM SSE generation) routinely outlives any
    /// total budget. An EXPLICIT `request.timeout` is honored as a
    /// total deadline when the caller wants one; otherwise the
    /// client-level idle-read guard (`read_timeout` = config.timeout)
    /// kills a STALLED stream without capping an active one.
    ///
    /// CANCEL SAFETY: cancel-safe from the reader side (drop the stream
    /// and the connection is released). Request-side safety follows
    /// `post` rules.
    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        let max = self.config.max_response_bytes;
        let timeout = request.timeout.unwrap_or(self.config.timeout);
        let explicit_deadline = request.timeout;
        let (response, final_url) = self.execute(request, explicit_deadline).await?;

        let content_length = response.content_length();
        if let Some(len) = content_length
            && len > max
        {
            return Err(HttpError::TooLarge { size: len, max });
        }

        let status = response.status().as_u16();
        let headers = headers_to_btreemap(response.headers());
        let body = CappedStream {
            inner: Box::pin(response.bytes_stream()),
            seen: 0,
            max,
            timeout,
            done: false,
        };

        Ok(HttpStreamResponse::new(
            status,
            headers,
            final_url,
            content_length,
            Box::pin(body),
        ))
    }
}

// The credential-header list is the KERNEL's — one list, shared with the
// Debug-redaction duty, because both answer « does this header's value
// carry a credential? ».
//
// This crate used to keep its own, mirroring reqwest's set, and the two
// diverged: reqwest has never heard of `x-api-key`, which is exactly
// what our Anthropic wire sends and what `nika:fetch` documents to
// workflow authors. A cross-origin 302 forwarded the live key.
use nika_kernel::io::http::is_credential_header;

/// Body-describing headers dropped when a redirect demotes a
/// body-bearing method to a bodyless GET — they must not ride along on
/// the now-empty request. Comparison is case-insensitive.
const BODY_HEADERS: &[&str] = &["content-type", "content-length", "transfer-encoding"];

/// Drop every credential-bearing header in place (case-insensitive),
/// per the kernel's one list.
fn strip_sensitive_headers(headers: &mut std::collections::BTreeMap<String, String>) {
    headers.retain(|key, _| !is_credential_header(key));
}

/// Remove every [`BODY_HEADERS`] entry (case-insensitive) in place.
fn strip_body_headers(headers: &mut std::collections::BTreeMap<String, String>) {
    retain_without(headers, BODY_HEADERS);
}

/// Drop every header whose lowercased name is in `drop_list`.
fn retain_without(headers: &mut std::collections::BTreeMap<String, String>, drop_list: &[&str]) {
    headers.retain(|key, _| !drop_list.contains(&key.to_ascii_lowercase().as_str()));
}

/// Two URLs share an origin when scheme, host, and effective port all
/// match (RFC 6454). Used to decide credential stripping on redirect.
fn same_origin(a: &url::Url, b: &url::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Bound on the SSRF DNS resolution per hop — a slow/hostile resolver
/// must not hang the whole operation past the request budget.
const DNS_RESOLVE_TIMEOUT: Duration = Duration::from_secs(5);

/// Verdict over a set of resolved addresses (extracted pure so the SSRF
/// range logic is unit-testable without a live resolver).
enum ResolveVerdict {
    /// At least one address, none private — safe to connect.
    AllPublic,
    /// A resolved address fell in a blocked range.
    Private,
    /// DNS returned zero addresses.
    Empty,
}

/// Classify resolved addresses: `Private` if ANY is in a blocked range,
/// else `AllPublic` (≥1) or `Empty` (none). Pure — the testable core of
/// the DNS-resolve SSRF layer.
///
/// `allow_loopback` is the #395 declassification verdict for the NAME
/// being resolved (its host is a permitted exact loopback literal): it
/// admits the LOOPBACK CLASS ONLY — a permitted `localhost` must reach
/// its resolved `127.0.0.1`/`::1`, while a rebind of the same name to
/// RFC1918/metadata still refuses.
fn classify_resolved(
    addrs: impl IntoIterator<Item = std::net::SocketAddr>,
    allow_loopback: bool,
) -> ResolveVerdict {
    let mut any = false;
    for addr in addrs {
        any = true;
        let ip = addr.ip();
        if ssrf::ip_is_blocked(ip) && !(allow_loopback && ip.is_loopback()) {
            return ResolveVerdict::Private;
        }
    }
    if any {
        ResolveVerdict::AllPublic
    } else {
        ResolveVerdict::Empty
    }
}

/// The reqwest DNS resolver that enforces the SSRF range check INSIDE
/// the connect path (crate docs · layer 3). Every address handed to the
/// transport went through [`classify_resolved`] in the same lookup —
/// a rebind between the advisory check and the connection has nowhere
/// to land. Resolution itself is `tokio::net::lookup_host` (the system
/// resolver, same as reqwest's default `GaiResolver`) under
/// [`DNS_RESOLVE_TIMEOUT`].
#[derive(Debug, Clone, Default)]
struct GuardedResolver {
    /// The workflow's declared `permits.net.http` entries — read ONLY for
    /// the exact-loopback declassification (#395): loopback addresses are
    /// admitted iff the NAME being resolved is itself a permitted exact
    /// loopback literal (`loopback_declassified` · per-name, so a redirect
    /// hop or rebound public name gets no clearance from someone else's
    /// grant). Empty = no boundary = today's strict floor.
    net_permits: std::sync::Arc<Vec<String>>,
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        let allow_loopback = nika_types::net::loopback_declassified(&self.net_permits, &host);
        Box::pin(async move {
            // Port 0 — reqwest substitutes the URL's effective port
            // (`dns_resolver` contract · verified docs.rs 2026-06-12).
            match guarded_lookup(&host, 0, allow_loopback).await {
                Ok(addrs) => Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs),
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>), // box-dyn-ok(vendor-seam): reqwest::dns::Resolving's error slot REQUIRES this exact type — the typed HttpError rides inside and is recovered by find_http_error
            }
        })
    }
}

/// One guarded DNS resolution — system lookup under
/// [`DNS_RESOLVE_TIMEOUT`], EVERY returned address range-checked
/// (loopback admitted only under the caller's `allow_loopback`
/// declassification verdict for THIS host · #395). Shared by the
/// advisory layer ([`resolve_guard`]) and the enforcement layer
/// ([`GuardedResolver`]): one logic, two seams (review lens 2 ·
/// P3-1 dedupe).
async fn guarded_lookup(
    host: &str,
    port: u16,
    allow_loopback: bool,
) -> Result<Vec<std::net::SocketAddr>, HttpError> {
    if nika_types::net::is_documentation_host(host) {
        return Err(HttpError::Connection {
            reason: format!("documentation host `{host}` is not dialed (RFC 2606/6761)"),
        });
    }
    let lookup = tokio::time::timeout(DNS_RESOLVE_TIMEOUT, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| HttpError::Timeout {
            duration_ms: u64::try_from(DNS_RESOLVE_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
        })?
        .map_err(|e| HttpError::Connection {
            reason: format!("DNS resolution failed for {host}: {e}"),
        })?;
    let addrs: Vec<std::net::SocketAddr> = lookup.collect();
    match classify_resolved(addrs.iter().copied(), allow_loopback) {
        ResolveVerdict::AllPublic => Ok(addrs),
        ResolveVerdict::Private => Err(HttpError::SsrfBlocked {
            url: host.to_owned(),
        }),
        ResolveVerdict::Empty => Err(HttpError::Connection {
            reason: format!("DNS returned no addresses for {host}"),
        }),
    }
}

/// Resolve a non-literal host and range-check every returned address.
/// Literal-IP hosts were already vetted by the static layer.
///
/// This is the ADVISORY layer (crate docs · layer 2): it fail-fasts
/// with a typed error before any connection attempt. The ENFORCEMENT
/// is [`GuardedResolver`] inside the connect path. `net_permits` is the
/// declared `permits.net.http` — both layers derive the SAME per-name
/// loopback-declassification verdict from it (#395).
async fn resolve_guard(parsed: &url::Url, net_permits: &[String]) -> Result<(), HttpError> {
    let Some(host) = parsed.host_str() else {
        return Err(HttpError::Other {
            reason: format!("URL has no host: {parsed}"),
        });
    };
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if bare.parse::<std::net::IpAddr>().is_ok() {
        return Ok(()); // literal — the static layer is authoritative
    }

    let port = parsed.port_or_known_default().unwrap_or(80);
    let allow_loopback = nika_types::net::loopback_declassified(net_permits, bare);
    // A Private verdict re-labels with the FULL url (the advisory layer
    // has it; the resolver only sees the bare host).
    match guarded_lookup(bare, port, allow_loopback).await {
        Ok(_) => Ok(()),
        Err(HttpError::SsrfBlocked { .. }) => Err(HttpError::SsrfBlocked {
            url: parsed.to_string(),
        }),
        Err(other) => Err(other),
    }
}

/// Map the kernel method enum onto reqwest's. The kernel enum is
/// `#[non_exhaustive]`: unknown future variants surface as
/// [`HttpError::Unsupported`] instead of silently degrading.
fn to_reqwest_method(method: HttpMethod) -> Result<reqwest::Method, HttpError> {
    match method {
        HttpMethod::Get => Ok(reqwest::Method::GET),
        HttpMethod::Post => Ok(reqwest::Method::POST),
        HttpMethod::Put => Ok(reqwest::Method::PUT),
        HttpMethod::Patch => Ok(reqwest::Method::PATCH),
        HttpMethod::Delete => Ok(reqwest::Method::DELETE),
        HttpMethod::Head => Ok(reqwest::Method::HEAD),
        HttpMethod::Options => Ok(reqwest::Method::OPTIONS),
        other => Err(HttpError::Unsupported {
            reason: format!("HTTP method {other} not supported by this client"),
        }),
    }
}

/// Map reqwest send errors onto the kernel error contract.
///
/// A [`GuardedResolver`] rejection arrives WRAPPED in reqwest's connect
/// error — dig the source chain first so an SSRF block keeps its typed
/// identity instead of collapsing into `Connection`.
fn map_send_error(error: &reqwest::Error, timeout: Duration) -> HttpError {
    if let Some(guard) = find_http_error(error) {
        return guard;
    }
    if error.is_timeout() {
        HttpError::Timeout {
            duration_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        }
    } else if error.is_connect() {
        HttpError::Connection {
            reason: error.to_string(),
        }
    } else {
        HttpError::Other {
            reason: error.to_string(),
        }
    }
}

/// Walk an error's source chain looking for a kernel [`HttpError`]
/// (the [`GuardedResolver`] emits one inside reqwest's wrapping).
/// `HttpError` is not `Clone` — the found variant is reconstructed
/// field-by-field. Best-effort: if reqwest's internal wrapping ever
/// stops exposing the chain, the caller's generic mapping still
/// applies (fail-closed either way — only the error TYPE degrades).
fn find_http_error(error: &(dyn std::error::Error + 'static)) -> Option<HttpError> {
    let mut source = error.source();
    while let Some(err) = source {
        if let Some(http) = err.downcast_ref::<HttpError>() {
            return Some(match http {
                HttpError::SsrfBlocked { url } => HttpError::SsrfBlocked { url: url.clone() },
                HttpError::Timeout { duration_ms } => HttpError::Timeout {
                    duration_ms: *duration_ms,
                },
                HttpError::Connection { reason } => HttpError::Connection {
                    reason: reason.clone(),
                },
                HttpError::TooLarge { size, max } => HttpError::TooLarge {
                    size: *size,
                    max: *max,
                },
                HttpError::Unsupported { reason } => HttpError::Unsupported {
                    reason: reason.clone(),
                },
                // WARN: deliberately LOSSY — a future HttpError variant
                // must gain an arm here to keep its typed identity
                // through the resolver seam (fail-closed either way,
                // only the error TYPE degrades to Other).
                other => HttpError::Other {
                    reason: other.to_string(),
                },
            });
        }
        source = err.source();
    }
    None
}

/// Lower response headers into the kernel's `BTreeMap<String, String>`
/// (header names are already lowercase per the `http` crate; non-UTF-8
/// values are skipped — brouillon parity).
///
/// Repeated field lines COMMA-JOIN per RFC 9110 §5.2 (semantically
/// identical to the combined form, and what the `Link:` consumer
/// already parses) — last-wins silently DROPPED earlier lines before
/// (an i18n site emitting one `Link: …; rel=alternate` per locale lost
/// all but the last). `set-cookie` is THE standard exception: its
/// `Expires` dates carry commas, so joining corrupts — last wins there.
fn headers_to_btreemap(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in headers {
        let Ok(v) = value.to_str() else { continue };
        let name = key.as_str();
        match map.entry(name.to_string()) {
            std::collections::btree_map::Entry::Occupied(mut slot) => {
                if name == "set-cookie" {
                    *slot.get_mut() = v.to_string();
                } else {
                    let joined = slot.get_mut();
                    joined.push_str(", ");
                    joined.push_str(v);
                }
            }
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(v.to_string());
            }
        }
    }
    map
}

/// Byte-counting wrapper over the reqwest body stream: yields
/// [`HttpError::TooLarge`] once the cumulative size passes the cap,
/// then fuses (no further items).
struct CappedStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    seen: u64,
    max: u64,
    /// The request timeout, so a mid-stream reqwest error keeps its
    /// Timeout/Connection identity instead of collapsing to `Other`.
    timeout: Duration,
    done: bool,
}

impl Stream for CappedStream {
    type Item = Result<Bytes, HttpError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        match self.inner.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                self.done = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(e))) => {
                self.done = true;
                let timeout = self.timeout;
                Poll::Ready(Some(Err(map_send_error(&e, timeout))))
            }
            Poll::Ready(Some(Ok(chunk))) => {
                // Saturating: matches read_capped's defensive arithmetic.
                self.seen = self.seen.saturating_add(chunk.len() as u64);
                if self.seen > self.max {
                    self.done = true;
                    return Poll::Ready(Some(Err(HttpError::TooLarge {
                        size: self.seen,
                        max: self.max,
                    })));
                }
                Poll::Ready(Some(Ok(chunk)))
            }
        }
    }
}

#[cfg(test)]
mod tests;
