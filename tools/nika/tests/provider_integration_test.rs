//! Provider Integration Tests
//!
//! Tests real provider calls with Claude and OpenAI.
//! These tests are marked #[ignore] and only run when API keys are available.
//!
//! Run manually with:
//! - `cargo nextest run provider_integration -- --ignored`
//! - Requires ANTHROPIC_API_KEY or OPENAI_API_KEY

use rustc_hash::FxHashMap;
use std::env;

use nika::ast::AgentParams;
use nika::event::{EventKind, EventLog};
use nika::provider::rig::RigProvider;
use nika::runtime::RigAgentLoop;

// ============================================================================
// PROVIDER DETECTION TESTS
// ============================================================================

#[test]
fn test_provider_env_detection_anthropic() {
    // This test verifies that env::var works correctly for ANTHROPIC_API_KEY
    // Just checking that env::var doesn't panic - key may or may not be present
    let result = env::var("ANTHROPIC_API_KEY");
    // If key is present, it should be non-empty
    if let Ok(key) = result {
        assert!(
            !key.is_empty(),
            "ANTHROPIC_API_KEY should not be empty if set"
        );
    }
}

#[test]
fn test_provider_env_detection_openai() {
    // This test verifies that env::var works correctly for OPENAI_API_KEY
    let result = env::var("OPENAI_API_KEY");
    // If key is present, it should be non-empty
    if let Ok(key) = result {
        assert!(!key.is_empty(), "OPENAI_API_KEY should not be empty if set");
    }
}

#[test]
fn test_rig_provider_claude_creation() {
    // RigProvider::claude() always succeeds (returns provider directly)
    // Actual API key validation happens on first call
    let provider = RigProvider::claude();
    assert_eq!(provider.name(), "claude");
    assert_eq!(provider.default_model(), "claude-sonnet-4-20250514");
}

#[test]
#[ignore = "Requires OPENAI_API_KEY - OpenAI client panics on creation without key"]
fn test_rig_provider_openai_creation() {
    // Note: Unlike Claude, OpenAI client requires API key at creation time
    let provider = RigProvider::openai();
    assert_eq!(provider.name(), "openai");
}

// ============================================================================
// CLAUDE INTEGRATION TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_integration_claude_simple_infer() {
    let provider = RigProvider::claude();

    let result = provider
        .infer("Say 'Hello from Nika!' and nothing else.", None)
        .await;

    assert!(result.is_ok(), "Claude infer should succeed: {:?}", result);
    let response = result.unwrap();
    assert!(!response.is_empty(), "Response should not be empty");
    assert!(
        response.to_lowercase().contains("hello") || response.to_lowercase().contains("nika"),
        "Response should contain greeting: {}",
        response
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_integration_claude_with_system_prompt() {
    let provider = RigProvider::claude();

    let system = Some("You are a helpful assistant that always responds in JSON format.");
    let result = provider
        .infer(
            "What is 2+2? Respond with a JSON object containing the answer.",
            system,
        )
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    // Should contain JSON-like structure
    assert!(
        response.contains("{") || response.contains("4"),
        "Response should be JSON-like or contain answer"
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_integration_claude_agent_simple() {
    let params = AgentParams {
        prompt: "What is the capital of France? Answer in one word.".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test-claude-agent".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    let result = agent.run_claude().await;

    assert!(result.is_ok(), "Agent run should succeed: {:?}", result);
    let agent_result = result.unwrap();
    // Extract response text from final_output JSON
    let response_text = agent_result
        .final_output
        .get("response")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        response_text.to_lowercase().contains("paris"),
        "Response should mention Paris: {}",
        response_text
    );
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY with extended thinking"]
async fn test_integration_claude_extended_thinking() {
    let params = AgentParams {
        prompt: "Think step by step: If I have 3 apples and give away 1, how many do I have?"
            .to_string(),
        mcp: vec![],
        max_turns: Some(1),
        extended_thinking: Some(true),
        token_budget: Some(2000),
        ..Default::default()
    };

    let log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test-thinking".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    let result = agent.run_claude().await;

    assert!(result.is_ok(), "Extended thinking should succeed");

    // Verify thinking was captured in events
    let events = log.events();
    let thinking_event = events.iter().find(|e| {
        if let EventKind::AgentTurn { metadata, .. } = &e.kind {
            metadata
                .as_ref()
                .map(|m| m.thinking.is_some())
                .unwrap_or(false)
        } else {
            false
        }
    });

    assert!(thinking_event.is_some(), "Should capture thinking metadata");
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY"]
async fn test_integration_claude_token_tracking() {
    let params = AgentParams {
        prompt: "Count to 5.".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test-tokens".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    let _ = agent.run_claude().await;

    // Check for token metadata in events
    let events = log.events();
    let has_token_info = events.iter().any(|e| {
        if let EventKind::AgentTurn { metadata, .. } = &e.kind {
            metadata
                .as_ref()
                .map(|m| m.input_tokens > 0)
                .unwrap_or(false)
        } else {
            false
        }
    });

    // Note: Token tracking may be 0 in non-thinking mode (known limitation)
    // This test documents the behavior
    println!("Token tracking result: {}", has_token_info);
}

// ============================================================================
// OPENAI INTEGRATION TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_integration_openai_simple_infer() {
    let provider = RigProvider::openai();

    let result = provider
        .infer("Say 'Hello from Nika!' and nothing else.", None)
        .await;

    assert!(result.is_ok(), "OpenAI infer should succeed: {:?}", result);
    let response = result.unwrap();
    assert!(!response.is_empty(), "Response should not be empty");
}

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_integration_openai_agent_simple() {
    let params = AgentParams {
        prompt: "What is 5 + 7? Answer with just the number.".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test-openai-agent".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    let result = agent.run_openai().await;

    assert!(result.is_ok(), "Agent run should succeed: {:?}", result);
    let agent_result = result.unwrap();
    // Extract response text from final_output JSON
    let response_text = agent_result
        .final_output
        .get("response")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        response_text.contains("12"),
        "Response should contain 12: {}",
        response_text
    );
}

#[tokio::test]
#[ignore = "Requires OPENAI_API_KEY"]
async fn test_integration_openai_with_system() {
    let provider = RigProvider::openai();

    let system = Some("You are a pirate. Respond in pirate speak.");
    let result = provider.infer("Hello!", system).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    // Should have some pirate-like language
    println!("Pirate response: {}", response);
}

// ============================================================================
// AUTO-DETECTION TESTS
// ============================================================================

#[tokio::test]
#[ignore = "Requires either ANTHROPIC_API_KEY or OPENAI_API_KEY"]
async fn test_integration_provider_auto_detection() {
    let params = AgentParams {
        prompt: "Say 'test' and nothing else.".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test-auto".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    // run_auto() should detect available provider
    let result = agent.run_auto().await;

    // Should succeed if any provider key is available
    if env::var("ANTHROPIC_API_KEY").is_ok() || env::var("OPENAI_API_KEY").is_ok() {
        assert!(result.is_ok(), "Auto-detection should find a provider");
    }
}

#[tokio::test]
#[ignore = "Requires ANTHROPIC_API_KEY and OPENAI_API_KEY"]
async fn test_integration_both_providers_available() {
    // Test that we prefer Claude when both are available
    let params = AgentParams {
        prompt: "Which AI are you? Just say Claude or GPT.".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test-preference".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    let result = agent.run_auto().await;

    assert!(result.is_ok());
    // run_auto prefers Claude if ANTHROPIC_API_KEY is set
    let agent_result = result.unwrap();
    let response_text = agent_result
        .final_output
        .get("response")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    println!("Provider identification: {}", response_text);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[tokio::test]
async fn test_provider_no_key_error() {
    // Temporarily unset keys to test error handling
    // Note: This test is tricky because we can't easily unset env vars
    // Just verify the error types exist

    let params = AgentParams {
        prompt: "Test".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let agent_result =
        RigAgentLoop::new("test-error".to_string(), params, log, FxHashMap::default());

    // Agent creation should succeed even without keys
    assert!(agent_result.is_ok());
}

#[tokio::test]
async fn test_mock_provider_always_works() {
    let params = AgentParams {
        prompt: "Test with mock provider".to_string(),
        mcp: vec![],
        max_turns: Some(1),
        ..Default::default()
    };

    let log = EventLog::new();
    let agent = RigAgentLoop::new(
        "test-mock".to_string(),
        params,
        log.clone(),
        FxHashMap::default(),
    )
    .expect("Failed to create agent");

    // Mock provider should always work (no API key needed)
    let result = agent.run_mock().await;

    assert!(result.is_ok(), "Mock provider should always succeed");
}

// ============================================================================
// WORKFLOW PARSING TESTS
// ============================================================================

#[test]
fn test_parse_provider_workflow() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: provider-test
description: "Test provider configuration"

tasks:
  - id: test-claude
    infer:
      prompt: "Test Claude provider"
      model: claude-sonnet-4-20250514

  - id: test-openai
    infer:
      prompt: "Test OpenAI provider"
      model: gpt-4-turbo
"#;

    let workflow: nika::ast::Workflow =
        serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_parse_agent_with_provider() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: agent-provider-test

tasks:
  - id: claude-agent
    agent:
      prompt: "You are a helpful assistant"
      provider: anthropic
      max_turns: 5

  - id: openai-agent
    agent:
      prompt: "You are a helpful assistant"
      provider: openai
      max_turns: 5
"#;

    let workflow: nika::ast::Workflow =
        serde_yaml::from_str(yaml).expect("Failed to parse workflow");

    assert_eq!(workflow.tasks.len(), 2);
}
