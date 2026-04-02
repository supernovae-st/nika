//! MCP server configuration management — global and project-level configs.
//!
//! ## Priority (project overrides global)
//!
//! ```text
//! 1. .mcp.json       (project root — Claude Code convention, preferred)
//! 2. .nika/mcp.yaml  (project root — legacy fallback)
//! 3. ~/.nika/mcp.yaml (global user config)
//! ```
//!
//! When `.mcp.json` exists, writes go there. Otherwise falls back to `.nika/mcp.yaml`.
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

/// Get the project MCP config file path.
///
/// Prefers `.mcp.json` (Claude Code convention). Falls back to `.nika/mcp.yaml`.
pub fn project_config_path() -> Option<PathBuf> {
    let root = find_project_root()?;
    let mcp_json = root.join(".mcp.json");
    if mcp_json.exists() {
        Some(mcp_json)
    } else {
        Some(root.join(".nika").join("mcp.yaml"))
    }
}

/// Get the project config path relative to a specific directory.
///
/// Prefers `.mcp.json` (Claude Code convention). Falls back to `.nika/mcp.yaml`.
pub fn project_config_path_from(dir: &Path) -> PathBuf {
    let mcp_json = dir.join(".mcp.json");
    if mcp_json.exists() {
        mcp_json
    } else {
        dir.join(".nika").join("mcp.yaml")
    }
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

/// Load the project MCP config.
///
/// Tries `.mcp.json` first (Claude Code convention), falls back to `.nika/mcp.yaml`.
/// Returns `None` if neither file exists, `Err` on parse errors.
pub fn load_project_config() -> Result<Option<McpConfig>, McpConfigError> {
    let root = match find_project_root() {
        Some(r) => r,
        None => return Ok(None),
    };

    // Try .mcp.json first
    let mcp_json_path = root.join(".mcp.json");
    if mcp_json_path.exists() {
        return load_mcp_json(&mcp_json_path).map(Some);
    }

    // Fallback: .nika/mcp.yaml
    load_config_from_path(Some(root.join(".nika").join("mcp.yaml")))
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

/// Save the project MCP config.
///
/// Writes to `.mcp.json` if it exists at project root, otherwise `.nika/mcp.yaml`.
pub fn save_project_config(config: &McpConfig) -> Result<(), McpConfigError> {
    let root = find_project_root().ok_or(McpConfigError::NoProjectRoot)?;
    let mcp_json_path = root.join(".mcp.json");

    if mcp_json_path.exists() {
        save_mcp_json(config, &mcp_json_path)
    } else {
        let yaml_path = root.join(".nika").join("mcp.yaml");
        save_config_to_path(config, &yaml_path)
    }
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
// .mcp.json SUPPORT (Claude Code Convention)
// ═══════════════════════════════════════════════════════════════════════════════

/// Claude Code `.mcp.json` format (used internally for parse/write).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpJsonFile {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpJsonServer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct McpJsonServer {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    env: HashMap<String, String>,
}

/// Load an MCP config from a `.mcp.json` file, converting to internal `McpConfig`.
fn load_mcp_json(path: &Path) -> Result<McpConfig, McpConfigError> {
    let content = std::fs::read_to_string(path).map_err(|e| McpConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;

    let json: McpJsonFile = serde_json::from_str(&content).map_err(|e| McpConfigError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    let mut servers = HashMap::new();
    for (name, s) in json.mcp_servers {
        servers.insert(
            name,
            McpServer {
                command: s.command,
                args: s.args,
                env: s.env,
                description: None,
                enabled: true,
                source: Some(McpSource::Project),
            },
        );
    }

    Ok(McpConfig {
        version: 1,
        servers,
    })
}

/// Save an MCP config to a `.mcp.json` file (Claude Code convention).
fn save_mcp_json(config: &McpConfig, path: &Path) -> Result<(), McpConfigError> {
    let mut mcp_servers = HashMap::new();
    for (name, server) in &config.servers {
        mcp_servers.insert(
            name.clone(),
            McpJsonServer {
                command: server.command.clone(),
                args: server.args.clone(),
                env: server.env.clone(),
            },
        );
    }

    let json = McpJsonFile { mcp_servers };
    let content = serde_json::to_string_pretty(&json)
        .map_err(|e| McpConfigError::Serialize(e.to_string()))?;

    std::fs::write(path, format!("{content}\n")).map_err(|e| McpConfigError::Io {
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
// RESOLVER (for from: references in workflows)
// ═══════════════════════════════════════════════════════════════════════════════

/// Resolves MCP server references from config files.
///
/// Loads config files once and caches them. Used by the lowering phase to
/// resolve `from: config` / `from: project` / `from: global` references.
#[derive(Debug, Clone, Default)]
pub struct McpConfigResolver {
    project: Option<McpConfig>,
    global: Option<McpConfig>,
}

impl McpConfigResolver {
    /// Create a resolver that loads configs from the standard paths.
    pub fn from_environment() -> Self {
        let project = load_project_config().ok().flatten();
        let global = load_global_config().ok().flatten();
        Self { project, global }
    }

    /// Create a resolver with explicit configs (for testing).
    pub fn with_configs(project: Option<McpConfig>, global: Option<McpConfig>) -> Self {
        Self { project, global }
    }

    /// Resolve a server by name from the specified source.
    ///
    /// Returns the McpServer if found, or None if the server doesn't exist
    /// in the requested scope.
    pub fn resolve(&self, name: &str, source: McpResolveSource) -> Option<&McpServer> {
        match source {
            McpResolveSource::Config => {
                // Project > global
                self.project
                    .as_ref()
                    .and_then(|c| c.servers.get(name))
                    .or_else(|| self.global.as_ref().and_then(|c| c.servers.get(name)))
            }
            McpResolveSource::Project => self.project.as_ref().and_then(|c| c.servers.get(name)),
            McpResolveSource::Global => self.global.as_ref().and_then(|c| c.servers.get(name)),
        }
    }

    /// Check if the resolver has any config loaded.
    pub fn has_configs(&self) -> bool {
        self.project.is_some() || self.global.is_some()
    }
}

/// Source for resolver lookups (mirrors McpFromSource from nika-core AST).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpResolveSource {
    /// .mcp.json > .nika/mcp.yaml > ~/.nika/mcp.yaml
    Config,
    /// .mcp.json or .nika/mcp.yaml only
    Project,
    /// ~/.nika/mcp.yaml only
    Global,
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

    // ═══════════════════════════════════════════════════════════════════════════
    // McpConfigResolver tests
    // ═══════════════════════════════════════════════════════════════════════════

    fn make_config(servers: Vec<(&str, &str)>) -> McpConfig {
        let mut config = McpConfig {
            version: 1,
            ..McpConfig::default()
        };
        for (name, cmd) in servers {
            config.servers.insert(
                name.to_string(),
                McpServer {
                    command: cmd.to_string(),
                    args: vec![],
                    env: HashMap::new(),
                    description: None,
                    enabled: true,
                    source: None,
                },
            );
        }
        config
    }

    #[test]
    fn test_resolver_config_project_over_global() {
        let project = make_config(vec![("neo4j", "project-cmd")]);
        let global = make_config(vec![("neo4j", "global-cmd"), ("perplexity", "pplx-cmd")]);
        let resolver = McpConfigResolver::with_configs(Some(project), Some(global));

        // Config source: project wins for neo4j
        let neo4j = resolver.resolve("neo4j", McpResolveSource::Config).unwrap();
        assert_eq!(neo4j.command, "project-cmd");

        // Config source: perplexity only in global
        let pplx = resolver
            .resolve("perplexity", McpResolveSource::Config)
            .unwrap();
        assert_eq!(pplx.command, "pplx-cmd");
    }

    #[test]
    fn test_resolver_project_scope_only() {
        let project = make_config(vec![("neo4j", "project-cmd")]);
        let global = make_config(vec![("perplexity", "global-cmd")]);
        let resolver = McpConfigResolver::with_configs(Some(project), Some(global));

        // Project scope: neo4j found
        assert!(resolver
            .resolve("neo4j", McpResolveSource::Project)
            .is_some());
        // Project scope: perplexity NOT found (it's global only)
        assert!(resolver
            .resolve("perplexity", McpResolveSource::Project)
            .is_none());
    }

    #[test]
    fn test_resolver_global_scope_only() {
        let project = make_config(vec![("neo4j", "project-cmd")]);
        let global = make_config(vec![("perplexity", "global-cmd")]);
        let resolver = McpConfigResolver::with_configs(Some(project), Some(global));

        // Global scope: perplexity found
        assert!(resolver
            .resolve("perplexity", McpResolveSource::Global)
            .is_some());
        // Global scope: neo4j NOT found (it's project only)
        assert!(resolver
            .resolve("neo4j", McpResolveSource::Global)
            .is_none());
    }

    #[test]
    fn test_resolver_not_found() {
        let resolver = McpConfigResolver::with_configs(None, None);
        assert!(resolver
            .resolve("neo4j", McpResolveSource::Config)
            .is_none());
        assert!(!resolver.has_configs());
    }
}
