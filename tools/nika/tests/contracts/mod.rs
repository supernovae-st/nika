//! Contract Tests for spn → Nika Fusion Migration
//!
//! These tests define the expected behavior of features being migrated from spn.
//! They must ALL PASS before any code migration begins.
//!
//! # Test Categories
//!
//! - Provider contracts (15 tests): API key management
//! - MCP contracts (12 tests): MCP server configuration
//! - Package contracts (20 tests): Package manager operations
//! - Model contracts (10 tests): Local model management
//! - Sync contracts (8 tests): Editor sync
//! - Setup contracts (10 tests): Setup wizard
//! - Daemon contracts (15 tests): Daemon IPC protocol
//! - Jobs contracts (10 tests): Job scheduler
//!
//! # Running Contract Tests
//!
//! ```bash
//! cargo test --test contracts -- --test-threads=1
//! ```
//!
//! # Philosophy
//!
//! Contract tests verify behavior, not implementation. They should pass
//! regardless of whether the underlying code is in spn or nika.

mod daemon_contracts;
mod jobs_contracts;
mod mcp_contracts;
mod model_contracts;
mod pkg_contracts;
mod provider_contracts;
mod setup_contracts;
mod sync_contracts;

/// Common test utilities for contract tests
pub mod common {
    use std::process::{Command, Output};

    /// Execute spn command and return output
    pub fn run_spn(args: &[&str]) -> Output {
        Command::new("spn")
            .args(args)
            .output()
            .expect("Failed to execute spn command")
    }

    /// Execute nika command and return output (for parity tests)
    pub fn run_nika(args: &[&str]) -> Output {
        Command::new("nika")
            .args(args)
            .output()
            .expect("Failed to execute nika command")
    }

    /// Check if spn daemon is running
    pub fn is_daemon_running() -> bool {
        let output = run_spn(&["daemon", "status"]);
        output.status.success()
    }

    /// Parse provider list output into a vec of provider names
    pub fn parse_provider_names(output: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(output)
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("Provider"))
            .filter_map(|line| line.split_whitespace().next())
            .map(|s| s.to_string())
            .collect()
    }

    /// Parse MCP server list output
    pub fn parse_mcp_servers(output: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(output)
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("Server"))
            .filter_map(|line| line.split_whitespace().next())
            .map(|s| s.to_string())
            .collect()
    }

    /// Known providers from spn-core (must match KNOWN_PROVIDERS)
    pub const KNOWN_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
        "ollama",
        "neo4j",
        "github",
        "slack",
        "perplexity",
        "firecrawl",
        "supadata",
    ];

    /// Known MCP aliases count
    pub const MCP_ALIAS_COUNT: usize = 48;

    /// Known models count
    pub const KNOWN_MODEL_COUNT: usize = 16;
}
