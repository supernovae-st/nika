//! Agent verb parsing tests
//!
//! Tests YAML deserialization and validation of agent verb parameters.

use nika::ast::AgentParams;

// ===============================================================
// Full Configuration Tests
// ===============================================================

#[test]
fn test_agent_params_full() {
    let yaml = r#"
prompt: |
  Generate native content for the homepage hero block.
  Use @entity:qr-code-generator for the main concept.
provider: claude
model: claude-sonnet-4
mcp:
  - novanet
max_turns: 10
stop_conditions:
  - "GENERATION_COMPLETE"
  - "VALIDATION_PASSED"
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert!(params.prompt.contains("homepage hero"));
    assert_eq!(params.provider, Some("claude".to_string()));
    assert_eq!(params.model, Some("claude-sonnet-4".to_string()));
    assert_eq!(params.mcp, vec!["novanet"]);
    assert_eq!(params.max_turns, Some(10));
    assert_eq!(params.stop_conditions.len(), 2);
    assert!(params
        .stop_conditions
        .contains(&"GENERATION_COMPLETE".to_string()));
    assert!(params
        .stop_conditions
        .contains(&"VALIDATION_PASSED".to_string()));
}

// ===============================================================
// Minimal Configuration Tests
// ===============================================================

#[test]
fn test_agent_params_minimal() {
    let yaml = r#"
prompt: "Simple task"
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.prompt, "Simple task");
    assert!(params.provider.is_none());
    assert!(params.model.is_none());
    assert!(params.mcp.is_empty());
    assert!(params.max_turns.is_none());
    assert!(params.stop_conditions.is_empty());
    assert!(params.scope.is_none());
}

// ===============================================================
// Default Values Tests
// ===============================================================

#[test]
fn test_agent_params_defaults() {
    let params = AgentParams::default();

    assert!(params.prompt.is_empty());
    assert_eq!(params.effective_max_turns(), 10); // Default max_turns
    assert!(params.mcp.is_empty());
    assert!(params.provider.is_none());
    assert!(params.model.is_none());
    assert!(params.stop_conditions.is_empty());
    assert!(params.scope.is_none());
}

// ===============================================================
// Stop Condition Tests
// ===============================================================

#[test]
fn test_agent_should_stop_matches_condition() {
    let params = AgentParams {
        prompt: "test".to_string(),
        stop_conditions: vec!["DONE".to_string(), "COMPLETE".to_string()],
        ..Default::default()
    };

    assert!(params.should_stop("Task is DONE"));
    assert!(params.should_stop("COMPLETE"));
    assert!(params.should_stop("Work is COMPLETE now"));
    assert!(!params.should_stop("Still working..."));
    assert!(!params.should_stop("done")); // Case sensitive
}

#[test]
fn test_agent_should_stop_empty_conditions() {
    let params = AgentParams {
        prompt: "test".to_string(),
        stop_conditions: vec![],
        ..Default::default()
    };

    // Empty conditions should never trigger stop
    assert!(!params.should_stop("DONE"));
    assert!(!params.should_stop("COMPLETE"));
    assert!(!params.should_stop("anything"));
}

// ===============================================================
// Validation Tests
// ===============================================================

#[test]
fn test_agent_validate_empty_prompt() {
    let params = AgentParams::default();

    let result = params.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("prompt"));
}

#[test]
fn test_agent_validate_valid_prompt() {
    let params = AgentParams {
        prompt: "test".to_string(),
        ..Default::default()
    };

    assert!(params.validate().is_ok());
}

#[test]
fn test_agent_validate_zero_max_turns() {
    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(0),
        ..Default::default()
    };

    let result = params.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("max_turns"));
}

#[test]
fn test_agent_validate_excessive_max_turns() {
    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(101),
        ..Default::default()
    };

    let result = params.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("100"));
}

#[test]
fn test_agent_validate_valid_max_turns() {
    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(50),
        ..Default::default()
    };

    assert!(params.validate().is_ok());
}

#[test]
fn test_agent_validate_boundary_max_turns() {
    // Test boundary values
    let params_one = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(1),
        ..Default::default()
    };
    assert!(params_one.validate().is_ok());

    let params_hundred = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(100),
        ..Default::default()
    };
    assert!(params_hundred.validate().is_ok());
}

// ===============================================================
// Effective Max Turns Tests
// ===============================================================

#[test]
fn test_agent_effective_max_turns_default() {
    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: None,
        ..Default::default()
    };

    assert_eq!(params.effective_max_turns(), 10);
}

#[test]
fn test_agent_effective_max_turns_custom() {
    let params = AgentParams {
        prompt: "test".to_string(),
        max_turns: Some(25),
        ..Default::default()
    };

    assert_eq!(params.effective_max_turns(), 25);
}

// ===============================================================
// Scope Tests
// ===============================================================

#[test]
fn test_agent_params_with_scope() {
    let yaml = r#"
prompt: "Run analysis"
scope: minimal
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.scope, Some("minimal".to_string()));
}

// ===============================================================
// Multiple MCP Servers Tests
// ===============================================================

#[test]
fn test_agent_params_multiple_mcp() {
    let yaml = r#"
prompt: "Complex task"
mcp:
  - novanet
  - filesystem
  - github
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.mcp.len(), 3);
    assert!(params.mcp.contains(&"novanet".to_string()));
    assert!(params.mcp.contains(&"filesystem".to_string()));
    assert!(params.mcp.contains(&"github".to_string()));
}
