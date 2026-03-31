//! NikaVault — encrypted local file store for API secrets.
//!
//! Uses XChaCha20Poly1305 (AEAD) for encryption with Argon2i KDF for key derivation.
//! The key is derived from a machine fingerprint (machine-id + username) or an explicit
//! passphrase set via `NIKA_VAULT_PASSPHRASE` (for CI/Docker).
//!
//! Layout:
//! - `<secrets_dir>/vault.enc` — encrypted JSON payload
//! - `<secrets_dir>/vault.salt` — 16-byte random salt (plaintext)

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use orion::aead;
use orion::kdf;
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Vault-specific error type (lightweight — no nika-engine dependency).
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("vault I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("vault crypto error: {0}")]
    Crypto(String),
    #[error("vault JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Internal plaintext structure stored inside the encrypted vault.
#[derive(Serialize, Deserialize, Default)]
struct VaultPayload {
    version: u32,
    secrets: BTreeMap<String, String>,
}

/// Encrypted local file store for API secrets.
pub struct NikaVault {
    vault_path: PathBuf,
    salt_path: PathBuf,
}

impl NikaVault {
    /// Create a new vault pointed at the given secrets directory.
    ///
    /// Does NOT create files — they are created lazily on first `set()`.
    pub fn new(secrets_dir: &Path) -> Self {
        Self {
            vault_path: secrets_dir.join("vault.enc"),
            salt_path: secrets_dir.join("vault.salt"),
        }
    }

    /// Get a secret by provider name.
    pub fn get(&self, provider: &str) -> Result<Option<SecretString>, VaultError> {
        let payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(None),
        };
        Ok(payload
            .secrets
            .get(provider)
            .map(|s| SecretString::from(s.clone())))
    }

    /// Store a secret for a provider (creates or updates).
    pub fn set(&self, provider: &str, secret: &str) -> Result<(), VaultError> {
        let mut payload = self.read_payload()?.unwrap_or_default();
        payload.version = 1;
        payload
            .secrets
            .insert(provider.to_string(), secret.to_string());
        self.write_payload(&payload)
    }

    /// Delete a secret. Returns true if it existed.
    pub fn delete(&self, provider: &str) -> Result<bool, VaultError> {
        let mut payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(false),
        };
        let existed = payload.secrets.remove(provider).is_some();
        if existed {
            self.write_payload(&payload)?;
        }
        Ok(existed)
    }

    /// List all provider names that have stored secrets.
    pub fn list(&self) -> Result<Vec<String>, VaultError> {
        let payload = self.read_payload()?.unwrap_or_default();
        Ok(payload.secrets.keys().cloned().collect())
    }

    // ── Internal ────────────────────────────────────────────────────────

    fn read_payload(&self) -> Result<Option<VaultPayload>, VaultError> {
        if !self.vault_path.exists() {
            return Ok(None);
        }
        let ciphertext = std::fs::read(&self.vault_path)?;
        let key = self.derive_key()?;
        let plaintext = aead::open(&key, &ciphertext)
            .map_err(|e| VaultError::Crypto(format!("decrypt failed: {e}")))?;
        let payload: VaultPayload = serde_json::from_slice(&plaintext)?;
        Ok(Some(payload))
    }

    fn write_payload(&self, payload: &VaultPayload) -> Result<(), VaultError> {
        if let Some(parent) = self.vault_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let plaintext = serde_json::to_vec(payload)?;
        let key = self.derive_key()?;
        let ciphertext = aead::seal(&key, &plaintext)
            .map_err(|e| VaultError::Crypto(format!("encrypt failed: {e}")))?;

        std::fs::write(&self.vault_path, &ciphertext)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&self.vault_path, perms.clone())?;
            if self.salt_path.exists() {
                std::fs::set_permissions(&self.salt_path, perms)?;
            }
        }

        debug!("vault written: {} providers", payload.secrets.len());
        Ok(())
    }

    fn derive_key(&self) -> Result<orion::aead::SecretKey, VaultError> {
        let salt = self.load_or_create_salt()?;
        let fingerprint = machine_fingerprint()?;

        let password = kdf::Password::from_slice(fingerprint.as_bytes())
            .map_err(|e| VaultError::Crypto(format!("KDF password: {e}")))?;
        let kdf_salt = kdf::Salt::from_slice(&salt)
            .map_err(|e| VaultError::Crypto(format!("KDF salt: {e}")))?;

        let derived = kdf::derive_key(&password, &kdf_salt, 3, 1 << 16, 32)
            .map_err(|e| VaultError::Crypto(format!("KDF derive: {e}")))?;

        orion::aead::SecretKey::from_slice(derived.unprotected_as_bytes())
            .map_err(|e| VaultError::Crypto(format!("AEAD key: {e}")))
    }

    fn load_or_create_salt(&self) -> Result<Vec<u8>, VaultError> {
        if self.salt_path.exists() {
            let salt = std::fs::read(&self.salt_path)?;
            if salt.len() >= 16 {
                return Ok(salt);
            }
            debug!("vault salt too short ({} bytes), regenerating", salt.len());
        }
        if let Some(parent) = self.salt_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut salt = vec![0u8; 16];
        orion::util::secure_rand_bytes(&mut salt)
            .map_err(|e| VaultError::Crypto(format!("CSPRNG: {e}")))?;
        std::fs::write(&self.salt_path, &salt)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.salt_path, std::fs::Permissions::from_mode(0o600))?;
        }

        debug!("vault salt created");
        Ok(salt)
    }
}

fn machine_fingerprint() -> Result<String, VaultError> {
    if let Ok(pass) = std::env::var("NIKA_VAULT_PASSPHRASE") {
        if !pass.is_empty() {
            return Ok(format!("nika-vault-v1:passphrase:{pass}"));
        }
    }
    let machine_id = get_machine_id()?;
    let username = whoami::username();
    Ok(format!("nika-vault-v1:{machine_id}:{username}"))
}

#[cfg(target_os = "linux")]
fn get_machine_id() -> Result<String, VaultError> {
    std::fs::read_to_string("/etc/machine-id")
        .map(|s| s.trim().to_string())
        .map_err(|e| {
            VaultError::Io(std::io::Error::new(
                e.kind(),
                format!("Cannot read /etc/machine-id: {e}. Set NIKA_VAULT_PASSPHRASE."),
            ))
        })
}

#[cfg(target_os = "macos")]
fn get_machine_id() -> Result<String, VaultError> {
    let output = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(uuid) = line.split('"').nth(3) {
                return Ok(uuid.to_string());
            }
        }
    }
    Err(VaultError::Crypto("IOPlatformUUID not found".into()))
}

#[cfg(target_os = "windows")]
fn get_machine_id() -> Result<String, VaultError> {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("MachineGuid") {
            if let Some(guid) = line.split_whitespace().last() {
                return Ok(guid.to_string());
            }
        }
    }
    Err(VaultError::Crypto("MachineGuid not found".into()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn get_machine_id() -> Result<String, VaultError> {
    Err(VaultError::Crypto(
        "No machine-id on this platform. Set NIKA_VAULT_PASSPHRASE.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;
    use serial_test::serial;
    use tempfile::TempDir;

    fn test_vault() -> (TempDir, NikaVault) {
        let dir = TempDir::new().unwrap();
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let vault = NikaVault::new(dir.path());
        (dir, vault)
    }

    #[test]
    #[serial]
    fn set_and_get() {
        let (_dir, vault) = test_vault();
        vault.set("anthropic", "sk-ant-test").unwrap();
        let s = vault.get("anthropic").unwrap().unwrap();
        assert_eq!(s.expose_secret(), "sk-ant-test");
    }

    #[test]
    #[serial]
    fn get_nonexistent() {
        let (_dir, vault) = test_vault();
        assert!(vault.get("nope").unwrap().is_none());
    }

    #[test]
    #[serial]
    fn overwrite() {
        let (_dir, vault) = test_vault();
        vault.set("k", "old").unwrap();
        vault.set("k", "new").unwrap();
        assert_eq!(vault.get("k").unwrap().unwrap().expose_secret(), "new");
    }

    #[test]
    #[serial]
    fn delete_existing() {
        let (_dir, vault) = test_vault();
        vault.set("x", "val").unwrap();
        assert!(vault.delete("x").unwrap());
        assert!(vault.get("x").unwrap().is_none());
    }

    #[test]
    #[serial]
    fn delete_nonexistent() {
        let (_dir, vault) = test_vault();
        assert!(!vault.delete("nope").unwrap());
    }

    #[test]
    #[serial]
    fn list_providers() {
        let (_dir, vault) = test_vault();
        vault.set("a", "1").unwrap();
        vault.set("b", "2").unwrap();
        let mut list = vault.list().unwrap();
        list.sort();
        assert_eq!(list, vec!["a", "b"]);
    }

    #[test]
    #[serial]
    fn corrupted_file_errors() {
        let (dir, vault) = test_vault();
        vault.set("dummy", "x").unwrap();
        std::fs::write(dir.path().join("vault.enc"), b"garbage").unwrap();
        assert!(vault.get("any").is_err());
    }

    #[test]
    #[serial]
    #[cfg(unix)]
    fn file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, vault) = test_vault();
        vault.set("test", "secret").unwrap();
        let perms = std::fs::metadata(dir.path().join("vault.enc"))
            .unwrap()
            .permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    #[serial]
    fn multiple_providers_persist() {
        let (_dir, vault) = test_vault();
        vault.set("anthropic", "sk-1").unwrap();
        vault.set("openai", "sk-2").unwrap();
        vault.set("gemini", "sk-3").unwrap();
        assert_eq!(
            vault.get("anthropic").unwrap().unwrap().expose_secret(),
            "sk-1"
        );
        assert_eq!(
            vault.get("openai").unwrap().unwrap().expose_secret(),
            "sk-2"
        );
        assert_eq!(
            vault.get("gemini").unwrap().unwrap().expose_secret(),
            "sk-3"
        );
    }
}
