// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-workflow-run Nika Shield state.
//!
//! Aggregates the spotlight fence, canary system, and security policy
//! references behind a single `Arc` so the runner pays one atomic refcount
//! per task spawn instead of four. Future Sprint 3 additions slot in here
//! without churning `TaskExecutor`'s field list.

use std::sync::Arc;

use nika_core::policy::{SecurityPolicyConfig, TaintMode};

use crate::runtime::canary::CanarySystem;
use crate::runtime::spotlight::SpotlightFence;

/// Per-workflow-run shield state. Cheap to clone — single `Arc` over an inner
/// `SecurityContextInner` that owns the fence and canary by value.
#[derive(Clone)]
pub struct SecurityContext {
    inner: Arc<SecurityContextInner>,
}

struct SecurityContextInner {
    fence: SpotlightFence,
    canary: CanarySystem,
    spotlight_enabled: bool,
    canary_enabled: bool,
    taint_mode: TaintMode,
    dangerous_tools: Arc<[String]>,
    max_run_depth: u8,
}

impl std::fmt::Debug for SecurityContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Intentionally hides the fence ID and canary tokens — exfil risk
        // via tracing logs. The Debug impl on CanarySystem is also redacted
        // (Sprint 2 Item 2 hardening).
        f.debug_struct("SecurityContext")
            .field("spotlight_enabled", &self.inner.spotlight_enabled)
            .field("canary_enabled", &self.inner.canary_enabled)
            .field("taint_mode", &self.inner.taint_mode)
            .field("max_run_depth", &self.inner.max_run_depth)
            .field("dangerous_tools_count", &self.inner.dangerous_tools.len())
            .finish_non_exhaustive()
    }
}

impl SecurityContext {
    /// Build a new shield context from a security policy. Allocates one
    /// fence + one canary per workflow run.
    #[must_use]
    pub fn from_policy(policy: &SecurityPolicyConfig) -> Self {
        Self {
            inner: Arc::new(SecurityContextInner {
                fence: SpotlightFence::new(),
                canary: CanarySystem::new(),
                spotlight_enabled: policy.spotlight,
                canary_enabled: policy.canary,
                taint_mode: policy.taint_mode,
                dangerous_tools: Arc::from(policy.dangerous_tools.as_slice()),
                max_run_depth: policy.max_run_depth,
            }),
        }
    }

    /// All-disabled shield context. Used by tests and `taint_mode = off`.
    #[must_use]
    pub fn disabled() -> Self {
        Self::from_policy(&SecurityPolicyConfig::disabled())
    }

    /// Spotlight fence used to wrap untrusted bindings in prompts.
    #[inline]
    #[must_use]
    pub fn fence(&self) -> &SpotlightFence {
        &self.inner.fence
    }

    /// Canary system used to detect system-prompt exfiltration.
    #[inline]
    #[must_use]
    pub fn canary(&self) -> &CanarySystem {
        &self.inner.canary
    }

    /// Whether per-binding spotlight wrapping is enabled.
    #[inline]
    #[must_use]
    pub fn spotlight_enabled(&self) -> bool {
        self.inner.spotlight_enabled
    }

    /// Whether canary token injection + detection is enabled.
    #[inline]
    #[must_use]
    pub fn canary_enabled(&self) -> bool {
        self.inner.canary_enabled
    }

    /// Current taint mode (off / warn / strict).
    #[inline]
    #[must_use]
    pub fn taint_mode(&self) -> TaintMode {
        self.inner.taint_mode
    }

    /// Tools that require elevated trust to execute on tainted data.
    #[inline]
    #[must_use]
    pub fn dangerous_tools(&self) -> &[String] {
        &self.inner.dangerous_tools
    }

    /// Maximum recursion depth for nested `nika:run` calls.
    #[inline]
    #[must_use]
    pub fn max_run_depth(&self) -> u8 {
        self.inner.max_run_depth
    }

    /// Strict mode short-circuit — true when violations should hard-fail.
    #[inline]
    #[must_use]
    pub fn is_strict(&self) -> bool {
        matches!(self.inner.taint_mode, TaintMode::Strict)
    }
}

impl Default for SecurityContext {
    fn default() -> Self {
        Self::from_policy(&SecurityPolicyConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_context_has_no_features_active() {
        let ctx = SecurityContext::disabled();
        assert!(!ctx.spotlight_enabled());
        assert!(!ctx.canary_enabled());
        assert!(!ctx.is_strict());
        assert_eq!(ctx.taint_mode(), TaintMode::Off);
        assert!(ctx.dangerous_tools().is_empty());
    }

    #[test]
    fn default_context_enables_spotlight_and_canary() {
        let ctx = SecurityContext::default();
        assert!(ctx.spotlight_enabled());
        assert!(ctx.canary_enabled());
        assert_eq!(ctx.taint_mode(), TaintMode::Warn);
        assert!(!ctx.is_strict());
        // The default policy lists nika:fetch + 4 write/edit/exec/run tools.
        assert!(ctx.dangerous_tools().len() >= 4);
        assert_eq!(ctx.max_run_depth(), 3);
    }

    #[test]
    fn clone_shares_arc_state_no_extra_allocation() {
        let ctx = SecurityContext::default();
        let cloned = ctx.clone();
        // Same Arc — refcount only.
        assert!(Arc::ptr_eq(&ctx.inner, &cloned.inner));
        // Both views must agree on every field.
        assert_eq!(ctx.spotlight_enabled(), cloned.spotlight_enabled());
        assert_eq!(ctx.canary_enabled(), cloned.canary_enabled());
        assert_eq!(ctx.is_strict(), cloned.is_strict());
        assert_eq!(ctx.fence().fence_id(), cloned.fence().fence_id());
    }

    #[test]
    fn debug_redacts_fence_id_and_canary_tokens() {
        let ctx = SecurityContext::default();
        let dbg = format!("{ctx:?}");
        // The fence ID is 12 hex chars — must not appear in Debug output.
        assert!(
            !dbg.contains(ctx.fence().fence_id()),
            "Debug must not leak the fence ID: {dbg}"
        );
        // The structure header is still useful.
        assert!(dbg.contains("SecurityContext"));
        assert!(dbg.contains("spotlight_enabled"));
    }

    #[test]
    fn strict_mode_flag_propagates_from_policy() {
        let policy = SecurityPolicyConfig {
            taint_mode: TaintMode::Strict,
            ..SecurityPolicyConfig::default()
        };
        let ctx = SecurityContext::from_policy(&policy);
        assert!(ctx.is_strict());
    }
}
