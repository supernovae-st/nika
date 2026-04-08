// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider status management
//!
//! Connection status tracking, latency history (sparklines), session statistics.

use std::collections::VecDeque;

use super::types::{ConnectionStatus, ProviderModalTab};
use super::ProviderModalState;

/// Maximum latency history samples per provider
const LATENCY_HISTORY_MAX: usize = 10;

/// Number of cloud providers (anthropic, openai, mistral, groq, deepseek, gemini, xai)
pub(crate) const CLOUD_PROVIDER_COUNT: usize = 7;

impl ProviderModalState {
    /// Set the active provider (the one currently used for inference)
    pub fn set_active_provider(&mut self, name: &str) {
        self.active_provider_idx = match name.to_lowercase().as_str() {
            "anthropic" | "claude" => Some(0),
            "openai" => Some(1),
            "mistral" => Some(2),
            "groq" => Some(3),
            "deepseek" => Some(4),
            "gemini" => Some(5),
            "native" => Some(6),
            _ => None,
        };
    }

    /// Get the name of the currently active provider
    pub fn active_provider_name(&self) -> Option<&'static str> {
        match self.active_provider_idx {
            Some(0) => Some("Claude"),
            Some(1) => Some("OpenAI"),
            Some(2) => Some("Mistral"),
            Some(3) => Some("Groq"),
            Some(4) => Some("DeepSeek"),
            Some(5) => Some("Gemini"),
            Some(6) => Some("Native"),
            _ => None,
        }
    }

    /// Set the active model name
    pub fn set_active_model(&mut self, model: impl Into<String>) {
        self.active_model = Some(model.into());
        // Invalidate cached label on model change
        self.cached_cloud_label = None;
    }

    /// Update provider status by index
    ///
    /// Guard against invalid index (max 6 for 7 cloud providers)
    /// Automatically pushes latency to history when Connected
    /// Syncs verification animation status
    pub fn set_provider_status(&mut self, index: usize, status: ConnectionStatus) {
        // Guard against unbounded growth (7 cloud providers: 0-6)
        if index >= CLOUD_PROVIDER_COUNT {
            tracing::warn!(
                index = %index,
                "Invalid provider index (max {}), ignoring status update",
                CLOUD_PROVIDER_COUNT - 1,
            );
            return;
        }
        // Ensure vector is large enough
        while self.provider_statuses.len() <= index {
            self.provider_statuses.push(ConnectionStatus::Unknown);
        }

        // Push latency to history when connected
        if let ConnectionStatus::Connected { latency_ms } = &status {
            self.push_latency(index, *latency_ms);
        }

        self.provider_statuses[index] = status;

        // Sync verification animation status
        self.sync_verification_status(index);
    }

    /// Update provider status by name (7 cloud providers)
    /// Native is handled separately via native_models
    pub fn set_provider_status_by_name(&mut self, name: &str, status: ConnectionStatus) {
        let index = match name.to_lowercase().as_str() {
            "anthropic" | "claude" => 0,
            "openai" => 1,
            "mistral" => 2,
            "groq" => 3,
            "deepseek" => 4,
            "gemini" => 5,
            "xai" | "grok" => 6,
            // Native is not a cloud provider, handled separately
            _ => return,
        };
        self.set_provider_status(index, status);
    }

    /// Get provider statuses for CloudTab (7 cloud providers)
    pub fn get_provider_statuses(&self) -> Vec<ConnectionStatus> {
        let mut statuses = Vec::with_capacity(CLOUD_PROVIDER_COUNT);
        for i in 0..CLOUD_PROVIDER_COUNT {
            statuses.push(
                self.provider_statuses
                    .get(i)
                    .cloned()
                    .unwrap_or(ConnectionStatus::Unknown),
            );
        }
        statuses
    }

    /// Check if any provider is connected (verified)
    pub fn has_any_connected(&self) -> bool {
        self.provider_statuses
            .iter()
            .any(|s| matches!(s, ConnectionStatus::Connected { .. }))
    }

    /// Push a latency sample to history for a provider
    /// Maintains a rolling window of LATENCY_HISTORY_MAX samples
    /// Updated guard for 7 cloud providers (0-6)
    pub fn push_latency(&mut self, index: usize, latency_ms: u64) {
        if index >= CLOUD_PROVIDER_COUNT {
            return;
        }
        // Ensure vector has space for this provider
        while self.latency_history.len() <= index {
            self.latency_history.push(VecDeque::new());
        }
        let history = &mut self.latency_history[index];
        // Push new value, remove oldest if at max
        if history.len() >= LATENCY_HISTORY_MAX {
            history.pop_front();
        }
        history.push_back(latency_ms);
    }

    /// Push latency by provider name
    /// Fixed Gemini (index 5) + Native (index 6) mapping
    pub fn push_latency_by_name(&mut self, name: &str, latency_ms: u64) {
        let index = match name.to_lowercase().as_str() {
            "anthropic" | "claude" => 0,
            "openai" => 1,
            "mistral" => 2,
            "groq" => 3,
            "deepseek" => 4,
            "gemini" => 5,
            "native" => 6,
            _ => return,
        };
        self.push_latency(index, latency_ms);
    }

    /// Get latency history for a provider (for sparkline)
    ///
    /// Returns a contiguous slice view. Since VecDeque storage may be
    /// split across two segments, we use make_contiguous to linearize
    /// (max 10 elements, negligible cost).
    pub fn get_latency_history(&self, index: usize) -> Vec<u64> {
        self.latency_history
            .get(index)
            .map(|h| h.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Compute session statistics for footer display
    pub fn get_session_stats(&self) -> super::super::components::SessionStats {
        use super::super::components::SessionStats;

        let connected_providers = self
            .provider_statuses
            .iter()
            .filter(|s| matches!(s, ConnectionStatus::Connected { .. }))
            .count();

        // Calculate average latency from connected providers
        let latencies: Vec<u64> = self
            .provider_statuses
            .iter()
            .filter_map(|s| {
                if let ConnectionStatus::Connected { latency_ms } = s {
                    Some(*latency_ms)
                } else {
                    None
                }
            })
            .collect();

        let avg_latency_ms = if !latencies.is_empty() {
            Some(latencies.iter().sum::<u64>() / latencies.len() as u64)
        } else {
            None
        };

        SessionStats {
            connected_providers,
            total_providers: CLOUD_PROVIDER_COUNT,
            tokens_used: self.session_tokens,
            mcp_connections: self.mcp_connections,
            avg_latency_ms,
        }
    }

    /// Set Native models, clamping selection index if the list shrinks
    pub fn set_native_models(&mut self, models: Vec<super::types::NativeModelInfo>) {
        self.native_models = models;
        // Keep selected_idx and item_count consistent when the list changes
        // (e.g. after a model delete or a native server restart with fewer models)
        if self.active_tab == ProviderModalTab::Native {
            let max = self.native_models.len().saturating_sub(1);
            self.selected_idx = self.selected_idx.min(max);
            self.item_count = self.native_models.len().max(1);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Session Stats Methods
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set session token count (from ChatView.total_tokens())
    pub fn set_session_tokens(&mut self, tokens: u64) {
        self.session_tokens = tokens;
    }

    /// Set MCP connection count (wired from ChatView.session_context.mcp_servers)
    pub fn set_mcp_connections(&mut self, count: usize) {
        self.mcp_connections = count;
    }

    /// Process a loader event and update state accordingly
    pub fn process_loader_event(&mut self, event: super::super::loader::LoaderEvent) {
        use super::super::loader::LoaderEvent;

        match event {
            LoaderEvent::ProviderStatus { provider, status } => {
                self.set_provider_status_by_name(provider, status);
            }
            LoaderEvent::ProvidersComplete => {
                // All providers checked, could update UI state if needed
            }
            LoaderEvent::NativeAvailable(available) => {
                self.native_available = available;
            }
            LoaderEvent::NativeModels(models) => {
                self.native_models = models;
            }
            LoaderEvent::Error { source, message } => {
                // Handle errors - could show in status bar or notification
                tracing::warn!("Loader error from {}: {}", source, message);
            }
        }
    }
}
