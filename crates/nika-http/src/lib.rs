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
    Declared(Vec<String>),
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
    if nika_types::net::host_in_allowlist(globs, &host) {
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
            // Layer 3: range-check INSIDE the connect path.
            builder = builder.dns_resolver(std::sync::Arc::new(GuardedResolver));
        }
        let inner = builder.build().map_err(|e| HttpError::Other {
            reason: format!("failed to build HTTP client: {e}"),
        })?;
        Ok(Self { inner, config })
    }

    /// Vet one absolute URL per the configured [`SsrfMode`]: static
    /// checks, then DNS-resolve every address for non-literal hosts.
    async fn vet(&self, raw: &str) -> Result<url::Url, HttpError> {
        // Parse + SSRF-vet FIRST so the permits check reads the SAME host
        // the transport will connect to (the bypass class is a string
        // parser disagreeing with the url crate · see `check_net_allowlist`).
        let parsed = match self.config.ssrf {
            SsrfMode::Disabled => url::Url::parse(raw).map_err(|e| HttpError::Other {
                reason: format!("invalid URL: {e}"),
            })?,
            SsrfMode::Enforce => {
                let parsed = ssrf::check_url(raw)?;
                resolve_guard(&parsed).await?;
                parsed
            }
        };
        // The workflow's DECLARED permits.net.http boundary (NIKA-SEC-004),
        // checked on EVERY hop (this fn runs once per redirect) and
        // independent of the SSRF floor: a declared boundary holds even
        // under SsrfMode::Disabled. This is the runtime half spec §permits
        // demands — it catches the dynamic hosts (and redirect bounces) the
        // static `nika check` cannot see.
        check_net_allowlist(&self.config.net, &parsed)?;
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

/// Request headers dropped when a redirect crosses to a different
/// origin — they carry caller credentials that must not leak to a host
/// the caller did not address. Mirrors reqwest's own
/// `remove_sensitive_headers` set. Comparison is case-insensitive.
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "cookie2",
    "proxy-authorization",
    "www-authenticate",
];

/// Body-describing headers dropped when a redirect demotes a
/// body-bearing method to a bodyless GET — they must not ride along on
/// the now-empty request. Comparison is case-insensitive.
const BODY_HEADERS: &[&str] = &["content-type", "content-length", "transfer-encoding"];

/// Remove every [`SENSITIVE_HEADERS`] entry (case-insensitive) in place.
fn strip_sensitive_headers(headers: &mut std::collections::BTreeMap<String, String>) {
    retain_without(headers, SENSITIVE_HEADERS);
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
fn classify_resolved(addrs: impl IntoIterator<Item = std::net::SocketAddr>) -> ResolveVerdict {
    let mut any = false;
    for addr in addrs {
        any = true;
        if ssrf::ip_is_blocked(addr.ip()) {
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
#[derive(Debug, Clone, Copy, Default)]
struct GuardedResolver;

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            // Port 0 — reqwest substitutes the URL's effective port
            // (`dns_resolver` contract · verified docs.rs 2026-06-12).
            match guarded_lookup(&host, 0).await {
                Ok(addrs) => Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs),
                Err(e) => Err(Box::new(e) as Box<dyn std::error::Error + Send + Sync>), // box-dyn-ok(vendor-seam): reqwest::dns::Resolving's error slot REQUIRES this exact type — the typed HttpError rides inside and is recovered by find_http_error
            }
        })
    }
}

/// One guarded DNS resolution — system lookup under
/// [`DNS_RESOLVE_TIMEOUT`], EVERY returned address range-checked.
/// Shared by the advisory layer ([`resolve_guard`]) and the
/// enforcement layer ([`GuardedResolver`]): one logic, two seams
/// (review lens 2 · P3-1 dedupe).
async fn guarded_lookup(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>, HttpError> {
    let lookup = tokio::time::timeout(DNS_RESOLVE_TIMEOUT, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| HttpError::Timeout {
            duration_ms: u64::try_from(DNS_RESOLVE_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
        })?
        .map_err(|e| HttpError::Connection {
            reason: format!("DNS resolution failed for {host}: {e}"),
        })?;
    let addrs: Vec<std::net::SocketAddr> = lookup.collect();
    match classify_resolved(addrs.iter().copied()) {
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
/// is [`GuardedResolver`] inside the connect path.
async fn resolve_guard(parsed: &url::Url) -> Result<(), HttpError> {
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
    // A Private verdict re-labels with the FULL url (the advisory layer
    // has it; the resolver only sees the bare host).
    match guarded_lookup(bare, port).await {
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
mod tests {
    use super::*;

    // Public-type assertions live in `tests/http_contract.rs` — here
    // only crate-private invariants.

    #[test]
    fn config_default_is_enforce() {
        assert_eq!(HttpConfig::default().ssrf, SsrfMode::Enforce);
    }

    #[test]
    fn net_default_is_unbounded() {
        // No declared boundary by default — today's behavior (the SSRF
        // floor is the only net guard until a `permits.net.http` is wired).
        assert_eq!(HttpConfig::default().net, NetBoundary::Unbounded);
    }

    #[test]
    fn net_boundary_gates_on_the_parsed_connect_host() {
        // The RUNTIME half of permits.net.http (NIKA-SEC-004) — it gates on
        // the PARSED url::Url host (what reqwest connects to), so a string-
        // parser-vs-url-crate disagreement cannot bypass it.
        let allow =
            NetBoundary::Declared(vec!["allowed.com".to_owned(), "*.github.com".to_owned()]);
        let u = |s: &str| url::Url::parse(s).expect("valid url");
        // in-bounds (exact + `*.` subdomain)
        assert!(check_net_allowlist(&allow, &u("https://allowed.com/p")).is_ok());
        assert!(check_net_allowlist(&allow, &u("https://api.github.com/p")).is_ok());
        // out-of-bounds → denied
        assert!(matches!(
            check_net_allowlist(&allow, &u("https://evil.com/p")),
            Err(HttpError::HostNotAllowed { host }) if host == "evil.com"
        ));
        // THE BYPASS (rust-security P0): `\@` — a string parser reads host
        // "allowed.com", but the connect host is "evil.com" (the `\` is a
        // WHATWG path separator for http/https). Gating on url::Url denies it.
        assert!(
            matches!(
                check_net_allowlist(&allow, &u(r"https://evil.com\@allowed.com/p")),
                Err(HttpError::HostNotAllowed { host }) if host == "evil.com"
            ),
            "the backslash-userinfo confusion must resolve to the real connect host"
        );
        // userinfo `@` — the real host is evil.com (the classic confusion).
        assert!(check_net_allowlist(&allow, &u("https://allowed.com@evil.com/p")).is_err());
        // case-insensitive (the url crate lowercases) — no false block.
        assert!(check_net_allowlist(&allow, &u("https://ALLOWED.COM/p")).is_ok());
        // IPv6 matched bracket-free (permits write `::1`, never `[::1]`).
        let v6 = NetBoundary::Declared(vec!["::1".to_owned()]);
        assert!(check_net_allowlist(&v6, &u("http://[::1]:8080/p")).is_ok());
        // trailing-dot FQDN (`allowed.com.`) ≡ `allowed.com` — no false block.
        assert!(check_net_allowlist(&allow, &u("https://allowed.com./p")).is_ok());
        // Unbounded = no declared boundary → the SSRF floor guards.
        assert!(
            check_net_allowlist(&NetBoundary::Unbounded, &u("https://anywhere.example/p")).is_ok()
        );
        // An empty Declared list (a `permits:` block that omits `net`) denies all.
        assert!(
            check_net_allowlist(
                &NetBoundary::Declared(Vec::new()),
                &u("https://allowed.com/p")
            )
            .is_err()
        );
    }

    #[test]
    fn host_of_matches_the_shared_parity_vectors() {
        // The runtime extractor (`host_of`) MUST agree with the static
        // checker (`nika-schema`'s `url_host`) on every shared vector — the
        // no-drift guarantee. nika-schema asserts the SAME table against its
        // extractor; if either drifts (`\@`, case, IPv6, trailing dot), one
        // of the two suites fails. Single source of cases · zero drift.
        for (input, expected) in nika_types::net::HOST_EXTRACTION_VECTORS {
            let got = url::Url::parse(input).ok().and_then(|u| host_of(&u));
            assert_eq!(got.as_deref(), *expected, "host_of disagrees on {input}");
        }
    }

    #[test]
    fn repeated_headers_comma_join_except_set_cookie() {
        // RFC 9110 §5.2: N field lines ≡ one comma-joined value — the
        // i18n shape (one Link per locale) must survive the lowering.
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append(
            "link",
            "<https://x.test/fr>; rel=\"alternate\"; hreflang=\"fr\""
                .parse()
                .expect("valid"),
        );
        headers.append(
            "link",
            "<https://x.test/de>; rel=\"alternate\"; hreflang=\"de\""
                .parse()
                .expect("valid"),
        );
        // set-cookie: comma-UNSAFE (Expires dates) → last wins.
        headers.append(
            "set-cookie",
            "a=1; Expires=Wed, 01 Jan 2025".parse().expect("valid"),
        );
        headers.append("set-cookie", "b=2".parse().expect("valid"));
        // Non-UTF-8 values are skipped, not joined.
        headers.append(
            "x-binary",
            reqwest::header::HeaderValue::from_bytes(&[0xff, 0xfe]).expect("bytes"),
        );

        let map = headers_to_btreemap(&headers);
        assert_eq!(
            map.get("link").map(String::as_str),
            Some(
                "<https://x.test/fr>; rel=\"alternate\"; hreflang=\"fr\", \
                 <https://x.test/de>; rel=\"alternate\"; hreflang=\"de\""
            ),
            "both locales survive, comma-joined"
        );
        assert_eq!(map.get("set-cookie").map(String::as_str), Some("b=2"));
        assert!(!map.contains_key("x-binary"), "non-UTF-8 skipped");
    }

    #[test]
    fn method_mapping_covers_all_current_variants() {
        for (kernel, expect) in [
            (HttpMethod::Get, reqwest::Method::GET),
            (HttpMethod::Post, reqwest::Method::POST),
            (HttpMethod::Put, reqwest::Method::PUT),
            (HttpMethod::Patch, reqwest::Method::PATCH),
            (HttpMethod::Delete, reqwest::Method::DELETE),
            (HttpMethod::Head, reqwest::Method::HEAD),
            (HttpMethod::Options, reqwest::Method::OPTIONS),
        ] {
            assert_eq!(to_reqwest_method(kernel).expect("supported"), expect);
        }
    }

    #[test]
    fn timeout_mapping_saturates_huge_durations() {
        let err = map_send_error_probe(Duration::from_secs(u64::MAX / 1000));
        assert!(
            matches!(err, HttpError::Timeout { duration_ms } if duration_ms > 0),
            "expected a positive Timeout, got {err:?}"
        );
    }

    #[test]
    fn classify_resolved_flags_any_private_address() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let mixed = vec![
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 80),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 80),
        ];
        assert!(matches!(classify_resolved(mixed), ResolveVerdict::Private));
    }

    #[test]
    fn classify_resolved_all_public_passes() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let public = vec![
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 443),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443),
        ];
        assert!(matches!(
            classify_resolved(public),
            ResolveVerdict::AllPublic
        ));
    }

    #[test]
    fn classify_resolved_empty_is_empty() {
        assert!(matches!(
            classify_resolved(std::iter::empty()),
            ResolveVerdict::Empty
        ));
    }

    #[test]
    fn classify_resolved_blocks_rebind_to_metadata_ip() {
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        // The cloud-metadata link-local address a DNS-rebind would return.
        let rebind = vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            80,
        )];
        assert!(matches!(classify_resolved(rebind), ResolveVerdict::Private));
    }

    #[test]
    fn strip_sensitive_removes_credentials_case_insensitive() {
        let mut h = std::collections::BTreeMap::new();
        h.insert("Authorization".to_string(), "Bearer x".to_string());
        h.insert("COOKIE".to_string(), "s=1".to_string());
        h.insert("X-Keep".to_string(), "ok".to_string());
        strip_sensitive_headers(&mut h);
        assert_eq!(
            h.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["X-Keep"]
        );
    }

    #[test]
    fn strip_body_removes_content_descriptors() {
        let mut h = std::collections::BTreeMap::new();
        h.insert("Content-Type".to_string(), "application/json".to_string());
        h.insert("X-Keep".to_string(), "ok".to_string());
        strip_body_headers(&mut h);
        assert_eq!(
            h.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["X-Keep"]
        );
    }

    /// Build a timeout-flavoured mapping without a real `reqwest::Error`
    /// (reqwest errors cannot be constructed directly): exercise the
    /// saturation arithmetic the mapper relies on.
    fn map_send_error_probe(timeout: Duration) -> HttpError {
        HttpError::Timeout {
            duration_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        }
    }

    /// A two-level wrapper replicating reqwest's connect-error nesting
    /// (outer error → middle wrapper → the resolver's `HttpError`).
    #[derive(Debug)]
    struct Wrap {
        msg: &'static str,
        source: Box<dyn std::error::Error + Send + Sync>, // box-dyn-ok(test-harness): #[cfg(test)] fixture replicating reqwest's nesting
    }
    impl std::fmt::Display for Wrap {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.msg)
        }
    }
    impl std::error::Error for Wrap {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self.source.as_ref())
        }
    }

    #[test]
    fn find_http_error_digs_a_nested_chain() {
        let chain = Wrap {
            msg: "client error (Connect)",
            source: Box::new(Wrap {
                msg: "dns error",
                source: Box::new(HttpError::SsrfBlocked {
                    url: "internal.corp".to_owned(),
                }),
            }),
        };
        let found = find_http_error(&chain).expect("digs through two levels");
        assert!(
            matches!(found, HttpError::SsrfBlocked { url } if url == "internal.corp"),
            "typed identity preserved"
        );
    }

    #[test]
    fn find_http_error_none_on_foreign_chain() {
        let chain = Wrap {
            msg: "outer",
            source: Box::new(std::io::Error::other("plain io")),
        };
        assert!(find_http_error(&chain).is_none());
    }

    #[tokio::test]
    async fn guarded_resolver_blocks_localhost_and_serves_public() {
        use reqwest::dns::Resolve;
        use std::str::FromStr;

        // `localhost` resolves to loopback on every OS — the resolver
        // must refuse to hand it to the transport. (`Addrs` is not
        // Debug — go through `.err()` instead of `expect_err`.)
        let name = reqwest::dns::Name::from_str("localhost").expect("valid name");
        let err = GuardedResolver
            .resolve(name)
            .await
            .err()
            .expect("localhost must be blocked by the guarded resolver");
        let http = err
            .downcast_ref::<HttpError>()
            .expect("the resolver emits the kernel error type");
        assert!(matches!(http, HttpError::SsrfBlocked { .. }), "{http}");
    }

    #[tokio::test]
    async fn enforce_mode_blocks_loopback_end_to_end() {
        // Through the full client: a literal loopback URL dies in the
        // STATIC layer; `localhost` dies static too (hostname list). The
        // resolver layer is exercised directly above — here we pin that
        // the composed client refuses both without any network.
        let client = ReqwestHttp::new().expect("client builds");
        for url in ["http://127.0.0.1:9/x", "http://localhost:9/x"] {
            let err = client
                .get(HttpRequest::get(url))
                .await
                .expect_err("must be blocked before any connection");
            assert!(matches!(err, HttpError::SsrfBlocked { .. }), "{url}: {err}");
        }
    }

    #[tokio::test]
    async fn disabled_mode_still_builds_a_working_client() {
        // SsrfMode::Disabled wires the DEFAULT resolver — the build path
        // must stay valid (connection refused ≠ build failure).
        let mut config = HttpConfig::new();
        config.ssrf = SsrfMode::Disabled;
        let client = ReqwestHttp::with_config(config).expect("builds without the guard");
        // Port 9 (discard) on loopback: NOT SsrfBlocked in Disabled mode —
        // the failure (if any) is a plain connection error.
        let res = client.get(HttpRequest::get("http://127.0.0.1:9/x")).await;
        if let Err(e) = res {
            assert!(
                !matches!(e, HttpError::SsrfBlocked { .. }),
                "Disabled mode must not SSRF-block: {e}"
            );
        }
    }
}

/// End-to-end over REAL sockets: a minimal in-process HTTP/1.1
/// responder (tokio `TcpListener` · zero extra deps) exercises the
/// production client's transport mechanics — redirect loop ·
/// cross-origin credential stripping · 303 demotion · size caps ·
/// timeouts · streaming. `SsrfMode::Disabled` everywhere here: the
/// targets ARE loopback (the SSRF layers have their own tests above).
#[cfg(test)]
mod e2e {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn client(max_bytes: u64) -> ReqwestHttp {
        let mut config = HttpConfig::new();
        config.ssrf = SsrfMode::Disabled;
        config.max_response_bytes = max_bytes;
        ReqwestHttp::with_config(config).expect("client builds")
    }

    /// Serve `responses` in order on a fresh loopback listener; each
    /// connection gets the REQUEST HEAD echoed nowhere — the canned
    /// response verbatim. Returns the bound address.
    async fn serve(responses: Vec<String>) -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            for response in responses {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = [0u8; 8192];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        addr
    }

    /// Serve one connection that ECHOES the received request head as
    /// the response body (the probe for what actually went on the wire).
    async fn serve_echo() -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]).into_owned();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{head}",
                head.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        });
        addr
    }

    fn ok_response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn redirect_chain_lands_with_final_url() {
        let dest = serve(vec![ok_response("arrived")]).await;
        let hop = serve(vec![format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{dest}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )])
        .await;

        let resp = client(1 << 20)
            .get(HttpRequest::get(format!("http://{hop}/start")))
            .await
            .expect("redirect followed");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"arrived");
        assert_eq!(resp.final_url, format!("http://{dest}/final"));
    }

    #[tokio::test]
    async fn cross_origin_redirect_strips_credentials_and_303_demotes() {
        // Echo destination shows exactly what crossed the wire.
        let dest = serve_echo().await;
        // 303 See Other from a DIFFERENT origin (different port =
        // different origin on loopback).
        let hop = serve(vec![format!(
            "HTTP/1.1 303 See Other\r\nLocation: http://{dest}/after\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )])
        .await;

        let mut request = HttpRequest::post(format!("http://{hop}/submit"));
        request
            .headers
            .insert("authorization".to_owned(), "Bearer sk-LEAK".to_owned());
        request
            .headers
            .insert("x-trace".to_owned(), "keep-me".to_owned());
        request.body = Some(Bytes::from_static(b"{\"payload\":1}"));

        let resp = client(1 << 20).post(request).await.expect("follows");
        let wire = String::from_utf8_lossy(&resp.body);
        // 303 demotion: POST became GET, body gone.
        assert!(wire.starts_with("GET /after"), "{wire}");
        assert!(!wire.contains("payload"), "body dropped on demote: {wire}");
        // Cross-origin: credentials stripped, neutral headers ride on.
        assert!(!wire.contains("sk-LEAK"), "credential stripped: {wire}");
        assert!(wire.contains("keep-me"), "neutral header kept: {wire}");
    }

    #[tokio::test]
    async fn content_length_over_cap_fails_fast_and_counted_read_caps() {
        // Declared Content-Length above the cap → TooLarge before the body.
        let big = "x".repeat(64);
        let declared = serve(vec![ok_response(&big)]).await;
        let err = client(16)
            .get(HttpRequest::get(format!("http://{declared}/big")))
            .await
            .expect_err("declared too large");
        assert!(matches!(err, HttpError::TooLarge { max: 16, .. }), "{err}");

        // No Content-Length (close-framed) → the counting reader caps.
        let unframed = serve(vec![format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{big}"
        )])
        .await;
        let err = client(16)
            .get(HttpRequest::get(format!("http://{unframed}/big")))
            .await
            .expect_err("counted read too large");
        assert!(matches!(err, HttpError::TooLarge { .. }), "{err}");
    }

    /// gzip of `"hello from the compressed side"` (`mtime=0` — fully
    /// deterministic bytes, no compressor dev-dep).
    const GZIP_BODY: &[u8] = &[
        0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0xcb, 0x48, 0xcd, 0xc9, 0xc9,
        0x57, 0x48, 0x2b, 0xca, 0xcf, 0x55, 0x28, 0xc9, 0x48, 0x55, 0x48, 0xce, 0xcf, 0x2d, 0x28,
        0x4a, 0x2d, 0x2e, 0x4e, 0x4d, 0x51, 0x28, 0xce, 0x4c, 0x49, 0x05, 0x00, 0xcf, 0xe4, 0x53,
        0xe4, 0x1e, 0x00, 0x00, 0x00,
    ];

    #[tokio::test]
    async fn gzip_responses_decompress_transparently() {
        // The wire carries gzip; the caller sees plain text — and the
        // client ADVERTISED the capability (the captured request head
        // carries Accept-Encoding).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        let (head_tx, head_rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap_or(0);
            let _ = head_tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                GZIP_BODY.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(GZIP_BODY).await;
            let _ = socket.shutdown().await;
        });

        let resp = client(1 << 20)
            .get(HttpRequest::get(format!("http://{addr}/gz")))
            .await
            .expect("decompresses");
        assert_eq!(resp.body.as_ref(), b"hello from the compressed side");
        let request_head = head_rx.await.expect("server captured the head");
        assert!(
            request_head
                .to_ascii_lowercase()
                .contains("accept-encoding"),
            "the client advertises compression: {request_head}"
        );
    }

    #[tokio::test]
    async fn per_request_timeout_maps_to_timeout_error() {
        // A listener that accepts and then stalls forever.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            tokio::time::sleep(Duration::from_secs(60)).await;
            drop(socket);
        });

        let mut request = HttpRequest::get(format!("http://{addr}/slow"));
        request.timeout = Some(Duration::from_millis(120));
        let err = client(1 << 20)
            .get(request)
            .await
            .expect_err("must time out");
        assert!(matches!(err, HttpError::Timeout { .. }), "{err}");
    }

    #[tokio::test]
    async fn streaming_delivers_chunks_and_caps_mid_stream() {
        use futures_util_shim::collect_stream;

        // Within cap: chunks arrive and concatenate.
        let body = "streamed-body-content";
        let addr = serve(vec![ok_response(body)]).await;
        let stream_resp = client(1 << 20)
            .send_streaming(HttpRequest::get(format!("http://{addr}/s")))
            .await
            .expect("stream opens");
        assert_eq!(stream_resp.status, 200);
        let collected = collect_stream(stream_resp.body).await.expect("in cap");
        assert_eq!(collected, body.as_bytes());

        // Past cap (close-framed, undeclared length): TooLarge mid-stream.
        let big = "y".repeat(64);
        let addr = serve(vec![format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{big}"
        )])
        .await;
        let stream_resp = client(16)
            .send_streaming(HttpRequest::get(format!("http://{addr}/s")))
            .await
            .expect("stream opens (length unknown)");
        let err = collect_stream(stream_resp.body)
            .await
            .expect_err("caps mid-stream");
        assert!(matches!(err, HttpError::TooLarge { .. }), "{err}");
    }

    /// A tiny hand-rolled stream collector (no futures-util dev-dep):
    /// polls the response stream to completion.
    mod futures_util_shim {
        use super::*;

        pub(super) async fn collect_stream(
            mut stream: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
        ) -> Result<Vec<u8>, HttpError> {
            let mut out = Vec::new();
            loop {
                let next = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await;
                match next {
                    Some(Ok(chunk)) => out.extend_from_slice(&chunk),
                    Some(Err(e)) => return Err(e),
                    None => return Ok(out),
                }
            }
        }
    }
}
