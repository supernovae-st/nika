// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
        /// Optional service URL (e.g., `https://api.stripe.com`)
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
            VaultEntry::Credential { fields, .. } => fields.values().next().map(|s| s.as_str()),
        }
    }
}

/// Which backend to use for secret storage/retrieval at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultBackend {
    /// Local encrypted vault (default).
    Local,
    /// Doppler CLI-based secret management.
    Doppler,
}

impl VaultBackend {
    /// Select backend from `NIKA_VAULT_BACKEND` env var.
    ///
    /// Returns `Doppler` if `NIKA_VAULT_BACKEND=doppler`, otherwise `Local`.
    pub fn from_env() -> Self {
        match std::env::var("NIKA_VAULT_BACKEND").as_deref() {
            Ok("doppler") => Self::Doppler,
            _ => Self::Local,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AUDIT LOG
// ═══════════════════════════════════════════════════════════════════════════

/// Audit log entry for vault operations.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AuditEntry {
    pub timestamp: String,
    pub op: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub source: String,
}

/// Append-only audit log for credential access tracking.
///
/// Writes JSON lines to `<secrets_dir>/audit.jsonl`.
pub struct VaultAuditLog {
    log_path: PathBuf,
}

impl VaultAuditLog {
    /// Create audit log for the given secrets directory.
    pub fn new(secrets_dir: &Path) -> Self {
        Self {
            log_path: secrets_dir.join("audit.jsonl"),
        }
    }

    /// Log path for inspection in tests.
    pub fn path(&self) -> &Path {
        &self.log_path
    }

    /// Append a single audit entry.
    pub fn log(
        &self,
        op: &str,
        service: &str,
        field: Option<&str>,
        source: &str,
    ) -> Result<(), VaultError> {
        let entry = AuditEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            op: op.to_string(),
            service: service.to_string(),
            field: field.map(|f| f.to_string()),
            source: source.to_string(),
        };

        let mut line = serde_json::to_string(&entry)?;
        line.push('\n');

        if let Some(parent) = self.log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        use std::io::Write;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        let mut writer = std::io::BufWriter::new(file);
        writer.write_all(line.as_bytes())?;
        writer.flush()?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&self.log_path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    /// Read all audit entries from the log.
    pub fn read_all(&self) -> Result<Vec<AuditEntry>, VaultError> {
        if !self.log_path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&self.log_path)?;
        let mut entries = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(line)?;
            entries.push(entry);
        }
        Ok(entries)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DOPPLER BACKEND
// ═══════════════════════════════════════════════════════════════════════════

/// Doppler CLI backend — delegates to `doppler secrets get/--json`.
pub struct DopplerBackend;

impl DopplerBackend {
    /// Get a single secret by key via `doppler secrets get KEY --plain`.
    pub fn get(key: &str) -> Result<Option<String>, VaultError> {
        let output = std::process::Command::new("doppler")
            .args(["secrets", "get", key, "--plain"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if value.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(value))
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::debug!("doppler get failed for {key}: {stderr}");
                Ok(None)
            }
            Err(e) => {
                tracing::debug!("doppler CLI not available: {e}");
                Ok(None)
            }
        }
    }

    /// List all secret keys via `doppler secrets --json`.
    pub fn list() -> Result<Vec<String>, VaultError> {
        let output = std::process::Command::new("doppler")
            .args(["secrets", "--json"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let parsed: serde_json::Value =
                    serde_json::from_slice(&out.stdout).map_err(VaultError::Json)?;
                if let serde_json::Value::Object(map) = parsed {
                    Ok(map.keys().cloned().collect())
                } else {
                    Ok(vec![])
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::debug!("doppler list failed: {stderr}");
                Ok(vec![])
            }
            Err(e) => {
                tracing::debug!("doppler CLI not available: {e}");
                Ok(vec![])
            }
        }
    }

    /// Check if the doppler CLI is available on PATH.
    pub fn is_available() -> bool {
        std::process::Command::new("doppler")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
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

impl Drop for VaultPayload {
    fn drop(&mut self) {
        // Zeroize secret values on drop to minimize plaintext exposure in memory.
        // BTreeMap itself can't be zeroized, but we clear the entries.
        for entry in self.secrets.values_mut() {
            match entry {
                VaultEntry::Key(ref mut s) => zeroize::Zeroize::zeroize(s),
                VaultEntry::Credential {
                    ref mut fields,
                    ref mut service_url,
                    ..
                } => {
                    for v in fields.values_mut() {
                        zeroize::Zeroize::zeroize(v);
                    }
                    if let Some(ref mut url) = service_url {
                        zeroize::Zeroize::zeroize(url);
                    }
                }
            }
        }
        self.secrets.clear();
    }
}

/// Encrypted local file store for API secrets.
pub struct NikaVault {
    vault_path: PathBuf,
    salt_path: PathBuf,
    /// Optional audit log for credential access tracking.
    audit: Option<VaultAuditLog>,
}

impl NikaVault {
    /// Create a new vault pointed at the given secrets directory.
    ///
    /// Does NOT create files — they are created lazily on first `set()`.
    /// Initializes an audit log in the same directory for access tracking.
    pub fn new(secrets_dir: &Path) -> Self {
        let audit = Some(VaultAuditLog::new(secrets_dir));
        Self {
            vault_path: secrets_dir.join("vault.enc"),
            salt_path: secrets_dir.join("vault.salt"),
            audit,
        }
    }

    /// Get a secret by provider name.
    ///
    /// For `VaultEntry::Key(s)`, returns the key string.
    /// For `VaultEntry::Credential { fields, .. }`, returns the first field value.
    pub fn get(&self, provider: &str) -> Result<Option<SecretString>, VaultError> {
        if let Some(ref audit) = self.audit {
            let _ = audit.log("get", provider, None, "runtime");
        }
        let payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(None),
        };
        Ok(payload.secrets.get(provider).and_then(|entry| {
            entry
                .primary_value()
                .map(|s| SecretString::from(s.to_owned()))
        }))
    }

    /// Store a simple secret for a provider (creates or updates).
    pub fn set(&self, provider: &str, secret: &str) -> Result<(), VaultError> {
        let provider = provider.to_string();
        let secret = secret.to_string();
        self.with_vault_lock(|vault| {
            let mut payload = vault.read_payload()?.unwrap_or_default();
            payload.version = 2;
            payload
                .secrets
                .insert(provider.clone(), VaultEntry::Key(secret.clone()));
            vault.write_payload(&payload)?;
            if let Some(ref audit) = vault.audit {
                let _ = audit.log("set", &provider, None, "cli");
            }
            Ok(())
        })
    }

    /// Delete a secret. Returns true if it existed.
    pub fn delete(&self, provider: &str) -> Result<bool, VaultError> {
        let provider = provider.to_string();
        self.with_vault_lock(|vault| {
            let mut payload = match vault.read_payload()? {
                Some(p) => p,
                None => return Ok(false),
            };
            let existed = payload.secrets.remove(&provider).is_some();
            if existed {
                vault.write_payload(&payload)?;
                if let Some(ref audit) = vault.audit {
                    let _ = audit.log("delete", &provider, None, "cli");
                }
            }
            Ok(existed)
        })
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
        if let Some(ref audit) = self.audit {
            let _ = audit.log("get_credential", service, Some(field), "runtime");
        }
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
            VaultEntry::Credential { fields, .. } => {
                Ok(fields.get(field).map(|s| SecretString::from(s.clone())))
            }
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
        let service = service.to_string();
        self.with_vault_lock(|vault| {
            let mut payload = vault.read_payload()?.unwrap_or_default();
            payload.version = 2;
            payload.secrets.insert(
                service.clone(),
                VaultEntry::Credential {
                    fields: fields.clone(),
                    service_url: service_url.clone(),
                    category: category.clone(),
                    created_at: Some(chrono::Utc::now().to_rfc3339()),
                    expires_at: None,
                },
            );
            vault.write_payload(&payload)?;
            if let Some(ref audit) = vault.audit {
                let _ = audit.log("set_credential", &service, None, "cli");
            }
            Ok(())
        })
    }

    /// Get the raw `VaultEntry` for a service (for introspection).
    pub fn get_entry(&self, service: &str) -> Result<Option<VaultEntry>, VaultError> {
        let payload = match self.read_payload()? {
            Some(p) => p,
            None => return Ok(None),
        };
        Ok(payload.secrets.get(service).cloned())
    }

    /// Check if vault.enc file exists on disk.
    pub fn exists(&self) -> bool {
        self.vault_path.exists()
    }

    /// Check if the vault exists and is readable with the current key.
    ///
    /// Returns:
    /// - `Ok(true)` — vault exists and decrypts successfully
    /// - `Ok(false)` — no vault file (nothing stored yet)
    /// - `Err(VaultError::Crypto(...))` — vault exists but can't be decrypted
    pub fn health_check(&self) -> Result<bool, VaultError> {
        if !self.vault_path.exists() {
            return Ok(false);
        }
        // Try to read/decrypt — if it fails, the key is wrong
        self.read_payload()?;
        Ok(true)
    }

    /// Delete vault files and start fresh.
    ///
    /// Removes `vault.enc`, `vault.salt`, and `audit.jsonl`.
    /// The next `set()` call will create a new vault with the current key.
    pub fn reset(&self) -> Result<(), VaultError> {
        for path in [&self.vault_path, &self.salt_path] {
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }
        // Also remove audit log for a clean start
        if let Some(parent) = self.vault_path.parent() {
            let audit_path = parent.join("audit.jsonl");
            if audit_path.exists() {
                let _ = std::fs::remove_file(audit_path);
            }
        }
        Ok(())
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Execute a closure with exclusive file lock on the vault.
    fn with_vault_lock<F, T>(&self, f: F) -> Result<T, VaultError>
    where
        F: FnOnce(&Self) -> Result<T, VaultError>,
    {
        use fs2::FileExt;

        // Ensure parent dir exists
        if let Some(parent) = self.vault_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }

        let lock_path = self.vault_path.with_extension("lock");
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_path)?;
        lock_file.lock_exclusive()?;

        let result = f(self);

        // Drop releases the lock (fs2 unlocks on close)
        drop(lock_file);
        result
    }

    fn read_payload(&self) -> Result<Option<VaultPayload>, VaultError> {
        if !self.vault_path.exists() {
            return Ok(None);
        }
        let ciphertext = std::fs::read(&self.vault_path)?;
        let key = self.derive_key()?;
        let mut plaintext = aead::open(&key, &ciphertext).map_err(|e| {
            VaultError::Crypto(format!(
                "decrypt failed: {e}. \
                 The vault was created with a different passphrase or machine. \
                 Fix: (1) set NIKA_VAULT_PASSPHRASE to the original passphrase, \
                 (2) delete ~/.nika/secrets/vault.enc to start fresh, or \
                 (3) use env vars (e.g. ANTHROPIC_API_KEY) instead."
            ))
        })?;
        let payload: VaultPayload = serde_json::from_slice(&plaintext)?;
        // Zeroize decrypted bytes immediately after deserialization
        zeroize::Zeroize::zeroize(&mut plaintext);
        Ok(Some(payload))
    }

    fn write_payload(&self, payload: &VaultPayload) -> Result<(), VaultError> {
        if let Some(parent) = self.vault_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let plaintext = serde_json::to_vec(payload)?;
        let key = self.derive_key()?;
        let ciphertext = aead::seal(&key, &plaintext)
            .map_err(|e| VaultError::Crypto(format!("encrypt failed: {e}")))?;

        // Atomic write: write to tmp file then rename, so a concurrent read
        // never sees a truncated/partial vault file.
        let tmp_path = self.vault_path.with_extension("enc.tmp");
        std::fs::write(&tmp_path, &ciphertext)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
        }

        if let Err(e) = std::fs::rename(&tmp_path, &self.vault_path) {
            // Clean up tmp file on rename failure
            let _ = std::fs::remove_file(&tmp_path);
            return Err(VaultError::Io(e));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if self.salt_path.exists() {
                let perms = std::fs::Permissions::from_mode(0o600);
                std::fs::set_permissions(&self.salt_path, perms)?;
            }
        }

        debug!("vault written: {} providers", payload.secrets.len());
        Ok(())
    }

    /// KDF memory parameter in KiB: 64 MiB (1 << 16 KiB).
    ///
    /// OWASP recommends Argon2id with 19 MiB minimum. Our 64 MiB with 6 iterations
    /// exceeds that by 3x. Do NOT change this without migration logic — existing
    /// vaults are encrypted with this key derivation.
    ///
    /// Note: orion's `kdf::derive_key` memory parameter is in KiB (kibibytes),
    /// so `1 << 16` = 65536 KiB = 64 MiB (NOT 64 KB).
    const KDF_MEMORY_KIB: u32 = 1 << 16;

    /// KDF iteration count (Argon2i time cost).
    const KDF_ITERATIONS: u32 = 6;

    fn derive_key(&self) -> Result<orion::aead::SecretKey, VaultError> {
        let salt = self.load_or_create_salt()?;
        let fingerprint = machine_fingerprint()?;

        let password = kdf::Password::from_slice(fingerprint.as_bytes())
            .map_err(|e| VaultError::Crypto(format!("KDF password: {e}")))?;
        let kdf_salt = kdf::Salt::from_slice(&salt)
            .map_err(|e| VaultError::Crypto(format!("KDF salt: {e}")))?;

        let derived = kdf::derive_key(
            &password,
            &kdf_salt,
            Self::KDF_ITERATIONS,
            Self::KDF_MEMORY_KIB,
            32,
        )
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let mut salt = vec![0u8; 16];
        orion::util::secure_rand_bytes(&mut salt)
            .map_err(|e| VaultError::Crypto(format!("CSPRNG: {e}")))?;

        // Atomic write for salt file too
        let tmp_salt = self.salt_path.with_extension("salt.tmp");
        std::fs::write(&tmp_salt, &salt)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp_salt, std::fs::Permissions::from_mode(0o600))?;
        }

        if let Err(e) = std::fs::rename(&tmp_salt, &self.salt_path) {
            let _ = std::fs::remove_file(&tmp_salt);
            return Err(VaultError::Io(e));
        }

        debug!("vault salt created");
        Ok(salt)
    }
}

fn machine_fingerprint() -> Result<String, VaultError> {
    if let Ok(pass) = std::env::var("NIKA_VAULT_PASSPHRASE") {
        if !pass.is_empty() {
            if pass.len() < 12 {
                tracing::warn!(
                    "NIKA_VAULT_PASSPHRASE is short ({} chars) — recommend 12+ for security",
                    pass.len()
                );
            }
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
    // SAFETY: expose_secret() is used throughout this test module to verify vault
    // roundtrip behavior (set → get → compare). All secret values are test fixtures
    // ("sk-ant-test", "sk_live_123", etc.) — never real credentials.
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

        vault.set_credential("stripe", fields, None, None).unwrap();

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
        vault.set_credential("stripe", fields, None, None).unwrap();

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
        vault.set_credential("stripe", fields, None, None).unwrap();

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
        vault.set_credential("stripe", fields, None, None).unwrap();

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
        vault.set_credential("stripe", fields, None, None).unwrap();

        // Overwrite with a simple Key
        vault.set("stripe", "simple-key").unwrap();

        // Credential fields gone; simple key accessible
        assert_eq!(
            vault.get("stripe").unwrap().unwrap().expose_secret(),
            "simple-key"
        );
        assert!(vault.get_credential("stripe", "api_key").unwrap().is_none());
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

    // ── VaultBackend tests ──────────────────────────────────────────────

    #[test]
    #[serial]
    fn local_backend_is_default() {
        // Ensure env var is unset
        unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") };
        assert_eq!(VaultBackend::from_env(), VaultBackend::Local);
    }

    #[test]
    #[serial]
    fn doppler_backend_selected_from_env() {
        std::env::set_var("NIKA_VAULT_BACKEND", "doppler");
        assert_eq!(VaultBackend::from_env(), VaultBackend::Doppler);
        unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") };
    }

    #[test]
    #[serial]
    fn unknown_backend_defaults_to_local() {
        std::env::set_var("NIKA_VAULT_BACKEND", "unknown-backend");
        assert_eq!(VaultBackend::from_env(), VaultBackend::Local);
        unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") };
    }

    #[test]
    #[serial]
    fn empty_backend_defaults_to_local() {
        std::env::set_var("NIKA_VAULT_BACKEND", "");
        assert_eq!(VaultBackend::from_env(), VaultBackend::Local);
        unsafe { std::env::remove_var("NIKA_VAULT_BACKEND") };
    }

    #[test]
    fn doppler_get_returns_none_when_cli_unavailable() {
        // On most CI/test envs, doppler is not installed, so this tests the fallback
        // If doppler IS installed, this still works — it returns whatever doppler has
        let result = DopplerBackend::get("NONEXISTENT_KEY_12345");
        assert!(result.is_ok(), "get should not error even without doppler");
    }

    #[test]
    fn doppler_list_returns_empty_when_cli_unavailable() {
        // If doppler is not on PATH, should gracefully return empty
        // If it IS installed, returns actual keys (still valid)
        let result = DopplerBackend::list();
        assert!(result.is_ok(), "list should not error even without doppler");
    }

    // ── Audit log tests ───────────────────────────────────────────────

    #[test]
    fn audit_log_writes_and_reads() {
        let dir = TempDir::new().unwrap();
        let audit = VaultAuditLog::new(dir.path());

        audit
            .log("get", "stripe", Some("secret"), "workflow")
            .unwrap();
        audit.log("set", "twilio", Some("sid"), "cli").unwrap();
        audit.log("delete", "old-service", None, "cli").unwrap();

        let entries = audit.read_all().unwrap();
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].op, "get");
        assert_eq!(entries[0].service, "stripe");
        assert_eq!(entries[0].field.as_deref(), Some("secret"));
        assert_eq!(entries[0].source, "workflow");

        assert_eq!(entries[1].op, "set");
        assert_eq!(entries[1].service, "twilio");
        assert_eq!(entries[1].field.as_deref(), Some("sid"));

        assert_eq!(entries[2].op, "delete");
        assert_eq!(entries[2].service, "old-service");
        assert!(entries[2].field.is_none());
    }

    #[test]
    fn audit_log_timestamp_is_rfc3339() {
        let dir = TempDir::new().unwrap();
        let audit = VaultAuditLog::new(dir.path());

        audit.log("get", "test", None, "test").unwrap();

        let entries = audit.read_all().unwrap();
        assert_eq!(entries.len(), 1);
        // Verify it parses as RFC 3339
        assert!(
            chrono::DateTime::parse_from_rfc3339(&entries[0].timestamp).is_ok(),
            "timestamp should be valid RFC 3339: {}",
            entries[0].timestamp
        );
    }

    #[test]
    fn audit_log_empty_file() {
        let dir = TempDir::new().unwrap();
        let audit = VaultAuditLog::new(dir.path());

        // No file yet — should return empty vec
        let entries = audit.read_all().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn audit_log_append_mode() {
        let dir = TempDir::new().unwrap();
        let audit = VaultAuditLog::new(dir.path());

        audit.log("get", "s1", None, "src1").unwrap();
        audit.log("set", "s2", None, "src2").unwrap();

        // Create a new audit log instance pointing to the same file
        let audit2 = VaultAuditLog::new(dir.path());
        audit2.log("delete", "s3", None, "src3").unwrap();

        // All 3 entries should be present
        let entries = audit2.read_all().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].service, "s1");
        assert_eq!(entries[1].service, "s2");
        assert_eq!(entries[2].service, "s3");
    }

    #[test]
    fn audit_entry_json_roundtrip() {
        let entry = AuditEntry {
            timestamp: "2026-04-01T00:00:00+00:00".to_string(),
            op: "get".to_string(),
            service: "stripe".to_string(),
            field: Some("secret".to_string()),
            source: "workflow".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, parsed);
    }

    #[test]
    fn audit_entry_skips_none_field() {
        let entry = AuditEntry {
            timestamp: "2026-04-01T00:00:00+00:00".to_string(),
            op: "list".to_string(),
            service: "all".to_string(),
            field: None,
            source: "cli".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(
            !json.contains("field"),
            "field should be skipped when None: {json}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn audit_log_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let audit = VaultAuditLog::new(dir.path());

        audit.log("get", "test", None, "test").unwrap();

        let perms = std::fs::metadata(audit.path()).unwrap().permissions();
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn vault_backend_clone_and_debug() {
        let b = VaultBackend::Local;
        let b2 = b.clone();
        assert_eq!(b, b2);
        assert_eq!(format!("{:?}", b), "Local");
        assert_eq!(format!("{:?}", VaultBackend::Doppler), "Doppler");
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn vault_secrets_dir_permissions_are_700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let secrets = dir.path().join("secrets");
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let vault = NikaVault::new(&secrets);
        vault.set("test", "key123").unwrap();
        let mode = std::fs::metadata(&secrets).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "secrets dir must be 0o700, got {mode:o}");
    }

    #[test]
    #[serial]
    fn kdf_params_above_owasp_minimum() {
        // Verify KDF constants: 64 MiB memory, 6 iterations
        // OWASP recommends: Argon2id, 19 MiB, 2 iterations
        // Our params exceed that: Argon2i, 64 MiB, 6 iterations
        assert_eq!(NikaVault::KDF_MEMORY_KIB, 1 << 16); // 64 MiB in KiB
        assert_eq!(NikaVault::KDF_ITERATIONS, 6);

        // Verify vault roundtrip works with documented params
        let (_dir, vault) = test_vault();
        vault.set("test_kdf", "secret_value").unwrap();
        let val = vault.get("test_kdf").unwrap().unwrap();
        assert_eq!(val.expose_secret(), "secret_value");
    }

    #[test]
    fn vault_concurrent_writes_dont_lose_data() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = dir.path().join("secrets");

        let handles: Vec<_> = (0..10)
            .map(|i| {
                let s = secrets.clone();
                std::thread::spawn(move || {
                    std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
                    let v = NikaVault::new(&s);
                    v.set(&format!("prov_{i}"), &format!("key_{i}")).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let vault = NikaVault::new(&secrets);
        let all = vault.list().unwrap();
        assert_eq!(
            all.len(),
            10,
            "all concurrent writes must survive: got {}",
            all.len()
        );
    }
}
