//! Sync Module — Editor MCP Configuration Synchronization
//!
//! Syncs MCP server configuration from `~/.nika/mcp.yaml` to supported editors.
//!
//! ## Supported Editors
//!
//! | Editor | Config Path | Format |
//! |--------|-------------|--------|
//! | Claude Code | `~/.claude/settings.json` | JSON (mcp.servers) |
//! | Cursor | `~/.cursor/mcp.json` | JSON |
//! | Windsurf | `~/.windsurf/mcp.json` | JSON |
//! | VS Code | `~/.vscode/mcp.json` | JSON |
//!
//! ## Usage
//!
//! ```bash
//! # Sync to all enabled editors
//! nika sync
//!
//! # Check sync status
//! nika sync --status
//!
//! # Enable editor for sync
//! nika sync --enable claude-code
//!
//! # Disable editor for sync
//! nika sync --disable cursor
//! ```
//!
//! ## Configuration
//!
//! Sync settings are stored in `~/.nika/sync.yaml`:
//!
//! ```yaml
//! enabled_editors:
//!   - claude-code
//!   - cursor
//! last_sync: 2024-01-15T10:30:00Z
//! ```

mod config;
mod editor;
mod operations;

pub use config::{SyncConfig, SyncState};
pub use editor::{Editor, EditorKind};
pub use operations::{sync_all, sync_editor, SyncResult};

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        // Verify all public types are accessible
        let _ = std::mem::size_of::<SyncConfig>();
        let _ = std::mem::size_of::<Editor>();
        let _ = std::mem::size_of::<EditorKind>();
    }
}
