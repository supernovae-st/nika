//! Sync Implementation
//!
//! Handles the actual synchronization of MCP config to editors.

use serde_json::{json, Value};
use std::path::Path;

use super::config::SyncConfig;
use super::editor::{Editor, EditorKind};
use crate::core::paths::{nika_home, MCP_CONFIG};
use crate::error::{NikaError, Result};
use crate::serde_yaml;

/// Result of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Editor that was synced.
    pub editor: EditorKind,
    /// Whether sync succeeded.
    pub success: bool,
    /// Error message if failed.
    pub error: Option<String>,
    /// Number of servers synced.
    pub servers_synced: usize,
}

/// Sync MCP configuration to all enabled editors.
pub fn sync_all(config: &mut SyncConfig) -> Result<Vec<SyncResult>> {
    let mcp_config = load_mcp_config()?;
    let mut results = Vec::new();

    for editor_kind in config.enabled_editors.clone() {
        let editor = Editor::detect(editor_kind);

        if !editor.installed {
            let result = SyncResult {
                editor: editor_kind,
                success: false,
                error: Some(format!("{} is not installed", editor.display_name())),
                servers_synced: 0,
            };
            config.record_sync_result(editor_kind, false, result.error.clone());
            results.push(result);
            continue;
        }

        let result = match sync_to_editor(&editor, &mcp_config) {
            Ok(count) => {
                config.record_sync_result(editor_kind, true, None);
                SyncResult {
                    editor: editor_kind,
                    success: true,
                    error: None,
                    servers_synced: count,
                }
            }
            Err(e) => {
                let error_msg = e.to_string();
                config.record_sync_result(editor_kind, false, Some(error_msg.clone()));
                SyncResult {
                    editor: editor_kind,
                    success: false,
                    error: Some(error_msg),
                    servers_synced: 0,
                }
            }
        };
        results.push(result);
    }

    if results.iter().any(|r| r.success) {
        config.update_last_sync();
    }

    config.save()?;
    Ok(results)
}

/// Sync MCP configuration to a specific editor.
pub fn sync_editor(editor_kind: EditorKind) -> Result<SyncResult> {
    let editor = Editor::detect(editor_kind);

    if !editor.installed {
        return Ok(SyncResult {
            editor: editor_kind,
            success: false,
            error: Some(format!("{} is not installed", editor.display_name())),
            servers_synced: 0,
        });
    }

    let mcp_config = load_mcp_config()?;
    match sync_to_editor(&editor, &mcp_config) {
        Ok(count) => Ok(SyncResult {
            editor: editor_kind,
            success: true,
            error: None,
            servers_synced: count,
        }),
        Err(e) => Ok(SyncResult {
            editor: editor_kind,
            success: false,
            error: Some(e.to_string()),
            servers_synced: 0,
        }),
    }
}

/// Load MCP config from ~/.nika/mcp.yaml
fn load_mcp_config() -> Result<Value> {
    let path = nika_home().join(MCP_CONFIG);

    if !path.exists() {
        // Return empty servers object if no MCP config
        return Ok(json!({ "servers": {} }));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| NikaError::IoPathError {
        path: path.clone(),
        operation: "read MCP config".to_string(),
        source: e,
    })?;

    // Parse YAML to JSON value
    let yaml_value: Value = serde_yaml::from_str(&content).map_err(|e| NikaError::SyncError {
        message: format!("Failed to parse MCP config: {}", e),
    })?;

    Ok(yaml_value)
}

/// Sync MCP config to a specific editor.
fn sync_to_editor(editor: &Editor, mcp_config: &Value) -> Result<usize> {
    // Ensure editor config directory exists
    if !editor.config_dir.exists() {
        std::fs::create_dir_all(&editor.config_dir).map_err(|e| NikaError::IoPathError {
            path: editor.config_dir.clone(),
            operation: "create editor config directory".to_string(),
            source: e,
        })?;
    }

    // Get servers from MCP config
    let servers = mcp_config
        .get("servers")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let server_count = servers.as_object().map(|o| o.len()).unwrap_or(0);

    // Write to editor config
    match editor.kind {
        EditorKind::ClaudeCode => write_claude_code_config(&editor.mcp_config_path, &servers)?,
        EditorKind::Cursor | EditorKind::Windsurf | EditorKind::VsCode => {
            write_mcp_json(&editor.mcp_config_path, &servers)?
        }
    }

    Ok(server_count)
}

/// Write Claude Code settings.json (merges with existing)
fn write_claude_code_config(path: &Path, servers: &Value) -> Result<()> {
    // Load existing config or create new
    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| NikaError::IoPathError {
            path: path.to_path_buf(),
            operation: "read Claude Code settings".to_string(),
            source: e,
        })?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Merge MCP servers under mcpServers key
    config["mcpServers"] = servers.clone();

    // Write back with pretty formatting
    let content = serde_json::to_string_pretty(&config).map_err(|e| NikaError::SyncError {
        message: format!("Failed to serialize Claude Code settings: {}", e),
    })?;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.to_path_buf(),
        operation: "write Claude Code settings".to_string(),
        source: e,
    })
}

/// Write standalone mcp.json file
fn write_mcp_json(path: &Path, servers: &Value) -> Result<()> {
    let config = json!({
        "mcpServers": servers
    });

    let content = serde_json::to_string_pretty(&config).map_err(|e| NikaError::SyncError {
        message: format!("Failed to serialize MCP config: {}", e),
    })?;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.to_path_buf(),
        operation: "write MCP config".to_string(),
        source: e,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_mcp_config_nonexistent() {
        // Should return empty servers when no config exists
        // This test relies on NIKA_HOME not being set to a path with mcp.yaml
        // Just verify the function doesn't panic
        let _ = load_mcp_config();
    }

    #[test]
    fn test_write_mcp_json() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("mcp.json");

        let servers = json!({
            "neo4j": {
                "command": "npx",
                "args": ["-y", "@neo4j/mcp-neo4j"]
            },
            "perplexity": {
                "command": "node",
                "args": ["server.js"]
            }
        });

        write_mcp_json(&path, &servers).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed.get("mcpServers").is_some());
        assert!(parsed["mcpServers"].get("neo4j").is_some());
        assert!(parsed["mcpServers"].get("perplexity").is_some());
    }

    #[test]
    fn test_write_claude_code_config_new() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("settings.json");

        let servers = json!({
            "neo4j": {
                "command": "npx",
                "args": ["-y", "@neo4j/mcp-neo4j"]
            }
        });

        write_claude_code_config(&path, &servers).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed.get("mcpServers").is_some());
        assert!(parsed["mcpServers"].get("neo4j").is_some());
    }

    #[test]
    fn test_write_claude_code_config_merge() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("settings.json");

        // Write initial config with other settings
        let initial = json!({
            "theme": "dark",
            "fontSize": 14,
            "mcpServers": {
                "old_server": {"command": "old"}
            }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        // Sync with new servers
        let servers = json!({
            "neo4j": {
                "command": "npx",
                "args": ["-y", "@neo4j/mcp-neo4j"]
            }
        });

        write_claude_code_config(&path, &servers).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Other settings preserved
        assert_eq!(parsed["theme"], "dark");
        assert_eq!(parsed["fontSize"], 14);

        // MCP servers replaced (not merged)
        assert!(parsed["mcpServers"].get("neo4j").is_some());
        // Old server should be gone (we replace, not merge)
        assert!(parsed["mcpServers"].get("old_server").is_none());
    }

    #[test]
    fn test_sync_result() {
        let result = SyncResult {
            editor: EditorKind::ClaudeCode,
            success: true,
            error: None,
            servers_synced: 3,
        };

        assert!(result.success);
        assert_eq!(result.servers_synced, 3);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_sync_editor_not_installed() {
        // Use a known non-existent editor path
        let result = sync_editor(EditorKind::Windsurf);
        assert!(result.is_ok());

        // Result depends on whether Windsurf is actually installed
        // We just verify it doesn't panic
    }
}
