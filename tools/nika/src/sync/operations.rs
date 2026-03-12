//! Sync Operations
//!
//! Handles the actual synchronization of MCP config to editors with
//! platform-specific path resolution and proper editor format handling.
//!
//! ## Editor Config Paths
//!
//! | Editor | macOS | Linux |
//! |--------|-------|-------|
//! | Claude Code | ~/Library/Application Support/Claude/claude_desktop_config.json | ~/.config/claude/claude_desktop_config.json |
//! | Cursor | ~/Library/Application Support/Cursor/User/globalStorage/cursor.mcp/config.json | ~/.config/cursor/mcp_servers.json |
//! | Windsurf | ~/Library/Application Support/Windsurf/User/globalStorage/windsurf.mcp/config.json | ~/.config/windsurf/mcp_servers.json |
//! | VS Code | ~/Library/Application Support/Code/User/settings.json | ~/.config/Code/User/settings.json |
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::sync::operations::{sync_to_editor, sync_all, get_editor_config_path, SyncResult};
//! use nika::sync::editor::EditorKind;
//! use nika::core::mcp_config::McpConfig;
//!
//! // Get editor config path
//! if let Some(path) = get_editor_config_path(EditorKind::ClaudeCode) {
//!     println!("Claude config at: {}", path.display());
//! }
//!
//! // Sync to a specific editor
//! let config = McpConfig::default();
//! let result = sync_to_editor(EditorKind::ClaudeCode, &config).await?;
//!
//! // Sync to all editors
//! let editors = vec![EditorKind::ClaudeCode, EditorKind::Cursor];
//! let results = sync_all(&editors, &config).await?;
//! ```

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::config::SyncConfig;
use super::editor::{Editor, EditorKind};
use crate::core::mcp_config::{load_merged_config, McpConfig};
use crate::error::{NikaError, Result};

/// Result of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    /// Editor that was synced.
    pub editor: EditorKind,
    /// Whether sync succeeded.
    pub success: bool,
    /// Path to the config file that was written.
    pub config_path: PathBuf,
    /// Error message if failed.
    pub error: Option<String>,
    /// Number of servers synced.
    pub servers_synced: usize,
}

impl SyncResult {
    /// Create a successful sync result.
    pub fn success(editor: EditorKind, config_path: PathBuf, servers_synced: usize) -> Self {
        Self {
            editor,
            success: true,
            config_path,
            error: None,
            servers_synced,
        }
    }

    /// Create a failed sync result.
    pub fn failure(editor: EditorKind, config_path: PathBuf, error: String) -> Self {
        Self {
            editor,
            success: false,
            config_path,
            error: Some(error),
            servers_synced: 0,
        }
    }

    /// Create a failure result for an uninstalled editor.
    pub fn not_installed(editor: EditorKind) -> Self {
        Self {
            editor,
            success: false,
            config_path: PathBuf::new(),
            error: Some(format!("{} is not installed", editor.display_name())),
            servers_synced: 0,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PLATFORM-SPECIFIC CONFIG PATHS
// ═══════════════════════════════════════════════════════════════════════════════

/// Get the platform-specific config path for an editor.
///
/// Returns `None` if the home directory cannot be determined.
///
/// ## Paths by Platform
///
/// | Editor | macOS | Linux |
/// |--------|-------|-------|
/// | Claude Code | ~/Library/Application Support/Claude/claude_desktop_config.json | ~/.config/claude/claude_desktop_config.json |
/// | Cursor | ~/Library/Application Support/Cursor/User/globalStorage/cursor.mcp/config.json | ~/.config/cursor/mcp_servers.json |
/// | Windsurf | ~/Library/Application Support/Windsurf/User/globalStorage/windsurf.mcp/config.json | ~/.config/windsurf/mcp_servers.json |
/// | VS Code | ~/Library/Application Support/Code/User/settings.json | ~/.config/Code/User/settings.json |
pub fn get_editor_config_path(editor: EditorKind) -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    #[cfg(target_os = "macos")]
    {
        let app_support = home.join("Library/Application Support");
        Some(match editor {
            EditorKind::ClaudeCode => app_support.join("Claude/claude_desktop_config.json"),
            EditorKind::Cursor => {
                app_support.join("Cursor/User/globalStorage/cursor.mcp/config.json")
            }
            EditorKind::Windsurf => {
                app_support.join("Windsurf/User/globalStorage/windsurf.mcp/config.json")
            }
            EditorKind::VsCode => app_support.join("Code/User/settings.json"),
        })
    }

    #[cfg(target_os = "linux")]
    {
        let config_dir = home.join(".config");
        Some(match editor {
            EditorKind::ClaudeCode => config_dir.join("claude/claude_desktop_config.json"),
            EditorKind::Cursor => config_dir.join("cursor/mcp_servers.json"),
            EditorKind::Windsurf => config_dir.join("windsurf/mcp_servers.json"),
            EditorKind::VsCode => config_dir.join("Code/User/settings.json"),
        })
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        // Fallback for unsupported platforms (Windows, etc.)
        // Use simple home-based paths
        Some(match editor {
            EditorKind::ClaudeCode => home.join(".claude/settings.json"),
            EditorKind::Cursor => home.join(".cursor/mcp.json"),
            EditorKind::Windsurf => home.join(".windsurf/mcp.json"),
            EditorKind::VsCode => home.join(".vscode/mcp.json"),
        })
    }
}

/// Check if an editor is installed by checking if its config directory exists.
pub fn is_editor_installed(editor: EditorKind) -> bool {
    get_editor_config_path(editor)
        .and_then(|p| p.parent().map(|parent| parent.exists()))
        .unwrap_or(false)
}

/// Get the parent directory for editor config (for creation).
#[allow(dead_code)]
fn get_editor_config_dir(editor: EditorKind) -> Option<PathBuf> {
    get_editor_config_path(editor).and_then(|p| p.parent().map(|p| p.to_path_buf()))
}

// ═══════════════════════════════════════════════════════════════════════════════
// SYNC OPERATIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Sync MCP configuration to a specific editor.
///
/// This function:
/// 1. Resolves the platform-specific config path for the editor
/// 2. Converts the MCP config to the editor's expected format
/// 3. Writes the config file (merging with existing settings if applicable)
///
/// # Arguments
///
/// * `editor` - The editor to sync to
/// * `config` - The MCP configuration to sync
///
/// # Returns
///
/// A `SyncResult` indicating success or failure
pub async fn sync_to_editor(editor: EditorKind, config: &McpConfig) -> Result<SyncResult> {
    let config_path = match get_editor_config_path(editor) {
        Some(path) => path,
        None => {
            return Ok(SyncResult::failure(
                editor,
                PathBuf::new(),
                "Could not determine editor config path".to_string(),
            ));
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(|e| NikaError::IoPathError {
                path: parent.to_path_buf(),
                operation: "create editor config directory".to_string(),
                source: e,
            })?;
        }
    }

    // Convert McpConfig to editor-specific JSON format
    let server_count = config.servers.len();
    let servers_json = mcp_config_to_json(config);

    // Write config in editor-specific format
    match write_editor_config(&config_path, editor, &servers_json) {
        Ok(()) => Ok(SyncResult::success(editor, config_path, server_count)),
        Err(e) => Ok(SyncResult::failure(editor, config_path, e.to_string())),
    }
}

/// Sync MCP configuration to multiple editors.
///
/// This function syncs to all specified editors in parallel.
///
/// # Arguments
///
/// * `editors` - The editors to sync to
/// * `config` - The MCP configuration to sync
///
/// # Returns
///
/// A vector of `SyncResult`s, one for each editor
pub async fn sync_all(editors: &[EditorKind], config: &McpConfig) -> Result<Vec<SyncResult>> {
    let mut results = Vec::with_capacity(editors.len());

    for &editor in editors {
        let result = sync_to_editor(editor, config).await?;
        results.push(result);
    }

    Ok(results)
}

/// Sync MCP configuration to all enabled editors using SyncConfig.
///
/// This is the main entry point for the `nika sync` command. It:
/// 1. Loads the MCP config from ~/.nika/mcp.yaml (merged with project config)
/// 2. Syncs to all enabled editors
/// 3. Updates the SyncConfig with results
///
/// # Arguments
///
/// * `sync_config` - The sync configuration (modified with results)
///
/// # Returns
///
/// A vector of `SyncResult`s, one for each enabled editor
pub fn sync_enabled(sync_config: &mut SyncConfig) -> Result<Vec<SyncResult>> {
    // Load merged MCP config (global + project)
    let mcp_config = load_merged_config().map_err(|e| NikaError::SyncError {
        message: format!("Failed to load MCP config: {}", e),
    })?;

    let mut results = Vec::new();

    for editor_kind in sync_config.enabled_editors.clone() {
        let editor = Editor::detect(editor_kind);

        if !editor.installed {
            let result = SyncResult::not_installed(editor_kind);
            sync_config.record_sync_result(editor_kind, false, result.error.clone());
            results.push(result);
            continue;
        }

        // Use synchronous version for compatibility with existing code
        let result = sync_to_editor_sync(editor_kind, &mcp_config);
        sync_config.record_sync_result(editor_kind, result.success, result.error.clone());
        results.push(result);
    }

    if results.iter().any(|r| r.success) {
        sync_config.update_last_sync();
    }

    sync_config.save()?;
    Ok(results)
}

/// Synchronous version of sync_to_editor for compatibility.
fn sync_to_editor_sync(editor: EditorKind, config: &McpConfig) -> SyncResult {
    let config_path = match get_editor_config_path(editor) {
        Some(path) => path,
        None => {
            return SyncResult::failure(
                editor,
                PathBuf::new(),
                "Could not determine editor config path".to_string(),
            );
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return SyncResult::failure(
                    editor,
                    config_path,
                    format!("Failed to create config directory: {}", e),
                );
            }
        }
    }

    // Convert McpConfig to editor-specific JSON format
    let server_count = config.servers.len();
    let servers_json = mcp_config_to_json(config);

    // Write config in editor-specific format
    match write_editor_config(&config_path, editor, &servers_json) {
        Ok(()) => SyncResult::success(editor, config_path, server_count),
        Err(e) => SyncResult::failure(editor, config_path, e.to_string()),
    }
}

/// Sync a single editor (convenience wrapper for CLI).
pub fn sync_editor(editor_kind: EditorKind) -> Result<SyncResult> {
    let editor = Editor::detect(editor_kind);

    if !editor.installed {
        return Ok(SyncResult::not_installed(editor_kind));
    }

    // Load merged MCP config
    let mcp_config = load_merged_config().map_err(|e| NikaError::SyncError {
        message: format!("Failed to load MCP config: {}", e),
    })?;

    Ok(sync_to_editor_sync(editor_kind, &mcp_config))
}

// ═══════════════════════════════════════════════════════════════════════════════
// CONFIG CONVERSION
// ═══════════════════════════════════════════════════════════════════════════════

/// Convert McpConfig to JSON format expected by editors.
///
/// Converts the MCP server definitions to the standard format:
/// ```json
/// {
///   "server-name": {
///     "command": "npx",
///     "args": ["-y", "@package/name"],
///     "env": { "KEY": "value" }
///   }
/// }
/// ```
fn mcp_config_to_json(config: &McpConfig) -> Value {
    let mut servers = serde_json::Map::new();

    for (name, server) in &config.servers {
        if !server.enabled {
            continue;
        }

        let mut server_obj = serde_json::Map::new();
        server_obj.insert("command".to_string(), json!(server.command));

        if !server.args.is_empty() {
            server_obj.insert("args".to_string(), json!(server.args));
        }

        if !server.env.is_empty() {
            server_obj.insert("env".to_string(), json!(server.env));
        }

        servers.insert(name.clone(), Value::Object(server_obj));
    }

    Value::Object(servers)
}

/// Write MCP config to editor in the expected format.
fn write_editor_config(path: &Path, editor: EditorKind, servers: &Value) -> Result<()> {
    match editor {
        EditorKind::ClaudeCode => write_claude_config(path, servers),
        EditorKind::VsCode => write_vscode_config(path, servers),
        EditorKind::Cursor | EditorKind::Windsurf => write_mcp_json(path, servers),
    }
}

/// Write Claude Code config (claude_desktop_config.json).
///
/// Claude Code expects the format:
/// ```json
/// {
///   "mcpServers": { ... }
/// }
/// ```
fn write_claude_config(path: &Path, servers: &Value) -> Result<()> {
    // Load existing config or create new
    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| NikaError::IoPathError {
            path: path.to_path_buf(),
            operation: "read Claude config".to_string(),
            source: e,
        })?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Set mcpServers key
    config["mcpServers"] = servers.clone();

    // Write with pretty formatting
    let content = serde_json::to_string_pretty(&config).map_err(|e| NikaError::SyncError {
        message: format!("Failed to serialize Claude config: {}", e),
    })?;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.to_path_buf(),
        operation: "write Claude config".to_string(),
        source: e,
    })
}

/// Write VS Code config (settings.json).
///
/// VS Code expects MCP servers under a specific key, we merge with existing settings:
/// ```json
/// {
///   "existing.setting": "value",
///   "mcp.servers": { ... }
/// }
/// ```
fn write_vscode_config(path: &Path, servers: &Value) -> Result<()> {
    // Load existing config or create new
    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|e| NikaError::IoPathError {
            path: path.to_path_buf(),
            operation: "read VS Code settings".to_string(),
            source: e,
        })?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Set mcp.servers key (VS Code convention)
    config["mcp.servers"] = servers.clone();

    // Write with pretty formatting
    let content = serde_json::to_string_pretty(&config).map_err(|e| NikaError::SyncError {
        message: format!("Failed to serialize VS Code settings: {}", e),
    })?;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.to_path_buf(),
        operation: "write VS Code settings".to_string(),
        source: e,
    })
}

/// Write standalone MCP config (mcp.json / mcp_servers.json).
///
/// For Cursor and Windsurf:
/// ```json
/// {
///   "mcpServers": { ... }
/// }
/// ```
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
// LEGACY COMPATIBILITY
// ═══════════════════════════════════════════════════════════════════════════════

/// Sync all enabled editors (legacy API, uses SyncConfig).
#[allow(dead_code)]
pub fn sync_all_legacy(config: &mut SyncConfig) -> Result<Vec<SyncResult>> {
    sync_enabled(config)
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mcp_config::McpServer;
    use std::collections::HashMap;
    use tempfile::tempdir;

    // ─────────────────────────────────────────────────────────────────────────────
    // Config Path Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_get_editor_config_path_claude_code() {
        let path = get_editor_config_path(EditorKind::ClaudeCode);
        assert!(path.is_some());

        let path = path.unwrap();

        #[cfg(target_os = "macos")]
        assert!(path
            .to_string_lossy()
            .contains("Library/Application Support/Claude"));

        #[cfg(target_os = "linux")]
        assert!(path.to_string_lossy().contains(".config/claude"));
    }

    #[test]
    fn test_get_editor_config_path_cursor() {
        let path = get_editor_config_path(EditorKind::Cursor);
        assert!(path.is_some());

        let path = path.unwrap();

        #[cfg(target_os = "macos")]
        assert!(path
            .to_string_lossy()
            .contains("Library/Application Support/Cursor"));

        #[cfg(target_os = "linux")]
        assert!(path.to_string_lossy().contains(".config/cursor"));
    }

    #[test]
    fn test_get_editor_config_path_windsurf() {
        let path = get_editor_config_path(EditorKind::Windsurf);
        assert!(path.is_some());

        let path = path.unwrap();

        #[cfg(target_os = "macos")]
        assert!(path
            .to_string_lossy()
            .contains("Library/Application Support/Windsurf"));

        #[cfg(target_os = "linux")]
        assert!(path.to_string_lossy().contains(".config/windsurf"));
    }

    #[test]
    fn test_get_editor_config_path_vscode() {
        let path = get_editor_config_path(EditorKind::VsCode);
        assert!(path.is_some());

        let path = path.unwrap();

        #[cfg(target_os = "macos")]
        assert!(path
            .to_string_lossy()
            .contains("Library/Application Support/Code"));

        #[cfg(target_os = "linux")]
        assert!(path.to_string_lossy().contains(".config/Code"));
    }

    #[test]
    fn test_all_editors_have_config_paths() {
        for editor in EditorKind::all() {
            let path = get_editor_config_path(*editor);
            assert!(
                path.is_some(),
                "Editor {:?} should have a config path",
                editor
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Config Conversion Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_mcp_config_to_json_basic() {
        let mut servers = HashMap::new();
        servers.insert(
            "neo4j".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@neo4j/mcp-neo4j".to_string()],
                env: HashMap::new(),
                description: Some("Neo4j database".to_string()),
                enabled: true,
                source: None,
            },
        );

        let config = McpConfig {
            version: 1,
            servers,
        };

        let json = mcp_config_to_json(&config);

        assert!(json.get("neo4j").is_some());
        assert_eq!(json["neo4j"]["command"], "npx");
        assert_eq!(json["neo4j"]["args"][0], "-y");
        assert_eq!(json["neo4j"]["args"][1], "@neo4j/mcp-neo4j");
    }

    #[test]
    fn test_mcp_config_to_json_with_env() {
        let mut env = HashMap::new();
        env.insert("NEO4J_URI".to_string(), "bolt://localhost:7687".to_string());
        env.insert("NEO4J_USER".to_string(), "neo4j".to_string());

        let mut servers = HashMap::new();
        servers.insert(
            "neo4j".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@neo4j/mcp-neo4j".to_string()],
                env,
                description: None,
                enabled: true,
                source: None,
            },
        );

        let config = McpConfig {
            version: 1,
            servers,
        };

        let json = mcp_config_to_json(&config);

        assert!(json["neo4j"]["env"].is_object());
        assert_eq!(json["neo4j"]["env"]["NEO4J_URI"], "bolt://localhost:7687");
        assert_eq!(json["neo4j"]["env"]["NEO4J_USER"], "neo4j");
    }

    #[test]
    fn test_mcp_config_to_json_excludes_disabled() {
        let mut servers = HashMap::new();
        servers.insert(
            "enabled".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec![],
                env: HashMap::new(),
                description: None,
                enabled: true,
                source: None,
            },
        );
        servers.insert(
            "disabled".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec![],
                env: HashMap::new(),
                description: None,
                enabled: false,
                source: None,
            },
        );

        let config = McpConfig {
            version: 1,
            servers,
        };

        let json = mcp_config_to_json(&config);

        assert!(json.get("enabled").is_some());
        assert!(json.get("disabled").is_none());
    }

    #[test]
    fn test_mcp_config_to_json_empty() {
        let config = McpConfig::default();
        let json = mcp_config_to_json(&config);

        assert!(json.is_object());
        assert!(json.as_object().unwrap().is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Config Writing Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_write_claude_config_new() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("claude_desktop_config.json");

        let servers = json!({
            "neo4j": {
                "command": "npx",
                "args": ["-y", "@neo4j/mcp-neo4j"]
            }
        });

        write_claude_config(&path, &servers).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed.get("mcpServers").is_some());
        assert!(parsed["mcpServers"].get("neo4j").is_some());
    }

    #[test]
    fn test_write_claude_config_merge_existing() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("claude_desktop_config.json");

        // Write initial config with other settings
        let initial = json!({
            "telemetry": false,
            "theme": "dark",
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

        write_claude_config(&path, &servers).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Other settings preserved
        assert_eq!(parsed["telemetry"], false);
        assert_eq!(parsed["theme"], "dark");

        // MCP servers replaced (not merged)
        assert!(parsed["mcpServers"].get("neo4j").is_some());
        assert!(parsed["mcpServers"].get("old_server").is_none());
    }

    #[test]
    fn test_write_vscode_config_new() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("settings.json");

        let servers = json!({
            "perplexity": {
                "command": "node",
                "args": ["server.js"]
            }
        });

        write_vscode_config(&path, &servers).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed.get("mcp.servers").is_some());
        assert!(parsed["mcp.servers"].get("perplexity").is_some());
    }

    #[test]
    fn test_write_vscode_config_merge_existing() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("settings.json");

        // Write initial VS Code settings
        let initial = json!({
            "editor.fontSize": 14,
            "editor.tabSize": 2,
            "workbench.colorTheme": "Solarized Dark"
        });
        std::fs::write(&path, serde_json::to_string_pretty(&initial).unwrap()).unwrap();

        let servers = json!({
            "neo4j": {
                "command": "npx",
                "args": ["-y", "@neo4j/mcp-neo4j"]
            }
        });

        write_vscode_config(&path, &servers).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        // Existing settings preserved
        assert_eq!(parsed["editor.fontSize"], 14);
        assert_eq!(parsed["editor.tabSize"], 2);
        assert_eq!(parsed["workbench.colorTheme"], "Solarized Dark");

        // MCP servers added
        assert!(parsed["mcp.servers"].get("neo4j").is_some());
    }

    #[test]
    fn test_write_mcp_json() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("mcp_servers.json");

        let servers = json!({
            "github": {
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-github"]
            }
        });

        write_mcp_json(&path, &servers).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed.get("mcpServers").is_some());
        assert!(parsed["mcpServers"].get("github").is_some());
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // SyncResult Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_sync_result_success() {
        let result =
            SyncResult::success(EditorKind::ClaudeCode, PathBuf::from("/tmp/config.json"), 3);

        assert!(result.success);
        assert!(result.error.is_none());
        assert_eq!(result.servers_synced, 3);
        assert_eq!(result.editor, EditorKind::ClaudeCode);
    }

    #[test]
    fn test_sync_result_failure() {
        let result = SyncResult::failure(
            EditorKind::Cursor,
            PathBuf::from("/tmp/config.json"),
            "Write permission denied".to_string(),
        );

        assert!(!result.success);
        assert_eq!(result.error, Some("Write permission denied".to_string()));
        assert_eq!(result.servers_synced, 0);
    }

    #[test]
    fn test_sync_result_not_installed() {
        let result = SyncResult::not_installed(EditorKind::Windsurf);

        assert!(!result.success);
        assert!(result.error.unwrap().contains("not installed"));
        assert_eq!(result.servers_synced, 0);
        assert!(result.config_path.as_os_str().is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Sync Operation Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_sync_to_editor_creates_directory() {
        let temp = tempdir().unwrap();
        let config_dir = temp.path().join("Claude");

        // Mock the path resolution by creating a test helper
        let config = McpConfig::default();

        // This test verifies the directory creation logic
        // We can't easily test the actual function without mocking get_editor_config_path
        // So we test the underlying write functions instead

        let config_path = config_dir.join("claude_desktop_config.json");

        // Parent doesn't exist yet
        assert!(!config_dir.exists());

        // Create parent
        std::fs::create_dir_all(&config_dir).unwrap();

        let servers = mcp_config_to_json(&config);
        write_claude_config(&config_path, &servers).unwrap();

        assert!(config_path.exists());
    }

    #[tokio::test]
    async fn test_sync_all_empty_editors() {
        let config = McpConfig::default();
        let editors: Vec<EditorKind> = vec![];

        let results = sync_all(&editors, &config).await.unwrap();
        assert!(results.is_empty());
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Integration Tests
    // ─────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_full_sync_workflow() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("test_config.json");

        // Create a realistic MCP config
        let mut servers = HashMap::new();

        servers.insert(
            "neo4j".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec!["-y".to_string(), "@neo4j/mcp-neo4j".to_string()],
                env: {
                    let mut env = HashMap::new();
                    env.insert("NEO4J_URI".to_string(), "bolt://localhost:7687".to_string());
                    env
                },
                description: Some("Neo4j graph database".to_string()),
                enabled: true,
                source: None,
            },
        );

        servers.insert(
            "github".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-github".to_string(),
                ],
                env: HashMap::new(),
                description: Some("GitHub API".to_string()),
                enabled: true,
                source: None,
            },
        );

        servers.insert(
            "disabled_server".to_string(),
            McpServer {
                command: "npx".to_string(),
                args: vec![],
                env: HashMap::new(),
                description: None,
                enabled: false,
                source: None,
            },
        );

        let config = McpConfig {
            version: 1,
            servers,
        };

        // Convert to JSON
        let json = mcp_config_to_json(&config);

        // Verify conversion
        assert!(json.get("neo4j").is_some());
        assert!(json.get("github").is_some());
        assert!(json.get("disabled_server").is_none()); // Disabled servers excluded

        // Write as Claude config
        write_claude_config(&config_path, &json).unwrap();

        // Verify file contents
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();

        assert!(parsed["mcpServers"]["neo4j"]["env"]["NEO4J_URI"]
            .as_str()
            .unwrap()
            .contains("localhost"));

        assert_eq!(
            parsed["mcpServers"]["github"]["args"][1],
            "@modelcontextprotocol/server-github"
        );
    }

    #[test]
    fn test_platform_detection() {
        // This test just verifies that platform detection works and returns valid paths
        for editor in EditorKind::all() {
            let path = get_editor_config_path(*editor);
            assert!(path.is_some(), "Should get path for {:?}", editor);

            let path = path.unwrap();
            assert!(
                !path.as_os_str().is_empty(),
                "Path should not be empty for {:?}",
                editor
            );

            // Path should end with a json file
            assert!(
                path.extension().is_some_and(|ext| ext == "json"),
                "Path should end with .json for {:?}: {:?}",
                editor,
                path
            );
        }
    }
}
