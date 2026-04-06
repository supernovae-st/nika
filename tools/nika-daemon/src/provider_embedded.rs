//! EmbeddedDaemonProvider — runs daemon services in-process.
//!
//! For the LSP embedded mode: the LSP process IS the daemon.
//! No Unix socket, no separate process. Zero config.
//!
//! MVP scope: provider status (env vars), cost estimation (static table),
//! synthetic capabilities. No job tracking or vault in embedded mode.

use std::time::Instant;

use async_trait::async_trait;

use nika_core::catalogs::lsp_types::{
    DaemonCapabilities, KeySource, ProviderStatusInfo, WorkflowRunInfo,
};
use nika_core::catalogs::providers::{ProviderCategory, KNOWN_PROVIDERS};
use nika_core::catalogs::{cost, CostEstimate};

use crate::provider::DaemonProvider;

/// Daemon provider that runs services directly in the LSP process.
///
/// Always connected (`is_connected() == true`).
/// Provider status comes from env var checks (no vault).
/// Cost estimation uses the static pricing table (no daemon IPC needed).
pub struct EmbeddedDaemonProvider {
    started_at: Instant,
}

impl EmbeddedDaemonProvider {
    pub fn new() -> Self {
        tracing::info!("EmbeddedDaemonProvider: running daemon services in-process");
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Default for EmbeddedDaemonProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DaemonProvider for EmbeddedDaemonProvider {
    fn is_connected(&self) -> bool {
        true // Always connected — we ARE the daemon
    }

    async fn provider_status(&self) -> Vec<ProviderStatusInfo> {
        KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.category == ProviderCategory::Llm)
            .map(|p| {
                let has_key = p.has_env_key();
                ProviderStatusInfo {
                    id: p.id.to_string(),
                    name: p.name.to_string(),
                    has_key,
                    source: if has_key {
                        KeySource::Env
                    } else {
                        KeySource::NotFound
                    },
                    category: p.category,
                    env_var: p.env_var.to_string(),
                }
            })
            .collect()
    }

    async fn estimate_cost(
        &self,
        _provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate> {
        cost::estimate_cost(model, input_tokens, output_tokens)
    }

    async fn workflow_history(&self, _workflow: &str) -> Vec<WorkflowRunInfo> {
        Vec::new() // No job tracking in embedded mode
    }

    async fn capabilities(&self) -> Option<DaemonCapabilities> {
        Some(DaemonCapabilities {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            cache_entries: 0,
            cache_hit_rate: 0.0,
            active_jobs: 0,
            watch_active: false,
            total_cost_saved: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn embedded_is_always_connected() {
        let p = EmbeddedDaemonProvider::new();
        assert!(p.is_connected());
    }

    #[tokio::test]
    async fn embedded_provider_status_returns_llm_providers() {
        let p = EmbeddedDaemonProvider::new();
        let status = p.provider_status().await;
        assert!(!status.is_empty(), "Should list LLM providers");
        assert!(
            status.iter().any(|s| s.id == "anthropic"),
            "Should include anthropic"
        );
    }

    #[tokio::test]
    async fn embedded_estimate_cost_known_model() {
        let p = EmbeddedDaemonProvider::new();
        let cost = p
            .estimate_cost("anthropic", "claude-sonnet-4-20250514", 1000, 500)
            .await;
        assert!(cost.is_some(), "Should estimate cost for known model");
    }

    #[tokio::test]
    async fn embedded_estimate_cost_unknown_model() {
        let p = EmbeddedDaemonProvider::new();
        let cost = p.estimate_cost("fake", "nonexistent-model", 1000, 500).await;
        assert!(cost.is_none(), "Should return None for unknown model");
    }

    #[tokio::test]
    async fn embedded_capabilities_returns_version() {
        let p = EmbeddedDaemonProvider::new();
        let caps = p.capabilities().await;
        assert!(caps.is_some());
        let caps = caps.unwrap();
        assert!(!caps.version.is_empty());
    }

    #[tokio::test]
    async fn embedded_workflow_history_empty() {
        let p = EmbeddedDaemonProvider::new();
        assert!(p.workflow_history("test.nika.yaml").await.is_empty());
    }

    #[tokio::test]
    async fn embedded_satisfies_contract() {
        let p = EmbeddedDaemonProvider::new();
        crate::provider::tests::verify_provider_contract_ext(&p).await;
    }
}
