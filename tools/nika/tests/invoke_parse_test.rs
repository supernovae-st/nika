//! Integration tests for InvokeParams parsing
//!
//! Tests YAML deserialization and validation of invoke verb parameters.

use nika::ast::InvokeParams;
use serde_json::json;

// ═══════════════════════════════════════════════════════════════
// Tool Call Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invoke_params_tool_call() {
    // Parse invoke with mcp, tool, params
    let yaml = r#"
mcp: novanet
tool: novanet_generate
params:
  mode: block
  entity: qr-code
  locale: fr-FR
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.mcp, "novanet");
    assert_eq!(params.tool, Some("novanet_generate".to_string()));
    assert_eq!(
        params.params,
        Some(json!({
            "mode": "block",
            "entity": "qr-code",
            "locale": "fr-FR"
        }))
    );
    assert!(params.resource.is_none());

    // Validation should pass
    assert!(params.validate().is_ok());

    // Helper methods
    assert!(params.is_tool_call());
    assert!(!params.is_resource_read());
}

#[test]
fn test_invoke_params_tool_call_minimal() {
    // Tool call without params
    let yaml = r#"
mcp: novanet
tool: novanet_list_entities
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.mcp, "novanet");
    assert_eq!(params.tool, Some("novanet_list_entities".to_string()));
    assert!(params.params.is_none());
    assert!(params.resource.is_none());

    assert!(params.validate().is_ok());
    assert!(params.is_tool_call());
}

// ═══════════════════════════════════════════════════════════════
// Resource Read Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invoke_params_resource_read() {
    // Parse invoke with mcp, resource
    let yaml = r#"
mcp: novanet
resource: entity://qr-code/fr-FR
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    assert_eq!(params.mcp, "novanet");
    assert!(params.tool.is_none());
    assert!(params.params.is_none());
    assert_eq!(params.resource, Some("entity://qr-code/fr-FR".to_string()));

    // Validation should pass
    assert!(params.validate().is_ok());

    // Helper methods
    assert!(!params.is_tool_call());
    assert!(params.is_resource_read());
}

// ═══════════════════════════════════════════════════════════════
// Validation Error Tests
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invoke_params_validation_both_tool_and_resource() {
    // Error if both tool and resource are set
    let yaml = r#"
mcp: novanet
tool: novanet_generate
resource: entity://qr-code/fr-FR
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    // Both are set
    assert!(params.tool.is_some());
    assert!(params.resource.is_some());

    // Validation should fail
    let result = params.validate();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("mutually exclusive") || err.contains("both"),
        "Error should mention mutual exclusivity: {}",
        err
    );
}

#[test]
fn test_invoke_params_validation_neither() {
    // Error if neither tool nor resource is set
    let yaml = r#"
mcp: novanet
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    // Neither is set
    assert!(params.tool.is_none());
    assert!(params.resource.is_none());

    // Validation should fail
    let result = params.validate();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("tool") || err.contains("resource"),
        "Error should mention required fields: {}",
        err
    );
}

// ═══════════════════════════════════════════════════════════════
// Edge Cases
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_invoke_params_complex_params() {
    // Complex nested params
    let yaml = r#"
mcp: novanet
tool: novanet_generate
params:
  forms:
    - text
    - title
    - meta_description
  options:
    style: casual
    max_length: 500
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    assert!(params.validate().is_ok());
    let p = params.params.unwrap();
    assert_eq!(p["forms"], json!(["text", "title", "meta_description"]));
    assert_eq!(p["options"]["style"], json!("casual"));
    assert_eq!(p["options"]["max_length"], json!(500));
}

#[test]
fn test_invoke_params_empty_params() {
    // Empty params object is valid
    let yaml = r#"
mcp: novanet
tool: novanet_ping
params: {}
"#;

    let params: InvokeParams = serde_yaml::from_str(yaml).unwrap();

    assert!(params.validate().is_ok());
    assert_eq!(params.params, Some(json!({})));
}
