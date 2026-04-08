// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Core modal state struct, Default, Debug, visibility, animation, verification, tab labels.

use std::collections::VecDeque;

use super::providers::CLOUD_PROVIDER_COUNT;
use super::types::{ConnectionStatus, DownloadState, NativeModelInfo, ProviderModalTab};

/// Main provider modal state
/// SEC-004: Custom Debug impl redacts key_input_buffer to prevent API key leaks in logs
#[derive(Clone)]
pub struct ProviderModalState {
    /// Modal visibility
    pub visible: bool,
    /// Active tab
    pub active_tab: ProviderModalTab,
    /// Selected index in current tab
    pub selected_idx: usize,
    /// Total items in current tab (for navigation bounds)
    pub item_count: usize,
    /// Download state for Native
    pub download_state: DownloadState,
    /// Key input mode active
    pub key_input_mode: bool,
    /// Key input buffer (may contain sensitive API keys)
    pub key_input_buffer: String,
    /// Whether Native is available (running)
    pub native_available: bool,
    /// Provider connection statuses (7 cloud providers)
    pub provider_statuses: Vec<ConnectionStatus>,
    /// Native models loaded from server
    pub native_models: Vec<NativeModelInfo>,
    /// Currently active provider index (the one being used for inference)
    pub active_provider_idx: Option<usize>,
    /// Currently active model name for display
    pub active_model: Option<String>,
    /// Animation frame counter for active provider cycling effect
    pub animation_frame: u8,
    /// Cached cloud tab label to avoid allocation per frame
    pub(super) cached_cloud_label: Option<String>,
    /// Latency history per provider (7 providers x 10 samples)
    pub(super) latency_history: Vec<VecDeque<u64>>,
    /// Matrix verification effect state
    pub verification_state: super::super::components::VerificationState,
    /// Whether verification animation is active
    pub verification_active: bool,
    /// Expanded provider index (for model selection)
    pub expanded_provider_idx: Option<usize>,
    /// Selected model index within expanded provider
    pub model_selection_idx: usize,
    /// Session token count (wired from ChatView)
    pub(super) session_tokens: u64,
    /// MCP connection count (wired from ChatView)
    pub(super) mcp_connections: usize,
}

impl Default for ProviderModalState {
    fn default() -> Self {
        Self {
            visible: false,
            active_tab: ProviderModalTab::Cloud,
            selected_idx: 0,
            item_count: CLOUD_PROVIDER_COUNT, // Cloud tab
            download_state: DownloadState::default(),
            key_input_mode: false,
            key_input_buffer: String::new(),
            native_available: false,
            provider_statuses: Vec::new(),
            native_models: Vec::new(),
            active_provider_idx: None,
            active_model: None,
            animation_frame: 0,
            cached_cloud_label: None,
            latency_history: vec![VecDeque::new(); CLOUD_PROVIDER_COUNT],
            verification_state: super::super::components::VerificationState::new_providers(),
            verification_active: false,
            expanded_provider_idx: None,
            model_selection_idx: 0,
            session_tokens: 0,
            mcp_connections: 0,
        }
    }
}

// SEC-004: Redact key_input_buffer in Debug output to prevent API key leaks
impl std::fmt::Debug for ProviderModalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderModalState")
            .field("visible", &self.visible)
            .field("active_tab", &self.active_tab)
            .field("selected_idx", &self.selected_idx)
            .field("item_count", &self.item_count)
            .field("download_state", &self.download_state)
            .field("key_input_mode", &self.key_input_mode)
            .field("key_input_buffer", &"[REDACTED]")
            .field("native_available", &self.native_available)
            .field("provider_statuses", &self.provider_statuses)
            .field(
                "native_models",
                &format!("[{} models]", self.native_models.len()),
            )
            .field("active_provider_idx", &self.active_provider_idx)
            .field("active_model", &self.active_model)
            .field("animation_frame", &self.animation_frame)
            .field(
                "latency_history",
                &format!(
                    "[{} providers]",
                    self.latency_history
                        .iter()
                        .filter(|h| !h.is_empty())
                        .count()
                ),
            )
            .field("verification_active", &self.verification_active)
            .field("expanded_provider_idx", &self.expanded_provider_idx)
            .field("model_selection_idx", &self.model_selection_idx)
            .finish()
    }
}

impl ProviderModalState {
    /// Toggle modal visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Open modal
    pub fn open(&mut self) {
        self.visible = true;
    }

    /// Close modal
    pub fn close(&mut self) {
        use zeroize::Zeroize;

        self.visible = false;
        self.key_input_mode = false;
        // SEC: Zeroize buffer to scrub API key material from memory
        self.key_input_buffer.zeroize();
    }

    /// Tick animation frame (call on each frame update)
    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);

        // Also tick verification animation if active
        if self.verification_active {
            self.verification_state.tick();

            // Auto-deactivate when all entries complete
            if self.verification_state.all_complete {
                self.verification_active = false;
            }
        }
    }

    /// Start verification animation (reset and activate)
    pub fn start_verification(&mut self) {
        self.verification_state.reset();
        self.verification_active = true;
    }

    /// Update verification entry when provider status changes
    pub fn sync_verification_status(&mut self, index: usize) {
        use super::super::components::ConnectionCheckStatus;

        if let Some(status) = self.provider_statuses.get(index) {
            let verify_status = match status {
                ConnectionStatus::Unknown => ConnectionCheckStatus::Checking,
                ConnectionStatus::Checking => ConnectionCheckStatus::Checking,
                ConnectionStatus::Connected { .. } => ConnectionCheckStatus::Connected,
                ConnectionStatus::Failed { .. } => ConnectionCheckStatus::Failed,
                ConnectionStatus::NotConfigured => ConnectionCheckStatus::NotConfigured,
            };
            self.verification_state.set_status(index, verify_status);
        }
    }

    /// Sync all verification statuses from provider_statuses
    pub fn sync_all_verification_statuses(&mut self) {
        for i in 0..CLOUD_PROVIDER_COUNT {
            self.sync_verification_status(i);
        }
    }

    /// Get animated indicator for active provider
    /// Returns cycling ASCII characters: ★ ✦ ● ◆ ✧
    pub fn active_indicator(&self) -> &'static str {
        const FRAMES: &[&str] = &["★", "✦", "●", "◆", "✧", "◉", "✴", "❋"];
        FRAMES[(self.animation_frame as usize / 4) % FRAMES.len()]
    }

    /// Get cloud tab label, cached to avoid format! work per frame
    /// Returns clone of cached string, recomputed only when model changes
    pub fn cloud_tab_label(&mut self) -> String {
        if self.cached_cloud_label.is_none() {
            let label = if let Some(ref model) = self.active_model {
                // Shorten model name for display (keep up to 20 chars)
                let short = if model.chars().count() > 20 {
                    let truncated: String = model.chars().take(17).collect();
                    format!("{}...", truncated)
                } else {
                    model.clone()
                };
                format!("☁️  CLOUD [{}]", short)
            } else {
                "☁️  CLOUD".to_string()
            };
            self.cached_cloud_label = Some(label);
        }
        // Clone is cheaper than format! on cache hit
        self.cached_cloud_label.as_ref().unwrap().clone()
    }

    /// Get Native tab label with model count
    /// Shows number of available models
    pub fn native_tab_label(&self) -> String {
        let count = self.native_models.len();
        if count > 0 {
            format!("🦙 NATIVE ({})", count)
        } else {
            "🦙 NATIVE".to_string()
        }
    }

    /// Get Keys tab label with status indicators
    /// Shows ✓ for configured keys, ✗ for missing
    pub fn keys_tab_label(&self) -> String {
        // Count configured vs missing keys from provider statuses
        let configured: usize = self
            .provider_statuses
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    ConnectionStatus::Connected { .. } | ConnectionStatus::NotConfigured
                )
            })
            .count();

        // Simple indicator: show count of verified
        if configured > 0 {
            let verified = self
                .provider_statuses
                .iter()
                .filter(|s| matches!(s, ConnectionStatus::Connected { .. }))
                .count();
            format!(
                "🔐 KEYS {}/{}",
                verified,
                configured.max(CLOUD_PROVIDER_COUNT)
            )
        } else {
            "🔐 KEYS".to_string()
        }
    }
}
