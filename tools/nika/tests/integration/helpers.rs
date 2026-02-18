//! Integration test helpers for testing against real MCP servers.
//!
//! These tests require:
//! - NovaNet MCP server binary at NOVANET_MCP_PATH or default location
//! - Neo4j running at localhost:7687 with novanetpassword
//!
//! # Environment Variables
//!
//! - `NOVANET_MCP_PATH`: Path to the novanet-mcp binary (optional)
//! - `NOVANET_MCP_NEO4J_URI`: Neo4j bolt URI (default: bolt://localhost:7687)
//! - `NOVANET_MCP_NEO4J_USER`: Neo4j username (default: neo4j)
//! - `NOVANET_MCP_NEO4J_PASSWORD`: Neo4j password (default: novanetpassword)

use std::net::TcpStream;
use std::path::PathBuf;

use nika::McpConfig;

/// Default path to NovaNet MCP server binary.
const DEFAULT_NOVANET_MCP_PATH: &str = "/Users/thibaut/supernovae-st/supernovae-agi/novanet-dev/tools/novanet-mcp/target/release/novanet-mcp";

/// Default Neo4j connection settings.
const DEFAULT_NEO4J_URI: &str = "bolt://localhost:7687";
const DEFAULT_NEO4J_USER: &str = "neo4j";
const DEFAULT_NEO4J_PASSWORD: &str = "novanetpassword";

/// Get NovaNet MCP server path from environment or use default.
///
/// Checks `NOVANET_MCP_PATH` environment variable first, falls back to
/// the default path at `/Users/thibaut/supernovae-st/supernovae-agi/novanet-dev/tools/novanet-mcp/target/release/novanet-mcp`.
///
/// # Example
///
/// ```rust,ignore
/// let path = novanet_mcp_path();
/// if path.exists() {
///     println!("NovaNet MCP binary found at {:?}", path);
/// }
/// ```
pub fn novanet_mcp_path() -> PathBuf {
    std::env::var("NOVANET_MCP_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_NOVANET_MCP_PATH))
}

/// Check if NovaNet MCP server binary exists at the expected path.
///
/// Returns `true` if the binary file exists and is readable.
///
/// # Example
///
/// ```rust,ignore
/// if !is_novanet_available() {
///     eprintln!("Skipping: NovaNet MCP binary not found");
///     return;
/// }
/// ```
pub fn is_novanet_available() -> bool {
    let path = novanet_mcp_path();
    path.exists() && path.is_file()
}

/// Check if Neo4j is running on the default port (7687).
///
/// Attempts a TCP connection to `127.0.0.1:7687` to verify Neo4j availability.
///
/// # Example
///
/// ```rust,ignore
/// if !is_neo4j_available() {
///     eprintln!("Skipping: Neo4j not running at localhost:7687");
///     return;
/// }
/// ```
pub fn is_neo4j_available() -> bool {
    TcpStream::connect("127.0.0.1:7687").is_ok()
}

/// Create MCP configuration for connecting to NovaNet server.
///
/// Returns an `McpConfig` configured with:
/// - Server name: "novanet"
/// - Command: path to novanet-mcp binary
/// - Environment variables for Neo4j connection
///
/// # Environment Variables Used
///
/// - `NOVANET_MCP_NEO4J_URI`: Neo4j bolt URI
/// - `NOVANET_MCP_NEO4J_USER`: Neo4j username
/// - `NOVANET_MCP_NEO4J_PASSWORD`: Neo4j password
///
/// # Example
///
/// ```rust,ignore
/// let config = novanet_config();
/// let client = McpClient::new(config)?;
/// client.connect().await?;
/// ```
pub fn novanet_config() -> McpConfig {
    let mcp_path = novanet_mcp_path();

    // Get Neo4j connection settings from environment or use defaults
    let neo4j_uri =
        std::env::var("NOVANET_MCP_NEO4J_URI").unwrap_or_else(|_| DEFAULT_NEO4J_URI.to_string());
    let neo4j_user =
        std::env::var("NOVANET_MCP_NEO4J_USER").unwrap_or_else(|_| DEFAULT_NEO4J_USER.to_string());
    let neo4j_password = std::env::var("NOVANET_MCP_NEO4J_PASSWORD")
        .unwrap_or_else(|_| DEFAULT_NEO4J_PASSWORD.to_string());

    McpConfig::new("novanet", mcp_path.to_string_lossy())
        .with_env("NOVANET_MCP_NEO4J_URI", neo4j_uri)
        .with_env("NOVANET_MCP_NEO4J_USER", neo4j_user)
        .with_env("NOVANET_MCP_NEO4J_PASSWORD", neo4j_password)
        .with_env("RUST_LOG", "info")
}

/// Check if all integration dependencies are available.
///
/// Returns `true` if both NovaNet MCP binary exists AND Neo4j is running.
///
/// # Example
///
/// ```rust,ignore
/// if !are_dependencies_available() {
///     eprintln!("Skipping: integration dependencies not available");
///     return;
/// }
/// ```
pub fn are_dependencies_available() -> bool {
    is_novanet_available() && is_neo4j_available()
}

/// Print skip message and return early if dependencies are not available.
///
/// This is a convenience function for test functions that need to check
/// for integration dependencies before running.
///
/// # Returns
///
/// - `true` if test should be skipped (dependencies not available)
/// - `false` if dependencies are available and test can proceed
///
/// # Example
///
/// ```rust,ignore
/// #[tokio::test]
/// #[ignore]
/// async fn test_novanet_connection() {
///     if should_skip_integration_test() {
///         return;
///     }
///     // ... actual test code
/// }
/// ```
pub fn should_skip_integration_test() -> bool {
    if !is_novanet_available() {
        eprintln!(
            "  Skipping: NovaNet MCP binary not found at {:?}",
            novanet_mcp_path()
        );
        return true;
    }

    if !is_neo4j_available() {
        eprintln!("  Skipping: Neo4j not available at localhost:7687");
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_novanet_mcp_path_returns_pathbuf() {
        let path = novanet_mcp_path();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn test_novanet_config_has_correct_name() {
        let config = novanet_config();
        assert_eq!(config.name, "novanet");
    }

    #[test]
    fn test_novanet_config_has_neo4j_env_vars() {
        let config = novanet_config();
        assert!(config.env.contains_key("NOVANET_MCP_NEO4J_URI"));
        assert!(config.env.contains_key("NOVANET_MCP_NEO4J_USER"));
        assert!(config.env.contains_key("NOVANET_MCP_NEO4J_PASSWORD"));
    }

    #[test]
    fn test_are_dependencies_available_matches_individual_checks() {
        // This just verifies the logic is correct, not the actual state
        let novanet = is_novanet_available();
        let neo4j = is_neo4j_available();
        let combined = are_dependencies_available();

        assert_eq!(combined, novanet && neo4j);
    }
}
