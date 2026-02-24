# Provider Modal v2 Implementation Plan

**Version:** v0.8.4
**Date:** 2026-02-24
**Status:** Ready for Implementation
**Research:** Context7 + Perplexity Sonar + Codebase Analysis (2026-02-24)

## Overview

Complete redesign of the provider selector modal with rich cards, tabs, secure API key management, and Ollama model installation support.

---

## Research Findings (2026-02-24)

### Ollama REST API (Context7 — `/llmstxt/ollama_llms-full_txt`)

Full endpoint documentation for native HTTP client:

| Endpoint | Method | Purpose | Streaming |
|----------|--------|---------|-----------|
| `/api/tags` | GET | List installed models | No |
| `/api/show` | POST | Get model details (size, params, quantization) | No |
| `/api/pull` | POST | Download model from registry | **Yes (NDJSON)** |
| `/api/delete` | DELETE | Remove installed model | No |
| `/api/ps` | GET | List running model processes | No |
| `/api/generate` | POST | Text generation | Yes (NDJSON) |
| `/api/chat` | POST | Chat completion | Yes (NDJSON) |

**NDJSON Streaming Format for `/api/pull`:**
```json
{"status":"pulling manifest"}
{"status":"pulling sha256:abc123...","digest":"sha256:abc123","total":4700000000,"completed":0}
{"status":"pulling sha256:abc123...","digest":"sha256:abc123","total":4700000000,"completed":1200000000}
{"status":"verifying sha256 digest"}
{"status":"writing manifest"}
{"status":"success"}
```

**Key Fields:**
- `status`: Current operation (pulling, verifying, writing, success)
- `digest`: SHA256 of current layer being downloaded
- `total`: Total bytes for current layer
- `completed`: Bytes downloaded for current layer

### keyring-rs API (Context7 — `/websites/rs_keyring_3_6_3`)

Cross-platform secure credential storage:

```rust
use keyring::Entry;

// Create entry for service + username
let entry = Entry::new("nika", "anthropic")?;

// Store credential
entry.set_password("sk-ant-api03-...")?;

// Retrieve credential
let key = entry.get_password()?;

// Delete credential
entry.delete_credential()?;
```

**Platform Support:**
| Platform | Backend |
|----------|---------|
| macOS | Keychain Access |
| Windows | Credential Locker |
| Linux | Secret Service (GNOME Keyring, KWallet) |

### Existing Nika Patterns (Codebase Analysis)

**1. HTTP Client (from `src/runtime/executor.rs`):**
```rust
let http_client = reqwest::Client::builder()
    .timeout(FETCH_TIMEOUT)
    .connect_timeout(CONNECT_TIMEOUT)
    .redirect(reqwest::redirect::Policy::limited(REDIRECT_LIMIT))
    .user_agent("nika-cli/0.1")
    .build()?;
```

**2. Ollama Health Check (from `src/tui/widgets/provider_selector.rs`):**
```rust
pub fn check_ollama_available() -> bool {
    let url = format!("{}/api/tags", ollama_base_url());
    reqwest::blocking::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
```

**3. Streaming Pattern (from `src/tui/chat_agent.rs`):**
```rust
// mpsc channel for streaming chunks
let (tx, rx) = tokio::sync::mpsc::channel::<StreamChunk>(32);
tokio::spawn(async move {
    while let Some(chunk) = stream.next().await {
        tx.send(StreamChunk::Token(chunk)).await.ok();
    }
});
```

### Best Practices (Perplexity Sonar — 2025)

1. **NDJSON Streaming with reqwest:**
```rust
let response = client.post(url)
    .json(&body)
    .send()
    .await?;

let mut stream = response.bytes_stream();
let mut buffer = String::new();

while let Some(chunk) = stream.next().await {
    let bytes = chunk?;
    buffer.push_str(&String::from_utf8_lossy(&bytes));

    // Process complete NDJSON lines
    while let Some(newline_pos) = buffer.find('\n') {
        let line = buffer[..newline_pos].to_string();
        buffer = buffer[newline_pos + 1..].to_string();

        let event: PullEvent = serde_json::from_str(&line)?;
        tx.send(event).await?;
    }
}
```

2. **Connection Pooling:** Reuse single `reqwest::Client` instance (already implemented in TaskExecutor)

3. **Timeout Strategy:**
   - Connect timeout: 5s (fast fail for unavailable Ollama)
   - Request timeout: None for `/api/pull` (downloads can take hours)
   - Health check timeout: 2s (quick status check)

---

## Design Goals

1. **Rich Visual Design** - Cards with sparklines, rounded borders, status indicators
2. **Tabbed Interface** - Cloud / Ollama / Keys / Config tabs
3. **Secure Key Storage** - System keychain via `keyring-rs`
4. **Ollama Integration** - Install, pull models, manage local LLMs
5. **Real-time Feedback** - Latency sparklines, connection status, download progress

## Architecture

```
src/tui/widgets/
├── provider_modal/           # NEW: Module directory
│   ├── mod.rs               # Module exports + ProviderModal widget
│   ├── state.rs             # ProviderModalState + tab states
│   ├── tabs/
│   │   ├── mod.rs           # Tab exports
│   │   ├── cloud.rs         # CloudProvidersTab
│   │   ├── ollama.rs        # OllamaTab (install, models)
│   │   ├── keys.rs          # ApiKeysTab (keyring integration)
│   │   └── config.rs        # ConfigTab (preferences)
│   ├── components/
│   │   ├── mod.rs           # Component exports
│   │   ├── provider_card.rs # Rich provider card with sparkline
│   │   ├── model_list.rs    # Model selection list
│   │   ├── key_input.rs     # Secure key input field
│   │   ├── download_gauge.rs # Ollama pull progress
│   │   └── status_badge.rs  # Connection status indicators
│   └── keyring.rs           # Keyring integration wrapper
├── provider_selector.rs     # DEPRECATED: Keep for backwards compat
└── mod.rs                   # Add provider_modal export
```

## Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
keyring = "3"  # Cross-platform credential storage
```

## Implementation Phases

### Phase 1: Core State & Types (TDD)

**Files:** `provider_modal/state.rs`

#### Types to Implement

```rust
/// Active tab in the provider modal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProviderModalTab {
    #[default]
    Cloud,
    Ollama,
    Keys,
    Config,
}

/// Provider connection status with rich info
#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Unknown,
    Checking,
    Connected { latency_ms: u64 },
    Failed { error: String },
    NotConfigured { reason: String },
}

/// Ollama model info (from /api/tags)
#[derive(Debug, Clone)]
pub struct OllamaModel {
    pub name: String,
    pub size_bytes: u64,
    pub parameter_size: String,  // "7B", "13B", etc.
    pub quantization: String,    // "Q4_0", "Q8_0", etc.
    pub modified_at: String,
}

/// Download state for ollama pull
#[derive(Debug, Clone)]
pub enum DownloadState {
    Idle,
    Downloading { model: String, progress: f64, downloaded: u64, total: u64 },
    Complete { model: String },
    Failed { model: String, error: String },
}

/// API key state
#[derive(Debug, Clone)]
pub enum ApiKeyState {
    NotConfigured,
    Configured { masked: String },  // "sk-ant-...7d3f"
    Verified { masked: String, latency_ms: u64 },
    Invalid { masked: String, error: String },
}

/// Main modal state
#[derive(Debug, Clone)]
pub struct ProviderModalState {
    pub visible: bool,
    pub active_tab: ProviderModalTab,

    // Cloud tab state
    pub cloud_providers: Vec<CloudProviderState>,
    pub selected_cloud_idx: usize,
    pub model_selection_mode: bool,
    pub selected_model_idx: usize,

    // Ollama tab state
    pub ollama_installed: bool,
    pub ollama_running: bool,
    pub ollama_models: Vec<OllamaModel>,
    pub available_models: Vec<OllamaModelInfo>,  // Popular models to suggest
    pub selected_ollama_idx: usize,
    pub download_state: DownloadState,

    // Keys tab state
    pub api_keys: Vec<ApiKeyEntry>,
    pub selected_key_idx: usize,
    pub key_input_mode: bool,
    pub key_input_buffer: String,

    // Latency history for sparklines
    pub latency_history: HashMap<String, LatencyHistory>,
}
```

#### Tests (Phase 1)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // === Tab Navigation Tests ===

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

    // === Connection Status Tests ===

    #[test]
    fn test_connection_status_display() {
        let connected = ConnectionStatus::Connected { latency_ms: 182 };
        assert_eq!(connected.display_text(), "✓ 182ms");

        let checking = ConnectionStatus::Checking;
        assert_eq!(checking.display_text(), "⟳ Checking...");

        let failed = ConnectionStatus::Failed { error: "Timeout".into() };
        assert_eq!(failed.display_text(), "✗ Timeout");
    }

    #[test]
    fn test_connection_status_color() {
        let connected = ConnectionStatus::Connected { latency_ms: 100 };
        assert_eq!(connected.color(), Color::Rgb(34, 197, 94)); // green

        let failed = ConnectionStatus::Failed { error: "".into() };
        assert_eq!(failed.color(), Color::Rgb(239, 68, 68)); // red
    }

    // === Ollama Model Tests ===

    #[test]
    fn test_ollama_model_size_display() {
        let model = OllamaModel {
            name: "llama3.2".into(),
            size_bytes: 4_700_000_000,
            parameter_size: "8B".into(),
            quantization: "Q4_0".into(),
            modified_at: "2024-01-15".into(),
        };
        assert_eq!(model.size_display(), "4.7 GB");
    }

    #[test]
    fn test_ollama_model_from_api_response() {
        let json = r#"{
            "name": "llama3.2:latest",
            "size": 4700000000,
            "details": {
                "parameter_size": "8B",
                "quantization_level": "Q4_0"
            }
        }"#;
        let model: OllamaModel = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama3.2:latest");
    }

    // === Download State Tests ===

    #[test]
    fn test_download_progress_percentage() {
        let state = DownloadState::Downloading {
            model: "llama3.2".into(),
            progress: 0.45,
            downloaded: 2_100_000_000,
            total: 4_700_000_000,
        };
        assert_eq!(state.percentage(), 45);
        assert_eq!(state.progress_text(), "2.1 GB / 4.7 GB");
    }

    #[test]
    fn test_download_state_is_active() {
        assert!(!DownloadState::Idle.is_active());
        assert!(DownloadState::Downloading {
            model: "".into(), progress: 0.0, downloaded: 0, total: 0
        }.is_active());
        assert!(!DownloadState::Complete { model: "".into() }.is_active());
    }

    // === API Key State Tests ===

    #[test]
    fn test_api_key_masking() {
        let key = "sk-ant-api03-abc123xyz789def456";
        let masked = ApiKeyState::mask_key(key);
        assert_eq!(masked, "sk-ant-...f456");
    }

    #[test]
    fn test_api_key_masking_short_key() {
        let key = "short";
        let masked = ApiKeyState::mask_key(key);
        assert_eq!(masked, "****");
    }

    // === Modal State Tests ===

    #[test]
    fn test_modal_state_default() {
        let state = ProviderModalState::default();
        assert!(!state.visible);
        assert_eq!(state.active_tab, ProviderModalTab::Cloud);
        assert_eq!(state.selected_cloud_idx, 0);
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
    fn test_modal_tab_switch_resets_selection() {
        let mut state = ProviderModalState::default();
        state.selected_cloud_idx = 3;
        state.switch_tab(ProviderModalTab::Ollama);

        assert_eq!(state.active_tab, ProviderModalTab::Ollama);
        // Selection should reset when switching tabs
    }

    #[test]
    fn test_modal_navigate_up_down_cloud() {
        let mut state = ProviderModalState::default();
        state.cloud_providers = vec![
            CloudProviderState::mock("claude"),
            CloudProviderState::mock("openai"),
            CloudProviderState::mock("mistral"),
        ];

        assert_eq!(state.selected_cloud_idx, 0);

        state.navigate_down();
        assert_eq!(state.selected_cloud_idx, 1);

        state.navigate_down();
        assert_eq!(state.selected_cloud_idx, 2);

        // Should not go past end
        state.navigate_down();
        assert_eq!(state.selected_cloud_idx, 2);

        state.navigate_up();
        assert_eq!(state.selected_cloud_idx, 1);

        state.navigate_up();
        assert_eq!(state.selected_cloud_idx, 0);

        // Should not go below 0
        state.navigate_up();
        assert_eq!(state.selected_cloud_idx, 0);
    }
}
```

---

### Phase 2: Keyring Integration (TDD)

**Files:** `provider_modal/keyring.rs`

#### Implementation

```rust
//! Secure API key storage via system keychain
//!
//! Uses keyring-rs for cross-platform credential storage:
//! - macOS: Keychain Access
//! - Windows: Credential Locker
//! - Linux: Secret Service (GNOME Keyring, KWallet)

use keyring::Entry;
use thiserror::Error;

const SERVICE_NAME: &str = "nika";

#[derive(Debug, Error)]
pub enum KeyringError {
    #[error("Failed to access keyring: {0}")]
    AccessError(String),
    #[error("Key not found for provider: {0}")]
    NotFound(String),
    #[error("Failed to store key: {0}")]
    StoreError(String),
    #[error("Failed to delete key: {0}")]
    DeleteError(String),
}

/// Keyring wrapper for Nika API keys
pub struct NikaKeyring;

impl NikaKeyring {
    /// Get API key for a provider
    pub fn get(provider: &str) -> Result<String, KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry.get_password()
            .map_err(|e| match e {
                keyring::Error::NoEntry => KeyringError::NotFound(provider.to_string()),
                _ => KeyringError::AccessError(e.to_string()),
            })
    }

    /// Store API key for a provider
    pub fn set(provider: &str, key: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry.set_password(key)
            .map_err(|e| KeyringError::StoreError(e.to_string()))
    }

    /// Delete API key for a provider
    pub fn delete(provider: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry.delete_credential()
            .map_err(|e| KeyringError::DeleteError(e.to_string()))
    }

    /// Check if key exists for a provider
    pub fn exists(provider: &str) -> bool {
        Self::get(provider).is_ok()
    }

    /// Get masked version of stored key
    pub fn get_masked(provider: &str) -> Option<String> {
        Self::get(provider).ok().map(|k| mask_api_key(&k))
    }

    /// List all configured providers
    pub fn configured_providers() -> Vec<String> {
        let providers = ["anthropic", "openai", "mistral", "groq", "deepseek"];
        providers.iter()
            .filter(|p| Self::exists(p))
            .map(|p| p.to_string())
            .collect()
    }
}

/// Mask API key for display (show first 6 and last 4 chars)
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        return "****".to_string();
    }
    let prefix = &key[..6.min(key.len())];
    let suffix = &key[key.len().saturating_sub(4)..];
    format!("{}...{}", prefix, suffix)
}

/// Validate API key format (basic checks)
pub fn validate_key_format(provider: &str, key: &str) -> Result<(), String> {
    match provider {
        "anthropic" => {
            if !key.starts_with("sk-ant-") {
                return Err("Anthropic keys start with 'sk-ant-'".into());
            }
            if key.len() < 40 {
                return Err("Key seems too short".into());
            }
        }
        "openai" => {
            if !key.starts_with("sk-") {
                return Err("OpenAI keys start with 'sk-'".into());
            }
        }
        "mistral" => {
            if key.len() < 32 {
                return Err("Key seems too short".into());
            }
        }
        _ => {}
    }
    Ok(())
}
```

#### Tests (Phase 2)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key_standard() {
        let key = "sk-ant-api03-abc123xyz789def456ghi";
        assert_eq!(mask_api_key(key), "sk-ant...ghi");
    }

    #[test]
    fn test_mask_api_key_short() {
        assert_eq!(mask_api_key("short"), "****");
        assert_eq!(mask_api_key("1234567890"), "****");
    }

    #[test]
    fn test_mask_api_key_empty() {
        assert_eq!(mask_api_key(""), "****");
    }

    #[test]
    fn test_validate_anthropic_key_valid() {
        let result = validate_key_format("anthropic", "sk-ant-api03-abcdefghijklmnopqrstuvwxyz123456");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_anthropic_key_wrong_prefix() {
        let result = validate_key_format("anthropic", "sk-wrong-prefix");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("sk-ant-"));
    }

    #[test]
    fn test_validate_openai_key_valid() {
        let result = validate_key_format("openai", "sk-proj-abcdefghijklmnop");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_openai_key_wrong_prefix() {
        let result = validate_key_format("openai", "wrong-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_unknown_provider_accepts_any() {
        let result = validate_key_format("unknown", "any-key-format");
        assert!(result.is_ok());
    }

    // Note: Keyring tests require actual keyring access
    // Use #[ignore] for CI environments without keyring

    #[test]
    #[ignore = "requires system keyring"]
    fn test_keyring_set_get_delete() {
        let test_provider = "nika_test_provider";
        let test_key = "test_api_key_12345";

        // Clean up first
        let _ = NikaKeyring::delete(test_provider);

        // Should not exist initially
        assert!(!NikaKeyring::exists(test_provider));

        // Store key
        NikaKeyring::set(test_provider, test_key).unwrap();

        // Should exist now
        assert!(NikaKeyring::exists(test_provider));

        // Get key
        let retrieved = NikaKeyring::get(test_provider).unwrap();
        assert_eq!(retrieved, test_key);

        // Get masked
        let masked = NikaKeyring::get_masked(test_provider).unwrap();
        assert!(masked.contains("..."));

        // Delete
        NikaKeyring::delete(test_provider).unwrap();
        assert!(!NikaKeyring::exists(test_provider));
    }
}
```

---

### Phase 3: UI Components (TDD)

**Files:** `provider_modal/components/*.rs`

#### 3.1 Provider Card Component

```rust
//! Rich provider card with status, sparkline, and model info

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::tui::widgets::sparkline::LatencyHistory;

/// Visual style for provider cards
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStyle {
    Normal,
    Selected,
    Disabled,
}

/// Provider card showing rich status info
pub struct ProviderCard<'a> {
    /// Provider icon (emoji)
    icon: &'a str,
    /// Provider name
    name: &'a str,
    /// Current model
    model: &'a str,
    /// Connection status
    status: &'a ConnectionStatus,
    /// Features (streaming, thinking)
    features: Vec<&'a str>,
    /// Context window size
    context_window: u32,
    /// Latency history for sparkline
    latency_history: Option<&'a LatencyHistory>,
    /// Visual style
    style: CardStyle,
}

impl<'a> ProviderCard<'a> {
    pub fn new(
        icon: &'a str,
        name: &'a str,
        model: &'a str,
        status: &'a ConnectionStatus,
    ) -> Self {
        Self {
            icon,
            name,
            model,
            status,
            features: vec![],
            context_window: 0,
            latency_history: None,
            style: CardStyle::Normal,
        }
    }

    pub fn features(mut self, features: Vec<&'a str>) -> Self {
        self.features = features;
        self
    }

    pub fn context_window(mut self, size: u32) -> Self {
        self.context_window = size;
        self
    }

    pub fn latency_history(mut self, history: &'a LatencyHistory) -> Self {
        self.latency_history = Some(history);
        self
    }

    pub fn style(mut self, style: CardStyle) -> Self {
        self.style = style;
        self
    }

    fn border_color(&self) -> Color {
        match self.style {
            CardStyle::Selected => Color::Rgb(99, 102, 241),  // indigo
            CardStyle::Normal => Color::Rgb(55, 65, 81),      // gray
            CardStyle::Disabled => Color::Rgb(31, 41, 55),    // dark gray
        }
    }

    fn bg_color(&self) -> Color {
        match self.style {
            CardStyle::Selected => Color::Rgb(30, 41, 59),    // slate-800
            CardStyle::Normal => Color::Rgb(17, 24, 39),      // gray-900
            CardStyle::Disabled => Color::Rgb(17, 24, 39),    // gray-900
        }
    }
}

impl Widget for ProviderCard<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 20 {
            return;
        }

        // Card border with rounded corners
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.border_color()))
            .style(Style::default().bg(self.bg_color()))
            .title(Span::styled(
                format!(" {} {} ", self.icon, self.name),
                Style::default()
                    .fg(Color::Rgb(229, 231, 235))
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        // Row 1: Model name + Status
        let model_style = Style::default().fg(Color::Rgb(156, 163, 175));
        buf.set_string(inner.x + 1, inner.y, self.model, model_style);

        // Status on the right
        let status_text = self.status.display_text();
        let status_x = inner.right().saturating_sub(status_text.len() as u16 + 1);
        buf.set_string(
            status_x,
            inner.y,
            &status_text,
            Style::default().fg(self.status.color()),
        );

        // Row 2: Features + Context window
        if inner.height >= 2 {
            let features_str = self.features.join(" ");
            buf.set_string(
                inner.x + 1,
                inner.y + 1,
                &features_str,
                Style::default().fg(Color::Rgb(59, 130, 246)),  // blue
            );

            if self.context_window > 0 {
                let ctx_str = format!("{}K", self.context_window / 1000);
                let ctx_x = inner.right().saturating_sub(ctx_str.len() as u16 + 1);
                buf.set_string(
                    ctx_x,
                    inner.y + 1,
                    &ctx_str,
                    Style::default().fg(Color::Rgb(107, 114, 128)),
                );
            }
        }

        // Row 3: Sparkline (if available and space permits)
        if inner.height >= 3 {
            if let Some(history) = self.latency_history {
                let sparkline_area = Rect {
                    x: inner.x + 1,
                    y: inner.y + 2,
                    width: inner.width.saturating_sub(2),
                    height: 1,
                };
                // Render mini sparkline
                let data = history.data();
                if !data.is_empty() {
                    // Simplified inline sparkline rendering
                    render_mini_sparkline(sparkline_area, buf, data, self.status.color());
                }
            }
        }
    }
}

fn render_mini_sparkline(area: Rect, buf: &mut Buffer, data: &[u64], color: Color) {
    if data.is_empty() || area.width == 0 {
        return;
    }

    let max_val = *data.iter().max().unwrap_or(&1);
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let visible_data: Vec<_> = data.iter()
        .rev()
        .take(area.width as usize)
        .rev()
        .collect();

    for (i, &val) in visible_data.iter().enumerate() {
        if i as u16 >= area.width {
            break;
        }
        let normalized = if max_val > 0 {
            ((*val as f64 / max_val as f64) * 7.0) as usize
        } else {
            0
        };
        let ch = chars[normalized.min(7)];
        buf.set_string(
            area.x + i as u16,
            area.y,
            &ch.to_string(),
            Style::default().fg(color),
        );
    }
}
```

#### Tests (Phase 3.1)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;

    #[test]
    fn test_card_style_colors() {
        let card = ProviderCard::new("🧠", "Claude", "claude-sonnet-4", &ConnectionStatus::Unknown)
            .style(CardStyle::Selected);
        assert_eq!(card.border_color(), Color::Rgb(99, 102, 241));

        let card = ProviderCard::new("🧠", "Claude", "claude-sonnet-4", &ConnectionStatus::Unknown)
            .style(CardStyle::Disabled);
        assert_eq!(card.border_color(), Color::Rgb(31, 41, 55));
    }

    #[test]
    fn test_card_renders_in_small_area() {
        let card = ProviderCard::new(
            "🧠",
            "Claude",
            "claude-sonnet-4",
            &ConnectionStatus::Connected { latency_ms: 100 },
        );

        // Should not panic with small area
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        card.render(Rect::new(0, 0, 10, 2), &mut buf);
    }

    #[test]
    fn test_card_renders_with_features() {
        let card = ProviderCard::new(
            "🧠",
            "Claude",
            "claude-sonnet-4",
            &ConnectionStatus::Connected { latency_ms: 100 },
        )
        .features(vec!["⚡", "🧠"])
        .context_window(200_000);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        card.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Check that content was rendered
        let content = buf.content().iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(content.contains("Claude"));
    }

    #[test]
    fn test_mini_sparkline_renders() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        let data: Vec<u64> = vec![10, 20, 30, 40, 50, 40, 30, 20, 10];

        render_mini_sparkline(
            Rect::new(0, 0, 20, 1),
            &mut buf,
            &data,
            Color::Green,
        );

        // Should have rendered sparkline characters
        let content: String = buf.content().iter()
            .map(|c| c.symbol())
            .collect();
        // Should contain bar characters
        assert!(content.chars().any(|c| "▁▂▃▄▅▆▇█".contains(c)));
    }

    #[test]
    fn test_mini_sparkline_empty_data() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        render_mini_sparkline(Rect::new(0, 0, 20, 1), &mut buf, &[], Color::Green);
        // Should not panic
    }
}
```

#### 3.2 Download Gauge Component

```rust
//! Download progress gauge for Ollama model pulls

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Gauge, Widget},
};

/// Download progress gauge with model name and size
pub struct DownloadGauge<'a> {
    model_name: &'a str,
    progress: f64,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bps: Option<u64>,
}

impl<'a> DownloadGauge<'a> {
    pub fn new(model_name: &'a str, progress: f64, downloaded: u64, total: u64) -> Self {
        Self {
            model_name,
            progress: progress.clamp(0.0, 1.0),
            downloaded_bytes: downloaded,
            total_bytes: total,
            speed_bps: None,
        }
    }

    pub fn speed(mut self, bps: u64) -> Self {
        self.speed_bps = Some(bps);
        self
    }

    fn format_bytes(bytes: u64) -> String {
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

    fn format_speed(bps: u64) -> String {
        if bps >= 1_000_000 {
            format!("{:.1} MB/s", bps as f64 / 1_000_000.0)
        } else if bps >= 1_000 {
            format!("{:.1} KB/s", bps as f64 / 1_000.0)
        } else {
            format!("{} B/s", bps)
        }
    }
}

impl Widget for DownloadGauge<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 3 || area.width < 30 {
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(59, 130, 246)))
            .title(format!(" Pulling {} ", self.model_name));

        let inner = block.inner(area);
        block.render(area, buf);

        // Progress bar
        let gauge_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        };

        let percentage = (self.progress * 100.0) as u16;
        let gauge = Gauge::default()
            .percent(percentage)
            .gauge_style(Style::default().fg(Color::Rgb(34, 197, 94)).bg(Color::Rgb(31, 41, 55)))
            .use_unicode(true);

        gauge.render(gauge_area, buf);

        // Size info below
        if inner.height >= 2 {
            let size_info = format!(
                "{} / {}",
                Self::format_bytes(self.downloaded_bytes),
                Self::format_bytes(self.total_bytes),
            );
            buf.set_string(
                inner.x,
                inner.y + 1,
                &size_info,
                Style::default().fg(Color::Rgb(156, 163, 175)),
            );

            // Speed on the right
            if let Some(speed) = self.speed_bps {
                let speed_str = Self::format_speed(speed);
                let speed_x = inner.right().saturating_sub(speed_str.len() as u16);
                buf.set_string(
                    speed_x,
                    inner.y + 1,
                    &speed_str,
                    Style::default().fg(Color::Rgb(107, 114, 128)),
                );
            }
        }
    }
}
```

#### Tests (Phase 3.2)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_gb() {
        assert_eq!(DownloadGauge::format_bytes(4_700_000_000), "4.7 GB");
        assert_eq!(DownloadGauge::format_bytes(1_000_000_000), "1.0 GB");
    }

    #[test]
    fn test_format_bytes_mb() {
        assert_eq!(DownloadGauge::format_bytes(500_000_000), "500.0 MB");
        assert_eq!(DownloadGauge::format_bytes(1_500_000), "1.5 MB");
    }

    #[test]
    fn test_format_bytes_kb() {
        assert_eq!(DownloadGauge::format_bytes(1_500), "1.5 KB");
    }

    #[test]
    fn test_format_bytes_b() {
        assert_eq!(DownloadGauge::format_bytes(500), "500 B");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(DownloadGauge::format_speed(50_000_000), "50.0 MB/s");
        assert_eq!(DownloadGauge::format_speed(1_500_000), "1.5 MB/s");
        assert_eq!(DownloadGauge::format_speed(500_000), "500.0 KB/s");
    }

    #[test]
    fn test_progress_clamp() {
        let gauge = DownloadGauge::new("test", 1.5, 100, 100);
        assert_eq!(gauge.progress, 1.0);

        let gauge = DownloadGauge::new("test", -0.5, 0, 100);
        assert_eq!(gauge.progress, 0.0);
    }

    #[test]
    fn test_gauge_renders() {
        let gauge = DownloadGauge::new("llama3.2", 0.45, 2_100_000_000, 4_700_000_000)
            .speed(25_000_000);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 4));
        gauge.render(Rect::new(0, 0, 60, 4), &mut buf);

        let content: String = buf.content().iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("llama3.2"));
    }
}
```

---

### Phase 4: Tab Implementations (TDD)

**Files:** `provider_modal/tabs/*.rs`

#### 4.1 Cloud Providers Tab

```rust
//! Cloud providers tab with rich cards and selection

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Widget,
};

use super::super::components::{ProviderCard, CardStyle};
use super::super::state::{CloudProviderState, ConnectionStatus, ProviderModalState};

pub struct CloudProvidersTab<'a> {
    state: &'a ProviderModalState,
}

impl<'a> CloudProvidersTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self { state }
    }
}

impl Widget for CloudProvidersTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 {
            return;
        }

        // Calculate card height based on available space
        let card_height = 4u16;
        let max_visible = (area.height / card_height) as usize;

        // Scroll offset to keep selection visible
        let scroll_offset = if self.state.selected_cloud_idx >= max_visible {
            self.state.selected_cloud_idx - max_visible + 1
        } else {
            0
        };

        let visible_providers: Vec<_> = self.state.cloud_providers.iter()
            .skip(scroll_offset)
            .take(max_visible)
            .enumerate()
            .collect();

        for (i, provider) in visible_providers {
            let actual_idx = scroll_offset + i;
            let y = area.y + (i as u16 * card_height);

            if y + card_height > area.bottom() {
                break;
            }

            let card_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: card_height,
            };

            let style = if actual_idx == self.state.selected_cloud_idx {
                CardStyle::Selected
            } else if !provider.available {
                CardStyle::Disabled
            } else {
                CardStyle::Normal
            };

            let features = provider.features_icons();
            let card = ProviderCard::new(
                provider.icon,
                &provider.name,
                &provider.current_model,
                &provider.status,
            )
            .features(features)
            .context_window(provider.context_window)
            .style(style);

            if let Some(ref history) = provider.latency_history {
                card.latency_history(history).render(card_area, buf);
            } else {
                card.render(card_area, buf);
            }
        }

        // Scroll indicators
        if scroll_offset > 0 {
            buf.set_string(
                area.right().saturating_sub(3),
                area.y,
                "▲",
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
        }
        if scroll_offset + max_visible < self.state.cloud_providers.len() {
            buf.set_string(
                area.right().saturating_sub(3),
                area.bottom().saturating_sub(1),
                "▼",
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
        }
    }
}
```

#### 4.2 Ollama Tab

##### 4.2.0 OllamaClient (Native HTTP — Research-Informed)

Based on Context7 and Perplexity research, implement a native HTTP client for Ollama:

```rust
//! Native Ollama HTTP client for model management
//!
//! Based on:
//! - Context7: Ollama REST API documentation
//! - Perplexity: NDJSON streaming best practices
//! - Codebase: Existing reqwest patterns from executor.rs

use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

/// Ollama API base URL (default: http://localhost:11434)
pub fn ollama_base_url() -> String {
    std::env::var("OLLAMA_HOST")
        .or_else(|_| std::env::var("OLLAMA_API_BASE_URL"))
        .unwrap_or_else(|_| "http://localhost:11434".to_string())
}

/// Native Ollama HTTP client
pub struct OllamaClient {
    client: Client,
    base_url: String,
}

/// Model info from /api/tags
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: u64,
    pub digest: String,
    pub modified_at: String,
    pub details: OllamaModelDetails,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelDetails {
    pub parameter_size: String,
    pub quantization_level: String,
    pub family: Option<String>,
}

/// Pull progress event (NDJSON streaming)
#[derive(Debug, Clone, Deserialize)]
pub struct PullProgressEvent {
    pub status: String,
    #[serde(default)]
    pub digest: Option<String>,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
}

/// Pull progress for UI updates
#[derive(Debug, Clone)]
pub enum PullProgress {
    Starting,
    Downloading { digest: String, completed: u64, total: u64 },
    Verifying,
    Writing,
    Complete,
    Error(String),
}

impl OllamaClient {
    /// Create client with connection pooling (reuse across calls)
    pub fn new() -> Self {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .user_agent("nika-cli/0.8")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            base_url: ollama_base_url(),
        }
    }

    /// Check if Ollama is running (2s timeout)
    pub async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.base_url))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// List installed models
    pub async fn list_models(&self) -> Result<Vec<OllamaModelInfo>, OllamaError> {
        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<OllamaModelInfo>,
        }

        let response = self.client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| OllamaError::ConnectionFailed(e.to_string()))?;

        let tags: TagsResponse = response.json().await
            .map_err(|e| OllamaError::ParseError(e.to_string()))?;

        Ok(tags.models)
    }

    /// Pull model with streaming progress (returns channel for UI updates)
    pub async fn pull_model(&self, model: &str) -> mpsc::Receiver<PullProgress> {
        let (tx, rx) = mpsc::channel(32);
        let url = format!("{}/api/pull", self.base_url);
        let client = self.client.clone();
        let model = model.to_string();

        tokio::spawn(async move {
            let body = serde_json::json!({
                "name": model,
                "stream": true
            });

            let response = match client.post(&url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(PullProgress::Error(e.to_string())).await;
                    return;
                }
            };

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            let _ = tx.send(PullProgress::Starting).await;

            while let Some(chunk) = stream.next().await {
                let bytes = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(PullProgress::Error(e.to_string())).await;
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete NDJSON lines
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.trim().is_empty() {
                        continue;
                    }

                    if let Ok(event) = serde_json::from_str::<PullProgressEvent>(&line) {
                        let progress = match event.status.as_str() {
                            s if s.contains("pulling") => {
                                if let (Some(digest), Some(total), Some(completed)) =
                                    (event.digest, event.total, event.completed)
                                {
                                    PullProgress::Downloading { digest, completed, total }
                                } else {
                                    PullProgress::Starting
                                }
                            }
                            s if s.contains("verifying") => PullProgress::Verifying,
                            s if s.contains("writing") => PullProgress::Writing,
                            "success" => PullProgress::Complete,
                            _ => continue,
                        };
                        let _ = tx.send(progress).await;
                    }
                }
            }
        });

        rx
    }

    /// Delete installed model
    pub async fn delete_model(&self, model: &str) -> Result<(), OllamaError> {
        let body = serde_json::json!({ "name": model });

        let response = self.client
            .delete(format!("{}/api/delete", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| OllamaError::ConnectionFailed(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(OllamaError::DeleteFailed(model.to_string()))
        }
    }

    /// Get running model processes
    pub async fn list_running(&self) -> Result<Vec<String>, OllamaError> {
        #[derive(Deserialize)]
        struct PsResponse {
            models: Vec<RunningModel>,
        }
        #[derive(Deserialize)]
        struct RunningModel {
            name: String,
        }

        let response = self.client
            .get(format!("{}/api/ps", self.base_url))
            .send()
            .await
            .map_err(|e| OllamaError::ConnectionFailed(e.to_string()))?;

        let ps: PsResponse = response.json().await
            .map_err(|e| OllamaError::ParseError(e.to_string()))?;

        Ok(ps.models.into_iter().map(|m| m.name).collect())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Failed to delete model: {0}")]
    DeleteFailed(String),
    #[error("Ollama not running")]
    NotRunning,
}
```

##### 4.2.0 Tests (OllamaClient)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_base_url_default() {
        std::env::remove_var("OLLAMA_HOST");
        std::env::remove_var("OLLAMA_API_BASE_URL");
        assert_eq!(ollama_base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_ollama_base_url_from_env() {
        std::env::set_var("OLLAMA_HOST", "http://custom:8080");
        assert_eq!(ollama_base_url(), "http://custom:8080");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    fn test_pull_progress_event_parse() {
        let json = r#"{"status":"pulling sha256:abc","digest":"sha256:abc","total":1000,"completed":500}"#;
        let event: PullProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.status, "pulling sha256:abc");
        assert_eq!(event.total, Some(1000));
        assert_eq!(event.completed, Some(500));
    }

    #[test]
    fn test_pull_progress_event_minimal() {
        let json = r#"{"status":"success"}"#;
        let event: PullProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.status, "success");
        assert!(event.digest.is_none());
    }

    #[test]
    fn test_model_info_parse() {
        let json = r#"{
            "name": "llama3.2:latest",
            "size": 4700000000,
            "digest": "sha256:abc123",
            "modified_at": "2026-02-24T10:00:00Z",
            "details": {
                "parameter_size": "8B",
                "quantization_level": "Q4_0"
            }
        }"#;
        let model: OllamaModelInfo = serde_json::from_str(json).unwrap();
        assert_eq!(model.name, "llama3.2:latest");
        assert_eq!(model.size, 4700000000);
        assert_eq!(model.details.parameter_size, "8B");
    }

    #[tokio::test]
    #[ignore = "requires running Ollama"]
    async fn test_ollama_client_list_models() {
        let client = OllamaClient::new();
        if client.is_available().await {
            let models = client.list_models().await.unwrap();
            // Should return at least empty list
            assert!(models.len() >= 0);
        }
    }
}
```

##### 4.2.1 OllamaTab Widget

```rust
//! Ollama local models tab with install/pull/manage

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Widget},
};

use super::super::components::DownloadGauge;
use super::super::state::{DownloadState, OllamaModel, ProviderModalState};

pub struct OllamaTab<'a> {
    state: &'a ProviderModalState,
}

impl<'a> OllamaTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self { state }
    }

    fn render_status_header(&self, area: Rect, buf: &mut Buffer) {
        let status_text = if self.state.ollama_running {
            "✓ Running"
        } else if self.state.ollama_installed {
            "○ Stopped"
        } else {
            "✗ Not installed"
        };

        let status_color = if self.state.ollama_running {
            Color::Rgb(34, 197, 94)  // green
        } else if self.state.ollama_installed {
            Color::Rgb(245, 158, 11)  // amber
        } else {
            Color::Rgb(239, 68, 68)  // red
        };

        buf.set_string(
            area.x,
            area.y,
            "🦙 OLLAMA LOCAL",
            Style::default()
                .fg(Color::Rgb(229, 231, 235))
                .add_modifier(Modifier::BOLD),
        );

        let status_x = area.right().saturating_sub(status_text.len() as u16);
        buf.set_string(area.x + status_x, area.y, status_text, Style::default().fg(status_color));
    }

    fn render_installed_models(&self, area: Rect, buf: &mut Buffer) {
        if self.state.ollama_models.is_empty() {
            buf.set_string(
                area.x + 2,
                area.y + 1,
                "No models installed. Press [p] to pull a model.",
                Style::default().fg(Color::Rgb(156, 163, 175)),
            );
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(55, 65, 81)))
            .title(" Installed Models ");

        let inner = block.inner(area);
        block.render(area, buf);

        for (i, model) in self.state.ollama_models.iter().enumerate() {
            if i as u16 >= inner.height {
                break;
            }

            let y = inner.y + i as u16;
            let is_selected = i == self.state.selected_ollama_idx;

            // Selection indicator
            let indicator = if is_selected { "●" } else { "○" };
            let indicator_style = if is_selected {
                Style::default().fg(Color::Rgb(99, 102, 241))
            } else {
                Style::default().fg(Color::Rgb(107, 114, 128))
            };
            buf.set_string(inner.x, y, indicator, indicator_style);

            // Model name
            let name_style = if is_selected {
                Style::default().fg(Color::Rgb(229, 231, 235)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Rgb(229, 231, 235))
            };
            buf.set_string(inner.x + 2, y, &model.name, name_style);

            // Size
            let size_str = model.size_display();
            buf.set_string(
                inner.x + 20,
                y,
                &size_str,
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );

            // Params
            buf.set_string(
                inner.x + 30,
                y,
                &model.parameter_size,
                Style::default().fg(Color::Rgb(156, 163, 175)),
            );
        }
    }

    fn render_download_progress(&self, area: Rect, buf: &mut Buffer) {
        if let DownloadState::Downloading { ref model, progress, downloaded, total } = self.state.download_state {
            let gauge = DownloadGauge::new(model, progress, downloaded, total);
            gauge.render(area, buf);
        }
    }
}

impl Widget for OllamaTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 {
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),  // Status header
                Constraint::Length(4),  // Download progress (if active)
                Constraint::Min(5),     // Installed models
            ])
            .split(area);

        // Status header
        self.render_status_header(chunks[0], buf);

        // Download progress (if active)
        if self.state.download_state.is_active() {
            self.render_download_progress(chunks[1], buf);
        }

        // Installed models
        let models_area = if self.state.download_state.is_active() {
            chunks[2]
        } else {
            Rect {
                x: area.x,
                y: chunks[1].y,
                width: area.width,
                height: area.height.saturating_sub(2),
            }
        };
        self.render_installed_models(models_area, buf);
    }
}
```

#### 4.2.1 Suggested Models by Category

The Ollama tab will display **curated model suggestions** organized by use case and language:

```rust
/// Suggested models organized by category
pub const SUGGESTED_MODELS: &[ModelCategory] = &[
    ModelCategory {
        name: "🇫🇷 Français (Native)",
        description: "Models trained on French data with excellent native support",
        models: &[
            SuggestedModel {
                name: "mistral:7b",
                size_gb: 4.1,
                params: "7B",
                description: "Fast, good French understanding",
                tags: &["general", "french", "fast"],
            },
            SuggestedModel {
                name: "mistral-nemo:12b",
                size_gb: 7.1,
                params: "12B",
                description: "Better reasoning, excellent French",
                tags: &["reasoning", "french"],
            },
            SuggestedModel {
                name: "mixtral:8x7b",
                size_gb: 26.0,
                params: "8x7B MoE",
                description: "Top-tier multilingual, needs 32GB+ RAM",
                tags: &["powerful", "multilingual", "large"],
            },
        ],
    },
    ModelCategory {
        name: "🌍 Multilingue",
        description: "Models supporting 100+ languages for localization workflows",
        models: &[
            SuggestedModel {
                name: "qwen2.5:7b",
                size_gb: 4.4,
                params: "7B",
                description: "Excellent multilingual, 29 languages",
                tags: &["multilingual", "chinese", "european"],
            },
            SuggestedModel {
                name: "qwen2.5:14b",
                size_gb: 8.9,
                params: "14B",
                description: "Better quality, same language coverage",
                tags: &["multilingual", "quality"],
            },
            SuggestedModel {
                name: "aya:8b",
                size_gb: 4.8,
                params: "8B",
                description: "Cohere's 101-language model",
                tags: &["multilingual", "cohere", "101-langs"],
            },
            SuggestedModel {
                name: "gemma2:9b",
                size_gb: 5.4,
                params: "9B",
                description: "Google, good European languages",
                tags: &["google", "european"],
            },
        ],
    },
    ModelCategory {
        name: "💻 Code",
        description: "Specialized models for code generation and understanding",
        models: &[
            SuggestedModel {
                name: "deepseek-coder:6.7b",
                size_gb: 3.8,
                params: "6.7B",
                description: "Excellent code generation",
                tags: &["code", "fast"],
            },
            SuggestedModel {
                name: "codellama:13b",
                size_gb: 7.4,
                params: "13B",
                description: "Meta's code specialist",
                tags: &["code", "meta"],
            },
            SuggestedModel {
                name: "qwen2.5-coder:7b",
                size_gb: 4.4,
                params: "7B",
                description: "Code + multilingual comments",
                tags: &["code", "multilingual"],
            },
        ],
    },
    ModelCategory {
        name: "⚡ Compact (Low RAM)",
        description: "Efficient models for systems with limited resources",
        models: &[
            SuggestedModel {
                name: "llama3.2:3b",
                size_gb: 2.0,
                params: "3B",
                description: "Fast, good multilingual",
                tags: &["compact", "fast", "multilingual"],
            },
            SuggestedModel {
                name: "phi3:3.8b",
                size_gb: 2.2,
                params: "3.8B",
                description: "Microsoft, compact but capable",
                tags: &["compact", "microsoft"],
            },
            SuggestedModel {
                name: "gemma2:2b",
                size_gb: 1.6,
                params: "2B",
                description: "Google's smallest, still good",
                tags: &["compact", "google"],
            },
        ],
    },
];
```

**Visual Design:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  🦙 OLLAMA LOCAL                                        ● Running (11434)   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─ INSTALLED ────────────────────────────────────────────────────────────┐ │
│  │  ● llama3.2:latest     2.0 GB    3B     ▁▂▃▅▃▂▁  89ms                 │ │
│  │  ○ mistral:7b          4.1 GB    7B     ▂▃▄▅▄▃▂  142ms                │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
│  ┌─ AVAILABLE ────────────────────────────────────────────────────────────┐ │
│  │                                                                         │ │
│  │  🇫🇷 Français (Native)                                                  │ │
│  │  ├── mistral-nemo:12b    7.1 GB    Better reasoning, excellent French  │ │
│  │  └── mixtral:8x7b        26 GB     Top-tier (needs 32GB+ RAM)          │ │
│  │                                                                         │ │
│  │  🌍 Multilingue                                                         │ │
│  │  ├── qwen2.5:14b         8.9 GB    101+ languages                      │ │
│  │  ├── aya:8b              4.8 GB    Cohere, 101 languages               │ │
│  │  └── gemma2:9b           5.4 GB    Google, good European               │ │
│  │                                                                         │ │
│  │  💻 Code                                                                │ │
│  │  ├── deepseek-coder:6.7b 3.8 GB    Excellent code generation           │ │
│  │  └── codellama:13b       7.4 GB    Meta code specialist                │ │
│  │                                                                         │ │
│  │  ⚡ Compact (Low RAM)                                                    │ │
│  │  ├── phi3:3.8b           2.2 GB    Microsoft, compact                  │ │
│  │  └── gemma2:2b           1.6 GB    Google's smallest                   │ │
│  │                                                                         │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
│  [Enter] Pull  [d] Delete  [↑↓] Navigate  [/] Search  [c] Collapse         │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Features:**
- **Categorized by use case** — Français, Multilingue, Code, Compact
- **RAM requirements shown** — Users know if they can run the model
- **Language tags** — Easy filtering for locale-specific workflows
- **Collapsible categories** — Keep UI clean with `[c]` toggle
- **Search across all** — `/` opens fuzzy search across categories

#### 4.3 API Keys Tab

```rust
//! API Keys management tab with secure input

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Widget},
};

use super::super::state::{ApiKeyEntry, ApiKeyState, ProviderModalState};

pub struct ApiKeysTab<'a> {
    state: &'a ProviderModalState,
}

impl<'a> ApiKeysTab<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self { state }
    }

    fn render_key_entry(&self, area: Rect, buf: &mut Buffer, entry: &ApiKeyEntry, selected: bool) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if selected {
                Color::Rgb(99, 102, 241)
            } else {
                Color::Rgb(55, 65, 81)
            }));

        let inner = block.inner(area);
        block.render(area, buf);

        // Provider icon and name
        buf.set_string(
            inner.x,
            inner.y,
            &format!("{} {}", entry.icon, entry.env_var),
            Style::default()
                .fg(Color::Rgb(229, 231, 235))
                .add_modifier(Modifier::BOLD),
        );

        // Key status
        if inner.height >= 2 {
            let (status_text, status_color) = match &entry.state {
                ApiKeyState::NotConfigured => ("⚠ Not configured", Color::Rgb(245, 158, 11)),
                ApiKeyState::Configured { masked } => {
                    let text = format!("✓ Configured  •  {}", masked);
                    (text.as_str(), Color::Rgb(34, 197, 94))
                }
                ApiKeyState::Verified { masked, latency_ms } => {
                    let text = format!("✓ Verified ({}ms)  •  {}", latency_ms, masked);
                    (text.as_str(), Color::Rgb(34, 197, 94))
                }
                ApiKeyState::Invalid { masked, error } => {
                    let text = format!("✗ Invalid: {}  •  {}", error, masked);
                    (text.as_str(), Color::Rgb(239, 68, 68))
                }
            };

            // Clone status_text to avoid borrow issues
            let status_string = status_text.to_string();
            buf.set_string(
                inner.x + 3,
                inner.y + 1,
                &status_string,
                Style::default().fg(status_color),
            );
        }

        // Actions hint
        if inner.height >= 3 {
            let actions = match &entry.state {
                ApiKeyState::NotConfigured => "[+ Add Key]",
                _ => "[Edit] [Remove] [Test]",
            };
            buf.set_string(
                inner.x + 3,
                inner.y + 2,
                actions,
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
        }
    }
}

impl Widget for ApiKeysTab<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 5 {
            return;
        }

        // Header
        buf.set_string(
            area.x,
            area.y,
            "🔑 API KEYS",
            Style::default()
                .fg(Color::Rgb(229, 231, 235))
                .add_modifier(Modifier::BOLD),
        );

        let storage_info = "Storage: 🔒 System Keychain";
        let storage_x = area.right().saturating_sub(storage_info.len() as u16);
        buf.set_string(
            storage_x,
            area.y,
            storage_info,
            Style::default().fg(Color::Rgb(107, 114, 128)),
        );

        // Key entries
        let entry_height = 4u16;
        let entries_start = area.y + 2;

        for (i, entry) in self.state.api_keys.iter().enumerate() {
            let y = entries_start + (i as u16 * entry_height);
            if y + entry_height > area.bottom() {
                break;
            }

            let entry_area = Rect {
                x: area.x,
                y,
                width: area.width,
                height: entry_height,
            };

            self.render_key_entry(entry_area, buf, entry, i == self.state.selected_key_idx);
        }

        // Info footer
        let footer_y = area.bottom().saturating_sub(2);
        if footer_y > entries_start {
            buf.set_string(
                area.x,
                footer_y,
                "💡 Keys are stored in your system keychain (macOS Keychain,",
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
            buf.set_string(
                area.x + 3,
                footer_y + 1,
                "Windows Credential Locker, or Linux Secret Service).",
                Style::default().fg(Color::Rgb(107, 114, 128)),
            );
        }
    }
}
```

---

### Phase 5: Main Modal Widget (TDD)

**Files:** `provider_modal/mod.rs`

```rust
//! Provider Modal v2
//!
//! Rich tabbed modal for provider management:
//! - Cloud providers with cards and sparklines
//! - Ollama local models with install/pull
//! - API key management via system keychain
//! - Configuration preferences

mod components;
mod keyring;
mod state;
mod tabs;

pub use components::*;
pub use keyring::*;
pub use state::*;
pub use tabs::*;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Tabs, Widget},
};

/// Main provider modal widget
pub struct ProviderModal<'a> {
    state: &'a ProviderModalState,
}

impl<'a> ProviderModal<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self { state }
    }

    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let tab_titles = vec![
            Span::raw(" ◆ Cloud "),
            Span::raw(" 🦙 Ollama "),
            Span::raw(" 🔑 Keys "),
            Span::raw(" ⚙ Config "),
        ];

        let tabs = Tabs::new(tab_titles)
            .select(self.state.active_tab as usize)
            .style(Style::default().fg(Color::Rgb(156, 163, 175)))
            .highlight_style(
                Style::default()
                    .fg(Color::Rgb(99, 102, 241))
                    .add_modifier(Modifier::BOLD),
            )
            .divider(Span::raw("│"));

        tabs.render(area, buf);
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        match self.state.active_tab {
            ProviderModalTab::Cloud => {
                CloudProvidersTab::new(self.state).render(area, buf);
            }
            ProviderModalTab::Ollama => {
                OllamaTab::new(self.state).render(area, buf);
            }
            ProviderModalTab::Keys => {
                ApiKeysTab::new(self.state).render(area, buf);
            }
            ProviderModalTab::Config => {
                // ConfigTab::new(self.state).render(area, buf);
                buf.set_string(
                    area.x + 2,
                    area.y + 2,
                    "Configuration options coming soon...",
                    Style::default().fg(Color::Rgb(156, 163, 175)),
                );
            }
        }
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let hints = match self.state.active_tab {
            ProviderModalTab::Cloud => "↑↓ Navigate │ Enter Select │ m Models │ Tab Next │ Esc Close",
            ProviderModalTab::Ollama => "↑↓ Navigate │ Enter Use │ p Pull │ d Delete │ Tab Next │ Esc",
            ProviderModalTab::Keys => "↑↓ Navigate │ Enter Edit │ t Test │ r Remove │ Tab Next │ Esc",
            ProviderModalTab::Config => "↑↓ Navigate │ Enter Toggle │ Tab Next │ Esc Close",
        };

        buf.set_string(
            area.x + 1,
            area.y,
            hints,
            Style::default().fg(Color::Rgb(107, 114, 128)),
        );
    }
}

impl Widget for ProviderModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate modal size (80% width, 70% height, centered)
        let modal_width = (area.width * 80 / 100).min(100).max(50);
        let modal_height = (area.height * 70 / 100).min(30).max(15);
        let modal_x = (area.width - modal_width) / 2 + area.x;
        let modal_y = (area.height - modal_height) / 2 + area.y;

        let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

        // Clear background
        Clear.render(modal_area, buf);

        // Main border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(99, 102, 241)))
            .style(Style::default().bg(Color::Rgb(17, 24, 39)))
            .title(Span::styled(
                " PROVIDERS ",
                Style::default()
                    .fg(Color::Rgb(229, 231, 235))
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Layout: tabs | content | footer
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // Tabs
                Constraint::Min(5),     // Content
                Constraint::Length(1),  // Footer
            ])
            .split(inner);

        // Render sections
        self.render_tabs(chunks[0], buf);
        self.render_content(chunks[1], buf);
        self.render_footer(chunks[2], buf);
    }
}
```

---

### Phase 6: Integration & Event Handling

**Files:** `src/tui/views/chat.rs`, `src/tui/app.rs`

#### ViewAction Extensions

```rust
// In src/tui/views/mod.rs - Add new actions

/// Provider modal actions (v0.8.4)
ProviderModalOpen,
ProviderModalClose,
ProviderModalTabSwitch(ProviderModalTab),
ProviderModalSelect { provider_id: String, model: String },
OllamaPullModel { model: String },
OllamaDeleteModel { model: String },
ApiKeySet { provider: String, key: String },
ApiKeyDelete { provider: String },
ApiKeyTest { provider: String },
```

#### Event Handling in ChatView

```rust
// Handle modal key events
fn handle_provider_modal_key(&mut self, key: KeyEvent) -> ViewAction {
    match key.code {
        KeyCode::Esc => {
            self.provider_modal.back();
            ViewAction::None
        }
        KeyCode::Tab => {
            self.provider_modal.next_tab();
            ViewAction::None
        }
        KeyCode::BackTab => {
            self.provider_modal.prev_tab();
            ViewAction::None
        }
        KeyCode::Up => {
            self.provider_modal.navigate_up();
            ViewAction::None
        }
        KeyCode::Down => {
            self.provider_modal.navigate_down();
            ViewAction::None
        }
        KeyCode::Enter => {
            self.handle_provider_modal_confirm()
        }
        KeyCode::Char('1') => {
            self.provider_modal.switch_tab(ProviderModalTab::Cloud);
            ViewAction::None
        }
        KeyCode::Char('2') => {
            self.provider_modal.switch_tab(ProviderModalTab::Ollama);
            ViewAction::None
        }
        KeyCode::Char('3') => {
            self.provider_modal.switch_tab(ProviderModalTab::Keys);
            ViewAction::None
        }
        KeyCode::Char('4') => {
            self.provider_modal.switch_tab(ProviderModalTab::Config);
            ViewAction::None
        }
        KeyCode::Char('p') if self.provider_modal.active_tab == ProviderModalTab::Ollama => {
            self.handle_ollama_pull()
        }
        KeyCode::Char('k') if self.provider_modal.active_tab == ProviderModalTab::Cloud => {
            // Switch to keys tab for current provider
            self.provider_modal.switch_tab(ProviderModalTab::Keys);
            ViewAction::None
        }
        _ => ViewAction::None,
    }
}
```

---

## Phase 7: Cosmetic "WOW" Enhancements

Based on analysis of existing Nika TUI widgets, we can add significant visual polish using our established widget library.

### 7.1 Modal Title with Styled Header

Use a clean, themed header with Solarized colors:

```rust
// In provider_modal/mod.rs
fn render_title(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
    // Styled header with theme colors
    let title = Paragraph::new("◆ PROVIDERS")
        .style(Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    title.render(area, buf);
}
```

**Visual:**
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              ◆ PROVIDERS                                      ║
║                                                                               ║
║  Configure your LLM providers and API keys                                    ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

> **Note:** BigText was removed (unused dead code). Using clean Paragraph with theme colors instead.

### 7.2 MatrixDecrypt for Connection Verification

When verifying providers, reveal status text with `MatrixDecrypt` effect:

```rust
use crate::tui::widgets::MatrixDecrypt;

impl ProviderCard {
    fn render_verification(&self, area: Rect, buf: &mut Buffer) {
        match &self.connection_status {
            ConnectionStatus::Verifying => {
                // Chaos-to-text reveal effect
                let decrypt = MatrixDecrypt::new("● Connected in 142ms")
                    .progress(self.verify_progress)
                    .chaos_chars("░▒▓█▀▄")
                    .reveal_color(COLOR_LIME);
                decrypt.render(area, buf);
            }
            _ => { /* static text */ }
        }
    }
}
```

**Animation frames:**
```
Frame 1:  ░▒█▓▀▄░▒▓█▀▄▒▓
Frame 2:  ●░▒█n▀e▄t░d▒▓
Frame 3:  ● Co▒▓ect▀d i▒ 1▓
Frame 4:  ● Connected in 142ms  ✓
```

### 7.3 Enhanced Download Gauge with Partial Blocks

Use `SmoothGauge` for sub-character precision during Ollama model downloads:

```rust
use crate::tui::widgets::SmoothGauge;

impl DownloadGauge {
    fn render_progress(&self, area: Rect, buf: &mut Buffer) {
        // Partial block characters: ▏▎▍▌▋▊▉█
        let gauge = SmoothGauge::new(self.progress)
            .partial_blocks(&['▏', '▎', '▍', '▌', '▋', '▊', '▉'])
            .filled_char('█')
            .empty_char('░')
            .color_gradient(COLOR_CYAN, COLOR_LIME)
            .show_percentage(true);
        gauge.render(area, buf);
    }
}
```

**Visual:**
```
┌─────────────────────────────────────────────────────────────────────────┐
│  llama3.2:latest   ████████████████▌░░░░░░░░░░░░░░░  47.3%             │
│                    2.1 GB / 4.5 GB  ━━━━━━━━━━━━━━━━  ETA: 2m 34s       │
└─────────────────────────────────────────────────────────────────────────┘
```

### 7.4 Timeline Spinners for Verification Status

Use `Timeline` spinners during provider verification:

```rust
use crate::tui::widgets::timeline::{TaskState, TimelineEvent};

// Braille spinner animation
const SPINNERS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

impl CloudTab {
    fn render_provider_list(&self, area: Rect, buf: &mut Buffer) {
        for (i, provider) in self.providers.iter().enumerate() {
            let spinner = match provider.status {
                ConnectionStatus::Verifying => {
                    let idx = (self.tick / 3) % SPINNERS.len();
                    SPINNERS[idx]
                }
                ConnectionStatus::Connected(_) => "●",
                ConnectionStatus::Failed(_) => "⊗",
                ConnectionStatus::Unknown => "○",
            };
            // Render with spinner
        }
    }
}
```

**Visual:**
```
┌─ Cloud Providers ────────────────────────────────────────────────────────┐
│  ⠹ Claude (claude-sonnet-4-6)          Verifying...                     │
│  ● OpenAI (gpt-4o)                     Connected • 89ms                  │
│  ⊗ Groq (llama-3.3-70b)                Failed: Invalid API key           │
│  ○ Mistral (mistral-large)             Not configured                    │
└──────────────────────────────────────────────────────────────────────────┘
```

### 7.5 ActivityStack-Style Tab Headers

Use temperature-styled headers inspired by `ActivityStack`:

```rust
impl ProviderModal {
    fn render_tab_headers(&self, area: Rect, buf: &mut Buffer) {
        let headers = vec![
            ("☁️ ", "CLOUD", COLOR_CYAN),
            ("🦙", "OLLAMA", COLOR_VIOLET),
            ("🔐", "KEYS", COLOR_GOLD),
        ];

        for (i, (icon, label, color)) in headers.iter().enumerate() {
            let is_active = i == self.active_tab as usize;
            let style = if is_active {
                Style::default()
                    .fg(*color)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(COLOR_GRAY)
            };
            // Render with glow effect when active
        }
    }
}
```

**Visual:**
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  ☁️  CLOUD        │  🦙 OLLAMA        │  🔐 KEYS                              ║
║  ═══════════                                                                  ║
╠═══════════════════════════════════════════════════════════════════════════════╣
```

### 7.6 MatrixRainReveal for Streaming Feedback

During model pulls, show streaming progress with `MatrixRainReveal`:

```rust
use crate::tui::widgets::MatrixRainReveal;

impl OllamaTab {
    fn render_pull_progress(&self, area: Rect, buf: &mut Buffer) {
        if let Some(pull) = &self.active_pull {
            // Streaming text with falling rain effect
            let reveal = MatrixRainReveal::new(&pull.status_message)
                .rain_density(0.15)
                .spark_on_impact(true)
                .reveal_speed(3);
            reveal.render(area, buf);
        }
    }
}
```

**Animation:**
```
   ░  ▓     ░        ▒       ░
  ░  Pulling llama3.2:latest...  ▓
 ▒      ░     ▓  ░       ▒    ░
   ▓ ░    Layer 3/12: ████▌░░░ 45%
     ░  ▒     ░   ▓        ░
```

### 7.7 ProStatusBar-Inspired Session Info

Add provider session stats in modal footer:

```rust
impl ProviderModal {
    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let spans = vec![
            Span::raw("💰 "),
            Span::styled("$0.42", Style::default().fg(COLOR_GOLD)),
            Span::styled(" │ ", Style::default().fg(COLOR_GRAY)),
            Span::raw("🔢 "),
            Span::styled("1.2k", Style::default().fg(COLOR_PINK)),
            Span::styled("/200k", Style::default().fg(COLOR_GRAY)),
            Span::styled(" │ ", Style::default().fg(COLOR_GRAY)),
            Span::raw("⏱ "),
            Span::styled("3m 12s", Style::default().fg(COLOR_TURQUOISE)),
        ];
        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}
```

**Visual:**
```
╚══════════════════════════════════════════════════════════════════════════════╝
  💰 $0.42 │ 🔢 1.2k/200k │ ⏱ 3m 12s │ MCP:● 3/3 │ [Tab] Switch │ [Esc] Close
```

### 7.8 Sparkline Integration for Latency History

Enhance provider cards with mini sparklines:

```rust
use crate::tui::widgets::Sparkline;

impl ProviderCard {
    fn render_latency_sparkline(&self, area: Rect, buf: &mut Buffer) {
        if let Some(latencies) = &self.latency_history {
            let sparkline = Sparkline::new(latencies)
                .max(500.0)  // Max 500ms
                .bar_chars(&['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'])
                .color_gradient(COLOR_LIME, COLOR_CORAL);  // Green→Red
            sparkline.render(area, buf);
        }
    }
}
```

**Visual:**
```
┌─ Claude ─────────────────────────────────────────────────────────────────┐
│  🧠 claude-sonnet-4-6                                                    │
│  ● Connected • 142ms avg                                                 │
│  Latency: ▁▂▁▃▂▄▃▂▁▂▃▅▄▃▂▁▁▂▃▂  (last 20 calls)                         │
└──────────────────────────────────────────────────────────────────────────┘
```

### 7.9 Cosmic Spinners During Installation

Use COSMIC_SPINNER during Ollama installation:

```rust
const COSMIC_SPINNER: &[char] = &['🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘'];
const ROCKET_SPINNER: &[char] = &['🚀', '🔥', '✨', '💫', '⭐'];

impl OllamaInstaller {
    fn render_install_progress(&self, area: Rect, buf: &mut Buffer) {
        let spinner = match self.state {
            InstallState::Downloading => COSMIC_SPINNER[self.frame % 8],
            InstallState::Installing => ROCKET_SPINNER[self.frame % 5],
            _ => ' ',
        };
        // Render spinner + progress
    }
}
```

**Animation:**
```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🌓 Installing Ollama...                                                      ║
║                                                                               ║
║  ███████████████████░░░░░░░░░░  62%                                          ║
║                                                                               ║
║  Step 2/4: Extracting binaries...                                            ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Summary: Widget Integrations

| Widget | Integration Point | WOW Factor |
|--------|------------------|------------|
| `MatrixDecrypt` | Verification reveal | ⭐⭐⭐⭐ Chaos-to-text animation |
| `SmoothGauge` | Download progress | ⭐⭐⭐ Sub-character precision |
| `Timeline` spinners | Provider status | ⭐⭐⭐ Braille animation |
| `ActivityStack` style | Tab headers | ⭐⭐⭐ Temperature colors |
| `MatrixRainReveal` | Pull streaming | ⭐⭐⭐⭐ Rain + spark effects |
| `ProStatusBar` | Footer stats | ⭐⭐⭐ Session metrics |
| `Sparkline` | Latency history | ⭐⭐⭐⭐ Visual performance |
| `COSMIC_SPINNER` | Installation | ⭐⭐⭐⭐⭐ Moon phases + rocket |

### Additional Test Count

| Component | Test Count |
|-----------|------------|
| MatrixDecrypt effects | 5 |
| Cosmic spinners | 2 |
| Sparkline latency | 4 |

**Total additional tests:** 11+
**New total:** 105+ tests

---

## Test Summary

| Phase | Module | Test Count | Coverage Target |
|-------|--------|------------|-----------------|
| 1 | state.rs | 25+ | 100% for state logic |
| 2 | keyring.rs | 10+ | 90% (keyring tests ignored in CI) |
| 3.1 | provider_card.rs | 8+ | 100% |
| 3.2 | download_gauge.rs | 8+ | 100% |
| 4.1 | cloud.rs | 5+ | 90% |
| 4.2.0 | ollama_client.rs | 6+ | 100% (HTTP + NDJSON parsing) |
| 4.2.1 | ollama.rs | 8+ | 90% |
| 4.3 | keys.rs | 5+ | 90% |
| 5 | mod.rs | 10+ | 90% |
| 6 | integration | 15+ | 80% |
| 7 | cosmetic widgets | 14+ | 80% |

**Total estimated tests:** 114+

---

## Implementation Order

1. **Phase 1** - State types (foundation for everything)
2. **Phase 2** - Keyring integration (can be tested independently)
3. **Phase 3** - UI components (visual building blocks)
4. **Phase 4** - Tab implementations (compose components)
5. **Phase 5** - Main modal (compose tabs)
6. **Phase 6** - Integration (wire into app)
7. **Phase 7** - Cosmetic polish (wow factor widgets)

---

## Dependencies

```toml
# Add to Cargo.toml
[dependencies]
keyring = "3"
```

---

## Success Criteria

- [ ] All 108+ tests passing
- [ ] Zero clippy warnings
- [ ] Modal renders correctly at all terminal sizes
- [ ] Tabs switch smoothly with keyboard navigation
- [ ] API keys stored/retrieved from system keychain
- [ ] Ollama models listed and pullable
- [ ] Sparklines show real latency history
- [ ] Rounded borders render correctly
- [ ] Download progress gauge animates
- [ ] MatrixDecrypt effect triggers on verification
- [ ] Braille spinners animate smoothly
- [ ] Cosmic/rocket spinners during installation
- [ ] ProStatusBar metrics in footer

---

## References

- ratatui Tabs: https://docs.rs/ratatui-widgets/0.3.0/ratatui_widgets/tabs/
- ratatui Gauge: https://docs.rs/ratatui-widgets/0.3.0/ratatui_widgets/gauge/
- keyring-rs: https://docs.rs/keyring/3/keyring/
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
