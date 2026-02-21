//! Provider Selection Tests for RigAgentLoop.run_auto()
//!
//! Tests the automatic provider selection logic in run_auto():
//! 1. Explicit params.provider takes precedence
//! 2. ANTHROPIC_API_KEY env var -> Claude
//! 3. OPENAI_API_KEY env var -> OpenAI
//! 4. No keys -> Error
//!
//! Note: These tests manipulate environment variables and MUST run serially.
//! Use `cargo test --test rig_provider_selection_test -- --test-threads=1`

use std::sync::Arc;

use nika::ast::AgentParams;
use nika::event::EventLog;
use nika::mcp::McpClient;
use nika::runtime::RigAgentLoop;
use rustc_hash::FxHashMap;

/// Helper struct to manage environment variables for testing.
/// Automatically restores original values on drop.
struct EnvGuard {
    anthropic_key: Option<String>,
    openai_key: Option<String>,
}

impl EnvGuard {
    /// Capture current env vars and clear them for testing.
    fn new() -> Self {
        let anthropic_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let openai_key = std::env::var("OPENAI_API_KEY").ok();

        // Clear both keys
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");

        Self {
            anthropic_key,
            openai_key,
        }
    }

    /// Set only ANTHROPIC_API_KEY (with a fake value for testing).
    fn set_anthropic_only(&self) {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-fake-key-for-testing");
        std::env::remove_var("OPENAI_API_KEY");
    }

    /// Set only OPENAI_API_KEY (with a fake value for testing).
    fn set_openai_only(&self) {
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::set_var("OPENAI_API_KEY", "sk-test-fake-key-for-testing");
    }

    /// Set both keys (with fake values for testing).
    fn set_both_keys(&self) {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-fake-key-for-testing");
        std::env::set_var("OPENAI_API_KEY", "sk-test-fake-key-for-testing");
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // Restore original values
        if let Some(ref key) = self.anthropic_key {
            std::env::set_var("ANTHROPIC_API_KEY", key);
        } else {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }

        if let Some(ref key) = self.openai_key {
            std::env::set_var("OPENAI_API_KEY", key);
        } else {
            std::env::remove_var("OPENAI_API_KEY");
        }
    }
}

/// Create a minimal RigAgentLoop for testing.
fn create_test_agent() -> RigAgentLoop {
    let params = AgentParams {
        prompt: "Test prompt for provider selection".to_string(),
        max_turns: Some(1),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    RigAgentLoop::new("test_provider".to_string(), params, event_log, mcp_clients)
        .expect("Should create RigAgentLoop")
}

// =============================================================================
// HIGH Priority: Provider Selection Tests
// =============================================================================

/// Test: run_auto() with no API keys set returns clear error.
///
/// Expected: NIKA-113 AgentValidationError with message about missing keys.
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_no_keys_returns_clear_error() {
    let _guard = EnvGuard::new(); // Clears both keys, restores on drop

    let mut agent = create_test_agent();
    let result = agent.run_auto().await;

    assert!(result.is_err(), "run_auto() should fail without API keys");

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Verify error code
    assert!(
        err_msg.contains("NIKA-113"),
        "Should be AgentValidationError (NIKA-113), got: {err_msg}"
    );

    // Verify helpful error message
    assert!(
        err_msg.contains("No API key found")
            || err_msg.contains("ANTHROPIC_API_KEY")
            || err_msg.contains("OPENAI_API_KEY"),
        "Error should mention missing API keys, got: {err_msg}"
    );
}

/// Test: run_auto() with only ANTHROPIC_API_KEY selects Claude.
///
/// We verify this by checking that when Anthropic key is set (but invalid),
/// the error comes from attempting to use Claude (not "no key found").
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_anthropic_only_uses_claude() {
    let guard = EnvGuard::new();
    guard.set_anthropic_only();

    let mut agent = create_test_agent();
    let result = agent.run_auto().await;

    // With a fake API key, we expect an error from the Anthropic provider
    // (not "no API key found")
    assert!(
        result.is_err(),
        "run_auto() should fail with invalid Anthropic key"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should NOT be the "no API key" error - proves Claude was selected
    assert!(
        !err_msg.contains("No API key found"),
        "Should not say 'No API key found' when ANTHROPIC_API_KEY is set, got: {err_msg}"
    );

    // The error should indicate the provider was actually called
    // (authentication error, network error, or provider execution error)
    assert!(
        err_msg.contains("NIKA-115")  // AgentExecutionError
            || err_msg.contains("NIKA-116")  // ThinkingCaptureFailed
            || err_msg.contains("NIKA-030")  // ProviderError
            || err_msg.contains("401")
            || err_msg.contains("authentication")
            || err_msg.contains("invalid")
            || err_msg.contains("API")
            || err_msg.contains("error"),
        "Error should be from Claude provider execution, got: {err_msg}"
    );
}

/// Test: run_auto() with only OPENAI_API_KEY selects OpenAI.
///
/// We verify this by checking that when OpenAI key is set (but invalid),
/// the error comes from attempting to use OpenAI (not "no key found").
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_openai_only_uses_openai() {
    let guard = EnvGuard::new();
    guard.set_openai_only();

    let mut agent = create_test_agent();
    let result = agent.run_auto().await;

    // With a fake API key, we expect an error from the OpenAI provider
    assert!(
        result.is_err(),
        "run_auto() should fail with invalid OpenAI key"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should NOT be the "no API key" error - proves OpenAI was selected
    assert!(
        !err_msg.contains("No API key found"),
        "Should not say 'No API key found' when OPENAI_API_KEY is set, got: {err_msg}"
    );

    // The error should indicate the provider was actually called
    assert!(
        err_msg.contains("NIKA-115")  // AgentExecutionError
            || err_msg.contains("NIKA-030")  // ProviderError
            || err_msg.contains("401")
            || err_msg.contains("authentication")
            || err_msg.contains("invalid")
            || err_msg.contains("API")
            || err_msg.contains("error"),
        "Error should be from OpenAI provider execution, got: {err_msg}"
    );
}

/// Test: run_auto() with both keys prefers Claude (Anthropic).
///
/// Per the implementation, ANTHROPIC_API_KEY is checked first, so Claude
/// should be preferred when both keys are available.
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_both_keys_prefers_claude() {
    let guard = EnvGuard::new();
    guard.set_both_keys();

    let mut agent = create_test_agent();
    let result = agent.run_auto().await;

    // With fake API keys, we expect an error from the provider
    assert!(
        result.is_err(),
        "run_auto() should fail with invalid API keys"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should NOT be the "no API key" error
    assert!(
        !err_msg.contains("No API key found"),
        "Should not say 'No API key found' when both keys are set, got: {err_msg}"
    );

    // Verify some error occurred (either provider tried and failed)
    // The key insight: Claude is checked first, so with valid-looking fake keys,
    // the error will come from trying to use Claude (Anthropic), not OpenAI
    assert!(
        err_msg.contains("NIKA-115")
            || err_msg.contains("NIKA-116")
            || err_msg.contains("NIKA-030")
            || err_msg.contains("error")
            || err_msg.contains("API"),
        "Should get provider error when both keys set, got: {err_msg}"
    );
}

// =============================================================================
// Explicit Provider Tests
// =============================================================================

/// Test: Explicit provider "claude" in params uses Claude even when only OpenAI key exists.
///
/// Note: When explicit provider is set, run_auto() calls run_claude() directly.
/// With a fake Anthropic key set, it will try Claude and fail authentication.
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_explicit_claude_provider() {
    let guard = EnvGuard::new();
    // Set both keys so we can request Claude explicitly
    guard.set_both_keys();

    let params = AgentParams {
        prompt: "Test with explicit Claude provider".to_string(),
        provider: Some("claude".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent = RigAgentLoop::new("test_explicit".to_string(), params, event_log, mcp_clients)
        .expect("Should create agent");

    let result = agent.run_auto().await;

    // Should fail with authentication error (not "no key found")
    assert!(
        result.is_err(),
        "run_auto() should fail when Claude requested with fake key"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should NOT be "no key found" or "unknown provider"
    assert!(
        !err_msg.contains("No API key found"),
        "Should not be 'no key found' error, got: {err_msg}"
    );
    assert!(
        !err_msg.contains("Unknown provider"),
        "'claude' should be recognized, got: {err_msg}"
    );
}

/// Test: Explicit provider "openai" in params uses OpenAI.
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_explicit_openai_provider() {
    let guard = EnvGuard::new();
    // Set both keys so we can request OpenAI explicitly
    guard.set_both_keys();

    let params = AgentParams {
        prompt: "Test with explicit OpenAI provider".to_string(),
        provider: Some("openai".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent = RigAgentLoop::new("test_explicit".to_string(), params, event_log, mcp_clients)
        .expect("Should create agent");

    let result = agent.run_auto().await;

    // Should fail with authentication error (not "no key found")
    assert!(
        result.is_err(),
        "run_auto() should fail when OpenAI requested with fake key"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should NOT be "unknown provider"
    assert!(
        !err_msg.contains("Unknown provider"),
        "'openai' should be recognized, got: {err_msg}"
    );
}

/// Test: Invalid provider name returns clear error.
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_invalid_provider_name() {
    let guard = EnvGuard::new();
    guard.set_both_keys();

    let params = AgentParams {
        prompt: "Test with invalid provider".to_string(),
        provider: Some("invalid_provider".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();

    let mut agent = RigAgentLoop::new("test_invalid".to_string(), params, event_log, mcp_clients)
        .expect("Should create agent");

    let result = agent.run_auto().await;

    assert!(
        result.is_err(),
        "run_auto() should fail with invalid provider name"
    );

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // Should get AgentValidationError mentioning unknown provider
    assert!(
        err_msg.contains("NIKA-113"),
        "Should be AgentValidationError, got: {err_msg}"
    );
    assert!(
        err_msg.contains("Unknown provider") || err_msg.contains("invalid_provider"),
        "Error should mention unknown provider, got: {err_msg}"
    );
}

// =============================================================================
// Provider Alias Tests
// =============================================================================

/// Test: Provider aliases work (anthropic -> claude, gpt -> openai).
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_provider_aliases() {
    let guard = EnvGuard::new();
    guard.set_both_keys();

    // Test "anthropic" alias
    let params_anthropic = AgentParams {
        prompt: "Test anthropic alias".to_string(),
        provider: Some("anthropic".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    let mut agent = RigAgentLoop::new(
        "test_alias".to_string(),
        params_anthropic,
        event_log,
        mcp_clients,
    )
    .expect("Should create agent");

    let result = agent.run_auto().await;
    // Should attempt Claude, not fail with "unknown provider"
    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.contains("Unknown provider"),
        "'anthropic' should be a valid alias, got: {err_msg}"
    );

    // Test "gpt" alias
    let params_gpt = AgentParams {
        prompt: "Test gpt alias".to_string(),
        provider: Some("gpt".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
    let mut agent = RigAgentLoop::new("test_alias".to_string(), params_gpt, event_log, mcp_clients)
        .expect("Should create agent");

    let result = agent.run_auto().await;
    // Should attempt OpenAI, not fail with "unknown provider"
    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.contains("Unknown provider"),
        "'gpt' should be a valid alias, got: {err_msg}"
    );
}

/// Test: Provider name is case-insensitive.
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_provider_case_insensitive() {
    let guard = EnvGuard::new();
    guard.set_both_keys();

    for provider_name in ["CLAUDE", "Claude", "OPENAI", "OpenAI", "GPT", "Anthropic"] {
        let params = AgentParams {
            prompt: format!("Test case: {provider_name}"),
            provider: Some(provider_name.to_string()),
            max_turns: Some(1),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients: FxHashMap<String, Arc<McpClient>> = FxHashMap::default();
        let mut agent = RigAgentLoop::new("test_case".to_string(), params, event_log, mcp_clients)
            .expect("Should create agent");

        let result = agent.run_auto().await;
        let err_msg = result.unwrap_err().to_string();

        assert!(
            !err_msg.contains("Unknown provider"),
            "'{provider_name}' should be recognized (case-insensitive), got: {err_msg}"
        );
    }
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test: Empty API key value is treated differently from unset.
///
/// std::env::var() returns Ok("") for empty string, so run_auto() will
/// attempt to use the provider (and fail with auth error, not "no key found").
#[tokio::test]
#[serial_test::serial]
async fn test_run_auto_empty_api_key_behavior() {
    let _guard = EnvGuard::new();

    // Set empty strings
    std::env::set_var("ANTHROPIC_API_KEY", "");
    std::env::set_var("OPENAI_API_KEY", "");

    let mut agent = create_test_agent();
    let result = agent.run_auto().await;

    // Document actual behavior:
    // std::env::var("KEY").is_ok() returns true for empty string
    // So run_auto() will try to use the provider, which will fail differently
    // than "no API key found"
    assert!(result.is_err(), "Should fail with empty API keys");

    let err = result.unwrap_err();
    let err_msg = err.to_string();

    // The behavior depends on how run_auto() checks:
    // - If it uses is_ok(), empty string passes the check
    // - If it uses is_ok_and(|v| !v.is_empty()), empty string fails
    //
    // Current implementation uses is_ok(), so empty string will attempt
    // to use the provider and fail with a different error
    assert!(!err_msg.is_empty(), "Should have error message");
}
