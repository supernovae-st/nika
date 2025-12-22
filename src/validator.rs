//! # Nika Validator (v4.7.1)
//!
//! 5-layer validation pipeline for `.nika.yaml` workflows.
//!
//! ## Overview
//!
//! The validator performs comprehensive checks on workflow files:
//!
//! 1. **Layer 1: Schema** - Required fields (model, systemPrompt)
//! 2. **Layer 2: Tasks** - ID format, keyword presence, keyword-specific rules
//! 3. **Layer 2.5: Tool Access** - allowedTools/disallowedTools validation
//! 4. **Layer 3: Flows** - Source/target existence, self-loop detection
//! 5. **Layer 4: Connections** - Connection matrix (subagent restrictions)
//! 6. **Layer 5: Graph** - Orphan tasks, cycle detection (warnings)
//!
//! ## Error Codes
//!
//! All errors use standardized NIKA-XXX codes for easy troubleshooting:
//!
//! | Code | Layer | Description |
//! |------|-------|-------------|
//! | NIKA-001 | L1 | Missing agent.model |
//! | NIKA-002 | L1 | Missing systemPrompt |
//! | NIKA-010 | L2 | Task validation error |
//! | NIKA-011 | L2 | Duplicate task ID |
//! | NIKA-015 | L2.5 | Tool access violation |
//! | NIKA-020 | L3 | Flow validation error |
//! | NIKA-030 | L4 | Connection blocked |
//! | NIKA-040 | L5 | Graph warning |
//!
//! ## Connection Matrix (v4.7.1)
//!
//! Only 1 connection is blocked (v4.7.1 change):
//!
//! - `subagent: → subagent:` - Subagent cannot spawn another subagent
//!
//! v4.7.1: `subagent: → agent:` is NOW ALLOWED (WorkflowRunner auto-writes output)
//! Bridge pattern is OPTIONAL (only for output transformation):
//! `subagent: → function: → agent:` (optional)
//!
//! ## Example
//!
//! ```rust
//! use nika::{Workflow, Validator, ValidationError};
//!
//! // v4.7.1: subagent → agent is allowed
//! let yaml = r#"
//! agent:
//!   model: claude-sonnet-4-5
//!   systemPrompt: "Test"
//! tasks:
//!   - id: worker
//!     subagent:
//!       prompt: "Work"
//!   - id: router
//!     agent:
//!       prompt: "Route"
//! flows:
//!   - source: worker
//!     target: router
//! "#;
//!
//! let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
//! let result = Validator::new().validate(&workflow, "test.nika.yaml");
//!
//! // v4.7.1: This passes! subagent → agent is allowed (WorkflowRunner auto-writes)
//! assert!(result.is_valid());
//! ```

use crate::workflow::Workflow;
use crate::{Task, TaskAction, TaskCategory, TaskKeyword};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

// ============================================================================
// ERROR CODES
// ============================================================================

// Layer 1: Schema errors (001-009)
const CODE_MISSING_MODEL: &str = "NIKA-001";
const CODE_MISSING_SYSTEM_PROMPT: &str = "NIKA-002";

// Layer 2: Task errors (010-019)
const CODE_TASK_ERROR: &str = "NIKA-010";
const CODE_DUPLICATE_TASK_ID: &str = "NIKA-011";

// Layer 2.5: Tool access errors (015-019)
const CODE_TOOL_ACCESS: &str = "NIKA-015";

// Layer 3: Flow errors (020-029)
const CODE_FLOW_ERROR: &str = "NIKA-020";

// Layer 4: Connection errors (030-039)
const CODE_CONNECTION_BLOCKED: &str = "NIKA-030";

// Layer 5: Graph warnings (040-049)
const CODE_GRAPH_WARNING: &str = "NIKA-040";

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Error, Debug)]
pub enum ValidationError {
    // Layer 1: Schema
    #[error("[{CODE_MISSING_MODEL}] Missing agent.model")]
    MissingModel,

    #[error("[{CODE_MISSING_SYSTEM_PROMPT}] Missing agent.systemPrompt or systemPromptFile")]
    MissingSystemPrompt,

    // Layer 2: Tasks
    #[error("[{CODE_TASK_ERROR}] Task '{task_id}': {message}")]
    TaskError { task_id: String, message: String },

    #[error("[{CODE_DUPLICATE_TASK_ID}] Duplicate task ID: '{task_id}'")]
    DuplicateTaskId { task_id: String },

    // Layer 2.5: Tool Access
    #[error("[{CODE_TOOL_ACCESS}] Task '{task_id}': {message}")]
    ToolAccessError { task_id: String, message: String },

    // Layer 3: Flows
    #[error("[{CODE_FLOW_ERROR}] Flow '{from_task}' → '{to_task}': {message}")]
    FlowError {
        from_task: String,
        to_task: String,
        message: String,
    },

    // Layer 4: Connections
    #[error("[{CODE_CONNECTION_BLOCKED}] Connection blocked: {from_task} ({from_key}) → {to_task} ({to_key})")]
    ConnectionBlocked {
        from_task: String,
        from_key: TaskCategory,
        to_task: String,
        to_key: TaskCategory,
    },

    // Layer 5: Graph (warnings)
    #[error("[{CODE_GRAPH_WARNING}] Warning: {message}")]
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
// CONNECTION MATRIX (v4)
// ============================================================================

/// Check if a connection is allowed (v4.7.1 rules)
///
/// v4.7.1 Change: Only 1 connection is blocked - everything else is allowed:
/// - Isolated → Isolated: subagent cannot spawn another subagent
///
/// v4.7.1: subagent → agent is NOW ALLOWED (WorkflowRunner auto-writes output)
/// Bridge pattern is OPTIONAL (only needed for output transformation)
pub fn is_connection_allowed(source: TaskCategory, target: TaskCategory) -> bool {
    // v4.7.1: Only subagent → subagent is blocked
    // subagent → agent is now allowed (WorkflowRunner auto-writes)
    !matches!(
        (source, target),
        (TaskCategory::Isolated, TaskCategory::Isolated)
    )
}

/// Generate fix suggestion for blocked connections (v4.7.1)
///
/// v4.7.1: Only subagent → subagent is blocked
pub fn bridge_suggestion(source: &Task, target: &Task) -> String {
    let source_key = source.connection_key();
    let target_key = target.connection_key();

    match (source_key, target_key) {
        (TaskCategory::Isolated, TaskCategory::Isolated) => {
            format!(
                "\n   💡 subagent: cannot directly spawn another subagent:.\n      \
                 Route through agent: (Main Agent):\n      \
                 {} → function: → agent: → {}",
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

        // Layer 2.5: Tool Access (NEW!)
        self.validate_tool_access(workflow, &mut result);

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
        if workflow.agent.model.is_empty() {
            result.add_error(ValidationError::MissingModel);
        }

        if workflow.agent.system_prompt.is_none() && workflow.agent.system_prompt_file.is_none() {
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

            // v4.7.1: Validate keyword presence (exactly one required)
            let keyword_count = task.keyword_count();
            if keyword_count == 0 {
                result.add_error(ValidationError::TaskError {
                    task_id: task.id.clone(),
                    message: "Task must have exactly one keyword (agent, subagent, shell, http, mcp, function, or llm)".into(),
                });
            } else if keyword_count > 1 {
                result.add_error(ValidationError::TaskError {
                    task_id: task.id.clone(),
                    message: format!(
                        "Task has {} keywords but must have exactly one",
                        keyword_count
                    ),
                });
            }

            // Keyword-specific validation
            use crate::task::TaskAction;
            match &task.action {
                TaskAction::Agent { .. } | TaskAction::Subagent { .. } => {
                    // Agent keywords are valid (agent/subagent value is the instruction)
                }
                TaskAction::Mcp { mcp } => {
                    // mcp: must use :: separator
                    if !mcp.reference.contains("::") {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "mcp: must use '::' separator (e.g., 'filesystem::read_file')"
                                .into(),
                        });
                    }
                }
                TaskAction::Function { function } => {
                    // function: must use :: separator
                    if !function.reference.contains("::") {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message:
                                "function: must use '::' separator (e.g., 'path::functionName')"
                                    .into(),
                        });
                    }
                }
                TaskAction::Http { http } => {
                    // http: should be a valid URL pattern
                    if !http.url.starts_with("http://")
                        && !http.url.starts_with("https://")
                        && !http.url.starts_with("${")
                    {
                        result.add_error(ValidationError::TaskError {
                            task_id: task.id.clone(),
                            message: "http: must be a valid URL (http:// or https://)".into(),
                        });
                    }
                }
                _ => {
                    // shell, llm - no special validation needed
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

    // ========== Layer 2.5: Tool Access ==========

    /// Validate tool access rules (v4.7.1)
    ///
    /// Rules:
    /// 1. agent: tasks can only use tools from the config pool (subset restriction)
    /// 2. subagent: tasks can have independent tool access (sandboxed)
    /// 3. tool tasks (shell, http, mcp, function, llm) cannot have allowedTools
    /// 4. allowedTools and disallowedTools cannot overlap
    fn validate_tool_access(&self, workflow: &Workflow, result: &mut ValidationResult) {
        // Get the config-level allowed tools (the pool)
        let config_tools: HashSet<&str> = workflow
            .agent
            .allowed_tools
            .as_ref()
            .map(|v| v.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        let has_config_pool = workflow.agent.allowed_tools.is_some();

        for task in &workflow.tasks {
            let keyword = task.keyword();

            match keyword {
                // Rule 1: agent: tasks can only RESTRICT from config pool
                TaskKeyword::Agent => {
                    if let TaskAction::Agent { agent } = &task.action {
                        if let Some(ref task_tools) = agent.allowed_tools {
                            // If config has a pool, agent tasks must use subset
                            if has_config_pool {
                                for tool in task_tools {
                                    if !config_tools.contains(tool.as_str()) {
                                        result.add_error(ValidationError::ToolAccessError {
                                            task_id: task.id.clone(),
                                            message: format!(
                                                "'{}' not in agent.allowedTools pool. \
                                                agent: tasks can only restrict, not expand tool access",
                                                tool
                                            ),
                                        });
                                    }
                                }
                            }
                        }
                        // If config has no pool, agent tasks defining tools is fine
                        // (they're defining their own restrictions)

                        // Check allowedTools and disallowedTools don't overlap
                        self.check_tool_overlap_agent(agent, &task.id, result);
                    }
                }

                // Rule 2: subagent: tasks can have independent tool access (OK)
                TaskKeyword::Subagent => {
                    if let TaskAction::Subagent { subagent } = &task.action {
                        // Subagent can have any tools - it's sandboxed
                        // Just check for overlap
                        self.check_tool_overlap_subagent(subagent, &task.id, result);
                    }
                }

                // Rule 3: tool tasks cannot have allowedTools/disallowedTools
                TaskKeyword::Shell
                | TaskKeyword::Http
                | TaskKeyword::Mcp
                | TaskKeyword::Function
                | TaskKeyword::Llm => {
                    // Tools cannot have allowed_tools - this is enforced at the structure level
                    // since tool definitions don't have allowed_tools field
                }
            }
        }

        // Check config-level overlap
        if let (Some(ref allowed), Some(ref disallowed)) = (
            &workflow.agent.allowed_tools,
            &workflow.agent.disallowed_tools,
        ) {
            let allowed_set: HashSet<&str> = allowed.iter().map(|s| s.as_str()).collect();
            let disallowed_set: HashSet<&str> = disallowed.iter().map(|s| s.as_str()).collect();

            let overlap: Vec<&str> = allowed_set.intersection(&disallowed_set).copied().collect();
            if !overlap.is_empty() {
                result.add_error(ValidationError::ToolAccessError {
                    task_id: "agent".to_string(),
                    message: format!(
                        "agent.allowedTools and agent.disallowedTools overlap: [{}]",
                        overlap.join(", ")
                    ),
                });
            }
        }
    }

    /// Check that allowedTools and disallowedTools don't overlap on a task
    fn check_tool_overlap_agent(
        &self,
        agent: &crate::task::AgentDef,
        task_id: &str,
        result: &mut ValidationResult,
    ) {
        if let Some(ref tools) = agent.allowed_tools {
            // Check for duplicates within allowedTools
            let mut seen: HashSet<&str> = HashSet::new();
            for tool in tools {
                if !seen.insert(tool.as_str()) {
                    result.add_error(ValidationError::ToolAccessError {
                        task_id: task_id.to_string(),
                        message: format!("Duplicate tool in allowedTools: '{}'", tool),
                    });
                }
            }

            // Check for empty tool names
            for tool in tools {
                if tool.trim().is_empty() {
                    result.add_error(ValidationError::ToolAccessError {
                        task_id: task_id.to_string(),
                        message: "Empty tool name in allowedTools".to_string(),
                    });
                }
            }
        }
    }

    fn check_tool_overlap_subagent(
        &self,
        subagent: &crate::task::SubagentDef,
        task_id: &str,
        result: &mut ValidationResult,
    ) {
        if let Some(ref tools) = subagent.allowed_tools {
            // Check for duplicates within allowedTools
            let mut seen: HashSet<&str> = HashSet::new();
            for tool in tools {
                if !seen.insert(tool.as_str()) {
                    result.add_error(ValidationError::ToolAccessError {
                        task_id: task_id.to_string(),
                        message: format!("Duplicate tool in allowedTools: '{}'", tool),
                    });
                }
            }

            // Check for empty tool names
            for tool in tools {
                if tool.trim().is_empty() {
                    result.add_error(ValidationError::ToolAccessError {
                        task_id: task_id.to_string(),
                        message: "Empty tool name in allowedTools".to_string(),
                    });
                }
            }
        }
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

                if !is_connection_allowed(source_key, target_key) {
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
// TESTS (v4.7.1 - keyword syntax)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_yaml(yaml: &str) -> ValidationResult {
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        Validator::new().validate(&workflow, "test.nika.yaml")
    }

    // ========== Connection Matrix Tests (9 rules) - v4.7.1 ==========

    #[test]
    fn test_agent_to_agent() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    agent:
      prompt: "A"
  - id: b
    agent:
      prompt: "B"
flows:
  - source: a
    target: b
"#,
        );
        assert!(result.is_valid(), "agent: → agent: should be valid");
    }

    #[test]
    fn test_agent_to_subagent() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: main
    agent:
      prompt: "Main"
  - id: sub
    subagent:
      prompt: "Sub"
flows:
  - source: main
    target: sub
"#,
        );
        assert!(result.is_valid(), "agent: → subagent: should be valid");
    }

    #[test]
    fn test_agent_to_tool() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: analyze
    agent:
      prompt: "Analyze"
  - id: save
    shell:
      command: "echo done"
flows:
  - source: analyze
    target: save
"#,
        );
        assert!(result.is_valid(), "agent: → tool should be valid");
    }

    #[test]
    fn test_subagent_to_agent_allowed_v471() {
        // v4.7.1: subagent → agent is NOW ALLOWED (WorkflowRunner auto-writes output)
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    subagent:
      prompt: "Work"
  - id: router
    agent:
      prompt: "Route"
flows:
  - source: worker
    target: router
"#,
        );
        assert!(
            result.is_valid(),
            "v4.7.1: subagent: → agent: should be ALLOWED (WorkflowRunner auto-writes)"
        );
    }

    #[test]
    fn test_subagent_to_subagent_blocked() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: sub1
    subagent:
      prompt: "Sub1"
  - id: sub2
    subagent:
      prompt: "Sub2"
flows:
  - source: sub1
    target: sub2
"#,
        );
        assert!(
            !result.is_valid(),
            "subagent: → subagent: should be BLOCKED"
        );
    }

    #[test]
    fn test_subagent_to_tool_valid() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    subagent:
      prompt: "Work"
  - id: collect
    function:

      reference: "aggregate::collect"
flows:
  - source: worker
    target: collect
"#,
        );
        assert!(
            result.is_valid(),
            "subagent: → tool should be valid (BRIDGE)"
        );
    }

    #[test]
    fn test_tool_to_agent() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: read
    mcp:
      reference: "filesystem::read_file"
  - id: analyze
    agent:
      prompt: "Analyze"
flows:
  - source: read
    target: analyze
"#,
        );
        assert!(result.is_valid(), "tool → agent: should be valid");
    }

    #[test]
    fn test_tool_to_subagent() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: read
    mcp:
      reference: "filesystem::read_file"
  - id: worker
    subagent:
      prompt: "Work"
flows:
  - source: read
    target: worker
"#,
        );
        assert!(result.is_valid(), "tool → subagent: should be valid");
    }

    #[test]
    fn test_tool_to_tool() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: read
    mcp:
      reference: "filesystem::read_file"
  - id: transform
    function:

      reference: "transform::json"
flows:
  - source: read
    target: transform
"#,
        );
        assert!(result.is_valid(), "tool → tool should be valid");
    }

    // ========== Bridge Pattern Test (v4.7.1) ==========

    #[test]
    fn test_bridge_pattern_optional_v471() {
        // v4.7.1: Bridge pattern is OPTIONAL (only for output transformation)
        // Direct subagent → agent is now allowed, but bridge still works
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    subagent:
      prompt: "Work"
  - id: bridge
    function:

      reference: "aggregate::collect"
  - id: router
    agent:
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
            "Bridge pattern (subagent: → function: → agent:) should still be valid in v4.7.1"
        );
    }

    // ========== Keyword Validation Tests (v4.7.1) ==========

    #[test]
    fn test_missing_keyword_fails_parsing() {
        // In v4.7.1, a task without a keyword fails at parse time (not validation)
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-task
flows: []
"#;
        let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "Task without keyword should fail to parse");
    }

    #[test]
    fn test_mcp_missing_separator() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-mcp
    mcp:
      reference: "filesystem_read_file"
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("::")
        )));
    }

    #[test]
    fn test_function_missing_separator() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-func
    function:
      reference: "transform_json"
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("::")
        )));
    }

    #[test]
    fn test_http_invalid_url() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-http
    http:
      url: "not-a-url"
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("URL")
        )));
    }

    #[test]
    fn test_http_with_variable_valid() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: webhook
    http:
      url: "${secrets.WEBHOOK_URL}"
flows: []
"#,
        );
        assert!(result.is_valid(), "http: with variable should be valid");
    }

    // ========== Flow Validation Tests ==========

    #[test]
    fn test_flow_missing_source() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    agent:
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
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    agent:
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
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: connected
    agent:
      prompt: "Connected"
  - id: orphan
    agent:
      prompt: "Orphan"
flows: []
"#,
        );
        assert!(result.is_valid()); // Warnings don't fail validation
        assert!(result.has_warnings());
    }

    // ========== All 7 Keywords Valid ==========

    #[test]
    fn test_all_7_keywords_valid() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: t1
    agent:
      prompt: "Main agent"
  - id: t2
    subagent:
      prompt: "Subagent"
  - id: t3
    shell:
      command: "npm test"
  - id: t4
    http:
      url: "https://api.example.com"
  - id: t5
    mcp:
      reference: "filesystem::read"
  - id: t6
    function:
      reference: "tools::transform"
  - id: t7
    llm:
      prompt: "Classify this"
flows: []
"#,
        );
        // Should only have orphan warnings, no errors
        assert!(result.is_valid(), "All 7 keywords should be valid");
    }

    // ========== Layer 2.5: Tool Access Tests ==========

    #[test]
    fn test_agent_task_subset_of_config_valid() {
        // agent: task uses subset of config tools - should pass
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  allowedTools: [Read, Write, Glob, Grep]
tasks:
  - id: reader
    agent:
      prompt: "Read files"
    allowedTools: [Read, Glob]  # Subset of config
flows: []
"#,
        );
        assert!(
            result.is_valid(),
            "agent: with subset of config tools should be valid"
        );
    }

    #[test]
    fn test_agent_task_not_in_pool_error() {
        // agent: task uses tool not in config pool - should fail
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  allowedTools: [Read, Write]
tasks:
  - id: hacker
    agent:
      prompt: "Do stuff"
      allowedTools: [Read, Bash]  # Bash not in pool!
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ToolAccessError { message, .. }
            if message.contains("Bash") && message.contains("not in agent.allowedTools")
        )));
    }

    #[test]
    fn test_agent_task_no_config_pool_ok() {
        // agent: task defines tools when config has no pool - should pass
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  # No allowedTools at config level
tasks:
  - id: worker
    agent:
      prompt: "Work"
    allowedTools: [Read, Write, Bash]  # Fine - no pool to restrict
flows: []
"#,
        );
        assert!(
            result.is_valid(),
            "agent: can define tools when config has no pool"
        );
    }

    #[test]
    fn test_subagent_independent_tools_ok() {
        // subagent: can have any tools - sandboxed
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  allowedTools: [Read]  # Limited config
tasks:
  - id: researcher
    subagent:
      prompt: "Deep research"
    allowedTools: [Read, Write, WebSearch, Bash]  # Completely different - OK!
flows: []
"#,
        );
        assert!(
            result.is_valid(),
            "subagent: can have independent tool access"
        );
    }

    // NOTE: In v4.7.1, tool tasks (shell/mcp/function/http/llm) CANNOT have allowedTools
    // structurally - the field only exists in AgentDef and SubagentDef.
    // Unknown fields are ignored by serde, so these tests are removed.

    #[test]
    fn test_config_allowed_disallowed_overlap_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  allowedTools: [Read, Write, Bash]
  disallowedTools: [Bash, Execute]  # Bash overlaps!
tasks:
  - id: work
    agent:
      prompt: "Work"
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ToolAccessError { message, .. }
            if message.contains("overlap") && message.contains("Bash")
        )));
    }

    #[test]
    fn test_duplicate_tool_in_allowed_tools_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: work
    agent:
      prompt: "Work"
      allowedTools: [Read, Write, Read]  # Duplicate Read!
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ToolAccessError { message, .. }
            if message.contains("Duplicate") && message.contains("Read")
        )));
    }

    #[test]
    fn test_empty_tool_name_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: work
    agent:
      prompt: "Work"
      allowedTools: [Read, "", Write]  # Empty tool name!
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ToolAccessError { message, .. }
            if message.contains("Empty tool name")
        )));
    }

    #[test]
    fn test_complex_tool_access_valid() {
        // Complex scenario: config pool, agent restricts, subagent independent
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Orchestrator"
  allowedTools: [Read, Write, Glob, Grep]
tasks:
  - id: analyze
    agent:
      prompt: "Analyze code"
    allowedTools: [Read, Grep]  # Subset - OK

  - id: deep-research
    subagent:
      prompt: "Deep security audit"
    allowedTools: [Read, WebSearch, Bash]  # Independent - OK

  - id: save
    function:
      reference: "output::save"
    # No allowedTools - OK for tool task

  - id: notify
    http:
      url: "https://webhook.example.com"
    # No allowedTools - OK for tool task

flows:
  - source: analyze
    target: deep-research
  - source: deep-research
    target: save
  - source: save
    target: notify
"#,
        );
        assert!(
            result.is_valid(),
            "Complex tool access scenario should pass"
        );
    }

    // ==========================================================================
    // ERROR MESSAGE FORMATTING TESTS
    // ==========================================================================

    #[test]
    fn test_error_message_contains_code() {
        // All error messages should start with [NIKA-XXX]
        let error = ValidationError::MissingModel;
        let msg = error.to_string();
        assert!(
            msg.starts_with("[NIKA-001]"),
            "Error message should start with code: {}",
            msg
        );
    }

    #[test]
    fn test_error_message_missing_system_prompt() {
        let error = ValidationError::MissingSystemPrompt;
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-002]"));
        assert!(msg.contains("systemPrompt"));
    }

    #[test]
    fn test_error_message_task_error_format() {
        let error = ValidationError::TaskError {
            task_id: "my-task".to_string(),
            message: "Invalid configuration".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-010]"));
        assert!(msg.contains("my-task"));
        assert!(msg.contains("Invalid configuration"));
    }

    #[test]
    fn test_error_message_duplicate_id_format() {
        let error = ValidationError::DuplicateTaskId {
            task_id: "duplicate".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-011]"));
        assert!(msg.contains("duplicate"));
    }

    #[test]
    fn test_error_message_tool_access_format() {
        let error = ValidationError::ToolAccessError {
            task_id: "worker".to_string(),
            message: "Tool not allowed".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-015]"));
        assert!(msg.contains("worker"));
    }

    #[test]
    fn test_error_message_flow_error_format() {
        let error = ValidationError::FlowError {
            from_task: "source".to_string(),
            to_task: "target".to_string(),
            message: "Connection problem".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-020]"));
        assert!(msg.contains("source"));
        assert!(msg.contains("target"));
        assert!(msg.contains("→")); // Arrow symbol
    }

    #[test]
    fn test_error_message_connection_blocked_format() {
        let error = ValidationError::ConnectionBlocked {
            from_task: "worker".to_string(),
            from_key: TaskCategory::Isolated,
            to_task: "router".to_string(),
            to_key: TaskCategory::Context,
        };
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-030]"));
        assert!(msg.contains("worker"));
        assert!(msg.contains("router"));
        assert!(msg.contains("subagent:")); // from_key display
        assert!(msg.contains("agent:")); // to_key display
    }

    #[test]
    fn test_error_message_warning_format() {
        let error = ValidationError::GraphWarning {
            message: "Orphan task detected".to_string(),
        };
        let msg = error.to_string();
        assert!(msg.starts_with("[NIKA-040]"));
        assert!(msg.contains("Warning"));
        assert!(msg.contains("Orphan"));
    }

    #[test]
    fn test_validation_result_is_valid() {
        let mut result = ValidationResult::new("test.nika.yaml");
        assert!(result.is_valid()); // Empty = valid

        result.add_error(ValidationError::GraphWarning {
            message: "Just a warning".to_string(),
        });
        assert!(result.is_valid()); // Warnings don't invalidate

        result.add_error(ValidationError::MissingModel);
        assert!(!result.is_valid()); // Real error invalidates
    }

    #[test]
    fn test_validation_result_error_and_warning_counts() {
        let mut result = ValidationResult::new("test.nika.yaml");

        result.add_error(ValidationError::MissingModel);
        result.add_error(ValidationError::MissingSystemPrompt);
        result.add_error(ValidationError::GraphWarning {
            message: "W1".to_string(),
        });
        result.add_error(ValidationError::GraphWarning {
            message: "W2".to_string(),
        });
        result.add_error(ValidationError::GraphWarning {
            message: "W3".to_string(),
        });

        assert_eq!(result.error_count(), 2);
        assert_eq!(result.warning_count(), 3);
        assert_eq!(result.errors.len(), 5);
    }

    #[test]
    fn test_validation_result_has_warnings() {
        let mut result = ValidationResult::new("test.nika.yaml");
        assert!(!result.has_warnings());

        result.add_error(ValidationError::MissingModel);
        assert!(!result.has_warnings()); // Error is not a warning

        result.add_error(ValidationError::GraphWarning {
            message: "Test".to_string(),
        });
        assert!(result.has_warnings());
    }

    #[test]
    fn test_is_warning_method() {
        assert!(ValidationError::GraphWarning {
            message: "Test".to_string()
        }
        .is_warning());
        assert!(!ValidationError::MissingModel.is_warning());
        assert!(!ValidationError::MissingSystemPrompt.is_warning());
        assert!(!ValidationError::TaskError {
            task_id: "t".to_string(),
            message: "m".to_string()
        }
        .is_warning());
        assert!(!ValidationError::DuplicateTaskId {
            task_id: "t".to_string()
        }
        .is_warning());
        assert!(!ValidationError::ToolAccessError {
            task_id: "t".to_string(),
            message: "m".to_string()
        }
        .is_warning());
        assert!(!ValidationError::FlowError {
            from_task: "a".to_string(),
            to_task: "b".to_string(),
            message: "m".to_string()
        }
        .is_warning());
        assert!(!ValidationError::ConnectionBlocked {
            from_task: "a".to_string(),
            from_key: TaskCategory::Tool,
            to_task: "b".to_string(),
            to_key: TaskCategory::Tool
        }
        .is_warning());
    }

    #[test]
    fn test_bridge_suggestion_isolated_to_context_v471() {
        // v4.7.1: subagent → agent is NOW ALLOWED, so no bridge suggestion
        let source: Task = serde_yaml::from_str(
            r#"
id: worker
subagent:
  prompt: "Work"
"#,
        )
        .unwrap();
        let target: Task = serde_yaml::from_str(
            r#"
id: router
agent:
  prompt: "Route"
"#,
        )
        .unwrap();

        let suggestion = bridge_suggestion(&source, &target);
        // v4.7.1: No suggestion needed - connection is allowed
        assert!(
            suggestion.is_empty(),
            "v4.7.1: subagent → agent is allowed, no bridge suggestion needed"
        );
    }

    #[test]
    fn test_bridge_suggestion_isolated_to_isolated() {
        let source: Task = serde_yaml::from_str(
            r#"
id: sub1
subagent:
  prompt: "Sub1"
"#,
        )
        .unwrap();
        let target: Task = serde_yaml::from_str(
            r#"
id: sub2
subagent:
  prompt: "Sub2"
"#,
        )
        .unwrap();

        let suggestion = bridge_suggestion(&source, &target);
        assert!(suggestion.contains("💡"));
        assert!(suggestion.contains("cannot directly spawn"));
    }

    #[test]
    fn test_bridge_suggestion_allowed_returns_empty() {
        let source: Task = serde_yaml::from_str(
            r#"
id: reader
mcp:
  reference: "filesystem::read"
"#,
        )
        .unwrap();
        let target: Task = serde_yaml::from_str(
            r#"
id: analyzer
agent:
  prompt: "Analyze"
"#,
        )
        .unwrap();

        let suggestion = bridge_suggestion(&source, &target);
        assert!(
            suggestion.is_empty(),
            "Allowed connection should return empty suggestion"
        );
    }

    #[test]
    fn test_is_connection_allowed_all_combinations() {
        // Test all 9 combinations of the connection matrix (v4.7.1)
        use TaskCategory::*;

        // Context (agent:) can connect to anything
        assert!(is_connection_allowed(Context, Context));
        assert!(is_connection_allowed(Context, Isolated));
        assert!(is_connection_allowed(Context, Tool));

        // v4.7.1: Isolated (subagent:) can connect to Context and Tool, but NOT Isolated
        assert!(is_connection_allowed(Isolated, Context)); // v4.7.1: NOW ALLOWED (WorkflowRunner auto-writes)
        assert!(!is_connection_allowed(Isolated, Isolated)); // STILL BLOCKED (can't spawn from sub)
        assert!(is_connection_allowed(Isolated, Tool)); // OK

        // Tool can connect to anything
        assert!(is_connection_allowed(Tool, Context));
        assert!(is_connection_allowed(Tool, Isolated));
        assert!(is_connection_allowed(Tool, Tool));
    }

    #[test]
    fn test_validation_result_file_path() {
        let result = ValidationResult::new("workflows/my-flow.nika.yaml");
        assert_eq!(result.file_path, "workflows/my-flow.nika.yaml");
    }

    #[test]
    fn test_validation_result_task_and_flow_counts() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    agent:
      prompt: "A"
  - id: b
    agent:
      prompt: "B"
  - id: c
    agent:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
"#,
        );
        assert_eq!(result.task_count, 3);
        assert_eq!(result.flow_count, 2);
    }

    #[test]
    fn test_is_valid_id_edge_cases() {
        assert!(Validator::is_valid_id("task1"));
        assert!(Validator::is_valid_id("my-task"));
        assert!(Validator::is_valid_id("my_task"));
        assert!(Validator::is_valid_id("Task123"));
        assert!(Validator::is_valid_id("a"));
        assert!(Validator::is_valid_id("A"));

        // Invalid cases
        assert!(!Validator::is_valid_id("")); // Empty
        assert!(!Validator::is_valid_id("-task")); // Starts with hyphen
        assert!(!Validator::is_valid_id("_task")); // Starts with underscore
        assert!(!Validator::is_valid_id("task@1")); // Special char
        assert!(!Validator::is_valid_id("task 1")); // Space
        assert!(!Validator::is_valid_id("task.1")); // Dot
    }

    #[test]
    fn test_validator_default() {
        let validator = Validator;
        // Just verify it can be created via Default
        let result = validator.validate(
            &serde_yaml::from_str::<Workflow>(
                r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks: []
flows: []
"#,
            )
            .unwrap(),
            "test.yaml",
        );
        assert!(result.is_valid());
    }

    #[test]
    fn test_cycle_detection() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    agent:
      prompt: "A"
  - id: b
    agent:
      prompt: "B"
  - id: c
    agent:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#,
        );
        assert!(result.has_warnings());
        assert!(result.errors.iter().any(|e| {
            if let ValidationError::GraphWarning { message } = e {
                message.contains("Cycle")
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_multiple_errors_accumulated() {
        // Test that multiple errors are accumulated, not just the first one
        let result = validate_yaml(
            r#"
agent:
  model: ""
  # Missing systemPrompt
tasks:
  - id: a
    agent:
      prompt: "First task"
  - id: a
    agent:
      prompt: "Duplicate ID"
  - id: -bad-id
    agent:
      prompt: "Bad ID"
flows:
  - source: nonexistent
    target: also-nonexistent
"#,
        );

        // Should have multiple errors: empty model, duplicate ID, bad ID format, missing sources
        assert!(!result.is_valid());
        assert!(
            result.error_count() >= 3,
            "Expected at least 3 errors, got {}",
            result.error_count()
        );
    }

    #[test]
    fn test_empty_workflow_valid() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Empty but valid workflow"
tasks: []
flows: []
"#,
        );
        assert!(result.is_valid());
        assert_eq!(result.task_count, 0);
        assert_eq!(result.flow_count, 0);
    }

    #[test]
    fn test_single_task_no_orphan_warning() {
        // Single task should NOT trigger orphan warning
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Single task workflow"
tasks:
  - id: only-task
    agent:
      prompt: "Do the thing"
flows: []
"#,
        );
        assert!(result.is_valid());
        assert!(
            !result.has_warnings(),
            "Single task should not be orphan warning"
        );
    }
}
