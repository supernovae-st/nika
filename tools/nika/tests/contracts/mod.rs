//! Nika CLI contract tests
//!
//! These tests verify the behavior of `nika` CLI commands.
//! All commands use `nika` as the primary CLI.
//!
//! # Test Categories
//!
//! - Provider contracts (15 tests): API key management (`nika provider *`)
//! - MCP contracts (12 tests): MCP server configuration (`nika mcp *`)
//! - Package contracts (20 tests): Package manager operations (`nika add/remove`)
//! - Model contracts (10 tests): Local model management (`nika model *`)
//! - Sync contracts (8 tests): Editor sync (`nika sync *`)
//! - Setup contracts (10 tests): Setup wizard (`nika setup *`)
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
//! commands work correctly.

mod mcp_contracts;
mod model_contracts;
mod pkg_contracts;
mod provider_contracts;
mod setup_contracts;
mod sync_contracts;

/// Common test utilities for contract tests
pub mod common {
    use std::process::{Command, Output};

    /// Execute nika command and return output
    pub fn run_nika(args: &[&str]) -> Output {
        Command::new("nika")
            .args(args)
            .output()
            .expect("Failed to execute nika command")
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
    /// NOTE: Ollama removed in v0.27 -- use provider: native instead
    pub const LLM_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
        "xai",
    ];

    /// MCP providers (not shown in `nika provider list`, managed via `nika mcp`)
    #[allow(dead_code)]
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
        "xai",
        "neo4j",
        "github",
        "slack",
        "perplexity",
        "firecrawl",
        "supadata",
    ];

    /// Known MCP aliases count
    pub const MCP_ALIAS_COUNT: usize = 113;

    /// Known models count
    pub const KNOWN_MODEL_COUNT: usize = 16;
}
