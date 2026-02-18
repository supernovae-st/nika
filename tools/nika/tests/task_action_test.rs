//! Integration tests for TaskAction enum parsing
//!
//! Tests that TaskAction correctly parses all 4 variants (v0.2):
//! - Infer: LLM inference
//! - Exec: Shell command execution
//! - Fetch: HTTP request
//! - Invoke: MCP tool call / resource read

use nika::ast::TaskAction;

// ═══════════════════════════════════════════════════════════════
// Invoke Variant Tests (NEW in v0.2)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_task_action_invoke_variant() {
    // Parse YAML with invoke: block
    let yaml = r#"
invoke:
  mcp: novanet
  tool: novanet_generate
  params:
    mode: block
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    // Verify it parsed as Invoke variant
    match action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp, "novanet");
            assert_eq!(invoke.tool, Some("novanet_generate".to_string()));
            assert!(invoke.resource.is_none());
            assert!(invoke.validate().is_ok());
        }
        other => panic!("Expected Invoke variant, got {:?}", other),
    }
}

#[test]
fn test_task_action_invoke_resource_read() {
    // Invoke with resource instead of tool
    let yaml = r#"
invoke:
  mcp: novanet
  resource: entity://qr-code/fr-FR
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Invoke { invoke } => {
            assert_eq!(invoke.mcp, "novanet");
            assert!(invoke.tool.is_none());
            assert_eq!(invoke.resource, Some("entity://qr-code/fr-FR".to_string()));
            assert!(invoke.is_resource_read());
        }
        other => panic!("Expected Invoke variant, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════
// Infer Variant Tests (existing)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_task_action_infer_still_works() {
    // Ensure existing infer: parsing still works
    let yaml = r#"
infer:
  prompt: "Say hello world"
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Say hello world");
            assert!(infer.provider.is_none());
            assert!(infer.model.is_none());
        }
        other => panic!("Expected Infer variant, got {:?}", other),
    }
}

#[test]
fn test_task_action_infer_with_overrides() {
    // Infer with provider and model overrides
    let yaml = r#"
infer:
  prompt: "Analyze this"
  provider: openai
  model: gpt-4
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Infer { infer } => {
            assert_eq!(infer.prompt, "Analyze this");
            assert_eq!(infer.provider, Some("openai".to_string()));
            assert_eq!(infer.model, Some("gpt-4".to_string()));
        }
        other => panic!("Expected Infer variant, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════
// Exec Variant Tests (existing)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_task_action_exec_variant() {
    let yaml = r#"
exec:
  command: "ls -la"
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Exec { exec } => {
            assert_eq!(exec.command, "ls -la");
        }
        other => panic!("Expected Exec variant, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════
// Fetch Variant Tests (existing)
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_task_action_fetch_variant() {
    let yaml = r#"
fetch:
  url: "https://api.example.com/data"
  method: POST
  headers:
    Authorization: "Bearer token"
  body: '{"key": "value"}'
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.url, "https://api.example.com/data");
            assert_eq!(fetch.method, "POST");
            assert_eq!(
                fetch.headers.get("Authorization"),
                Some(&"Bearer token".to_string())
            );
            assert_eq!(fetch.body, Some(r#"{"key": "value"}"#.to_string()));
        }
        other => panic!("Expected Fetch variant, got {:?}", other),
    }
}

#[test]
fn test_task_action_fetch_defaults() {
    // Fetch with minimal config - method defaults to GET
    let yaml = r#"
fetch:
  url: "https://example.com"
"#;

    let action: TaskAction = serde_yaml::from_str(yaml).unwrap();

    match action {
        TaskAction::Fetch { fetch } => {
            assert_eq!(fetch.url, "https://example.com");
            assert_eq!(fetch.method, "GET"); // Default
            assert!(fetch.headers.is_empty());
            assert!(fetch.body.is_none());
        }
        other => panic!("Expected Fetch variant, got {:?}", other),
    }
}

// ═══════════════════════════════════════════════════════════════
// All Variants Test
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_task_action_all_variants() {
    // Test that all 4 variants parse correctly (v0.2)

    // 1. Infer
    let infer_yaml = r#"
infer:
  prompt: "Test"
"#;
    let infer_action: TaskAction = serde_yaml::from_str(infer_yaml).unwrap();
    assert!(matches!(infer_action, TaskAction::Infer { .. }));

    // 2. Exec
    let exec_yaml = r#"
exec:
  command: "echo test"
"#;
    let exec_action: TaskAction = serde_yaml::from_str(exec_yaml).unwrap();
    assert!(matches!(exec_action, TaskAction::Exec { .. }));

    // 3. Fetch
    let fetch_yaml = r#"
fetch:
  url: "https://example.com"
"#;
    let fetch_action: TaskAction = serde_yaml::from_str(fetch_yaml).unwrap();
    assert!(matches!(fetch_action, TaskAction::Fetch { .. }));

    // 4. Invoke (NEW)
    let invoke_yaml = r#"
invoke:
  mcp: novanet
  tool: test_tool
"#;
    let invoke_action: TaskAction = serde_yaml::from_str(invoke_yaml).unwrap();
    assert!(matches!(invoke_action, TaskAction::Invoke { .. }));
}
