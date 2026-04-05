//! NikaVault — re-exported from nika-core.
//!
//! The vault implementation lives in nika-core so both nika-engine and nika-daemon
//! can use it without circular dependencies.

pub use nika_vault::{
    AuditEntry, DopplerBackend, NikaVault, VaultAuditLog, VaultBackend, VaultEntry, VaultError,
};

/// Try to open the default NikaVault from `~/.nika/secrets/`.
///
/// Returns `Some(vault)` if the secrets directory can be resolved.
/// Returns `None` if home directory detection fails (graceful fallback).
/// This never panics or errors -- vault availability is optional.
pub fn try_open_vault() -> Option<NikaVault> {
    let nika_home = nika_home_path()?;
    Some(NikaVault::new(&nika_home.join("secrets")))
}

/// Resolve `~/.nika` path using the same logic as `fallback.rs`.
fn nika_home_path() -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        Some(
            nika_daemon::daemon_dir()
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| {
                    dirs::home_dir()
                        .unwrap_or_else(std::env::temp_dir)
                        .join(".nika")
                }),
        )
    }
    #[cfg(not(unix))]
    {
        dirs::home_dir().map(|h| h.join(".nika"))
    }
}
