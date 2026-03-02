# Unified Keyring Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Unify keyring management so both `nika` and `spn` use the same keychain service `"spn"`.

**Architecture:** Change Nika's service name from `"nika"` to `"spn"` and rename `NikaKeyring` to `SpnKeyring`. Add security features (zeroize, SecretString).

**Tech Stack:** `keyring = "3"`, `secrecy`, `zeroize`, Rust

---

## What Changes

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           BEFORE → AFTER                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  BEFORE:                              AFTER:                                │
│  ├── Service: "nika" (Nika)           └── Service: "spn" (both)            │
│  └── Service: "spn"  (spn)                ├── anthropic: sk-ant-...        │
│      ❌ DUPLICATED KEYS                    ├── openai: sk-...               │
│                                           └── ✅ SINGLE SOURCE OF TRUTH    │
│                                                                             │
│  # Both use service "spn"                                                   │
│  nika provider set anthropic                                                │
│  # → Keychain: service="spn", account="anthropic"                          │
│                                                                             │
│  spn provider set anthropic                                                 │
│  # → Keychain: service="spn", account="anthropic"                          │
│  # ✅ SAME ENTRY!                                                           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Summary

| Question | Answer |
|----------|--------|
| Nika garde provider/model selection ? | ✅ Oui, inchangé |
| Nika garde MCP client ? | ✅ Oui, inchangé |
| Qui gère les clés ? | Les deux lisent/écrivent au même endroit |
| Où sont stockées les clés ? | `service="spn"` dans le keychain OS |
| Pourquoi "spn" ? | Plus complet (+ de providers, + sécurisé) |

**Le seul changement technique :** `SERVICE_NAME = "nika"` → `SERVICE_NAME = "spn"`

---

## Files to Modify

| File | Action | Purpose |
|------|--------|---------|
| `tools/nika/Cargo.toml` | Modify | Add `secrecy`, `zeroize` deps |
| `tools/nika/src/tui/widgets/provider_modal/keyring.rs` | Rewrite | Rename to SpnKeyring, change service name |
| `tools/nika/src/tui/widgets/provider_modal/mod.rs` | Modify | Re-export SpnKeyring |
| `tools/nika/src/main.rs` | Modify | Update to SpnKeyring |
| `tools/nika/src/tui/views/chat.rs` | Modify | Update to SpnKeyring |
| `tools/nika/src/tui/widgets/provider_modal/tabs/keys.rs` | Modify | Update to SpnKeyring |

---

## Task 1: Add Security Dependencies

**Files:**
- Modify: `tools/nika/Cargo.toml:53-54`

**Step 1: Add secrecy and zeroize crates**

After line 53 (`keyring = "3"`), add:

```toml
# Secure credential storage
keyring = "3"
secrecy = "0.8"    # SecretString wrapper
zeroize = "1.8"    # Auto-clear memory on drop
```

**Step 2: Verify**

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika && cargo check
```

**Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps(nika): add secrecy and zeroize for secure keyring"
```

---

## Task 2: Rewrite keyring.rs with SpnKeyring

**Files:**
- Rewrite: `tools/nika/src/tui/widgets/provider_modal/keyring.rs`

**Step 1: Replace entire file content**

```rust
//! Secure API key storage via system keychain.
//!
//! Uses keyring-rs for cross-platform credential storage:
//! - macOS: Keychain Access
//! - Windows: Credential Manager
//! - Linux: Secret Service (GNOME Keyring, KWallet)
//!
//! Service name "spn" is shared with supernovae-cli for unified key management.

use colored::Colorize;
use keyring::Entry;
use secrecy::SecretString;
use thiserror::Error;
use zeroize::Zeroizing;

/// Service name for keyring entries.
/// Uses "spn" for unified keyring with supernovae-cli.
const SERVICE_NAME: &str = "spn";

/// Keyring error types.
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

/// Keyring wrapper for spn API keys (unified with supernovae-cli).
pub struct SpnKeyring;

impl SpnKeyring {
    /// Get API key for a provider as zeroizing string.
    ///
    /// The returned string will be automatically zeroized when dropped.
    pub fn get(provider: &str) -> Result<Zeroizing<String>, KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        let password = entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => KeyringError::NotFound(provider.to_string()),
            _ => KeyringError::AccessError(e.to_string()),
        })?;

        Ok(Zeroizing::new(password))
    }

    /// Get API key wrapped in SecretString for maximum safety.
    pub fn get_secret(provider: &str) -> Result<SecretString, KeyringError> {
        let key = Self::get(provider)?;
        Ok(SecretString::from((*key).clone()))
    }

    /// Store API key for a provider.
    pub fn set(provider: &str, key: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .set_password(key)
            .map_err(|e| KeyringError::StoreError(e.to_string()))
    }

    /// Delete API key for a provider.
    pub fn delete(provider: &str) -> Result<(), KeyringError> {
        let entry = Entry::new(SERVICE_NAME, provider)
            .map_err(|e| KeyringError::AccessError(e.to_string()))?;

        entry
            .delete_credential()
            .map_err(|e| KeyringError::DeleteError(e.to_string()))
    }

    /// Check if key exists for a provider.
    pub fn exists(provider: &str) -> bool {
        Self::get(provider).is_ok()
    }

    /// Get masked version of stored key.
    pub fn get_masked(provider: &str) -> Option<String> {
        Self::get(provider).ok().map(|k| mask_api_key(&k))
    }
}

/// Mask API key for display (show first 6 and last 1 char).
pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        return "****".to_string();
    }
    let prefix = &key[..6.min(key.len())];
    let suffix = &key[key.len().saturating_sub(1)..];
    format!("{}...{}", prefix, suffix)
}

/// Validate API key format (basic checks).
pub fn validate_key_format(provider: &str, key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("API key cannot be empty".into());
    }

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
        "mistral" | "groq" | "deepseek" => {
            if key.len() < 32 {
                return Err("Key seems too short".into());
            }
        }
        "ollama" => {
            // Ollama doesn't use API keys, but may use base URL
        }
        _ => {
            if key.len() < 10 {
                return Err("Key seems too short".into());
            }
        }
    }
    Ok(())
}

/// Get environment variable name for provider.
pub fn provider_env_var(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        "mistral" => "MISTRAL_API_KEY",
        "groq" => "GROQ_API_KEY",
        "deepseek" => "DEEPSEEK_API_KEY",
        "gemini" => "GEMINI_API_KEY",
        "ollama" => "OLLAMA_API_BASE_URL",
        _ => "UNKNOWN_API_KEY",
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// MIGRATION (env → keyring)
// ═══════════════════════════════════════════════════════════════════════════════

const MIGRATEABLE_PROVIDERS: &[&str] = &["anthropic", "openai", "mistral", "groq", "deepseek", "gemini"];

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub migrated: usize,
    pub skipped: usize,
    pub not_found: Vec<String>,
    pub errors: Vec<(String, String)>,
}

impl MigrationReport {
    pub fn summary(&self) -> String {
        format!(
            "Migration complete: {} migrated, {} skipped, {} not found",
            self.migrated,
            self.skipped,
            self.not_found.len()
        )
    }
}

/// Migrate API keys from environment variables to system keychain.
pub fn migrate_env_to_keyring() -> MigrationReport {
    let mut report = MigrationReport::default();

    for provider in MIGRATEABLE_PROVIDERS {
        let env_var = provider_env_var(provider);

        match std::env::var(env_var) {
            Ok(key) if !key.is_empty() => {
                if SpnKeyring::exists(provider) {
                    println!(
                        "  ├── {}: Found → {}",
                        env_var,
                        "Already in keychain".yellow()
                    );
                    report.skipped += 1;
                    continue;
                }

                print!("  ├── {}: Found → Migrating... ", env_var);
                match SpnKeyring::set(provider, &key) {
                    Ok(()) => {
                        println!("{}", "✓".green());
                        report.migrated += 1;
                    }
                    Err(e) => {
                        println!("{} ({})", "✗".red(), e);
                        report.errors.push((provider.to_string(), e.to_string()));
                    }
                }
            }
            _ => {
                println!("  ├── {}: {}", env_var, "Not found".dimmed());
                report.not_found.push(provider.to_string());
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_name_is_spn() {
        assert_eq!(SERVICE_NAME, "spn");
    }

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
    }

    #[test]
    fn test_validate_openai_key_valid() {
        let result = validate_key_format("openai", "sk-proj-abcdefghijklmnop");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_empty_key_rejected() {
        let result = validate_key_format("anthropic", "");
        assert!(result.is_err());
    }

    #[test]
    fn test_provider_env_var() {
        assert_eq!(provider_env_var("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(provider_env_var("openai"), "OPENAI_API_KEY");
        assert_eq!(provider_env_var("gemini"), "GEMINI_API_KEY");
    }
}
```

**Step 2: Verify**

```bash
cargo check
```

**Step 3: Commit**

```bash
git add src/tui/widgets/provider_modal/keyring.rs
git commit -m "feat(keyring): rename to SpnKeyring with unified 'spn' service"
```

---

## Task 3: Update mod.rs exports

**Files:**
- Modify: `tools/nika/src/tui/widgets/provider_modal/mod.rs:20`

**Step 1: Change re-export**

From:
```rust
pub use keyring::*;
```

To:
```rust
pub use keyring::{
    mask_api_key, migrate_env_to_keyring, provider_env_var, validate_key_format,
    KeyringError, MigrationReport, SpnKeyring,
};
```

**Step 2: Commit**

```bash
git add src/tui/widgets/provider_modal/mod.rs
git commit -m "refactor(keyring): explicit re-exports for SpnKeyring"
```

---

## Task 4: Update main.rs

**Files:**
- Modify: `tools/nika/src/main.rs`

**Step 1: Find and replace all `NikaKeyring` with `SpnKeyring`**

Search for `NikaKeyring` (around lines 1184-1340) and replace with `SpnKeyring`.

**Step 2: Verify**

```bash
cargo check
```

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "refactor(cli): use SpnKeyring for unified keychain"
```

---

## Task 5: Update chat.rs

**Files:**
- Modify: `tools/nika/src/tui/views/chat.rs`

**Step 1: Find and replace all `NikaKeyring` with `SpnKeyring`**

Around lines 4244-4280.

**Step 2: Verify**

```bash
cargo check
```

**Step 3: Commit**

```bash
git add src/tui/views/chat.rs
git commit -m "refactor(chat): use SpnKeyring for unified keychain"
```

---

## Task 6: Update keys.rs

**Files:**
- Modify: `tools/nika/src/tui/widgets/provider_modal/tabs/keys.rs`

**Step 1: Find and replace all `NikaKeyring` with `SpnKeyring`**

Around lines 44 and 106.

**Step 2: Verify**

```bash
cargo check
```

**Step 3: Commit**

```bash
git add src/tui/widgets/provider_modal/tabs/keys.rs
git commit -m "refactor(keys): use SpnKeyring for unified keychain"
```

---

## Task 7: Run Tests

**Step 1: Run full test suite**

```bash
cargo test
```

Expected: All 3,375+ tests pass

**Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```

Expected: No warnings

**Step 3: Final commit**

```bash
git add -A
git commit -m "test: verify unified keyring works"
```

---

## Summary

| Task | Description | Files |
|------|-------------|-------|
| 1 | Add secrecy/zeroize deps | Cargo.toml |
| 2 | Rewrite keyring.rs with SpnKeyring | keyring.rs |
| 3 | Update mod.rs exports | mod.rs |
| 4 | Update main.rs | main.rs |
| 5 | Update chat.rs | chat.rs |
| 6 | Update keys.rs | keys.rs |
| 7 | Run tests | - |

**Total commits:** 7
**Estimated time:** 30-45 minutes

---

## Changelog Entry

```markdown
## [0.17.3] - 2026-03-03

### Changed
- **Unified Keyring with spn CLI** - Both tools now share `"spn"` keychain service
  - Renamed `NikaKeyring` → `SpnKeyring`
  - Changed service name from `"nika"` to `"spn"`
  - Added `Zeroizing<String>` return type for secure memory handling
  - Added `SecretString` support for API safety
  - Keys stored by either tool are accessible by both
```
