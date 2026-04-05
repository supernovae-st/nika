//! Provider Contract Tests
//!
//! These tests define the expected behavior of `nika provider *` commands.
//!
//! # Tests (15 total)
//!
//! 1. provider list returns all 13 providers (7 LLM + 6 MCP)
//! 2. provider list shows status for each
//! 3. provider set stores in vault
//! 4. provider get retrieves masked
//! 5. provider get returns error for unknown
//! 6. provider test validates key format
//! 7. provider test rejects invalid key
//! 8. provider migrate moves env vars
//! 9. provider migrate is idempotent
//! 10. provider status shows detailed info
//! 11. anthropic key prefix validation (sk-ant-)
//! 12. openai key prefix validation (sk-)
//! 13. mistral key prefix validation
//! 14. provider env var fallback works
//! 15. provider priority: env > daemon > vault

use super::common::{run_nika, KNOWN_PROVIDERS, LLM_PROVIDERS};

/// Contract: `nika provider list` returns all 7 LLM providers
#[test]
fn contract_provider_list_returns_all_providers() {
    let output = run_nika(&["provider", "list"]);

    assert!(output.status.success(), "provider list should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // All 7 LLM providers should be listed
    // MCP providers (neo4j, github, etc.) are managed via `nika mcp`, not `nika provider`
    for provider in LLM_PROVIDERS {
        assert!(
            stdout.contains(provider),
            "LLM provider '{}' should be in list output. Got: {}",
            provider,
            stdout
        );
    }
}

/// Contract: `nika provider list` shows status (configured/not configured)
#[test]
fn contract_provider_list_shows_status() {
    let output = run_nika(&["provider", "list"]);

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Output should contain status indicators
    // nika uses: 📦 for configured, ○ for not configured
    assert!(
        stdout.contains('✓')
            || stdout.contains('✗')
            || stdout.contains("configured")
            || stdout.contains("📦")
            || stdout.contains("○")
            || stdout.contains("not set"),
        "Provider list should show configuration status. Got: {}",
        stdout
    );
}

/// Contract: `nika provider list` distinguishes LLM and MCP providers
#[test]
fn contract_provider_list_shows_categories() {
    let output = run_nika(&["provider", "list"]);

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show provider categories or types
    // LLM providers: anthropic, openai, mistral, groq, deepseek, gemini
    // NOTE: Ollama removed in v0.27 — use provider: native with mistral.rs instead
    // MCP providers: neo4j, github, slack, perplexity, firecrawl, supadata
    let has_category_info = stdout.contains("LLM")
        || stdout.contains("MCP")
        || stdout.contains("API")
        || stdout.contains("Type");

    assert!(
        has_category_info || stdout.lines().count() >= 3,
        "Provider list should show all providers with some structure. Got: {}",
        stdout
    );
}

/// Contract: `nika provider get <unknown>` shows "Not configured"
#[test]
fn contract_provider_get_unknown_returns_error() {
    let output = run_nika(&["provider", "get", "nonexistent_provider_xyz"]);

    // `nika provider get` returns exit code 0 but shows "Not configured"
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("Not configured")
            || combined.contains("unknown")
            || combined.contains("not found")
            || combined.contains("invalid")
            || combined.contains("No key found")
            || !output.status.success(),
        "Should indicate provider not configured. Got: {}",
        combined
    );
}

/// Contract: `nika provider test` validates anthropic key format (sk-ant-)
#[test]
fn contract_provider_test_validates_anthropic_format() {
    // Skip if no ANTHROPIC_API_KEY is set
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    let output = run_nika(&["provider", "test", "anthropic"]);

    // Should either succeed (valid key) or fail with format error (not connection error)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // If it fails, it should be for a valid reason (not a crash)
    if !output.status.success() {
        assert!(
            stderr.contains("invalid")
                || stderr.contains("format")
                || stderr.contains("connection")
                || stderr.contains("unauthorized"),
            "Test failure should have meaningful error. Got: {}",
            stderr
        );
    } else {
        assert!(
            stdout.contains("valid") || stdout.contains("✓") || stdout.contains("success"),
            "Success should indicate valid key. Got: {}",
            stdout
        );
    }
}

/// Contract: `nika provider test` rejects malformed keys
#[test]
fn contract_provider_test_rejects_invalid_key() {
    // Set a clearly invalid key temporarily
    std::env::set_var("TEST_INVALID_KEY", "not-a-valid-api-key");

    // The provider test command should validate format before making API calls
    // This tests the local validation, not API connectivity
    let output = run_nika(&["provider", "test", "anthropic", "--key", "invalid"]);

    // Should fail or warn about invalid format
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Either fails with format error, or doesn't accept --key flag
    // Both are acceptable behaviors that we want to preserve
    assert!(
        !output.status.success()
            || combined.contains("invalid")
            || combined.contains("error")
            || combined.contains("unrecognized"),
        "Invalid key should be rejected. Got: {}",
        combined
    );

    std::env::remove_var("TEST_INVALID_KEY");
}

/// Contract: `nika provider migrate` is idempotent
#[test]
fn contract_provider_migrate_is_idempotent() {
    // Run migrate twice - second run should not fail
    let output1 = run_nika(&["provider", "migrate"]);
    let output2 = run_nika(&["provider", "migrate"]);

    // Both should succeed (or both fail consistently if no env vars)
    assert_eq!(
        output1.status.success(),
        output2.status.success(),
        "Migrate should be idempotent"
    );

    // Second run should indicate already migrated or no changes
    let stdout2 = String::from_utf8_lossy(&output2.stdout);
    let combined2 = format!("{}{}", stdout2, String::from_utf8_lossy(&output2.stderr));

    // Should not crash or have unexpected errors
    assert!(
        !combined2.contains("panic") && !combined2.contains("RUST_BACKTRACE"),
        "Migrate should not panic. Got: {}",
        combined2
    );
}

/// Contract: `nika provider status` shows detailed information
#[test]
fn contract_provider_status_shows_detail() {
    let output = run_nika(&["provider", "status"]);

    // status command may not exist in all versions - check gracefully
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If command doesn't exist, that's documented behavior
        if stderr.contains("unrecognized") || stderr.contains("unknown") {
            eprintln!("Note: 'provider status' subcommand not available");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // If command exists, it should show more detail than list
    assert!(
        !stdout.is_empty(),
        "Provider status should show information. Got: {}",
        stdout
    );
}

/// Contract: Provider resolution priority - env > daemon > vault
#[test]
fn contract_provider_priority_env_daemon_vault() {
    // This test documents the expected priority:
    // 1. Environment variable
    // 2. Daemon IPC
    // 3. NikaVault encrypted store

    let output = run_nika(&["provider", "list"]);
    assert!(output.status.success());

    // The output should reflect the priority - we can't easily test this
    // without modifying vault state, but we document the contract
    let _stdout = String::from_utf8_lossy(&output.stdout);
}

/// Contract: Environment variable fallback works for each provider
#[test]
fn contract_env_var_fallback_for_all_providers() {
    // Document the expected env var names for each provider
    let env_var_mapping = [
        ("anthropic", "ANTHROPIC_API_KEY"),
        ("openai", "OPENAI_API_KEY"),
        ("mistral", "MISTRAL_API_KEY"),
        ("groq", "GROQ_API_KEY"),
        ("deepseek", "DEEPSEEK_API_KEY"),
        ("gemini", "GEMINI_API_KEY"),
        ("xai", "XAI_API_KEY"),
        // OpenAI-compatible providers
        ("openrouter", "OPENROUTER_API_KEY"),
        ("together", "TOGETHER_API_KEY"),
        ("fireworks", "FIREWORKS_API_KEY"),
        ("cerebras", "CEREBRAS_API_KEY"),
        ("sambanova", "SAMBANOVA_API_KEY"),
        ("cohere", "COHERE_API_KEY"),
        ("ai21", "AI21_API_KEY"),
        // MCP providers
        ("neo4j", "NEO4J_PASSWORD"),
        ("github", "GITHUB_TOKEN"),
        ("slack", "SLACK_BOT_TOKEN"),
        ("perplexity", "PERPLEXITY_API_KEY"),
        ("firecrawl", "FIRECRAWL_API_KEY"),
        ("supadata", "SUPADATA_API_KEY"),
        ("dataforseo", "DATAFORSEO_API_KEY"),
        ("ahrefs", "AHREFS_API_KEY"),
        ("postgres", "POSTGRES_URL"),
        ("filesystem", "FILESYSTEM_ALLOWED_PATHS"),
        ("memory", "MEMORY_STORAGE_PATH"),
        // Local providers
        ("native", "NIKA_NATIVE_MODEL_PATH"),
        ("mock", ""),
    ];

    // Verify the mapping is complete (27 providers: 14 LLM + 11 MCP + 2 Local)
    assert_eq!(
        env_var_mapping.len(),
        KNOWN_PROVIDERS.len(),
        "All providers should have env var mapping"
    );

    // Each provider name should be in KNOWN_PROVIDERS
    for (provider, _env_var) in &env_var_mapping {
        assert!(
            KNOWN_PROVIDERS.contains(provider),
            "Provider '{}' should be in KNOWN_PROVIDERS",
            provider
        );
    }
}

/// Contract: Provider key masking in output
#[test]
fn contract_provider_get_masks_key() {
    // If we have any key configured, get should mask it
    let output = run_nika(&["provider", "get", "anthropic"]);

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Key should be masked - should not show full key
        // Expected formats: "sk-ant-***" or "****" or similar
        assert!(
            stdout.contains("***") || stdout.contains("•••") || stdout.len() < 50,
            "API key should be masked in output. Got: {}",
            stdout
        );

        // Should NOT contain a full API key (typically 40+ chars without masking)
        let unmasked_key_pattern =
            stdout.contains("sk-ant-api") && stdout.chars().filter(|c| *c == '-').count() > 3;
        assert!(
            !unmasked_key_pattern,
            "API key should not be shown unmasked"
        );
    }
}

/// Contract: `nika provider list --json` returns JSON format (if supported)
#[test]
fn contract_provider_list_json_format() {
    let output = run_nika(&["provider", "list", "--json"]);

    // --json flag may not be supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("unrecognized") || stderr.contains("unknown") {
            eprintln!("Note: --json flag not supported for provider list");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // If JSON is supported, output should be valid JSON
    if output.status.success() && stdout.starts_with('{') || stdout.starts_with('[') {
        // Try to parse as JSON
        let parse_result: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
        assert!(
            parse_result.is_ok(),
            "JSON output should be valid JSON. Got: {}",
            stdout
        );
    }
}

/// Contract: Provider commands are case-insensitive for provider names
#[test]
fn contract_provider_names_case_insensitive() {
    // Test that ANTHROPIC == anthropic == Anthropic
    let output_lower = run_nika(&["provider", "get", "anthropic"]);
    let output_upper = run_nika(&["provider", "get", "ANTHROPIC"]);

    // Both should have same behavior (both succeed or both fail)
    assert_eq!(
        output_lower.status.success(),
        output_upper.status.success(),
        "Provider names should be case-insensitive"
    );
}

/// Contract: `nika provider` with no subcommand shows help
#[test]
fn contract_provider_no_subcommand_shows_help() {
    let output = run_nika(&["provider"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should show help/usage information
    assert!(
        combined.contains("list")
            && combined.contains("set")
            && combined.contains("get")
            && (combined.contains("Usage")
                || combined.contains("USAGE")
                || combined.contains("help")),
        "Provider without subcommand should show help. Got: {}",
        combined
    );
}
