// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration tests for Agent Completion v0.21
//!
//! Tests the complete flow of completion configuration:
//! 1. YAML parsing → CompletionConfig
//! 2. System instruction generation
//! 3. nika:complete tool execution
//! 4. Completion signal detection in RigAgentLoop
//!
//! These tests don't require external services (MCP, LLM).

use nika::ast::completion::{
    CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction, OnLowConfidenceConfig,
    PatternConfig, PatternType, SignalConfig, SignalFields,
};
use nika::ast::AgentParams;
use nika::event::EventLog;
use nika::runtime::builtin::{
    is_completion_signal, parse_completion_response, CompleteTool, COMPLETION_MARKER,
};
// CompleteTool implements kernel BuiltinTool, not engine BuiltinTool
use nika::runtime::{RigAgentLoop, RigAgentStatus};
use nika_kernel::builtin::BuiltinTool;
use rustc_hash::FxHashMap;

// ============================================================================
// System Instruction Generation Tests
// ============================================================================

#[test]
fn test_explicit_mode_generates_complete_tool_instruction() {
    let config = CompletionConfig {
        mode: CompletionMode::Explicit,
        signal: Some(SignalConfig {
            tool: "nika:complete".to_string(),
            fields: SignalFields {
                required: vec!["result".to_string()],
                optional: vec!["confidence".to_string()],
            },
        }),
        instruction: None,
        confidence: None,
        patterns: vec![],
    };

    let instruction = config.generate_system_instruction();

    // Should mention nika:complete
    assert!(instruction.contains("nika:complete"));
    // Should mention required fields
    assert!(instruction.contains("result"));
}

#[test]
fn test_pattern_mode_generates_pattern_instruction() {
    let config = CompletionConfig {
        mode: CompletionMode::Pattern,
        patterns: vec![PatternConfig::new("DONE", PatternType::Exact)],
        instruction: None,
        confidence: None,
        signal: None,
    };

    let instruction = config.generate_system_instruction();

    // Should mention the pattern or completion concept
    assert!(
        instruction.contains("DONE")
            || instruction.contains("complete")
            || instruction.contains("pattern")
    );
}

#[test]
fn test_natural_mode_generates_minimal_instruction() {
    let config = CompletionConfig {
        mode: CompletionMode::Natural,
        instruction: None,
        confidence: None,
        signal: None,
        patterns: vec![],
    };

    let instruction = config.generate_system_instruction();

    // Natural mode should generate minimal instruction
    // Either empty or short
    assert!(instruction.len() < 200);
}

// ============================================================================
// Agent Params Integration Tests
// ============================================================================

#[test]
fn test_agent_params_with_completion_config() {
    let params = AgentParams {
        prompt: "Test agent".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            signal: None,
            patterns: vec![],
            confidence: None,
            instruction: None,
        }),
        ..Default::default()
    };

    assert!(params.completion.is_some());
    assert_eq!(
        params.completion.as_ref().unwrap().mode,
        CompletionMode::Explicit
    );
}

#[test]
fn test_agent_params_effective_completion_with_config() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            instruction: None,
            confidence: None,
            signal: None,
            patterns: vec![],
        }),
        ..Default::default()
    };

    let effective = params.effective_completion();
    assert!(effective.is_some());
    assert_eq!(effective.unwrap().mode, CompletionMode::Explicit);
}

#[test]
fn test_agent_params_completion_system_instruction() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            signal: Some(SignalConfig {
                tool: "nika:complete".to_string(),
                fields: SignalFields {
                    required: vec!["result".to_string()],
                    optional: vec![],
                },
            }),
            instruction: None,
            confidence: None,
            patterns: vec![],
        }),
        ..Default::default()
    };

    let instruction = params.completion_system_instruction();
    assert!(!instruction.is_empty());
    assert!(instruction.contains("nika:complete"));
}

// ============================================================================
// nika:complete Tool Tests
// ============================================================================

#[tokio::test]
async fn test_complete_tool_returns_marker() {
    let tool = CompleteTool;

    let result = tool
        .call(r#"{"result": "Task completed successfully"}"#.to_string())
        .await
        .unwrap();

    // Should contain the completion marker
    assert!(result.contains(COMPLETION_MARKER));
}

#[tokio::test]
async fn test_complete_tool_with_confidence() {
    let tool = CompleteTool;

    let result = tool
        .call(r#"{"result": "Answer is 42", "confidence": 0.95}"#.to_string())
        .await
        .unwrap();

    // Parse the response
    let response = parse_completion_response(&result).unwrap();
    assert!(response.completed);
    assert_eq!(response.confidence, Some(0.95));
}

#[tokio::test]
async fn test_complete_tool_validates_confidence_range() {
    let tool = CompleteTool;

    // Confidence > 1.0 should fail
    let result = tool
        .call(r#"{"result": "x", "confidence": 1.5}"#.to_string())
        .await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("confidence"));
}

// ============================================================================
// Completion Signal Detection Tests
// ============================================================================

#[test]
fn test_is_completion_signal_detects_marker() {
    let response = format!(
        r#"{{"completed": true, "marker": "{}"}}"#,
        COMPLETION_MARKER
    );

    assert!(is_completion_signal("nika:complete", &response));
    assert!(is_completion_signal("complete", &response));
}

#[test]
fn test_is_completion_signal_rejects_wrong_tool() {
    let response = format!(r#"{{"marker": "{}"}}"#, COMPLETION_MARKER);

    assert!(!is_completion_signal("nika:log", &response));
    assert!(!is_completion_signal("other", &response));
}

#[test]
fn test_is_completion_signal_rejects_missing_marker() {
    let response = r#"{"completed": true}"#;

    assert!(!is_completion_signal("nika:complete", response));
}

// ============================================================================
// RigAgentLoop Status Detection Tests
// ============================================================================

#[test]
fn test_rig_agent_loop_explicit_completion_status() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    // Response with completion marker
    let response = format!(r#"Result: {{"marker": "{}"}}"#, COMPLETION_MARKER);
    let status = agent.determine_status(&response);

    assert_eq!(status, RigAgentStatus::ExplicitCompletion);
}

#[test]
fn test_rig_agent_loop_natural_completion_status() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    // Regular response, no marker, no stop condition
    let status = agent.determine_status("Task completed normally.");

    assert_eq!(status, RigAgentStatus::NaturalCompletion);
}

#[test]
fn test_rig_agent_loop_explicit_takes_priority() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    // Response with completion marker
    let response = format!("Result with marker: {}", COMPLETION_MARKER);
    let status = agent.determine_status(&response);

    // Explicit completion detected
    assert_eq!(status, RigAgentStatus::ExplicitCompletion);
}

// ============================================================================
// Status Canonical String Tests
// ============================================================================

#[test]
fn test_explicit_completion_canonical_str() {
    assert_eq!(
        RigAgentStatus::ExplicitCompletion.as_canonical_str(),
        "tool_complete"
    );
}

#[test]
fn test_all_status_canonical_strs() {
    assert_eq!(
        RigAgentStatus::NaturalCompletion.as_canonical_str(),
        "end_turn"
    );
    assert_eq!(
        RigAgentStatus::ExplicitCompletion.as_canonical_str(),
        "tool_complete"
    );
    assert_eq!(
        RigAgentStatus::MaxTurnsReached.as_canonical_str(),
        "max_turns"
    );
    assert_eq!(
        RigAgentStatus::TokenBudgetExceeded.as_canonical_str(),
        "max_tokens"
    );
    assert_eq!(RigAgentStatus::Failed.as_canonical_str(), "error");
}

// ============================================================================
// Validation Tests
// ============================================================================

#[test]
fn test_completion_config_validation_valid() {
    let config = CompletionConfig {
        mode: CompletionMode::Explicit,
        signal: Some(SignalConfig::default()),
        patterns: vec![],
        confidence: Some(ConfidenceConfig {
            threshold: 0.8,
            on_low: OnLowConfidenceConfig {
                action: LowConfidenceAction::Retry,
                max_retries: 3,
                feedback: None,
            },
            routing: None,
        }),
        instruction: None,
    };

    assert!(config.validate().is_ok());
}

#[test]
fn test_completion_config_validation_invalid_threshold() {
    let config = CompletionConfig {
        mode: CompletionMode::Explicit,
        signal: None,
        patterns: vec![],
        confidence: Some(ConfidenceConfig {
            threshold: 1.5, // Invalid: > 1.0
            on_low: OnLowConfidenceConfig::default(),
            routing: None,
        }),
        instruction: None,
    };

    assert!(config.validate().is_err());
}

#[test]
fn test_pattern_mode_without_patterns_fails() {
    let config = CompletionConfig {
        mode: CompletionMode::Pattern,
        signal: None,
        patterns: vec![], // Empty patterns
        confidence: None,
        instruction: None,
    };

    assert!(config.validate().is_err());
}
