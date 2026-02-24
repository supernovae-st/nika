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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Not yet checked
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

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Unknown
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyState {
    /// No key configured
    NotConfigured,
    /// Key is stored (masked for display)
    Configured { masked: String },
    /// Key verified working with latency
    Verified { masked: String, latency_ms: u64 },
    /// Key is invalid
    Invalid { masked: String, error: String },
}

impl Default for ApiKeyState {
    fn default() -> Self {
        Self::NotConfigured
    }
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
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadState {
    /// No download in progress
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

impl Default for DownloadState {
    fn default() -> Self {
        Self::Idle
    }
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
        assert_eq!(ProviderModalTab::from_key('1'), Some(ProviderModalTab::Cloud));
        assert_eq!(ProviderModalTab::from_key('2'), Some(ProviderModalTab::Ollama));
        assert_eq!(ProviderModalTab::from_key('3'), Some(ProviderModalTab::Keys));
        assert_eq!(ProviderModalTab::from_key('4'), Some(ProviderModalTab::Config));
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

        let failed = ConnectionStatus::Failed { error: "Timeout".into() };
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

        let configured = ApiKeyState::Configured { masked: "sk-...xyz".into() };
        assert_eq!(configured.status_icon(), "✓");

        let invalid = ApiKeyState::Invalid { masked: "sk-...xyz".into(), error: "Bad".into() };
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
            model: "".into(), progress: 0.0, downloaded: 0, total: 0
        }.is_active());
        assert!(!DownloadState::Complete { model: "".into() }.is_active());
        assert!(!DownloadState::Failed { model: "".into(), error: "".into() }.is_active());
    }

    #[test]
    fn test_download_format_bytes() {
        assert_eq!(DownloadState::format_bytes(4_700_000_000), "4.7 GB");
        assert_eq!(DownloadState::format_bytes(500_000_000), "500.0 MB");
        assert_eq!(DownloadState::format_bytes(1_500), "1.5 KB");
        assert_eq!(DownloadState::format_bytes(500), "500 B");
    }
}
