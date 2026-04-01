//! HMAC-signed webhook notifications for job completion.
//!
//! When `NIKA_WEBHOOK_URL` is set, sends a POST request to the configured URL
//! when a job reaches a terminal state (completed, failed, cancelled).
//!
//! The request includes an `X-Nika-Signature` header with an HMAC-SHA256
//! signature of the body, using `NIKA_WEBHOOK_SECRET` as the key.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, warn};

type HmacSha256 = Hmac<Sha256>;

/// Webhook configuration, loaded from environment variables.
#[derive(Clone, Debug)]
pub struct WebhookConfig {
    pub url: String,
    pub secret: String,
    /// Pre-built HTTP client with pinned DNS to prevent DNS rebinding (M1).
    pub client: reqwest::Client,
}

impl WebhookConfig {
    /// Load webhook config from environment variables.
    ///
    /// Returns `None` if `NIKA_WEBHOOK_URL` is not set or fails SSRF validation.
    /// Sync version — validates URL string only (no DNS pinning).
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("NIKA_WEBHOOK_URL").ok()?;

        if let Err(reason) = validate_webhook_url(&url) {
            warn!(url = %url, "NIKA_WEBHOOK_URL blocked: {reason}");
            return None;
        }

        let secret = match std::env::var("NIKA_WEBHOOK_SECRET") {
            Ok(s) if !s.is_empty() => s,
            _ => {
                warn!(
                    "NIKA_WEBHOOK_URL is set but NIKA_WEBHOOK_SECRET is missing or empty — \
                     webhook delivery disabled. Set NIKA_WEBHOOK_SECRET to enable."
                );
                return None;
            }
        };

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Some(Self { url, secret, client })
    }

    /// Async version that resolves DNS and validates the resolved IP.
    /// Pins the resolved IP in the client to prevent DNS rebinding (M1).
    pub async fn resolve_and_pin(&mut self) {
        let host = extract_host(&self.url);
        if host.is_empty() {
            return;
        }

        // Skip DNS resolution for IP literals (already validated by SSRF checks)
        if host.parse::<std::net::Ipv4Addr>().is_ok()
            || host.parse::<std::net::Ipv6Addr>().is_ok()
        {
            return;
        }

        // Determine port from URL scheme
        let port = if self.url.starts_with("https://") {
            443
        } else {
            80
        };

        match tokio::net::lookup_host(format!("{host}:{port}")).await {
            Ok(addrs) => {
                let addrs: Vec<std::net::SocketAddr> = addrs.collect();
                // Validate all resolved IPs against SSRF
                for addr in &addrs {
                    if let Err(reason) = check_resolved_ip(addr.ip()) {
                        warn!(
                            url = %self.url,
                            ip = %addr.ip(),
                            "webhook DNS resolved to blocked IP: {reason}"
                        );
                        // Poison the config — clear URL so notify() becomes a no-op
                        self.url.clear();
                        return;
                    }
                }
                // Pin the first resolved IP in the client
                if let Some(first) = addrs.first() {
                    if let Ok(pinned) = reqwest::Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .resolve(&host, *first)
                        .build()
                    {
                        debug!(host = %host, ip = %first.ip(), "webhook DNS pinned");
                        self.client = pinned;
                    }
                }
            }
            Err(e) => {
                warn!(url = %self.url, error = %e, "webhook DNS resolution failed");
            }
        }
    }
}

/// Extract the host from a URL (after SSRF-safe parsing).
fn extract_host(url_str: &str) -> String {
    let after_scheme = url_str
        .strip_prefix("https://")
        .or_else(|| url_str.strip_prefix("http://"))
        .unwrap_or(url_str);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') {
        authority
            .strip_prefix('[')
            .and_then(|s| s.split(']').next())
            .unwrap_or(authority)
    } else {
        authority.split(':').next().unwrap_or(authority)
    }
    .to_lowercase()
}

/// Validate a resolved IP address against private/loopback/link-local ranges.
fn check_resolved_ip(ip: std::net::IpAddr) -> Result<(), String> {
    match ip {
        std::net::IpAddr::V4(v4) => check_ipv4(v4),
        std::net::IpAddr::V6(v6) => {
            if v6.is_loopback() {
                return Err("blocked IPv6 loopback".into());
            }
            if let Some(v4) = v6.to_ipv4_mapped() {
                return check_ipv4(v4);
            }
            let segments = v6.segments();
            if segments[0] & 0xffc0 == 0xfe80 {
                return Err("blocked IPv6 link-local".into());
            }
            Ok(())
        }
    }
}

/// Compute HMAC-SHA256 signature for a payload.
pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    let result = mac.finalize();
    let bytes = result.into_bytes();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256={hex}")
}

/// Send a webhook notification for a completed job.
///
/// Non-blocking: spawns a tokio task to send the request.
/// Failures are logged but do not affect job status.
pub fn notify(config: &WebhookConfig, job_id: &str, status: &str, output: Option<&str>) {
    if config.url.is_empty() {
        return; // Poisoned by resolve_and_pin (DNS resolved to private IP)
    }
    let url = config.url.clone();
    let secret = config.secret.clone();
    let client = config.client.clone();
    let body = serde_json::json!({
        "job_id": job_id,
        "status": status,
        "output": output,
    });
    let body_bytes = serde_json::to_vec(&body).unwrap_or_default();

    tokio::spawn(async move {
        let signature = sign(&secret, &body_bytes);

        match client
            .post(&url)
            .header("content-type", "application/json")
            .header("x-nika-signature", &signature)
            .body(body_bytes)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                debug!(
                    url = %url,
                    status = resp.status().as_u16(),
                    "webhook delivered"
                );
            }
            Err(e) => {
                warn!(url = %url, error = %e, "webhook delivery failed");
            }
        }
    });
}

/// Validate webhook URL against SSRF (private IPs, metadata endpoints).
///
/// Handles: userinfo (`user@host`), IPv6 brackets (`[::1]`), IPv6-mapped IPv4.
fn validate_webhook_url(url_str: &str) -> Result<(), String> {
    // Basic scheme check
    if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
        return Err("unsupported scheme — only http/https allowed".into());
    }

    // Extract authority: strip scheme, take everything before first /path
    let after_scheme = url_str
        .strip_prefix("https://")
        .or_else(|| url_str.strip_prefix("http://"))
        .unwrap_or(url_str);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);

    // C3: Strip userinfo (user:pass@host) — reqwest follows the real host
    let authority = authority.rsplit('@').next().unwrap_or(authority);

    // C2: Handle IPv6 bracket notation [::1]:port
    let host = if authority.starts_with('[') {
        // Extract content between [ and ]
        authority
            .strip_prefix('[')
            .and_then(|s| s.split(']').next())
            .unwrap_or(authority)
    } else {
        // Strip :port for non-bracketed hosts
        authority.split(':').next().unwrap_or(authority)
    }
    .to_lowercase();

    if host.is_empty() {
        return Err("URL has no host".into());
    }

    // Block cloud metadata endpoints
    if host == "169.254.169.254" || host == "metadata.google.internal" {
        return Err("blocked metadata endpoint".into());
    }

    // Check IPv4
    if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
        return check_ipv4(ip);
    }

    // Check IPv6 (including mapped IPv4)
    if let Ok(ip6) = host.parse::<std::net::Ipv6Addr>() {
        if ip6.is_loopback() {
            return Err("blocked IPv6 loopback".into());
        }
        // H1: Check IPv6-mapped IPv4 (::ffff:10.0.0.1)
        if let Some(ip4) = ip6.to_ipv4_mapped() {
            return check_ipv4(ip4);
        }
        // Block link-local IPv6 (fe80::/10)
        let segments = ip6.segments();
        if segments[0] & 0xffc0 == 0xfe80 {
            return Err("blocked IPv6 link-local".into());
        }
    }

    Ok(())
}

/// Check an IPv4 address against private/loopback/link-local ranges.
fn check_ipv4(ip: std::net::Ipv4Addr) -> Result<(), String> {
    let o = ip.octets();
    if o[0] == 127 {
        return Err("blocked loopback address".into());
    }
    if o[0] == 169 && o[1] == 254 {
        return Err("blocked link-local address".into());
    }
    if o[0] == 10 || (o[0] == 172 && (16..=31).contains(&o[1])) || (o[0] == 192 && o[1] == 168) {
        return Err("blocked private IP range".into());
    }
    if o[0] == 0 {
        return Err("blocked unspecified address".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_produces_sha256_prefix() {
        let sig = sign("my-secret", b"hello world");
        assert!(
            sig.starts_with("sha256="),
            "signature must start with sha256="
        );
        // sha256= prefix + 64 hex chars
        assert_eq!(sig.len(), 7 + 64);
    }

    #[test]
    fn sign_deterministic() {
        let a = sign("key", b"data");
        let b = sign("key", b"data");
        assert_eq!(a, b, "same key+data must produce same signature");
    }

    #[test]
    fn sign_different_keys_produce_different_sigs() {
        let a = sign("key1", b"data");
        let b = sign("key2", b"data");
        assert_ne!(a, b);
    }

    #[test]
    fn webhook_config_from_env_none_when_unset() {
        // No NIKA_WEBHOOK_URL set — should return None
        std::env::remove_var("NIKA_WEBHOOK_URL");
        assert!(WebhookConfig::from_env().is_none());
    }

    #[test]
    fn webhook_config_none_when_secret_missing() {
        std::env::set_var("NIKA_WEBHOOK_URL", "https://hooks.example.com/test");
        std::env::remove_var("NIKA_WEBHOOK_SECRET");
        let config = WebhookConfig::from_env();
        assert!(config.is_none(), "must refuse webhook without secret");
        std::env::remove_var("NIKA_WEBHOOK_URL");
    }

    #[test]
    fn ssrf_blocks_metadata() {
        assert!(validate_webhook_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_webhook_url("http://metadata.google.internal/").is_err());
    }

    #[test]
    fn ssrf_blocks_private_ips() {
        assert!(validate_webhook_url("http://10.0.0.1/hook").is_err());
        assert!(validate_webhook_url("http://172.16.0.1/hook").is_err());
        assert!(validate_webhook_url("http://192.168.1.1/hook").is_err());
        assert!(validate_webhook_url("http://127.0.0.1/hook").is_err());
    }

    #[test]
    fn ssrf_blocks_ipv6_bracket_bypass() {
        // C2: bracketed IPv6 must be parsed and checked
        assert!(validate_webhook_url("http://[::1]/hook").is_err());
        assert!(validate_webhook_url("http://[::1]:8080/hook").is_err());
    }

    #[test]
    fn ssrf_blocks_userinfo_bypass() {
        // C3: userinfo@host — reqwest follows the real host
        assert!(validate_webhook_url("http://public@169.254.169.254/meta").is_err());
        assert!(validate_webhook_url("http://user:pass@10.0.0.1/hook").is_err());
        assert!(validate_webhook_url("http://safe.com@127.0.0.1/hook").is_err());
    }

    #[test]
    fn ssrf_blocks_ipv6_mapped_ipv4() {
        // H1: ::ffff:10.0.0.1 maps to private IPv4
        assert!(validate_webhook_url("http://[::ffff:127.0.0.1]/hook").is_err());
        assert!(validate_webhook_url("http://[::ffff:10.0.0.1]/hook").is_err());
        assert!(validate_webhook_url("http://[::ffff:169.254.169.254]/meta").is_err());
    }

    #[test]
    fn ssrf_allows_public_urls() {
        assert!(validate_webhook_url("https://hooks.slack.com/services/xxx").is_ok());
        assert!(validate_webhook_url("https://api.example.com/webhook").is_ok());
        assert!(validate_webhook_url("https://api.example.com:8443/hook").is_ok());
    }
}
