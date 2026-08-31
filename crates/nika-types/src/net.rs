// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `permits.net.http` allowlist glob matching — the single canonical
//! matcher.
//!
//! The static checker (`nika-schema` · `nika check`) AND the runtime http
//! effect (`nika-http` · the `NIKA-SEC-004` enforcement point) both call
//! these — one definition so the check-time verdict and the run-time block
//! can never DRIFT on the GLOB (the `*.`-subdomain wildcard especially).
//!
//! Host EXTRACTION is deliberately NOT here: it must use the `url` crate
//! (the WHATWG parse the transport actually connects with), which is `std`
//! and so cannot live in this `no_std` leaf. Both callers parse with
//! `url::Url` and feed the resulting host to [`host_in_allowlist`] — using
//! the SAME crate, so the extraction agrees too. (A hand-rolled string
//! parser disagrees with `url` on `\`/userinfo/case — that gap is a
//! boundary bypass, which is why it was removed from here.)
//!
//! Spec `01-envelope.md` §permits · `net: { http: [...] }`.

use alloc::format;
use alloc::string::String;

/// Host glob match — exact, a LEADING `*.` subdomain wildcard
/// (`*.github.com` matches `api.github.com` AND the bare `github.com`), or
/// the BARE `*` — the explicit allow-all escape hatch. Distinct from the
/// tool-id trailing-`*` glob (`nika:*`): an embedded `*` (`foo*.com`) still
/// matches nothing; only the whole-entry `*` opens the boundary, and only
/// because the author typed it in-file (never a default — the sandbox
/// network arm maps it to `allow`, the explicit egress escape hatch).
///
/// CASE-INSENSITIVE (RFC 4343 · DNS hostnames are case-insensitive). The
/// connect host arrives WHATWG-lowercased, so an author-written permit entry
/// in any case (`EXAMPLE.COM` · `*.GitHub.com`) still matches — without this
/// an uppercase permit would false-block its own lowercase host. ASCII-fold
/// is sufficient: a host is punycode-ASCII by the time it reaches here.
#[must_use]
pub fn host_glob_matches(glob: &str, host: &str) -> bool {
    let glob = glob.to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    if glob == "*" {
        return true;
    }
    if let Some(suffix) = glob.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    glob == host
}

/// Whether `host` matches ANY glob in the declared `permits.net.http`
/// allowlist. The list is the canonical default-deny boundary: an empty
/// list admits nothing. (Whether a `permits:` block is present at all is
/// the caller's concern — `None` net = no outbound network.)
#[must_use]
pub fn host_in_allowlist(allowlist: &[String], host: &str) -> bool {
    allowlist.iter().any(|g| host_glob_matches(g, host))
}

/// The canonical host-extraction parity vectors — `(url, expected host)`.
///
/// The static checker (`nika-schema`'s `url_host`) and the runtime http
/// effect (`nika-http`'s `host_of`) each parse the host from their own
/// `url::Url` (the `url` crate is `std`, so the extractor itself cannot live
/// in this `no_std` leaf). They MUST agree, or `nika check` and `nika run`
/// drift — a security bug (a host the check passes but the runtime blocks,
/// or worse, the reverse). This table is the shared SOURCE OF TRUTH for that
/// agreement: BOTH crates run their extractor over it and assert the same
/// expected host. A future change to either extractor that diverges on
/// `\`-userinfo confusion, case, IPv6 brackets, or the trailing-dot FQDN is
/// then caught mechanically, in both test suites, against ONE list of cases.
pub const HOST_EXTRACTION_VECTORS: &[(&str, Option<&str>)] = &[
    // a plain host (query + fragment dropped)
    ("https://api.example.com/p?q=1#f", Some("api.example.com")),
    // the `\@` bypass: `\` is a WHATWG path separator for http/https, so the
    // host is `evil.com` — NOT `allowed.com` (what a string parser would read)
    (r"https://evil.com\@allowed.com/x", Some("evil.com")),
    // userinfo is stripped — the real host is `evil.com`
    ("https://user:pass@evil.com/x", Some("evil.com")),
    // the host is lowercased by WHATWG parsing
    ("https://ALLOWED.com/x", Some("allowed.com")),
    // IPv6 is bracket-free (permits are written `::1`, never `[::1]`)
    ("http://[::1]:8080/x", Some("::1")),
    // an absolute-FQDN trailing dot is stripped (`allowed.com.` ≡ `allowed.com`)
    ("https://allowed.com./x", Some("allowed.com")),
    // the tab/newline bypass: WHATWG REMOVES ASCII tab + LF + CR from the URL
    // before parsing, so the real host is `allowed.com.evil.com` — NOT
    // `allowed.com` (what a parser splitting on the raw tab would read · the
    // exact string-parser-vs-WHATWG gap this table guards). CR variant too.
    (
        "https://allowed.com\t.evil.com/x",
        Some("allowed.com.evil.com"),
    ),
    (
        "https://allowed.com\r\n.evil.com/x",
        Some("allowed.com.evil.com"),
    ),
    // a hostless URL has no extractable host (a declared boundary denies it)
    ("mailto:user@example.com", None),
];

/// The single SSRF IP-range oracle: `true` when `ip` is NOT a public unicast
/// address (loopback · private · link-local/metadata · CGN · documentation ·
/// reserved · and any of those smuggled inside an IPv4-mapped / NAT64 / 6to4
/// v6 wrapper). Every egress boundary that has a resolved/literal IP funnels
/// through this ONE predicate — the http effect (`nika-http`), the browser
/// effect (`nika-browser` navigate), and any future resolver — so a host the
/// one blocks another can never reach. Pure · `no_std` (`core::net`).
#[must_use]
pub fn ip_is_blocked(ip: core::net::IpAddr) -> bool {
    match ip {
        core::net::IpAddr::V4(v4) => v4_is_blocked(v4),
        core::net::IpAddr::V6(v6) => v6_is_blocked(v6),
    }
}

/// True HOSTNAME strings the SSRF floor always refuses (cloud metadata).
/// The `localhost` FAMILY (`localhost` + any `*.localhost` label · RFC 6761
/// reserves the TLD as loopback) is matched structurally in
/// [`host_is_blocked`], and IP literals do NOT belong here — the literal-IP
/// parse there is authoritative for every address form (a string entry for
/// either class would be unreachable dead code · review swarm P2/P0
/// 2026-06-10/12).
const BLOCKED_HOSTNAMES: &[&str] = &["metadata.google.internal", "metadata.goog"];

/// The single STATIC SSRF host oracle: `true` when `host` — a hostname or a
/// literal IP in any spelling (dotted v4 · bracketed or bare v6) — is a
/// target the always-on SSRF floor refuses (`NIKA-SEC-005`): the `localhost`
/// family (RFC 6761), a cloud-metadata hostname, or a literal address
/// [`ip_is_blocked`] classifies as non-public. Case-insensitive and
/// trailing-dot-normalized (`localhost.` is the absolute-FQDN spelling of
/// `localhost`). Every egress boundary funnels its STATIC host knowledge
/// through this one predicate — the http effect (`nika-http`'s `check_url`),
/// the browser navigate gate (`nika-browser`), and the `nika check` floor
/// parity pass (`nika-schema`) — so check and run cannot drift. A DNS name
/// that merely RESOLVES to a private address is invisible statically; the
/// runtime `GuardedResolver` owns that half.
#[must_use]
pub fn host_is_blocked(host: &str) -> bool {
    let trimmed = host.trim_end_matches('.');
    let last_label = trimmed.rsplit('.').next().unwrap_or(trimmed);
    if last_label.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if BLOCKED_HOSTNAMES
        .iter()
        .any(|b| trimmed.eq_ignore_ascii_case(b))
    {
        return true;
    }
    let literal = trimmed.trim_start_matches('[').trim_end_matches(']');
    literal
        .parse::<core::net::IpAddr>()
        .is_ok_and(ip_is_blocked)
}

/// RFC 2606 / 6761 documentation names — public, so the SSRF floor
/// admits them ([`host_is_blocked`] is false), but they are not a live
/// API. The live HTTP client must not dial them (persona 17 · `nika try
/// 05-fetch-chain` was a 21ms miss to `api.example.com` before recover).
///
/// Covers `example.com` / `.net` / `.org` / `.edu` and those TLDs as
/// labels (`.example` · `.invalid` · `.test`). `.localhost` stays the
/// SSRF floor's.
#[must_use]
pub fn is_documentation_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    let last = h.rsplit('.').next().unwrap_or(h.as_str());
    if matches!(last, "invalid" | "test" | "example") {
        return true;
    }
    h == "example.com"
        || h == "example.net"
        || h == "example.org"
        || h == "example.edu"
        || h.ends_with(".example.com")
        || h.ends_with(".example.net")
        || h.ends_with(".example.org")
        || h.ends_with(".example.edu")
}

/// The loopback identity an exact literal can carry — the CLOSED
/// declassification vocabulary of the SSRF floor (issue #395).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopbackLiteral {
    /// The bare `localhost` name (RFC 6761) — never the `*.localhost`
    /// family (a subdomain names an arbitrary service).
    Localhost,
    /// A literal loopback address — v4 `127.0.0.0/8` or v6 `::1`.
    Ip(core::net::IpAddr),
}

/// Classify an EXACT loopback literal, `None` for everything else. Same
/// normalization as [`host_is_blocked`] (trailing FQDN dot · v6 authority
/// brackets · ASCII case), so the two predicates read one host language.
/// Globs are excluded structurally: a `*` can neither equal `localhost`
/// nor parse as an address.
fn exact_loopback(entry: &str) -> Option<LoopbackLiteral> {
    let trimmed = entry.trim_end_matches('.');
    if trimmed.eq_ignore_ascii_case("localhost") {
        return Some(LoopbackLiteral::Localhost);
    }
    let literal = trimmed.trim_start_matches('[').trim_end_matches(']');
    let ip = literal.parse::<core::net::IpAddr>().ok()?;
    // `is_loopback` is EXACT: v4 127/8, v6 `::1` — never the unspecified
    // addresses, never a mapped/NAT64 wrapper smuggling 127.x (those parse
    // as non-loopback v6 and stay floor-blocked).
    ip.is_loopback().then_some(LoopbackLiteral::Ip(ip))
}

/// True when `entry` is an EXACT loopback literal — the bare `localhost`
/// name, a `127.0.0.0/8` v4 literal, or the v6 loopback `::1` (bracketed
/// or bare). The closed vocabulary an author may DECLASSIFY the SSRF
/// floor with (issue #395 · the ADR-092 secrets-`egress:` precedent: the
/// owner's explicit act, co-located with the boundary). NEVER a glob,
/// NEVER the `*.localhost` family, NEVER RFC1918 / link-local / CGN /
/// metadata — those stay floor-blocked even when named in permits.
#[must_use]
pub fn is_exact_loopback_literal(entry: &str) -> bool {
    exact_loopback(entry).is_some()
}

/// True when `host` is cleared from the SSRF floor by an exact loopback
/// literal in the declared `permits.net.http` allowlist (issue #395).
/// EXACT host comparison — never a glob, prefix, or subnet: `localhost`
/// clears `localhost` only (not `127.0.0.1` — the declassification is
/// the literal in the file, never what it resolves to), `127.0.0.1`
/// clears `127.0.0.1` only. Both sides must be loopback: a non-loopback
/// host is never declassifiable, so RFC1918/metadata grants stay inert.
/// Every floor surface reads THIS one predicate — the http static vet
/// (`nika-http`), the resolved-address guard's name check, and the
/// `nika check` floor-parity pass — so check and run cannot drift.
#[must_use]
pub fn loopback_declassified(allowlist: &[String], host: &str) -> bool {
    let Some(target) = exact_loopback(host) else {
        return false;
    };
    allowlist.iter().any(|e| exact_loopback(e) == Some(target))
}

/// The v4 address a transition-format v6 embeds in two segments (NAT64
/// `64:ff9b::a.b.c.d` in segments 6+7 · 6to4 `2002:a.b:c.d::` in segments 1+2).
fn embedded_v4(hi: u16, lo: u16) -> core::net::Ipv4Addr {
    let [a, b] = hi.to_be_bytes();
    let [c, d] = lo.to_be_bytes();
    core::net::Ipv4Addr::new(a, b, c, d)
}

fn v6_is_blocked(v6: core::net::Ipv6Addr) -> bool {
    let s = v6.segments();
    v6.is_loopback()           // ::1
        || v6.is_unspecified() // ::
        || v6.is_multicast()   // ff00::/8
        // fc00::/7 (unique local)
        || (s[0] & 0xfe00) == 0xfc00
        // fe80::/10 (link-local)
        || (s[0] & 0xffc0) == 0xfe80
        // fec0::/10 (site-local · deprecated but still honored by old gear)
        || (s[0] & 0xffc0) == 0xfec0
        // 2001:db8::/32 (documentation — never routable)
        || (s[0] == 0x2001 && s[1] == 0x0db8)
        // 64:ff9b:1::/48 (NAT64 local-use · RFC 8215 — internal by definition)
        || (s[0] == 0x0064 && s[1] == 0xff9b && s[2] == 0x0001)
        // 64:ff9b::/96 (NAT64 well-known · RFC 6052) — re-check the embedded v4.
        || (s[..6] == [0x0064, 0xff9b, 0, 0, 0, 0] && v4_is_blocked(embedded_v4(s[6], s[7])))
        // 2002::/16 (6to4 · RFC 3056) — same embedded-v4 re-check.
        || (s[0] == 0x2002 && v4_is_blocked(embedded_v4(s[1], s[2])))
        // IPv4-mapped/compatible — re-check the inner address (`to_ipv4()`
        // covers both ::ffff:a.b.c.d and the deprecated ::a.b.c.d forms).
        || v6.to_ipv4().is_some_and(v4_is_blocked)
}

fn v4_is_blocked(v4: core::net::Ipv4Addr) -> bool {
    let o = v4.octets();
    v4.is_loopback()              // 127.0.0.0/8
        || v4.is_private()        // 10/8 · 172.16/12 · 192.168/16
        || v4.is_link_local()     // 169.254.0.0/16 (cloud metadata IP)
        || v4.is_unspecified()    // 0.0.0.0
        || v4.is_broadcast()      // 255.255.255.255
        || v4.is_multicast()      // 224.0.0.0/4
        || v4.is_documentation()  // TEST-NET-1/2/3 (192.0.2 · 198.51.100 · 203.0.113)
        || o[0] == 0              // 0.0.0.0/8 — "this network" (= this host on Linux)
        || o[0] >= 240            // 240.0.0.0/4 — reserved
        // 100.64.0.0/10 — CGN / shared address space (Alibaba metadata)
        || (o[0] == 100 && (64..=127).contains(&o[1]))
        // 198.18.0.0/15 — benchmarking (RFC 2544)
        || (o[0] == 198 && (o[1] & 0xfe) == 18)
        // 192.0.0.0/24 — IETF protocol assignments (incl. NAT64/DNS64 discovery)
        || (o[0] == 192 && o[1] == 0 && o[2] == 0)
        // 192.88.99.0/24 — 6to4 relay anycast (deprecated)
        || (o[0] == 192 && o[1] == 88 && o[2] == 99)
}

/// `nika:fetch` `traverse:` page ceiling — the bound shared by the
/// static checker (`nika-schema` · the arg range rule + the effect
/// certificate's worst-case) and the runtime crawl loop
/// (`nika-builtin`), so the check-time bound and the run-time cap can
/// never drift (the same one-definition rule as the host matcher
/// above). Spec `stdlib/builtins-v0.1.md` §nika:fetch · traverse.
pub const MAX_TRAVERSE_PAGES: u64 = 25;

/// `nika:fetch` `multipart:` part-key vocabulary — the CLOSED shape
/// (`{name, value}` text XOR `{name, path, filename?, content_type?}`
/// file), shared by the static shape rule (`nika-schema`) and the
/// runtime part resolver (`nika-builtin`) — one definition, no drift
/// (the same rule as the traverse bound above).
pub const MULTIPART_PART_KEYS: [&str; 5] = ["name", "value", "path", "filename", "content_type"];

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn host_glob_exact_and_subdomain_wildcard() {
        // exact
        assert!(host_glob_matches("example.com", "example.com"));
        assert!(!host_glob_matches("example.com", "evil.com"));
        assert!(!host_glob_matches("example.com", "sub.example.com"));
        // leading *. matches a subdomain AND the bare apex
        assert!(host_glob_matches("*.github.com", "api.github.com"));
        assert!(host_glob_matches("*.github.com", "github.com"));
        assert!(!host_glob_matches("*.github.com", "github.com.evil.com"));
        assert!(!host_glob_matches("*.github.com", "notgithub.com"));
    }

    #[test]
    fn host_glob_is_case_insensitive() {
        // DNS hostnames are case-insensitive (RFC 4343) — an author-written
        // permit entry in any case matches the WHATWG-lowercased connect host
        // (else an uppercase permit false-blocks its own host).
        assert!(host_glob_matches("EXAMPLE.COM", "example.com"));
        assert!(host_glob_matches("example.com", "EXAMPLE.COM"));
        assert!(host_glob_matches("*.GitHub.com", "API.github.com"));
        assert!(host_glob_matches("*.github.com", "GITHUB.COM"));
        // case-folding does not loosen the boundary — a different host stays out
        assert!(!host_glob_matches("EXAMPLE.COM", "evil.com"));
        assert!(!host_glob_matches("*.GITHUB.COM", "github.com.evil.com"));
    }

    #[test]
    fn bare_star_is_the_explicit_allow_all_hatch() {
        // The escape hatch: `net: { http: ["*"] }` opens the boundary to every
        // host — the author's explicit in-file act (maps the sandbox network
        // arm to `allow`), never a default.
        assert!(host_glob_matches("*", "example.com"));
        assert!(host_glob_matches("*", "anything.at.all"));
        assert!(host_glob_matches("*", ""));
        // ONLY the bare entry: an embedded `*` is not a wildcard here (the
        // host language has exact + leading-`*.` forms only), so a typo like
        // `foo*.com` keeps matching nothing (fail-closed).
        assert!(!host_glob_matches("foo*.com", "foo1.com"));
        assert!(!host_glob_matches("*.com", "example.com."));
    }

    #[test]
    fn allowlist_is_default_deny() {
        let empty: vec::Vec<String> = vec![];
        assert!(
            !host_in_allowlist(&empty, "example.com"),
            "empty allowlist admits nothing"
        );
        let list = vec!["example.com".to_string(), "*.github.com".to_string()];
        assert!(host_in_allowlist(&list, "example.com"));
        assert!(host_in_allowlist(&list, "api.github.com"));
        assert!(!host_in_allowlist(&list, "example.org"));
    }

    #[test]
    fn ip_oracle_blocks_the_non_public_ranges() {
        use core::net::IpAddr;
        for s in [
            "127.0.0.1",
            "0.0.0.0",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",
            "100.127.255.255", // CGN boundaries
            "198.18.0.1",
            "192.0.0.1",
            "192.88.99.1",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            let ip: IpAddr = s.parse().expect("v4");
            assert!(ip_is_blocked(ip), "{s} must be blocked");
        }
        for s in [
            "::1",
            "::",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254", // IPv4-mapped smuggling
            "64:ff9b::7f00:1",        // NAT64 well-known wrapping 127.0.0.1
            "2002:a9fe:a9fe::1",      // 6to4 wrapping 169.254.169.254
        ] {
            let ip: IpAddr = s.parse().expect("v6");
            assert!(ip_is_blocked(ip), "{s} must be blocked");
        }
    }

    #[test]
    fn ip_oracle_admits_public_unicast() {
        use core::net::IpAddr;
        for s in [
            "1.1.1.1",
            "8.8.8.8",
            "100.128.0.1",
            "93.184.216.34",
            "2606:4700:4700::1111",
        ] {
            let ip: IpAddr = s.parse().expect("ip");
            assert!(!ip_is_blocked(ip), "{s} is public, must be admitted");
        }
    }

    #[test]
    fn host_oracle_blocks_names_and_literals_in_every_spelling() {
        for h in [
            "localhost",
            "LOCALHOST",
            "localhost.",    // absolute-FQDN spelling
            "api.localhost", // RFC 6761: the TLD is loopback
            "a.b.localhost",
            "metadata.google.internal",
            "Metadata.Goog.",
            "127.0.0.1",
            "0.0.0.0",
            "10.0.0.1",
            "169.254.169.254", // cloud metadata IP
            "::1",
            "[::1]", // bracketed spelling (URL authority form)
            "[fe80::1]",
            "100.100.100.200", // CGN (Alibaba metadata)
        ] {
            assert!(host_is_blocked(h), "{h} must be floor-blocked");
        }
    }

    #[test]
    fn host_oracle_admits_public_hosts_and_stays_silent_on_globs() {
        for h in [
            "api.example.com",
            "example.com.", // trailing-dot public FQDN
            "8.8.8.8",
            "[2606:4700:4700::1111]",
            "localhost.example.com", // `localhost` as a NON-last label is a real name
            "*.github.com",          // a permits glob is not a host — never classified
            "",
        ] {
            assert!(!host_is_blocked(h), "{h} must not be floor-blocked");
        }
    }

    #[test]
    fn documentation_hosts_are_public_but_not_live_apis() {
        for h in [
            "example.com",
            "api.example.com",
            "EXAMPLE.COM.",
            "example.net",
            "foo.example.org",
            "example.edu",
            "foo.invalid",
            "bar.test",
            "doc.example",
        ] {
            assert!(is_documentation_host(h), "{h} is a documentation host");
            assert!(!host_is_blocked(h), "{h} stays off the SSRF floor");
        }
        for h in ["iana.org", "github.com", "harvard.edu", "localhost"] {
            assert!(!is_documentation_host(h), "{h} is not a documentation host");
        }
    }

    #[test]
    fn exact_loopback_literal_is_the_closed_declassification_vocabulary() {
        // The qualifying set (issue #395): the bare `localhost` name, a
        // 127.0.0.0/8 v4 literal, the v6 loopback `::1` (bracketed or
        // bare — the URL-authority spelling included).
        for e in [
            "localhost",
            "LOCALHOST",
            "localhost.", // absolute-FQDN spelling
            "127.0.0.1",
            "127.5.6.7", // any exact 127/8 literal
            "::1",
            "[::1]",
            "0:0:0:0:0:0:0:1", // the expanded spelling of ::1
        ] {
            assert!(is_exact_loopback_literal(e), "{e} must qualify");
        }
    }

    #[test]
    fn never_list_and_globs_never_qualify_as_loopback_literals() {
        // The NEVER list: globs, the `*.localhost` FAMILY (an exact literal
        // only — a subdomain reaches arbitrary services), RFC1918,
        // link-local/metadata, CGN, the unspecified addresses, NAT64/mapped
        // wrappers SMUGGLING loopback, and plain public hosts.
        for e in [
            "*.localhost",
            "*",
            "api.localhost", // the RFC 6761 family is NOT the bare name
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata IP
            "metadata.google.internal",
            "fe80::1",
            "fc00::1",
            "100.100.100.200",  // CGN (Alibaba metadata)
            "0.0.0.0",          // unspecified is NOT loopback
            "::",               // v6 unspecified
            "::ffff:127.0.0.1", // mapped wrapper — not the ::1 literal
            "64:ff9b::7f00:1",  // NAT64 wrapper smuggling 127.0.0.1
            "2130706433",       // decimal spelling — not an exact literal
            "127.0.0.1:8080",   // a host:port is not a host literal
            "example.com",
            "",
        ] {
            assert!(!is_exact_loopback_literal(e), "{e} must NOT qualify");
        }
    }

    #[test]
    fn declassification_clears_the_exact_host_only() {
        let perm = |entries: &[&str]| -> vec::Vec<String> {
            entries.iter().map(|e| (*e).to_string()).collect()
        };
        // The exact literal clears ITS host — in any equivalent spelling
        // pair (case · brackets · FQDN dot), and on no other host.
        assert!(loopback_declassified(&perm(&["127.0.0.1"]), "127.0.0.1"));
        assert!(loopback_declassified(&perm(&["localhost"]), "localhost"));
        assert!(loopback_declassified(&perm(&["LOCALHOST"]), "localhost."));
        assert!(loopback_declassified(&perm(&["[::1]"]), "::1"));
        assert!(loopback_declassified(&perm(&["::1"]), "[::1]"));
        // Exact means EXACT: `localhost` is not `127.0.0.1` (the
        // declassification is the literal in the file, never what it
        // resolves to), and 127/8 neighbours are distinct hosts.
        assert!(!loopback_declassified(&perm(&["localhost"]), "127.0.0.1"));
        assert!(!loopback_declassified(&perm(&["127.0.0.1"]), "localhost"));
        assert!(!loopback_declassified(&perm(&["127.0.0.1"]), "127.0.0.2"));
        // A non-loopback HOST is never declassifiable — even when the list
        // names it (the never-list holds from the host side too).
        assert!(!loopback_declassified(&perm(&["10.0.0.1"]), "10.0.0.1"));
        assert!(!loopback_declassified(
            &perm(&["169.254.169.254"]),
            "169.254.169.254"
        ));
        // The `*.localhost` family: neither a glob ENTRY nor a family HOST
        // ever participates.
        assert!(!loopback_declassified(
            &perm(&["*.localhost"]),
            "api.localhost"
        ));
        assert!(!loopback_declassified(
            &perm(&["api.localhost"]),
            "api.localhost"
        ));
        // Empty list declassifies nothing.
        let empty: vec::Vec<String> = vec![];
        assert!(!loopback_declassified(&empty, "localhost"));
    }
}
