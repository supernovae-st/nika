// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration tests for reasoning capture and token tracking (v0.4.1 → v0.7.2)
//!
//! These tests verify that extended thinking and token tracking work correctly.
//!
//! **v0.7.2 Updates:**
//! - Standard mode (no tools) now uses streaming for token tracking
//! - Token tracking works for Claude, OpenAI, and all 6 providers when no tools
//! - When tools are used, tokens are still 0 (rig-core agent.prompt() limitation)
//!
//! Run with: cargo test --test thinking_capture_test -- --ignored

use nika::ast::AgentParams;
use nika::event::{EventKind, EventLog};
use nika::runtime::RigAgentLoop;
use rustc_hash::FxHashMap;

/// Test that tokens are captured when using extended thinking mode.
///
/// This test verifies that:
/// 1. Thinking content is captured in metadata
/// 2. Input/output tokens are non-zero (not hardcoded to 0)
/// 3. Stop reason is correct
#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY - run with: cargo test --test thinking_capture_test -- --ignored"]
async fn test_extended_thinking_captures_tokens() {
    // Skip if no API key
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    // Arrange
    let params = AgentParams {
        prompt: "What is 2+2? Think step by step before answering.".to_string(),
        extended_thinking: Some(true),
        thinking_budget: Some(1024), // Small budget for fast test
        provider: Some(nika_core::ProviderName::Anthropic),
        model: Some("claude-sonnet-4-6".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let mut agent = RigAgentLoop::new(
        "test-thinking".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
        None,
        None,
    )
    .expect("Agent creation should succeed");

    // Act
    let result = agent.run().await;

    // Assert - execution should succeed
    assert!(
        result.is_ok(),
        "Agent execution should succeed: {:?}",
        result.err()
    );

    let result = result.unwrap();
    println!("Result: {:?}", result);

    // Find the AgentTurn completion event
    let events = event_log.events();
    let completion_event = events.iter().find(|e| {
        matches!(
            &e.kind,
            EventKind::AgentTurn {
                kind,
                metadata: Some(_),
                ..
            } if *kind != nika::event::AgentTurnKind::Started
        )
    });

    assert!(
        completion_event.is_some(),
        "Should have AgentTurn completion event with metadata"
    );

    if let EventKind::AgentTurn {
        metadata: Some(metadata),
        ..
    } = &completion_event.unwrap().kind
    {
        // CRITICAL: Tokens should be non-zero when using streaming API
        // This is the main assertion that will FAIL before our fix
        assert!(
            metadata.input_tokens > 0,
            "input_tokens should be non-zero, got {}",
            metadata.input_tokens
        );
        assert!(
            metadata.output_tokens > 0,
            "output_tokens should be non-zero, got {}",
            metadata.output_tokens
        );

        // Thinking should be captured (extended_thinking is enabled)
        // Note: This might be None if the model doesn't produce thinking for simple prompts
        println!("Thinking captured: {:?}", metadata.thinking.is_some());
        println!("Response: {}", metadata.response_text);
        println!(
            "Tokens: in={}, out={}",
            metadata.input_tokens, metadata.output_tokens
        );

        // Response should contain an answer
        assert!(
            !metadata.response_text.is_empty(),
            "Response should not be empty"
        );
    } else {
        panic!("Expected AgentTurn with metadata");
    }
}

/// Test that tokens are captured in standard mode (v0.7.2 streaming migration).
///
/// With v0.7.2, standard mode (no tools) uses streaming, which enables token tracking.
/// This test verifies that tokens ARE captured when not using tools.
#[tokio::test]
#[ignore = "requires ANTHROPIC_API_KEY - run with: cargo test --test thinking_capture_test -- --ignored"]
async fn test_standard_mode_captures_tokens() {
    // Skip if no API key
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("Skipping: ANTHROPIC_API_KEY not set");
        return;
    }

    // Arrange
    let params = AgentParams {
        prompt: "Say hello in exactly one word.".to_string(),
        extended_thinking: Some(false), // Explicitly disabled
        provider: Some(nika_core::ProviderName::Anthropic),
        model: Some("claude-sonnet-4-6".to_string()),
        max_turns: Some(1),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let mut agent = RigAgentLoop::new(
        "test-standard".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
        None,
        None,
    )
    .expect("Agent creation should succeed");

    // Act
    let result = agent.run().await;

    // Assert
    assert!(result.is_ok(), "Agent execution should succeed");

    let events = event_log.events();
    let completion_event = events.iter().find(|e| {
        matches!(
            &e.kind,
            EventKind::AgentTurn {
                kind,
                metadata: Some(_),
                ..
            } if *kind != nika::event::AgentTurnKind::Started
        )
    });

    assert!(
        completion_event.is_some(),
        "Should have AgentTurn completion event"
    );

    if let EventKind::AgentTurn {
        metadata: Some(metadata),
        ..
    } = &completion_event.unwrap().kind
    {
        // v0.7.2: Standard mode now uses streaming when no tools are present,
        // which enables token tracking. Tokens should be non-zero.
        println!(
            "Standard mode tokens: in={}, out={}",
            metadata.input_tokens, metadata.output_tokens
        );
        println!("Response: {}", metadata.response_text);

        // CRITICAL: Tokens should now be tracked in standard mode (v0.7.2 streaming migration)
        assert!(
            metadata.input_tokens > 0,
            "input_tokens should be non-zero in standard mode, got {}",
            metadata.input_tokens
        );
        assert!(
            metadata.output_tokens > 0,
            "output_tokens should be non-zero in standard mode, got {}",
            metadata.output_tokens
        );

        // Response should exist
        assert!(!metadata.response_text.is_empty());

        // Thinking should NOT be captured in standard mode
        assert!(
            metadata.thinking.is_none(),
            "Thinking should be None in standard mode"
        );
    }
}

/// Test that mock mode works correctly (for CI without API key).
#[tokio::test]
async fn test_mock_mode_has_tokens() {
    // Arrange
    let params = AgentParams {
        prompt: "Test prompt".to_string(),
        max_turns: Some(1),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "test-mock".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
        None,
        None,
    )
    .expect("Agent creation should succeed");

    // Act
    let result = agent.run_mock().await;

    // Assert
    assert!(result.is_ok());

    let events = event_log.events();
    let completion_event = events.iter().find(|e| {
        matches!(
            &e.kind,
            EventKind::AgentTurn {
                kind,
                metadata: Some(_),
                ..
            } if *kind != nika::event::AgentTurnKind::Started
        )
    });

    assert!(completion_event.is_some());

    if let EventKind::AgentTurn {
        metadata: Some(metadata),
        ..
    } = &completion_event.unwrap().kind
    {
        // Mock mode should have non-zero tokens (hardcoded test values)
        assert_eq!(
            metadata.input_tokens, 50,
            "Mock should have input_tokens=50"
        );
        assert_eq!(
            metadata.output_tokens, 50,
            "Mock should have output_tokens=50"
        );
        assert_eq!(metadata.response_text, "Mock response from rig agent");
    }
}

/// Test that OpenAI also captures tokens in standard mode (v0.7.2).
///
/// Verifies that the streaming migration works across providers.
#[tokio::test]
#[ignore = "requires OPENAI_API_KEY - run with: cargo test --test thinking_capture_test -- --ignored"]
async fn test_openai_standard_mode_captures_tokens() {
    // Skip if no API key
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("Skipping: OPENAI_API_KEY not set");
        return;
    }

    // Arrange
    let params = AgentParams {
        prompt: "Say hello in exactly one word.".to_string(),
        provider: Some(nika_core::ProviderName::OpenAI),
        model: Some("gpt-4o-mini".to_string()), // Use cheaper model for tests
        max_turns: Some(1),
        ..Default::default()
    };

    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let mut agent = RigAgentLoop::new(
        "test-openai".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
        None,
        None,
    )
    .expect("Agent creation should succeed");

    // Act
    let result = agent.run().await;

    // Assert
    assert!(result.is_ok(), "Agent execution should succeed");

    let events = event_log.events();
    let completion_event = events.iter().find(|e| {
        matches!(
            &e.kind,
            EventKind::AgentTurn {
                kind,
                metadata: Some(_),
                ..
            } if *kind != nika::event::AgentTurnKind::Started
        )
    });

    assert!(
        completion_event.is_some(),
        "Should have AgentTurn completion event"
    );

    if let EventKind::AgentTurn {
        metadata: Some(metadata),
        ..
    } = &completion_event.unwrap().kind
    {
        // v0.7.2: OpenAI should also have token tracking via streaming
        println!(
            "OpenAI tokens: in={}, out={}",
            metadata.input_tokens, metadata.output_tokens
        );
        println!("Response: {}", metadata.response_text);

        // CRITICAL: Tokens should be tracked for OpenAI too
        assert!(
            metadata.input_tokens > 0,
            "input_tokens should be non-zero for OpenAI, got {}",
            metadata.input_tokens
        );
        assert!(
            metadata.output_tokens > 0,
            "output_tokens should be non-zero for OpenAI, got {}",
            metadata.output_tokens
        );

        // Response should exist
        assert!(!metadata.response_text.is_empty());
    }
}
