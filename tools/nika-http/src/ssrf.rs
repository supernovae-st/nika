// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! SSRF (Server-Side Request Forgery) protection.
//!
//! Blocks requests to private IP ranges, loopback addresses, link-local,
//! IPv4-mapped IPv6, cloud metadata endpoints, and CGN ranges.

use nika_kernel::http::HttpError;

/// Hostnames that are always blocked (cloud metadata, loopback aliases).
const BLOCKED_HOSTNAMES: &[&str] = &[
    "localhost",
    "0.0.0.0",
    "[::]",
    "[::1]",
    // Cloud metadata endpoints
    "metadata.google.internal",
    "metadata.goog",
    "169.254.169.254", // AWS/GCP/Azure metadata IP (also caught by link-local check)
];

/// Check whether a URL targets a private/internal address.
///
/// Blocks:
/// - 127.0.0.0/8 (loopback)
/// - 10.0.0.0/8 (Class A private)
/// - 172.16.0.0/12 (Class B private)
/// - 192.168.0.0/16 (Class C private)
/// - 169.254.0.0/16 (link-local)
/// - 100.64.0.0/10 (CGN / shared address space — Alibaba Cloud metadata)
/// - ::1 (IPv6 loopback)
/// - ::ffff:127.0.0.1 (IPv4-mapped IPv6 — re-checks inner v4)
/// - fc00::/7 (IPv6 unique local)
/// - fe80::/10 (IPv6 link-local)
/// - file:// scheme
/// - Cloud metadata hostnames (metadata.google.internal, etc.)
pub fn check_url(url: &str) -> Result<(), HttpError> {
    let parsed = url::Url::parse(url).map_err(|e| HttpError::Other {
        reason: format!("invalid URL: {e}"),
    })?;

    // Block non-HTTP schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(HttpError::SsrfBlocked {
                url: format!("blocked scheme: {scheme}"),
            });
        }
    }

    let host = parsed.host_str().unwrap_or("");
    let host_lower = host.to_lowercase();

    // Block known dangerous hostnames
    if BLOCKED_HOSTNAMES
        .iter()
        .any(|blocked| host_lower == *blocked)
    {
        return Err(HttpError::SsrfBlocked {
            url: url.to_string(),
        });
    }

    // Parse as IP and check ranges
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_private_ip(ip) {
            return Err(HttpError::SsrfBlocked {
                url: url.to_string(),
            });
        }
    }

    // Also handle bracketed IPv6
    let trimmed = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        if is_private_ip(ip) {
            return Err(HttpError::SsrfBlocked {
                url: url.to_string(),
            });
        }
    }

    Ok(())
}

fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => is_private_v4(v4),
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
                || v6.is_unspecified() // ::
                // fc00::/7 (unique local)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 (link-local)
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped IPv6 (::ffff:a.b.c.d) — re-check inner v4
                || v6.to_ipv4_mapped().is_some_and(is_private_v4)
                // IPv4-compatible IPv6 (::a.b.c.d, deprecated) — same check
                || v6.to_ipv4().is_some_and(is_private_v4)
        }
    }
}

fn is_private_v4(v4: std::net::Ipv4Addr) -> bool {
    v4.is_loopback()           // 127.0.0.0/8
        || v4.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
        || v4.is_link_local()  // 169.254.0.0/16
        || v4.is_unspecified() // 0.0.0.0
        || v4.is_broadcast()   // 255.255.255.255
        // 100.64.0.0/10 (CGN / shared address space — Alibaba Cloud metadata at 100.100.100.200)
        || { let o = v4.octets(); o[0] == 100 && (64..=127).contains(&o[1]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_localhost() {
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
    fn blocks_cgn_range() {
        // 100.64.0.0/10 — Carrier-Grade NAT / shared address space
        assert!(check_url("http://100.64.0.1/api").is_err());
        assert!(check_url("http://100.100.100.200/latest/meta-data/").is_err()); // Alibaba Cloud
        assert!(check_url("http://100.127.255.255/api").is_err());
        // 100.128.0.0 is outside CGN range — should be allowed
        assert!(check_url("http://100.128.0.1/api").is_ok());
    }

    #[test]
    fn blocks_ipv6_loopback() {
        assert!(check_url("http://[::1]/api").is_err());
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6() {
        // ::ffff:127.0.0.1 — IPv4-mapped IPv6 loopback
        assert!(check_url("http://[::ffff:127.0.0.1]/api").is_err());
        // ::ffff:10.0.0.1 — IPv4-mapped private
        assert!(check_url("http://[::ffff:10.0.0.1]/api").is_err());
        // ::ffff:169.254.169.254 — IPv4-mapped link-local (metadata)
        assert!(check_url("http://[::ffff:169.254.169.254]/api").is_err());
        // ::ffff:100.100.100.200 — IPv4-mapped CGN
        assert!(check_url("http://[::ffff:100.100.100.200]/api").is_err());
    }

    #[test]
    fn blocks_cloud_metadata_hostnames() {
        assert!(check_url("http://metadata.google.internal/computeMetadata/v1/").is_err());
        assert!(check_url("http://metadata.goog/computeMetadata/v1/").is_err());
    }

    #[test]
    fn blocks_file_scheme() {
        assert!(check_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn allows_public_urls() {
        assert!(check_url("https://api.anthropic.com/v1/messages").is_ok());
        assert!(check_url("https://example.com").is_ok());
        assert!(check_url("http://8.8.8.8/dns").is_ok());
    }

    #[test]
    fn allows_public_ipv6() {
        // 2001:db8:: is documentation range but not private in our check
        assert!(check_url("http://[2607:f8b0:4004:800::200e]/").is_ok()); // Google public
    }

    #[test]
    fn rejects_invalid_urls() {
        assert!(check_url("not a url").is_err());
    }
}
