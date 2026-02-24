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
}
