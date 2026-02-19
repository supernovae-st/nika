//! Tests for spawn_agent tool (MVP 8 Phase 2)
//!
//! TDD RED phase: These tests should FAIL initially.
//! They define the expected behavior for nested agent spawning.

use nika::ast::AgentParams;
use nika::event::{EventKind, EventLog};
use serde_json::json;

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 1: AgentParams depth_limit field
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_agent_params_has_depth_limit_field() {
    // AgentParams should have an optional depth_limit field
    let params = AgentParams {
        prompt: "test prompt".to_string(),
        depth_limit: Some(3), // NEW FIELD - should exist
        ..Default::default()
    };
    assert_eq!(params.depth_limit, Some(3));
}

#[test]
fn test_agent_params_depth_limit_defaults_to_none() {
    let params = AgentParams::default();
    assert!(params.depth_limit.is_none());
}

#[test]
fn test_agent_params_effective_depth_limit_default() {
    // Should have an effective_depth_limit() method returning 3 by default
    let params = AgentParams {
        prompt: "test".to_string(),
        ..Default::default()
    };
    assert_eq!(params.effective_depth_limit(), 3); // NEW METHOD
}

#[test]
fn test_agent_params_effective_depth_limit_custom() {
    let params = AgentParams {
        prompt: "test".to_string(),
        depth_limit: Some(5),
        ..Default::default()
    };
    assert_eq!(params.effective_depth_limit(), 5);
}

#[test]
fn test_agent_params_depth_limit_parses_from_yaml() {
    let yaml = r#"
prompt: "Test prompt"
depth_limit: 4
"#;
    let params: AgentParams = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(params.depth_limit, Some(4));
}

#[test]
fn test_agent_params_validate_zero_depth_limit_fails() {
    let params = AgentParams {
        prompt: "test".to_string(),
        depth_limit: Some(0),
        ..Default::default()
    };
    let err = params.validate().unwrap_err();
    assert!(err.contains("depth_limit must be > 0"));
}

#[test]
fn test_agent_params_validate_excessive_depth_limit_fails() {
    let params = AgentParams {
        prompt: "test".to_string(),
        depth_limit: Some(11), // Max should be 10
        ..Default::default()
    };
    let err = params.validate().unwrap_err();
    assert!(err.contains("depth_limit cannot exceed"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 2: AgentSpawned event
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_event_kind_has_agent_spawned_variant() {
    // EventKind should have an AgentSpawned variant
    let event = EventKind::AgentSpawned {
        parent_task_id: "parent-1".into(),
        child_task_id: "child-1".into(),
        depth: 2,
    };

    // Verify we can match on it
    if let EventKind::AgentSpawned {
        parent_task_id,
        child_task_id,
        depth,
    } = event
    {
        assert_eq!(&*parent_task_id, "parent-1");
        assert_eq!(&*child_task_id, "child-1");
        assert_eq!(depth, 2);
    } else {
        panic!("Expected AgentSpawned variant");
    }
}

#[test]
fn test_agent_spawned_serializes_correctly() {
    let event = EventKind::AgentSpawned {
        parent_task_id: "parent".into(),
        child_task_id: "child".into(),
        depth: 1,
    };

    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("agent_spawned"));
    assert!(json.contains("parent_task_id"));
    assert!(json.contains("child_task_id"));
    assert!(json.contains("depth"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 3: SpawnAgentTool existence and interface
// ═══════════════════════════════════════════════════════════════════════════════

// Note: These tests require the spawn module to exist
// They will fail until runtime/spawn.rs is created

#[test]
fn test_spawn_agent_tool_exists() {
    // SpawnAgentTool should exist with new() constructor
    use nika::runtime::spawn::SpawnAgentTool;

    let _tool = SpawnAgentTool::new(
        1,                    // current_depth
        3,                    // max_depth
        "parent-task".into(), // parent_task_id
        EventLog::new(),      // event_log
    );
}

#[test]
fn test_spawn_agent_tool_name() {
    use nika::runtime::spawn::SpawnAgentTool;

    let tool = SpawnAgentTool::new(1, 3, "parent".into(), EventLog::new());
    assert_eq!(tool.name(), "spawn_agent");
}

#[test]
fn test_spawn_agent_tool_definition_has_required_params() {
    use nika::runtime::spawn::SpawnAgentTool;

    let tool = SpawnAgentTool::new(1, 3, "parent".into(), EventLog::new());
    let def = tool.definition();

    // Should have task_id and prompt as required
    let schema = &def.parameters;
    let required = schema
        .get("required")
        .and_then(|v| v.as_array())
        .expect("required should be an array");

    assert!(required.iter().any(|v| v == "task_id"));
    assert!(required.iter().any(|v| v == "prompt"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 4: Depth limit enforcement
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_spawn_agent_at_max_depth_returns_error() {
    use nika::runtime::spawn::SpawnAgentTool;

    // At max depth (3/3), spawn should fail
    let tool = SpawnAgentTool::new(3, 3, "parent".into(), EventLog::new());

    let args = json!({
        "task_id": "child-1",
        "prompt": "Do something"
    })
    .to_string();

    let result = tool.call(args).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.to_string().contains("depth limit"));
}

#[tokio::test]
async fn test_spawn_agent_below_max_depth_allowed() {
    use nika::runtime::spawn::SpawnAgentTool;

    // Below max depth (2/3), spawn should be allowed (mock mode)
    let tool = SpawnAgentTool::new(2, 3, "parent".into(), EventLog::new());

    let args = json!({
        "task_id": "child-1",
        "prompt": "Do something"
    })
    .to_string();

    // Note: This will fail in real execution without MCP clients
    // but should not fail with depth limit error
    let result = tool.call(args).await;

    // Either succeeds or fails for a reason OTHER than depth limit
    if let Err(e) = &result {
        assert!(
            !e.to_string().contains("depth limit"),
            "Should not fail due to depth limit"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 5: Event emission
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_spawn_agent_emits_agent_spawned_event() {
    use nika::runtime::spawn::SpawnAgentTool;

    let event_log = EventLog::new();
    let tool = SpawnAgentTool::new(1, 3, "parent".into(), event_log.clone());

    let args = json!({
        "task_id": "child-1",
        "prompt": "Do something"
    })
    .to_string();

    // Attempt to spawn (may fail for other reasons, but should emit event first)
    let _ = tool.call(args).await;

    // Check that AgentSpawned event was emitted
    let events = event_log.events();
    let spawned_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::AgentSpawned { .. }))
        .collect();

    assert!(
        !spawned_events.is_empty(),
        "Expected AgentSpawned event to be emitted"
    );

    // Verify event content
    if let EventKind::AgentSpawned {
        parent_task_id,
        child_task_id,
        depth,
    } = &spawned_events[0].kind
    {
        assert_eq!(&**parent_task_id, "parent");
        assert_eq!(&**child_task_id, "child-1");
        assert_eq!(*depth, 2); // current_depth + 1
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// SECTION 6: SpawnAgentParams
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn test_spawn_agent_params_struct_exists() {
    use nika::runtime::spawn::SpawnAgentParams;

    let params = SpawnAgentParams {
        task_id: "child-1".to_string(),
        prompt: "Do something".to_string(),
        context: Some(json!({"key": "value"})),
        max_turns: Some(5),
    };

    assert_eq!(params.task_id, "child-1");
    assert_eq!(params.prompt, "Do something");
    assert!(params.context.is_some());
    assert_eq!(params.max_turns, Some(5));
}

#[test]
fn test_spawn_agent_params_deserializes_from_json() {
    use nika::runtime::spawn::SpawnAgentParams;

    let json = json!({
        "task_id": "child-1",
        "prompt": "Do something",
        "context": {"key": "value"},
        "max_turns": 5
    });

    let params: SpawnAgentParams = serde_json::from_value(json).unwrap();
    assert_eq!(params.task_id, "child-1");
    assert_eq!(params.max_turns, Some(5));
}

#[test]
fn test_spawn_agent_params_context_optional() {
    use nika::runtime::spawn::SpawnAgentParams;

    let json = json!({
        "task_id": "child-1",
        "prompt": "Do something"
    });

    let params: SpawnAgentParams = serde_json::from_value(json).unwrap();
    assert!(params.context.is_none());
    assert!(params.max_turns.is_none());
}
