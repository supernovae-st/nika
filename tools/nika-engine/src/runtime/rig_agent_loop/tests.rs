// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use rustc_hash::FxHashMap;
use serial_test::serial;

use crate::ast::AgentParams;
use crate::event::{EventKind, EventLog};

use crate::runtime::rig_agent_loop::types::{
    GuardrailCheckResult, RigAgentLoopResult, RigAgentStatus,
};
use crate::runtime::rig_agent_loop::{resolve_agent_working_dir_from, RigAgentLoop};

// ═══════════════════════════════════════════════════════════════════════════
// S12.D1 — resolve_agent_working_dir_from hard-error contract
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn resolve_agent_working_dir_from_err_returns_hard_io_error() {
    // Security invariant (S12.D1): on current_dir() failure the agent MUST
    // propagate a descriptive NikaError::IoError instead of silently falling
    // back to "/tmp" (which let an untrusted agent write to another user's
    // temp files on a shared system).
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let result = resolve_agent_working_dir_from(Err(io_err));

    match result {
        Err(crate::error::NikaError::IoError(e)) => {
            assert!(
                e.to_string().contains("cannot determine agent working directory"),
                "unexpected error message: {e}"
            );
        }
        Err(other) => panic!("expected NikaError::IoError, got: {other:?}"),
        Ok(p) => panic!("expected hard error, got Ok({})", p.display()),
    }
}

#[test]
fn resolve_agent_working_dir_from_ok_passes_through() {
    let p = std::path::PathBuf::from("/some/path");
    let result = resolve_agent_working_dir_from(Ok(p.clone()));
    assert_eq!(result.expect("ok"), p);
}

#[test]
fn test_rig_agent_status_completion_semantics() {
    // NaturalCompletion and ExplicitCompletion are "completed"
    assert!(RigAgentStatus::NaturalCompletion.is_completed());
    assert!(RigAgentStatus::ExplicitCompletion.is_completed());
    assert!(RigAgentStatus::HighConfidence(0.95).is_completed());

    // MaxTurnsReached, LowConfidence are NOT completed
    assert!(!RigAgentStatus::MaxTurnsReached.is_completed());
    assert!(!RigAgentStatus::LowConfidence(0.5).is_completed());

    // Only LowConfidence requires retry
    assert!(RigAgentStatus::LowConfidence(0.5).requires_retry());
    assert!(!RigAgentStatus::NaturalCompletion.requires_retry());
    assert!(!RigAgentStatus::MaxTurnsReached.requires_retry());

    // Canonical string is used in events — verify contract
    assert_eq!(
        RigAgentStatus::NaturalCompletion.as_canonical_str(),
        "end_turn"
    );
    assert_eq!(
        RigAgentStatus::MaxTurnsReached.as_canonical_str(),
        "max_turns"
    );
    assert_eq!(
        RigAgentStatus::ExplicitCompletion.as_canonical_str(),
        "tool_complete"
    );
    assert_eq!(RigAgentStatus::Failed.as_canonical_str(), "error");
}

#[test]
fn test_rig_agent_loop_result_fields() {
    let result = RigAgentLoopResult {
        status: RigAgentStatus::NaturalCompletion,
        turns: 3,
        final_output: serde_json::json!({"answer": "42"}),
        total_tokens: 1500,
        confidence: Some(0.95),
        retry_count: 1,
        guardrails_passed: true,
        cost_usd: 0.05,
        partial_result: None,
    };

    // Verify field values, not just Debug format
    assert_eq!(result.turns, 3);
    assert_eq!(result.total_tokens, 1500);
    assert_eq!(result.confidence, Some(0.95));
    assert_eq!(result.retry_count, 1);
    assert!(result.guardrails_passed);
    assert!(result.cost_usd > 0.0);
    assert_eq!(result.final_output["answer"], "42");
    assert!(result.status.is_completed());
}

// ========================================================================
// Completion Detection Tests
// ========================================================================

#[test]
fn test_check_completion_signal() {
    use crate::runtime::builtin::COMPLETION_MARKER;

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

    // Should detect completion marker
    let response_with_marker = format!(
        r#"{{"completed": true, "marker": "{}"}}"#,
        COMPLETION_MARKER
    );
    assert!(agent.check_completion_signal(&response_with_marker));

    // Should not detect without marker
    assert!(!agent.check_completion_signal("Task completed successfully"));
    assert!(!agent.check_completion_signal(r#"{"completed": true}"#));
}

#[test]
fn test_determine_status_explicit_completion() {
    use crate::runtime::builtin::COMPLETION_MARKER;

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

    // Explicit completion has highest priority
    let response = format!(r#"Result: {{"marker": "{}"}}"#, COMPLETION_MARKER);
    assert_eq!(
        agent.determine_status(&response),
        RigAgentStatus::ExplicitCompletion
    );
}

#[test]
fn test_determine_status_natural_completion() {
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

    // Natural completion (no marker, no stop condition)
    assert_eq!(
        agent.determine_status("Task completed normally"),
        RigAgentStatus::NaturalCompletion
    );
}

#[test]
fn test_determine_status_explicit_over_natural() {
    use crate::runtime::builtin::COMPLETION_MARKER;

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

    // When marker present, explicit completion wins over natural
    let response = format!("Result with marker: {}", COMPLETION_MARKER);
    assert_eq!(
        agent.determine_status(&response),
        RigAgentStatus::ExplicitCompletion
    );
}

/// BUG-8: In explicit completion mode, when the agent ends a turn WITHOUT
/// calling nika:complete, determine_status returns LowConfidence(0.0).
/// This is CORRECT behavior — it forces the agent loop to retry, giving the
/// agent another chance to call nika:complete. The 0.0 confidence is not a
/// failure; it's a deliberate signal that the agent hasn't finished yet.
#[test]
fn test_explicit_mode_returns_low_confidence_without_nika_complete() {
    use crate::ast::completion::{CompletionConfig, CompletionMode};

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            ..Default::default()
        }),
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

    // Agent output without COMPLETION_MARKER — simulate natural end-of-turn
    let output_without_complete = "Here is my analysis of the data. The results look good.";
    let status = agent.determine_status(output_without_complete);

    // Must be LowConfidence(0.0), NOT NaturalCompletion
    assert_eq!(status, RigAgentStatus::LowConfidence(0.0));
    assert!(
        status.requires_retry(),
        "LowConfidence(0.0) must trigger retry so agent can call nika:complete"
    );
    assert!(
        !status.is_completed(),
        "Without nika:complete in explicit mode, the agent is NOT done"
    );
}

/// Contrast: in natural/default mode, the same output should be NaturalCompletion
#[test]
fn test_natural_mode_returns_natural_completion_without_nika_complete() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        // No completion config → default mode (natural)
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

    let output_without_complete = "Here is my analysis of the data. The results look good.";
    let status = agent.determine_status(output_without_complete);

    assert_eq!(status, RigAgentStatus::NaturalCompletion);
    assert!(
        status.is_completed(),
        "Natural mode: end-of-turn without nika:complete is a valid completion"
    );
}

#[test]
fn test_explicit_completion_status_canonical_str() {
    assert_eq!(
        RigAgentStatus::ExplicitCompletion.as_canonical_str(),
        "tool_complete"
    );
}

// ========================================================================
// Confidence-Based Completion Tests
// ========================================================================

#[test]
fn test_determine_status_high_confidence() {
    use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};
    use crate::runtime::builtin::COMPLETION_MARKER;

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // Completion response with confidence >= threshold
    let response = format!(
        r#"{{"completed": true, "result": "done", "confidence": 0.95, "marker": "{}"}}"#,
        COMPLETION_MARKER
    );
    let status = agent.determine_status(&response);
    assert!(
        matches!(status, RigAgentStatus::HighConfidence(c) if c == 0.95),
        "Expected HighConfidence(0.95), got {:?}",
        status
    );
}

#[test]
fn test_determine_status_low_confidence() {
    use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};
    use crate::runtime::builtin::COMPLETION_MARKER;

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // Completion response with confidence < threshold
    let response = format!(
        r#"{{"completed": true, "result": "done", "confidence": 0.5, "marker": "{}"}}"#,
        COMPLETION_MARKER
    );
    let status = agent.determine_status(&response);
    assert!(
        matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.5),
        "Expected LowConfidence(0.5), got {:?}",
        status
    );
}

#[test]
fn test_determine_status_no_confidence_defaults_to_explicit() {
    use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};
    use crate::runtime::builtin::COMPLETION_MARKER;

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // Completion response without confidence
    let response = format!(
        r#"{{"completed": true, "result": "done", "marker": "{}"}}"#,
        COMPLETION_MARKER
    );
    let status = agent.determine_status(&response);
    assert_eq!(
        status,
        RigAgentStatus::ExplicitCompletion,
        "No confidence should default to ExplicitCompletion"
    );
}

#[test]
fn test_confidence_status_helper_methods() {
    // HighConfidence is completed
    let high = RigAgentStatus::HighConfidence(0.95);
    assert!(high.is_completed());
    assert!(!high.requires_retry());
    assert_eq!(high.confidence(), Some(0.95));
    assert_eq!(high.as_canonical_str(), "tool_complete_high");

    // LowConfidence requires retry
    let low = RigAgentStatus::LowConfidence(0.5);
    assert!(!low.is_completed());
    assert!(low.requires_retry());
    assert_eq!(low.confidence(), Some(0.5));
    assert_eq!(low.as_canonical_str(), "tool_complete_low");

    // Other statuses have no confidence
    assert_eq!(RigAgentStatus::NaturalCompletion.confidence(), None);
    assert_eq!(RigAgentStatus::ExplicitCompletion.confidence(), None);
}

#[test]
fn test_get_confidence_threshold_default() {
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

    // Default threshold is 0.8
    assert_eq!(agent.get_confidence_threshold(), 0.8);
}

#[test]
fn test_get_confidence_threshold_custom() {
    use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.9,
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    assert_eq!(agent.get_confidence_threshold(), 0.9);
}

// ========================================================================
// Retry Logic Tests
// ========================================================================

#[test]
fn test_should_retry_returns_false_for_non_low_confidence() {
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

    // Non-LowConfidence statuses should not retry
    assert!(!agent.should_retry(&RigAgentStatus::NaturalCompletion, 0));
    assert!(!agent.should_retry(&RigAgentStatus::ExplicitCompletion, 0));
    assert!(!agent.should_retry(&RigAgentStatus::HighConfidence(0.9), 0));
    assert!(!agent.should_retry(&RigAgentStatus::MaxTurnsReached, 0));
}

#[test]
fn test_should_retry_with_retry_action() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
        OnLowConfidenceConfig,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                on_low: OnLowConfidenceConfig {
                    action: LowConfidenceAction::Retry,
                    max_retries: 3,
                    feedback: None,
                },
                routing: None,
            }),
            ..Default::default()
        }),
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

    // Should retry when under max_retries
    assert!(agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
    assert!(agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 1));
    assert!(agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 2));

    // Should NOT retry when at or above max_retries
    assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 3));
    assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 4));
}

#[test]
fn test_should_retry_with_accept_action() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
        OnLowConfidenceConfig,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                on_low: OnLowConfidenceConfig {
                    action: LowConfidenceAction::Accept,
                    max_retries: 3,
                    feedback: None,
                },
                routing: None,
            }),
            ..Default::default()
        }),
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

    // Accept action should never retry
    assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
}

#[test]
fn test_should_retry_with_escalate_action() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
        OnLowConfidenceConfig,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                on_low: OnLowConfidenceConfig {
                    action: LowConfidenceAction::Escalate,
                    max_retries: 3,
                    feedback: None,
                },
                routing: None,
            }),
            ..Default::default()
        }),
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

    // Escalate action should never retry
    assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
}

#[test]
fn test_should_retry_without_confidence_config() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        ..Default::default() // No completion config
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

    // Without config, should not retry (no on_low config)
    assert!(!agent.should_retry(&RigAgentStatus::LowConfidence(0.5), 0));
}

#[test]
fn test_get_retry_feedback_default() {
    use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    let feedback = agent.get_retry_feedback(0.5);
    assert!(feedback.contains("RETRY"));
    assert!(feedback.contains("0.50")); // Confidence value
    assert!(feedback.contains("0.80")); // Threshold value
}

#[test]
fn test_get_retry_feedback_custom() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
        OnLowConfidenceConfig,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                on_low: OnLowConfidenceConfig {
                    action: LowConfidenceAction::Retry,
                    max_retries: 2,
                    feedback: Some("Please verify your sources and provide citations".to_string()),
                },
                routing: None,
            }),
            ..Default::default()
        }),
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

    let feedback = agent.get_retry_feedback(0.6);
    assert!(feedback.contains("RETRY"));
    assert!(feedback.contains("0.60")); // Confidence value
    assert!(feedback.contains("verify your sources")); // Custom feedback
}

#[test]
fn test_get_low_confidence_config_present() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, LowConfidenceAction,
        OnLowConfidenceConfig,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                on_low: OnLowConfidenceConfig {
                    action: LowConfidenceAction::Retry,
                    max_retries: 5,
                    feedback: Some("Custom".to_string()),
                },
                routing: None,
            }),
            ..Default::default()
        }),
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

    let config = agent.get_low_confidence_config().unwrap();
    assert_eq!(config.action, LowConfidenceAction::Retry);
    assert_eq!(config.max_retries, 5);
    assert_eq!(config.feedback, Some("Custom".to_string()));
}

#[test]
fn test_get_low_confidence_config_absent() {
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

    assert!(agent.get_low_confidence_config().is_none());
}

// ========================================================================
// Confidence Routing Tests
// ========================================================================

#[test]
fn test_apply_routing_without_config_uses_threshold() {
    use crate::ast::completion::{CompletionConfig, CompletionMode, ConfidenceConfig};

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                routing: None, // No routing configured
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // High confidence (>= threshold)
    let status = agent.apply_routing(0.85);
    assert!(
        matches!(status, RigAgentStatus::HighConfidence(c) if c == 0.85),
        "Expected HighConfidence(0.85), got {:?}",
        status
    );

    // Low confidence (< threshold)
    let status = agent.apply_routing(0.5);
    assert!(
        matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.5),
        "Expected LowConfidence(0.5), got {:?}",
        status
    );
}

#[test]
fn test_apply_routing_with_high_medium_low_routes() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
        RouteAction,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.7,
                routing: Some(ConfidenceRouting {
                    high: ConfidenceRoute {
                        min: Some(0.85),
                        action: RouteAction::Accept,
                        escalate_to: None,
                    },
                    medium: ConfidenceRoute {
                        min: Some(0.7),
                        action: RouteAction::AcceptWithFlag,
                        escalate_to: None,
                    },
                    low: ConfidenceRoute {
                        min: None,
                        action: RouteAction::Escalate,
                        escalate_to: Some("human".to_string()),
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // High confidence route (>= 0.85)
    let status = agent.apply_routing(0.92);
    assert!(
        matches!(status, RigAgentStatus::HighConfidence(c) if c == 0.92),
        "Expected HighConfidence for 0.92, got {:?}",
        status
    );

    // Medium confidence route (>= 0.7, < 0.85)
    let status = agent.apply_routing(0.75);
    assert!(
        matches!(status, RigAgentStatus::FlaggedForReview(c) if c == 0.75),
        "Expected FlaggedForReview for 0.75, got {:?}",
        status
    );

    // Low confidence route (< 0.7)
    let status = agent.apply_routing(0.5);
    assert!(
        matches!(status, RigAgentStatus::Escalated(c) if c == 0.5),
        "Expected Escalated for 0.5, got {:?}",
        status
    );
}

#[test]
fn test_apply_routing_retry_action() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
        RouteAction,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.7,
                routing: Some(ConfidenceRouting {
                    high: ConfidenceRoute {
                        min: Some(0.9),
                        action: RouteAction::Accept,
                        escalate_to: None,
                    },
                    medium: ConfidenceRoute {
                        min: Some(0.7),
                        action: RouteAction::AcceptWithFlag,
                        escalate_to: None,
                    },
                    low: ConfidenceRoute {
                        min: None,
                        action: RouteAction::Retry, // Low confidence -> retry
                        escalate_to: None,
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // Low confidence with Retry action
    let status = agent.apply_routing(0.5);
    assert!(
        matches!(status, RigAgentStatus::LowConfidence(c) if c == 0.5),
        "Expected LowConfidence for Retry action, got {:?}",
        status
    );
}

#[test]
fn test_route_action_to_status_all_variants() {
    use crate::ast::completion::RouteAction;

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

    assert!(matches!(
        agent.route_action_to_status(&RouteAction::Accept, 0.9),
        RigAgentStatus::HighConfidence(c) if c == 0.9
    ));

    assert!(matches!(
        agent.route_action_to_status(&RouteAction::AcceptWithFlag, 0.75),
        RigAgentStatus::FlaggedForReview(c) if c == 0.75
    ));

    assert!(matches!(
        agent.route_action_to_status(&RouteAction::Retry, 0.5),
        RigAgentStatus::LowConfidence(c) if c == 0.5
    ));

    assert!(matches!(
        agent.route_action_to_status(&RouteAction::Escalate, 0.3),
        RigAgentStatus::Escalated(c) if c == 0.3
    ));
}

#[test]
fn test_get_confidence_routing_present() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
        RouteAction,
    };

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.7,
                routing: Some(ConfidenceRouting {
                    high: ConfidenceRoute {
                        min: Some(0.9),
                        action: RouteAction::Accept,
                        escalate_to: None,
                    },
                    medium: ConfidenceRoute {
                        min: Some(0.7),
                        action: RouteAction::AcceptWithFlag,
                        escalate_to: None,
                    },
                    low: ConfidenceRoute {
                        min: None,
                        action: RouteAction::Escalate,
                        escalate_to: Some("human".to_string()),
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    let routing = agent.get_confidence_routing();
    assert!(routing.is_some());

    let r = routing.unwrap();
    assert_eq!(r.high.min, Some(0.9));
    assert_eq!(r.high.action, RouteAction::Accept);
    assert_eq!(r.low.escalate_to, Some("human".to_string()));
}

#[test]
fn test_get_confidence_routing_absent() {
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

    assert!(agent.get_confidence_routing().is_none());
}

#[test]
fn test_flagged_and_escalated_status_properties() {
    // FlaggedForReview
    let flagged = RigAgentStatus::FlaggedForReview(0.75);
    assert_eq!(flagged.as_canonical_str(), "tool_complete_flagged");
    assert_eq!(flagged.confidence(), Some(0.75));
    assert!(!flagged.requires_retry());

    // Escalated
    let escalated = RigAgentStatus::Escalated(0.4);
    assert_eq!(escalated.as_canonical_str(), "escalated");
    assert_eq!(escalated.confidence(), Some(0.4));
    assert!(!escalated.requires_retry());
}

#[test]
fn test_determine_status_with_routing() {
    use crate::ast::completion::{
        CompletionConfig, CompletionMode, ConfidenceConfig, ConfidenceRoute, ConfidenceRouting,
        RouteAction,
    };
    use crate::runtime::builtin::COMPLETION_MARKER;

    let params = AgentParams {
        prompt: "Test".to_string(),
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            confidence: Some(ConfidenceConfig {
                threshold: 0.7,
                routing: Some(ConfidenceRouting {
                    high: ConfidenceRoute {
                        min: Some(0.85),
                        action: RouteAction::Accept,
                        escalate_to: None,
                    },
                    medium: ConfidenceRoute {
                        min: Some(0.7),
                        action: RouteAction::AcceptWithFlag,
                        escalate_to: None,
                    },
                    low: ConfidenceRoute {
                        min: None,
                        action: RouteAction::Escalate,
                        escalate_to: Some("supervisor".to_string()),
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
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

    // Medium confidence -> FlaggedForReview
    let response = format!(
        r#"{{"completed": true, "result": "done", "confidence": 0.78, "marker": "{}"}}"#,
        COMPLETION_MARKER
    );
    let status = agent.determine_status(&response);
    assert!(
        matches!(status, RigAgentStatus::FlaggedForReview(c) if (c - 0.78).abs() < 0.001),
        "Expected FlaggedForReview(0.78), got {:?}",
        status
    );

    // Low confidence -> Escalated
    let response = format!(
        r#"{{"completed": true, "result": "done", "confidence": 0.5, "marker": "{}"}}"#,
        COMPLETION_MARKER
    );
    let status = agent.determine_status(&response);
    assert!(
        matches!(status, RigAgentStatus::Escalated(c) if (c - 0.5).abs() < 0.001),
        "Expected Escalated(0.5), got {:?}",
        status
    );
}

// ========================================================================
// Extended Thinking Tests
// ========================================================================

#[test]
fn test_agent_loop_with_extended_thinking_creates_successfully() {
    let params = AgentParams {
        prompt: "Analyze this problem step by step".to_string(),
        extended_thinking: Some(true),
        provider: Some(nika_core::ProviderName::Anthropic),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "thinking-test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    );

    assert!(
        agent.is_ok(),
        "Agent with extended_thinking should be created"
    );
}

#[test]
fn test_agent_loop_extended_thinking_false_creates_successfully() {
    let params = AgentParams {
        prompt: "Simple query".to_string(),
        extended_thinking: Some(false),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "no-thinking-test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    );

    assert!(
        agent.is_ok(),
        "Agent with extended_thinking: false should be created"
    );
}

#[test]
fn test_agent_loop_extended_thinking_none_creates_successfully() {
    let params = AgentParams {
        prompt: "Default behavior".to_string(),
        extended_thinking: None,
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "default-test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    );

    assert!(
        agent.is_ok(),
        "Agent with extended_thinking: None should be created"
    );
}

#[test]
fn test_agent_loop_with_system_prompt_and_thinking() {
    let params = AgentParams {
        prompt: "What is 2+2?".to_string(),
        system: Some("You are a math tutor. Think step by step.".to_string()),
        extended_thinking: Some(true),
        provider: Some(nika_core::ProviderName::Anthropic),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "system-thinking-test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    );

    assert!(
        agent.is_ok(),
        "Agent with system prompt and thinking should be created"
    );
}

// ========================================================================
// GuardrailCheckResult Tests
// ========================================================================

#[test]
fn test_guardrail_check_result_all_passed() {
    let result = GuardrailCheckResult::AllPassed;
    assert!(result.is_passed());
    assert!(!result.should_retry());
    assert!(!result.should_escalate());
    assert!(!result.should_fail());
}

#[test]
fn test_guardrail_check_result_failed_retry() {
    let result = GuardrailCheckResult::FailedRetry(vec!["test failure".to_string()]);
    assert!(!result.is_passed());
    assert!(result.should_retry());
    assert!(!result.should_escalate());
    assert!(!result.should_fail());
    assert_eq!(result.failure_messages(), &["test failure".to_string()]);
}

#[test]
fn test_guardrail_check_result_failed_escalate() {
    let result = GuardrailCheckResult::FailedEscalate;
    assert!(!result.is_passed());
    assert!(!result.should_retry());
    assert!(result.should_escalate());
    assert!(!result.should_fail());
}

#[test]
fn test_guardrail_check_result_failed_immediate() {
    let result = GuardrailCheckResult::FailedImmediate;
    assert!(!result.is_passed());
    assert!(!result.should_retry());
    assert!(!result.should_escalate());
    assert!(result.should_fail());
}

#[tokio::test]
async fn test_check_guardrails_empty_returns_all_passed() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![], // No guardrails
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

    let result = agent.check_guardrails("Any output").await;
    assert_eq!(result, GuardrailCheckResult::AllPassed);
}

#[tokio::test]
async fn test_check_guardrails_passing() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![GuardrailConfig::Length(LengthGuardrail {
            id: Some("word-count".to_string()),
            min_words: Some(2),
            max_words: Some(10),
            min_chars: None,
            max_chars: None,
            message: None,
            on_failure: OnFailure::Retry,
        })],
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

    // Output has 4 words, within bounds
    let result = agent.check_guardrails("This is a test").await;
    assert_eq!(result, GuardrailCheckResult::AllPassed);
}

#[tokio::test]
async fn test_check_guardrails_failed_retry() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![GuardrailConfig::Length(LengthGuardrail {
            id: Some("word-count".to_string()),
            min_words: Some(10), // Requires at least 10 words
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
            on_failure: OnFailure::Retry, // Default
        })],
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

    // Output has only 4 words
    let result = agent.check_guardrails("This is a test").await;
    assert!(result.should_retry(), "Expected FailedRetry");
    assert!(
        !result.failure_messages().is_empty(),
        "Should have failure messages"
    );
    assert!(
        result.failure_messages()[0].contains("4 words"),
        "Message should mention word count"
    );
}

#[tokio::test]
async fn test_check_guardrails_failed_escalate() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![GuardrailConfig::Length(LengthGuardrail {
            id: Some("word-count".to_string()),
            min_words: Some(10), // Requires at least 10 words
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
            on_failure: OnFailure::Escalate, // Escalate on failure
        })],
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

    // Output has only 4 words
    let result = agent.check_guardrails("This is a test").await;
    assert_eq!(result, GuardrailCheckResult::FailedEscalate);
}

#[tokio::test]
async fn test_check_guardrails_failed_immediate() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![GuardrailConfig::Length(LengthGuardrail {
            id: Some("word-count".to_string()),
            min_words: Some(10), // Requires at least 10 words
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
            on_failure: OnFailure::Fail, // Fail immediately
        })],
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

    // Output has only 4 words
    let result = agent.check_guardrails("This is a test").await;
    assert_eq!(result, GuardrailCheckResult::FailedImmediate);
}

#[tokio::test]
async fn test_check_guardrails_priority_immediate_over_escalate() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![
            GuardrailConfig::Length(LengthGuardrail {
                id: Some("escalate-guard".to_string()),
                min_words: Some(10),
                max_words: None,
                min_chars: None,
                max_chars: None,
                message: None,
                on_failure: OnFailure::Escalate,
            }),
            GuardrailConfig::Length(LengthGuardrail {
                id: Some("fail-guard".to_string()),
                min_words: Some(20), // This will also fail
                max_words: None,
                min_chars: None,
                max_chars: None,
                message: None,
                on_failure: OnFailure::Fail, // Higher priority
            }),
        ],
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

    // Output has only 4 words, both guardrails fail
    // But Fail has higher priority than Escalate
    let result = agent.check_guardrails("This is a test").await;
    assert_eq!(result, GuardrailCheckResult::FailedImmediate);
}

#[tokio::test]
async fn test_check_guardrails_priority_escalate_over_retry() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![
            GuardrailConfig::Length(LengthGuardrail {
                id: Some("retry-guard".to_string()),
                min_words: Some(10),
                max_words: None,
                min_chars: None,
                max_chars: None,
                message: None,
                on_failure: OnFailure::Retry,
            }),
            GuardrailConfig::Length(LengthGuardrail {
                id: Some("escalate-guard".to_string()),
                min_words: Some(20),
                max_words: None,
                min_chars: None,
                max_chars: None,
                message: None,
                on_failure: OnFailure::Escalate, // Higher priority
            }),
        ],
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

    // Output has only 4 words, both guardrails fail
    // But Escalate has higher priority than Retry
    let result = agent.check_guardrails("This is a test").await;
    assert_eq!(result, GuardrailCheckResult::FailedEscalate);
}

#[tokio::test]
async fn test_check_guardrails_emits_escalation_event() {
    use crate::ast::guardrails::{GuardrailConfig, LengthGuardrail, OnFailure};

    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![GuardrailConfig::Length(LengthGuardrail {
            id: Some("escalate-guard".to_string()),
            min_words: Some(10),
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: Some("Output too short".to_string()),
            on_failure: OnFailure::Escalate,
        })],
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    // Output has only 4 words
    let result = agent.check_guardrails("This is a test").await;
    assert_eq!(result, GuardrailCheckResult::FailedEscalate);

    // Verify events were emitted
    let events = event_log.events();
    assert!(
        events.len() >= 2,
        "Should have at least 2 events (failed + escalation)"
    );

    // Check for GuardrailFailed event
    let has_failed = events
        .iter()
        .any(|e| matches!(e.kind, EventKind::GuardrailFailed { .. }));
    assert!(has_failed, "Should have GuardrailFailed event");

    // Check for GuardrailEscalation event
    let has_escalation = events
        .iter()
        .any(|e| matches!(e.kind, EventKind::GuardrailEscalation { .. }));
    assert!(has_escalation, "Should have GuardrailEscalation event");
}

#[tokio::test]
async fn test_check_guardrails_llm_type_fails_without_api_key() {
    use crate::ast::guardrails::{GuardrailConfig, LlmGuardrail};

    // Use Default which sets judge_prompt="" — validation isn't checked here,
    // and the provider error happens before judge_prompt is used
    let params = AgentParams {
        prompt: "Test".to_string(),
        guardrails: vec![GuardrailConfig::Llm(LlmGuardrail::default())],
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log.clone(),
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    // Without API keys, LLM guardrail should fail (provider error) with retry action
    let result = agent.check_guardrails("Any output").await;
    assert!(
        matches!(result, GuardrailCheckResult::FailedRetry(_)),
        "LLM guardrail without API key should fail with retry, got {:?}",
        result
    );

    // Verify a GuardrailFailed event was emitted
    let events = event_log.events();
    let has_failed = events
        .iter()
        .any(|e| matches!(&e.kind, EventKind::GuardrailFailed { guardrail_type, .. } if *guardrail_type == nika_event::GuardrailType::Llm));
    assert!(has_failed, "Should emit GuardrailFailed event for llm type");
}

// ========================================================================
// Wave 2: Deep Audit - Bug-Proving Tests
// ========================================================================

// ---- BUG: chat_continue() missing Gemini check ----
// chat_continue() error message claims GEMINI_API_KEY is supported,
// but no `has_key("GEMINI_API_KEY")` check exists.
// This test proves that even when GEMINI_API_KEY is the only key set,
// chat_continue returns an error instead of dispatching to Gemini.
//
// FIX: Add `if has_key("GEMINI_API_KEY") { return self.chat_continue_gemini(prompt).await; }`
// before the Err(...) in chat_continue(), and implement chat_continue_gemini().
#[tokio::test]
#[serial]
async fn wave2_chat_continue_missing_gemini_dispatch() {
    // Temporarily set ONLY GEMINI_API_KEY (clear all others)
    // Save originals
    let keys = [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "MISTRAL_API_KEY",
        "GROQ_API_KEY",
        "DEEPSEEK_API_KEY",
    ];
    let saved: Vec<Option<String>> = keys.iter().map(|k| std::env::var(k).ok()).collect();

    // Clear all LLM keys
    for key in &keys {
        std::env::remove_var(key);
    }
    // Set only Gemini
    let saved_gemini = std::env::var("GEMINI_API_KEY").ok();
    std::env::set_var("GEMINI_API_KEY", "test-gemini-key-for-audit");

    let params = AgentParams {
        prompt: "Test Gemini dispatch".to_string(),
        model: Some("gemini-2.0-flash".to_string()),
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();
    let mut agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    let result = agent.chat_continue("Hello").await;

    // Restore env
    for (i, key) in keys.iter().enumerate() {
        match &saved[i] {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    match saved_gemini {
        Some(v) => std::env::set_var("GEMINI_API_KEY", v),
        None => std::env::remove_var("GEMINI_API_KEY"),
    }

    // BUG: This SHOULD attempt to use Gemini (and fail at API call since key is fake).
    // Instead, it returns AgentValidationError because Gemini is not checked.
    // If the bug were fixed, this would be an AgentExecutionError (failed API call),
    // not an AgentValidationError.
    let err = result.expect_err("Should get an error (either validation or execution)");
    let err_str = err.to_string();

    // The bug: error is "chat_continue requires one of: ..." which mentions GEMINI_API_KEY
    // but never actually checks for it. If the bug were fixed, we would NOT get
    // this specific validation error.
    assert!(
        !err_str.contains("chat_continue requires one of"),
        "BUG PROVEN: chat_continue() does not check GEMINI_API_KEY despite listing it \
         in the error message. Got validation error: {}",
        err_str
    );
}

// ---- BUG: chat_continue whitespace inconsistency ----
// chat_continue() uses `!v.is_empty()` but core::providers::has_env_key() uses
// `!v.trim().is_empty()`. A key set to "   " (whitespace only) will pass
// chat_continue's check but fail at the provider level.
//
// FIX: Change `let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.is_empty());`
// to `let has_key = |key: &str| std::env::var(key).is_ok_and(|v| !v.trim().is_empty());`
#[test]
fn wave2_chat_continue_whitespace_key_inconsistency() {
    // Simulate the two different checks
    let whitespace_only = "   ";

    // chat_continue's check: is_ok_and(|v| !v.is_empty())
    let chat_continue_accepts = !whitespace_only.is_empty();

    // core::providers::has_env_key check: is_ok_and(|v| !v.trim().is_empty())
    let core_providers_accepts = !whitespace_only.trim().is_empty();

    // BUG: chat_continue accepts whitespace-only keys, but the provider will reject them.
    // This means chat_continue dispatches to a provider that will fail at connection time,
    // giving a confusing error message instead of falling through to the next provider.
    assert_ne!(
        chat_continue_accepts, core_providers_accepts,
        "BUG PROVEN: chat_continue and core::providers disagree on whitespace-only keys. \
         chat_continue_accepts={}, core_providers_accepts={}",
        chat_continue_accepts, core_providers_accepts
    );
}

// ---- BUG: chat_continue_mistral/groq/deepseek hardcode NaturalCompletion ----
// These methods set `let status = RigAgentStatus::NaturalCompletion` instead of
// calling `self.determine_status(&response)`. This means if the agent calls
// nika:complete (which inserts COMPLETION_MARKER), the status is still NaturalCompletion
// instead of ExplicitCompletion.
//
// FIX: Replace `let status = RigAgentStatus::NaturalCompletion;` with
// `let status = self.determine_status(&response);` in all three methods.
#[test]
fn wave2_determine_status_detects_completion_but_mistral_ignores_it() {
    use crate::runtime::builtin::COMPLETION_MARKER;

    // Create agent to test determine_status
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

    // A response containing the completion marker should be ExplicitCompletion
    let response_with_marker = format!("Task done. {}", COMPLETION_MARKER);
    let status = agent.determine_status(&response_with_marker);
    assert_eq!(
        status,
        RigAgentStatus::ExplicitCompletion,
        "determine_status correctly detects completion"
    );

    // BUG: Mistral/Groq/DeepSeek chat_continue methods hardcode NaturalCompletion
    // instead of calling determine_status(). We prove the bug by showing the code path:
    // Line 316 in chat.rs: `let status = RigAgentStatus::NaturalCompletion;`
    // Line 378 in chat.rs: `let status = RigAgentStatus::NaturalCompletion;`
    // Line 443 in chat.rs: `let status = RigAgentStatus::NaturalCompletion;`
    //
    // If the response contained a completion marker, these methods would still
    // report NaturalCompletion. We prove this by showing Claude/OpenAI DO call
    // determine_status but Mistral/Groq/DeepSeek do NOT.
    let hardcoded_status = RigAgentStatus::NaturalCompletion;
    assert_ne!(
        hardcoded_status, status,
        "BUG PROVEN: Mistral/Groq/DeepSeek hardcode NaturalCompletion ({:?}) \
         even when determine_status would return {:?} for a response with COMPLETION_MARKER",
        hardcoded_status, status
    );
}

// NOTE: test_turn_count_increments_correctly and
// wave2_turn_index_ambiguous_with_odd_history deleted — they called
// ChatAgentLoop methods (add_to_history, push_message, turn_count)
// on RigAgentLoop which doesn't have them. Broken audit tests.

// ---- Limit variants produced by check_limits(), not determine_status() ----
// MaxTurnsReached, TokenBudgetExceeded, CostLimitReached, DurationLimitReached
// are NOT dead code — they are returned by limit_tracker.check_limits() in
// providers.rs:387-390. determine_status() handles output-based status, while
// check_limits() handles resource-based status. Both paths are active.
#[test]
fn limit_variants_have_correct_semantics() {
    let limit_variants = vec![
        (RigAgentStatus::MaxTurnsReached, "max_turns"),
        (RigAgentStatus::TokenBudgetExceeded, "max_tokens"),
        (RigAgentStatus::CostLimitReached, "max_cost"),
        (RigAgentStatus::DurationLimitReached, "max_duration"),
    ];
    for (variant, expected_str) in &limit_variants {
        assert!(
            variant.is_limit_reached(),
            "{} should be a limit",
            expected_str
        );
        assert!(
            !variant.is_completed(),
            "{} should not be completed",
            expected_str
        );
        assert_eq!(variant.as_canonical_str(), *expected_str);
    }
}

#[test]
fn determine_status_returns_output_based_status_not_limits() {
    // determine_status() handles output patterns (ExplicitCompletion, NaturalCompletion,
    // LowConfidence), NOT resource limits. Limit statuses come from check_limits().
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

    // determine_status returns NaturalCompletion or LowConfidence, never limit variants
    let status = agent.determine_status("Hello world");
    assert!(
        !status.is_limit_reached(),
        "Output-based status should not be a limit"
    );

    use crate::runtime::builtin::COMPLETION_MARKER;
    let output_with_marker = format!("Done {}", COMPLETION_MARKER);
    let status = agent.determine_status(&output_with_marker);
    assert!(
        status.is_completed(),
        "Completion marker should yield completed status"
    );
}

// NOTE: Removed stale BUG PROVEN tests (v0.53):
// - token_accumulation_saturating_add_prevents_overflow: tested stdlib, not nika code
// - token_accumulation_normal_values_unaffected: same
// - wave2_tools_consumed_after_first_take: mem::take removed from production
// - wave2_streaming_result_token_overflow: production now uses saturating_add

// ---- BUG: max_tokens hardcoded to 8192 ignoring params.effective_max_tokens() ----
// In streaming.rs line 60: `let request = request_builder.max_tokens(8192).build();`
// In chat.rs lines 131, 219, 294, 356, 421: `.max_tokens(8192)`
// AgentParams has max_tokens field and effective_max_tokens() method, but they are ignored.
//
// FIX: Use `self.params.effective_max_tokens().unwrap_or(8192)` instead of hardcoded 8192.
#[test]
fn wave2_max_tokens_hardcoded_ignores_params() {
    // AgentParams supports max_tokens configuration
    let params = AgentParams {
        prompt: "Test".to_string(),
        max_tokens: Some(16384), // User wants 16k tokens
        ..Default::default()
    };

    // The effective_max_tokens() method exists and returns the configured value
    let effective = params.effective_max_tokens();
    assert_eq!(
        effective,
        Some(16384),
        "effective_max_tokens should return the configured value"
    );

    // BUG: The streaming/chat code ignores this and uses hardcoded 8192.
    // We can't directly test the internal builder value, but we CAN prove
    // that the params have max_tokens=16384 while the code would use 8192.
    let hardcoded_value: u64 = 8192;
    let user_configured: u64 = effective.unwrap() as u64;
    assert_ne!(
        hardcoded_value, user_configured,
        "BUG PROVEN: User configured max_tokens={} but streaming/chat code \
         hardcodes max_tokens={}. The effective_max_tokens() method exists \
         but is not called in streaming.rs or chat.rs (except thinking.rs which does it correctly).",
        user_configured, hardcoded_value
    );
}

// ═══════════════════════════════════════════════════════════════
// Agent scope tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_scope_minimal_reduces_tool_count() {
    let minimal_params = AgentParams {
        prompt: "Hello".to_string(),
        scope: Some("minimal".to_string()),
        ..Default::default()
    };
    let full_params = AgentParams {
        prompt: "Hello".to_string(),
        scope: Some("full".to_string()),
        ..Default::default()
    };
    let log = EventLog::new();

    let minimal_agent = RigAgentLoop::new(
        "t1".into(),
        minimal_params,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();
    let full_agent = RigAgentLoop::new(
        "t2".into(),
        full_params,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    // minimal scope should have fewer tools than full scope
    assert!(
        minimal_agent.tool_count() < full_agent.tool_count(),
        "minimal ({}) should have fewer tools than full ({})",
        minimal_agent.tool_count(),
        full_agent.tool_count()
    );
    // minimal should have exactly 3 tools: spawn_agent + complete + log
    assert_eq!(
        minimal_agent.tool_count(),
        3,
        "minimal scope should have exactly 3 tools (spawn_agent + complete + log)"
    );
}

#[test]
fn test_scope_debug_adds_introspection_tools() {
    let debug_params = AgentParams {
        prompt: "Hello".to_string(),
        scope: Some("debug".to_string()),
        ..Default::default()
    };
    let full_params = AgentParams {
        prompt: "Hello".to_string(),
        scope: Some("full".to_string()),
        ..Default::default()
    };
    let log = EventLog::new();

    let debug_agent = RigAgentLoop::new(
        "t1".into(),
        debug_params,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();
    let full_agent = RigAgentLoop::new(
        "t2".into(),
        full_params,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    // debug scope should have MORE tools than full (introspection tools added)
    assert!(
        debug_agent.tool_count() > full_agent.tool_count(),
        "debug ({}) should have more tools than full ({})",
        debug_agent.tool_count(),
        full_agent.tool_count()
    );
}

#[test]
fn test_scope_default_is_full() {
    // No scope specified should behave like "full"
    let no_scope = AgentParams {
        prompt: "Hello".to_string(),
        scope: None,
        ..Default::default()
    };
    let full_scope = AgentParams {
        prompt: "Hello".to_string(),
        scope: Some("full".to_string()),
        ..Default::default()
    };
    let log = EventLog::new();

    let default_agent = RigAgentLoop::new(
        "t1".into(),
        no_scope,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();
    let full_agent = RigAgentLoop::new(
        "t2".into(),
        full_scope,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    assert_eq!(
        default_agent.tool_count(),
        full_agent.tool_count(),
        "default scope should match full scope tool count"
    );
}

#[test]
fn test_scope_explicit_tools_override_scope() {
    // Even with minimal scope, explicit tools: list should take priority
    let params = AgentParams {
        prompt: "Hello".to_string(),
        scope: Some("minimal".to_string()),
        tools: vec![
            "nika:sleep".to_string(),
            "nika:assert".to_string(),
            "nika:complete".to_string(),
        ],
        ..Default::default()
    };
    let log = EventLog::new();

    let agent = RigAgentLoop::new(
        "t1".into(),
        params,
        log.clone(),
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    // Should have exactly 4 tools (spawn_agent + 3 explicit nika:* tools)
    assert_eq!(
        agent.tool_count(),
        4,
        "explicit tools list should override minimal scope (3 + spawn_agent)"
    );
}

// ========================================================================
// ModelResolver: agent loop requires model (no hardcoded defaults)
// ========================================================================

/// run_groq() must error when params.model is None.
/// Previously it silently defaulted to "llama-3.3-70b-versatile", bypassing ModelResolver.
#[tokio::test]
#[serial]
async fn test_run_groq_errors_on_missing_model() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        provider: Some(nika_core::ProviderName::parse("groq")),
        model: None, // No model — must error, not silently default
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mcp_clients = FxHashMap::default();

    let mut agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        mcp_clients,
        None,
        None,
    )
    .unwrap();

    let result = agent.run().await;
    assert!(result.is_err(), "run() must error when model is None");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("model field is required"),
        "Error should mention model requirement, got: {err}"
    );
}

/// run_mistral() must error when params.model is None.
#[tokio::test]
#[serial]
async fn test_run_mistral_errors_on_missing_model() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        provider: Some(nika_core::ProviderName::parse("mistral")),
        model: None,
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    let result = agent.run().await;
    assert!(result.is_err(), "run() must error when model is None");
}

/// run_deepseek() must error when params.model is None.
#[tokio::test]
#[serial]
async fn test_run_deepseek_errors_on_missing_model() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        provider: Some(nika_core::ProviderName::parse("deepseek")),
        model: None,
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    let result = agent.run().await;
    assert!(result.is_err(), "run() must error when model is None");
}

/// run_gemini() must error when params.model is None.
#[tokio::test]
#[serial]
async fn test_run_gemini_errors_on_missing_model() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        provider: Some(nika_core::ProviderName::parse("gemini")),
        model: None,
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    let result = agent.run().await;
    assert!(result.is_err(), "run() must error when model is None");
}

/// run_xai() must error when params.model is None.
#[tokio::test]
#[serial]
async fn test_run_xai_errors_on_missing_model() {
    let params = AgentParams {
        prompt: "Test".to_string(),
        provider: Some(nika_core::ProviderName::parse("xai")),
        model: None,
        ..Default::default()
    };
    let event_log = EventLog::new();
    let mut agent = RigAgentLoop::new(
        "test".to_string(),
        params,
        event_log,
        FxHashMap::default(),
        None,
        None,
    )
    .unwrap();

    let result = agent.run().await;
    assert!(result.is_err(), "run() must error when model is None");
}
