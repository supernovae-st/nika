//! Keyring re-exports from `secrets::keyring` (canonical home since v0.28).
//!
//! This module existed here originally but was moved to `secrets::keyring`
//! to break the circular dependency: secrets → tui.
//! All types are re-exported for backward compatibility within TUI code.

pub use crate::secrets::keyring::{
    mask_api_key, migrate_env_to_keyring, validate_key_format, KeyringError, MigrationReport,
    NikaKeyring,
};
