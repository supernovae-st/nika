// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent verb parsing tests
//!
//! Tests YAML deserialization and validation of agent verb parameters.

use nika::ast::AgentParams;
use nika::serde_yaml;

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
model: claude-sonnet-4-6
mcp:
  - novanet
max_turns: 10
"#;

    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();

    assert!(params.prompt.contains("homepage hero"));
    assert_eq!(params.provider, Some(nika_core::ProviderName::Anthropic));
    assert_eq!(params.model, Some("claude-sonnet-4-6".to_string()));
    assert_eq!(params.mcp, vec!["novanet"]);
    assert_eq!(params.max_turns, Some(10));
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
    assert!(params.scope.is_none());
}

// ===============================================================
// Validation Tests
// ===============================================================

#[test]
fn test_agent_validate_empty_prompt() {
    let params = AgentParams::default();

    let result = params.validate();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("prompt"));
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
    assert!(result.unwrap_err().to_string().contains("max_turns"));
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
    assert!(result.unwrap_err().to_string().contains("100"));
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
