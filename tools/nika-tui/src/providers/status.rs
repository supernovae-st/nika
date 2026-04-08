// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider status types for Settings view
//!
//! Tracks provider connectivity and secret source information.

use std::time::Instant;

/// Source of an API key/secret
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// Retrieved from encrypted vault
    Vault,
    /// Retrieved from environment variable
    EnvVar,
    /// No key configured
    NotConfigured,
}

impl KeySource {
    /// Get display label for the key source
    pub fn label(&self) -> &'static str {
        match self {
            KeySource::Vault => "vault",
            KeySource::EnvVar => "env",
            KeySource::NotConfigured => "none",
        }
    }

    /// Get icon for the key source
    pub fn icon(&self) -> &'static str {
        match self {
            KeySource::Vault => "🔑",
            KeySource::EnvVar => "📦",
            KeySource::NotConfigured => "❌",
        }
    }

    /// Is the key secure (vault-based)?
    pub fn is_secure(&self) -> bool {
        matches!(self, KeySource::Vault)
    }
}

/// Connection status for a provider
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Connected successfully
    Connected { latency_ms: u64 },
    /// Not configured (no API key)
    NotConfigured,
    /// Connection failed
    Failed { error: String },
    /// Currently checking
    Checking,
    /// Not yet checked
    Unknown,
}

impl ConnectionStatus {
    /// Get status icon
    pub fn icon(&self) -> &'static str {
        match self {
            ConnectionStatus::Connected { .. } => "✅",
            ConnectionStatus::NotConfigured => "⚪",
            ConnectionStatus::Failed { .. } => "❌",
            ConnectionStatus::Checking => "🔄",
            ConnectionStatus::Unknown => "❓",
        }
    }

    /// Get status label
    pub fn label(&self) -> &'static str {
        match self {
            ConnectionStatus::Connected { .. } => "connected",
            ConnectionStatus::NotConfigured => "not configured",
            ConnectionStatus::Failed { .. } => "failed",
            ConnectionStatus::Checking => "checking...",
            ConnectionStatus::Unknown => "unknown",
        }
    }

    /// Is the provider usable?
    pub fn is_usable(&self) -> bool {
        matches!(self, ConnectionStatus::Connected { .. })
    }
}

/// Complete status for a provider
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    /// Provider ID (e.g., "anthropic", "neo4j")
    pub id: &'static str,
    /// Connection status
    pub connection: ConnectionStatus,
    /// Key source
    pub key_source: KeySource,
    /// Last check timestamp
    pub last_checked: Option<Instant>,
}

impl ProviderStatus {
    /// Create a new provider status with unknown state
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            connection: ConnectionStatus::Unknown,
            key_source: KeySource::NotConfigured,
            last_checked: None,
        }
    }

    /// Is this provider fully configured and working?
    pub fn is_ready(&self) -> bool {
        self.connection.is_usable() && self.key_source != KeySource::NotConfigured
    }
}

/// Cache for provider status information
#[derive(Debug, Clone, Default)]
pub struct ProviderStatusCache {
    /// All provider statuses
    pub providers: Vec<ProviderStatus>,
    /// Last full refresh timestamp
    pub last_refresh: Option<Instant>,
    /// Is currently refreshing?
    pub refreshing: bool,
}

impl ProviderStatusCache {
    /// Create empty cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Count configured LLM providers (includes "Local" category for native)
    pub fn llm_configured_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|p| {
                let cat = super::icons::provider_category(p.id);
                (cat == "LLM" || cat == "Local") && p.key_source != KeySource::NotConfigured
            })
            .count()
    }

    /// Count configured MCP providers
    pub fn mcp_configured_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|p| {
                super::icons::provider_category(p.id) == "MCP"
                    && p.key_source != KeySource::NotConfigured
            })
            .count()
    }

    /// Get summary string (e.g., "LLM: 4/7 | MCP: 2/6")
    pub fn summary(&self) -> String {
        format!(
            "LLM: {}/7 | MCP: {}/6",
            self.llm_configured_count(),
            self.mcp_configured_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_source_secure() {
        assert!(KeySource::Vault.is_secure());
        assert!(!KeySource::EnvVar.is_secure());
        assert!(!KeySource::NotConfigured.is_secure());
    }

    #[test]
    fn test_connection_status_usable() {
        assert!(ConnectionStatus::Connected { latency_ms: 100 }.is_usable());
        assert!(!ConnectionStatus::NotConfigured.is_usable());
        assert!(!ConnectionStatus::Failed {
            error: "test".into()
        }
        .is_usable());
    }

    #[test]
    fn test_provider_status_ready() {
        let mut status = ProviderStatus::new("anthropic");
        assert!(!status.is_ready());

        status.connection = ConnectionStatus::Connected { latency_ms: 50 };
        assert!(!status.is_ready()); // Still no key source

        status.key_source = KeySource::EnvVar;
        assert!(status.is_ready());
    }

    #[test]
    fn test_cache_summary() {
        let cache = ProviderStatusCache::new();
        assert_eq!(cache.summary(), "LLM: 0/7 | MCP: 0/6");
    }
}
