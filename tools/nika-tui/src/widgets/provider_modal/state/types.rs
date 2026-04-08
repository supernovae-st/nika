// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider modal data types
//!
//! All value types used by the provider modal state machine:
//! native model info, tab enum, connection status, API key state, download state.

/// Native model info for local inference
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

/// Native model details
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
    /// Native local inference (was Native)
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
    /// Key is being saved to vault
    Saving { masked: String },
    /// Key is stored, now testing
    Testing { masked: String },
    /// Key from environment variable (session-based, ephemeral)
    Configured { masked: String },
    /// Key from encrypted vault (persisted, secure)
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
            Self::Stored { .. } => "🔐", // Keyring stored
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
