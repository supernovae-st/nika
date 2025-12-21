//! Nika Validator (v4.5)
//!
//! 5-layer validation pipeline for .nika.yaml workflows.
//! Rules are embedded - no external YAML files needed.
//! Validates 7 keywords: agent, subagent, shell, http, mcp, function, llm.

use crate::workflow::{ConnectionKey, Task, TaskKeyword, Workflow};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Error, Debug)]
pub enum ValidationError {
    // Layer 1: Schema
    #[error("[L1] Missing agent.model")]
    MissingModel,

    #[error("[L1] Missing agent.systemPrompt or systemPromptFile")]
    MissingSystemPrompt,

    // Layer 2: Tasks
    #[error("[L2] Task '{task_id}': {message}")]
    TaskError { task_id: String, message: String },

    #[error("[L2] Duplicate task ID: '{task_id}'")]
    DuplicateTaskId { task_id: String },

    // Layer 2.5: Tool Access
    #[error("[L2.5] Task '{task_id}': {message}")]
    ToolAccessError { task_id: String, message: String },

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
// CONNECTION MATRIX (v4)
// ============================================================================

/// Check if a connection is allowed (v4.5 rules)
///
/// ```text
/// SOURCE              │ TARGET              │ OK?
/// ─────────────────────────────────────────────────
/// agent:              │ agent:              │ ✅  Context enrichment
/// agent:              │ subagent:           │ ✅  Spawn subagent
/// agent:              │ tool                │ ✅  Execute tool
/// subagent:           │ agent:              │ ❌  NEEDS BRIDGE
/// subagent:           │ subagent:           │ ❌  Can't spawn from sub
/// subagent:           │ tool                │ ✅  THIS IS THE BRIDGE
/// tool                │ agent:              │ ✅  Feed data to context
/// tool                │ subagent:           │ ✅  Trigger subagent
/// tool                │ tool                │ ✅  Chain tools
/// ```
pub fn is_connection_allowed(source: &ConnectionKey, target: &ConnectionKey) -> bool {
    use ConnectionKey::*;

    match (source, target) {
        // agent: can connect to anything
        (Agent, Agent) => true,
        (Agent, Subagent) => true,
        (Agent, Tool) => true,

        // subagent: can ONLY connect to tool (bridge)
        (Subagent, Agent) => false,
        (Subagent, Subagent) => false,
        (Subagent, Tool) => true,

        // tool can connect to anything
        (Tool, Agent) => true,
        (Tool, Subagent) => true,
        (Tool, Tool) => true,
    }
}

/// Generate fix suggestion for blocked connections (v4.5)
pub fn bridge_suggestion(source: &Task, target: &Task) -> String {
    let source_key = source.connection_key();
    let target_key = target.connection_key();

    match (&source_key, &target_key) {
        (ConnectionKey::Subagent, ConnectionKey::Agent) => {
            format!(
                "\n   💡 Add a tool between them (bridge pattern):\n      \
                 {} (subagent:) → [function:] → {} (agent:)",
                source.id, target.id
            )
        }
        (ConnectionKey::Subagent, ConnectionKey::Subagent) => {
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

        if workflow.agent.system_prompt.is_none()
            && workflow.agent.system_prompt_file.is_none()
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

            // v4.5: Validate keyword presence (exactly one required)
            let keyword_count = task.keyword_count();
            if keyword_count == 0 {
                result.add_error(ValidationError::TaskError {
                    task_id: task.id.clone(),
                    message: "Task must have exactly one keyword (agent, subagent, shell, http, mcp, function, or llm)".into(),
                });
            } else if keyword_count > 1 {
                result.add_error(ValidationError::TaskError {
                    task_id: task.id.clone(),
                    message: format!("Task has {} keywords but must have exactly one", keyword_count),
                });
            }

            // Keyword-specific validation
            if let Some(keyword) = task.keyword() {
                match keyword {
                    TaskKeyword::Agent | TaskKeyword::Subagent => {
                        // Agent keywords are valid (agent/subagent value is the instruction)
                    }
                    TaskKeyword::Mcp => {
                        // mcp: must use :: separator
                        if let Some(mcp) = &task.mcp {
                            if !mcp.contains("::") {
                                result.add_error(ValidationError::TaskError {
                                    task_id: task.id.clone(),
                                    message: "mcp: must use '::' separator (e.g., 'filesystem::read_file')".into(),
                                });
                            }
                        }
                    }
                    TaskKeyword::Function => {
                        // function: must use :: separator
                        if let Some(func) = &task.function {
                            if !func.contains("::") {
                                result.add_error(ValidationError::TaskError {
                                    task_id: task.id.clone(),
                                    message: "function: must use '::' separator (e.g., 'path::functionName')".into(),
                                });
                            }
                        }
                    }
                    TaskKeyword::Http => {
                        // http: should be a valid URL pattern
                        if let Some(url) = &task.http {
                            if !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("${") {
                                result.add_error(ValidationError::TaskError {
                                    task_id: task.id.clone(),
                                    message: "http: must be a valid URL (http:// or https://)".into(),
                                });
                            }
                        }
                    }
                    _ => {
                        // shell, llm - no special validation needed
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

    // ========== Layer 2.5: Tool Access ==========

    /// Validate tool access rules (v4.5)
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
                Some(TaskKeyword::Agent) => {
                    if let Some(ref task_tools) = task.allowed_tools {
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
                        // If config has no pool, agent tasks defining tools is fine
                        // (they're defining their own restrictions)
                    }

                    // Check allowedTools and disallowedTools don't overlap
                    self.check_tool_overlap(task, result);
                }

                // Rule 2: subagent: tasks can have independent tool access (OK)
                Some(TaskKeyword::Subagent) => {
                    // Subagent can have any tools - it's sandboxed
                    // Just check for overlap
                    self.check_tool_overlap(task, result);
                }

                // Rule 3: tool tasks cannot have allowedTools/disallowedTools
                Some(TaskKeyword::Shell)
                | Some(TaskKeyword::Http)
                | Some(TaskKeyword::Mcp)
                | Some(TaskKeyword::Function)
                | Some(TaskKeyword::Llm) => {
                    if task.allowed_tools.is_some() {
                        result.add_error(ValidationError::ToolAccessError {
                            task_id: task.id.clone(),
                            message: "Tool tasks cannot have 'allowedTools'. \
                                Only agent: and subagent: tasks can restrict tool access"
                                .to_string(),
                        });
                    }
                    // Note: We don't check disallowed_tools on task struct since
                    // the workflow.rs doesn't have that field per-task, only at config level
                }

                None => {
                    // Already handled in Layer 2
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
    fn check_tool_overlap(&self, task: &Task, result: &mut ValidationResult) {
        // Note: Task struct doesn't have disallowed_tools, but if it did:
        // We would check overlap here. For now, this is a placeholder
        // that validates the allowed_tools field exists and is reasonable.

        if let Some(ref tools) = task.allowed_tools {
            // Check for duplicates within allowedTools
            let mut seen: HashSet<&str> = HashSet::new();
            for tool in tools {
                if !seen.insert(tool.as_str()) {
                    result.add_error(ValidationError::ToolAccessError {
                        task_id: task.id.clone(),
                        message: format!("Duplicate tool in allowedTools: '{}'", tool),
                    });
                }
            }

            // Check for empty tool names
            for tool in tools {
                if tool.trim().is_empty() {
                    result.add_error(ValidationError::ToolAccessError {
                        task_id: task.id.clone(),
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
// TESTS (v4.5 - keyword syntax)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn validate_yaml(yaml: &str) -> ValidationResult {
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        Validator::new().validate(&workflow, "test.nika.yaml")
    }

    // ========== Connection Matrix Tests (9 rules) - v4.5 ==========

    #[test]
    fn test_agent_to_agent() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: a
    agent: "A"
  - id: b
    agent: "B"
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
    agent: "Main"
  - id: sub
    subagent: "Sub"
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
    agent: "Analyze"
  - id: save
    shell: "echo done"
flows:
  - source: analyze
    target: save
"#,
        );
        assert!(result.is_valid(), "agent: → tool should be valid");
    }

    #[test]
    fn test_subagent_to_agent_blocked() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    subagent: "Work"
  - id: router
    agent: "Route"
flows:
  - source: worker
    target: router
"#,
        );
        assert!(
            !result.is_valid(),
            "subagent: → agent: should be BLOCKED"
        );
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ConnectionBlocked { .. }
        )));
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
    subagent: "Sub1"
  - id: sub2
    subagent: "Sub2"
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
    subagent: "Work"
  - id: collect
    function: aggregate::collect
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
    mcp: "filesystem::read_file"
  - id: analyze
    agent: "Analyze"
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
    mcp: "filesystem::read_file"
  - id: worker
    subagent: "Work"
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
    mcp: "filesystem::read_file"
  - id: transform
    function: transform::json
flows:
  - source: read
    target: transform
"#,
        );
        assert!(result.is_valid(), "tool → tool should be valid");
    }

    // ========== Bridge Pattern Test (v4.5) ==========

    #[test]
    fn test_bridge_pattern_v45() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: worker
    subagent: "Work"
  - id: bridge
    function: aggregate::collect
  - id: router
    agent: "Route"
flows:
  - source: worker
    target: bridge
  - source: bridge
    target: router
"#,
        );
        assert!(
            result.is_valid(),
            "Bridge pattern (subagent: → function: → agent:) should be valid"
        );
    }

    // ========== Keyword Validation Tests (v4.5) ==========

    #[test]
    fn test_missing_keyword() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: bad-task
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::TaskError { message, .. } if message.contains("exactly one keyword")
        )));
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
    mcp: "filesystem_read_file"
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
    function: "transform_json"
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
    http: "not-a-url"
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
    http: "${secrets.WEBHOOK_URL}"
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
    agent: "A"
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
    agent: "A"
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
    agent: "Connected"
  - id: orphan
    agent: "Orphan"
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
    agent: "Main agent"
  - id: t2
    subagent: "Subagent"
  - id: t3
    shell: "npm test"
  - id: t4
    http: "https://api.example.com"
  - id: t5
    mcp: "filesystem::read"
  - id: t6
    function: "tools::transform"
  - id: t7
    llm: "Classify this"
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
    agent: "Read files"
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
    agent: "Do stuff"
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
    agent: "Work"
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
    subagent: "Deep research"
    allowedTools: [Read, Write, WebSearch, Bash]  # Completely different - OK!
flows: []
"#,
        );
        assert!(
            result.is_valid(),
            "subagent: can have independent tool access"
        );
    }

    #[test]
    fn test_tool_task_with_allowed_tools_error() {
        // shell: task with allowedTools - should fail
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: runner
    shell: "npm test"
    allowedTools: [Read]  # ERROR: tool tasks can't have this
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ToolAccessError { message, .. }
            if message.contains("Tool tasks cannot have")
        )));
    }

    #[test]
    fn test_mcp_task_with_allowed_tools_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: reader
    mcp: "filesystem::read"
    allowedTools: [Glob]  # ERROR
flows: []
"#,
        );
        assert!(!result.is_valid());
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::ToolAccessError { task_id, .. } if task_id == "reader"
        )));
    }

    #[test]
    fn test_function_task_with_allowed_tools_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: transform
    function: "utils::process"
    allowedTools: [Read]  # ERROR
flows: []
"#,
        );
        assert!(!result.is_valid());
    }

    #[test]
    fn test_http_task_with_allowed_tools_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: webhook
    http: "https://api.example.com"
    allowedTools: [Read]  # ERROR
flows: []
"#,
        );
        assert!(!result.is_valid());
    }

    #[test]
    fn test_llm_task_with_allowed_tools_error() {
        let result = validate_yaml(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: classify
    llm: "Classify this"
    allowedTools: [Read]  # ERROR
flows: []
"#,
        );
        assert!(!result.is_valid());
    }

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
    agent: "Work"
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
    agent: "Work"
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
    agent: "Work"
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
    agent: "Analyze code"
    allowedTools: [Read, Grep]  # Subset - OK

  - id: deep-research
    subagent: "Deep security audit"
    allowedTools: [Read, WebSearch, Bash]  # Independent - OK

  - id: save
    function: "output::save"
    # No allowedTools - OK for tool task

  - id: notify
    http: "https://webhook.example.com"
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
        assert!(result.is_valid(), "Complex tool access scenario should pass");
    }
}
