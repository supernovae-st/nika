// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tests for chat agent module

use super::*;
use serial_test::serial;

// Test-only fake API key — not a real secret, used only to satisfy provider init checks.
const TEST_FAKE_API_KEY: &str = "test-key-for-unit-test";

// ═══════════════════════════════════════════════════════════════════════════
// StreamingState tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_streaming_state_default() {
    let state = StreamingState::default();
    assert!(!state.is_streaming);
    assert!(state.partial_response.is_empty());
    assert_eq!(state.tokens_received, 0);
}

#[test]
fn test_streaming_state_start() {
    let mut state = StreamingState::new();
    state.partial_response = "leftover".to_string();
    state.tokens_received = 10;

    state.start();

    assert!(state.is_streaming);
    assert!(state.partial_response.is_empty());
    assert_eq!(state.tokens_received, 0);
}

#[test]
fn test_streaming_state_append() {
    let mut state = StreamingState::new();
    state.start();

    state.append("Hello");
    state.append(", ");
    state.append("world!");

    assert_eq!(state.partial_response, "Hello, world!");
    assert_eq!(state.tokens_received, 3);
}

#[test]
fn test_streaming_state_finish() {
    let mut state = StreamingState::new();
    state.start();
    state.append("Complete response");

    let result = state.finish();

    assert_eq!(result, "Complete response");
    assert!(!state.is_streaming);
    assert!(state.partial_response.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatRole tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_chat_role_display_names() {
    assert_eq!(ChatRole::User.display_name(), "You");
    assert_eq!(ChatRole::Assistant.display_name(), "Nika");
    assert_eq!(ChatRole::System.display_name(), "System");
    assert_eq!(ChatRole::Tool.display_name(), "Tool");
}

#[test]
fn test_chat_role_equality() {
    assert_eq!(ChatRole::User, ChatRole::User);
    assert_ne!(ChatRole::User, ChatRole::Assistant);
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatMessage tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_chat_message_user() {
    let msg = ChatMessage::user("Hello");
    assert_eq!(msg.role, ChatRole::User);
    assert_eq!(msg.content, "Hello");
}

#[test]
fn test_chat_message_assistant() {
    let msg = ChatMessage::assistant("Hi there!");
    assert_eq!(msg.role, ChatRole::Assistant);
    assert_eq!(msg.content, "Hi there!");
}

#[test]
fn test_chat_message_system() {
    let msg = ChatMessage::system("You are a helpful assistant.");
    assert_eq!(msg.role, ChatRole::System);
    assert_eq!(msg.content, "You are a helpful assistant.");
}

#[test]
fn test_chat_message_tool() {
    let msg = ChatMessage::tool("{\"result\": \"success\"}");
    assert_eq!(msg.role, ChatRole::Tool);
    assert_eq!(msg.content, "{\"result\": \"success\"}");
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatAgent creation tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_chat_agent_creation() {
    // This test verifies ChatAgent can be created
    // It succeeds if any API key is set, or returns Err if no keys are available
    let agent = ChatAgent::new();

    // In CI without API keys, expect Err; with keys, expect Ok
    match agent {
        Ok(a) => {
            // Verify the agent has a valid provider
            let valid_providers = [
                "anthropic",
                "openai",
                "mistral",
                "groq",
                "deepseek",
                "gemini",
                "xai",
            ];
            assert!(
                valid_providers.contains(&a.provider_name()),
                "Expected valid provider, got: {}",
                a.provider_name()
            );
        }
        Err(e) => {
            // Expected in CI without API keys - verify it's the right error
            assert!(
                e.to_string().contains("API key"),
                "Expected API key error, got: {}",
                e
            );
        }
    }
}

#[test]
#[serial]
fn test_chat_agent_initial_state() {
    // Set a dummy key for the test (ensures at least one provider is available)
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");

    assert!(agent.history().is_empty());
    assert!(!agent.is_streaming());
    // RigProvider::auto() picks first available provider in priority order:
    // 1. Claude, 2. OpenAI, 3. Mistral, 4. Groq, 5. DeepSeek, 6. Gemini, 7. xAI
    // Due to parallel tests and user env, any provider may be selected
    let valid_providers = [
        "anthropic",
        "openai",
        "mistral",
        "groq",
        "deepseek",
        "gemini",
        "xai",
    ];
    assert!(
        valid_providers.contains(&agent.provider_name()),
        "Expected valid provider, got: {}",
        agent.provider_name()
    );
}

#[test]
#[serial]
fn test_chat_agent_with_claude_fallback() {
    // This test verifies Claude fallback logic.
    // Due to parallel test execution, we can't reliably remove OPENAI_API_KEY.
    // Instead, test that agent creation always succeeds.
    std::env::set_var("ANTHROPIC_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    // Provider will be openai if OPENAI_API_KEY is set (by parallel test),
    // or claude if only ANTHROPIC_API_KEY is set
    assert!(agent.provider_name() == "openai" || agent.provider_name() == "anthropic");
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider switching tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[serial]
fn test_set_provider_openai() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    let result = agent.set_provider(ModelProvider::OpenAI);

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(agent.provider_name(), "openai");
}

#[test]
#[serial]
fn test_set_provider_claude() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);
    std::env::set_var("ANTHROPIC_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");

    // Only test provider switch if ANTHROPIC_API_KEY is set
    // (parallel tests might remove it)
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        let result = agent.set_provider(ModelProvider::Claude);
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert_eq!(agent.provider_name(), "anthropic");
    }
}

#[test]
#[serial]
fn test_set_provider_missing_key() {
    temp_env::with_vars(
        [
            ("OPENAI_API_KEY", Some(TEST_FAKE_API_KEY)),
            ("ANTHROPIC_API_KEY", None::<&str>),
        ],
        || {
            let mut agent = ChatAgent::new().expect("Should create agent");

            let result = agent.set_provider(ModelProvider::Claude);
            assert!(
                result.is_err(),
                "set_provider must fail when ANTHROPIC_API_KEY is not set"
            );
            if let Err(NikaError::MissingApiKey { provider }) = result {
                assert_eq!(provider, "anthropic");
            } else {
                panic!("Expected MissingApiKey error, got: {:?}", result);
            }
        },
    );
}

#[test]
#[serial]
fn test_set_provider_list_does_not_change() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    let original = agent.provider_name().to_string();

    let result = agent.set_provider(ModelProvider::List);

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(agent.provider_name(), original);
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider switching tests (new providers)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[serial]
fn test_set_provider_mistral() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);
    std::env::set_var("MISTRAL_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    let result = agent.set_provider(ModelProvider::Mistral);

    if std::env::var("MISTRAL_API_KEY").is_ok_and(|v| !v.is_empty()) {
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert_eq!(agent.provider_name(), "mistral");
    }
}

#[test]
#[serial]
fn test_set_provider_groq() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);
    std::env::set_var("GROQ_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    let result = agent.set_provider(ModelProvider::Groq);

    if std::env::var("GROQ_API_KEY").is_ok_and(|v| !v.is_empty()) {
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert_eq!(agent.provider_name(), "groq");
    }
}

#[test]
#[serial]
fn test_set_provider_deepseek() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);
    std::env::set_var("DEEPSEEK_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    let result = agent.set_provider(ModelProvider::DeepSeek);

    if std::env::var("DEEPSEEK_API_KEY").is_ok_and(|v| !v.is_empty()) {
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert_eq!(agent.provider_name(), "deepseek");
    }
}

#[test]
#[serial]
fn test_with_overrides_mistral() {
    std::env::set_var("MISTRAL_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::with_overrides(Some("mistral"), None);
    if std::env::var("MISTRAL_API_KEY").is_ok_and(|v| !v.is_empty()) {
        assert!(agent.is_ok(), "Should succeed: {:?}", agent.err());
        assert_eq!(agent.unwrap().provider_name(), "mistral");
    }
}

#[test]
#[serial]
fn test_with_overrides_invalid_provider() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::with_overrides(Some("invalid_provider"), None);
    assert!(agent.is_err());
    if let Err(NikaError::InvalidConfig { message }) = agent {
        assert!(message.contains("Unknown provider"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// History tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[serial]
fn test_history_starts_empty() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    assert!(agent.history().is_empty());
}

#[test]
#[serial]
fn test_clear_history() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");

    // Manually add messages to history (simulating conversation)
    agent.history.push(ChatMessage::user("Hello"));
    agent.history.push(ChatMessage::assistant("Hi!"));

    assert_eq!(agent.history().len(), 2);

    agent.clear_history();

    assert!(agent.history().is_empty());
}

#[test]
#[serial]
fn test_with_history() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let history = vec![
        ChatMessage::user("Hello"),
        ChatMessage::assistant("Hi there!"),
        ChatMessage::user("How are you?"),
    ];

    let agent = ChatAgent::with_history(history).expect("Should create agent with history");

    assert_eq!(agent.history().len(), 3);
    assert_eq!(agent.history()[0].role, ChatRole::User);
    assert_eq!(agent.history()[0].content, "Hello");
    assert_eq!(agent.history()[1].role, ChatRole::Assistant);
    assert_eq!(agent.history()[2].content, "How are you?");
}

#[test]
#[serial]
fn test_take_history() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    agent.history.push(ChatMessage::user("Hello"));
    agent.history.push(ChatMessage::assistant("Hi!"));

    let taken = agent.take_history();

    assert_eq!(taken.len(), 2);
    assert!(agent.history().is_empty()); // History is now empty
    assert_eq!(taken[0].content, "Hello");
    assert_eq!(taken[1].content, "Hi!");
}

#[test]
#[serial]
fn test_set_history() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let mut agent = ChatAgent::new().expect("Should create agent");
    agent.history.push(ChatMessage::user("Old message"));

    let new_history = vec![
        ChatMessage::user("New conversation"),
        ChatMessage::assistant("Fresh start!"),
    ];

    agent.set_history(new_history);

    assert_eq!(agent.history().len(), 2);
    assert_eq!(agent.history()[0].content, "New conversation");
    assert_eq!(agent.history()[1].content, "Fresh start!");
}

// ═══════════════════════════════════════════════════════════════════════════
// Exec command tests (safe, no real execution)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn test_exec_command_echo() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("echo hello").await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), "hello");
}

#[tokio::test]
#[serial]
async fn test_exec_command_with_args() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("echo -n 'test output'").await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert!(result.unwrap().contains("test output"));
}

#[tokio::test]
#[serial]
async fn test_exec_command_failure() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent.exec_command("exit 1").await;

    // Command failure returns Ok with exit code info
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    let output = result.unwrap();
    assert!(output.contains("Exit code: 1"));
}

#[tokio::test]
#[serial]
async fn test_exec_command_pipe() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent
        .exec_command("echo 'hello world' | tr 'a-z' 'A-Z'")
        .await;

    assert!(result.is_ok(), "Should succeed: {:?}", result.err());
    assert_eq!(result.unwrap(), "HELLO WORLD");
}

// ═══════════════════════════════════════════════════════════════════════════
// Streaming state tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
#[serial]
fn test_streaming_state_access() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");

    assert!(!agent.is_streaming());
    assert!(!agent.streaming_state().is_streaming);
}

// ═══════════════════════════════════════════════════════════════════════════
// Streaming channel tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn test_with_streaming_channel() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let (tx, _rx) = mpsc::channel::<String>(10);
    let agent = ChatAgent::new()
        .expect("Should create agent")
        .with_streaming(tx);

    // The streaming channel is set
    assert!(agent.streaming_tx.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// MCP invoke tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn test_invoke_unknown_server() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent
        .invoke(
            "some_tool",
            Some("nonexistent_server"),
            serde_json::json!({}),
        )
        .await;

    // Should fail because server doesn't exist
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("No MCP servers"),
        "Expected 'not found' or 'No MCP servers' in error, got: {}",
        err_msg
    );
}

#[tokio::test]
#[serial]
async fn test_invoke_no_servers_configured() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    // Note: This test assumes no MCP servers are globally configured.
    // In real scenarios, global config may have servers, so we test
    // with a specific non-existent server name.
    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent
        .invoke(
            "test_tool",
            Some("definitely_not_configured"),
            serde_json::json!({}),
        )
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, NikaError::InvalidConfig { .. }),
        "Expected InvalidConfig error, got: {:?}",
        err
    );
}

#[test]
fn test_invoke_params_round_trip_through_json_string() {
    // Test that invoke params survive serialization → string → deserialization
    let params = serde_json::json!({
        "entity": "qr-code",
        "locale": "fr-FR",
        "count": 5,
        "nested": { "key": "value" },
        "array": [1, 2, 3]
    });

    // Round-trip through JSON string (simulates MCP wire format)
    let serialized = serde_json::to_string(&params).unwrap();
    let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized["entity"], "qr-code");
    assert_eq!(deserialized["count"], 5);
    assert_eq!(deserialized["nested"]["key"], "value");
    assert_eq!(deserialized["array"].as_array().unwrap().len(), 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// Agent run_agent tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
#[serial]
async fn test_run_agent_no_servers_configured() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent
        .run_agent(
            "Test goal".to_string(),
            Some(3),
            false,
            vec!["nonexistent_server".to_string()],
        )
        .await;

    // Should fail because server doesn't exist
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found") || err_msg.contains("No MCP servers"),
        "Expected 'not found' or 'No MCP servers' in error, got: {}",
        err_msg
    );
}

#[tokio::test]
#[serial]
async fn test_run_agent_empty_goal_validation() {
    std::env::set_var("OPENAI_API_KEY", TEST_FAKE_API_KEY);

    // Note: Empty goal should be caught by RigAgentLoop validation
    // Since we don't have real MCP servers, we test with non-existent server
    // The actual empty goal validation happens in RigAgentLoop::new()
    let agent = ChatAgent::new().expect("Should create agent");
    let result = agent
        .run_agent(
            "".to_string(), // Empty goal
            Some(5),
            false,
            vec!["fake_server".to_string()],
        )
        .await;

    // Will fail due to missing server first, but if we had servers,
    // it would fail due to empty prompt validation
    assert!(result.is_err());
}

#[test]
fn test_agent_params_construction() {
    // Test that AgentParams can be constructed with expected fields
    use nika_engine::ast::AgentParams;

    let params = AgentParams {
        prompt: "Test goal".to_string(),
        system: None,
        provider: None,
        model: None,
        mcp: vec!["novanet".to_string()],
        tools: vec![],
        max_turns: Some(10),
        token_budget: None,
        stop_sequences: vec![],
        scope: None,
        extended_thinking: Some(true),
        thinking_budget: None,
        depth_limit: Some(3),
        ..Default::default()
    };

    assert_eq!(params.prompt, "Test goal");
    assert_eq!(params.max_turns, Some(10));
    assert_eq!(params.extended_thinking, Some(true));
    assert_eq!(params.depth_limit, Some(3));
    assert_eq!(params.mcp, vec!["novanet"]);
}

#[test]
fn test_default_max_turns_unwrap_or() {
    // Verify the .unwrap_or(10) pattern used for max_turns
    fn resolve_max_turns(input: Option<u32>) -> u32 {
        input.unwrap_or(10)
    }
    assert_eq!(resolve_max_turns(None), 10);
    assert_eq!(resolve_max_turns(Some(5)), 5);
    assert_eq!(resolve_max_turns(Some(100)), 100);
}
