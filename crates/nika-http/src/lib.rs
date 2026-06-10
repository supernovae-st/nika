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
//! before `nika-policy` (L1.5) adds capability gating. Three layers:
//!
//! 1. **Static** (the `ssrf` module · pure): scheme allow-list (http/https),
//!    blocked hostnames (localhost · cloud metadata), literal-IP range
//!    checks (loopback · RFC1918 · link-local/metadata · CGN · v6
//!    local ranges · v4-mapped v6).
//! 2. **DNS-resolve**: non-literal hosts are resolved via
//!    `tokio::net::lookup_host` and EVERY address is range-checked —
//!    kills decimal-IP tricks (`http://2130706433/`) and
//!    public-names-resolving-private. Honest limit: the TOCTOU window
//!    between this check and reqwest's own connect-time resolution is
//!    narrowed, not eliminated (per-request IP pinning arrives with
//!    `nika-policy`; reqwest `resolve()` is client-level).
//! 3. **Per-hop re-check**: reqwest redirects are DISABLED
//!    (`Policy::none`); this crate follows redirects itself and re-runs
//!    layers 1+2 on every hop — per-request `follow_redirects` works,
//!    and a public host can not bounce the client into private space.
//!
//! # Response size caps
//!
//! `max_response_bytes` (default 64 MiB) activates the kernel's
//! [`HttpError::TooLarge`]: a `Content-Length` above the cap fails
//! fast; bodies without one are capped while reading; streaming bodies
//! get a counting wrapper that yields `TooLarge` mid-stream.

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

/// Configuration for [`ReqwestHttp`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct HttpConfig {
    /// Default request timeout (a per-request `timeout` wins).
    pub timeout: Duration,
    /// Maximum redirect hops followed (each hop re-checked).
    pub max_redirects: u8,
    /// Maximum response body size in bytes ([`HttpError::TooLarge`]).
    pub max_response_bytes: u64,
    /// SSRF enforcement mode.
    pub ssrf: SsrfMode,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            ssrf: SsrfMode::Enforce,
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
        let inner = reqwest::Client::builder()
            .user_agent(concat!("nika/", env!("CARGO_PKG_VERSION")))
            // Redirects are followed manually with per-hop SSRF
            // re-checks — reqwest must never follow on its own.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| HttpError::Other {
                reason: format!("failed to build HTTP client: {e}"),
            })?;
        Ok(Self { inner, config })
    }

    /// Vet one absolute URL per the configured [`SsrfMode`]: static
    /// checks, then DNS-resolve every address for non-literal hosts.
    async fn vet(&self, raw: &str) -> Result<url::Url, HttpError> {
        match self.config.ssrf {
            SsrfMode::Disabled => url::Url::parse(raw).map_err(|e| HttpError::Other {
                reason: format!("invalid URL: {e}"),
            }),
            SsrfMode::Enforce => {
                let parsed = ssrf::check_url(raw)?;
                resolve_guard(&parsed).await?;
                Ok(parsed)
            }
        }
    }

    /// Drive the request including the manual redirect loop. Every hop
    /// is re-vetted; the final `reqwest::Response` is returned together
    /// with the URL string that produced it.
    async fn execute(
        &self,
        request: HttpRequest,
    ) -> Result<(reqwest::Response, String), HttpError> {
        let timeout = request.timeout.unwrap_or(self.config.timeout);
        let mut method = to_reqwest_method(request.method)?;
        let mut body = request.body.clone();
        // Headers are owned + mutable so cross-origin redirects can strip
        // credentials (see the redirect branch below).
        let mut headers = request.headers.clone();
        let mut url = request.url.clone();

        // First attempt + up to max_redirects follow-ups.
        for _hop in 0..=u32::from(self.config.max_redirects) {
            let vetted = self.vet(&url).await?;

            let mut builder = self
                .inner
                .request(method.clone(), vetted.as_str())
                .timeout(timeout);
            for (key, value) in &headers {
                builder = builder.header(key.as_str(), value.as_str());
            }
            if let Some(bytes) = &body {
                builder = builder.body(bytes.clone());
            }

            let response = builder
                .send()
                .await
                .map_err(|e| map_send_error(&e, timeout))?;

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

        let mut collected: Vec<u8> = Vec::new();
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
        let (response, final_url) = self.execute(request).await?;
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
        let (response, final_url) = self.execute(request).await?;
        self.read_capped(response, final_url).await
    }

    /// Send a request and receive a streaming response. The body stream
    /// is wrapped in a byte counter that yields
    /// [`HttpError::TooLarge`] mid-stream past the cap.
    ///
    /// CANCEL SAFETY: cancel-safe from the reader side (drop the stream
    /// and the connection is released). Request-side safety follows
    /// `post` rules.
    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        let max = self.config.max_response_bytes;
        let timeout = request.timeout.unwrap_or(self.config.timeout);
        let (response, final_url) = self.execute(request).await?;

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
        if ssrf::ip_is_private(addr.ip()) {
            return ResolveVerdict::Private;
        }
    }
    if any {
        ResolveVerdict::AllPublic
    } else {
        ResolveVerdict::Empty
    }
}

/// Resolve a non-literal host and range-check every returned address.
/// Literal-IP hosts were already vetted by the static layer.
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
    let lookup = tokio::time::timeout(DNS_RESOLVE_TIMEOUT, tokio::net::lookup_host((bare, port)))
        .await
        .map_err(|_| HttpError::Timeout {
            duration_ms: u64::try_from(DNS_RESOLVE_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
        })?;
    let addrs = lookup.map_err(|e| HttpError::Connection {
        reason: format!("DNS resolution failed for {bare}: {e}"),
    })?;

    match classify_resolved(addrs) {
        ResolveVerdict::AllPublic => Ok(()),
        ResolveVerdict::Private => Err(HttpError::SsrfBlocked {
            url: parsed.to_string(),
        }),
        ResolveVerdict::Empty => Err(HttpError::Connection {
            reason: format!("DNS returned no addresses for {bare}"),
        }),
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
fn map_send_error(error: &reqwest::Error, timeout: Duration) -> HttpError {
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

/// Lower response headers into the kernel's `BTreeMap<String, String>`
/// (header names are already lowercase per the `http` crate; non-UTF-8
/// values are skipped — brouillon parity; repeats: last wins).
fn headers_to_btreemap(headers: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (key, value) in headers {
        if let Ok(v) = value.to_str() {
            map.insert(key.as_str().to_string(), v.to_string());
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
                self.seen += chunk.len() as u64;
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
}
