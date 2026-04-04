//! NikaVault — re-exported from nika-core.
//!
//! The vault implementation lives in nika-core so both nika-engine and nika-daemon
//! can use it without circular dependencies.

pub use nika_vault::{
    AuditEntry, DopplerBackend, NikaVault, VaultAuditLog, VaultBackend, VaultEntry, VaultError,
};
