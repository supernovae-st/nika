// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The crate test module — unit invariants + the loopback e2e suite,
//! out of `lib.rs` per the #380 cohesion pattern (`src/tests.rs` is the
//! prod-LOC-exempt convention the hygiene vectors share; a sibling
//! `e2e.rs` was NOT — the unwrap vector flagged its test-only expects
//! as production, which is exactly the convention working as designed).
//! `super` resolves to the crate root — semantics unchanged.

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
    let allow = NetBoundary::Declared(vec!["allowed.com".to_owned(), "*.github.com".to_owned()]);
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
    assert!(check_net_allowlist(&NetBoundary::Unbounded, &u("https://anywhere.example/p")).is_ok());
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
    assert!(matches!(
        classify_resolved(mixed, false),
        ResolveVerdict::Private
    ));
}

#[test]
fn classify_resolved_all_public_passes() {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    let public = vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 443),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443),
    ];
    assert!(matches!(
        classify_resolved(public, false),
        ResolveVerdict::AllPublic
    ));
}

#[test]
fn classify_resolved_empty_is_empty() {
    assert!(matches!(
        classify_resolved(std::iter::empty(), false),
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
    assert!(matches!(
        classify_resolved(rebind, false),
        ResolveVerdict::Private
    ));
}

#[test]
fn classify_resolved_allow_loopback_clears_loopback_only() {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    // The #395 declassification at the RESOLVED layer: when the request's
    // host is a permitted exact loopback literal, its resolved LOOPBACK
    // addresses are admitted (a permitted `localhost` must reach its
    // 127.0.0.1/::1)…
    let loopback = vec![
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 80),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 80),
    ];
    assert!(matches!(
        classify_resolved(loopback.clone(), true),
        ResolveVerdict::AllPublic
    ));
    // …without the declassification the same set refuses (today's law)…
    assert!(matches!(
        classify_resolved(loopback, false),
        ResolveVerdict::Private
    ));
    // …and the clearing is the LOOPBACK CLASS ONLY: a rebind of the
    // permitted name to metadata/RFC1918 still refuses.
    for private in [
        Ipv4Addr::new(169, 254, 169, 254),
        Ipv4Addr::new(10, 0, 0, 5),
    ] {
        let rebind = vec![SocketAddr::new(IpAddr::V4(private), 80)];
        assert!(
            matches!(classify_resolved(rebind, true), ResolveVerdict::Private),
            "{private} must stay blocked even under allow_loopback"
        );
    }
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
fn strip_sensitive_covers_the_headers_this_engine_actually_sends() {
    // The regression of 2026-08-02: the strip list was copied from a
    // general-purpose HTTP client, which has never heard of `x-api-key`
    // — the header our Anthropic wire sends, and the one `nika:fetch`
    // documents to workflow authors. The sibling test above passed
    // throughout, because it only ever tried `authorization`, the one
    // name both lists happened to share. A mechanism test is not a
    // coverage test.
    for name in [
        "x-api-key",      // anthropic wire
        "X-Goog-Api-Key", // gemini wire, and case must not matter
        "xi-api-key",     // elevenlabs
        "api-key",
        "apikey",
        "x-auth-token",
        "proxy-authorization",
    ] {
        let mut h = std::collections::BTreeMap::new();
        h.insert(name.to_string(), "sk-LEAK".to_string());
        h.insert("X-Keep".to_string(), "ok".to_string());
        strip_sensitive_headers(&mut h);
        assert!(
            !h.contains_key(name),
            "{name} carries a credential and must not survive a cross-origin hop"
        );
        assert!(h.contains_key("X-Keep"), "neutral header kept for {name}");
    }
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
    let err = GuardedResolver::default()
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
async fn guarded_resolver_does_not_dial_documentation_hosts() {
    use reqwest::dns::Resolve;
    use std::str::FromStr;
    use std::time::Instant;

    let name = reqwest::dns::Name::from_str("api.example.com").expect("valid name");
    let start = Instant::now();
    let err = GuardedResolver::default()
        .resolve(name)
        .await
        .err()
        .expect("example.com is not dialed");
    assert!(
        start.elapsed() < std::time::Duration::from_millis(50),
        "documentation hosts fail before DNS: {:?}",
        start.elapsed()
    );
    let http = err
        .downcast_ref::<HttpError>()
        .expect("the resolver emits the kernel error type");
    assert!(
        matches!(http, HttpError::Connection { reason } if reason.contains("RFC 2606")),
        "{http}"
    );
}

#[tokio::test]
async fn guarded_resolver_clears_a_permitted_localhost_name() {
    use reqwest::dns::Resolve;
    use std::str::FromStr;

    // The exact literal `localhost` in `permits.net.http` (#395): the
    // resolver admits the loopback addresses it resolves to — the permit
    // must clear the resolved 127.0.0.1/::1, or the grant would be a lie
    // at the connect path (layer 3).
    let resolver = GuardedResolver {
        net_permits: std::sync::Arc::new(vec!["localhost".to_owned()]),
    };
    let name = reqwest::dns::Name::from_str("localhost").expect("valid name");
    assert!(
        resolver.resolve(name).await.is_ok(),
        "the permitted exact literal must clear its own resolution"
    );
}

#[tokio::test]
async fn guarded_resolver_declassification_is_exact_per_name() {
    use reqwest::dns::Resolve;
    use std::str::FromStr;

    // Permits name `127.0.0.1` — the RESOLVED name `localhost` is a
    // DIFFERENT host: still refused. The declassification is the literal
    // in the file (per-name), never the loopback class wholesale — the
    // `mylocal.dev`-resolves-to-127.0.0.1 case rides this same seam.
    let resolver = GuardedResolver {
        net_permits: std::sync::Arc::new(vec!["127.0.0.1".to_owned()]),
    };
    let name = reqwest::dns::Name::from_str("localhost").expect("valid name");
    let err = resolver
        .resolve(name)
        .await
        .err()
        .expect("an un-permitted loopback NAME stays blocked");
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

#[test]
fn bracketed_loopback_entry_admits_its_host_at_the_declared_boundary() {
    // `[::1]` — the URL-authority spelling of `::1`. As a plain glob it
    // matches nothing (permits are written bare), but as a qualifying
    // loopback literal (#395) it must ALSO admit its host at the declared
    // boundary — otherwise the entry would clear the floor and then fail
    // NIKA-SEC-004: check-green + run-refused, the exact drift class this
    // issue exists to close.
    let b = NetBoundary::Declared(vec!["[::1]".to_owned()]);
    let u = url::Url::parse("http://[::1]:8080/p").expect("valid url");
    assert!(check_net_allowlist(&b, &u).is_ok());
    // …and it admits ONLY its host — the boundary stays default-deny.
    let pub_u = url::Url::parse("https://api.example.com/p").expect("valid url");
    assert!(check_net_allowlist(&b, &pub_u).is_err());
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

/// Loopback e2e over REAL sockets — `SsrfMode::Disabled` for the transport
/// suites (the targets ARE loopback; the SSRF layers have their own tests
/// above), plus the `Enforce`+declassification suite (#395): the ONE lawful
/// way an enforcing client reaches a loopback socket is the author's exact
/// literal in `permits.net.http`.
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

    /// An ENFORCING client whose boundary declares the exact literal —
    /// the #395 declassification shape, used by the suite below.
    fn enforcing_client(entries: &[&str]) -> ReqwestHttp {
        let mut config = HttpConfig::new(); // SsrfMode::Enforce
        config.net = NetBoundary::Declared(entries.iter().map(|e| (*e).to_string()).collect());
        ReqwestHttp::with_config(config).expect("client builds")
    }

    #[tokio::test]
    async fn enforce_with_permitted_loopback_literal_reaches_a_real_local_server() {
        // THE issue-#395 e2e: SSRF Enforce + `permits.net.http:
        // ["127.0.0.1"]` — the author's explicit act clears the floor for
        // that host, and the request lands on a REAL loopback socket
        // (previously only `SsrfMode::Disabled` could).
        let addr = serve(vec![ok_response("local fixture")]).await;
        let resp = enforcing_client(&["127.0.0.1"])
            .get(HttpRequest::get(format!("http://{addr}/price.json")))
            .await
            .expect("the permitted exact literal clears the floor");
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"local fixture");
    }

    #[tokio::test]
    async fn enforce_permitted_loopback_redirect_within_the_host_lands() {
        // Both hops are the permitted host (different ports — permits are
        // host-level): the per-hop re-vet clears each one.
        let dest = serve(vec![ok_response("arrived")]).await;
        let hop = serve(vec![format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{dest}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )])
        .await;
        let resp = enforcing_client(&["127.0.0.1"])
            .get(HttpRequest::get(format!("http://{hop}/start")))
            .await
            .expect("both hops are the permitted host");
        assert_eq!(resp.body.as_ref(), b"arrived");
    }

    #[tokio::test]
    async fn enforce_redirect_to_unpermitted_floor_host_refuses() {
        // A permitted 127.0.0.1 server bounces to `localhost` — NOT
        // permitted: the per-hop re-vet refuses (the declassification
        // clears ONE exact host, it never travels with the request).
        let hop = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: http://localhost:9/x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned(),
        ])
        .await;
        let err = enforcing_client(&["127.0.0.1"])
            .get(HttpRequest::get(format!("http://{hop}/start")))
            .await
            .expect_err("the hop target is floor-blocked and unpermitted");
        assert!(matches!(err, HttpError::SsrfBlocked { .. }), "{err}");
    }

    #[tokio::test]
    async fn enforce_unpermitted_loopback_stays_blocked_under_a_declared_boundary() {
        // A declared boundary WITHOUT the loopback literal: the floor
        // holds — and it speaks SEC-005's SsrfBlocked (the floor gates
        // before the allowlist), with zero network activity.
        let err = enforcing_client(&["api.example.com"])
            .get(HttpRequest::get("http://127.0.0.1:9/x"))
            .await
            .expect_err("no declassifying literal → the floor holds");
        assert!(matches!(err, HttpError::SsrfBlocked { .. }), "{err}");
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
        // The provider headers, on the wire, for real (2026-08-02): these
        // are what the Anthropic and Gemini wires send, and until this
        // line existed a 302 carried them to the redirect target.
        request
            .headers
            .insert("x-api-key".to_owned(), "sk-ant-LEAK".to_owned());
        request
            .headers
            .insert("x-goog-api-key".to_owned(), "goog-LEAK".to_owned());
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
        assert!(
            !wire.contains("sk-ant-LEAK"),
            "the provider API key must not cross an origin: {wire}"
        );
        assert!(
            !wire.contains("goog-LEAK"),
            "the provider API key must not cross an origin: {wire}"
        );
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
