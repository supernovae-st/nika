//! Policy Enforcer - Security policy enforcement
//!
//! Enforces allow/block rules for:
//! - Shell commands (exec: verb)
//! - Network access (fetch: verb)
//! - Token budget limits
//! - Host restrictions

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::NikaError;
use crate::runtime::boot::PolicyConfig;
use url::Url;

/// Exact-match SSRF blocklist: cloud metadata hostnames and special addresses.
///
/// These are ALWAYS blocked regardless of user configuration.
/// Cloud metadata services (AWS/GCP/Alibaba) and special addresses are
/// common SSRF targets that should never be reachable from workflow fetch: verbs.
const SSRF_BLOCKED_EXACT: &[&str] = &["metadata.google.internal", "localhost", "0.0.0.0"];

/// Check whether a host (already lowercased, brackets stripped) is SSRF-blocked.
///
/// 1. Exact-match against known hostnames (metadata.google.internal, localhost, 0.0.0.0).
/// 2. Parse as IP and check private/reserved CIDR ranges:
///    - 127.0.0.0/8       (loopback)
///    - 10.0.0.0/8        (private class A)
///    - 172.16.0.0/12     (private class B)
///    - 192.168.0.0/16    (private class C)
///    - 169.254.0.0/16    (link-local, includes AWS metadata 169.254.169.254)
///    - 100.64.0.0/10     (CGN / shared, includes Alibaba 100.100.100.200)
///    - ::1               (IPv6 loopback)
///    - ::ffff:0:0/96     (IPv4-mapped IPv6 — re-checks the inner v4 address)
pub(crate) fn is_ssrf_blocked(host: &str) -> bool {
    // 1. Exact hostname match
    if SSRF_BLOCKED_EXACT.contains(&host) {
        return true;
    }

    // 2. Try to parse as IP
    let ip: IpAddr = match host.parse() {
        Ok(addr) => addr,
        Err(_) => return false, // Not an IP, and not in exact list — allow
    };

    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => {
            // IPv6 unspecified (::) — equivalent to 0.0.0.0
            if v6 == Ipv6Addr::UNSPECIFIED {
                return true;
            }
            // IPv6 loopback
            if v6 == Ipv6Addr::LOCALHOST {
                return true;
            }
            // fe80::/10 — link-local (IPv6 equivalent of 169.254.0.0/16)
            if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                return true;
            }
            // fc00::/7 — unique local address (IPv6 equivalent of RFC 1918)
            if (v6.segments()[0] & 0xfe00) == 0xfc00 {
                return true;
            }
            // IPv4-mapped IPv6 (::ffff:a.b.c.d) — extract and re-check inner v4
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_blocked_v4(mapped);
            }
            // IPv4-compatible IPv6 (::a.b.c.d, deprecated RFC 4291) — extract inner v4.
            // Catches bypass attempts like ::127.0.0.1, ::169.254.169.254.
            // to_ipv4() covers both mapped and compatible; mapped is handled above.
            if let Some(compat) = v6.to_ipv4() {
                return is_blocked_v4(compat);
            }
            false
        }
    }
}

/// Resolve a hostname to IP addresses and check each against SSRF blocklist.
///
/// Defends against DNS rebinding: a hostname passes the string-level check
/// but resolves to a private/link-local IP at connect time.
///
/// Returns `true` if ANY resolved IP is blocked.
pub(crate) async fn resolve_and_check_ssrf(host: &str) -> bool {
    resolve_and_pin_ssrf(host).await.is_err()
}

/// Build a redirect policy that checks each hop against the SSRF blocklist.
///
/// `allowed_hosts` are exempted from blocking (e.g., explicitly whitelisted private hosts).
/// Stops after `max_redirects` hops regardless of SSRF status.
pub fn ssrf_safe_redirect_policy(
    allowed_hosts: Vec<String>,
    max_redirects: usize,
) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= max_redirects {
            attempt.stop()
        } else {
            let blocked = attempt.url().host_str().and_then(|host| {
                let h = host.to_lowercase();
                let h_normalized = h.trim_start_matches('[').trim_end_matches(']');
                let explicitly_allowed = allowed_hosts
                    .iter()
                    .any(|a| h_normalized == a.to_lowercase());
                if !explicitly_allowed && is_ssrf_blocked(h_normalized) {
                    Some(h)
                } else {
                    None
                }
            });
            if let Some(host) = blocked {
                attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("SSRF protection: redirect to '{}' blocked", host),
                ))
            } else {
                attempt.follow()
            }
        }
    })
}

/// Resolve DNS for SSRF check and return pinned addresses.
///
/// Returns `Ok(Vec<SocketAddr>)` if all resolved IPs are safe (can be used for pinning).
/// Returns `Err(reason)` if any resolved IP is blocked or resolution fails.
/// DNS pinning prevents TOCTOU rebinding: the resolved IPs from this check
/// are reused for the actual HTTP request via reqwest's `.resolve()`.
pub(crate) async fn resolve_and_pin_ssrf(host: &str) -> Result<Vec<std::net::SocketAddr>, String> {
    // Skip raw IPs — they were already checked by the string-level check_fetch().
    if host.parse::<IpAddr>().is_ok() {
        return Ok(vec![]);
    }

    // Attempt DNS resolution with timeout
    let lookup = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::lookup_host(format!("{host}:0")),
    )
    .await;

    match lookup {
        Ok(Ok(addrs)) => {
            let addrs: Vec<std::net::SocketAddr> = addrs.collect();
            for addr in &addrs {
                let ip_str = addr.ip().to_string();
                if is_ssrf_blocked(&ip_str) {
                    tracing::warn!(
                        host = %host,
                        resolved_ip = %ip_str,
                        "DNS rebinding SSRF blocked: hostname resolved to private IP"
                    );
                    return Err(format!(
                        "DNS rebinding SSRF: '{}' resolved to blocked IP {}",
                        host, ip_str
                    ));
                }
            }
            Ok(addrs)
        }
        Ok(Err(e)) => {
            tracing::warn!(host = %host, error = %e, "DNS resolution failed for SSRF check — blocking (fail-closed)");
            Err(format!("DNS resolution failed: {e}"))
        }
        Err(_) => {
            tracing::warn!(host = %host, "DNS resolution timed out (3s) for SSRF check — blocking (fail-closed)");
            Err("DNS resolution timed out (3s)".to_string())
        }
    }
}

/// Check an IPv4 address against blocked private/reserved ranges.
fn is_blocked_v4(v4: Ipv4Addr) -> bool {
    let octets = v4.octets();
    // 127.0.0.0/8 — loopback
    if octets[0] == 127 {
        return true;
    }
    // 10.0.0.0/8 — private class A
    if octets[0] == 10 {
        return true;
    }
    // 172.16.0.0/12 — private class B (172.16.x.x – 172.31.x.x)
    if octets[0] == 172 && (16..=31).contains(&octets[1]) {
        return true;
    }
    // 192.168.0.0/16 — private class C
    if octets[0] == 192 && octets[1] == 168 {
        return true;
    }
    // 169.254.0.0/16 — link-local (covers AWS metadata 169.254.169.254)
    if octets[0] == 169 && octets[1] == 254 {
        return true;
    }
    // 100.64.0.0/10 — CGN / shared address space (covers Alibaba 100.100.100.200)
    // 100.64.0.0 – 100.127.255.255
    if octets[0] == 100 && (64..=127).contains(&octets[1]) {
        return true;
    }
    // 0.0.0.0
    if v4 == Ipv4Addr::UNSPECIFIED {
        return true;
    }
    false
}

/// Policy enforcement result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Action is allowed
    Allow,
    /// Action is blocked with reason
    Block(String),
    /// Action requires user confirmation
    RequiresApproval(String),
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }

    pub fn is_blocked(&self) -> bool {
        matches!(self, Self::Block(_))
    }
}

/// Token budget tracker
#[derive(Debug, Clone, Default)]
pub struct TokenBudget {
    pub limit: Option<u64>,
    pub used: u64,
}

impl TokenBudget {
    pub fn new(limit: Option<u64>) -> Self {
        Self { limit, used: 0 }
    }

    /// Check if spending tokens would exceed budget
    pub fn can_spend(&self, tokens: u64) -> bool {
        match self.limit {
            Some(limit) => self
                .used
                .checked_add(tokens)
                .is_some_and(|total| total <= limit),
            None => true,
        }
    }

    /// Record token usage (saturating to prevent u64 overflow)
    pub fn spend(&mut self, tokens: u64) {
        self.used = self.used.saturating_add(tokens);
    }

    /// Remaining budget
    pub fn remaining(&self) -> Option<u64> {
        self.limit.map(|l| l.saturating_sub(self.used))
    }
}

/// Policy enforcer instance
#[derive(Debug, Clone)]
pub struct PolicyEnforcer {
    config: PolicyConfig,
    token_budget: TokenBudget,
}

impl Default for PolicyEnforcer {
    fn default() -> Self {
        Self::new(PolicyConfig::default())
    }
}

impl PolicyEnforcer {
    /// Create a new policy enforcer with configuration
    pub fn new(config: PolicyConfig) -> Self {
        let token_budget = TokenBudget::new(config.max_token_spend);
        Self {
            config,
            token_budget,
        }
    }

    /// Get the list of explicitly allowed hosts (overrides SSRF blocklist).
    pub fn allowed_hosts(&self) -> &[String] {
        &self.config.allowed_hosts
    }

    /// Check if the exec verb is allowed for a command
    pub fn check_exec(&self, command: &str) -> PolicyDecision {
        // Check if exec is globally disabled
        if !self.config.allow_exec {
            return PolicyDecision::Block("exec: verb is disabled by policy".into());
        }

        // Check for blocked command patterns
        let command_lower = command.to_lowercase();
        for blocked in &self.config.blocked_commands {
            if command_lower.contains(&blocked.to_lowercase()) {
                return PolicyDecision::Block(format!(
                    "Command contains blocked pattern: '{}'",
                    blocked
                ));
            }
        }

        PolicyDecision::Allow
    }

    /// Check if fetch: verb is allowed for a URL
    pub fn check_fetch(&self, url: &str) -> PolicyDecision {
        // Check if network is globally disabled
        if !self.config.allow_network {
            return PolicyDecision::Block(
                "fetch: verb (network access) is disabled by policy".into(),
            );
        }

        // Parse URL to check host — fail-closed on invalid URLs
        let parsed = match Url::parse(url) {
            Ok(u) => u,
            Err(_) => {
                return PolicyDecision::Block(format!(
                    "Unparseable URL rejected (fail-closed): '{}'",
                    url
                ));
            }
        };

        let host = match parsed.host_str() {
            Some(h) => h.to_lowercase(),
            None => {
                return PolicyDecision::Block(format!("URL has no host (fail-closed): '{}'", url));
            }
        };

        // Normalize IPv6: url crate returns "[::1]" with brackets, blocklist uses "::1"
        let host_normalized = host.trim_start_matches('[').trim_end_matches(']');

        // SSRF protection: block cloud metadata, loopback, and private ranges.
        // Exception: explicit allowed_hosts override SSRF blocklist (for testing, local services)
        let explicitly_allowed = self
            .config
            .allowed_hosts
            .iter()
            .any(|allowed| host_normalized == allowed.to_lowercase());
        if !explicitly_allowed && is_ssrf_blocked(host_normalized) {
            return PolicyDecision::Block(format!(
                "SSRF protection: access to '{}' is blocked",
                host
            ));
        }

        // Check blocked hosts first (takes precedence).
        // Uses proper domain-suffix matching: "evil.com" blocks "evil.com" and
        // "sub.evil.com" but NOT "not-evil.com".
        for blocked in &self.config.blocked_hosts {
            let blocked_lower = blocked.to_lowercase();
            if host == blocked_lower || host.ends_with(&format!(".{}", blocked_lower)) {
                return PolicyDecision::Block(format!("Host '{}' is blocked by policy", host));
            }
        }

        // If allowed_hosts is non-empty, only those hosts are allowed.
        // Uses proper domain-suffix matching: "openai.com" allows "openai.com"
        // and "api.openai.com" but NOT "openai.com.evil.com".
        if !self.config.allowed_hosts.is_empty() {
            let is_allowed = self.config.allowed_hosts.iter().any(|allowed| {
                let allowed_lower = allowed.to_lowercase();
                host == allowed_lower || host.ends_with(&format!(".{}", allowed_lower))
            });
            if !is_allowed {
                return PolicyDecision::Block(format!(
                    "Host '{}' is not in allowed hosts list",
                    host
                ));
            }
        }

        PolicyDecision::Allow
    }

    /// Async version of check_fetch that also performs DNS resolution checks.
    ///
    /// Call this instead of check_fetch() for the main fetch: verb request.
    /// Adds DNS rebinding protection on top of string-level SSRF checks.
    pub async fn check_fetch_async(&self, url: &str) -> PolicyDecision {
        // Run all string-level checks first
        let string_decision = self.check_fetch(url);
        if !string_decision.is_allowed() {
            return string_decision;
        }

        // Resolve hostname and check resolved IPs against SSRF blocklist
        if let Ok(parsed) = Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                let h = host.to_lowercase();
                let h = h.trim_start_matches('[').trim_end_matches(']');
                if resolve_and_check_ssrf(h).await {
                    return PolicyDecision::Block(format!(
                        "DNS rebinding SSRF: '{}' resolved to blocked IP",
                        host
                    ));
                }
            }
        }

        PolicyDecision::Allow
    }

    /// Check if token spend is within budget
    pub fn check_token_spend(&self, tokens: u64) -> PolicyDecision {
        if !self.token_budget.can_spend(tokens) {
            let remaining = self.token_budget.remaining().unwrap_or(0);
            return PolicyDecision::Block(format!(
                "Token budget exceeded: requested {} but only {} remaining",
                tokens, remaining
            ));
        }
        PolicyDecision::Allow
    }

    /// Atomically reserve tokens from the budget (check + spend in one call).
    ///
    /// Prevents TOCTOU races where concurrent for_each tasks all pass the
    /// check before any records spending.
    pub fn reserve_tokens(&mut self, estimated: u64) -> Result<(), String> {
        if !self.token_budget.can_spend(estimated) {
            return Err(format!(
                "Token budget exceeded: {} used + {} estimated > {} limit",
                self.token_budget.used,
                estimated,
                self.token_budget.limit.unwrap_or(u64::MAX),
            ));
        }
        self.token_budget.spend(estimated);
        Ok(())
    }

    /// Adjust a previous reservation to match actual token usage.
    pub fn adjust_reservation(&mut self, estimated: u64, actual: u64) {
        if actual < estimated {
            self.token_budget.used = self.token_budget.used.saturating_sub(estimated - actual);
        } else if actual > estimated {
            self.token_budget.spend(actual - estimated);
        }
    }

    /// Record token usage
    pub fn record_token_spend(&mut self, tokens: u64) {
        self.token_budget.spend(tokens);
    }

    /// Get remaining token budget
    pub fn remaining_budget(&self) -> Option<u64> {
        self.token_budget.remaining()
    }

    /// Get total tokens used
    pub fn tokens_used(&self) -> u64 {
        self.token_budget.used
    }

    /// Add a host to the allowed_hosts list (for auto-allowing custom endpoints).
    ///
    /// Returns `true` if the host was added (not already present), `false` if duplicate.
    pub fn add_allowed_host(&mut self, host: &str) -> bool {
        if self.config.allowed_hosts.contains(&host.to_string()) {
            return false;
        }
        self.config.allowed_hosts.push(host.to_string());
        true
    }

    /// Check if an agent tool call is allowed.
    ///
    /// Enforces:
    /// - `allow_exec: false` blocks tools with exec/run/shell in the name
    /// - `allow_network: false` blocks tools with fetch/http/request in the name
    /// - `blocked_commands` patterns checked against tool name
    pub fn check_tool_call(&self, tool_name: &str) -> PolicyDecision {
        let lower = tool_name.to_lowercase();

        // Block exec-like tools when exec is disabled
        if !self.config.allow_exec {
            for pattern in &["exec", "run_command", "shell", "terminal", "bash"] {
                if lower.contains(pattern) {
                    return PolicyDecision::Block(format!(
                        "Agent tool '{}' blocked: exec is disabled by policy",
                        tool_name
                    ));
                }
            }
        }

        // Block network-like tools when network is disabled
        if !self.config.allow_network {
            for pattern in &["fetch", "http", "request", "curl", "download"] {
                if lower.contains(pattern) {
                    return PolicyDecision::Block(format!(
                        "Agent tool '{}' blocked: network access is disabled by policy",
                        tool_name
                    ));
                }
            }
        }

        // Check blocked_commands patterns against tool name
        for blocked in &self.config.blocked_commands {
            if lower.contains(&blocked.to_lowercase()) {
                return PolicyDecision::Block(format!(
                    "Agent tool '{}' blocked: matches blocked pattern '{}'",
                    tool_name, blocked
                ));
            }
        }

        PolicyDecision::Allow
    }

    /// Convert policy decision to result
    pub fn enforce(&self, decision: PolicyDecision) -> Result<(), NikaError> {
        match decision {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Block(reason) => Err(NikaError::PolicyViolation { reason }),
            PolicyDecision::RequiresApproval(reason) => {
                // For now, treat as block. HITL integration can handle approval flow.
                Err(NikaError::PolicyViolation {
                    reason: format!("Requires approval: {}", reason),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_policy_allows_exec() {
        let enforcer = PolicyEnforcer::default();
        assert!(enforcer.check_exec("ls -la").is_allowed());
    }

    #[test]
    fn test_policy_blocks_dangerous_commands() {
        let enforcer = PolicyEnforcer::default();

        // Default blocked commands
        assert!(enforcer.check_exec("sudo apt install").is_blocked());
        assert!(enforcer.check_exec("rm -rf /").is_blocked());
        assert!(enforcer.check_exec("chmod 777 /etc").is_blocked());

        // Safe commands allowed
        assert!(enforcer.check_exec("echo hello").is_allowed());
        assert!(enforcer.check_exec("npm run build").is_allowed());
    }

    #[test]
    fn test_policy_disables_exec() {
        let config = PolicyConfig {
            allow_exec: false,
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);

        assert!(enforcer.check_exec("echo hello").is_blocked());
    }

    #[test]
    fn test_default_policy_allows_fetch() {
        let enforcer = PolicyEnforcer::default();
        assert!(enforcer
            .check_fetch("https://api.example.com/data")
            .is_allowed());
    }

    #[test]
    fn test_policy_disables_network() {
        let config = PolicyConfig {
            allow_network: false,
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);

        assert!(enforcer.check_fetch("https://example.com").is_blocked());
    }

    #[test]
    fn test_policy_blocks_hosts() {
        let config = PolicyConfig {
            blocked_hosts: vec!["evil.com".into(), "malware.io".into()],
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);

        assert!(enforcer.check_fetch("https://evil.com/path").is_blocked());
        assert!(enforcer
            .check_fetch("https://sub.evil.com/path")
            .is_blocked());
        assert!(enforcer.check_fetch("https://malware.io/api").is_blocked());
        assert!(enforcer.check_fetch("https://api.example.com").is_allowed());
    }

    #[test]
    fn test_policy_allowed_hosts_whitelist() {
        let config = PolicyConfig {
            allowed_hosts: vec!["api.openai.com".into(), "anthropic.com".into()],
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);

        assert!(enforcer
            .check_fetch("https://api.openai.com/v1")
            .is_allowed());
        assert!(enforcer
            .check_fetch("https://anthropic.com/api")
            .is_allowed());
        assert!(enforcer.check_fetch("https://other.com/api").is_blocked());
    }

    #[test]
    fn test_token_budget_unlimited() {
        let budget = TokenBudget::new(None);
        assert!(budget.can_spend(1_000_000));
        assert!(budget.remaining().is_none());
    }

    #[test]
    fn test_token_budget_limited() {
        let mut budget = TokenBudget::new(Some(10000));
        assert!(budget.can_spend(5000));
        budget.spend(5000);
        assert_eq!(budget.used, 5000);
        assert_eq!(budget.remaining(), Some(5000));

        assert!(budget.can_spend(5000));
        assert!(!budget.can_spend(5001));
    }

    #[test]
    fn test_enforcer_token_budget() {
        let config = PolicyConfig {
            max_token_spend: Some(1000),
            ..Default::default()
        };
        let mut enforcer = PolicyEnforcer::new(config);

        assert!(enforcer.check_token_spend(500).is_allowed());
        enforcer.record_token_spend(500);

        assert!(enforcer.check_token_spend(500).is_allowed());
        enforcer.record_token_spend(500);

        // Now at limit
        assert!(enforcer.check_token_spend(1).is_blocked());
        assert_eq!(enforcer.remaining_budget(), Some(0));
    }

    #[test]
    fn test_policy_decision_properties() {
        let allow = PolicyDecision::Allow;
        let block = PolicyDecision::Block("reason".into());

        assert!(allow.is_allowed());
        assert!(!allow.is_blocked());
        assert!(block.is_blocked());
        assert!(!block.is_allowed());
    }

    // =========================================================================
    // Regression: Bug 18 — unparseable URLs must be blocked (fail-closed)
    // =========================================================================

    #[test]
    fn test_policy_blocks_unparseable_url() {
        let enforcer = PolicyEnforcer::default();
        let decision = enforcer.check_fetch("not a url at all %%%");
        assert!(
            decision.is_blocked(),
            "Unparseable URL should be blocked (fail-closed), got: {:?}",
            decision
        );
    }

    #[test]
    fn test_policy_blocks_url_without_host() {
        let enforcer = PolicyEnforcer::default();
        // data: URIs have no host
        let decision = enforcer.check_fetch("data:text/html,<script>alert(1)</script>");
        assert!(
            decision.is_blocked(),
            "URL without host should be blocked (fail-closed), got: {:?}",
            decision
        );
    }

    #[test]
    fn test_policy_still_allows_valid_urls() {
        let enforcer = PolicyEnforcer::default();
        assert!(enforcer.check_fetch("https://example.com/api").is_allowed());
    }

    // =========================================================================
    // SSRF protection: cloud metadata + loopback always blocked
    // =========================================================================

    #[test]
    fn test_ssrf_blocks_cloud_metadata() {
        let enforcer = PolicyEnforcer::default();

        // AWS/GCP metadata endpoint
        assert!(enforcer
            .check_fetch("http://169.254.169.254/latest/meta-data/")
            .is_blocked());
        // GCP internal DNS
        assert!(enforcer
            .check_fetch("http://metadata.google.internal/computeMetadata/v1/")
            .is_blocked());
        // Alibaba metadata
        assert!(enforcer
            .check_fetch("http://100.100.100.200/latest/meta-data/")
            .is_blocked());
    }

    #[test]
    fn test_ssrf_blocks_loopback() {
        let enforcer = PolicyEnforcer::default();

        assert!(enforcer.check_fetch("http://localhost:8080").is_blocked());
        assert!(enforcer
            .check_fetch("http://127.0.0.1:3000/api")
            .is_blocked());
        assert!(enforcer
            .check_fetch("http://[::1]:9090/health")
            .is_blocked());
        assert!(enforcer.check_fetch("http://0.0.0.0/admin").is_blocked());
    }

    #[test]
    fn test_ssrf_does_not_block_external_hosts() {
        let enforcer = PolicyEnforcer::default();
        assert!(enforcer
            .check_fetch("https://api.openai.com/v1")
            .is_allowed());
        assert!(enforcer.check_fetch("https://example.com").is_allowed());
    }

    // =========================================================================
    // H4: SSRF blocks private/reserved IP ranges
    // =========================================================================

    #[test]
    fn test_ssrf_blocks_private_ranges() {
        let enforcer = PolicyEnforcer::default();

        // 10.0.0.0/8
        assert!(enforcer.check_fetch("http://10.0.0.1/admin").is_blocked());
        assert!(enforcer.check_fetch("http://10.255.255.255/x").is_blocked());

        // 172.16.0.0/12
        assert!(enforcer.check_fetch("http://172.16.0.1/api").is_blocked());
        assert!(enforcer.check_fetch("http://172.31.255.255/x").is_blocked());
        // 172.15.x.x is NOT private — should be allowed
        assert!(enforcer.check_fetch("http://172.15.0.1/api").is_allowed());
        // 172.32.x.x is NOT private — should be allowed
        assert!(enforcer.check_fetch("http://172.32.0.1/api").is_allowed());

        // 192.168.0.0/16
        assert!(enforcer
            .check_fetch("http://192.168.1.1/admin")
            .is_blocked());
        assert!(enforcer.check_fetch("http://192.168.0.100/x").is_blocked());

        // 127.0.0.0/8 — full loopback range
        assert!(enforcer.check_fetch("http://127.0.0.2:8080/x").is_blocked());
        assert!(enforcer
            .check_fetch("http://127.255.255.255/x")
            .is_blocked());

        // 169.254.0.0/16 — link-local
        assert!(enforcer.check_fetch("http://169.254.0.1/x").is_blocked());
        assert!(enforcer
            .check_fetch("http://169.254.169.254/latest")
            .is_blocked());

        // 100.64.0.0/10 — CGN / shared (covers Alibaba 100.100.100.200)
        assert!(enforcer.check_fetch("http://100.64.0.1/x").is_blocked());
        assert!(enforcer
            .check_fetch("http://100.100.100.200/meta")
            .is_blocked());
        assert!(enforcer
            .check_fetch("http://100.127.255.255/x")
            .is_blocked());
        // 100.128.x.x is outside CGN — should be allowed
        assert!(enforcer.check_fetch("http://100.128.0.1/api").is_allowed());
    }

    #[test]
    fn test_ssrf_blocks_ipv6_mapped() {
        let enforcer = PolicyEnforcer::default();

        // ::ffff:127.0.0.1 (IPv4-mapped loopback)
        assert!(enforcer
            .check_fetch("http://[::ffff:127.0.0.1]:8080/x")
            .is_blocked());
        // ::ffff:10.0.0.1 (IPv4-mapped private)
        assert!(enforcer
            .check_fetch("http://[::ffff:10.0.0.1]/admin")
            .is_blocked());
        // ::ffff:192.168.1.1
        assert!(enforcer
            .check_fetch("http://[::ffff:192.168.1.1]/x")
            .is_blocked());
        // ::ffff:169.254.169.254
        assert!(enforcer
            .check_fetch("http://[::ffff:169.254.169.254]/meta")
            .is_blocked());

        // ::1 (pure IPv6 loopback)
        assert!(enforcer
            .check_fetch("http://[::1]:9090/health")
            .is_blocked());

        // fe80::1 (IPv6 link-local)
        assert!(enforcer
            .check_fetch("http://[fe80::1]:8080/admin")
            .is_blocked());
        assert!(enforcer
            .check_fetch("http://[fe80::1%25eth0]:8080/admin")
            .is_blocked());

        // fd00::1 (IPv6 unique local / ULA)
        assert!(enforcer
            .check_fetch("http://[fd12:3456:789a::1]:8080/api")
            .is_blocked());
    }

    // =========================================================================
    // H5: Proper domain-suffix matching (no substring bypass)
    // =========================================================================

    #[test]
    fn test_host_matching_no_substring_bypass() {
        // Blocked hosts: should NOT over-block unrelated domains
        let config = PolicyConfig {
            blocked_hosts: vec!["evil.com".into()],
            allowed_hosts: vec![], // no whitelist
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);

        // "evil.com" and subdomains blocked
        assert!(enforcer.check_fetch("https://evil.com/x").is_blocked());
        assert!(enforcer.check_fetch("https://sub.evil.com/x").is_blocked());
        // "not-evil.com" must NOT be blocked (old substring match would block it)
        assert!(enforcer.check_fetch("https://not-evil.com/x").is_allowed());
        // "evil.com.attacker.com" must NOT be blocked
        assert!(enforcer
            .check_fetch("https://evil.com.attacker.com/x")
            .is_allowed());

        // Allowed hosts: should NOT allow spoofed domains
        let config2 = PolicyConfig {
            allowed_hosts: vec!["api.openai.com".into()],
            ..Default::default()
        };
        let enforcer2 = PolicyEnforcer::new(config2);

        // Exact match and subdomains allowed
        assert!(enforcer2
            .check_fetch("https://api.openai.com/v1")
            .is_allowed());
        assert!(enforcer2
            .check_fetch("https://sub.api.openai.com/v1")
            .is_allowed());
        // Attacker domain with allowed host as prefix must be BLOCKED
        assert!(enforcer2
            .check_fetch("https://api.openai.com.evil.com/v1")
            .is_blocked());
        // Unrelated domain must be blocked
        assert!(enforcer2.check_fetch("https://other.com/api").is_blocked());
    }

    // =========================================================================
    // SEC-1: IPv4-compatible IPv6 bypass (::a.b.c.d, deprecated RFC 4291)
    // =========================================================================

    #[test]
    fn test_ssrf_blocks_ipv4_compatible_ipv6() {
        let enforcer = PolicyEnforcer::default();

        // ::127.0.0.1 (IPv4-compatible loopback) — must be blocked
        assert!(
            enforcer
                .check_fetch("http://[::127.0.0.1]:8080/x")
                .is_blocked(),
            "IPv4-compatible loopback ::127.0.0.1 must be blocked"
        );

        // ::169.254.169.254 (IPv4-compatible AWS metadata)
        assert!(
            enforcer
                .check_fetch("http://[::169.254.169.254]/meta")
                .is_blocked(),
            "IPv4-compatible AWS metadata ::169.254.169.254 must be blocked"
        );

        // ::10.0.0.1 (IPv4-compatible private)
        assert!(
            enforcer
                .check_fetch("http://[::10.0.0.1]/admin")
                .is_blocked(),
            "IPv4-compatible private ::10.0.0.1 must be blocked"
        );

        // ::192.168.1.1 (IPv4-compatible private)
        assert!(
            enforcer
                .check_fetch("http://[::192.168.1.1]/x")
                .is_blocked(),
            "IPv4-compatible private ::192.168.1.1 must be blocked"
        );

        // :: (IPv6 unspecified, equivalent to 0.0.0.0)
        assert!(
            enforcer.check_fetch("http://[::]:8080/admin").is_blocked(),
            "IPv6 unspecified [::] must be blocked"
        );
    }

    // =========================================================================
    // DNS rebinding: resolve_and_check_ssrf
    // =========================================================================

    #[tokio::test]
    async fn test_dns_rebinding_blocks_localhost() {
        assert!(
            super::resolve_and_check_ssrf("localhost").await,
            "localhost should be blocked after DNS resolution"
        );
    }

    #[tokio::test]
    async fn test_dns_rebinding_skips_raw_ips() {
        assert!(
            !super::resolve_and_check_ssrf("127.0.0.1").await,
            "Raw IP should be skipped (already handled by check_fetch)"
        );
    }

    #[tokio::test]
    async fn test_dns_rebinding_blocks_unresolvable() {
        // Fail-closed: DNS failure must BLOCK (not allow) the request
        assert!(
            super::resolve_and_check_ssrf("this-host-does-not-exist-nika-test.invalid").await,
            "Non-resolving hostname must be blocked (fail-closed)"
        );
    }

    // =========================================================================
    // check_tool_call tests (SEC-AGENT-01)
    // =========================================================================

    #[test]
    fn test_tool_call_allowed_by_default() {
        let enforcer = PolicyEnforcer::default();
        assert!(enforcer.check_tool_call("novanet_search").is_allowed());
        assert!(enforcer.check_tool_call("nika:complete").is_allowed());
    }

    #[test]
    fn test_tool_call_blocks_exec_tools_when_disabled() {
        let config = PolicyConfig {
            allow_exec: false,
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);
        assert!(enforcer.check_tool_call("run_command").is_blocked());
        assert!(enforcer.check_tool_call("exec_sql").is_blocked());
        assert!(enforcer.check_tool_call("bash_shell").is_blocked());
        // Safe tools still allowed
        assert!(enforcer.check_tool_call("novanet_search").is_allowed());
    }

    #[test]
    fn test_tool_call_blocks_network_tools_when_disabled() {
        let config = PolicyConfig {
            allow_network: false,
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);
        assert!(enforcer.check_tool_call("http_request").is_blocked());
        assert!(enforcer.check_tool_call("fetch_url").is_blocked());
        assert!(enforcer.check_tool_call("curl_api").is_blocked());
        // Non-network tools still allowed
        assert!(enforcer.check_tool_call("novanet_context").is_allowed());
    }

    #[test]
    fn test_tool_call_blocked_commands_pattern() {
        let config = PolicyConfig {
            blocked_commands: vec!["dangerous_tool".to_string()],
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);
        assert!(enforcer.check_tool_call("dangerous_tool").is_blocked());
        assert!(enforcer
            .check_tool_call("my_dangerous_tool_v2")
            .is_blocked());
        assert!(enforcer.check_tool_call("safe_tool").is_allowed());
    }

    // ════════════════════════════════════════════════════════
    // add_allowed_host + custom endpoint auto-allow
    // ════════════════════════════════════════════════════════

    #[test]
    fn test_add_allowed_host_adds_new() {
        let mut enforcer = PolicyEnforcer::default();
        assert!(enforcer.add_allowed_host("10.0.1.42"));
        // Verify it's now allowed
        assert!(
            enforcer
                .check_fetch("http://10.0.1.42:8000/v1/chat")
                .is_allowed(),
            "Custom endpoint IP should be allowed after add_allowed_host"
        );
    }

    #[test]
    fn test_add_allowed_host_deduplicates() {
        let mut enforcer = PolicyEnforcer::default();
        assert!(enforcer.add_allowed_host("10.0.1.42"));
        assert!(
            !enforcer.add_allowed_host("10.0.1.42"),
            "Duplicate host should return false"
        );
    }

    #[test]
    fn test_custom_endpoint_ip_auto_allowed_in_policy() {
        // Simulate what with_policy does: extract hosts from custom endpoints
        let mut policy = PolicyConfig::default();
        let endpoints: std::collections::HashMap<
            String,
            crate::provider::endpoints::ResolvedEndpoint,
        > = {
            let mut map = std::collections::HashMap::new();
            map.insert(
                "h100".to_string(),
                crate::provider::endpoints::ResolvedEndpoint {
                    base_url: "http://10.0.1.42:8000/v1".to_string(),
                    api_key: "test".to_string(),
                    default_model: None,
                    timeout_secs: 300,
                    hourly_rate: None,
                    currency: "USD".to_string(),
                },
            );
            map.insert(
                "ollama".to_string(),
                crate::provider::endpoints::ResolvedEndpoint {
                    base_url: "http://192.168.1.100:11434/v1".to_string(),
                    api_key: "ollama".to_string(),
                    default_model: None,
                    timeout_secs: 300,
                    hourly_rate: None,
                    currency: "USD".to_string(),
                },
            );
            map
        };

        // Extract hosts from endpoints (same logic as with_policy)
        for ep in endpoints.values() {
            if let Ok(url) = url::Url::parse(&ep.base_url) {
                if let Some(host) = url.host_str() {
                    let host_str = host.to_string();
                    if !policy.allowed_hosts.contains(&host_str) {
                        policy.allowed_hosts.push(host_str);
                    }
                }
            }
        }

        let enforcer = PolicyEnforcer::new(policy);

        // Both custom endpoint IPs should be allowed
        assert!(
            enforcer
                .check_fetch("http://10.0.1.42:8000/v1/chat")
                .is_allowed(),
            "h100 endpoint IP should be auto-allowed"
        );
        assert!(
            enforcer
                .check_fetch("http://192.168.1.100:11434/v1/chat")
                .is_allowed(),
            "ollama endpoint IP should be auto-allowed"
        );

        // Other private IPs should still be blocked
        assert!(
            enforcer
                .check_fetch("http://10.0.2.99:9090/api")
                .is_blocked(),
            "Non-endpoint private IPs should still be blocked"
        );
    }
}
