//! MCP server configuration management — global and project-level configs.
//!
//! This module provides types and functions for managing MCP server configurations
//! at both global (~/.nika/mcp.yaml) and project (.nika/mcp.yaml) levels.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  MCP CONFIG HIERARCHY (lowest wins)                                         │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  Global:   ~/.nika/mcp.yaml        ← User's personal MCP servers            │
//! │              ↑                                                              │
//! │  Project:  ./.nika/mcp.yaml        ← Project-specific servers               │
//! │              ↑                                                              │
//! │  Workflow: (inline in .nika.yaml)  ← Workflow-specific servers              │
//! │                                                                             │
//! │  Resolution: project overrides global, workflow overrides both              │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::core::mcp_config::{McpConfig, McpServer, load_global_config, save_global_config};
//!
//! // Load existing config (or create empty)
//! let mut config = load_global_config().unwrap_or_default();
//!
//! // Add a server
//! config.servers.insert("neo4j".to_string(), McpServer {
//!     command: "npx".to_string(),
//!     args: vec!["-y".to_string(), "@neo4j/mcp-neo4j".to_string()],
//!     env: Default::default(),
//!     description: Some("Neo4j graph database".to_string()),
//!     enabled: true,
//!     source: Some(McpSource::Global),
//! });
//!
//! // Save
//! save_global_config(&config)?;
//! ```

use crate::serde_yaml;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// MCP configuration file structure.
///
/// Contains a version and a map of server names to their configurations.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct McpConfig {
    /// Config file version (currently 1)
    #[serde(default = "default_version")]
    pub version: u32,

    /// Map of server names to their configurations
    #[serde(default)]
    pub servers: HashMap<String, McpServer>,
}

fn default_version() -> u32 {
    1
}

/// MCP server configuration.
///
/// Defines how to start and configure an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServer {
    /// Command to run (e.g., "npx", "node", "python")
    pub command: String,

    /// Command arguments (e.g., `["-y", "@neo4j/mcp-neo4j"]`)
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables for the server
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this server is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Source of this configuration (for merged configs)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<McpSource>,
}

fn default_enabled() -> bool {
    true
}

/// Source of an MCP server configuration.
///
/// Used to track where a server definition came from when configs are merged.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpSource {
    /// Global config (~/.nika/mcp.yaml)
    Global,
    /// Project config (.mcp.json or .nika/mcp.yaml)
    Project,
}

impl std::fmt::Display for McpSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpSource::Global => write!(f, "global"),
            McpSource::Project => write!(f, "project"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// PATH HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Get the global MCP config directory (~/.nika/).
///
/// Creates the directory if it doesn't exist.
pub fn global_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".nika"))
}

/// Get the global MCP config file path (~/.nika/mcp.yaml).
pub fn global_config_path() -> Option<PathBuf> {
    global_config_dir().map(|d| d.join("mcp.yaml"))
}

/// Get the project MCP config file path (.nika/mcp.yaml).
///
/// Searches upward from current directory for a .nika directory.
pub fn project_config_path() -> Option<PathBuf> {
    find_project_root().map(|r| r.join(".nika").join("mcp.yaml"))
}

/// Get the project config path relative to a specific directory.
pub fn project_config_path_from(dir: &Path) -> PathBuf {
    dir.join(".nika").join("mcp.yaml")
}

/// Find the project root by searching for .nika, .git, or nika.yaml.
fn find_project_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let mut current = cwd.as_path();

    loop {
        // Check for .nika directory
        if current.join(".nika").is_dir() {
            return Some(current.to_path_buf());
        }
        // Check for .git directory (common project root indicator)
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        // Check for nika.yaml (project manifest)
        if current.join("nika.yaml").exists() {
            return Some(current.to_path_buf());
        }

        current = current.parent()?;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LOAD / SAVE
// ═══════════════════════════════════════════════════════════════════════════════

/// Load the global MCP config (~/.nika/mcp.yaml).
///
/// Returns `None` if the file doesn't exist, `Err` on parse errors.
pub fn load_global_config() -> Result<Option<McpConfig>, McpConfigError> {
    load_config_from_path(global_config_path())
}

/// Load the project MCP config (.nika/mcp.yaml).
///
/// Returns `None` if the file doesn't exist, `Err` on parse errors.
pub fn load_project_config() -> Result<Option<McpConfig>, McpConfigError> {
    load_config_from_path(project_config_path())
}

/// Load an MCP config from a specific path.
pub fn load_config_from_path(path: Option<PathBuf>) -> Result<Option<McpConfig>, McpConfigError> {
    let path = match path {
        Some(p) => p,
        None => return Ok(None),
    };

    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path).map_err(|e| McpConfigError::Io {
        path: path.clone(),
        source: e,
    })?;

    let config: McpConfig = serde_yaml::from_str(&content).map_err(|e| McpConfigError::Parse {
        path: path.clone(),
        message: e.to_string(),
    })?;

    Ok(Some(config))
}

/// Save the global MCP config (~/.nika/mcp.yaml).
///
/// Creates the directory if it doesn't exist.
pub fn save_global_config(config: &McpConfig) -> Result<(), McpConfigError> {
    let path = global_config_path().ok_or(McpConfigError::NoHomeDir)?;
    save_config_to_path(config, &path)
}

/// Save the project MCP config (.nika/mcp.yaml).
///
/// Creates the directory if it doesn't exist.
pub fn save_project_config(config: &McpConfig) -> Result<(), McpConfigError> {
    let path = project_config_path().ok_or(McpConfigError::NoProjectRoot)?;
    save_config_to_path(config, &path)
}

/// Save an MCP config to a specific path.
pub fn save_config_to_path(config: &McpConfig, path: &Path) -> Result<(), McpConfigError> {
    // Create parent directory if needed
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| McpConfigError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }

    let content =
        serde_yaml::to_string(config).map_err(|e| McpConfigError::Serialize(e.to_string()))?;
    std::fs::write(path, content).map_err(|e| McpConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// MERGE
// ═══════════════════════════════════════════════════════════════════════════════

/// Merge multiple configs with precedence: later configs override earlier ones.
///
/// The `source` field is set on each server to indicate its origin.
pub fn merge_configs(configs: Vec<(McpConfig, McpSource)>) -> McpConfig {
    let mut merged = McpConfig::default();

    for (config, source) in configs {
        for (name, mut server) in config.servers {
            server.source = Some(source);
            merged.servers.insert(name, server);
        }
    }

    merged
}

/// Load and merge global + project configs.
///
/// Project config overrides global config.
pub fn load_merged_config() -> Result<McpConfig, McpConfigError> {
    let mut configs = Vec::new();

    // Load global config (if exists)
    if let Some(global) = load_global_config()? {
        configs.push((global, McpSource::Global));
    }

    // Load project config (if exists)
    if let Some(project) = load_project_config()? {
        configs.push((project, McpSource::Project));
    }

    Ok(merge_configs(configs))
}

// ═══════════════════════════════════════════════════════════════════════════════
// SERVER MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════════════

/// Create an McpServer from an npm package name.
///
/// Uses `npx -y <package>` as the command.
pub fn server_from_npm_package(package: &str, description: Option<&str>) -> McpServer {
    McpServer {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), package.to_string()],
        env: HashMap::new(),
        description: description.map(|s| s.to_string()),
        enabled: true,
        source: None,
    }
}

/// Add a server to the global config.
///
/// Returns `true` if the server was added, `false` if it already exists.
pub fn add_server_to_global(name: &str, server: McpServer) -> Result<bool, McpConfigError> {
    let mut config = load_global_config()?.unwrap_or_default();

    if config.servers.contains_key(name) {
        return Ok(false);
    }

    config.servers.insert(name.to_string(), server);
    save_global_config(&config)?;
    Ok(true)
}

/// Add a server to the project config.
///
/// Returns `true` if the server was added, `false` if it already exists.
pub fn add_server_to_project(name: &str, server: McpServer) -> Result<bool, McpConfigError> {
    let mut config = load_project_config()?.unwrap_or_default();

    if config.servers.contains_key(name) {
        return Ok(false);
    }

    config.servers.insert(name.to_string(), server);
    save_project_config(&config)?;
    Ok(true)
}

/// Remove a server from the global config.
///
/// Returns `true` if the server was removed, `false` if it didn't exist.
pub fn remove_server_from_global(name: &str) -> Result<bool, McpConfigError> {
    let mut config = match load_global_config()? {
        Some(c) => c,
        None => return Ok(false),
    };

    let removed = config.servers.remove(name).is_some();
    if removed {
        save_global_config(&config)?;
    }
    Ok(removed)
}

/// Remove a server from the project config.
///
/// Returns `true` if the server was removed, `false` if it didn't exist.
pub fn remove_server_from_project(name: &str) -> Result<bool, McpConfigError> {
    let mut config = match load_project_config()? {
        Some(c) => c,
        None => return Ok(false),
    };

    let removed = config.servers.remove(name).is_some();
    if removed {
        save_project_config(&config)?;
    }
    Ok(removed)
}

// ═══════════════════════════════════════════════════════════════════════════════
// ERRORS
// ═══════════════════════════════════════════════════════════════════════════════

/// Errors that can occur when working with MCP configs.
#[derive(Debug)]
pub enum McpConfigError {
    /// IO error (reading/writing files)
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// YAML parse error
    Parse { path: PathBuf, message: String },
    /// YAML serialize error
    Serialize(String),
    /// Home directory not found
    NoHomeDir,
    /// Project root not found
    NoProjectRoot,
}

impl std::fmt::Display for McpConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpConfigError::Io { path, source } => {
                write!(f, "I/O error for {}: {}", path.display(), source)
            }
            McpConfigError::Parse { path, message } => {
                write!(f, "Parse error in {}: {}", path.display(), message)
            }
            McpConfigError::Serialize(e) => write!(f, "Serialize error: {}", e),
            McpConfigError::NoHomeDir => write!(f, "Home directory not found"),
            McpConfigError::NoProjectRoot => write!(f, "Project root not found"),
        }
    }
}

impl std::error::Error for McpConfigError {}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = McpConfig::default();
        assert_eq!(config.version, 0); // serde default is 0, not 1 (default_version only for deserialize)
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_server_from_npm_package() {
        let server = server_from_npm_package("@neo4j/mcp-neo4j", Some("Neo4j database"));
        assert_eq!(server.command, "npx");
        assert_eq!(server.args, vec!["-y", "@neo4j/mcp-neo4j"]);
        assert_eq!(server.description, Some("Neo4j database".to_string()));
        assert!(server.enabled);
        assert!(server.source.is_none());
    }

    #[test]
    fn test_mcp_source_display() {
        assert_eq!(McpSource::Global.to_string(), "global");
        assert_eq!(McpSource::Project.to_string(), "project");
    }

    #[test]
    fn test_merge_configs() {
        let global = McpConfig {
            version: 1,
            servers: {
                let mut m = HashMap::new();
                m.insert(
                    "neo4j".to_string(),
                    McpServer {
                        command: "npx".to_string(),
                        args: vec!["-y".to_string(), "@neo4j/mcp-neo4j".to_string()],
                        env: HashMap::new(),
                        description: Some("Global neo4j".to_string()),
                        enabled: true,
                        source: None,
                    },
                );
                m.insert(
                    "github".to_string(),
                    McpServer {
                        command: "npx".to_string(),
                        args: vec![
                            "-y".to_string(),
                            "@modelcontextprotocol/server-github".to_string(),
                        ],
                        env: HashMap::new(),
                        description: Some("GitHub".to_string()),
                        enabled: true,
                        source: None,
                    },
                );
                m
            },
        };

        let project = McpConfig {
            version: 1,
            servers: {
                let mut m = HashMap::new();
                m.insert(
                    "neo4j".to_string(),
                    McpServer {
                        command: "npx".to_string(),
                        args: vec!["-y".to_string(), "@neo4j/mcp-neo4j".to_string()],
                        env: {
                            let mut e = HashMap::new();
                            e.insert("NEO4J_URI".to_string(), "bolt://project:7687".to_string());
                            e
                        },
                        description: Some("Project neo4j".to_string()),
                        enabled: true,
                        source: None,
                    },
                );
                m
            },
        };

        let merged = merge_configs(vec![
            (global, McpSource::Global),
            (project, McpSource::Project),
        ]);

        // Project neo4j should override global neo4j
        assert_eq!(merged.servers.len(), 2);
        let neo4j = merged.servers.get("neo4j").unwrap();
        assert_eq!(neo4j.description, Some("Project neo4j".to_string()));
        assert_eq!(neo4j.source, Some(McpSource::Project));
        assert!(neo4j.env.contains_key("NEO4J_URI"));

        // GitHub should remain from global
        let github = merged.servers.get("github").unwrap();
        assert_eq!(github.source, Some(McpSource::Global));
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let config = McpConfig {
            version: 1,
            servers: {
                let mut m = HashMap::new();
                m.insert(
                    "test".to_string(),
                    McpServer {
                        command: "npx".to_string(),
                        args: vec!["-y".to_string(), "test-package".to_string()],
                        env: {
                            let mut e = HashMap::new();
                            e.insert("KEY".to_string(), "value".to_string());
                            e
                        },
                        description: Some("Test server".to_string()),
                        enabled: true,
                        source: Some(McpSource::Global),
                    },
                );
                m
            },
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        let parsed: McpConfig = serde_yaml::from_str(&yaml).unwrap();

        assert_eq!(config.version, parsed.version);
        assert_eq!(config.servers.len(), parsed.servers.len());
        assert_eq!(
            config.servers.get("test").unwrap().command,
            parsed.servers.get("test").unwrap().command
        );
    }

    #[test]
    fn test_global_config_path() {
        let path = global_config_path();
        // Should be Some on most systems
        if let Some(p) = path {
            assert!(p.ends_with(".nika/mcp.yaml") || p.ends_with(".nika\\mcp.yaml"));
        }
    }

    #[test]
    fn test_load_nonexistent_config() {
        // Loading a nonexistent path should return Ok(None)
        let result = load_config_from_path(Some(PathBuf::from("/nonexistent/path/mcp.yaml")));
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
