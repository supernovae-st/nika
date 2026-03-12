//! Provider modal state types
//!
//! v0.27: Native tab replaced with Native tab (mistral.rs)

/// Native model info for local inference (v0.27 - replaces NativeModelInfo)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeModelInfo {
    /// Model name (e.g., "llama3.2:1b")
    pub name: String,
    /// Model size in bytes
    pub size: u64,
    /// Model path or digest
    pub digest: String,
    /// Last modified date
    pub modified_at: String,
    /// Model details
    pub details: NativeModelDetails,
}

/// Native model details (v0.27 - replaces NativeModelDetails)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativeModelDetails {
    /// Parameter size (e.g., "1B", "8B")
    pub parameter_size: String,
    /// Quantization level (e.g., "Q4_0", "Q8_0")
    pub quantization_level: String,
    /// Model family (e.g., "llama", "qwen")
    pub family: Option<String>,
}

/// Active tab in the provider modal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderModalTab {
    #[default]
    Cloud,
    /// v0.27: Native local inference (was Native)
    Native,
    Keys,
    Config,
}

impl ProviderModalTab {
    /// Get next tab (cycles)
    pub fn next(self) -> Self {
        match self {
            Self::Cloud => Self::Native,
            Self::Native => Self::Keys,
            Self::Keys => Self::Config,
            Self::Config => Self::Cloud,
        }
    }

    /// Get previous tab (cycles)
    pub fn prev(self) -> Self {
        match self {
            Self::Cloud => Self::Config,
            Self::Native => Self::Cloud,
            Self::Keys => Self::Native,
            Self::Config => Self::Keys,
        }
    }

    /// Create from keyboard key
    pub fn from_key(c: char) -> Option<Self> {
        match c {
            '1' => Some(Self::Cloud),
            '2' => Some(Self::Native),
            '3' => Some(Self::Keys),
            '4' => Some(Self::Config),
            _ => None,
        }
    }

    /// Tab label for display
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cloud => "☁️  CLOUD",
            Self::Native => "🦙 NATIVE",
            Self::Keys => "🔐 KEYS",
            Self::Config => "⚙️  CONFIG",
        }
    }
}

/// Provider connection status with rich info
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    /// Not yet checked
    #[default]
    Unknown,
    /// Currently checking connection
    Checking,
    /// Successfully connected with latency
    Connected { latency_ms: u64 },
    /// Connection failed with error
    Failed { error: String },
    /// API key not configured
    NotConfigured,
}

impl ConnectionStatus {
    /// Display text for UI
    pub fn display_text(&self) -> String {
        match self {
            Self::Unknown => "○ Unknown".to_string(),
            Self::Checking => "⠹ Checking...".to_string(),
            Self::Connected { latency_ms } => format!("● {}ms", latency_ms),
            Self::Failed { error } => format!("✗ {}", error),
            Self::NotConfigured => "○ Not configured".to_string(),
        }
    }

    /// Check if provider is available for use
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }
}

/// API key configuration state
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ApiKeyState {
    /// No key configured
    #[default]
    NotConfigured,
    /// Key is being saved to keyring
    Saving { masked: String },
    /// Key is stored, now testing
    Testing { masked: String },
    /// Key from environment variable (session-based, ephemeral)
    Configured { masked: String },
    /// Key from system keyring (persisted, secure) - v0.11.0
    Stored { masked: String },
    /// Key verified working with latency
    Verified { masked: String, latency_ms: u64 },
    /// Key is invalid
    Invalid { masked: String, error: String },
}

impl ApiKeyState {
    /// Mask API key for display (show first 6 and last 1 char)
    pub fn mask_key(key: &str) -> String {
        if key.len() <= 10 {
            return "****".to_string();
        }
        let prefix = &key[..6.min(key.len())];
        let suffix = &key[key.len().saturating_sub(1)..];
        format!("{}...{}", prefix, suffix)
    }

    /// Status icon for display
    pub fn status_icon(&self) -> &'static str {
        match self {
            Self::NotConfigured => "⚠",
            Self::Saving { .. } => "⏳",
            Self::Testing { .. } => "⠹",
            Self::Configured { .. } => "✓",
            Self::Stored { .. } => "🔐", // v0.11.0: Keyring stored
            Self::Verified { .. } => "✓",
            Self::Invalid { .. } => "✗",
        }
    }

    /// Check if key is configured (valid or not)
    pub fn is_configured(&self) -> bool {
        !matches!(self, Self::NotConfigured)
    }
}

/// Download state for Native model pulls
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DownloadState {
    /// No download in progress
    #[default]
    Idle,
    /// Downloading with progress
    Downloading {
        model: String,
        progress: f64,
        downloaded: u64,
        total: u64,
    },
    /// Download completed
    Complete { model: String },
    /// Download failed
    Failed { model: String, error: String },
}

impl DownloadState {
    /// Check if download is active
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Downloading { .. })
    }

    /// Get percentage (0-100)
    pub fn percentage(&self) -> u16 {
        match self {
            Self::Downloading { progress, .. } => (*progress * 100.0) as u16,
            Self::Complete { .. } => 100,
            _ => 0,
        }
    }

    /// Format bytes for display
    pub fn format_bytes(bytes: u64) -> String {
        if bytes >= 1_000_000_000 {
            format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
        } else if bytes >= 1_000_000 {
            format!("{:.1} MB", bytes as f64 / 1_000_000.0)
        } else if bytes >= 1_000 {
            format!("{:.1} KB", bytes as f64 / 1_000.0)
        } else {
            format!("{} B", bytes)
        }
    }
}

/// Maximum latency history samples per provider
const LATENCY_HISTORY_MAX: usize = 10;

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
    /// Provider connection statuses (6 cloud providers)
    pub provider_statuses: Vec<ConnectionStatus>,
    /// Native models loaded from server
    pub native_models: Vec<NativeModelInfo>,
    /// Currently active provider index (the one being used for inference)
    pub active_provider_idx: Option<usize>,
    /// Currently active model name for display
    pub active_model: Option<String>,
    /// Animation frame counter for active provider cycling effect
    pub animation_frame: u8,
    /// v0.8.9: Cached cloud tab label to avoid allocation per frame
    cached_cloud_label: Option<String>,
    /// v0.8.9: Latency history per provider (6 providers × 10 samples)
    latency_history: Vec<Vec<u64>>,
    /// v0.8.9: Matrix verification effect state
    pub verification_state: super::components::VerificationState,
    /// v0.8.9: Whether verification animation is active
    pub verification_active: bool,
    /// v0.8.95: Expanded provider index (for model selection)
    pub expanded_provider_idx: Option<usize>,
    /// v0.8.95: Selected model index within expanded provider
    pub model_selection_idx: usize,
    /// v0.9.5: Session token count (wired from ChatView)
    session_tokens: u64,
    /// v0.9.5: MCP connection count (wired from ChatView)
    mcp_connections: usize,
}

impl Default for ProviderModalState {
    fn default() -> Self {
        Self {
            visible: false,
            active_tab: ProviderModalTab::Cloud,
            selected_idx: 0,
            item_count: 6, // Cloud tab has 6 providers by default (v0.27: Ollama removed)
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
            latency_history: vec![Vec::new(); 6], // Pre-allocate for 6 providers
            verification_state: super::components::VerificationState::new_providers(),
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
        self.visible = false;
        self.key_input_mode = false;
        self.key_input_buffer.clear();
    }

    /// Switch to a different tab
    pub fn switch_tab(&mut self, tab: ProviderModalTab) {
        self.active_tab = tab;
        self.selected_idx = 0; // Reset selection on tab change
                               // Update item_count based on tab
        self.item_count = match tab {
            ProviderModalTab::Cloud => 6, // 6 cloud providers (v0.27: Ollama removed)
            ProviderModalTab::Native => self.native_models.len().max(1), // Dynamic
            ProviderModalTab::Keys => 6,  // 6 API key entries (cloud providers only)
            ProviderModalTab::Config => 6, // 6 config entries (matches ConfigTab::new())
        };
    }

    /// Navigate up in current list/grid (wraps around)
    /// For Cloud tab (2x3 grid, 6 items): moves up one row (idx - 3), wraps to last row
    /// For other tabs: moves up one item (idx - 1), wraps to last
    pub fn navigate_up(&mut self) {
        if self.item_count == 0 {
            return;
        }
        if self.active_tab == ProviderModalTab::Cloud {
            // 3-column grid (6 items): move up one row, wrap to last row
            // v0.27: 2x3 grid (2 rows, 6 items), Ollama removed
            if self.selected_idx >= 3 {
                self.selected_idx -= 3;
            } else {
                // Wrap to last row with this column (2x3 grid: idx 0-2 wraps to 3-5)
                let col = self.selected_idx;
                let last_row_idx = col + 3;
                self.selected_idx = last_row_idx.min(self.item_count - 1);
            }
        } else {
            // List navigation: wrap to last
            if self.selected_idx == 0 {
                self.selected_idx = self.item_count - 1;
            } else {
                self.selected_idx -= 1;
            }
        }
    }

    /// Navigate down in current list/grid (wraps around)
    /// For Cloud tab (2x3 grid, 6 items): moves down one row (idx + 3), wraps to top
    /// For other tabs: moves down one item (idx + 1), wraps to first
    pub fn navigate_down(&mut self) {
        if self.item_count == 0 {
            return;
        }
        if self.active_tab == ProviderModalTab::Cloud {
            // 3-column grid (6 items): move down one row, wrap to top
            if self.selected_idx + 3 < self.item_count {
                self.selected_idx += 3;
            } else {
                // Wrap to top row, same column
                let col = self.selected_idx % 3;
                self.selected_idx = col;
            }
        } else {
            // List navigation: wrap to first
            if self.selected_idx >= self.item_count - 1 {
                self.selected_idx = 0;
            } else {
                self.selected_idx += 1;
            }
        }
    }

    /// Navigate left in grid (Cloud tab only, wraps within row)
    pub fn navigate_left(&mut self) {
        if self.active_tab == ProviderModalTab::Cloud {
            if self.selected_idx % 3 != 0 {
                self.selected_idx -= 1;
            } else {
                // Wrap to end of row
                let row_end = (self.selected_idx + 2).min(self.item_count - 1);
                self.selected_idx = row_end;
            }
        }
    }

    /// Navigate right in grid (Cloud tab only, wraps within row)
    pub fn navigate_right(&mut self) {
        if self.active_tab == ProviderModalTab::Cloud && self.item_count > 0 {
            if self.selected_idx % 3 != 2 && self.selected_idx < self.item_count - 1 {
                self.selected_idx += 1;
            } else {
                // Wrap to start of row
                let row_start = (self.selected_idx / 3) * 3;
                self.selected_idx = row_start;
            }
        }
    }

    /// Go to next tab
    pub fn next_tab(&mut self) {
        self.switch_tab(self.active_tab.next());
    }

    /// Go to previous tab
    pub fn prev_tab(&mut self) {
        self.switch_tab(self.active_tab.prev());
    }

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
        // v0.8.9: Invalidate cached label on model change
        self.cached_cloud_label = None;
    }

    /// Tick animation frame (call on each frame update)
    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);

        // v0.8.9: Also tick verification animation if active
        if self.verification_active {
            self.verification_state.tick();

            // Auto-deactivate when all entries complete
            if self.verification_state.all_complete {
                self.verification_active = false;
            }
        }
    }

    /// v0.8.9: Start verification animation (reset and activate)
    pub fn start_verification(&mut self) {
        self.verification_state.reset();
        self.verification_active = true;
    }

    /// v0.8.9: Update verification entry when provider status changes
    pub fn sync_verification_status(&mut self, index: usize) {
        use super::components::VerifyStatus;

        if let Some(status) = self.provider_statuses.get(index) {
            let verify_status = match status {
                ConnectionStatus::Unknown => VerifyStatus::Checking,
                ConnectionStatus::Checking => VerifyStatus::Checking,
                ConnectionStatus::Connected { .. } => VerifyStatus::Connected,
                ConnectionStatus::Failed { .. } => VerifyStatus::Failed,
                ConnectionStatus::NotConfigured => VerifyStatus::NotConfigured,
            };
            self.verification_state.set_status(index, verify_status);
        }
    }

    /// v0.8.9: Sync all verification statuses from provider_statuses
    /// v0.27: Updated to sync all 6 cloud providers (Ollama removed)
    pub fn sync_all_verification_statuses(&mut self) {
        for i in 0..6 {
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
    /// v0.8.9: Returns clone of cached string, recomputed only when model changes
    pub fn cloud_tab_label(&mut self) -> String {
        if self.cached_cloud_label.is_none() {
            let label = if let Some(ref model) = self.active_model {
                // Shorten model name for display (keep up to 20 chars)
                let short = if model.len() > 20 {
                    format!("{}...", &model[..17])
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
    /// v0.8.9: Shows number of available models
    /// v0.27: Ollama removed
    pub fn native_tab_label(&self) -> String {
        let count = self.native_models.len();
        if count > 0 {
            format!("🦙 NATIVE ({})", count)
        } else {
            "🦙 NATIVE".to_string()
        }
    }

    /// Get Keys tab label with status indicators
    /// v0.8.9: Shows ✓ for configured keys, ✗ for missing
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
            format!("🔐 KEYS {}/{}", verified, configured.max(6))
        } else {
            "🔐 KEYS".to_string()
        }
    }

    /// Update provider status by index
    ///
    /// v0.27: Guard against invalid index (max 5 for 6 cloud providers)
    /// v0.8.9: Automatically pushes latency to history when Connected
    /// v0.8.9: Syncs verification animation status
    pub fn set_provider_status(&mut self, index: usize, status: ConnectionStatus) {
        // Guard against unbounded growth (6 cloud providers: 0-5)
        if index >= 6 {
            tracing::warn!(
                index = %index,
                "Invalid provider index (max 5), ignoring status update"
            );
            return;
        }
        // Ensure vector is large enough
        while self.provider_statuses.len() <= index {
            self.provider_statuses.push(ConnectionStatus::Unknown);
        }

        // v0.8.9: Push latency to history when connected
        if let ConnectionStatus::Connected { latency_ms } = &status {
            self.push_latency(index, *latency_ms);
        }

        self.provider_statuses[index] = status;

        // v0.8.9: Sync verification animation status
        self.sync_verification_status(index);
    }

    /// Update provider status by name (6 cloud providers only)
    /// v0.27: Native is handled separately via native_models
    pub fn set_provider_status_by_name(&mut self, name: &str, status: ConnectionStatus) {
        let index = match name.to_lowercase().as_str() {
            "anthropic" | "claude" => 0,
            "openai" => 1,
            "mistral" => 2,
            "groq" => 3,
            "deepseek" => 4,
            "gemini" => 5,
            // v0.27: Native is not a cloud provider, handled separately
            _ => return,
        };
        self.set_provider_status(index, status);
    }

    /// Get provider statuses for CloudTab (6 cloud providers)
    pub fn get_provider_statuses(&self) -> Vec<ConnectionStatus> {
        let mut statuses = Vec::with_capacity(6);
        for i in 0..6 {
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

    /// v0.8.9: Push a latency sample to history for a provider
    /// Maintains a rolling window of LATENCY_HISTORY_MAX samples
    /// v0.27: Updated guard for 6 cloud providers (0-5)
    pub fn push_latency(&mut self, index: usize, latency_ms: u64) {
        if index >= 6 {
            return;
        }
        // Ensure vector has space for this provider
        while self.latency_history.len() <= index {
            self.latency_history.push(Vec::new());
        }
        let history = &mut self.latency_history[index];
        // Push new value, remove oldest if at max
        if history.len() >= LATENCY_HISTORY_MAX {
            history.remove(0);
        }
        history.push(latency_ms);
    }

    /// v0.8.9: Push latency by provider name
    /// v0.21.2: Fixed Gemini (index 5) + Native (index 6) mapping
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

    /// v0.8.9: Get latency history for a provider (for sparkline)
    pub fn get_latency_history(&self, index: usize) -> &[u64] {
        self.latency_history
            .get(index)
            .map(|h| h.as_slice())
            .unwrap_or(&[])
    }

    /// v0.8.9: Compute session statistics for footer display
    pub fn get_session_stats(&self) -> super::components::SessionStats {
        use super::components::SessionStats;

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
            total_providers: 6, // 6 cloud providers: anthropic, openai, mistral, groq, deepseek, gemini
            tokens_used: self.session_tokens,
            mcp_connections: self.mcp_connections,
            avg_latency_ms,
        }
    }

    /// Set Native models
    pub fn set_native_models(&mut self, models: Vec<NativeModelInfo>) {
        self.native_models = models;
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.9.5: Session Stats Wiring Methods
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set session token count (wired from ChatView.total_tokens())
    pub fn set_session_tokens(&mut self, tokens: u64) {
        self.session_tokens = tokens;
    }

    /// Set MCP connection count (wired from ChatView.session_context.mcp_servers)
    pub fn set_mcp_connections(&mut self, count: usize) {
        self.mcp_connections = count;
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // v0.8.95: Model Expand/Collapse Methods
    // ═══════════════════════════════════════════════════════════════════════════

    /// Check if a provider is currently expanded
    pub fn is_provider_expanded(&self) -> bool {
        self.expanded_provider_idx.is_some()
    }

    /// Check if a specific provider is expanded
    pub fn is_provider_idx_expanded(&self, idx: usize) -> bool {
        self.expanded_provider_idx == Some(idx)
    }

    /// Expand the currently selected provider (show model list)
    pub fn expand_selected_provider(&mut self) {
        if self.active_tab == ProviderModalTab::Cloud {
            self.expanded_provider_idx = Some(self.selected_idx);
            self.model_selection_idx = 0;
        }
    }

    /// Collapse the expanded provider
    pub fn collapse_provider(&mut self) {
        self.expanded_provider_idx = None;
        self.model_selection_idx = 0;
    }

    /// Toggle expand/collapse for selected provider
    pub fn toggle_provider_expand(&mut self) {
        if self.is_provider_idx_expanded(self.selected_idx) {
            self.collapse_provider();
        } else {
            self.expand_selected_provider();
        }
    }

    /// Navigate up in model list (when expanded)
    pub fn model_navigate_up(&mut self, model_count: usize) {
        if self.expanded_provider_idx.is_some() && model_count > 0 {
            if self.model_selection_idx == 0 {
                self.model_selection_idx = model_count - 1;
            } else {
                self.model_selection_idx -= 1;
            }
        }
    }

    /// Navigate down in model list (when expanded)
    pub fn model_navigate_down(&mut self, model_count: usize) {
        if self.expanded_provider_idx.is_some() && model_count > 0 {
            if self.model_selection_idx >= model_count - 1 {
                self.model_selection_idx = 0;
            } else {
                self.model_selection_idx += 1;
            }
        }
    }

    /// Process a loader event and update state accordingly
    pub fn process_loader_event(&mut self, event: super::loader::LoaderEvent) {
        use super::loader::LoaderEvent;

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

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_default_is_cloud() {
        assert_eq!(ProviderModalTab::default(), ProviderModalTab::Cloud);
    }

    #[test]
    fn test_tab_next_cycles() {
        assert_eq!(ProviderModalTab::Cloud.next(), ProviderModalTab::Native);
        assert_eq!(ProviderModalTab::Native.next(), ProviderModalTab::Keys);
        assert_eq!(ProviderModalTab::Keys.next(), ProviderModalTab::Config);
        assert_eq!(ProviderModalTab::Config.next(), ProviderModalTab::Cloud);
    }

    #[test]
    fn test_tab_prev_cycles() {
        assert_eq!(ProviderModalTab::Cloud.prev(), ProviderModalTab::Config);
        assert_eq!(ProviderModalTab::Config.prev(), ProviderModalTab::Keys);
    }

    #[test]
    fn test_tab_from_key() {
        assert_eq!(
            ProviderModalTab::from_key('1'),
            Some(ProviderModalTab::Cloud)
        );
        assert_eq!(
            ProviderModalTab::from_key('2'),
            Some(ProviderModalTab::Native)
        );
        assert_eq!(
            ProviderModalTab::from_key('3'),
            Some(ProviderModalTab::Keys)
        );
        assert_eq!(
            ProviderModalTab::from_key('4'),
            Some(ProviderModalTab::Config)
        );
        assert_eq!(ProviderModalTab::from_key('x'), None);
    }

    #[test]
    fn test_tab_label() {
        assert_eq!(ProviderModalTab::Cloud.label(), "☁️  CLOUD");
        assert_eq!(ProviderModalTab::Native.label(), "🦙 NATIVE");
    }

    #[test]
    fn test_connection_status_display() {
        let connected = ConnectionStatus::Connected { latency_ms: 182 };
        assert_eq!(connected.display_text(), "● 182ms");

        let checking = ConnectionStatus::Checking;
        assert_eq!(checking.display_text(), "⠹ Checking...");

        let failed = ConnectionStatus::Failed {
            error: "Timeout".into(),
        };
        assert_eq!(failed.display_text(), "✗ Timeout");

        let not_configured = ConnectionStatus::NotConfigured;
        assert_eq!(not_configured.display_text(), "○ Not configured");
    }

    #[test]
    fn test_connection_status_is_available() {
        assert!(ConnectionStatus::Connected { latency_ms: 100 }.is_available());
        assert!(!ConnectionStatus::Checking.is_available());
        assert!(!ConnectionStatus::Failed { error: "".into() }.is_available());
        assert!(!ConnectionStatus::NotConfigured.is_available());
    }

    #[test]
    fn test_api_key_masking() {
        let key = "sk-ant-api03-abc123xyz789def456ghi";
        let masked = ApiKeyState::mask_key(key);
        assert_eq!(masked, "sk-ant...i");
    }

    #[test]
    fn test_api_key_masking_short_key() {
        let key = "short";
        let masked = ApiKeyState::mask_key(key);
        assert_eq!(masked, "****");
    }

    #[test]
    fn test_api_key_state_display() {
        let not_configured = ApiKeyState::NotConfigured;
        assert_eq!(not_configured.status_icon(), "⚠");

        let configured = ApiKeyState::Configured {
            masked: "sk-...xyz".into(),
        };
        assert_eq!(configured.status_icon(), "✓");

        let invalid = ApiKeyState::Invalid {
            masked: "sk-...xyz".into(),
            error: "Bad".into(),
        };
        assert_eq!(invalid.status_icon(), "✗");
    }

    #[test]
    fn test_download_progress_percentage() {
        let state = DownloadState::Downloading {
            model: "llama3.2".into(),
            progress: 0.45,
            downloaded: 2_100_000_000,
            total: 4_700_000_000,
        };
        assert_eq!(state.percentage(), 45);
    }

    #[test]
    fn test_download_state_is_active() {
        assert!(!DownloadState::Idle.is_active());
        assert!(DownloadState::Downloading {
            model: "".into(),
            progress: 0.0,
            downloaded: 0,
            total: 0
        }
        .is_active());
        assert!(!DownloadState::Complete { model: "".into() }.is_active());
        assert!(!DownloadState::Failed {
            model: "".into(),
            error: "".into()
        }
        .is_active());
    }

    #[test]
    fn test_download_format_bytes() {
        assert_eq!(DownloadState::format_bytes(4_700_000_000), "4.7 GB");
        assert_eq!(DownloadState::format_bytes(500_000_000), "500.0 MB");
        assert_eq!(DownloadState::format_bytes(1_500), "1.5 KB");
        assert_eq!(DownloadState::format_bytes(500), "500 B");
    }

    #[test]
    fn test_modal_state_default() {
        let state = ProviderModalState::default();
        assert!(!state.visible);
        assert_eq!(state.active_tab, ProviderModalTab::Cloud);
        assert_eq!(state.selected_idx, 0);
    }

    #[test]
    fn test_modal_toggle_visibility() {
        let mut state = ProviderModalState::default();
        assert!(!state.visible);

        state.toggle();
        assert!(state.visible);

        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn test_modal_open_close() {
        let mut state = ProviderModalState::default();
        state.open();
        assert!(state.visible);
        state.close();
        assert!(!state.visible);
    }

    #[test]
    fn test_modal_tab_switch() {
        let mut state = ProviderModalState::default();
        state.selected_idx = 3;
        state.switch_tab(ProviderModalTab::Native);

        assert_eq!(state.active_tab, ProviderModalTab::Native);
        assert_eq!(state.selected_idx, 0); // Reset on tab switch
    }

    #[test]
    fn test_modal_navigate_list_mode_wrapping() {
        // Use Keys tab for list navigation test (not grid)
        let mut state = ProviderModalState::default();
        state.switch_tab(ProviderModalTab::Keys); // Keys tab uses list navigation
        state.item_count = 5;

        assert_eq!(state.selected_idx, 0);

        state.navigate_down();
        assert_eq!(state.selected_idx, 1);

        state.navigate_down();
        state.navigate_down();
        state.navigate_down();
        assert_eq!(state.selected_idx, 4);

        // Wraps to first
        state.navigate_down();
        assert_eq!(state.selected_idx, 0);

        // Wraps to last when at first and going up
        state.navigate_up();
        assert_eq!(state.selected_idx, 4);

        state.navigate_up();
        assert_eq!(state.selected_idx, 3);
    }

    #[test]
    fn test_modal_navigate_grid_mode_wrapping() {
        // Cloud tab (default) uses 3-column grid navigation with wrapping (6 providers)
        let mut state = ProviderModalState::default();
        // Default is Cloud tab with item_count = 6

        // Navigation uses 3-column grid layout (v0.27: 2x3 grid, Ollama removed):
        //   0 1 2  (row 0: Claude, OpenAI, Mistral)
        //   3 4 5  (row 1: Groq, DeepSeek, Gemini)

        assert_eq!(state.selected_idx, 0);
        assert_eq!(state.active_tab, ProviderModalTab::Cloud);

        // Navigate right: 0 -> 1 -> 2 -> wraps to 0
        state.navigate_right();
        assert_eq!(state.selected_idx, 1);
        state.navigate_right();
        assert_eq!(state.selected_idx, 2);
        state.navigate_right();
        assert_eq!(state.selected_idx, 0); // Wraps to start of row

        // Navigate down: 0 -> 3 -> wraps to 0 (v0.27: 2x3 grid, no row 2)
        state.navigate_down();
        assert_eq!(state.selected_idx, 3);
        state.navigate_down();
        assert_eq!(state.selected_idx, 0); // Wraps: col 0, row 0

        // Navigate to position 2, then down wraps to same column
        state.selected_idx = 2;
        state.navigate_down();
        assert_eq!(state.selected_idx, 5);
        state.navigate_down();
        assert_eq!(state.selected_idx, 2); // Wraps to row 0 col 2

        // Navigate left wrapping: 3 -> wraps to 5
        state.selected_idx = 3;
        state.navigate_left();
        assert_eq!(state.selected_idx, 5); // Wraps to end of row

        // Navigate up wrapping: 0 -> wraps to 3 (v0.27: last row with col 0)
        state.selected_idx = 0;
        state.navigate_up();
        assert_eq!(state.selected_idx, 3); // Wraps: col 0 last is idx 3

        // Navigate up wrapping: 1 -> wraps to 4 (last row with col 1 is row 1)
        state.selected_idx = 1;
        state.navigate_up();
        assert_eq!(state.selected_idx, 4); // Wraps: col 1 last is idx 4

        // Navigate up wrapping: 2 -> wraps to 5 (last row with col 2 is row 1)
        state.selected_idx = 2;
        state.navigate_up();
        assert_eq!(state.selected_idx, 5); // Wraps: col 2 last is idx 5
    }

    // SEC-004: Debug redacts key_input_buffer
    #[test]
    fn test_modal_state_debug_redacts_key_buffer() {
        let mut state = ProviderModalState::default();
        state.key_input_buffer = "sk-ant-secret-key-12345".to_string();

        let debug_output = format!("{:?}", state);

        // API key should NOT appear in debug output
        assert!(!debug_output.contains("sk-ant"));
        assert!(!debug_output.contains("secret"));
        // Should show [REDACTED] instead
        assert!(debug_output.contains("[REDACTED]"));
    }

    #[test]
    fn test_modal_close_clears_input() {
        let mut state = ProviderModalState::default();
        state.open();
        state.key_input_mode = true;
        state.key_input_buffer = "sk-ant-test".to_string();

        state.close();

        assert!(!state.visible);
        assert!(!state.key_input_mode);
        assert!(state.key_input_buffer.is_empty());
    }

    #[test]
    fn test_set_provider_status_by_index() {
        let mut state = ProviderModalState::default();
        state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
        state.set_provider_status(
            1,
            ConnectionStatus::Failed {
                error: "No key".into(),
            },
        );

        assert_eq!(state.provider_statuses.len(), 2);
        assert!(matches!(
            state.provider_statuses[0],
            ConnectionStatus::Connected { latency_ms: 100 }
        ));
    }

    #[test]
    fn test_set_provider_status_by_name() {
        let mut state = ProviderModalState::default();
        state.set_provider_status_by_name(
            "anthropic",
            ConnectionStatus::Connected { latency_ms: 150 },
        );
        state.set_provider_status_by_name("openai", ConnectionStatus::Checking);
        state.set_provider_status_by_name(
            "claude",
            ConnectionStatus::Failed {
                error: "Updated".into(),
            },
        ); // Same as anthropic (index 0)

        assert!(matches!(
            state.provider_statuses[0],
            ConnectionStatus::Failed { .. }
        ));
        assert!(matches!(
            state.provider_statuses[1],
            ConnectionStatus::Checking
        ));
    }

    #[test]
    fn test_set_provider_status_by_name_all_providers() {
        let mut state = ProviderModalState::default();
        state.set_provider_status_by_name(
            "anthropic",
            ConnectionStatus::Connected { latency_ms: 1 },
        );
        state.set_provider_status_by_name("openai", ConnectionStatus::Connected { latency_ms: 2 });
        state.set_provider_status_by_name("mistral", ConnectionStatus::Connected { latency_ms: 3 });
        state.set_provider_status_by_name("groq", ConnectionStatus::Connected { latency_ms: 4 });
        state
            .set_provider_status_by_name("deepseek", ConnectionStatus::Connected { latency_ms: 5 });
        state.set_provider_status_by_name("gemini", ConnectionStatus::Connected { latency_ms: 6 });
        // v0.27: Native is not a cloud provider, handled separately

        assert_eq!(state.provider_statuses.len(), 6);
    }

    #[test]
    fn test_get_provider_statuses_returns_6() {
        let state = ProviderModalState::default();
        let statuses = state.get_provider_statuses();
        assert_eq!(statuses.len(), 6); // v0.27: 6 cloud providers
                                       // All should be Unknown by default
        assert!(statuses
            .iter()
            .all(|s| matches!(s, ConnectionStatus::Unknown)));
    }

    #[test]
    fn test_get_provider_statuses_with_partial_data() {
        let mut state = ProviderModalState::default();
        state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
        state.set_provider_status(2, ConnectionStatus::Checking);

        let statuses = state.get_provider_statuses();
        assert_eq!(statuses.len(), 6); // v0.27: 6 cloud providers
        assert!(matches!(statuses[0], ConnectionStatus::Connected { .. }));
        assert!(matches!(statuses[1], ConnectionStatus::Unknown));
        assert!(matches!(statuses[2], ConnectionStatus::Checking));
    }

    #[test]
    fn test_set_native_models() {
        // NativeModelDetails and NativeModelInfo are defined in this file

        let mut state = ProviderModalState::default();
        let models = vec![NativeModelInfo {
            name: "llama3.2".to_string(),
            size: 4_700_000_000,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-02-24".to_string(),
            details: NativeModelDetails {
                parameter_size: "8B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }];

        state.set_native_models(models);
        assert_eq!(state.native_models.len(), 1);
        assert_eq!(state.native_models[0].name, "llama3.2");
    }

    #[test]
    fn test_modal_state_default_has_empty_statuses() {
        let state = ProviderModalState::default();
        assert!(state.provider_statuses.is_empty());
        assert!(state.native_models.is_empty());
    }

    #[test]
    fn test_process_loader_event_provider_status() {
        use super::super::loader::LoaderEvent;

        let mut state = ProviderModalState::default();
        state.process_loader_event(LoaderEvent::ProviderStatus {
            provider: "anthropic",
            status: ConnectionStatus::Connected { latency_ms: 120 },
        });

        let statuses = state.get_provider_statuses();
        assert!(matches!(
            statuses[0],
            ConnectionStatus::Connected { latency_ms: 120 }
        ));
    }

    #[test]
    fn test_process_loader_event_native_available() {
        use super::super::loader::LoaderEvent;

        let mut state = ProviderModalState::default();
        assert!(!state.native_available);

        state.process_loader_event(LoaderEvent::NativeAvailable(true));
        assert!(state.native_available);

        state.process_loader_event(LoaderEvent::NativeAvailable(false));
        assert!(!state.native_available);
    }

    #[test]
    fn test_process_loader_event_native_models() {
        use super::super::loader::LoaderEvent;
        // NativeModelDetails and NativeModelInfo are defined in this file

        let mut state = ProviderModalState::default();
        let models = vec![NativeModelInfo {
            name: "llama3.2".to_string(),
            size: 4_700_000_000,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-02-24".to_string(),
            details: NativeModelDetails {
                parameter_size: "8B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }];

        state.process_loader_event(LoaderEvent::NativeModels(models));
        assert_eq!(state.native_models.len(), 1);
    }

    #[test]
    fn test_process_loader_event_providers_complete() {
        use super::super::loader::LoaderEvent;

        let mut state = ProviderModalState::default();
        // Should not panic
        state.process_loader_event(LoaderEvent::ProvidersComplete);
    }

    #[test]
    fn test_process_loader_event_error() {
        use super::super::loader::LoaderEvent;

        let mut state = ProviderModalState::default();
        // Should not panic, just log
        state.process_loader_event(LoaderEvent::Error {
            source: "native".to_string(),
            message: "Connection refused".to_string(),
        });
    }

    #[test]
    fn test_active_model_and_tab_label() {
        let mut state = ProviderModalState::default();
        assert!(state.active_model.is_none());
        assert_eq!(state.cloud_tab_label(), "☁️  CLOUD");

        state.set_active_model("claude-sonnet-4-6");
        assert_eq!(state.active_model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(state.cloud_tab_label(), "☁️  CLOUD [claude-sonnet-4-6]");
    }

    #[test]
    fn test_active_model_long_name_truncated() {
        let mut state = ProviderModalState::default();
        state.set_active_model("claude-3-5-sonnet-latest-version-2025");
        // Should truncate to 17 chars + "..." (threshold is >20)
        let label = state.cloud_tab_label();
        assert!(label.contains("..."));
        assert!(label.len() < 50);
    }

    #[test]
    fn test_animation_frame_cycles() {
        let mut state = ProviderModalState::default();
        assert_eq!(state.animation_frame, 0);

        state.tick_animation();
        assert_eq!(state.animation_frame, 1);

        // Should cycle through indicators
        for _ in 0..100 {
            state.tick_animation();
        }
        // Should not panic and indicator should be valid
        let indicator = state.active_indicator();
        assert!(!indicator.is_empty());
    }

    #[test]
    fn test_active_indicator_returns_valid_chars() {
        let mut state = ProviderModalState::default();
        let valid_chars = ["★", "✦", "●", "◆", "✧", "◉", "✴", "❋"];

        for _ in 0..32 {
            let indicator = state.active_indicator();
            assert!(valid_chars.contains(&indicator));
            state.tick_animation();
        }
    }

    #[test]
    fn test_native_tab_label_empty() {
        let state = ProviderModalState::default();
        assert_eq!(state.native_tab_label(), "🦙 NATIVE");
    }

    #[test]
    fn test_native_tab_label_with_models() {
        use super::NativeModelDetails;

        let mut state = ProviderModalState::default();
        let details = NativeModelDetails {
            parameter_size: "7B".to_string(),
            quantization_level: "Q4_0".to_string(),
            family: Some("llama".to_string()),
        };
        state.native_models = vec![
            NativeModelInfo {
                name: "llama3.2".to_string(),
                size: 4_200_000_000,
                digest: "sha256:abc123".to_string(),
                modified_at: "2024-01-01".to_string(),
                details: details.clone(),
            },
            NativeModelInfo {
                name: "codellama".to_string(),
                size: 3_800_000_000,
                digest: "sha256:def456".to_string(),
                modified_at: "2024-01-01".to_string(),
                details,
            },
        ];
        assert_eq!(state.native_tab_label(), "🦙 NATIVE (2)");
    }

    #[test]
    fn test_keys_tab_label_empty() {
        let state = ProviderModalState::default();
        assert_eq!(state.keys_tab_label(), "🔐 KEYS");
    }

    #[test]
    fn test_keys_tab_label_with_verified() {
        let mut state = ProviderModalState::default();
        state.provider_statuses = vec![
            ConnectionStatus::Connected { latency_ms: 100 },
            ConnectionStatus::Connected { latency_ms: 150 },
            ConnectionStatus::NotConfigured,
        ];
        // 2 verified out of 6 max
        assert!(state.keys_tab_label().contains("2/6"));
    }

    // v0.8.9: Latency history tests
    #[test]
    fn test_latency_history_default_empty() {
        let state = ProviderModalState::default();
        assert_eq!(state.latency_history.len(), 6); // v0.27: 6 cloud providers
        for history in &state.latency_history {
            assert!(history.is_empty());
        }
    }

    #[test]
    fn test_push_latency_adds_sample() {
        let mut state = ProviderModalState::default();
        state.push_latency(0, 100);
        state.push_latency(0, 150);
        state.push_latency(0, 120);

        let history = state.get_latency_history(0);
        assert_eq!(history, &[100, 150, 120]);
    }

    #[test]
    fn test_push_latency_respects_max() {
        let mut state = ProviderModalState::default();
        // Push more than max (10)
        for i in 0..15 {
            state.push_latency(0, i as u64 * 10);
        }

        let history = state.get_latency_history(0);
        assert_eq!(history.len(), 10);
        // Should have kept the last 10 values (50, 60, ..., 140)
        assert_eq!(history[0], 50);
        assert_eq!(history[9], 140);
    }

    #[test]
    fn test_push_latency_by_name() {
        let mut state = ProviderModalState::default();
        state.push_latency_by_name("anthropic", 100);
        state.push_latency_by_name("claude", 120); // Same as anthropic
        state.push_latency_by_name("openai", 200);
        state.push_latency_by_name("mistral", 80);

        assert_eq!(state.get_latency_history(0), &[100, 120]); // claude/anthropic
        assert_eq!(state.get_latency_history(1), &[200]); // openai
        assert_eq!(state.get_latency_history(2), &[80]); // mistral
    }

    #[test]
    fn test_push_latency_invalid_index_ignored() {
        let mut state = ProviderModalState::default();
        state.push_latency(10, 100); // Invalid
        state.push_latency_by_name("unknown", 100); // Invalid

        // No history added
        for history in &state.latency_history {
            assert!(history.is_empty());
        }
    }

    #[test]
    fn test_get_latency_history_invalid_index() {
        let state = ProviderModalState::default();
        let empty: &[u64] = &[];
        assert_eq!(state.get_latency_history(10), empty); // Invalid
    }

    #[test]
    fn test_set_provider_status_pushes_latency() {
        let mut state = ProviderModalState::default();
        state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 150 });
        state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 180 });

        let history = state.get_latency_history(0);
        assert_eq!(history, &[150, 180]);
    }

    #[test]
    fn test_set_provider_status_by_name_pushes_latency() {
        let mut state = ProviderModalState::default();
        state
            .set_provider_status_by_name("openai", ConnectionStatus::Connected { latency_ms: 200 });

        let history = state.get_latency_history(1);
        assert_eq!(history, &[200]);
    }

    #[test]
    fn test_latency_history_not_pushed_for_non_connected() {
        let mut state = ProviderModalState::default();
        state.set_provider_status(0, ConnectionStatus::Checking);
        state.set_provider_status(
            0,
            ConnectionStatus::Failed {
                error: "timeout".into(),
            },
        );
        state.set_provider_status(0, ConnectionStatus::NotConfigured);

        let history = state.get_latency_history(0);
        assert!(history.is_empty());
    }

    #[test]
    fn test_get_session_stats_empty() {
        let state = ProviderModalState::default();
        let stats = state.get_session_stats();

        assert_eq!(stats.connected_providers, 0);
        assert_eq!(stats.total_providers, 6); // v0.27: 6 cloud providers
        assert_eq!(stats.tokens_used, 0);
        assert!(stats.avg_latency_ms.is_none());
    }

    #[test]
    fn test_get_session_stats_with_connections() {
        let mut state = ProviderModalState::default();
        state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
        state.set_provider_status(1, ConnectionStatus::Connected { latency_ms: 200 });
        state.set_provider_status(2, ConnectionStatus::NotConfigured);

        let stats = state.get_session_stats();

        assert_eq!(stats.connected_providers, 2);
        assert_eq!(stats.total_providers, 6); // v0.27: 6 cloud providers
                                              // Average of 100 and 200 is 150
        assert_eq!(stats.avg_latency_ms, Some(150));
    }

    // v0.8.9: Verification state tests
    #[test]
    fn test_verification_state_default() {
        // v0.27: Ollama removed
        let state = ProviderModalState::default();
        assert!(!state.verification_active);
        assert_eq!(state.verification_state.entries.len(), 6);
    }

    #[test]
    fn test_start_verification() {
        let mut state = ProviderModalState::default();
        state.start_verification();

        assert!(state.verification_active);
        // All entries should be reset to Checking
        for entry in &state.verification_state.entries {
            assert_eq!(
                entry.status,
                super::super::components::VerifyStatus::Checking
            );
            assert_eq!(entry.progress, 0.0);
        }
    }

    #[test]
    fn test_sync_verification_status() {
        use super::super::components::VerifyStatus;

        let mut state = ProviderModalState::default();
        state.set_provider_status(0, ConnectionStatus::Connected { latency_ms: 100 });
        state.set_provider_status(
            1,
            ConnectionStatus::Failed {
                error: "Timeout".into(),
            },
        );
        state.set_provider_status(2, ConnectionStatus::NotConfigured);

        // Verification status should be synced
        assert_eq!(
            state.verification_state.entries[0].status,
            VerifyStatus::Connected
        );
        assert_eq!(
            state.verification_state.entries[1].status,
            VerifyStatus::Failed
        );
        assert_eq!(
            state.verification_state.entries[2].status,
            VerifyStatus::NotConfigured
        );
    }

    #[test]
    fn test_tick_animation_advances_verification() {
        let mut state = ProviderModalState::default();
        state.start_verification();

        let initial_frame = state.verification_state.frame;
        state.tick_animation();

        assert_eq!(state.verification_state.frame, initial_frame + 1);
    }

    #[test]
    fn test_verification_auto_deactivates_when_complete() {
        use super::super::components::VerifyStatus;

        let mut state = ProviderModalState::default();
        state.start_verification();
        assert!(state.verification_active);

        // Set all 6 cloud providers to connected (v0.27)
        for i in 0..6 {
            state
                .verification_state
                .set_status(i, VerifyStatus::Connected);
        }

        // Tick until complete
        for _ in 0..30 {
            state.tick_animation();
        }

        // Should auto-deactivate when all complete
        assert!(!state.verification_active);
    }

    #[test]
    fn test_sync_all_verification_statuses() {
        use super::super::components::VerifyStatus;

        let mut state = ProviderModalState::default();

        // Set provider statuses directly (bypassing set_provider_status)
        state.provider_statuses = vec![
            ConnectionStatus::Connected { latency_ms: 100 },
            ConnectionStatus::Checking,
            ConnectionStatus::Failed {
                error: "err".into(),
            },
            ConnectionStatus::NotConfigured,
            ConnectionStatus::Unknown,
            ConnectionStatus::Connected { latency_ms: 200 },
        ];

        state.sync_all_verification_statuses();

        assert_eq!(
            state.verification_state.entries[0].status,
            VerifyStatus::Connected
        );
        assert_eq!(
            state.verification_state.entries[1].status,
            VerifyStatus::Checking
        );
        assert_eq!(
            state.verification_state.entries[2].status,
            VerifyStatus::Failed
        );
        assert_eq!(
            state.verification_state.entries[3].status,
            VerifyStatus::NotConfigured
        );
        assert_eq!(
            state.verification_state.entries[4].status,
            VerifyStatus::Checking
        );
        assert_eq!(
            state.verification_state.entries[5].status,
            VerifyStatus::Connected
        );
    }

    // ═══ TESTS: Session Token Tracking ═══

    #[test]
    fn test_set_session_tokens_updates_stats() {
        let mut state = ProviderModalState::default();

        // Initially tokens should be 0
        assert_eq!(state.get_session_stats().tokens_used, 0);

        // Set session tokens
        state.set_session_tokens(12345);

        // Stats should now reflect the tokens
        let stats = state.get_session_stats();
        assert_eq!(
            stats.tokens_used, 12345,
            "Session stats should reflect set tokens"
        );
    }

    #[test]
    fn test_session_tokens_update_multiple_times() {
        let mut state = ProviderModalState::default();

        state.set_session_tokens(100);
        assert_eq!(state.get_session_stats().tokens_used, 100);

        state.set_session_tokens(500);
        assert_eq!(state.get_session_stats().tokens_used, 500);

        state.set_session_tokens(0);
        assert_eq!(state.get_session_stats().tokens_used, 0);
    }

    // ═══ TESTS: MCP Connection Status ═══

    #[test]
    fn test_set_mcp_connections_updates_stats() {
        let mut state = ProviderModalState::default();

        // Initially MCP connections should be 0
        assert_eq!(state.get_session_stats().mcp_connections, 0);

        // Set MCP connection count
        state.set_mcp_connections(3);

        // Stats should now reflect the connections
        let stats = state.get_session_stats();
        assert_eq!(
            stats.mcp_connections, 3,
            "Session stats should reflect MCP connections"
        );
    }

    #[test]
    fn test_mcp_connections_update_multiple_times() {
        let mut state = ProviderModalState::default();

        state.set_mcp_connections(2);
        assert_eq!(state.get_session_stats().mcp_connections, 2);

        state.set_mcp_connections(5);
        assert_eq!(state.get_session_stats().mcp_connections, 5);

        state.set_mcp_connections(0);
        assert_eq!(state.get_session_stats().mcp_connections, 0);
    }
}
