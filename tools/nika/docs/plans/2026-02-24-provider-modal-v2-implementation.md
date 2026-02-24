# Provider Modal v2 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a rich tabbed modal for LLM provider management with secure keyring storage and native Ollama HTTP client.

**Architecture:** New `provider_modal/` module with 4 tabs (Cloud, Ollama, Keys, Config), native `OllamaClient` for HTTP streaming, `keyring-rs` for secure credential storage. Follows existing Nika TUI patterns with ratatui widgets.

**Tech Stack:** Rust, ratatui, reqwest (streaming), keyring-rs, tokio, serde_json (NDJSON parsing)

---

## Visual Reference

```
╔══════════════════════════════════════════════════════════════════════════════╗
║                              ◆ PROVIDERS                                     ║
╠══════════════════════════════════════════════════════════════════════════════╣
║   ☁️  CLOUD        │  🦙 OLLAMA        │  🔐 KEYS         │  ⚙️  CONFIG       ║
║   ═══════════                                                                ║
║  ╭────────────────────────────────────────────────────────────────────────╮  ║
║  │  🧠 CLAUDE                                      ● Connected 142ms      │  ║
║  │  claude-sonnet-4-6                                                     │  ║
║  │  ⚡ streaming  🧠 thinking                              200K context   │  ║
║  │  ▁▂▁▃▂▄▃▂▁▂▃▅▄▃▂▁▁▂▃▂                                                 │  ║
║  ╰────────────────────────────────────────────────────────────────────────╯  ║
╚══════════════════════════════════════════════════════════════════════════════╝
```

---

## Phase 1: Dependencies & Module Setup

### Task 1.1: Add keyring dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add keyring to dependencies**

Add after line with `reqwest`:

```toml
keyring = "3"
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "deps: add keyring-rs for secure credential storage"
```

---

### Task 1.2: Create provider_modal module structure

**Files:**
- Create: `src/tui/widgets/provider_modal/mod.rs`
- Create: `src/tui/widgets/provider_modal/state.rs`
- Create: `src/tui/widgets/provider_modal/keyring.rs`
- Create: `src/tui/widgets/provider_modal/ollama_client.rs`
- Create: `src/tui/widgets/provider_modal/components/mod.rs`
- Create: `src/tui/widgets/provider_modal/tabs/mod.rs`
- Modify: `src/tui/widgets/mod.rs`

**Step 1: Create module directory and mod.rs**

```rust
// src/tui/widgets/provider_modal/mod.rs
//! Provider Modal v2
//!
//! Rich tabbed modal for provider management:
//! - Cloud providers with cards and sparklines
//! - Ollama local models with install/pull
//! - API key management via system keychain
//! - Configuration preferences

mod components;
mod keyring;
mod ollama_client;
mod state;
mod tabs;

pub use components::*;
pub use keyring::*;
pub use ollama_client::*;
pub use state::*;
pub use tabs::*;
```

**Step 2: Create empty submodules**

```rust
// src/tui/widgets/provider_modal/state.rs
//! Provider modal state types

// TODO: Implement in Task 2

// src/tui/widgets/provider_modal/keyring.rs
//! Secure API key storage via system keychain

// TODO: Implement in Task 3

// src/tui/widgets/provider_modal/ollama_client.rs
//! Native Ollama HTTP client

// TODO: Implement in Task 4

// src/tui/widgets/provider_modal/components/mod.rs
//! UI components for provider modal

// TODO: Implement in Task 5

// src/tui/widgets/provider_modal/tabs/mod.rs
//! Tab implementations

// TODO: Implement in Task 6
```

**Step 3: Export from widgets/mod.rs**

Add to `src/tui/widgets/mod.rs`:

```rust
pub mod provider_modal;
pub use provider_modal::*;
```

**Step 4: Verify compilation**

Run: `cargo check`
Expected: Compiles (empty modules are valid)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/
git add src/tui/widgets/mod.rs
git commit -m "feat(tui): scaffold provider_modal module structure"
```

---

## Phase 2: Core State & Types (TDD)

### Task 2.1: ProviderModalTab enum

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/provider_modal/state.rs
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
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::state::tests -- --nocapture`
Expected: FAIL with "cannot find type `ProviderModalTab`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/provider_modal/state.rs
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

#[cfg(test)]
mod tests {
    // ... tests from Step 1
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::state::tests -- --nocapture`
Expected: PASS (5 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/state.rs
git commit -m "feat(tui): add ProviderModalTab enum with navigation"
```

---

### Task 2.2: ConnectionStatus enum

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`

**Step 1: Write the failing test**

Add to tests module:

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_connection_status -- --nocapture`
Expected: FAIL with "cannot find type `ConnectionStatus`"

**Step 3: Write minimal implementation**

Add to `state.rs`:

```rust
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_connection_status -- --nocapture`
Expected: PASS (2 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/state.rs
git commit -m "feat(tui): add ConnectionStatus enum with display"
```

---

### Task 2.3: ApiKeyState enum

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`

**Step 1: Write the failing test**

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_api_key -- --nocapture`
Expected: FAIL with "cannot find type `ApiKeyState`"

**Step 3: Write minimal implementation**

```rust
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_api_key -- --nocapture`
Expected: PASS (3 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/state.rs
git commit -m "feat(tui): add ApiKeyState enum with masking"
```

---

### Task 2.4: DownloadState enum

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`

**Step 1: Write the failing test**

```rust
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
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_download -- --nocapture`
Expected: FAIL with "cannot find type `DownloadState`"

**Step 3: Write minimal implementation**

```rust
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
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_download -- --nocapture`
Expected: PASS (3 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/state.rs
git commit -m "feat(tui): add DownloadState enum with progress tracking"
```

---

### Task 2.5: ProviderModalState struct

**Files:**
- Modify: `src/tui/widgets/provider_modal/state.rs`

**Step 1: Write the failing test**

```rust
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
fn test_modal_navigate() {
    let mut state = ProviderModalState::default();
    state.item_count = 5;

    assert_eq!(state.selected_idx, 0);

    state.navigate_down();
    assert_eq!(state.selected_idx, 1);

    state.navigate_down();
    state.navigate_down();
    state.navigate_down();
    assert_eq!(state.selected_idx, 4);

    // Should not go past end
    state.navigate_down();
    assert_eq!(state.selected_idx, 4);

    state.navigate_up();
    assert_eq!(state.selected_idx, 3);

    // Go to top
    state.navigate_up();
    state.navigate_up();
    state.navigate_up();
    assert_eq!(state.selected_idx, 0);

    // Should not go below 0
    state.navigate_up();
    assert_eq!(state.selected_idx, 0);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_modal -- --nocapture`
Expected: FAIL with "cannot find type `ProviderModalState`"

**Step 3: Write minimal implementation**

```rust
/// Main provider modal state
#[derive(Debug, Clone, Default)]
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
    /// Key input buffer
    pub key_input_buffer: String,
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
    }

    /// Navigate up in current list
    pub fn navigate_up(&mut self) {
        self.selected_idx = self.selected_idx.saturating_sub(1);
    }

    /// Navigate down in current list
    pub fn navigate_down(&mut self) {
        if self.item_count > 0 && self.selected_idx < self.item_count - 1 {
            self.selected_idx += 1;
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
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::state::tests::test_modal -- --nocapture`
Expected: PASS (5 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/state.rs
git commit -m "feat(tui): add ProviderModalState with navigation"
```

---

## Phase 3: Keyring Integration (TDD)

### Task 3.1: NikaKeyring basic operations

**Files:**
- Modify: `src/tui/widgets/provider_modal/keyring.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/provider_modal/keyring.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key_standard() {
        let key = "sk-ant-api03-abc123xyz789def456ghi";
        assert_eq!(mask_api_key(key), "sk-ant...i");
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

    #[test]
    fn test_provider_env_var() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("mistral"), "MISTRAL_API_KEY");
        assert_eq!(provider_env_var("groq"), "GROQ_API_KEY");
        assert_eq!(provider_env_var("deepseek"), "DEEPSEEK_API_KEY");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::keyring::tests -- --nocapture`
Expected: FAIL with "cannot find function `mask_api_key`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/provider_modal/keyring.rs
//! Secure API key storage via system keychain
//!
//! Uses keyring-rs for cross-platform credential storage:
//! - macOS: Keychain Access
//! - Windows: Credential Locker
//! - Linux: Secret Service (GNOME Keyring, KWallet)

use keyring::Entry;
use thiserror::Error;

/// Service name for keyring entries
const SERVICE_NAME: &str = "nika";

/// Keyring error types
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

        entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => KeyringError::NotFound(provider.to_string()),
            _ => KeyringError::AccessError(e.to_string()),
        })
    }

    /// Store API key for a provider
    pub fn set(provider: &str, key: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .set_password(key)
            .map_err(|e| KeyringError::StoreError(e.to_string()))
    }

    /// Delete API key for a provider
    pub fn delete(provider: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .delete_credential()
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
}

/// Mask API key for display (show first 6 and last 1 char)
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        return "****".to_string();
    }
    let prefix = &key[..6.min(key.len())];
    let suffix = &key[key.len().saturating_sub(1)..];
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

/// Get environment variable name for provider
pub fn provider_env_var(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "groq" => "GROQ_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "ollama" => "OLLAMA_API_BASE_URL",
        _ => "UNKNOWN_API_KEY",
    }
}

#[cfg(test)]
mod tests {
    // ... tests from Step 1
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::keyring::tests -- --nocapture`
Expected: PASS (9 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/keyring.rs
git commit -m "feat(tui): add NikaKeyring with secure credential storage"
```

---

## Phase 4: OllamaClient (TDD)

### Task 4.1: OllamaClient types

**Files:**
- Modify: `src/tui/widgets/provider_modal/ollama_client.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/provider_modal/ollama_client.rs
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

    #[test]
    fn test_model_info_size_display() {
        let model = OllamaModelInfo {
            name: "test".into(),
            size: 4_700_000_000,
            digest: "sha256:abc".into(),
            modified_at: "2026-01-01".into(),
            details: OllamaModelDetails {
                parameter_size: "8B".into(),
                quantization_level: "Q4_0".into(),
                family: None,
            },
        };
        assert_eq!(model.size_display(), "4.7 GB");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::ollama_client::tests -- --nocapture`
Expected: FAIL with "cannot find function `ollama_base_url`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/provider_modal/ollama_client.rs
//! Native Ollama HTTP client for model management
//!
//! Based on:
//! - Context7: Ollama REST API documentation
//! - Perplexity: NDJSON streaming best practices
//! - Codebase: Existing reqwest patterns from executor.rs

use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::mpsc;

/// Ollama API base URL (default: http://localhost:11434)
pub fn ollama_base_url() -> String {
    std::env::var("OLLAMA_HOST")
        .or_else(|_| std::env::var("OLLAMA_API_BASE_URL"))
        .unwrap_or_else(|_| "http://localhost:11434".to_string())
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

impl OllamaModelInfo {
    /// Format size for display
    pub fn size_display(&self) -> String {
        if self.size >= 1_000_000_000 {
            format!("{:.1} GB", self.size as f64 / 1_000_000_000.0)
        } else if self.size >= 1_000_000 {
            format!("{:.1} MB", self.size as f64 / 1_000_000.0)
        } else {
            format!("{:.1} KB", self.size as f64 / 1_000.0)
        }
    }
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

/// Ollama client error types
#[derive(Debug, Error)]
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

/// Native Ollama HTTP client
pub struct OllamaClient {
    client: Client,
    base_url: String,
}

impl Default for OllamaClient {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaClient {
    /// Create client with connection pooling
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

        let response = self
            .client
            .get(format!("{}/api/tags", self.base_url))
            .send()
            .await
            .map_err(|e| OllamaError::ConnectionFailed(e.to_string()))?;

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| OllamaError::ParseError(e.to_string()))?;

        Ok(tags.models)
    }

    /// Pull model with streaming progress
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

        let response = self
            .client
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
}

#[cfg(test)]
mod tests {
    // ... tests from Step 1
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::ollama_client::tests -- --nocapture`
Expected: PASS (5 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/ollama_client.rs
git commit -m "feat(tui): add OllamaClient with streaming NDJSON support"
```

---

## Phase 5: UI Components (TDD)

### Task 5.1: ProviderCard widget

**Files:**
- Create: `src/tui/widgets/provider_modal/components/provider_card.rs`
- Modify: `src/tui/widgets/provider_modal/components/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/provider_modal/components/provider_card.rs
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_card_style_colors() {
        assert_eq!(CardStyle::Selected.border_color(), Color::Rgb(99, 102, 241));
        assert_eq!(CardStyle::Normal.border_color(), Color::Rgb(55, 65, 81));
        assert_eq!(CardStyle::Disabled.border_color(), Color::Rgb(31, 41, 55));
    }

    #[test]
    fn test_card_renders_without_panic() {
        let status = super::super::super::state::ConnectionStatus::Connected { latency_ms: 100 };
        let card = ProviderCard::new("🧠", "Claude", "claude-sonnet-4", &status);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 5));
        card.render(Rect::new(0, 0, 60, 5), &mut buf);

        // Should render without panic
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Claude"));
    }

    #[test]
    fn test_card_renders_in_small_area() {
        let status = super::super::super::state::ConnectionStatus::Unknown;
        let card = ProviderCard::new("🧠", "Claude", "claude-sonnet-4", &status);

        // Should not panic with small area
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
        card.render(Rect::new(0, 0, 10, 2), &mut buf);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::components::provider_card::tests -- --nocapture`
Expected: FAIL with "cannot find type `ProviderCard`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/provider_modal/components/provider_card.rs
//! Rich provider card with status, sparkline, and model info

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Widget},
};

use super::super::state::ConnectionStatus;

/// Visual style for provider cards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardStyle {
    #[default]
    Normal,
    Selected,
    Disabled,
}

impl CardStyle {
    pub fn border_color(&self) -> Color {
        match self {
            Self::Selected => Color::Rgb(99, 102, 241),  // indigo
            Self::Normal => Color::Rgb(55, 65, 81),      // gray
            Self::Disabled => Color::Rgb(31, 41, 55),    // dark gray
        }
    }

    pub fn bg_color(&self) -> Color {
        match self {
            Self::Selected => Color::Rgb(30, 41, 59),    // slate-800
            Self::Normal => Color::Rgb(17, 24, 39),      // gray-900
            Self::Disabled => Color::Rgb(17, 24, 39),
        }
    }
}

/// Provider card showing rich status info
pub struct ProviderCard<'a> {
    icon: &'a str,
    name: &'a str,
    model: &'a str,
    status: &'a ConnectionStatus,
    features: Vec<&'a str>,
    context_window: u32,
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

    pub fn style(mut self, style: CardStyle) -> Self {
        self.style = style;
        self
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
            .border_style(Style::default().fg(self.style.border_color()))
            .style(Style::default().bg(self.style.bg_color()))
            .title(Span::styled(
                format!(" {} {} ", self.icon, self.name),
                Style::default()
                    .fg(Color::Rgb(229, 231, 235))
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        block.render(area, buf);

        // Row 1: Model name
        let model_style = Style::default().fg(Color::Rgb(156, 163, 175));
        buf.set_string(inner.x + 1, inner.y, self.model, model_style);

        // Status on the right
        let status_text = self.status.display_text();
        let status_color = match self.status {
            ConnectionStatus::Connected { .. } => Color::Rgb(34, 197, 94),
            ConnectionStatus::Failed { .. } => Color::Rgb(239, 68, 68),
            ConnectionStatus::Checking => Color::Rgb(59, 130, 246),
            _ => Color::Rgb(107, 114, 128),
        };
        let status_x = inner.right().saturating_sub(status_text.len() as u16 + 1);
        buf.set_string(
            status_x,
            inner.y,
            &status_text,
            Style::default().fg(status_color),
        );

        // Row 2: Features + Context window
        if inner.height >= 2 {
            let features_str = self.features.join(" ");
            buf.set_string(
                inner.x + 1,
                inner.y + 1,
                &features_str,
                Style::default().fg(Color::Rgb(59, 130, 246)),
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
    }
}

#[cfg(test)]
mod tests {
    // ... tests from Step 1
}
```

**Step 4: Update components/mod.rs**

```rust
// src/tui/widgets/provider_modal/components/mod.rs
//! UI components for provider modal

mod provider_card;

pub use provider_card::*;
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::components::provider_card::tests -- --nocapture`
Expected: PASS (3 tests)

**Step 6: Commit**

```bash
git add src/tui/widgets/provider_modal/components/
git commit -m "feat(tui): add ProviderCard widget with status display"
```

---

### Task 5.2: DownloadGauge widget

**Files:**
- Create: `src/tui/widgets/provider_modal/components/download_gauge.rs`
- Modify: `src/tui/widgets/provider_modal/components/mod.rs`

**Step 1: Write the failing test**

```rust
// src/tui/widgets/provider_modal/components/download_gauge.rs
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

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
    fn test_format_speed() {
        assert_eq!(DownloadGauge::format_speed(50_000_000), "50.0 MB/s");
        assert_eq!(DownloadGauge::format_speed(500_000), "500.0 KB/s");
    }

    #[test]
    fn test_progress_clamp() {
        let gauge = DownloadGauge::new("test", 1.5, 100, 100);
        assert!((gauge.progress - 1.0).abs() < f64::EPSILON);

        let gauge = DownloadGauge::new("test", -0.5, 0, 100);
        assert!((gauge.progress - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_gauge_renders() {
        let gauge = DownloadGauge::new("llama3.2", 0.45, 2_100_000_000, 4_700_000_000);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 4));
        gauge.render(Rect::new(0, 0, 60, 4), &mut buf);

        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("llama3.2"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::components::download_gauge::tests -- --nocapture`
Expected: FAIL with "cannot find type `DownloadGauge`"

**Step 3: Write minimal implementation**

```rust
// src/tui/widgets/provider_modal/components/download_gauge.rs
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
    pub progress: f64,
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

    pub fn format_speed(bps: u64) -> String {
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
            .gauge_style(
                Style::default()
                    .fg(Color::Rgb(34, 197, 94))
                    .bg(Color::Rgb(31, 41, 55)),
            )
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

#[cfg(test)]
mod tests {
    // ... tests from Step 1
}
```

**Step 4: Update components/mod.rs**

```rust
// src/tui/widgets/provider_modal/components/mod.rs
mod download_gauge;
mod provider_card;

pub use download_gauge::*;
pub use provider_card::*;
```

**Step 5: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::components::download_gauge::tests -- --nocapture`
Expected: PASS (5 tests)

**Step 6: Commit**

```bash
git add src/tui/widgets/provider_modal/components/
git commit -m "feat(tui): add DownloadGauge widget with progress display"
```

---

## Phase 6: Tab Implementations

### Task 6.1: Create tabs module structure

**Files:**
- Modify: `src/tui/widgets/provider_modal/tabs/mod.rs`
- Create: `src/tui/widgets/provider_modal/tabs/cloud.rs`
- Create: `src/tui/widgets/provider_modal/tabs/ollama.rs`
- Create: `src/tui/widgets/provider_modal/tabs/keys.rs`

**Step 1: Create tab stubs**

```rust
// src/tui/widgets/provider_modal/tabs/mod.rs
mod cloud;
mod keys;
mod ollama;

pub use cloud::*;
pub use keys::*;
pub use ollama::*;
```

```rust
// src/tui/widgets/provider_modal/tabs/cloud.rs
//! Cloud providers tab
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

pub struct CloudTab;

impl Widget for CloudTab {
    fn render(self, _area: Rect, _buf: &mut Buffer) {
        // TODO: Implement
    }
}
```

```rust
// src/tui/widgets/provider_modal/tabs/ollama.rs
//! Ollama local models tab
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

pub struct OllamaTab;

impl Widget for OllamaTab {
    fn render(self, _area: Rect, _buf: &mut Buffer) {
        // TODO: Implement
    }
}
```

```rust
// src/tui/widgets/provider_modal/tabs/keys.rs
//! API Keys tab
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

pub struct KeysTab;

impl Widget for KeysTab {
    fn render(self, _area: Rect, _buf: &mut Buffer) {
        // TODO: Implement
    }
}
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles successfully

**Step 3: Commit**

```bash
git add src/tui/widgets/provider_modal/tabs/
git commit -m "feat(tui): scaffold tab implementations"
```

---

## Phase 7: Main Modal Widget

### Task 7.1: ProviderModal widget

**Files:**
- Modify: `src/tui/widgets/provider_modal/mod.rs`

**Step 1: Write the failing test**

Add to `mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_modal_hidden_does_not_render() {
        let state = ProviderModalState::default(); // visible: false
        let modal = ProviderModal::new(&state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        modal.render(Rect::new(0, 0, 80, 24), &mut buf);

        // All cells should be empty space
        assert!(buf.content().iter().all(|c| c.symbol() == " "));
    }

    #[test]
    fn test_modal_visible_renders_border() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        let modal = ProviderModal::new(&state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        modal.render(Rect::new(0, 0, 80, 24), &mut buf);

        // Should render PROVIDERS title
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("PROVIDERS"));
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p nika --lib provider_modal::tests -- --nocapture`
Expected: FAIL with "cannot find type `ProviderModal`"

**Step 3: Write minimal implementation**

Update `mod.rs`:

```rust
//! Provider Modal v2

mod components;
mod keyring;
mod ollama_client;
mod state;
mod tabs;

pub use components::*;
pub use keyring::*;
pub use ollama_client::*;
pub use state::*;
pub use tabs::*;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
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
        let tab_titles: Vec<Span> = vec![
            Span::raw(ProviderModalTab::Cloud.label()),
            Span::raw(ProviderModalTab::Ollama.label()),
            Span::raw(ProviderModalTab::Keys.label()),
            Span::raw(ProviderModalTab::Config.label()),
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

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let hints = match self.state.active_tab {
            ProviderModalTab::Cloud => "[↑↓] Navigate │ [Enter] Select │ [Tab] Next │ [Esc] Close",
            ProviderModalTab::Ollama => "[↑↓] Navigate │ [p] Pull │ [d] Delete │ [Esc] Close",
            ProviderModalTab::Keys => "[↑↓] Navigate │ [Enter] Edit │ [t] Test │ [Esc] Close",
            ProviderModalTab::Config => "[↑↓] Navigate │ [Enter] Toggle │ [Esc] Close",
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

        // Render tabs
        self.render_tabs(chunks[0], buf);

        // Render tab content
        match self.state.active_tab {
            ProviderModalTab::Cloud => CloudTab.render(chunks[1], buf),
            ProviderModalTab::Ollama => OllamaTab.render(chunks[1], buf),
            ProviderModalTab::Keys => KeysTab.render(chunks[1], buf),
            ProviderModalTab::Config => {
                buf.set_string(
                    chunks[1].x + 2,
                    chunks[1].y + 1,
                    "Configuration coming soon...",
                    Style::default().fg(Color::Rgb(156, 163, 175)),
                );
            }
        }

        // Render footer
        self.render_footer(chunks[2], buf);
    }
}

#[cfg(test)]
mod tests {
    // ... tests from Step 1
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p nika --lib provider_modal::tests -- --nocapture`
Expected: PASS (2 tests)

**Step 5: Commit**

```bash
git add src/tui/widgets/provider_modal/mod.rs
git commit -m "feat(tui): add ProviderModal main widget with tabs"
```

---

## Summary

| Phase | Tasks | Tests | Description |
|-------|-------|-------|-------------|
| 1 | 2 | 0 | Dependencies & module setup |
| 2 | 5 | 18 | Core state types (TDD) |
| 3 | 1 | 9 | Keyring integration (TDD) |
| 4 | 1 | 5 | OllamaClient (TDD) |
| 5 | 2 | 8 | UI components (TDD) |
| 6 | 1 | 0 | Tab scaffolding |
| 7 | 1 | 2 | Main modal widget |

**Total: 13 tasks, 42 tests**

---

## Next Steps (Future Tasks)

After completing this foundation:

1. **Task 8-10:** Implement full CloudTab with provider cards
2. **Task 11-13:** Implement full OllamaTab with download streaming
3. **Task 14-16:** Implement full KeysTab with input handling
4. **Task 17-19:** Integration with ChatView
5. **Task 20-22:** Phase 7 cosmetic enhancements (MatrixDecrypt, spinners)

---

## References

- @src/tui/widgets/provider_selector.rs — Existing provider selector (deprecated)
- @src/tui/views/chat.rs — ChatView integration point
- @Cargo.toml — Dependencies
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
- keyring-rs: https://docs.rs/keyring/3/keyring/
