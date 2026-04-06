//! DaemonProvider trait — abstraction over daemon services.
//!
//! Two implementations:
//! - `IpcDaemonProvider`: wraps the existing Unix socket client (standalone daemon)
//! - `DaemonBridge` in nika-lsp already implements this pattern — this trait extracts it
//!
//! The LSP backend uses `Arc<dyn DaemonProvider>` instead of
//! `Arc<RwLock<Option<DaemonBridge>>>`, simplifying all call sites.

use async_trait::async_trait;

use nika_core::catalogs::{CostEstimate, DaemonCapabilities, ProviderStatusInfo, WorkflowRunInfo};

/// Abstraction over daemon services — IPC or in-process.
///
/// All methods are infallible: they return `Option<T>`, `Vec<T>`, or `bool`.
/// Never panics, never blocks the LSP.
#[async_trait]
pub trait DaemonProvider: Send + Sync + 'static {
    /// Is the daemon currently connected/available?
    fn is_connected(&self) -> bool;

    /// Get provider status (with API key availability).
    async fn provider_status(&self) -> Vec<ProviderStatusInfo>;

    /// Estimate cost for a model invocation.
    async fn estimate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate>;

    /// Get workflow run history.
    async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo>;

    /// Get daemon capabilities and stats.
    async fn capabilities(&self) -> Option<DaemonCapabilities>;
}

/// A provider that is always disconnected — returns empty/None for everything.
/// Used as the default before daemon connection is established.
pub struct DisconnectedProvider;

#[async_trait]
impl DaemonProvider for DisconnectedProvider {
    fn is_connected(&self) -> bool {
        false
    }

    async fn provider_status(&self) -> Vec<ProviderStatusInfo> {
        Vec::new()
    }

    async fn estimate_cost(
        &self,
        _provider: &str,
        _model: &str,
        _input_tokens: u64,
        _output_tokens: u64,
    ) -> Option<CostEstimate> {
        None
    }

    async fn workflow_history(&self, _workflow: &str) -> Vec<WorkflowRunInfo> {
        Vec::new()
    }

    async fn capabilities(&self) -> Option<DaemonCapabilities> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contract test — verifies any DaemonProvider impl behaves correctly.
    async fn verify_provider_contract(p: &dyn DaemonProvider) {
        // is_connected returns consistent state
        let _connected = p.is_connected();

        // provider_status returns a vec (empty or not)
        let _status = p.provider_status().await;

        // estimate_cost returns None for unknown model
        assert!(p.estimate_cost("fake", "fake", 0, 0).await.is_none());

        // workflow_history returns empty for unknown workflow
        assert!(p.workflow_history("nonexistent.nika.yaml").await.is_empty());

        // capabilities is None when disconnected
        if !p.is_connected() {
            assert!(p.capabilities().await.is_none());
        }
    }

    #[tokio::test]
    async fn disconnected_provider_satisfies_contract() {
        let p = DisconnectedProvider;
        verify_provider_contract(&p).await;
        assert!(!p.is_connected());
        assert!(p.provider_status().await.is_empty());
    }
}
