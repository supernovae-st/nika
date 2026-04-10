// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mock PolicyChecker for verb crate unit tests.

use nika_kernel::policy::{PolicyChecker, PolicyDecision};

/// Mock policy checker that allows or blocks everything.
///
/// Defaults to allowing all operations. Use `deny_all()` for a
/// restrictive mock that blocks every check.
#[derive(Debug, Clone)]
pub struct MockPolicyChecker {
    allow: bool,
}

impl MockPolicyChecker {
    /// Create a mock that allows all operations.
    pub fn allow_all() -> Self {
        Self { allow: true }
    }

    /// Create a mock that blocks all operations.
    pub fn deny_all() -> Self {
        Self { allow: false }
    }

    fn decision(&self, reason: &str) -> PolicyDecision {
        if self.allow {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Block(reason.to_string())
        }
    }
}

impl Default for MockPolicyChecker {
    fn default() -> Self {
        Self::allow_all()
    }
}

impl PolicyChecker for MockPolicyChecker {
    fn check_exec(&self, command: &str) -> PolicyDecision {
        self.decision(&format!("mock denied exec: {command}"))
    }

    fn check_fetch(&self, url: &str) -> PolicyDecision {
        self.decision(&format!("mock denied fetch: {url}"))
    }

    fn check_token_spend(&self, _tokens: u64) -> PolicyDecision {
        self.decision("mock denied token spend")
    }

    fn is_host_allowed(&self, _host: &str) -> bool {
        self.allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_all_permits_everything() {
        let p = MockPolicyChecker::allow_all();
        assert!(p.check_exec("rm -rf /").is_allowed());
        assert!(p.check_fetch("http://evil.com").is_allowed());
        assert!(p.check_token_spend(999_999).is_allowed());
        assert!(p.is_host_allowed("anything"));
    }

    #[test]
    fn deny_all_blocks_everything() {
        let p = MockPolicyChecker::deny_all();
        assert!(p.check_exec("echo hi").is_blocked());
        assert!(p.check_fetch("http://safe.com").is_blocked());
        assert!(p.check_token_spend(1).is_blocked());
        assert!(!p.is_host_allowed("anything"));
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MockPolicyChecker>();
    }

    #[test]
    fn implements_policy_checker_trait() {
        let checker: &dyn PolicyChecker = &MockPolicyChecker::allow_all();
        assert!(checker.check_exec("test").is_allowed());
    }
}
