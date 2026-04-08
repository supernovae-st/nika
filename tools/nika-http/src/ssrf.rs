// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! SSRF (Server-Side Request Forgery) protection.
//!
//! Blocks requests to private IP ranges, loopback addresses, and link-local.

use nika_kernel::http::HttpError;

/// Check whether a URL targets a private/internal address.
///
/// Blocks:
/// - 127.0.0.0/8 (loopback)
/// - 10.0.0.0/8 (Class A private)
/// - 172.16.0.0/12 (Class B private)
/// - 192.168.0.0/16 (Class C private)
/// - 169.254.0.0/16 (link-local)
/// - ::1 (IPv6 loopback)
/// - fc00::/7 (IPv6 unique local)
/// - fe80::/10 (IPv6 link-local)
/// - file:// scheme
/// - Hostnames: localhost, 0.0.0.0
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

    // Block known dangerous hostnames
    if matches!(host, "localhost" | "0.0.0.0" | "[::]" | "[::1]") {
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
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()           // 127.0.0.0/8
                || v4.is_private()     // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                || v4.is_link_local()  // 169.254.0.0/16
                || v4.is_unspecified() // 0.0.0.0
                || v4.is_broadcast()   // 255.255.255.255
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()           // ::1
                || v6.is_unspecified() // ::
                // fc00::/7 (unique local)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // fe80::/10 (link-local)
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
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
    fn blocks_ipv6_loopback() {
        assert!(check_url("http://[::1]/api").is_err());
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
    fn rejects_invalid_urls() {
        assert!(check_url("not a url").is_err());
    }
}
