//! Backup Module — SuperNovae Data Backup
//!
//! Creates and restores backups of Nika configuration and data.
//!
//! ## Commands
//!
//! - `nika backup create` — Create unified backup
//! - `nika backup list` — List available backups
//! - `nika backup restore` — Restore from backup
//! - `nika backup prune` — Clean up old backups
//!
//! ## Backup Contents
//!
//! - `~/.nika/config.toml` — Configuration
//! - `~/.nika/mcp.yaml` — MCP server configuration
//! - `~/.nika/sync.yaml` — Editor sync configuration
//! - `~/.nika/sessions/` — Session data

mod operations;

pub use operations::{
    create_backup, list_backups, prune_backups, restore_backup, BackupInfo, BackupOptions,
};

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _ = std::mem::size_of::<BackupInfo>();
        let _ = std::mem::size_of::<BackupOptions>();
    }
}
