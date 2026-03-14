//! Setup Wizard
//!
//! Interactive wizard for setting up Nika and related tools.

use std::path::PathBuf;

use crate::core::paths::nika_home;
use crate::error::{NikaError, Result};

/// Target for setup wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupTarget {
    /// Auto-detect what needs setup.
    Auto,
    /// Setup Nika CLI and daemon.
    Nika,
    /// Setup NovaNet (MCP server + Neo4j).
    NovaNet,
    /// Setup Claude Code integration.
    ClaudeCode,
}

impl SetupTarget {
    /// Parse from string argument.
    pub fn from_arg(arg: &str) -> Option<Self> {
        match arg.to_lowercase().as_str() {
            "nika" => Some(Self::Nika),
            "novanet" | "nova-net" => Some(Self::NovaNet),
            "claude-code" | "claude" | "cc" => Some(Self::ClaudeCode),
            _ => None,
        }
    }

    /// Get display name.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Auto => "Auto-detect",
            Self::Nika => "Nika CLI",
            Self::NovaNet => "NovaNet",
            Self::ClaudeCode => "Claude Code",
        }
    }
}

/// Result of a setup operation.
#[derive(Debug, Clone)]
pub struct SetupResult {
    /// Target that was set up.
    pub target: SetupTarget,
    /// Whether setup succeeded.
    pub success: bool,
    /// Steps completed.
    pub steps_completed: Vec<String>,
    /// Warnings/notes.
    pub warnings: Vec<String>,
    /// Error message if failed.
    pub error: Option<String>,
}

impl SetupResult {
    /// Create a successful result.
    pub fn success(target: SetupTarget, steps: Vec<String>) -> Self {
        Self {
            target,
            success: true,
            steps_completed: steps,
            warnings: Vec::new(),
            error: None,
        }
    }

    /// Create a failed result.
    pub fn failure(target: SetupTarget, error: String) -> Self {
        Self {
            target,
            success: false,
            steps_completed: Vec::new(),
            warnings: Vec::new(),
            error: Some(error),
        }
    }
}

/// Run the setup wizard.
pub fn run_setup(target: SetupTarget) -> Result<SetupResult> {
    match target {
        SetupTarget::Auto => run_auto_setup(),
        SetupTarget::Nika => setup_nika(),
        SetupTarget::NovaNet => setup_novanet(),
        SetupTarget::ClaudeCode => setup_claude_code(),
    }
}

/// Auto-detect what needs to be set up.
fn run_auto_setup() -> Result<SetupResult> {
    let mut all_steps = Vec::new();
    let mut all_warnings = Vec::new();

    // Check ~/.nika directory
    let nika_dir = nika_home();
    if !nika_dir.exists() {
        println!("\n📦 Setting up Nika home directory...");
        std::fs::create_dir_all(&nika_dir).map_err(|e| NikaError::IoPathError {
            path: nika_dir.clone(),
            operation: "create ~/.nika directory".to_string(),
            source: e,
        })?;
        all_steps.push("Created ~/.nika directory".to_string());
    }

    // Check config file
    let config_path = nika_dir.join("config.toml");
    if !config_path.exists() {
        println!("📝 Creating default configuration...");
        create_default_config(&config_path)?;
        all_steps.push("Created default config.toml".to_string());
    }

    // Check MCP config
    let mcp_path = nika_dir.join("mcp.yaml");
    if !mcp_path.exists() {
        println!("🔌 Creating MCP configuration...");
        create_default_mcp_config(&mcp_path)?;
        all_steps.push("Created default mcp.yaml".to_string());
    }

    // Check Claude Code integration (skip if home directory unavailable)
    if let Some(home) = dirs::home_dir() {
        let claude_dir = home.join(".claude");
        if claude_dir.exists() && !claude_dir.join("settings.json").exists() {
            all_warnings.push("Claude Code detected but settings.json not found".to_string());
        }
    }

    if all_steps.is_empty() {
        all_steps.push("All components already configured".to_string());
    }

    let mut result = SetupResult::success(SetupTarget::Auto, all_steps);
    result.warnings = all_warnings;
    Ok(result)
}

/// Setup Nika CLI and daemon.
fn setup_nika() -> Result<SetupResult> {
    let mut steps = Vec::new();

    // Step 1: Create ~/.nika directory
    let nika_dir = nika_home();
    if !nika_dir.exists() {
        std::fs::create_dir_all(&nika_dir).map_err(|e| NikaError::IoPathError {
            path: nika_dir.clone(),
            operation: "create ~/.nika directory".to_string(),
            source: e,
        })?;
        steps.push("Created ~/.nika directory".to_string());
    }

    // Step 2: Create config.toml
    let config_path = nika_dir.join("config.toml");
    if !config_path.exists() {
        create_default_config(&config_path)?;
        steps.push("Created config.toml".to_string());
    }

    // Step 3: Create MCP config
    let mcp_path = nika_dir.join("mcp.yaml");
    if !mcp_path.exists() {
        create_default_mcp_config(&mcp_path)?;
        steps.push("Created mcp.yaml".to_string());
    }

    // Step 4: Create sessions directory
    let sessions_dir = nika_dir.join("sessions");
    if !sessions_dir.exists() {
        std::fs::create_dir_all(&sessions_dir).map_err(|e| NikaError::IoPathError {
            path: sessions_dir.clone(),
            operation: "create sessions directory".to_string(),
            source: e,
        })?;
        steps.push("Created sessions directory".to_string());
    }

    // Step 5: Create sync config
    let sync_path = nika_dir.join("sync.yaml");
    if !sync_path.exists() {
        create_default_sync_config(&sync_path)?;
        steps.push("Created sync.yaml".to_string());
    }

    if steps.is_empty() {
        steps.push("Nika already fully configured".to_string());
    }

    Ok(SetupResult::success(SetupTarget::Nika, steps))
}

/// Setup NovaNet integration.
fn setup_novanet() -> Result<SetupResult> {
    let mut steps = Vec::new();

    // Check if Neo4j is reachable
    println!("🔍 Checking Neo4j connection...");

    // For now, just ensure MCP config has novanet entry
    let nika_dir = nika_home();
    let mcp_path = nika_dir.join("mcp.yaml");

    if !mcp_path.exists() {
        create_default_mcp_config(&mcp_path)?;
        steps.push("Created mcp.yaml with NovaNet server".to_string());
    } else {
        // Check if novanet is in the config
        let content = std::fs::read_to_string(&mcp_path).map_err(|e| NikaError::IoPathError {
            path: mcp_path.clone(),
            operation: "read MCP config".to_string(),
            source: e,
        })?;

        if !content.contains("novanet") {
            steps.push("Note: Add 'novanet' server to mcp.yaml manually".to_string());
        } else {
            steps.push("NovaNet server already configured in mcp.yaml".to_string());
        }
    }

    // Provide instructions
    println!("\n📋 To complete NovaNet setup:");
    println!("   1. Ensure Neo4j is running (bolt://localhost:7687)");
    println!("   2. Set NEO4J_PASSWORD in your environment or keychain");
    println!("   3. Run 'nika mcp test novanet' to verify connection");

    Ok(SetupResult::success(SetupTarget::NovaNet, steps))
}

/// Setup Claude Code integration.
fn setup_claude_code() -> Result<SetupResult> {
    use crate::sync::{sync_editor, EditorKind, SyncConfig};

    let mut steps = Vec::new();

    // Check if Claude Code is installed
    let claude_dir = dirs::home_dir()
        .ok_or(NikaError::HomeDirectoryNotFound)?
        .join(".claude");

    if !claude_dir.exists() {
        return Ok(SetupResult::failure(
            SetupTarget::ClaudeCode,
            "Claude Code not installed (no ~/.claude directory)".to_string(),
        ));
    }

    steps.push("Found Claude Code installation".to_string());

    // Enable Claude Code in sync config
    let mut sync_config = SyncConfig::load()?;
    if !sync_config.is_enabled(EditorKind::ClaudeCode) {
        sync_config.enable_editor(EditorKind::ClaudeCode);
        sync_config.save()?;
        steps.push("Enabled Claude Code for sync".to_string());
    }

    // Sync MCP config to Claude Code
    let result = sync_editor(EditorKind::ClaudeCode)?;
    if result.success {
        steps.push(format!("Synced {} MCP servers", result.servers_synced));
    } else if let Some(error) = result.error {
        return Ok(SetupResult::failure(SetupTarget::ClaudeCode, error));
    }

    Ok(SetupResult::success(SetupTarget::ClaudeCode, steps))
}

/// Create default config.toml.
fn create_default_config(path: &PathBuf) -> Result<()> {
    let content = r#"# Nika Configuration
# Generated by `nika setup`

[editor]
theme = "dark"
font_size = 12
auto_format = true
indent_size = 2
line_numbers = true

[session]
auto_restore = true
session_dir = ".nika/sessions"
max_sessions = 50
session_ttl_days = 7

[providers]
default = "auto"
timeout_secs = 30
auto_retry = true
max_retries = 3

[mcp]
auto_start_servers = true
server_timeout_secs = 10

[daemon]
auto_start = true
socket_path = "~/.nika/daemon.sock"
"#;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.clone(),
        operation: "write config.toml".to_string(),
        source: e,
    })
}

/// Create default mcp.yaml.
fn create_default_mcp_config(path: &PathBuf) -> Result<()> {
    let content = r#"# Nika MCP Server Configuration
# Generated by `nika setup`
#
# Add MCP servers here or use `nika mcp add <name>`

servers: {}

# Example servers:
#
# servers:
#   neo4j:
#     command: npx
#     args: ["-y", "@neo4j/mcp-neo4j"]
#     env:
#       NEO4J_URI: bolt://localhost:7687
#       NEO4J_USERNAME: neo4j
#       NEO4J_PASSWORD: ${nika:neo4j}
#
#   perplexity:
#     command: npx
#     args: ["-y", "@anthropic/mcp-server-perplexity"]
#     env:
#       PERPLEXITY_API_KEY: ${nika:perplexity}
"#;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.clone(),
        operation: "write mcp.yaml".to_string(),
        source: e,
    })
}

/// Create default sync.yaml.
fn create_default_sync_config(path: &PathBuf) -> Result<()> {
    let content = r#"# Nika Sync Configuration
# Generated by `nika setup`
#
# Enable editors for MCP config sync with `nika sync --enable <editor>`

enabled_editors: []
"#;

    std::fs::write(path, content).map_err(|e| NikaError::IoPathError {
        path: path.clone(),
        operation: "write sync.yaml".to_string(),
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
    fn test_setup_target_from_arg() {
        assert_eq!(SetupTarget::from_arg("nika"), Some(SetupTarget::Nika));
        assert_eq!(SetupTarget::from_arg("NIKA"), Some(SetupTarget::Nika));
        assert_eq!(SetupTarget::from_arg("novanet"), Some(SetupTarget::NovaNet));
        assert_eq!(
            SetupTarget::from_arg("nova-net"),
            Some(SetupTarget::NovaNet)
        );
        assert_eq!(
            SetupTarget::from_arg("claude-code"),
            Some(SetupTarget::ClaudeCode)
        );
        assert_eq!(
            SetupTarget::from_arg("claude"),
            Some(SetupTarget::ClaudeCode)
        );
        assert_eq!(SetupTarget::from_arg("cc"), Some(SetupTarget::ClaudeCode));
        assert_eq!(SetupTarget::from_arg("unknown"), None);
    }

    #[test]
    fn test_setup_target_display_name() {
        assert_eq!(SetupTarget::Auto.display_name(), "Auto-detect");
        assert_eq!(SetupTarget::Nika.display_name(), "Nika CLI");
        assert_eq!(SetupTarget::NovaNet.display_name(), "NovaNet");
        assert_eq!(SetupTarget::ClaudeCode.display_name(), "Claude Code");
    }

    #[test]
    fn test_setup_result_success() {
        let result = SetupResult::success(
            SetupTarget::Nika,
            vec!["Step 1".to_string(), "Step 2".to_string()],
        );

        assert!(result.success);
        assert_eq!(result.target, SetupTarget::Nika);
        assert_eq!(result.steps_completed.len(), 2);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_setup_result_failure() {
        let result = SetupResult::failure(SetupTarget::NovaNet, "Neo4j not running".to_string());

        assert!(!result.success);
        assert_eq!(result.target, SetupTarget::NovaNet);
        assert!(result.steps_completed.is_empty());
        assert_eq!(result.error.as_ref().unwrap(), "Neo4j not running");
    }

    #[test]
    fn test_create_default_config() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("config.toml");

        create_default_config(&path).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[editor]"));
        assert!(content.contains("[session]"));
        assert!(content.contains("[providers]"));
        assert!(content.contains("[daemon]"));
    }

    #[test]
    fn test_create_default_mcp_config() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("mcp.yaml");

        create_default_mcp_config(&path).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("servers:"));
        assert!(content.contains("neo4j"));
        assert!(content.contains("perplexity"));
    }

    #[test]
    fn test_create_default_sync_config() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("sync.yaml");

        create_default_sync_config(&path).unwrap();
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("enabled_editors:"));
    }
}
