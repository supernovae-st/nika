//! Spawn Agent Tool (MVP 8 Phase 2)
//!
//! Internal tool for recursive agent spawning. Allows an agent to delegate
//! subtasks to child agents, enabling hierarchical task decomposition.
//!
//! ## Depth Limit
//!
//! To prevent infinite recursion, each spawn tracks depth and enforces limits:
//! - Default limit: 3 levels
//! - Maximum limit: 10 levels
//! - Spawning at max depth returns an error
//!
//! ## Events
//!
//! Spawning emits an `AgentSpawned` event with:
//! - `parent_task_id`: The spawning agent's task ID
//! - `child_task_id`: The new agent's task ID
//! - `depth`: Current recursion depth
//!
//! ## Example
//!
//! ```json
//! {
//!   "task_id": "subtask-1",
//!   "prompt": "Generate the header section",
//!   "context": {"entity": "qr-code"},
//!   "max_turns": 5
//! }
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::event::{EventKind, EventLog};

/// Parameters for spawning a child agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnAgentParams {
    /// Unique identifier for the child task
    pub task_id: String,
    /// Prompt/goal for the child agent
    pub prompt: String,
    /// Optional context data to pass to child
    #[serde(default)]
    pub context: Option<Value>,
    /// Optional max turns override for child
    #[serde(default)]
    pub max_turns: Option<u32>,
}

/// Internal tool for spawning sub-agents
///
/// This tool is automatically added to agents that have depth_limit > current_depth.
/// It allows recursive task decomposition with safety limits.
#[derive(Clone)]
pub struct SpawnAgentTool {
    /// Current recursion depth (1 = root agent)
    current_depth: u32,
    /// Maximum allowed depth
    max_depth: u32,
    /// Parent task ID for event linking
    parent_task_id: Arc<str>,
    /// Event log for emitting AgentSpawned events
    event_log: EventLog,
}

impl SpawnAgentTool {
    /// Create a new SpawnAgentTool
    ///
    /// # Arguments
    /// * `current_depth` - Current recursion depth (starts at 1 for root)
    /// * `max_depth` - Maximum allowed depth (default 3)
    /// * `parent_task_id` - ID of the parent task
    /// * `event_log` - Shared event log for observability
    pub fn new(
        current_depth: u32,
        max_depth: u32,
        parent_task_id: Arc<str>,
        event_log: EventLog,
    ) -> Self {
        Self {
            current_depth,
            max_depth,
            parent_task_id,
            event_log,
        }
    }

    /// Get the tool name
    pub fn name(&self) -> &str {
        "spawn_agent"
    }

    /// Get the JSON Schema definition for this tool
    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn_agent".to_string(),
            description: "Spawn a sub-agent to handle a delegated subtask. The child agent \
                         runs independently and returns its result when complete."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "Unique identifier for the child task"
                    },
                    "prompt": {
                        "type": "string",
                        "description": "Goal/prompt for the child agent"
                    },
                    "context": {
                        "type": "object",
                        "description": "Optional context data to pass to child"
                    },
                    "max_turns": {
                        "type": "integer",
                        "description": "Optional max turns override (default: 10)"
                    }
                },
                "required": ["task_id", "prompt"]
            }),
        }
    }

    /// Execute the spawn_agent tool
    ///
    /// # Errors
    /// Returns an error if:
    /// - Current depth >= max depth (depth limit reached)
    /// - Invalid arguments
    pub async fn call(&self, args: String) -> Result<String, SpawnAgentError> {
        // Parse arguments
        let params: SpawnAgentParams = serde_json::from_str(&args)
            .map_err(|e| SpawnAgentError::InvalidArgs(e.to_string()))?;

        // Check depth limit
        if self.current_depth >= self.max_depth {
            return Err(SpawnAgentError::DepthLimitReached {
                current: self.current_depth,
                max: self.max_depth,
            });
        }

        // Emit AgentSpawned event
        let child_depth = self.current_depth + 1;
        self.event_log.emit(EventKind::AgentSpawned {
            parent_task_id: self.parent_task_id.clone(),
            child_task_id: Arc::from(params.task_id.as_str()),
            depth: child_depth,
        });

        // In a full implementation, we would:
        // 1. Create a new RigAgentLoop with reduced depth_limit
        // 2. Execute the child agent
        // 3. Return the child's result
        //
        // For now, we return a placeholder indicating the spawn was accepted.
        // Full implementation requires MCP client injection.
        Ok(json!({
            "status": "spawned",
            "child_task_id": params.task_id,
            "depth": child_depth,
            "note": "Child agent execution requires MCP client context"
        })
        .to_string())
    }

    /// Check if spawning is allowed at current depth
    pub fn can_spawn(&self) -> bool {
        self.current_depth < self.max_depth
    }

    /// Get the depth that child agents would have
    pub fn child_depth(&self) -> u32 {
        self.current_depth + 1
    }
}

/// Tool definition structure (compatible with MCP/rig)
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Errors that can occur when spawning agents
#[derive(Debug, thiserror::Error)]
pub enum SpawnAgentError {
    #[error("spawn_agent: depth limit reached (current: {current}, max: {max})")]
    DepthLimitReached { current: u32, max: u32 },

    #[error("spawn_agent: invalid arguments - {0}")]
    InvalidArgs(String),

    #[error("spawn_agent: execution failed - {0}")]
    ExecutionFailed(String),
}

impl std::fmt::Debug for SpawnAgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnAgentTool")
            .field("current_depth", &self.current_depth)
            .field("max_depth", &self.max_depth)
            .field("parent_task_id", &self.parent_task_id)
            .finish()
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// rig::ToolDyn implementation (for integration with RigAgentLoop)
// ═══════════════════════════════════════════════════════════════════════════════

// TODO(MVP 8): rig::ToolDyn integration when full agent spawning is implemented
// The rig-core 0.31 ToolDyn trait has changed (async definition, different lifetimes).
// Full implementation requires MCP client injection into spawned agents.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_agent_tool_name() {
        let tool = SpawnAgentTool::new(1, 3, "parent".into(), EventLog::new());
        assert_eq!(tool.name(), "spawn_agent");
    }

    #[test]
    fn spawn_agent_tool_can_spawn() {
        let tool = SpawnAgentTool::new(1, 3, "parent".into(), EventLog::new());
        assert!(tool.can_spawn());

        let at_limit = SpawnAgentTool::new(3, 3, "parent".into(), EventLog::new());
        assert!(!at_limit.can_spawn());
    }

    #[test]
    fn spawn_agent_tool_child_depth() {
        let tool = SpawnAgentTool::new(1, 3, "parent".into(), EventLog::new());
        assert_eq!(tool.child_depth(), 2);
    }

    #[test]
    fn spawn_agent_params_deserializes() {
        let json = json!({
            "task_id": "child-1",
            "prompt": "Do something",
            "context": {"key": "value"},
            "max_turns": 5
        });

        let params: SpawnAgentParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.task_id, "child-1");
        assert_eq!(params.prompt, "Do something");
        assert!(params.context.is_some());
        assert_eq!(params.max_turns, Some(5));
    }

    #[test]
    fn spawn_agent_params_minimal() {
        let json = json!({
            "task_id": "child-1",
            "prompt": "Do something"
        });

        let params: SpawnAgentParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.task_id, "child-1");
        assert!(params.context.is_none());
        assert!(params.max_turns.is_none());
    }

    #[tokio::test]
    async fn spawn_agent_at_max_depth_fails() {
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
    async fn spawn_agent_below_max_depth_succeeds() {
        let tool = SpawnAgentTool::new(2, 3, "parent".into(), EventLog::new());

        let args = json!({
            "task_id": "child-1",
            "prompt": "Do something"
        })
        .to_string();

        let result = tool.call(args).await;
        assert!(result.is_ok());

        let response: Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["status"], "spawned");
        assert_eq!(response["child_task_id"], "child-1");
        assert_eq!(response["depth"], 3);
    }

    #[tokio::test]
    async fn spawn_agent_emits_event() {
        let event_log = EventLog::new();
        let tool = SpawnAgentTool::new(1, 3, "parent".into(), event_log.clone());

        let args = json!({
            "task_id": "child-1",
            "prompt": "Do something"
        })
        .to_string();

        let _ = tool.call(args).await;

        // Check that AgentSpawned event was emitted
        let events = event_log.events();
        let spawned_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::AgentSpawned { .. }))
            .collect();

        assert_eq!(spawned_events.len(), 1);

        if let EventKind::AgentSpawned {
            parent_task_id,
            child_task_id,
            depth,
        } = &spawned_events[0].kind
        {
            assert_eq!(&**parent_task_id, "parent");
            assert_eq!(&**child_task_id, "child-1");
            assert_eq!(*depth, 2);
        }
    }

    #[test]
    fn tool_definition_has_required_params() {
        let tool = SpawnAgentTool::new(1, 3, "parent".into(), EventLog::new());
        let def = tool.definition();

        let required = def
            .parameters
            .get("required")
            .and_then(|v| v.as_array())
            .expect("required should be an array");

        assert!(required.iter().any(|v| v == "task_id"));
        assert!(required.iter().any(|v| v == "prompt"));
    }
}
