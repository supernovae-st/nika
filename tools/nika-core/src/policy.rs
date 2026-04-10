// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika Shield security policy configuration.
//!
//! Lives in `nika-core` (L0) so both `nika-engine` (which enforces it) and
//! `nika-display` (which renders the security summary) can read it without
//! violating the diamond layering.

use serde::{Deserialize, Serialize};

/// Taint analysis strictness mode.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaintMode {
    /// Disable taint analysis entirely.
    Off,
    /// Emit warnings (default).
    #[default]
    Warn,
    /// Treat taint warnings as errors / hard runtime failures.
    Strict,
}

impl std::fmt::Display for TaintMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::Warn => "warn",
            Self::Strict => "strict",
        })
    }
}

/// Nika Shield security configuration (from `[policy.security]` in nika.toml).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityPolicyConfig {
    /// Taint analysis mode: off | warn | strict.
    pub taint_mode: TaintMode,
    /// Enable per-binding spotlight wrapping in infer/agent prompts.
    pub spotlight: bool,
    /// Enable canary tokens in system prompts.
    pub canary: bool,
    /// Block exec tasks that consume untrusted upstream data.
    pub gate_untrusted_to_exec: bool,
    /// Require `structured:` for infer tasks consuming untrusted data.
    pub require_structured_for_untrusted: bool,
    /// Maximum DAG hops from `fetch:` to `exec:` before warning.
    pub max_fetch_to_exec_depth: u8,
    /// Treat `$env.*` bindings as untrusted (true = locked-down deployment).
    pub untrusted_env: bool,
    /// Maximum recursion depth for nested `nika:run` calls.
    pub max_run_depth: u8,
    /// Tools that require elevated trust to execute on tainted data.
    pub dangerous_tools: Vec<String>,
}

impl Default for SecurityPolicyConfig {
    fn default() -> Self {
        Self {
            taint_mode: TaintMode::Warn,
            spotlight: true,
            canary: true,
            gate_untrusted_to_exec: false,
            require_structured_for_untrusted: false,
            max_fetch_to_exec_depth: 3,
            untrusted_env: false,
            max_run_depth: 3,
            dangerous_tools: vec![
                "nika:write".to_string(),
                "nika:edit".to_string(),
                "nika:exec".to_string(),
                "nika:run".to_string(),
                "nika:fetch".to_string(),
            ],
        }
    }
}

impl SecurityPolicyConfig {
    /// All-disabled policy — for tests and `taint_mode = off`.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            taint_mode: TaintMode::Off,
            spotlight: false,
            canary: false,
            gate_untrusted_to_exec: false,
            require_structured_for_untrusted: false,
            max_fetch_to_exec_depth: 0,
            untrusted_env: false,
            max_run_depth: 10,
            dangerous_tools: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Policy configuration for security enforcement.
///
/// Lives in `nika-core` (L0) so both `nika-policy` (which enforces it) and
/// `nika-engine` / `nika-runtime` (which pass it through) can read it
/// without violating the diamond layering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    /// Allow exec: verb
    #[serde(default = "default_true")]
    pub allow_exec: bool,
    /// Allow fetch: verb (network access)
    #[serde(default = "default_true")]
    pub allow_network: bool,
    /// Blocked shell commands (substrings)
    #[serde(default)]
    pub blocked_commands: Vec<String>,
    /// Maximum token spend per workflow run
    pub max_token_spend: Option<u64>,
    /// Allowed hosts for fetch:
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    /// Blocked hosts for fetch:
    #[serde(default)]
    pub blocked_hosts: Vec<String>,
    /// Nika Shield security configuration
    #[serde(default)]
    pub security: SecurityPolicyConfig,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            allow_exec: true,
            allow_network: true,
            blocked_commands: vec![
                "rm -rf /".to_string(),
                "sudo".to_string(),
                "chmod 777".to_string(),
            ],
            max_token_spend: None,
            allowed_hosts: vec![],
            blocked_hosts: vec![],
            security: SecurityPolicyConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_security_policy_is_warn_mode() {
        let p = SecurityPolicyConfig::default();
        assert_eq!(p.taint_mode, TaintMode::Warn);
        assert!(p.spotlight);
        assert!(p.canary);
        assert_eq!(p.max_run_depth, 3);
        assert!(p.dangerous_tools.contains(&"nika:run".to_string()));
    }

    #[test]
    fn test_disabled_security_policy_turns_everything_off() {
        let p = SecurityPolicyConfig::disabled();
        assert_eq!(p.taint_mode, TaintMode::Off);
        assert!(!p.spotlight);
        assert!(!p.canary);
        assert!(p.dangerous_tools.is_empty());
    }

    #[test]
    fn test_taint_mode_display() {
        assert_eq!(TaintMode::Off.to_string(), "off");
        assert_eq!(TaintMode::Warn.to_string(), "warn");
        assert_eq!(TaintMode::Strict.to_string(), "strict");
    }

    #[test]
    fn test_security_policy_serde_roundtrip() {
        let p = SecurityPolicyConfig::default();
        let json = serde_json::to_string(&p).unwrap();
        let restored: SecurityPolicyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(p, restored);
    }
}
