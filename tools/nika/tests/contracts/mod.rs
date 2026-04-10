// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

    /// LLM providers shown by `nika provider list`
    pub const LLM_PROVIDERS: &[&str] = &[
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
        "xai",
        "openrouter",
        "together",
        "fireworks",
        "cerebras",
        "sambanova",
        "cohere",
        "ai21",
    ];

    /// All known providers (14 LLM + 11 MCP + 2 Local = 27 total)
    pub const KNOWN_PROVIDERS: &[&str] = &[
        // LLM rig-core (7)
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
        "xai",
        // LLM OpenAI-compat (7)
        "openrouter",
        "together",
        "fireworks",
        "cerebras",
        "sambanova",
        "cohere",
        "ai21",
        // MCP (11)
        "neo4j",
        "github",
        "slack",
        "perplexity",
        "firecrawl",
        "supadata",
        "dataforseo",
        "ahrefs",
        "postgres",
        "filesystem",
        "memory",
        // Local (2)
        "native",
        "mock",
    ];

    /// Known MCP aliases count
    pub const MCP_ALIAS_COUNT: usize = 113;

    /// Known models count
    pub const KNOWN_MODEL_COUNT: usize = 16;
}
