//! Agent Loop Edge Cases Tests
//!
//! Tests for agent loop edge cases:
//! - Token budget exhaustion
//! - System prompts
//! - Retry logic on transient failures
//! - Tool error recovery
//! - Parallel tool execution

use nika::error::NikaError;
use nika::runtime::AgentLoop;

// ============================================================================
// Is Retryable Provider Error Tests
// ============================================================================

#[test]
fn test_is_retryable_rate_limit_429() {
    let error = NikaError::ProviderApiError {
        message: "Rate limit exceeded (429)".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_too_many_requests() {
    let error = NikaError::ProviderApiError {
        message: "Too many requests - please slow down".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_500_internal_server_error() {
    let error = NikaError::ProviderApiError {
        message: "Internal server error (500)".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_502_bad_gateway() {
    let error = NikaError::ProviderApiError {
        message: "Bad gateway (502)".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_503_service_unavailable() {
    let error = NikaError::ProviderApiError {
        message: "Service unavailable (503)".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_504_gateway_timeout() {
    let error = NikaError::ProviderApiError {
        message: "Gateway timeout (504)".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_timeout() {
    let error = NikaError::ProviderApiError {
        message: "Request timed out after 30s".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_connection_reset() {
    let error = NikaError::ProviderApiError {
        message: "Connection reset by peer".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_connection_refused() {
    let error = NikaError::ProviderApiError {
        message: "Connection refused".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_network_error() {
    let error = NikaError::ProviderApiError {
        message: "Network error - DNS resolution failed".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_overloaded() {
    let error = NikaError::ProviderApiError {
        message: "Server overloaded, try again later".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_is_retryable_capacity() {
    let error = NikaError::ProviderApiError {
        message: "API at capacity".to_string(),
    };
    assert!(AgentLoop::is_retryable_provider_error(&error));
}

// ============================================================================
// Non-Retryable Errors
// ============================================================================

#[test]
fn test_not_retryable_invalid_api_key() {
    let error = NikaError::ProviderApiError {
        message: "Invalid API key".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_not_retryable_401_unauthorized() {
    let error = NikaError::ProviderApiError {
        message: "401 Unauthorized".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_not_retryable_403_forbidden() {
    let error = NikaError::ProviderApiError {
        message: "403 Forbidden - access denied".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_not_retryable_400_bad_request() {
    let error = NikaError::ProviderApiError {
        message: "400 Bad Request - invalid parameters".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_not_retryable_invalid_model() {
    let error = NikaError::ProviderApiError {
        message: "Model 'gpt-99' does not exist".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_not_retryable_content_policy() {
    let error = NikaError::ProviderApiError {
        message: "Content policy violation".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

#[test]
fn test_not_retryable_billing_issue() {
    let error = NikaError::ProviderApiError {
        message: "Billing hard limit reached".to_string(),
    };
    assert!(!AgentLoop::is_retryable_provider_error(&error));
}

// ============================================================================
// Token Budget Tests
// ============================================================================

#[test]
fn test_agent_params_default_token_budget() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        ..Default::default()
    };

    // Default should be u32::MAX (unlimited)
    assert_eq!(params.effective_token_budget(), u32::MAX);
}

#[test]
fn test_agent_params_custom_token_budget() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        token_budget: Some(50_000),
        ..Default::default()
    };

    assert_eq!(params.effective_token_budget(), 50_000);
}

// ============================================================================
// System Prompt Tests
// ============================================================================

#[test]
fn test_agent_params_with_system_prompt_validates() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "User message".to_string(),
        system: Some("You are a helpful coding assistant.".to_string()),
        ..Default::default()
    };

    assert!(params.validate().is_ok());
}

#[test]
fn test_agent_params_system_prompt_optional() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "User message without system".to_string(),
        system: None,
        ..Default::default()
    };

    assert!(params.validate().is_ok());
}

// ============================================================================
// Max Turns Tests
// ============================================================================

#[test]
fn test_agent_params_max_turns_default() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        ..Default::default()
    };

    // Default should be 10
    assert_eq!(params.effective_max_turns(), 10);
}

#[test]
fn test_agent_params_max_turns_custom() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(25),
        ..Default::default()
    };

    assert_eq!(params.effective_max_turns(), 25);
}

#[test]
fn test_agent_params_max_turns_zero_invalid() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(0),
        ..Default::default()
    };

    assert!(params.validate().is_err());
}

#[test]
fn test_agent_params_max_turns_over_100_invalid() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(101),
        ..Default::default()
    };

    assert!(params.validate().is_err());
}

// ============================================================================
// Stop Conditions Tests
// ============================================================================

#[test]
fn test_agent_params_stop_condition_match() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        stop_conditions: vec!["DONE".to_string(), "COMPLETE".to_string()],
        ..Default::default()
    };

    assert!(params.should_stop("Task DONE successfully"));
    assert!(params.should_stop("COMPLETE - all finished"));
    assert!(!params.should_stop("Still working on it"));
}

#[test]
fn test_agent_params_stop_condition_empty() {
    use nika::ast::AgentParams;

    let params = AgentParams {
        prompt: "test".to_string(),
        stop_conditions: vec![],
        ..Default::default()
    };

    // No stop conditions means never stop based on content
    assert!(!params.should_stop("Any content here"));
}

// ============================================================================
// AgentStatus Tests
// ============================================================================

#[test]
fn test_agent_status_all_variants() {
    use nika::runtime::AgentStatus;

    let statuses = [
        AgentStatus::NaturalCompletion,
        AgentStatus::StopConditionMet,
        AgentStatus::MaxTurnsReached,
        AgentStatus::TokenBudgetExceeded,
        AgentStatus::Failed,
    ];

    // Verify all are distinct
    for (i, s1) in statuses.iter().enumerate() {
        for (j, s2) in statuses.iter().enumerate() {
            if i == j {
                assert_eq!(s1, s2);
            } else {
                assert_ne!(s1, s2);
            }
        }
    }
}

#[test]
fn test_agent_status_is_copy() {
    use nika::runtime::AgentStatus;

    let status1 = AgentStatus::TokenBudgetExceeded;
    let status2 = status1; // Copy
    assert_eq!(status1, status2);
}

#[test]
fn test_agent_status_debug() {
    use nika::runtime::AgentStatus;

    let debug = format!("{:?}", AgentStatus::TokenBudgetExceeded);
    assert!(debug.contains("TokenBudgetExceeded"));
}
