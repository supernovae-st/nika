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

/// Host glob match — exact, or a LEADING `*.` subdomain wildcard
/// (`*.github.com` matches `api.github.com` AND the bare `github.com`).
/// Distinct from the tool-id trailing-`*` glob (`nika:*`).
#[must_use]
pub fn host_glob_matches(glob: &str, host: &str) -> bool {
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
    // a hostless URL has no extractable host (a declared boundary denies it)
    ("mailto:user@example.com", None),
];

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
}
