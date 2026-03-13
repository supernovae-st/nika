//! Contract Tests for Nika CLI (v0.28: spn→nika fusion complete)
//!
//! These tests verify the behavior of features migrated from spn to nika.
//! All commands now use `nika` as the primary CLI.
//!
//! # Test Categories
//!
//! - Provider contracts (15 tests): API key management (`nika provider *`)
//! - MCP contracts (12 tests): MCP server configuration (`nika mcp *`)
//! - Package contracts (20 tests): Package manager operations (`nika add/remove`)
//! - Model contracts (10 tests): Local model management (`nika model *`)
//! - Sync contracts (8 tests): Editor sync (`nika sync *`)
//! - Setup contracts (10 tests): Setup wizard (`nika setup *`)
//! - Daemon contracts (15 tests): Daemon IPC protocol (`nika daemon *`)
//! - Jobs contracts (10 tests): Job scheduler (`nika job *`)
//!
//! # Running Contract Tests
//!
//! ```bash
//! cargo test --test contract_tests -- --test-threads=1
//! ```
//!
//! # Philosophy
//!
//! Contract tests verify behavior, not implementation. They ensure nika
//! commands work correctly after the spn→nika fusion.

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

    /// Execute nika command and return output (primary CLI since v0.28)
    pub fn run_nika(args: &[&str]) -> Output {
        Command::new("nika")
            .args(args)
            .output()
            .expect("Failed to execute nika command")
    }

    /// Execute spn command and return output (deprecated, forwards to nika)
    #[deprecated(since = "0.28.0", note = "Use run_nika instead - spn is deprecated")]
    #[allow(dead_code)]
    pub fn run_spn(args: &[&str]) -> Output {
        Command::new("spn")
            .args(args)
            .output()
            .expect("Failed to execute spn command")
    }

    /// Check if nika daemon is running
    pub fn is_daemon_running() -> bool {
        let output = run_nika(&["daemon", "status"]);
        output.status.success()
    }

    /// Parse provider list output into a vec of provider names
    #[allow(dead_code)]
    pub fn parse_provider_names(output: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(output)
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("Provider"))
            .filter_map(|line| line.split_whitespace().next())
            .map(|s| s.to_string())
            .collect()
    }

    /// Parse MCP server list output
    #[allow(dead_code)]
    pub fn parse_mcp_servers(output: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(output)
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with("Server"))
            .filter_map(|line| line.split_whitespace().next())
            .map(|s| s.to_string())
            .collect()
    }

    /// LLM providers shown by `nika provider list`
    /// NOTE: Ollama removed in v0.27 — use provider: native instead
    pub const LLM_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
    ];

    /// MCP providers (not shown in `nika provider list`, managed via `nika mcp`)
    pub const MCP_PROVIDERS: &[&str] = &[
        "neo4j",
        "github",
        "slack",
        "perplexity",
        "firecrawl",
        "supadata",
    ];

    /// All known providers (LLM + MCP) for reference
    pub const KNOWN_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
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
