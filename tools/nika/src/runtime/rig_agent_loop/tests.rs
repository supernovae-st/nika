#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use crate::ast::AgentParams;
    use crate::event::{EventKind, EventLog};

    use crate::runtime::rig_agent_loop::types::{
        GuardrailCheckResult, RigAgentLoopResult, RigAgentStatus,
    };
    use crate::runtime::rig_agent_loop::RigAgentLoop;

    #[test]
    fn test_rig_agent_status_variants() {
        let status = RigAgentStatus::NaturalCompletion;
        assert_eq!(status, RigAgentStatus::NaturalCompletion);

        let status = RigAgentStatus::MaxTurnsReached;
        assert_eq!(status, RigAgentStatus::MaxTurnsReached);
    }

    #[test]
    fn test_rig_agent_loop_result_debug() {
        let result = RigAgentLoopResult {
            status: RigAgentStatus::NaturalCompletion,
            turns: 1,
            final_output: serde_json::json!({}),
            total_tokens: 50,
            confidence: None,
            retry_count: 0,
            guardrails_passed: true,
            cost_usd: 0.0,
            partial_result: None,
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("NaturalCompletion"));
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // When marker present, explicit completion wins over natural
        let response = format!("Result with marker: {}", COMPLETION_MARKER);
        assert_eq!(
            agent.determine_status(&response),
            RigAgentStatus::ExplicitCompletion
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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
                        feedback: Some(
                            "Please verify your sources and provide citations".to_string(),
                        ),
                    },
                    routing: None,
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

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
            provider: Some("claude".to_string()),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("thinking-test".to_string(), params, event_log, mcp_clients);

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

        let agent = RigAgentLoop::new("default-test".to_string(), params, event_log, mcp_clients);

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
            provider: Some("claude".to_string()),
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new(
            "system-thinking-test".to_string(),
            params,
            event_log,
            mcp_clients,
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
        let result = GuardrailCheckResult::FailedRetry;
        assert!(!result.is_passed());
        assert!(result.should_retry());
        assert!(!result.should_escalate());
        assert!(!result.should_fail());
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

    #[test]
    fn test_check_guardrails_empty_returns_all_passed() {
        let params = AgentParams {
            prompt: "Test".to_string(),
            guardrails: vec![], // No guardrails
            ..Default::default()
        };
        let event_log = EventLog::new();
        let mcp_clients = FxHashMap::default();

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        let result = agent.check_guardrails("Any output");
        assert_eq!(result, GuardrailCheckResult::AllPassed);
    }

    #[test]
    fn test_check_guardrails_passing() {
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Output has 4 words, within bounds
        let result = agent.check_guardrails("This is a test");
        assert_eq!(result, GuardrailCheckResult::AllPassed);
    }

    #[test]
    fn test_check_guardrails_failed_retry() {
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Output has only 4 words
        let result = agent.check_guardrails("This is a test");
        assert_eq!(result, GuardrailCheckResult::FailedRetry);
    }

    #[test]
    fn test_check_guardrails_failed_escalate() {
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Output has only 4 words
        let result = agent.check_guardrails("This is a test");
        assert_eq!(result, GuardrailCheckResult::FailedEscalate);
    }

    #[test]
    fn test_check_guardrails_failed_immediate() {
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Output has only 4 words
        let result = agent.check_guardrails("This is a test");
        assert_eq!(result, GuardrailCheckResult::FailedImmediate);
    }

    #[test]
    fn test_check_guardrails_priority_immediate_over_escalate() {
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Output has only 4 words, both guardrails fail
        // But Fail has higher priority than Escalate
        let result = agent.check_guardrails("This is a test");
        assert_eq!(result, GuardrailCheckResult::FailedImmediate);
    }

    #[test]
    fn test_check_guardrails_priority_escalate_over_retry() {
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

        let agent = RigAgentLoop::new("test".to_string(), params, event_log, mcp_clients).unwrap();

        // Output has only 4 words, both guardrails fail
        // But Escalate has higher priority than Retry
        let result = agent.check_guardrails("This is a test");
        assert_eq!(result, GuardrailCheckResult::FailedEscalate);
    }

    #[test]
    fn test_check_guardrails_emits_escalation_event() {
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

        let agent =
            RigAgentLoop::new("test".to_string(), params, event_log.clone(), mcp_clients).unwrap();

        // Output has only 4 words
        let result = agent.check_guardrails("This is a test");
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
}
