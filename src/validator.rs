//! Nika Validator (v3)
//!
//! 5-layer validation pipeline for .nika.yaml workflows.
//! Rules are embedded - no external YAML files needed.

use crate::workflow::{ConnectionKey, Task, TaskType, Workflow};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Error, Debug)]
pub enum ValidationError {
    // Layer 1: Schema
    #[error("[L1] Missing mainAgent.model")]
    MissingModel,

    #[error("[L1] Missing mainAgent.systemPrompt or systemPromptFile")]
    MissingSystemPrompt,

    // Layer 2: Tasks
    #[error("[L2] Task '{task_id}': {message}")]
    TaskError { task_id: String, message: String },

    #[error("[L2] Duplicate task ID: '{task_id}'")]
    DuplicateTaskId { task_id: String },

    // Layer 3: Flows
    #[error("[L3] Flow '{from_task}' → '{to_task}': {message}")]
    FlowError {
        from_task: String,
        to_task: String,
        message: String,
    },

    // Layer 4: Connections
    #[error("[L4] Connection blocked: {from_task} ({from_key}) → {to_task} ({to_key})")]
    ConnectionBlocked {
        from_task: String,
        from_key: ConnectionKey,
        to_task: String,
        to_key: ConnectionKey,
    },

    // Layer 5: Graph (warnings)
    #[error("[L5] Warning: {message}")]
    GraphWarning { message: String },
}

impl ValidationError {
    pub fn is_warning(&self) -> bool {
        matches!(self, ValidationError::GraphWarning { .. })
    }
}

// ============================================================================
// VALIDATION RESULT
// ============================================================================

#[derive(Debug, Default)]
pub struct ValidationResult {
    pub file_path: String,
    pub errors: Vec<ValidationError>,
    pub task_count: usize,
    pub flow_count: usize,
}

impl ValidationResult {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
            ..Default::default()
        }
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.errors.push(error);
    }

    pub fn is_valid(&self) -> bool {
        self.errors.iter().all(|e| e.is_warning())
    }

    pub fn has_warnings(&self) -> bool {
        self.errors.iter().any(|e| e.is_warning())
    }

    pub fn error_count(&self) -> usize {
        self.errors.iter().filter(|e| !e.is_warning()).count()
    }

    pub fn warning_count(&self) -> usize {
        self.errors.iter().filter(|e| e.is_warning()).count()
    }
}

// ============================================================================
// CONNECTION MATRIX (v3)
// ============================================================================

/// Check if a connection is allowed (v3 rules)
///
/// ```text
/// SOURCE              │ TARGET              │ OK?
/// ─────────────────────────────────────────────────
/// agent (main)        │ agent (main)        │ ✅
/// agent (main)        │ agent (isolated)    │ ✅
/// agent (main)        │ action              │ ✅
/// agent (isolated)    │ agent (main)        │ ❌  NEEDS BRIDGE
/// agent (isolated)    │ agent (isolated)    │ ❌  Can't spawn from sub
/// agent (isolated)    │ action              │ ✅  THIS IS THE BRIDGE
/// action              │ agent (main)        │ ✅
/// action              │ agent (isolated)    │ ✅
/// action              │ action              │ ✅
/// ```
pub fn is_connection_allowed(source: &ConnectionKey, target: &ConnectionKey) -> bool {
    use ConnectionKey::*;

    match (source, target) {
        // agent(main) can connect to anything
        (AgentMain, AgentMain) => true,
        (AgentMain, AgentIsolated) => true,
        (AgentMain, Action) => true,

        // agent(isolated) can ONLY connect to action (bridge)
        (AgentIsolated, AgentMain) => false,
        (AgentIsolated, AgentIsolated) => false,
        (AgentIsolated, Action) => true,

        // action can connect to anything
        (Action, AgentMain) => true,
        (Action, AgentIsolated) => true,
        (Action, Action) => true,
    }
}

/// Generate fix suggestion for blocked connections
pub fn bridge_suggestion(source: &Task, target: &Task) -> String {
    let source_key = source.connection_key();
    let target_key = target.connection_key();

    match (&source_key, &target_key) {
        (ConnectionKey::AgentIsolated, ConnectionKey::AgentMain) => {
            format!(
                "\n   💡 Add an action between them (bridge pattern):\n      \
                 {} (isolated) → [action] → {} (main)",
                source.id, target.id
            )
        }
        (ConnectionKey::AgentIsolated, ConnectionKey::AgentIsolated) => {
            format!(
                "\n   💡 Isolated agents cannot spawn other isolated agents.\n      \
                 Route through Main Agent:\n      \
                 {} → action → agent(main) → {}",
                source.id, target.id
            )
        }
        _ => String::new(),
    }
}

// ============================================================================
// VALIDATOR
// ============================================================================

pub struct Validator;

impl Validator {
    pub fn new() -> Self {
        Self
    }

    /// Validate a workflow through all 5 layers
    pub fn validate(&self, workflow: &Workflow, file_path: &str) -> ValidationResult {
        let mut result = ValidationResult::new(file_path);
        result.task_count = workflow.tasks.len();
        result.flow_count = workflow.flows.len();

        // Layer 1: Schema
        self.validate_schema(workflow, &mut result);

        // Layer 2: Tasks
        let task_map = self.validate_tasks(workflow, &mut result);

        // Layer 3: Flows
        self.validate_flows(workflow, &task_map, &mut result);

        // Layer 4: Connections (THE KEY RULE!)
        self.validate_connections(workflow, &task_map, &mut result);

        // Layer 5: Graph (warnings)
        self.validate_graph(workflow, &mut result);

        result
    }

    // ========== Layer 1: Schema ==========

    fn validate_schema(&self, workflow: &Workflow, result: &mut ValidationResult) {
        if workflow.main_agent.model.is_empty() {
            result.add_error(ValidationError::MissingModel);
        }

        if workflow.main_agent.system_prompt.is_none()
            && workflow.main_agent.system_prompt_file.is_none()
        {
            result.add_error(ValidationError::MissingSystemPrompt);
        }
    }

    // ========== Layer 2: Tasks ==========

    fn validate_tasks<'a>(
        &self,
        workflow: &'a Workflow,
        result: &mut ValidationResult,
    ) -> HashMap<&'a str, &'a Task> {
        let mut task_map: HashMap<&str, &Task> = HashMap::new();

        for task in &workflow.tasks {
            // Check duplicate ID
            if task_map.contains_key(task.id.as_str()) {
                result.add_error(ValidationError::DuplicateTaskId {
                    task_id: task.id.clone(),
                });
            }
            task_map.insert(&task.id, task);

            // Validate ID format
            if !Self::is_valid_id(&task.id) {
                result.add_error(ValidationError::TaskError {
                    task_id: task.id.clone(),
                    message: "Invalid ID format (use alphanumeric, hyphens, underscores)".into(),
                });
            }

            // Type-specific validation
            match task.task_type {
                TaskType::Agent => {
                    // Agent requires prompt
                    if task.prompt.is_none() {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "Agent task requires 'prompt'".into(),
                        });
                    }
                    // Agent should not have run
                    if task.run.is_some() {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "'run' is not valid on agent type".into(),
                        });
                    }
                }
                TaskType::Action => {
                    // Action requires run
                    if task.run.is_none() {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "Action task requires 'run'".into(),
                        });
                    }
                    // Action should not have scope
                    if task.scope.is_some() {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "'scope' is not valid on action type".into(),
                        });
                    }
                    // Action should not have prompt
                    if task.prompt.is_some() {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "'prompt' is not valid on action type".into(),
                        });
                    }
                }
            }
        }

        task_map
    }

    fn is_valid_id(id: &str) -> bool {
        !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            && !id.starts_with('-')
            && !id.starts_with('_')
    }

    // ========== Layer 3: Flows ==========

    fn validate_flows(
        &self,
        workflow: &Workflow,
        task_map: &HashMap<&str, &Task>,
        result: &mut ValidationResult,
    ) {
        for flow in &workflow.flows {
            // Check source exists
            if !task_map.contains_key(flow.source.as_str()) {
                result.add_error(ValidationError::FlowError {
                    from_task: flow.source.clone(),
                    to_task: flow.target.clone(),
                    message: format!("Source task '{}' not found", flow.source),
                });
            }

            // Check target exists
            if !task_map.contains_key(flow.target.as_str()) {
                result.add_error(ValidationError::FlowError {
                    from_task: flow.source.clone(),
                    to_task: flow.target.clone(),
                    message: format!("Target task '{}' not found", flow.target),
                });
            }

            // Check self-loop
            if flow.source == flow.target {
                result.add_error(ValidationError::FlowError {
                    from_task: flow.source.clone(),
                    to_task: flow.target.clone(),
                    message: "Self-loop not allowed".into(),
                });
            }
        }
    }

    // ========== Layer 4: Connections ==========

    fn validate_connections(
        &self,
        workflow: &Workflow,
        task_map: &HashMap<&str, &Task>,
        result: &mut ValidationResult,
    ) {
        for flow in &workflow.flows {
            if let (Some(source_task), Some(target_task)) = (
                task_map.get(flow.source.as_str()),
                task_map.get(flow.target.as_str()),
            ) {
                let source_key = source_task.connection_key();
                let target_key = target_task.connection_key();

                if !is_connection_allowed(&source_key, &target_key) {
                    result.add_error(ValidationError::ConnectionBlocked {
                        from_task: flow.source.clone(),
                        from_key: source_key,
                        to_task: flow.target.clone(),
                        to_key: target_key,
                    });
                }
            }
        }
    }

    // ========== Layer 5: Graph ==========

    fn validate_graph(&self, workflow: &Workflow, result: &mut ValidationResult) {
        // Collect all task IDs in flows
        let tasks_in_flows: HashSet<&str> = workflow
            .flows
            .iter()
            .flat_map(|f| vec![f.source.as_str(), f.target.as_str()])
            .collect();

        // Check for orphan tasks (only if more than 1 task)
        if workflow.tasks.len() > 1 {
            for task in &workflow.tasks {
                if !tasks_in_flows.contains(task.id.as_str()) {
                    result.add_error(ValidationError::GraphWarning {
                        message: format!("Orphan task: '{}' has no connections", task.id),
                    });
                }
            }
        }

        // Check for cycles (simple detection)
        if let Some(cycle) = self.detect_cycle(workflow) {
            result.add_error(ValidationError::GraphWarning {
                message: format!("Cycle detected: {}", cycle.join(" → ")),
            });
        }
    }

    fn detect_cycle(&self, workflow: &Workflow) -> Option<Vec<String>> {
        // Build adjacency list
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for flow in &workflow.flows {
            adj.entry(flow.source.as_str())
                .or_default()
                .push(flow.target.as_str());
        }

        // DFS cycle detection
        let mut visited: HashSet<&str> = HashSet::new();
        let mut rec_stack: HashSet<&str> = HashSet::new();
        let mut path: Vec<&str> = Vec::new();

        for task in &workflow.tasks {
            if self.dfs_cycle(&task.id, &adj, &mut visited, &mut rec_stack, &mut path) {
                return Some(path.iter().map(|s| s.to_string()).collect());
            }
        }

        None
    }

    fn dfs_cycle<'a>(
        &self,
        node: &'a str,
        adj: &HashMap<&str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        rec_stack: &mut HashSet<&'a str>,
        path: &mut Vec<&'a str>,
    ) -> bool {
        if rec_stack.contains(node) {
            path.push(node);
            return true;
        }

        if visited.contains(node) {
            return false;
        }

        visited.insert(node);
        rec_stack.insert(node);
        path.push(node);

        if let Some(neighbors) = adj.get(node) {
            for &neighbor in neighbors {
                if self.dfs_cycle(neighbor, adj, visited, rec_stack, path) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        path.pop();
        false
    }

    /// Validate a workflow file from path
    pub fn validate_file(&self, path: &std::path::Path) -> anyhow::Result<ValidationResult> {
        use anyhow::Context;

        let yaml = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read workflow file {:?}", path))?;

        let workflow: Workflow = serde_yaml::from_str(&yaml)
            .with_context(|| format!("Failed to parse workflow YAML from {:?}", path))?;

        let file_path = path.to_string_lossy().to_string();
        Ok(self.validate(&workflow, &file_path))
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_yaml(yaml: &str) -> ValidationResult {
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        Validator::new().validate(&workflow, "test.nika.yaml")
    }

    // ========== Connection Matrix Tests (9 rules) ==========

    #[test]
    fn test_agent_main_to_agent_main() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    type: agent
    prompt: "A"
  - id: b
    type: agent
    prompt: "B"
flows:
  - source: a
    target: b
"#,
        );
        assert!(result.is_valid(), "agent(main) → agent(main) should be valid");
    }

    #[test]
    fn test_agent_main_to_agent_isolated() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: main
    type: agent
    prompt: "Main"
  - id: sub
    type: agent
    scope: isolated
    prompt: "Sub"
flows:
  - source: main
    target: sub
"#,
        );
        assert!(
            result.is_valid(),
            "agent(main) → agent(isolated) should be valid"
        );
    }

    #[test]
    fn test_agent_main_to_action() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: analyze
    type: agent
    prompt: "Analyze"
  - id: save
    type: action
    run: Write
    file: "output.txt"
flows:
  - source: analyze
    target: save
"#,
        );
        assert!(result.is_valid(), "agent(main) → action should be valid");
    }

    #[test]
    fn test_agent_isolated_to_agent_main_blocked() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    type: agent
    scope: isolated
    prompt: "Work"
  - id: router
    type: agent
    prompt: "Route"
flows:
  - source: worker
    target: router
"#,
        );
        assert!(
            !result.is_valid(),
            "agent(isolated) → agent(main) should be BLOCKED"
        );
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ConnectionBlocked { .. }
        )));
    }

    #[test]
    fn test_agent_isolated_to_agent_isolated_blocked() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: sub1
    type: agent
    scope: isolated
    prompt: "Sub1"
  - id: sub2
    type: agent
    scope: isolated
    prompt: "Sub2"
flows:
  - source: sub1
    target: sub2
"#,
        );
        assert!(
            !result.is_valid(),
            "agent(isolated) → agent(isolated) should be BLOCKED"
        );
    }

    #[test]
    fn test_agent_isolated_to_action_valid() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    type: agent
    scope: isolated
    prompt: "Work"
  - id: collect
    type: action
    run: aggregate
flows:
  - source: worker
    target: collect
"#,
        );
        assert!(
            result.is_valid(),
            "agent(isolated) → action should be valid (BRIDGE)"
        );
    }

    #[test]
    fn test_action_to_agent_main() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: read
    type: action
    run: Read
    file: "input.txt"
  - id: analyze
    type: agent
    prompt: "Analyze"
flows:
  - source: read
    target: analyze
"#,
        );
        assert!(result.is_valid(), "action → agent(main) should be valid");
    }

    #[test]
    fn test_action_to_agent_isolated() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: read
    type: action
    run: Read
    file: "input.txt"
  - id: worker
    type: agent
    scope: isolated
    prompt: "Work"
flows:
  - source: read
    target: worker
"#,
        );
        assert!(
            result.is_valid(),
            "action → agent(isolated) should be valid"
        );
    }

    #[test]
    fn test_action_to_action() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: read
    type: action
    run: Read
    file: "input.txt"
  - id: transform
    type: action
    run: transform
flows:
  - source: read
    target: transform
"#,
        );
        assert!(result.is_valid(), "action → action should be valid");
    }

    // ========== Bridge Pattern Test ==========

    #[test]
    fn test_bridge_pattern() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    type: agent
    scope: isolated
    prompt: "Work"
  - id: bridge
    type: action
    run: aggregate
  - id: router
    type: agent
    prompt: "Route"
flows:
  - source: worker
    target: bridge
  - source: bridge
    target: router
"#,
        );
        assert!(
            result.is_valid(),
            "Bridge pattern (isolated → action → main) should be valid"
        );
    }

    // ========== Task Validation Tests ==========

    #[test]
    fn test_agent_missing_prompt() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-agent
    type: agent
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("prompt")
        )));
    }

    #[test]
    fn test_action_missing_run() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-action
    type: action
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("run")
        )));
    }

    #[test]
    fn test_action_with_scope_invalid() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-action
    type: action
    run: Read
    scope: isolated
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("scope")
        )));
    }

    // ========== Flow Validation Tests ==========

    #[test]
    fn test_flow_missing_source() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    type: agent
    prompt: "A"
flows:
  - source: nonexistent
    target: a
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::FlowError { message, .. } if message.contains("not found")
        )));
    }

    #[test]
    fn test_flow_self_loop() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    type: agent
    prompt: "A"
flows:
  - source: a
    target: a
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::FlowError { message, .. } if message.contains("Self-loop")
        )));
    }

    // ========== Graph Tests ==========

    #[test]
    fn test_orphan_task_warning() {
        let result = validate_yaml(
            r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: connected
    type: agent
    prompt: "Connected"
  - id: orphan
    type: agent
    prompt: "Orphan"
flows: []
"#,
        );
        assert!(result.is_valid()); // Warnings don't fail validation
        assert!(result.has_warnings());
    }
}
