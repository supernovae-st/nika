// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! SSRF (Server-Side Request Forgery) defense — pure checks.
//!
//! Two layers live here, both side-effect-free and unit/property
//! tested; the async DNS-resolve layer (which consumes
//! [`ip_is_private`]) lives in `lib.rs` next to the runtime:
//!
//! 1. [`check_url`] — static URL vetting: scheme allow-list,
//!    blocked hostnames (cloud metadata · loopback aliases),
//!    literal-IP range checks.
//! 2. [`ip_is_private`] — the single range oracle every layer shares
//!    (static literal IPs, DNS-resolved addresses, redirect hops).
//!
//! Blocked ranges: loopback 127/8 + `::1` · RFC1918 (10/8 ·
//! 172.16/12 · 192.168/16) · link-local 169.254/16 (cloud metadata
//! IP lives here) + `fe80::/10` · CGN 100.64/10 (Alibaba metadata) ·
//! unspecified + broadcast · IPv6 unique-local `fc00::/7` ·
//! IPv4-mapped/compatible IPv6 re-checked as the inner v4.

use std::net::{IpAddr, Ipv4Addr};

use nika_kernel::HttpError;

/// True HOSTNAME strings that are always blocked (loopback alias ·
/// cloud metadata). IP literals — dotted v4, bracketed v6 — do NOT
/// belong here: `url::Url::host_str()` strips v6 brackets and the
/// literal-IP parse branch below is authoritative for every address
/// form (review swarm P2 · 2026-06-10: the former `"[::1]"`/`"[::]"`/
/// `"0.0.0.0"` entries were unreachable or redundant dead code).
/// Comparison is case-insensitive and trailing-dot-normalized
/// (`localhost.` is the absolute-FQDN spelling of `localhost`).
const BLOCKED_HOSTNAMES: &[&str] = &[
    "localhost",
    // Cloud metadata endpoints
    "metadata.google.internal",
    "metadata.goog",
    "169.254.169.254", // AWS/GCP/Azure metadata IP (also caught by link-local)
];

/// Statically vet a URL before any network activity.
///
/// Returns the parsed [`url::Url`] on success so callers never parse
/// twice (parse-once discipline; the redirect loop re-enters here for
/// every hop).
///
/// # Errors
///
/// [`HttpError::SsrfBlocked`] for non-http(s) schemes, blocked
/// hostnames, and literal IPs in private ranges; [`HttpError::Other`]
/// for unparseable URLs.
pub(crate) fn check_url(raw: &str) -> Result<url::Url, HttpError> {
    let parsed = url::Url::parse(raw).map_err(|e| HttpError::Other {
        reason: format!("invalid URL: {e}"),
    })?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(HttpError::SsrfBlocked {
                url: format!("blocked scheme {scheme}: {raw}"),
            });
        }
    }

    let host = parsed.host_str().unwrap_or("");
    // Strip the absolute-FQDN trailing dot so `localhost.` fails fast
    // in the static layer (it would still be caught by DNS-resolve).
    let host_lower = host.trim_end_matches('.').to_ascii_lowercase();

    if BLOCKED_HOSTNAMES.iter().any(|b| host_lower == *b) {
        return Err(HttpError::SsrfBlocked {
            url: raw.to_string(),
        });
    }

    // Literal IP in the authority — the single, authoritative oracle.
    // EMPIRICAL (url 2.x · verified 2026-06-10): `host_str()` keeps the
    // brackets for IPv6 (`"[::1]"`) and canonicalizes every IPv4 spelling
    // — decimal `2130706433`, octal `0177.0.0.1`, hex — to dotted form.
    // So we strip the v6 brackets and parse ONCE: that catches dotted v4
    // AND bracketed v6 literals. (A bare `host.parse()` would miss v6
    // because of the brackets — the brackets-stripped parse is the only
    // form that handles both.)
    let literal = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = literal.parse::<IpAddr>()
        && ip_is_private(ip)
    {
        return Err(HttpError::SsrfBlocked {
            url: raw.to_string(),
        });
    }

    Ok(parsed)
}

/// The single private-range oracle. Every defense layer (static
/// literal, DNS-resolved, per-hop) funnels through this predicate.
pub(crate) fn ip_is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4_is_private(v4),
        IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
                || v6.is_unspecified() // ::
                // fc00::/7 (unique local)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 (link-local)
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // Embedded v4 — re-check the inner address. `to_ipv4()`
                // covers BOTH IPv4-mapped (::ffff:a.b.c.d) AND the
                // deprecated IPv4-compatible (::a.b.c.d) forms, so one
                // arm suffices (a separate `to_ipv4_mapped()` arm would
                // be redundant — and would mask this one under mutation
                // testing).
                || v6.to_ipv4().is_some_and(v4_is_private)
        }
    }
}

fn v4_is_private(v4: Ipv4Addr) -> bool {
    v4.is_loopback()           // 127.0.0.0/8
        || v4.is_private()     // 10/8 · 172.16/12 · 192.168/16
        || v4.is_link_local()  // 169.254.0.0/16 (cloud metadata IP)
        || v4.is_unspecified() // 0.0.0.0
        || v4.is_broadcast()   // 255.255.255.255
        // 100.64.0.0/10 — CGN / shared address space (Alibaba metadata)
        || (v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── brouillon parity vectors (Gate 10) ──────────────────────────

    #[test]
    fn blocks_localhost_aliases() {
        assert!(check_url("http://localhost/api").is_err());
        assert!(check_url("http://127.0.0.1/api").is_err());
        assert!(check_url("http://0.0.0.0/api").is_err());
    }

    #[test]
    fn blocks_private_ranges() {
        assert!(check_url("http://10.0.0.1/api").is_err());
        assert!(check_url("http://172.16.0.1/api").is_err());
        assert!(check_url("http://192.168.1.1/api").is_err());
        assert!(check_url("http://169.254.1.1/api").is_err());
    }

    #[test]
    fn blocks_cgn_range_boundaries() {
        assert!(check_url("http://100.64.0.1/api").is_err());
        assert!(check_url("http://100.100.100.200/latest/meta-data/").is_err());
        assert!(check_url("http://100.127.255.255/api").is_err());
        // 100.128.0.0 is outside CGN — allowed.
        assert!(check_url("http://100.128.0.1/api").is_ok());
    }

    #[test]
    fn blocks_ipv6_loopback_and_mapped() {
        assert!(check_url("http://[::1]/api").is_err());
        assert!(check_url("http://[::ffff:127.0.0.1]/api").is_err());
        assert!(check_url("http://[::ffff:10.0.0.1]/api").is_err());
        assert!(check_url("http://[::ffff:169.254.169.254]/api").is_err());
        assert!(check_url("http://[::ffff:100.100.100.200]/api").is_err());
    }

    #[test]
    fn blocks_unspecified_hosts_via_ip_path() {
        // The former string entries for these were dead code — the
        // literal-IP branch is the authoritative defense.
        assert!(check_url("http://0.0.0.0/api").is_err());
        assert!(check_url("http://[::]/api").is_err());
    }

    #[test]
    fn blocks_trailing_dot_fqdn_statically() {
        assert!(check_url("http://localhost./api").is_err());
        assert!(check_url("http://metadata.google.internal./x").is_err());
        assert!(check_url("http://Metadata.Goog./x").is_err());
    }

    #[test]
    fn blocks_ipv6_local_ranges() {
        assert!(check_url("http://[fc00::1]/api").is_err()); // unique local
        assert!(check_url("http://[fe80::1]/api").is_err()); // link-local
    }

    #[test]
    fn blocks_cloud_metadata_hostnames() {
        assert!(check_url("http://metadata.google.internal/computeMetadata/v1/").is_err());
        assert!(check_url("http://metadata.goog/computeMetadata/v1/").is_err());
        assert!(check_url("http://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn blocks_non_http_schemes() {
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("ftp://example.com/x").is_err());
        assert!(check_url("gopher://example.com/x").is_err());
    }

    #[test]
    fn allows_public_urls() {
        assert!(check_url("https://api.anthropic.com/v1/messages").is_ok());
        assert!(check_url("https://example.com").is_ok());
        assert!(check_url("http://8.8.8.8/dns").is_ok());
        assert!(check_url("http://[2607:f8b0:4004:800::200e]/").is_ok());
    }

    #[test]
    fn rejects_invalid_urls_as_other() {
        let err = check_url("not a url").unwrap_err();
        assert!(matches!(err, HttpError::Other { .. }));
    }

    #[test]
    fn blocked_hostname_check_is_case_insensitive() {
        assert!(check_url("http://LOCALHOST/api").is_err());
        assert!(check_url("http://Metadata.Google.Internal/x").is_err());
    }

    #[test]
    fn check_url_returns_parsed_url_on_success() {
        let parsed = check_url("https://example.com/path?q=1").expect("public url");
        assert_eq!(parsed.host_str(), Some("example.com"));
        assert_eq!(parsed.path(), "/path");
    }

    // ─── property tests (Gate 6 · security ranges) ───────────────────

    mod properties {
        use super::*;
        use proptest::prelude::*;

        fn v4_url(ip: Ipv4Addr) -> String {
            format!("http://{ip}/x")
        }

        fn scheme_url(scheme: &str) -> String {
            format!("{scheme}://example.com/x")
        }

        fn v6_url(ip: std::net::Ipv6Addr) -> String {
            format!("http://[{ip}]/x")
        }

        /// Generator over the blocked v4 ranges.
        fn private_v4() -> impl Strategy<Value = Ipv4Addr> {
            prop_oneof![
                // 127.0.0.0/8
                (any::<u8>(), any::<u8>(), any::<u8>())
                    .prop_map(|(b, c, d)| Ipv4Addr::new(127, b, c, d)),
                // 10.0.0.0/8
                (any::<u8>(), any::<u8>(), any::<u8>())
                    .prop_map(|(b, c, d)| Ipv4Addr::new(10, b, c, d)),
                // 172.16.0.0/12
                (16u8..=31, any::<u8>(), any::<u8>())
                    .prop_map(|(b, c, d)| Ipv4Addr::new(172, b, c, d)),
                // 192.168.0.0/16
                (any::<u8>(), any::<u8>()).prop_map(|(c, d)| Ipv4Addr::new(192, 168, c, d)),
                // 169.254.0.0/16 (link-local / metadata)
                (any::<u8>(), any::<u8>()).prop_map(|(c, d)| Ipv4Addr::new(169, 254, c, d)),
                // 100.64.0.0/10 (CGN)
                (64u8..=127, any::<u8>(), any::<u8>())
                    .prop_map(|(b, c, d)| Ipv4Addr::new(100, b, c, d)),
            ]
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(256))]

            /// Every address in a blocked range is private — and its
            /// literal URL is SsrfBlocked.
            #[test]
            fn private_v4_always_blocked(ip in private_v4()) {
                prop_assert!(ip_is_private(IpAddr::V4(ip)));
                let err = check_url(&v4_url(ip)).unwrap_err();
                let blocked = matches!(err, HttpError::SsrfBlocked { .. });
                prop_assert!(blocked, "expected SsrfBlocked for private literal");
            }

            /// Every fc00::/7 (ULA) and fe80::/10 (link-local) address
            /// is private — and its bracketed-literal URL is blocked.
            #[test]
            fn private_v6_local_ranges_always_blocked(
                s in proptest::array::uniform8(any::<u16>()),
                ula in any::<bool>(),
            ) {
                use std::net::Ipv6Addr;
                // fc00::/7 → first segment 0xfc00..=0xfdff ; fe80::/10 →
                // 0xfe80..=0xfebf. Force the high bits, keep the rest random.
                let first = if ula {
                    0xfc00 | (s[0] & 0x01ff)   // fc00::/7
                } else {
                    0xfe80 | (s[0] & 0x003f)   // fe80::/10
                };
                let v6 = Ipv6Addr::new(first, s[1], s[2], s[3], s[4], s[5], s[6], s[7]);
                prop_assert!(ip_is_private(IpAddr::V6(v6)));
                let blocked = matches!(check_url(&v6_url(v6)), Err(HttpError::SsrfBlocked { .. }));
                prop_assert!(blocked, "fc00::/7 + fe80::/10 must be SsrfBlocked");
            }

            /// v4 verdict and its IPv4-mapped-v6 twin always agree.
            #[test]
            fn v4_and_mapped_v6_agree(a in any::<u8>(), b in any::<u8>(), c in any::<u8>(), d in any::<u8>()) {
                let v4 = Ipv4Addr::new(a, b, c, d);
                let mapped = v4.to_ipv6_mapped();
                prop_assert_eq!(
                    ip_is_private(IpAddr::V4(v4)),
                    ip_is_private(IpAddr::V6(mapped)),
                    "mapped-v6 must never widen or narrow the v4 verdict"
                );
            }

            /// Arbitrary addresses the oracle calls public are accepted
            /// as literal URLs (the dual of the blocked property).
            #[test]
            fn public_v4_always_allowed(a in any::<u8>(), b in any::<u8>(), c in any::<u8>(), d in any::<u8>()) {
                let ip = Ipv4Addr::new(a, b, c, d);
                prop_assume!(!ip_is_private(IpAddr::V4(ip)));
                let allowed = check_url(&v4_url(ip)).is_ok();
                prop_assert!(allowed, "public literal must pass");
            }

            /// No non-http(s) scheme ever passes.
            #[test]
            fn foreign_schemes_always_blocked(scheme in "[a-z][a-z0-9+]{1,8}") {
                prop_assume!(scheme != "http" && scheme != "https");
                let res = check_url(&scheme_url(&scheme));
                prop_assert!(res.is_err());
            }
        }
    }
}
