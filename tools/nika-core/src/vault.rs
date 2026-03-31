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

/// A single vault entry — either a simple API key string (v1 compat)
/// or a multi-field credential (v2).
///
/// Uses `#[serde(untagged)]` so a plain JSON string deserializes as `Key`
/// and a JSON object deserializes as `Credential`. This gives seamless
/// v1 → v2 migration: existing vault files with string values just work.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum VaultEntry {
    /// Simple API key (v1 backward compat)
    Key(String),
    /// Multi-field credential (v2)
    Credential {
        /// Named fields (e.g., "api_key", "secret", "org_id")
        fields: BTreeMap<String, String>,
        /// Optional service URL (e.g., "https://api.stripe.com")
        #[serde(skip_serializing_if = "Option::is_none")]
        service_url: Option<String>,
        /// Optional category (e.g., "payment", "llm", "storage")
        #[serde(skip_serializing_if = "Option::is_none")]
        category: Option<String>,
        /// ISO 8601 timestamp when the credential was stored
        #[serde(skip_serializing_if = "Option::is_none")]
        created_at: Option<String>,
        /// ISO 8601 timestamp when the credential expires
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_at: Option<String>,
    },
}

impl VaultEntry {
    /// Get the primary key value (for backward compat).
    ///
    /// - `Key(s)` → returns `s`
    /// - `Credential { fields, .. }` → returns the first field value (alphabetical)
    fn primary_value(&self) -> Option<&str> {
        match self {
            VaultEntry::Key(s) => Some(s.as_str()),
            VaultEntry::Credential { fields, .. } => {
                fields.values().next().map(|s| s.as_str())
            }
        }
    }
}

/// Internal plaintext structure stored inside the encrypted vault.
///
/// v1: `secrets` contained `BTreeMap<String, String>` (plain keys).
/// v2: `secrets` contains `BTreeMap<String, VaultEntry>` (keys or credentials).
///
/// Thanks to `VaultEntry`'s `#[serde(untagged)]`, v1 payloads deserialize
/// transparently — each `String` becomes `VaultEntry::Key(s)`.
#[derive(Serialize, Deserialize, Default)]
struct VaultPayload {
    version: u32,
    secrets: BTreeMap<String, VaultEntry>,
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
    ///
    /// For `VaultEntry::Key(s)`, returns the key string.
    /// For `VaultEntry::Credential { fields, .. }`, returns the first field value.
    pub fn get(&self, provider: &str) -> Result<Option<SecretString>, VaultError> {
        let payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(None),
        };
        Ok(payload.secrets.get(provider).and_then(|entry| {
            entry.primary_value().map(|s| SecretString::from(s.to_owned()))
        }))
    }

    /// Store a simple secret for a provider (creates or updates).
    pub fn set(&self, provider: &str, secret: &str) -> Result<(), VaultError> {
        let mut payload = self.read_payload()?.unwrap_or_default();
        payload.version = 2;
        payload
            .secrets
            .insert(provider.to_string(), VaultEntry::Key(secret.to_string()));
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

    /// List all service/provider names that have stored secrets.
    pub fn list(&self) -> Result<Vec<String>, VaultError> {
        let payload = self.read_payload()?.unwrap_or_default();
        Ok(payload.secrets.keys().cloned().collect())
    }

    // ── Credential API (v2) ─────────────────────────────────────────

    /// Get a specific field from a credential.
    ///
    /// - For `VaultEntry::Key(s)`: the field "key" returns the value; all other
    ///   fields return `None`.
    /// - For `VaultEntry::Credential { fields, .. }`: looks up the field by name.
    pub fn get_credential(
        &self,
        service: &str,
        field: &str,
    ) -> Result<Option<SecretString>, VaultError> {
        let payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(None),
        };
        let entry = match payload.secrets.get(service) {
            Some(e) => e,
            None => return Ok(None),
        };
        match entry {
            VaultEntry::Key(s) => {
                // Backward compat: simple keys expose themselves as "key"
                if field == "key" {
                    Ok(Some(SecretString::from(s.clone())))
                } else {
                    Ok(None)
                }
            }
            VaultEntry::Credential { fields, .. } => Ok(fields
                .get(field)
                .map(|s| SecretString::from(s.clone()))),
        }
    }

    /// Store a multi-field credential for a service.
    ///
    /// Replaces any existing entry (Key or Credential) for this service.
    pub fn set_credential(
        &self,
        service: &str,
        fields: BTreeMap<String, String>,
        service_url: Option<String>,
        category: Option<String>,
    ) -> Result<(), VaultError> {
        let mut payload = self.read_payload()?.unwrap_or_default();
        payload.version = 2;
        payload.secrets.insert(
            service.to_string(),
            VaultEntry::Credential {
                fields,
                service_url,
                category,
                created_at: Some(chrono::Utc::now().to_rfc3339()),
                expires_at: None,
            },
        );
        self.write_payload(&payload)
    }

    /// Get the raw `VaultEntry` for a service (for introspection).
    pub fn get_entry(&self, service: &str) -> Result<Option<VaultEntry>, VaultError> {
        let payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(None),
        };
        Ok(payload.secrets.get(service).cloned())
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

    // ═══════════════════════════════════════════════════════════════
    // v2: VaultEntry + Credential API
    // ═══════════════════════════════════════════════════════════════

    #[test]
    #[serial]
    fn backward_compat_key_still_works() {
        // v2 must still handle simple Key entries identically to v1
        let (_dir, vault) = test_vault();
        vault.set("anthropic", "sk-ant-test").unwrap();

        // get() returns the key value
        let s = vault.get("anthropic").unwrap().unwrap();
        assert_eq!(s.expose_secret(), "sk-ant-test");

        // get_credential with "key" field returns the value
        let s2 = vault.get_credential("anthropic", "key").unwrap().unwrap();
        assert_eq!(s2.expose_secret(), "sk-ant-test");

        // get_credential with any other field returns None
        assert!(vault
            .get_credential("anthropic", "secret")
            .unwrap()
            .is_none());

        // get_entry returns Key variant
        let entry = vault.get_entry("anthropic").unwrap().unwrap();
        assert!(matches!(entry, VaultEntry::Key(ref s) if s == "sk-ant-test"));
    }

    #[test]
    #[serial]
    fn credential_set_and_get() {
        let (_dir, vault) = test_vault();

        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live_123".to_string());
        fields.insert("secret".to_string(), "whsec_456".to_string());
        fields.insert("org_id".to_string(), "org_789".to_string());

        vault
            .set_credential(
                "stripe",
                fields,
                Some("https://api.stripe.com".to_string()),
                Some("payment".to_string()),
            )
            .unwrap();

        // get_credential retrieves individual fields
        let api_key = vault.get_credential("stripe", "api_key").unwrap().unwrap();
        assert_eq!(api_key.expose_secret(), "sk_live_123");

        let secret = vault.get_credential("stripe", "secret").unwrap().unwrap();
        assert_eq!(secret.expose_secret(), "whsec_456");

        let org_id = vault.get_credential("stripe", "org_id").unwrap().unwrap();
        assert_eq!(org_id.expose_secret(), "org_789");

        // Missing field returns None
        assert!(vault
            .get_credential("stripe", "nonexistent")
            .unwrap()
            .is_none());

        // get_entry returns Credential variant with metadata
        let entry = vault.get_entry("stripe").unwrap().unwrap();
        match entry {
            VaultEntry::Credential {
                fields,
                service_url,
                category,
                created_at,
                ..
            } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(service_url.as_deref(), Some("https://api.stripe.com"));
                assert_eq!(category.as_deref(), Some("payment"));
                assert!(created_at.is_some(), "created_at should be auto-set");
            }
            VaultEntry::Key(_) => panic!("Expected Credential, got Key"),
        }
    }

    #[test]
    #[serial]
    fn credential_get_returns_primary_for_simple_get() {
        // get() on a Credential returns the first field value (alphabetical)
        let (_dir, vault) = test_vault();

        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live_first".to_string());
        fields.insert("secret".to_string(), "whsec_second".to_string());

        vault
            .set_credential("stripe", fields, None, None)
            .unwrap();

        // get() returns the first field (alphabetical: "api_key")
        let s = vault.get("stripe").unwrap().unwrap();
        assert_eq!(s.expose_secret(), "sk_live_first");
    }

    #[test]
    #[serial]
    fn credential_list_services() {
        let (_dir, vault) = test_vault();

        // Mix of Key and Credential entries
        vault.set("anthropic", "sk-ant").unwrap();

        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live".to_string());
        vault
            .set_credential("stripe", fields, None, None)
            .unwrap();

        let mut list = vault.list().unwrap();
        list.sort();
        assert_eq!(list, vec!["anthropic", "stripe"]);
    }

    #[test]
    #[serial]
    fn credential_delete() {
        let (_dir, vault) = test_vault();

        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live".to_string());
        vault
            .set_credential("stripe", fields, None, None)
            .unwrap();

        // Credential exists
        assert!(vault.get_credential("stripe", "api_key").unwrap().is_some());

        // Delete it
        assert!(vault.delete("stripe").unwrap());

        // Gone
        assert!(vault.get_credential("stripe", "api_key").unwrap().is_none());
        assert!(vault.get("stripe").unwrap().is_none());

        // Double delete returns false
        assert!(!vault.delete("stripe").unwrap());
    }

    #[test]
    #[serial]
    fn credential_overwrite_key_with_credential() {
        let (_dir, vault) = test_vault();

        // Start with a simple Key
        vault.set("stripe", "old-key").unwrap();
        assert_eq!(
            vault.get("stripe").unwrap().unwrap().expose_secret(),
            "old-key"
        );

        // Overwrite with a Credential
        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live_new".to_string());
        vault
            .set_credential("stripe", fields, None, None)
            .unwrap();

        // Old key is gone; credential fields accessible
        let val = vault.get_credential("stripe", "api_key").unwrap().unwrap();
        assert_eq!(val.expose_secret(), "sk_live_new");
    }

    #[test]
    #[serial]
    fn credential_overwrite_credential_with_key() {
        let (_dir, vault) = test_vault();

        // Start with a Credential
        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live".to_string());
        vault
            .set_credential("stripe", fields, None, None)
            .unwrap();

        // Overwrite with a simple Key
        vault.set("stripe", "simple-key").unwrap();

        // Credential fields gone; simple key accessible
        assert_eq!(
            vault.get("stripe").unwrap().unwrap().expose_secret(),
            "simple-key"
        );
        assert!(vault
            .get_credential("stripe", "api_key")
            .unwrap()
            .is_none());
    }

    #[test]
    fn vault_entry_serde_roundtrip_key() {
        let entry = VaultEntry::Key("sk-test".to_string());
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: VaultEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn vault_entry_serde_roundtrip_credential() {
        let mut fields = BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live".to_string());
        fields.insert("secret".to_string(), "whsec_456".to_string());

        let entry = VaultEntry::Credential {
            fields,
            service_url: Some("https://api.stripe.com".to_string()),
            category: Some("payment".to_string()),
            created_at: Some("2026-03-31T12:00:00Z".to_string()),
            expires_at: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: VaultEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn vault_entry_deserialize_plain_string_as_key() {
        // Crucial for v1 compat: a bare JSON string becomes VaultEntry::Key
        let deserialized: VaultEntry = serde_json::from_str(r#""sk-ant-test""#).unwrap();
        assert_eq!(deserialized, VaultEntry::Key("sk-ant-test".to_string()));
    }

    #[test]
    #[serial]
    fn credential_nonexistent_service() {
        let (_dir, vault) = test_vault();
        assert!(vault
            .get_credential("nonexistent", "key")
            .unwrap()
            .is_none());
    }
}
