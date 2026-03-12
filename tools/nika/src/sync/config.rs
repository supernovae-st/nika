//! Sync Configuration
//!
//! Manages which editors are enabled for MCP config synchronization.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use super::editor::EditorKind;
use crate::core::paths::nika_home;
use crate::error::{NikaError, Result};
use crate::serde_yaml;

/// Sync configuration filename.
const SYNC_CONFIG_FILE: &str = "sync.yaml";

/// Sync configuration state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Editors enabled for automatic sync.
    #[serde(default)]
    pub enabled_editors: HashSet<EditorKind>,

    /// Last successful sync timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<DateTime<Utc>>,

    /// Per-editor sync state.
    #[serde(default)]
    pub editor_states: Vec<EditorSyncState>,
}

/// Per-editor sync state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSyncState {
    /// Editor kind.
    pub editor: EditorKind,
    /// Last sync timestamp for this editor.
    pub last_sync: Option<DateTime<Utc>>,
    /// Whether last sync succeeded.
    pub last_success: bool,
    /// Error message if last sync failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// Summary of sync state for display.
#[derive(Debug, Clone)]
pub struct SyncState {
    /// Total editors available.
    pub total_editors: usize,
    /// Installed editors.
    pub installed_editors: usize,
    /// Enabled editors.
    pub enabled_editors: usize,
    /// Last global sync time.
    pub last_sync: Option<DateTime<Utc>>,
    /// Per-editor details.
    pub editors: Vec<EditorSyncDetail>,
}

/// Editor sync detail for status display.
#[derive(Debug, Clone)]
pub struct EditorSyncDetail {
    /// Editor kind.
    pub kind: EditorKind,
    /// Display name.
    pub name: String,
    /// Whether installed.
    pub installed: bool,
    /// Whether enabled for sync.
    pub enabled: bool,
    /// Last sync time.
    pub last_sync: Option<DateTime<Utc>>,
    /// Last sync result.
    pub last_success: Option<bool>,
}

impl SyncConfig {
    /// Load sync config from disk.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        Self::load_from(&path)
    }

    /// Load sync config from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        // TOCTOU-safe: Attempt to read file directly instead of exists() check.
        // If file doesn't exist, return default config.
        // This avoids race where file is created/deleted between exists() and read.
        match std::fs::read_to_string(path) {
            Ok(content) => serde_yaml::from_str(&content).map_err(|e| NikaError::SyncError {
                message: format!("Failed to parse sync config: {}", e),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(NikaError::IoPathError {
                path: path.to_path_buf(),
                operation: "read sync config".to_string(),
                source: e,
            }),
        }
    }

    /// Save sync config to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        self.save_to(&path)
    }

    /// Save sync config to a specific path.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| NikaError::IoPathError {
                path: parent.to_path_buf(),
                operation: "create config directory".to_string(),
                source: e,
            })?;
        }

        let content = serde_yaml::to_string(self).map_err(|e| NikaError::SyncError {
            message: format!("Failed to serialize sync config: {}", e),
        })?;

        std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
            path: path.to_path_buf(),
            operation: "write sync config".to_string(),
            source: e,
        })
    }

    /// Get the default config path.
    pub fn config_path() -> std::path::PathBuf {
        nika_home().join(SYNC_CONFIG_FILE)
    }

    /// Enable an editor for sync.
    pub fn enable_editor(&mut self, editor: EditorKind) {
        self.enabled_editors.insert(editor);
    }

    /// Disable an editor for sync.
    pub fn disable_editor(&mut self, editor: EditorKind) {
        self.enabled_editors.remove(&editor);
    }

    /// Check if an editor is enabled.
    pub fn is_enabled(&self, editor: EditorKind) -> bool {
        self.enabled_editors.contains(&editor)
    }

    /// Update the last sync time.
    pub fn update_last_sync(&mut self) {
        self.last_sync = Some(Utc::now());
    }

    /// Record sync result for an editor.
    pub fn record_sync_result(&mut self, editor: EditorKind, success: bool, error: Option<String>) {
        // Find or create editor state
        let state = self.editor_states.iter_mut().find(|s| s.editor == editor);

        match state {
            Some(s) => {
                s.last_sync = Some(Utc::now());
                s.last_success = success;
                s.last_error = error;
            }
            None => {
                self.editor_states.push(EditorSyncState {
                    editor,
                    last_sync: Some(Utc::now()),
                    last_success: success,
                    last_error: error,
                });
            }
        }
    }

    /// Get sync state for display.
    pub fn get_state(&self) -> SyncState {
        use super::editor::Editor;

        let editors = Editor::detect_all();
        let installed_count = editors.iter().filter(|e| e.installed).count();

        let details: Vec<EditorSyncDetail> = editors
            .iter()
            .map(|e| {
                let state = self.editor_states.iter().find(|s| s.editor == e.kind);
                EditorSyncDetail {
                    kind: e.kind,
                    name: e.display_name().to_string(),
                    installed: e.installed,
                    enabled: self.is_enabled(e.kind),
                    last_sync: state.and_then(|s| s.last_sync),
                    last_success: state.map(|s| s.last_success),
                }
            })
            .collect();

        SyncState {
            total_editors: editors.len(),
            installed_editors: installed_count,
            enabled_editors: self.enabled_editors.len(),
            last_sync: self.last_sync,
            editors: details,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_sync_config_default() {
        let config = SyncConfig::default();
        assert!(config.enabled_editors.is_empty());
        assert!(config.last_sync.is_none());
        assert!(config.editor_states.is_empty());
    }

    #[test]
    fn test_enable_disable_editor() {
        let mut config = SyncConfig::default();

        // Enable
        config.enable_editor(EditorKind::ClaudeCode);
        assert!(config.is_enabled(EditorKind::ClaudeCode));
        assert!(!config.is_enabled(EditorKind::Cursor));

        // Enable another
        config.enable_editor(EditorKind::Cursor);
        assert!(config.is_enabled(EditorKind::ClaudeCode));
        assert!(config.is_enabled(EditorKind::Cursor));

        // Disable
        config.disable_editor(EditorKind::ClaudeCode);
        assert!(!config.is_enabled(EditorKind::ClaudeCode));
        assert!(config.is_enabled(EditorKind::Cursor));
    }

    #[test]
    fn test_update_last_sync() {
        let mut config = SyncConfig::default();
        assert!(config.last_sync.is_none());

        config.update_last_sync();
        assert!(config.last_sync.is_some());

        // Verify it's recent (within last second)
        let now = Utc::now();
        let diff = now - config.last_sync.unwrap();
        assert!(diff.num_seconds() < 2);
    }

    #[test]
    fn test_record_sync_result() {
        let mut config = SyncConfig::default();

        // Record success
        config.record_sync_result(EditorKind::ClaudeCode, true, None);
        assert_eq!(config.editor_states.len(), 1);

        let state = &config.editor_states[0];
        assert_eq!(state.editor, EditorKind::ClaudeCode);
        assert!(state.last_success);
        assert!(state.last_error.is_none());

        // Record failure
        config.record_sync_result(
            EditorKind::ClaudeCode,
            false,
            Some("Test error".to_string()),
        );
        assert_eq!(config.editor_states.len(), 1); // Updated, not added

        let state = &config.editor_states[0];
        assert!(!state.last_success);
        assert_eq!(state.last_error.as_ref().unwrap(), "Test error");
    }

    #[test]
    fn test_save_and_load() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("sync.yaml");

        let mut config = SyncConfig::default();
        config.enable_editor(EditorKind::ClaudeCode);
        config.enable_editor(EditorKind::Cursor);
        config.update_last_sync();
        config.record_sync_result(EditorKind::ClaudeCode, true, None);

        // Save
        config.save_to(&path).unwrap();
        assert!(path.exists());

        // Load
        let loaded = SyncConfig::load_from(&path).unwrap();
        assert!(loaded.is_enabled(EditorKind::ClaudeCode));
        assert!(loaded.is_enabled(EditorKind::Cursor));
        assert!(!loaded.is_enabled(EditorKind::Windsurf));
        assert!(loaded.last_sync.is_some());
        assert_eq!(loaded.editor_states.len(), 1);
    }

    #[test]
    fn test_load_nonexistent() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("nonexistent.yaml");

        let config = SyncConfig::load_from(&path).unwrap();
        assert!(config.enabled_editors.is_empty());
    }

    #[test]
    fn test_get_state() {
        let mut config = SyncConfig::default();
        config.enable_editor(EditorKind::ClaudeCode);

        let state = config.get_state();
        assert_eq!(state.total_editors, 4);
        assert_eq!(state.enabled_editors, 1);
        assert_eq!(state.editors.len(), 4);

        // Find Claude Code in details
        let claude = state
            .editors
            .iter()
            .find(|e| e.kind == EditorKind::ClaudeCode);
        assert!(claude.is_some());
        assert!(claude.unwrap().enabled);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut config = SyncConfig::default();
        config.enable_editor(EditorKind::ClaudeCode);
        config.update_last_sync();
        config.record_sync_result(EditorKind::ClaudeCode, true, None);

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: SyncConfig = serde_yaml::from_str(&yaml).unwrap();

        assert!(parsed.is_enabled(EditorKind::ClaudeCode));
        assert!(parsed.last_sync.is_some());
        assert_eq!(parsed.editor_states.len(), 1);
    }
}
