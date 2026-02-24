//! Provider modal state types

/// Active tab in the provider modal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderModalTab {
    #[default]
    Cloud,
    Ollama,
    Keys,
    Config,
}

impl ProviderModalTab {
    /// Get next tab (cycles)
    pub fn next(self) -> Self {
        match self {
            Self::Cloud => Self::Ollama,
            Self::Ollama => Self::Keys,
            Self::Keys => Self::Config,
            Self::Config => Self::Cloud,
        }
    }

    /// Get previous tab (cycles)
    pub fn prev(self) -> Self {
        match self {
            Self::Cloud => Self::Config,
            Self::Ollama => Self::Cloud,
            Self::Keys => Self::Ollama,
            Self::Config => Self::Keys,
        }
    }

    /// Create from keyboard key
    pub fn from_key(c: char) -> Option<Self> {
        match c {
            '1' => Some(Self::Cloud),
            '2' => Some(Self::Ollama),
            '3' => Some(Self::Keys),
            '4' => Some(Self::Config),
            _ => None,
        }
    }

    /// Tab label for display
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cloud => "☁️  CLOUD",
            Self::Ollama => "🦙 OLLAMA",
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
    /// Key is stored (masked for display)
    Configured { masked: String },
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
            Self::Configured { .. } => "✓",
            Self::Verified { .. } => "✓",
            Self::Invalid { .. } => "✗",
        }
    }

    /// Check if key is configured (valid or not)
    pub fn is_configured(&self) -> bool {
        !matches!(self, Self::NotConfigured)
    }
}

/// Download state for Ollama model pulls
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
    /// Download state for Ollama
    pub download_state: DownloadState,
    /// Key input mode active
    pub key_input_mode: bool,
    /// Key input buffer (may contain sensitive API keys)
    pub key_input_buffer: String,
    /// Whether Ollama is available (running)
    pub ollama_available: bool,
    /// Provider connection statuses (6 providers)
    pub provider_statuses: Vec<ConnectionStatus>,
    /// Ollama models loaded from server
    pub ollama_models: Vec<super::ollama_client::OllamaModelInfo>,
    /// Currently active provider index (the one being used for inference)
    pub active_provider_idx: Option<usize>,
    /// Currently active model name for display
    pub active_model: Option<String>,
    /// Animation frame counter for active provider cycling effect
    pub animation_frame: u8,
}

impl Default for ProviderModalState {
    fn default() -> Self {
        Self {
            visible: false,
            active_tab: ProviderModalTab::Cloud,
            selected_idx: 0,
            item_count: 6, // Cloud tab has 6 providers by default
            download_state: DownloadState::default(),
            key_input_mode: false,
            key_input_buffer: String::new(),
            ollama_available: false,
            provider_statuses: Vec::new(),
            ollama_models: Vec::new(),
            active_provider_idx: None,
            active_model: None,
            animation_frame: 0,
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
            .field("ollama_available", &self.ollama_available)
            .field("provider_statuses", &self.provider_statuses)
            .field(
                "ollama_models",
                &format!("[{} models]", self.ollama_models.len()),
            )
            .field("active_provider_idx", &self.active_provider_idx)
            .field("active_model", &self.active_model)
            .field("animation_frame", &self.animation_frame)
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
            ProviderModalTab::Cloud => 6, // 6 cloud providers
            ProviderModalTab::Ollama => self.ollama_models.len().max(1), // Dynamic
            ProviderModalTab::Keys => 6,  // 6 API key entries
            ProviderModalTab::Config => 6, // 6 config entries (matches ConfigTab::new())
        };
    }

    /// Navigate up in current list/grid (wraps around)
    /// For Cloud tab (2x3 grid): moves up one row (idx - 3), wraps to bottom
    /// For other tabs: moves up one item (idx - 1), wraps to last
    pub fn navigate_up(&mut self) {
        if self.item_count == 0 {
            return;
        }
        if self.active_tab == ProviderModalTab::Cloud {
            // 2x3 grid: move up one row, wrap to bottom
            if self.selected_idx >= 3 {
                self.selected_idx -= 3;
            } else {
                // Wrap to bottom row, same column
                self.selected_idx += 3;
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
    /// For Cloud tab (2x3 grid): moves down one row (idx + 3), wraps to top
    /// For other tabs: moves down one item (idx + 1), wraps to first
    pub fn navigate_down(&mut self) {
        if self.item_count == 0 {
            return;
        }
        if self.active_tab == ProviderModalTab::Cloud {
            // 2x3 grid: move down one row, wrap to top
            if self.selected_idx + 3 < self.item_count {
                self.selected_idx += 3;
            } else {
                // Wrap to top row, same column
                self.selected_idx -= 3;
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
            "ollama" => Some(5),
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
            Some(5) => Some("Ollama"),
            _ => None,
        }
    }

    /// Set the active model name
    pub fn set_active_model(&mut self, model: impl Into<String>) {
        self.active_model = Some(model.into());
    }

    /// Tick animation frame (call on each frame update)
    pub fn tick_animation(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
    }

    /// Get animated indicator for active provider
    /// Returns cycling ASCII characters: ★ ✦ ● ◆ ✧
    pub fn active_indicator(&self) -> &'static str {
        const FRAMES: &[&str] = &["★", "✦", "●", "◆", "✧", "◉", "✴", "❋"];
        FRAMES[(self.animation_frame as usize / 4) % FRAMES.len()]
    }

    /// Get Cloud tab label with active model if set
    pub fn cloud_tab_label(&self) -> String {
        if let Some(ref model) = self.active_model {
            // Shorten model name for display (keep up to 20 chars)
            let short = if model.len() > 20 {
                format!("{}...", &model[..17])
            } else {
                model.clone()
            };
            format!("☁️  CLOUD [{}]", short)
        } else {
            "☁️  CLOUD".to_string()
        }
    }

    /// Update provider status by index
    pub fn set_provider_status(&mut self, index: usize, status: ConnectionStatus) {
        // Ensure vector is large enough
        while self.provider_statuses.len() <= index {
            self.provider_statuses.push(ConnectionStatus::Unknown);
        }
        self.provider_statuses[index] = status;
    }

    /// Update provider status by name
    pub fn set_provider_status_by_name(&mut self, name: &str, status: ConnectionStatus) {
        let index = match name.to_lowercase().as_str() {
            "anthropic" | "claude" => 0,
            "openai" => 1,
            "mistral" => 2,
            "groq" => 3,
            "deepseek" => 4,
            "ollama" => 5,
            _ => return,
        };
        self.set_provider_status(index, status);
    }

    /// Get provider statuses for CloudTab
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

    /// Set Ollama models
    pub fn set_ollama_models(&mut self, models: Vec<super::ollama_client::OllamaModelInfo>) {
        self.ollama_models = models;
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
            LoaderEvent::OllamaAvailable(available) => {
                self.ollama_available = available;
            }
            LoaderEvent::OllamaModels(models) => {
                self.ollama_models = models;
            }
            LoaderEvent::Error { source, message } => {
                // Handle errors - could show in status bar or notification
                tracing::warn!("Loader error from {}: {}", source, message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_default_is_cloud() {
        assert_eq!(ProviderModalTab::default(), ProviderModalTab::Cloud);
    }

    #[test]
    fn test_tab_next_cycles() {
        assert_eq!(ProviderModalTab::Cloud.next(), ProviderModalTab::Ollama);
        assert_eq!(ProviderModalTab::Ollama.next(), ProviderModalTab::Keys);
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
            Some(ProviderModalTab::Ollama)
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
        assert_eq!(ProviderModalTab::Ollama.label(), "🦙 OLLAMA");
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
        state.switch_tab(ProviderModalTab::Ollama);

        assert_eq!(state.active_tab, ProviderModalTab::Ollama);
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
        // Cloud tab (default) uses 2x3 grid navigation with wrapping
        let mut state = ProviderModalState::default();
        // Default is Cloud tab with item_count = 6

        // Grid layout:
        //   0 1 2  (row 0)
        //   3 4 5  (row 1)

        assert_eq!(state.selected_idx, 0);
        assert_eq!(state.active_tab, ProviderModalTab::Cloud);

        // Navigate right: 0 -> 1 -> 2 -> wraps to 0
        state.navigate_right();
        assert_eq!(state.selected_idx, 1);
        state.navigate_right();
        assert_eq!(state.selected_idx, 2);
        state.navigate_right();
        assert_eq!(state.selected_idx, 0); // Wraps to start of row

        // Navigate down: 0 -> 3 -> wraps to 0
        state.navigate_down();
        assert_eq!(state.selected_idx, 3);
        state.navigate_down();
        assert_eq!(state.selected_idx, 0); // Wraps to top

        // Navigate to position 5, then down wraps
        state.navigate_right();
        state.navigate_right();
        state.navigate_down();
        assert_eq!(state.selected_idx, 5);
        state.navigate_down();
        assert_eq!(state.selected_idx, 2); // Wraps to top, same column

        // Navigate left wrapping: 5 -> 4 -> 3 -> wraps to 5
        state.selected_idx = 3;
        state.navigate_left();
        assert_eq!(state.selected_idx, 5); // Wraps to end of row

        // Navigate up wrapping: 0 -> wraps to 3
        state.selected_idx = 0;
        state.navigate_up();
        assert_eq!(state.selected_idx, 3); // Wraps to bottom, same column
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
        state.set_provider_status_by_name("ollama", ConnectionStatus::Connected { latency_ms: 6 });

        assert_eq!(state.provider_statuses.len(), 6);
    }

    #[test]
    fn test_get_provider_statuses_returns_6() {
        let state = ProviderModalState::default();
        let statuses = state.get_provider_statuses();
        assert_eq!(statuses.len(), 6);
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
        assert_eq!(statuses.len(), 6);
        assert!(matches!(statuses[0], ConnectionStatus::Connected { .. }));
        assert!(matches!(statuses[1], ConnectionStatus::Unknown));
        assert!(matches!(statuses[2], ConnectionStatus::Checking));
    }

    #[test]
    fn test_set_ollama_models() {
        use super::super::ollama_client::{OllamaModelDetails, OllamaModelInfo};

        let mut state = ProviderModalState::default();
        let models = vec![OllamaModelInfo {
            name: "llama3.2".to_string(),
            size: 4_700_000_000,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-02-24".to_string(),
            details: OllamaModelDetails {
                parameter_size: "8B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }];

        state.set_ollama_models(models);
        assert_eq!(state.ollama_models.len(), 1);
        assert_eq!(state.ollama_models[0].name, "llama3.2");
    }

    #[test]
    fn test_modal_state_default_has_empty_statuses() {
        let state = ProviderModalState::default();
        assert!(state.provider_statuses.is_empty());
        assert!(state.ollama_models.is_empty());
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
    fn test_process_loader_event_ollama_available() {
        use super::super::loader::LoaderEvent;

        let mut state = ProviderModalState::default();
        assert!(!state.ollama_available);

        state.process_loader_event(LoaderEvent::OllamaAvailable(true));
        assert!(state.ollama_available);

        state.process_loader_event(LoaderEvent::OllamaAvailable(false));
        assert!(!state.ollama_available);
    }

    #[test]
    fn test_process_loader_event_ollama_models() {
        use super::super::loader::LoaderEvent;
        use super::super::ollama_client::{OllamaModelDetails, OllamaModelInfo};

        let mut state = ProviderModalState::default();
        let models = vec![OllamaModelInfo {
            name: "llama3.2".to_string(),
            size: 4_700_000_000,
            digest: "sha256:abc".to_string(),
            modified_at: "2026-02-24".to_string(),
            details: OllamaModelDetails {
                parameter_size: "8B".to_string(),
                quantization_level: "Q4_0".to_string(),
                family: Some("llama".to_string()),
            },
        }];

        state.process_loader_event(LoaderEvent::OllamaModels(models));
        assert_eq!(state.ollama_models.len(), 1);
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
            source: "ollama".to_string(),
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
}
