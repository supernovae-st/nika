//! SuperNovae CLI MCP Configuration Loader
//!
//! Loads MCP server configurations from `~/.nika/mcp.yaml` - the single source
//! of truth for MCP servers in the SuperNovae ecosystem.
//!
//! This module bridges the global MCP configuration with Nika's MCP client,
//! allowing workflows to use globally configured MCP servers without duplication.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::nika_config::{load_nika_mcp_servers, NikaMcpConfigManager};
//!
//! // Load all enabled servers from ~/.nika/mcp.yaml
//! let servers = load_nika_mcp_servers()?;
//! for (name, config) in &servers {
//!     println!("Server: {} -> {}", name, config.command);
//! }
//!
//! // Or use the manager for more control
//! let manager = NikaMcpConfigManager::new();
//! let config = manager.load_global()?;
//! println!("Found {} servers", config.servers.len());
//! ```
//!
//! ## File Format
//!
//! The `~/.nika/mcp.yaml` file follows this format:
//!
//! ```yaml
//! version: 1
//! servers:
//!   neo4j:
//!     command: npx
//!     args: ["-y", "@neo4j/mcp-server"]
//!     env:
//!       NEO4J_URI: bolt://localhost:7687
//!     description: "Neo4j graph database"
//!     enabled: true
//!   novanet:
//!     command: novanet-mcp
//!     enabled: true
//! ```
//!
//! ## Integration with Nika Workflows
//!
//! Workflows can use globally-configured servers by name:
//!
//! ```yaml
//! schema: nika/workflow@0.9
//! workflow: example
//!
//! # Optional: explicitly list which servers to use
//! # If omitted, all enabled servers are available
//! mcp_servers:
//!   - neo4j
//!   - novanet
//!
//! tasks:
//!   - id: query
//!     invoke:
//!       mcp: neo4j  # Uses server from ~/.nika/mcp.yaml
//!       tool: query
//!       params:
//!         cypher: "MATCH (n) RETURN n LIMIT 10"
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use super::types::McpConfig;
use crate::error::McpError;
use serde_saphyr as serde_yaml;

// ═══════════════════════════════════════════════════════════════════════════════
// NIKA MCP Configuration Types
// ═══════════════════════════════════════════════════════════════════════════════

/// Root configuration for MCP servers from `~/.nika/mcp.yaml`.
///
/// This mirrors the structure used by the Nika MCP Config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NikaMcpConfig {
    /// Configuration version for migrations.
    #[serde(default = "default_version")]
    pub version: u32,

    /// MCP server definitions.
    #[serde(default)]
    pub servers: HashMap<String, NikaMcpServer>,
}

fn default_version() -> u32 {
    1
}

/// Individual MCP server configuration.
///
/// This is the format used in `~/.nika/mcp.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NikaMcpServer {
    /// Command to execute (e.g., "npx", "node", "novanet-mcp").
    pub command: String,

    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,

    /// Environment variables for the server process.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Optional description for documentation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Whether this server is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Source of this server (global, project, or workflow).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<NikaMcpSource>,
}

fn default_enabled() -> bool {
    true
}

/// Source of an MCP server configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NikaMcpSource {
    /// Global configuration (~/.nika/mcp.yaml).
    #[default]
    Global,
    /// Project configuration (.nika/mcp.yaml).
    Project,
    /// Workflow-level configuration (inline in workflow.nika.yaml).
    Workflow,
}

impl NikaMcpServer {
    /// Convert to Nika's McpConfig format.
    pub fn to_mcp_config(&self, name: &str) -> McpConfig {
        let mut env = FxHashMap::default();
        for (k, v) in &self.env {
            env.insert(k.clone(), v.clone());
        }

        McpConfig {
            name: name.to_string(),
            command: self.command.clone(),
            args: self.args.clone(),
            env,
            cwd: None, // mcp.yaml doesn't have cwd, default to None
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Configuration Manager
// ═══════════════════════════════════════════════════════════════════════════════

/// Manager for loading MCP configurations from config files.
///
/// Handles reading from:
/// - Global: `~/.nika/mcp.yaml`
/// - Project: `.nika/mcp.yaml` (relative to project root)
#[derive(Debug, Clone)]
pub struct NikaMcpConfigManager {
    global_path: PathBuf,
    project_root: Option<PathBuf>,
}

impl NikaMcpConfigManager {
    /// Create a new config manager with default paths.
    pub fn new() -> Self {
        Self {
            global_path: Self::default_global_path(),
            project_root: None,
        }
    }

    /// Create a config manager with a specific project root.
    pub fn with_project(project_root: PathBuf) -> Self {
        Self {
            global_path: Self::default_global_path(),
            project_root: Some(project_root),
        }
    }

    /// Create a config manager with a custom global path (for testing).
    pub fn with_global_path(global_path: PathBuf) -> Self {
        Self {
            global_path,
            project_root: None,
        }
    }

    /// Get the default global config path (~/.nika/mcp.yaml).
    pub fn default_global_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".nika")
            .join("mcp.yaml")
    }

    /// Get the project config path (.nika/mcp.yaml relative to project root).
    pub fn project_path(&self) -> Option<PathBuf> {
        self.project_root
            .as_ref()
            .map(|root| root.join(".nika").join("mcp.yaml"))
    }

    /// Check if the global config file exists.
    pub fn global_exists(&self) -> bool {
        self.global_path.exists()
    }

    /// Load global MCP configuration from ~/.nika/mcp.yaml.
    pub fn load_global(&self) -> Result<NikaMcpConfig, McpError> {
        Self::load_from_path(&self.global_path)
    }

    /// Load project MCP configuration from .nika/mcp.yaml.
    pub fn load_project(&self) -> Result<Option<NikaMcpConfig>, McpError> {
        let Some(path) = self.project_path() else {
            return Ok(None);
        };

        if !path.exists() {
            return Ok(None);
        }

        Self::load_from_path(&path).map(Some)
    }

    /// Load and merge configurations (global + project).
    ///
    /// Project servers override global servers with the same name.
    pub fn load_merged(&self) -> Result<NikaMcpConfig, McpError> {
        let mut merged = self.load_global()?;

        if let Some(project) = self.load_project()? {
            for (name, server) in project.servers {
                merged.servers.insert(name, server);
            }
        }

        Ok(merged)
    }

    /// Load configuration from a specific path.
    fn load_from_path(path: &Path) -> Result<NikaMcpConfig, McpError> {
        if !path.exists() {
            return Ok(NikaMcpConfig::default());
        }

        let content = std::fs::read_to_string(path).map_err(|e| McpError::ConfigError {
            reason: format!("Failed to read MCP config at '{}': {}", path.display(), e),
        })?;

        serde_yaml::from_str(&content).map_err(|e| McpError::ParseError {
            details: format!("Invalid MCP config YAML at '{}': {}", path.display(), e),
        })
    }
}

impl Default for NikaMcpConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Convenience Functions
// ═══════════════════════════════════════════════════════════════════════════════

/// Load all enabled MCP servers from ~/.nika/mcp.yaml.
///
/// Returns a HashMap of server name -> McpConfig, ready for use with Nika's
/// MCP client.
///
/// # Example
///
/// ```rust,ignore
/// use nika::mcp::nika_config::load_nika_mcp_servers;
///
/// let servers = load_nika_mcp_servers()?;
/// for (name, config) in &servers {
///     println!("Found server: {} ({})", name, config.command);
/// }
/// ```
pub fn load_nika_mcp_servers() -> Result<FxHashMap<String, McpConfig>, McpError> {
    let manager = NikaMcpConfigManager::new();
    load_nika_mcp_servers_with_manager(&manager)
}

/// Load all enabled MCP servers using a specific manager.
///
/// This allows customizing the config paths for testing.
pub fn load_nika_mcp_servers_with_manager(
    manager: &NikaMcpConfigManager,
) -> Result<FxHashMap<String, McpConfig>, McpError> {
    let config = manager.load_merged()?;
    let mut servers = FxHashMap::default();

    for (name, server) in config.servers {
        // Skip disabled servers
        if !server.enabled {
            continue;
        }

        servers.insert(name.clone(), server.to_mcp_config(&name));
    }

    Ok(servers)
}

/// Load specific MCP servers by name from ~/.nika/mcp.yaml.
///
/// Returns only the requested servers that are enabled. Returns an error
/// if any requested server is not found.
///
/// # Example
///
/// ```rust,ignore
/// use nika::mcp::nika_config::load_nika_mcp_servers_by_name;
///
/// let servers = load_nika_mcp_servers_by_name(&["neo4j", "novanet"])?;
/// ```
pub fn load_nika_mcp_servers_by_name(
    names: &[&str],
) -> Result<FxHashMap<String, McpConfig>, McpError> {
    let manager = NikaMcpConfigManager::new();
    let config = manager.load_merged()?;
    let mut servers = FxHashMap::default();
    let mut missing = Vec::new();

    for &name in names {
        match config.servers.get(name) {
            Some(server) if server.enabled => {
                servers.insert(name.to_string(), server.to_mcp_config(name));
            }
            Some(_) => {
                // Server exists but is disabled - treat as not found
                missing.push(name);
            }
            None => {
                missing.push(name);
            }
        }
    }

    if !missing.is_empty() {
        let available: Vec<_> = config
            .servers
            .keys()
            .filter(|k| config.servers.get(*k).map(|s| s.enabled).unwrap_or(false))
            .cloned()
            .collect();
        return Err(McpError::ConfigError {
            reason: format!(
                "MCP server(s) not found in config: [{}]. Available: [{}]",
                missing.join(", "),
                available.join(", ")
            ),
        });
    }

    Ok(servers)
}

/// Check if the MCP config file exists.
///
/// Returns true if ~/.nika/mcp.yaml exists.
pub fn nika_mcp_config_exists() -> bool {
    NikaMcpConfigManager::new().global_exists()
}

/// List available MCP server names from ~/.nika/mcp.yaml.
///
/// Returns only enabled servers.
pub fn list_nika_mcp_servers() -> Result<Vec<String>, McpError> {
    let manager = NikaMcpConfigManager::new();
    let config = manager.load_merged()?;

    Ok(config
        .servers
        .into_iter()
        .filter(|(_, s)| s.enabled)
        .map(|(name, _)| name)
        .collect())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(content: &str) -> (TempDir, NikaMcpConfigManager) {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("mcp.yaml");
        std::fs::write(&config_path, content).unwrap();

        let manager = NikaMcpConfigManager::with_global_path(config_path);
        (temp, manager)
    }

    #[test]
    fn test_load_empty_config() {
        let (_temp, manager) = create_test_config("");
        let config = manager.load_global().unwrap();
        assert!(config.servers.is_empty());
        assert_eq!(config.version, 1);
    }

    #[test]
    fn test_load_single_server() {
        let yaml = r#"
version: 1
servers:
  neo4j:
    command: npx
    args:
      - "-y"
      - "@neo4j/mcp-server"
    env:
      NEO4J_URI: bolt://localhost:7687
    enabled: true
"#;

        let (_temp, manager) = create_test_config(yaml);
        let config = manager.load_global().unwrap();

        assert_eq!(config.version, 1);
        assert_eq!(config.servers.len(), 1);

        let server = config.servers.get("neo4j").unwrap();
        assert_eq!(server.command, "npx");
        assert_eq!(server.args, vec!["-y", "@neo4j/mcp-server"]);
        assert_eq!(
            server.env.get("NEO4J_URI"),
            Some(&"bolt://localhost:7687".to_string())
        );
        assert!(server.enabled);
    }

    #[test]
    fn test_load_multiple_servers() {
        let yaml = r#"
version: 1
servers:
  neo4j:
    command: npx
    args: ["-y", "@neo4j/mcp-server"]
    enabled: true
  novanet:
    command: novanet-mcp
    enabled: true
  disabled_server:
    command: echo
    enabled: false
"#;

        let (_temp, manager) = create_test_config(yaml);
        let config = manager.load_global().unwrap();

        assert_eq!(config.servers.len(), 3);
        assert!(config.servers.contains_key("neo4j"));
        assert!(config.servers.contains_key("novanet"));
        assert!(config.servers.contains_key("disabled_server"));
    }

    #[test]
    fn test_convert_to_mcp_config() {
        let server = NikaMcpServer {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@test/server".to_string()],
            env: {
                let mut env = HashMap::new();
                env.insert("API_KEY".to_string(), "secret".to_string());
                env
            },
            description: Some("Test server".to_string()),
            enabled: true,
            source: Some(NikaMcpSource::Global),
        };

        let config = server.to_mcp_config("test");

        assert_eq!(config.name, "test");
        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "@test/server"]);
        assert_eq!(config.env.get("API_KEY"), Some(&"secret".to_string()));
        assert!(config.cwd.is_none());
    }

    #[test]
    fn test_load_enabled_servers_only() {
        let yaml = r#"
version: 1
servers:
  enabled_server:
    command: echo
    enabled: true
  disabled_server:
    command: echo
    enabled: false
"#;

        let (_temp, manager) = create_test_config(yaml);
        let servers = load_nika_mcp_servers_with_manager(&manager).unwrap();

        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("enabled_server"));
        assert!(!servers.contains_key("disabled_server"));
    }

    #[test]
    fn test_load_servers_by_name() {
        let yaml = r#"
version: 1
servers:
  neo4j:
    command: npx
    enabled: true
  novanet:
    command: novanet-mcp
    enabled: true
  other:
    command: echo
    enabled: true
"#;

        let (temp, manager) = create_test_config(yaml);

        // Override the global path temporarily
        let config_path = temp.path().join("mcp.yaml");
        std::env::set_var("SPN_MCP_CONFIG_PATH", config_path.to_str().unwrap());

        // Test loading specific servers
        let config = manager.load_global().unwrap();
        let mut servers = FxHashMap::default();

        for name in ["neo4j", "novanet"] {
            if let Some(server) = config.servers.get(name) {
                if server.enabled {
                    servers.insert(name.to_string(), server.to_mcp_config(name));
                }
            }
        }

        assert_eq!(servers.len(), 2);
        assert!(servers.contains_key("neo4j"));
        assert!(servers.contains_key("novanet"));
        assert!(!servers.contains_key("other"));
    }

    #[test]
    fn test_default_enabled() {
        let yaml = r#"
version: 1
servers:
  no_enabled_field:
    command: echo
"#;

        let (_temp, manager) = create_test_config(yaml);
        let config = manager.load_global().unwrap();

        let server = config.servers.get("no_enabled_field").unwrap();
        assert!(server.enabled); // Default is true
    }

    #[test]
    fn test_missing_config_returns_empty() {
        let temp = TempDir::new().unwrap();
        let nonexistent = temp.path().join("nonexistent.yaml");
        let manager = NikaMcpConfigManager::with_global_path(nonexistent);

        let config = manager.load_global().unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_source_enum() {
        let yaml = r#"
version: 1
servers:
  test:
    command: echo
    source: global
    enabled: true
"#;

        let (_temp, manager) = create_test_config(yaml);
        let config = manager.load_global().unwrap();

        let server = config.servers.get("test").unwrap();
        assert_eq!(server.source, Some(NikaMcpSource::Global));
    }

    #[test]
    fn test_env_variables() {
        let yaml = r#"
version: 1
servers:
  test:
    command: echo
    env:
      VAR1: value1
      VAR2: value2
      VAR3: "value with spaces"
    enabled: true
"#;

        let (_temp, manager) = create_test_config(yaml);
        let config = manager.load_global().unwrap();

        let server = config.servers.get("test").unwrap();
        assert_eq!(server.env.get("VAR1"), Some(&"value1".to_string()));
        assert_eq!(server.env.get("VAR2"), Some(&"value2".to_string()));
        assert_eq!(
            server.env.get("VAR3"),
            Some(&"value with spaces".to_string())
        );
    }

    #[test]
    fn test_config_manager_paths() {
        let manager = NikaMcpConfigManager::new();

        let expected_global = dirs::home_dir().unwrap().join(".nika").join("mcp.yaml");
        assert_eq!(manager.global_path, expected_global);
        assert!(manager.project_root.is_none());
    }

    #[test]
    fn test_config_manager_with_project() {
        let project_root = PathBuf::from("/my/project");
        let manager = NikaMcpConfigManager::with_project(project_root.clone());

        assert_eq!(manager.project_root, Some(project_root));
        assert_eq!(
            manager.project_path(),
            Some(PathBuf::from("/my/project/.nika/mcp.yaml"))
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════════
    // Tests for secrets sharing
    // ═══════════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_secrets_env_var_references_preserved() {
        // Verify that ${VAR} references are preserved when loading
        let yaml = r#"
version: 1
servers:
  perplexity:
    command: npx
    args: ["-y", "@anthropic/mcp-server-perplexity"]
    env:
      PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
    enabled: true
"#;

        let (_temp, manager) = create_test_config(yaml);
        let config = manager.load_global().unwrap();

        let server = config.servers.get("perplexity").unwrap();
        // The ${VAR} reference should be preserved as-is (shell will expand it)
        assert_eq!(
            server.env.get("PERPLEXITY_API_KEY"),
            Some(&"${PERPLEXITY_API_KEY}".to_string())
        );
    }

    #[test]
    fn test_multiple_secrets_per_server() {
        // Test that servers with multiple secrets (like Neo4j) load correctly
        let yaml = r#"
version: 1
servers:
  neo4j:
    command: npx
    args: ["-y", "@neo4j/mcp-server-neo4j"]
    env:
      NEO4J_URI: bolt://localhost:7687
      NEO4J_USER: neo4j
      NEO4J_PASSWORD: "${NEO4J_PASSWORD}"
    enabled: true
"#;

        let (_temp, manager) = create_test_config(yaml);
        let servers = load_nika_mcp_servers_with_manager(&manager).unwrap();

        let mcp_config = servers.get("neo4j").unwrap();
        assert_eq!(
            mcp_config.env.get("NEO4J_URI"),
            Some(&"bolt://localhost:7687".to_string())
        );
        assert_eq!(mcp_config.env.get("NEO4J_USER"), Some(&"neo4j".to_string()));
        assert_eq!(
            mcp_config.env.get("NEO4J_PASSWORD"),
            Some(&"${NEO4J_PASSWORD}".to_string())
        );
    }

    #[test]
    fn test_spn_mcp_server_types_with_secrets() {
        // Test all common MCP server types that use secrets from nika
        let yaml = r#"
version: 1
servers:
  neo4j:
    command: npx
    args: ["-y", "@neo4j/mcp-server-neo4j"]
    env:
      NEO4J_PASSWORD: "${NEO4J_PASSWORD}"
    enabled: true
  perplexity:
    command: npx
    args: ["-y", "@anthropic/mcp-server-perplexity"]
    env:
      PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
    enabled: true
  firecrawl:
    command: npx
    args: ["-y", "@anthropic/mcp-server-firecrawl"]
    env:
      FIRECRAWL_API_KEY: "${FIRECRAWL_API_KEY}"
    enabled: true
  supadata:
    command: npx
    args: ["-y", "@supadata/mcp-server"]
    env:
      SUPADATA_API_KEY: "${SUPADATA_API_KEY}"
    enabled: true
  github:
    command: npx
    args: ["-y", "@anthropic/mcp-server-github"]
    env:
      GITHUB_TOKEN: "${GITHUB_TOKEN}"
    enabled: true
  slack:
    command: npx
    args: ["-y", "@anthropic/mcp-server-slack"]
    env:
      SLACK_BOT_TOKEN: "${SLACK_BOT_TOKEN}"
      SLACK_TEAM_ID: "${SLACK_TEAM_ID}"
    enabled: true
"#;

        let (_temp, manager) = create_test_config(yaml);
        let servers = load_nika_mcp_servers_with_manager(&manager).unwrap();

        // Verify all 6 servers loaded
        assert_eq!(servers.len(), 6);

        // Verify each server has its secret reference preserved
        assert!(servers
            .get("neo4j")
            .unwrap()
            .env
            .contains_key("NEO4J_PASSWORD"));
        assert!(servers
            .get("perplexity")
            .unwrap()
            .env
            .contains_key("PERPLEXITY_API_KEY"));
        assert!(servers
            .get("firecrawl")
            .unwrap()
            .env
            .contains_key("FIRECRAWL_API_KEY"));
        assert!(servers
            .get("supadata")
            .unwrap()
            .env
            .contains_key("SUPADATA_API_KEY"));
        assert!(servers
            .get("github")
            .unwrap()
            .env
            .contains_key("GITHUB_TOKEN"));
        assert!(servers
            .get("slack")
            .unwrap()
            .env
            .contains_key("SLACK_BOT_TOKEN"));
        assert!(servers
            .get("slack")
            .unwrap()
            .env
            .contains_key("SLACK_TEAM_ID"));
    }

    #[test]
    fn test_mcp_config_conversion_preserves_secrets() {
        // Test that to_mcp_config() preserves secret references
        let server = NikaMcpServer {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@test/server".to_string()],
            env: {
                let mut env = HashMap::new();
                env.insert("API_KEY".to_string(), "${API_KEY}".to_string());
                env.insert("STATIC_VALUE".to_string(), "fixed_value".to_string());
                env
            },
            description: None,
            enabled: true,
            source: None,
        };

        let mcp_config = server.to_mcp_config("test");

        // Secret reference preserved
        assert_eq!(
            mcp_config.env.get("API_KEY"),
            Some(&"${API_KEY}".to_string())
        );
        // Static value preserved
        assert_eq!(
            mcp_config.env.get("STATIC_VALUE"),
            Some(&"fixed_value".to_string())
        );
    }

    #[test]
    fn test_project_config_merges_with_global_secrets() {
        // Test that project MCP config can override/extend global config with secrets
        let temp = TempDir::new().unwrap();

        // Create global config
        let global_path = temp.path().join("global_mcp.yaml");
        std::fs::write(
            &global_path,
            r#"
version: 1
servers:
  perplexity:
    command: npx
    args: ["-y", "@anthropic/mcp-server-perplexity"]
    env:
      PERPLEXITY_API_KEY: "${PERPLEXITY_API_KEY}"
    enabled: true
"#,
        )
        .unwrap();

        // Create project config with additional server
        let project_root = temp.path().join("project");
        std::fs::create_dir_all(project_root.join(".nika")).unwrap();
        let project_path = project_root.join(".nika/mcp.yaml");
        std::fs::write(
            &project_path,
            r#"
version: 1
servers:
  neo4j:
    command: npx
    args: ["-y", "@neo4j/mcp-server-neo4j"]
    env:
      NEO4J_PASSWORD: "${NEO4J_PASSWORD}"
    enabled: true
"#,
        )
        .unwrap();

        // Create manager with both paths
        let mut manager = NikaMcpConfigManager::with_global_path(global_path);
        manager.project_root = Some(project_root);

        let merged = manager.load_merged().unwrap();

        // Both servers should be present
        assert_eq!(merged.servers.len(), 2);
        assert!(merged.servers.contains_key("perplexity"));
        assert!(merged.servers.contains_key("neo4j"));

        // Both secrets should be preserved
        assert_eq!(
            merged
                .servers
                .get("perplexity")
                .unwrap()
                .env
                .get("PERPLEXITY_API_KEY"),
            Some(&"${PERPLEXITY_API_KEY}".to_string())
        );
        assert_eq!(
            merged
                .servers
                .get("neo4j")
                .unwrap()
                .env
                .get("NEO4J_PASSWORD"),
            Some(&"${NEO4J_PASSWORD}".to_string())
        );
    }

    #[test]
    fn test_disabled_server_secrets_not_loaded() {
        // Verify that disabled servers (and their secrets) are not loaded
        let yaml = r#"
version: 1
servers:
  active:
    command: npx
    env:
      SECRET: "${SECRET}"
    enabled: true
  disabled:
    command: npx
    env:
      DISABLED_SECRET: "${DISABLED_SECRET}"
    enabled: false
"#;

        let (_temp, manager) = create_test_config(yaml);
        let servers = load_nika_mcp_servers_with_manager(&manager).unwrap();

        assert_eq!(servers.len(), 1);
        assert!(servers.contains_key("active"));
        assert!(!servers.contains_key("disabled"));

        // Active server secret is present
        assert!(servers.get("active").unwrap().env.contains_key("SECRET"));
    }
}
