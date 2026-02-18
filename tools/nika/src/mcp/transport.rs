//! MCP Transport Layer - manages server process lifecycle
//!
//! This module handles spawning MCP server processes and managing their lifecycle.
//! MCP servers communicate over stdio (stdin/stdout) using JSON-RPC 2.0.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::McpTransport;
//!
//! let transport = McpTransport::new("npx", &["-y", "@novanet/mcp-server"])
//!     .with_env("NEO4J_URI", "bolt://localhost:7687");
//!
//! let mut child = transport.spawn().await?;
//! // Use child.stdin/stdout for JSON-RPC communication
//! ```

use rustc_hash::FxHashMap;
use std::process::Stdio;

use tokio::process::{Child, Command};

use crate::error::{NikaError, Result};

/// MCP Transport - spawns and manages MCP server process.
///
/// The transport is responsible for:
/// - Spawning the server process with correct stdio configuration
/// - Passing environment variables to the child process
/// - Returning handles for stdin/stdout communication
#[derive(Debug)]
pub struct McpTransport {
    /// Command to execute (e.g., "npx", "node", "python")
    command: String,

    /// Command arguments
    args: Vec<String>,

    /// Environment variables for the process
    env: FxHashMap<String, String>,
}

impl McpTransport {
    /// Create a new transport with the given command and arguments.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let transport = McpTransport::new("npx", &["-y", "@novanet/mcp-server"]);
    /// ```
    pub fn new(command: &str, args: &[&str]) -> Self {
        Self {
            command: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: FxHashMap::default(),
        }
    }

    /// Add an environment variable to the process.
    ///
    /// Can be chained for multiple variables.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let transport = McpTransport::new("node", &["server.js"])
    ///     .with_env("API_KEY", "secret")
    ///     .with_env("DEBUG", "true");
    /// ```
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Get the command.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Get the arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Get the environment variables.
    pub fn env(&self) -> &FxHashMap<String, String> {
        &self.env
    }

    /// Spawn the MCP server process.
    ///
    /// The process is spawned with:
    /// - stdin piped for sending JSON-RPC requests
    /// - stdout piped for receiving JSON-RPC responses
    /// - stderr inherited for debugging
    ///
    /// # Errors
    ///
    /// Returns `NikaError::McpStartError` if the process fails to spawn.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let transport = McpTransport::new("echo", &["hello"]);
    /// let mut child = transport.spawn().await?;
    ///
    /// // Read from stdout
    /// let stdout = child.stdout.take().unwrap();
    /// ```
    pub async fn spawn(&self) -> Result<Child> {
        let mut cmd = Command::new(&self.command);

        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());

        // Add environment variables
        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        cmd.spawn().map_err(|e| NikaError::McpStartError {
            name: self.command.clone(),
            reason: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_new() {
        let transport = McpTransport::new("echo", &["hello"]);

        assert_eq!(transport.command(), "echo");
        assert_eq!(transport.args(), &["hello"]);
    }

    #[test]
    fn test_transport_with_env() {
        let transport = McpTransport::new("node", &[]).with_env("KEY", "value");

        assert_eq!(transport.env().get("KEY"), Some(&"value".to_string()));
    }
}
