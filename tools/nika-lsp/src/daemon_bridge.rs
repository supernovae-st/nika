//! Daemon bridge — optional connection to the Nika daemon.
//!
//! The LSP server works without the daemon (graceful degradation).
//! When connected, it provides live provider status, cost estimates,
//! workflow history, and event subscriptions.

use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use nika_core::catalogs::{CostEstimate, DaemonCapabilities, ProviderStatusInfo, WorkflowRunInfo};
use nika_daemon::provider::DaemonProvider;

const CACHE_TTL: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const RECONNECT_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(30);

/// Bridge between the LSP server and the Nika daemon.
///
/// All methods return `Option<T>` or empty `Vec` — never panics, never blocks
/// the LSP even if the daemon is down or slow.
#[derive(Clone)]
#[allow(dead_code)] // Methods used incrementally as LSP features are wired up
pub struct DaemonBridge {
    #[cfg(unix)]
    client: Arc<RwLock<Option<nika_daemon::client::ConnectedClient>>>,
    connected: Arc<AtomicBool>,
    providers_cache: Arc<RwLock<CachedProviders>>,
}

#[allow(dead_code)]
struct CachedProviders {
    data: Vec<ProviderStatusInfo>,
    fetched_at: Instant,
}

#[allow(dead_code)] // Methods used incrementally as LSP features are wired up
impl DaemonBridge {
    /// Try to connect to a running daemon. Returns `None` if daemon not running.
    #[cfg(unix)]
    pub async fn try_connect() -> Option<Self> {
        let daemon_client = nika_daemon::client::DaemonClient::default_path();
        if !daemon_client.socket_exists() {
            tracing::debug!("Daemon socket not found — LSP features degraded");
            return None;
        }

        match tokio::time::timeout(REQUEST_TIMEOUT, daemon_client.connect()).await {
            Ok(Ok(connected)) => {
                tracing::info!("Connected to Nika daemon");
                Some(Self {
                    client: Arc::new(RwLock::new(Some(connected))),
                    connected: Arc::new(AtomicBool::new(true)),
                    providers_cache: Arc::new(RwLock::new(CachedProviders {
                        data: Vec::new(),
                        fetched_at: Instant::now() - CACHE_TTL, // force first refresh
                    })),
                })
            }
            Ok(Err(e)) => {
                tracing::debug!("Daemon connection failed: {e}");
                None
            }
            Err(_) => {
                tracing::debug!("Daemon connection timed out");
                None
            }
        }
    }

    /// Create a disconnected bridge (for platforms without daemon or testing).
    pub fn disconnected() -> Self {
        Self {
            #[cfg(unix)]
            client: Arc::new(RwLock::new(None)),
            connected: Arc::new(AtomicBool::new(false)),
            providers_cache: Arc::new(RwLock::new(CachedProviders {
                data: Vec::new(),
                fetched_at: Instant::now() - CACHE_TTL, // force first refresh on connect
            })),
        }
    }

    /// Is daemon currently connected?
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Get provider status (cached for 60s). Returns empty vec if disconnected.
    pub async fn provider_status(&self) -> Vec<ProviderStatusInfo> {
        // Check cache
        {
            let cache = self.providers_cache.read().await;
            if cache.fetched_at.elapsed() < CACHE_TTL && !cache.data.is_empty() {
                return cache.data.clone();
            }
        }

        // Refresh from daemon
        if self.is_connected() {
            #[cfg(unix)]
            if let Some(providers) = self.send_list_provider_status().await {
                let mut cache = self.providers_cache.write().await;
                cache.data.clone_from(&providers);
                cache.fetched_at = Instant::now();
                return providers;
            }
        }

        Vec::new()
    }

    /// Estimate cost. Returns `None` if disconnected or model unknown.
    pub async fn estimate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate> {
        if !self.is_connected() {
            return None;
        }
        #[cfg(unix)]
        return self
            .send_estimate_cost(provider, model, input_tokens, output_tokens)
            .await;
        #[cfg(not(unix))]
        return None;
    }

    /// Get workflow run history. Returns empty vec if disconnected.
    pub async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo> {
        if !self.is_connected() {
            return Vec::new();
        }
        #[cfg(unix)]
        return self
            .send_get_workflow_history(workflow)
            .await
            .unwrap_or_default();
        #[cfg(not(unix))]
        return Vec::new();
    }

    /// Get daemon capabilities. Returns `None` if disconnected.
    pub async fn capabilities(&self) -> Option<DaemonCapabilities> {
        if !self.is_connected() {
            return None;
        }
        #[cfg(unix)]
        return self.send_get_daemon_capabilities().await;
        #[cfg(not(unix))]
        return None;
    }

    // ── Unix-only daemon IPC ─────────────────────────────────────────────

    #[cfg(unix)]
    async fn send_request(
        &self,
        req: nika_daemon::protocol::DaemonRequest,
    ) -> Option<nika_daemon::protocol::DaemonResponse> {
        let mut guard = self.client.write().await;
        let client = guard.as_mut()?;

        if client.is_poisoned() {
            tracing::debug!("Daemon connection poisoned — marking disconnected");
            self.connected.store(false, Ordering::Relaxed);
            *guard = None;
            return None;
        }

        match tokio::time::timeout(REQUEST_TIMEOUT, client.request(req)).await {
            Ok(Ok(resp)) => Some(resp),
            Ok(Err(e)) => {
                tracing::debug!("Daemon request failed: {e}");
                self.connected.store(false, Ordering::Relaxed);
                *guard = None;
                None
            }
            Err(_) => {
                tracing::debug!("Daemon request timed out");
                None
            }
        }
    }

    #[cfg(unix)]
    async fn send_list_provider_status(&self) -> Option<Vec<ProviderStatusInfo>> {
        use nika_daemon::protocol::{DaemonRequest, DaemonResponse};
        let resp = self.send_request(DaemonRequest::ListProviderStatus).await?;
        match resp {
            DaemonResponse::ProviderStatusList { providers } => Some(providers),
            _ => None,
        }
    }

    #[cfg(unix)]
    async fn send_estimate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate> {
        use nika_daemon::protocol::{DaemonRequest, DaemonResponse};
        let resp = self
            .send_request(DaemonRequest::EstimateCost {
                provider: provider.to_string(),
                model: model.to_string(),
                input_tokens,
                output_tokens,
            })
            .await?;
        match resp {
            DaemonResponse::CostEstimateResult { estimate } => Some(estimate),
            _ => None,
        }
    }

    #[cfg(unix)]
    async fn send_get_workflow_history(&self, workflow: &str) -> Option<Vec<WorkflowRunInfo>> {
        use nika_daemon::protocol::{DaemonRequest, DaemonResponse};
        let resp = self
            .send_request(DaemonRequest::GetWorkflowHistory {
                workflow: workflow.to_string(),
            })
            .await?;
        match resp {
            DaemonResponse::WorkflowHistoryResult { runs } => Some(runs),
            _ => None,
        }
    }

    #[cfg(unix)]
    async fn send_get_daemon_capabilities(&self) -> Option<DaemonCapabilities> {
        use nika_daemon::protocol::{DaemonRequest, DaemonResponse};
        let resp = self
            .send_request(DaemonRequest::GetDaemonCapabilities)
            .await?;
        match resp {
            DaemonResponse::DaemonCapabilitiesResult { capabilities } => Some(capabilities),
            _ => None,
        }
    }

}

// ── DaemonProvider trait implementation ─────────────────────────────────
#[async_trait]
impl DaemonProvider for DaemonBridge {
    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    async fn provider_status(&self) -> Vec<ProviderStatusInfo> {
        DaemonBridge::provider_status(self).await
    }

    async fn estimate_cost(
        &self,
        provider: &str,
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Option<CostEstimate> {
        DaemonBridge::estimate_cost(self, provider, model, input_tokens, output_tokens).await
    }

    async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo> {
        DaemonBridge::workflow_history(self, workflow).await
    }

    async fn capabilities(&self) -> Option<DaemonCapabilities> {
        DaemonBridge::capabilities(self).await
    }
}

impl DaemonBridge {
    // Note: Event subscription (WatchTriggered, JobCompleted) requires raw socket streaming
    // which ConnectedClient::request() doesn't support. Live refresh is handled by:
    // - 30s daemon status poll in the VS Code extension (triggers code lens refresh)
    // - File system watcher in clientOptions.synchronize (triggers revalidation)
    // True event streaming will be added when ConnectedClient supports it.

    /// Spawn a background reconnection task.
    pub fn spawn_reconnect_loop(bridge: Arc<RwLock<Option<DaemonBridge>>>) {
        #[cfg(unix)]
        tokio::spawn(async move {
            let mut delay = RECONNECT_INITIAL;
            loop {
                tokio::time::sleep(delay).await;

                let needs_reconnect = {
                    let guard = bridge.read().await;
                    match guard.as_ref() {
                        Some(b) => !b.is_connected(),
                        None => true,
                    }
                };

                if needs_reconnect {
                    if let Some(new_bridge) = DaemonBridge::try_connect().await {
                        let mut guard = bridge.write().await;
                        *guard = Some(new_bridge);
                        delay = RECONNECT_INITIAL;
                        tracing::info!("Reconnected to Nika daemon");
                    } else {
                        delay = (delay * 2).min(RECONNECT_MAX);
                    }
                } else {
                    // Already connected — sleep longer before next health check
                    delay = RECONNECT_MAX;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bridge_disconnected_is_not_connected() {
        let bridge = DaemonBridge::disconnected();
        assert!(!bridge.is_connected());
    }

    #[tokio::test]
    async fn bridge_provider_status_returns_empty_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let status = bridge.provider_status().await;
        assert!(status.is_empty());
    }

    #[tokio::test]
    async fn bridge_estimate_cost_returns_none_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let cost = bridge
            .estimate_cost("anthropic", "claude-sonnet-4", 1000, 500)
            .await;
        assert!(cost.is_none());
    }

    #[tokio::test]
    async fn bridge_workflow_history_returns_empty_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let history = bridge.workflow_history("test.nika.yaml").await;
        assert!(history.is_empty());
    }

    #[tokio::test]
    async fn bridge_capabilities_returns_none_when_disconnected() {
        let bridge = DaemonBridge::disconnected();
        let caps = bridge.capabilities().await;
        assert!(caps.is_none());
    }
}
