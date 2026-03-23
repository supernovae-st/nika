//! Policy Enforcer - Security policy enforcement
//!
//! Enforces allow/block rules for:
//! - Shell commands (exec: verb)
//! - Network access (fetch: verb)
//! - Token budget limits
//! - Host restrictions

use crate::error::NikaError;
use crate::runtime::boot::PolicyConfig;
use url::Url;

/// Hardcoded SSRF blocklist: cloud metadata endpoints and loopback addresses.
///
/// These are ALWAYS blocked regardless of user configuration.
/// Cloud metadata services (AWS/GCP/Alibaba) and loopback addresses are
/// common SSRF targets that should never be reachable from workflow fetch: verbs.
const SSRF_BLOCKED_HOSTS: &[&str] = &[
    "169.254.169.254",
    "metadata.google.internal",
    "100.100.100.200",
    "localhost",
    "127.0.0.1",
    "::1",
    "0.0.0.0",
];

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
            Some(limit) => self.used + tokens <= limit,
            None => true,
        }
    }

    /// Record token usage
    pub fn spend(&mut self, tokens: u64) {
        self.used += tokens;
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

        // SSRF protection: block cloud metadata and loopback
        // Exception: explicit allowed_hosts override SSRF blocklist (for testing, local services)
        let explicitly_allowed = self
            .config
            .allowed_hosts
            .iter()
            .any(|allowed| host_normalized == allowed.to_lowercase());
        if !explicitly_allowed && SSRF_BLOCKED_HOSTS.contains(&host_normalized) {
            return PolicyDecision::Block(format!(
                "SSRF protection: access to '{}' is blocked",
                host
            ));
        }

        // Check blocked hosts first (takes precedence)
        for blocked in &self.config.blocked_hosts {
            if host.contains(&blocked.to_lowercase()) || blocked.to_lowercase().contains(&host) {
                return PolicyDecision::Block(format!("Host '{}' is blocked by policy", host));
            }
        }

        // If allowed_hosts is non-empty, only those hosts are allowed
        if !self.config.allowed_hosts.is_empty() {
            let is_allowed = self.config.allowed_hosts.iter().any(|allowed| {
                host.contains(&allowed.to_lowercase()) || allowed.to_lowercase().contains(&host)
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
            blocked_hosts: vec!["evil.com".into(), "malware".into()],
            ..Default::default()
        };
        let enforcer = PolicyEnforcer::new(config);

        assert!(enforcer.check_fetch("https://evil.com/path").is_blocked());
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
}
