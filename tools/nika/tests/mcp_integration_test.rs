//! MCP Integration Tests
//!
//! Tests MCP scenarios including single MCP, multi-MCP coordination,
//! tool discovery, error handling, and NovaNet introspection.
//!
//! Tests marked #[ignore] require real MCP servers and Neo4j.
//!
//! Run manually:
//! - `cargo nextest run mcp_integration -- --ignored`

use rustc_hash::FxHashMap;

use nika::ast::{McpConfigInline, Workflow};
use nika::event::{EventKind, EventLog};

// ============================================================================
// MCP CONFIG PARSING TESTS
// ============================================================================

#[test]
fn test_parse_single_mcp_config() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: single-mcp-test
description: "Test single MCP configuration"

mcp:
  novanet:
    command: "cargo run --manifest-path ../novanet-mcp/Cargo.toml"

tasks:
  - id: describe
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");

    assert!(workflow.mcp.is_some());
    let mcp = workflow.mcp.as_ref().unwrap();
    assert!(mcp.contains_key("novanet"));
}

#[test]
fn test_parse_multi_mcp_config() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: multi-mcp-test
description: "Test multiple MCP servers"

mcp:
  novanet:
    command: "cargo run --manifest-path ../novanet-mcp/Cargo.toml"
    args:
      - "--port"
      - "8001"
  perplexity:
    command: "npx"
    args:
      - "@anthropic/perplexity-mcp"
  firecrawl:
    command: "npx"
    args:
      - "@anthropic/firecrawl-mcp"

tasks:
  - id: research
    agent:
      prompt: "Research using all available tools"
      mcp:
        - novanet
        - perplexity
        - firecrawl
      max_turns: 10
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");

    let mcp = workflow.mcp.as_ref().unwrap();
    assert_eq!(mcp.len(), 3);
    assert!(mcp.contains_key("novanet"));
    assert!(mcp.contains_key("perplexity"));
    assert!(mcp.contains_key("firecrawl"));
}

#[test]
fn test_parse_mcp_with_env() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: mcp-env-test

mcp:
  external:
    command: "external-mcp-server"
    env:
      API_KEY: "${EXTERNAL_API_KEY}"
      DEBUG: "true"

tasks:
  - id: call
    invoke:
      mcp: external
      tool: external_tool
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");

    let mcp = workflow.mcp.as_ref().unwrap();
    let external = mcp.get("external").unwrap();
    assert!(!external.env.is_empty());
}

// ============================================================================
// MCP CONFIG INLINE TESTS
// ============================================================================

#[test]
fn test_mcp_config_inline_creation() {
    let mut env = FxHashMap::default();
    env.insert("RUST_LOG".to_string(), "info".to_string());

    let config = McpConfigInline {
        command: "cargo run".to_string(),
        args: vec!["--release".to_string()],
        env,
        cwd: None,
    };

    assert_eq!(config.command, "cargo run");
    assert!(!config.args.is_empty());
    assert!(!config.env.is_empty());
}

#[test]
fn test_mcp_config_inline_default_args() {
    let config = McpConfigInline {
        command: "simple-command".to_string(),
        args: vec![],
        env: FxHashMap::default(),
        cwd: None,
    };

    assert_eq!(config.command, "simple-command");
    assert!(config.args.is_empty());
    assert!(config.env.is_empty());
}

// ============================================================================
// MCP EVENT TESTS (using correct EventKind variants)
// ============================================================================

#[test]
fn test_mcp_invoke_event() {
    let log = EventLog::new();

    // Use correct EventKind::McpInvoke variant
    log.emit(EventKind::McpInvoke {
        task_id: "task-1".into(),
        call_id: "call-001".to_string(),
        mcp_server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        params: Some(serde_json::json!({"entity": "qr-code"})),
    });

    let events = log.events();
    assert_eq!(events.len(), 1);

    if let EventKind::McpInvoke {
        mcp_server, tool, ..
    } = &events[0].kind
    {
        assert_eq!(mcp_server, "novanet");
        assert_eq!(tool.as_deref(), Some("novanet_describe"));
    } else {
        panic!("Expected McpInvoke event");
    }
}

#[test]
fn test_mcp_response_event() {
    let log = EventLog::new();

    // Use correct EventKind::McpResponse variant
    log.emit(EventKind::McpResponse {
        task_id: "task-1".into(),
        call_id: "call-001".to_string(),
        output_len: 256,
        duration_ms: 150,
        cached: false,
        is_error: false,
        response: Some(serde_json::json!({
            "entity": {
                "key": "qr-code",
                "name": "QR Code",
                "description": "Quick Response code"
            }
        })),
    });

    let events = log.events();
    if let EventKind::McpResponse { duration_ms, .. } = &events[0].kind {
        assert_eq!(*duration_ms, 150);
    }
}

#[test]
fn test_mcp_tool_sequence() {
    let log = EventLog::new();

    // Simulate realistic MCP tool sequence using correct variants
    let tools = [
        ("novanet_describe", r#"{"entity": "qr-code"}"#),
        (
            "novanet_traverse",
            r#"{"start": "entity:qr-code", "arc": "HAS_NATIVE"}"#,
        ),
        (
            "novanet_generate",
            r#"{"entity": "qr-code", "locale": "fr-FR"}"#,
        ),
    ];

    for (idx, (tool, params)) in tools.iter().enumerate() {
        let call_id = format!("call-{:03}", idx + 1);

        log.emit(EventKind::McpInvoke {
            task_id: "task-1".into(),
            call_id: call_id.clone(),
            mcp_server: "novanet".to_string(),
            tool: Some(tool.to_string()),
            resource: None,
            params: Some(serde_json::from_str(params).unwrap()),
        });

        log.emit(EventKind::McpResponse {
            task_id: "task-1".into(),
            call_id,
            output_len: 100,
            duration_ms: 100,
            cached: false,
            is_error: false,
            response: Some(serde_json::json!({"success": true})),
        });
    }

    let events = log.events();
    assert_eq!(events.len(), 6); // 3 invokes + 3 responses
}

// ============================================================================
// NOVANET INTROSPECT TESTS
// ============================================================================

#[test]
fn test_parse_introspect_workflow() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: introspect-test
description: "Test novanet_introspect tool"

mcp:
  novanet:
    command: "cargo run --manifest-path ../novanet-mcp/Cargo.toml"

tasks:
  - id: get-schema
    invoke:
      mcp: novanet
      tool: novanet_introspect
      params:
        query: "nodes"

  - id: get-arcs
    invoke:
      mcp: novanet
      tool: novanet_introspect
      params:
        query: "arcs"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");

    assert_eq!(workflow.tasks.len(), 2);
}

#[test]
fn test_introspect_event() {
    let log = EventLog::new();

    log.emit(EventKind::McpInvoke {
        task_id: "introspect".into(),
        call_id: "intro-001".to_string(),
        mcp_server: "novanet".to_string(),
        tool: Some("novanet_introspect".to_string()),
        resource: None,
        params: Some(serde_json::json!({"query": "nodes"})),
    });

    log.emit(EventKind::McpResponse {
        task_id: "introspect".into(),
        call_id: "intro-001".to_string(),
        output_len: 500,
        duration_ms: 50,
        cached: false,
        is_error: false,
        response: Some(serde_json::json!({
            "nodes": ["Entity", "EntityNative", "Page", "PageNative"],
            "count": 61
        })),
    });

    let events = log.events();
    assert_eq!(events.len(), 2);
}

// ============================================================================
// MULTI-MCP COORDINATION TESTS
// ============================================================================

#[test]
fn test_parse_multi_mcp_agent_workflow() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: multi-mcp-coordination

mcp:
  novanet:
    command: "novanet-mcp"
  search:
    command: "perplexity-mcp"

tasks:
  - id: research-agent
    agent:
      prompt: |
        Research QR codes and find relevant information.
        Use novanet for entity knowledge and perplexity for web search.
      mcp:
        - novanet
        - search
      max_turns: 15
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);

    let mcp = workflow.mcp.as_ref().unwrap();
    assert_eq!(mcp.len(), 2);
}

#[test]
fn test_multi_mcp_events() {
    let log = EventLog::new();

    // NovaNet call
    log.emit(EventKind::McpInvoke {
        task_id: "research".into(),
        call_id: "novanet-001".to_string(),
        mcp_server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        params: Some(serde_json::json!({"entity": "qr-code"})),
    });

    log.emit(EventKind::McpResponse {
        task_id: "research".into(),
        call_id: "novanet-001".to_string(),
        output_len: 500,
        duration_ms: 120,
        cached: false,
        is_error: false,
        response: None,
    });

    // Perplexity call
    log.emit(EventKind::McpInvoke {
        task_id: "research".into(),
        call_id: "perplexity-001".to_string(),
        mcp_server: "perplexity".to_string(),
        tool: Some("search".to_string()),
        resource: None,
        params: Some(serde_json::json!({"query": "QR code best practices 2025"})),
    });

    log.emit(EventKind::McpResponse {
        task_id: "research".into(),
        call_id: "perplexity-001".to_string(),
        output_len: 1500,
        duration_ms: 800,
        cached: false,
        is_error: false,
        response: None,
    });

    let events = log.events();
    assert_eq!(events.len(), 4);

    // Verify different servers used
    let servers: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let EventKind::McpInvoke { mcp_server, .. } = &e.kind {
                Some(mcp_server.clone())
            } else {
                None
            }
        })
        .collect();

    assert!(servers.contains(&"novanet".to_string()));
    assert!(servers.contains(&"perplexity".to_string()));
}

// ============================================================================
// MCP ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_mcp_error_event() {
    let log = EventLog::new();

    log.emit(EventKind::McpInvoke {
        task_id: "error-test".into(),
        call_id: "err-001".to_string(),
        mcp_server: "novanet".to_string(),
        tool: Some("novanet_describe".to_string()),
        resource: None,
        params: Some(serde_json::json!({"entity": "nonexistent"})),
    });

    log.emit(EventKind::McpResponse {
        task_id: "error-test".into(),
        call_id: "err-001".to_string(),
        output_len: 50,
        duration_ms: 30,
        cached: false,
        is_error: true,
        response: Some(serde_json::json!({"error": "Entity not found: nonexistent"})),
    });

    let events = log.events();
    assert_eq!(events.len(), 2);

    // Verify error flag
    if let EventKind::McpResponse { is_error, .. } = &events[1].kind {
        assert!(*is_error);
    }
}

#[test]
fn test_mcp_cached_response() {
    let log = EventLog::new();

    log.emit(EventKind::McpResponse {
        task_id: "cached-test".into(),
        call_id: "cache-001".to_string(),
        output_len: 256,
        duration_ms: 5, // Very fast because cached
        cached: true,
        is_error: false,
        response: Some(serde_json::json!({"entity": "qr-code"})),
    });

    let events = log.events();
    if let EventKind::McpResponse { cached, .. } = &events[0].kind {
        assert!(*cached);
    }
}

// ============================================================================
// MCP RESOURCE TESTS
// ============================================================================

#[test]
fn test_mcp_resource_read() {
    let log = EventLog::new();

    // MCP can also read resources, not just call tools
    log.emit(EventKind::McpInvoke {
        task_id: "resource-test".into(),
        call_id: "res-001".to_string(),
        mcp_server: "novanet".to_string(),
        tool: None, // No tool - this is a resource read
        resource: Some("entity://qr-code".to_string()),
        params: None,
    });

    let events = log.events();
    if let EventKind::McpInvoke { tool, resource, .. } = &events[0].kind {
        assert!(tool.is_none());
        assert_eq!(resource.as_deref(), Some("entity://qr-code"));
    }
}

// ============================================================================
// WORKFLOW VALIDATION TESTS
// ============================================================================

#[test]
fn test_parse_invoke_task() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: invoke-test

tasks:
  - id: call-tool
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        entity: "qr-code"
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}

#[test]
fn test_parse_invoke_without_params() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: invoke-no-params

tasks:
  - id: list-tools
    invoke:
      mcp: novanet
      tool: novanet_list_tools
"#;

    let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
    assert_eq!(workflow.tasks.len(), 1);
}
