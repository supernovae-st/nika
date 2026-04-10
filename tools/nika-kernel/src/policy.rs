// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! PolicyChecker trait — abstract security policy questions for verbs.
//!
//! Concrete impl lives in `nika-policy` (L1). Runtime and verb crates
//! consume this trait only, never the concrete `PolicyEnforcer`.

/// Policy decision returned by every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Block(String),
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

/// Errors returned by a policy checker.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("policy violation: {0}")]
    Violation(String),
    #[error("policy evaluation failed: {0}")]
    Internal(String),
}

/// The four questions a verb can ask the policy layer.
///
/// Object-safe (no generics, no `Self`-returning methods) so callers can
/// hold it as `&dyn PolicyChecker` or `Arc<dyn PolicyChecker>` in caps structs.
pub trait PolicyChecker: Send + Sync + std::fmt::Debug {
    fn check_exec(&self, command: &str) -> PolicyDecision;
    fn check_fetch(&self, url: &str) -> PolicyDecision;
    fn check_token_spend(&self, tokens: u64) -> PolicyDecision;
    fn is_host_allowed(&self, host: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct AlwaysAllow;
    impl PolicyChecker for AlwaysAllow {
        fn check_exec(&self, _: &str) -> PolicyDecision {
            PolicyDecision::Allow
        }
        fn check_fetch(&self, _: &str) -> PolicyDecision {
            PolicyDecision::Allow
        }
        fn check_token_spend(&self, _: u64) -> PolicyDecision {
            PolicyDecision::Allow
        }
        fn is_host_allowed(&self, _: &str) -> bool {
            true
        }
    }

    #[test]
    fn allow_is_allowed() {
        assert!(PolicyDecision::Allow.is_allowed());
        assert!(!PolicyDecision::Allow.is_blocked());
    }

    #[test]
    fn block_is_not_allowed() {
        let d = PolicyDecision::Block("denied".into());
        assert!(!d.is_allowed());
        assert!(d.is_blocked());
    }

    #[test]
    fn checker_is_object_safe() {
        let checker: std::sync::Arc<dyn PolicyChecker> = std::sync::Arc::new(AlwaysAllow);
        assert!(checker.check_exec("noop").is_allowed());
        assert!(checker.is_host_allowed("example.com"));
    }

    #[test]
    fn error_display() {
        let e = PolicyError::Violation("policy off".into());
        assert_eq!(format!("{e}"), "policy violation: policy off");
    }
}
