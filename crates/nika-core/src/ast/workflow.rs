//! Workflow Types - main workflow structure (v0.1 - v0.5)
//!
//! Contains the core YAML-parsed types:
//! - `Workflow`: Root workflow with tasks and flows
//! - `Task`: Individual task definition
//! - `Flow`: DAG edge between tasks
//! - `FlowEndpoint`: Single or multiple task references
//! - `McpConfigInline`: Inline MCP server configuration (v0.2+)

use rustc_hash::FxHashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::binding::WiringSpec;
use crate::error::NikaError;

use super::action::TaskAction;
use super::decompose::DecomposeSpec;
use super::output::OutputPolicy;

/// Expected schema version for v0.1 workflows
pub const SCHEMA_V01: &str = "nika/workflow@0.1";

/// Expected schema version for v0.2 workflows
pub const SCHEMA_V02: &str = "nika/workflow@0.2";

/// Expected schema version for v0.3 workflows (for_each parallelism)
pub const SCHEMA_V03: &str = "nika/workflow@0.3";

/// Expected schema version for v0.4 workflows (extended thinking)
pub const SCHEMA_V04: &str = "nika/workflow@0.4";

/// Expected schema version for v0.5 workflows (decompose, lazy bindings, spawn_agent)
pub const SCHEMA_V05: &str = "nika/workflow@0.5";

/// Inline MCP server configuration (v0.2)
///
/// Allows workflows to define MCP servers directly in YAML.
/// The server name is the map key in the `mcp` field.
///
/// # Example
///
/// ```yaml
/// mcp:
///   novanet:
///     command: cargo
///     args: [run, -p, novanet-mcp]
///     env:
///       NEO4J_URI: bolt://localhost:7687
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct McpConfigInline {
    /// Command to spawn the MCP server
    pub command: String,
    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables for the server process
    #[serde(default)]
    pub env: FxHashMap<String, String>,
    /// Working directory for the server process
    pub cwd: Option<String>,
}

/// Workflow parsed from YAML (raw)
#[derive(Debug, Deserialize)]
struct WorkflowRaw {
    pub schema: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    /// MCP server configurations (v0.2)
    #[serde(default)]
    pub mcp: Option<FxHashMap<String, McpConfigInline>>,
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub flows: Vec<Flow>,
}

/// Workflow with Arc-wrapped tasks for efficient cloning
#[derive(Debug)]
pub struct Workflow {
    pub schema: String,
    pub provider: String,
    pub model: Option<String>,
    /// MCP server configurations (v0.2)
    ///
    /// Allows workflows to define MCP servers inline rather than
    /// referencing external configuration. The map key is the server
    /// name used in `invoke.mcp` fields.
    pub mcp: Option<FxHashMap<String, McpConfigInline>>,
    pub tasks: Vec<Arc<Task>>,
    pub flows: Vec<Flow>,
}

impl<'de> Deserialize<'de> for Workflow {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = WorkflowRaw::deserialize(deserializer)?;
        Ok(Workflow {
            schema: raw.schema,
            provider: raw.provider,
            model: raw.model,
            mcp: raw.mcp,
            tasks: raw.tasks.into_iter().map(Arc::new).collect(),
            flows: raw.flows,
        })
    }
}

impl Workflow {
    /// Compute a hash of the workflow for cache invalidation (v0.4.1)
    ///
    /// Uses xxhash3 for fast hashing. The hash is computed from:
    /// - Schema version
    /// - Provider + model
    /// - Task count and IDs
    ///
    /// Returns a 16-character hex string (64-bit hash).
    pub fn compute_hash(&self) -> String {
        use xxhash_rust::xxh3::xxh3_64;

        let mut hasher_input = String::new();
        hasher_input.push_str(&self.schema);
        hasher_input.push_str(&self.provider);
        if let Some(ref model) = self.model {
            hasher_input.push_str(model);
        }
        hasher_input.push_str(&self.tasks.len().to_string());
        for task in &self.tasks {
            hasher_input.push_str(&task.id);
        }

        let hash = xxh3_64(hasher_input.as_bytes());
        format!("{:016x}", hash)
    }

    /// Validate the workflow schema version and task configuration
    ///
    /// Returns error if:
    /// - Schema doesn't match expected version (v0.1, v0.2, v0.3, v0.4, or v0.5)
    /// - Any task has invalid for_each configuration (non-array or empty)
    pub fn validate_schema(&self) -> Result<(), NikaError> {
        // Validate schema version
        if self.schema != SCHEMA_V01
            && self.schema != SCHEMA_V02
            && self.schema != SCHEMA_V03
            && self.schema != SCHEMA_V04
            && self.schema != SCHEMA_V05
        {
            return Err(NikaError::InvalidSchema {
                expected: format!(
                    "{} or {} or {} or {} or {}",
                    SCHEMA_V01, SCHEMA_V02, SCHEMA_V03, SCHEMA_V04, SCHEMA_V05
                ),
                actual: self.schema.clone(),
            });
        }

        // Validate for_each on all tasks
        for task in &self.tasks {
            task.validate_for_each()?;
        }

        Ok(())
    }
}

fn default_provider() -> String {
    "claude".to_string()
}

#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: String,
    /// Explicit data wiring (v0.1)
    #[serde(default, rename = "use")]
    pub use_wiring: Option<WiringSpec>,
    /// Output format and validation (v0.1)
    #[serde(default)]
    pub output: Option<OutputPolicy>,
    /// Runtime DAG expansion via semantic traversal (v0.5)
    ///
    /// When specified, the task will be decomposed at runtime based on
    /// graph traversal results. This takes precedence over static `for_each`.
    ///
    /// # Example
    ///
    /// ```yaml
    /// tasks:
    ///   - id: generate_children
    ///     decompose:
    ///       strategy: semantic
    ///       traverse: HAS_CHILD
    ///       source: $entity
    ///     infer: "Generate for {{use.item}}"
    /// ```
    #[serde(default)]
    pub decompose: Option<DecomposeSpec>,
    /// Parallel iteration over array values (v0.3)
    ///
    /// When specified, the task will be executed once for each value in the array.
    /// Each iteration runs in parallel with its own bindings.
    ///
    /// # Example
    ///
    /// ```yaml
    /// tasks:
    ///   - id: process_locales
    ///     for_each: ["en-US", "fr-FR", "de-DE"]
    ///     as: locale
    ///     exec:
    ///       command: "echo {{use.locale}}"
    /// ```
    #[serde(default)]
    pub for_each: Option<serde_json::Value>,
    /// Variable name for current iteration value (v0.3)
    ///
    /// Defaults to "item" if not specified.
    /// The value is accessible as `{{use.<as>}}` in templates.
    #[serde(default, rename = "as")]
    pub for_each_as: Option<String>,
    /// Maximum parallel executions for for_each (v0.3)
    ///
    /// Controls how many iterations run concurrently.
    /// Defaults to 1 (sequential). Set higher for parallel execution.
    ///
    /// # Example
    ///
    /// ```yaml
    /// for_each: ["a", "b", "c", "d", "e"]
    /// concurrency: 3  # Run at most 3 at a time
    /// ```
    #[serde(default)]
    pub concurrency: Option<usize>,
    /// Stop all iterations on first error (v0.3)
    ///
    /// When true (default), aborts remaining iterations if any fails.
    /// When false, continues executing remaining iterations.
    ///
    /// # Example
    ///
    /// ```yaml
    /// for_each: $items
    /// fail_fast: false  # Continue even if some fail
    /// ```
    #[serde(default)]
    pub fail_fast: Option<bool>,
    #[serde(flatten)]
    pub action: TaskAction,
}

impl Task {
    /// Validate for_each configuration (v0.3)
    ///
    /// Returns error if:
    /// - for_each is not an array and not a binding expression
    /// - for_each array is empty
    ///
    /// Binding expressions (strings containing `{{`) are accepted because
    /// they resolve to arrays at runtime.
    pub fn validate_for_each(&self) -> Result<(), NikaError> {
        if let Some(for_each) = &self.for_each {
            // Accept arrays
            if for_each.is_array() {
                if let Some(arr) = for_each.as_array() {
                    if arr.is_empty() {
                        return Err(NikaError::ValidationError {
                            reason: "for_each array cannot be empty".to_string(),
                        });
                    }
                }
                return Ok(());
            }
            // Accept binding expressions (e.g., "{{use.items}}", "$items")
            if let Some(s) = for_each.as_str() {
                if s.contains("{{") || s.starts_with('$') {
                    return Ok(());
                }
            }
            // Reject everything else
            return Err(NikaError::ValidationError {
                reason: format!(
                    "for_each must be an array or binding expression, got {}",
                    for_each
                ),
            });
        }
        Ok(())
    }

    /// Check if this task has for_each iteration
    pub fn has_for_each(&self) -> bool {
        self.for_each.is_some()
    }

    /// Get the iteration variable name (defaults to "item")
    pub fn for_each_var(&self) -> &str {
        self.for_each_as.as_deref().unwrap_or("item")
    }

    /// Get the concurrency limit for for_each (defaults to 1 = sequential)
    pub fn for_each_concurrency(&self) -> usize {
        self.concurrency.unwrap_or(1).max(1) // At least 1
    }

    /// Get the fail_fast setting for for_each (defaults to true)
    pub fn for_each_fail_fast(&self) -> bool {
        self.fail_fast.unwrap_or(true)
    }

    /// Check if this task has decompose modifier (v0.5)
    pub fn has_decompose(&self) -> bool {
        self.decompose.is_some()
    }

    /// Get the decompose spec if present (v0.5)
    pub fn decompose_spec(&self) -> Option<&DecomposeSpec> {
        self.decompose.as_ref()
    }

    /// Get the action icon for TUI display
    ///
    /// Returns an emoji icon based on the task's verb type.
    /// Canonical icons from CLAUDE.md:
    /// - ⚡ infer (LLM generation)
    /// - 📟 exec (Shell command)
    /// - 🛰️ fetch (HTTP request)
    /// - 🔌 invoke (MCP tool)
    /// - 🐔 agent (Agentic loop - parent)
    /// - 🐤 subagent (spawned via spawn_agent)
    pub fn action_icon(&self) -> &'static str {
        match &self.action {
            TaskAction::Infer { .. } => "⚡",  // LLM generation
            TaskAction::Exec { .. } => "📟",   // Shell command
            TaskAction::Fetch { .. } => "🛰️",  // HTTP request
            TaskAction::Invoke { .. } => "🔌", // MCP tool
            TaskAction::Agent { .. } => "🐔",  // Agentic loop (parent)
        }
    }

    /// Get the icon for a subagent (spawned via spawn_agent)
    pub fn subagent_icon() -> &'static str {
        "🐤" // Spawned subagent
    }

    /// Get list of task IDs this task depends on
    ///
    /// Note: Task-level dependencies are defined via `flows` at the Workflow level.
    /// This method returns an empty vector as tasks don't have inline `depends_on`.
    /// Use FlowGraph for full dependency analysis.
    pub fn depends_on_ids(&self) -> Vec<&str> {
        Vec::new()
    }
}

#[derive(Debug, Deserialize)]
pub struct Flow {
    pub source: FlowEndpoint,
    pub target: FlowEndpoint,
}

/// Handles string OR array for source/target
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FlowEndpoint {
    Single(String),
    Multiple(Vec<String>),
}

impl FlowEndpoint {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            FlowEndpoint::Single(s) => vec![s],
            FlowEndpoint::Multiple(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // WORKFLOW PARSING TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_workflow_parse_minimal_v05() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: hello
    infer: "Say hello"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.schema, "nika/workflow@0.5");
        assert_eq!(workflow.provider, "claude"); // default
        assert_eq!(workflow.tasks.len(), 1);
        assert_eq!(workflow.tasks[0].id, "hello");
        assert!(workflow.model.is_none());
        assert!(workflow.mcp.is_none());
        assert!(workflow.flows.is_empty());
    }

    #[test]
    fn test_workflow_parse_with_provider_and_model() {
        let yaml = r#"
schema: nika/workflow@0.5
provider: openai
model: gpt-4-turbo
tasks:
  - id: task1
    exec: "echo test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.provider, "openai");
        assert_eq!(workflow.model, Some("gpt-4-turbo".to_string()));
    }

    #[test]
    fn test_workflow_parse_multiple_tasks() {
        let yaml = r#"
schema: nika/workflow@0.1
tasks:
  - id: task1
    infer: "First task"
  - id: task2
    exec: "echo done"
  - id: task3
    fetch:
      url: "https://example.com"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.tasks.len(), 3);
        assert_eq!(workflow.tasks[0].id, "task1");
        assert_eq!(workflow.tasks[1].id, "task2");
        assert_eq!(workflow.tasks[2].id, "task3");
    }

    #[test]
    fn test_workflow_parse_with_flows() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    infer: "Refine"
flows:
  - source: step1
    target: step2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.flows.len(), 1);
    }

    #[test]
    fn test_workflow_parse_with_mcp_config() {
        let yaml = r#"
schema: nika/workflow@0.2
mcp:
  novanet:
    command: cargo
    args: [run, -p, novanet-mcp]
    env:
      NEO4J_URI: bolt://localhost:7687
tasks:
  - id: invoke_task
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: qr-code
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse workflow");

        assert!(workflow.mcp.is_some());
        let mcp = workflow.mcp.unwrap();
        assert!(mcp.contains_key("novanet"));

        let novanet_config = &mcp["novanet"];
        assert_eq!(novanet_config.command, "cargo");
        assert_eq!(novanet_config.args.len(), 3);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SCHEMA VALIDATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_validate_schema_v01() {
        let yaml = r#"
schema: nika/workflow@0.1
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_validate_schema_v02() {
        let yaml = r#"
schema: nika/workflow@0.2
tasks:
  - id: task1
    invoke:
      mcp: novanet
      tool: novanet_generate
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_validate_schema_v03() {
        let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: task1
    for_each: ["a", "b"]
    infer: "Test {{use.item}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_validate_schema_v04() {
        let yaml = r#"
schema: nika/workflow@0.4
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_validate_schema_v05() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_validate_schema_invalid_version() {
        let yaml = r#"
schema: nika/workflow@0.99
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        let result = workflow.validate_schema();

        assert!(result.is_err());
        if let Err(e) = result {
            let error_str = format!("{:?}", e);
            assert!(error_str.contains("InvalidSchema"));
        }
    }

    #[test]
    fn test_validate_schema_unknown_version() {
        let yaml = r#"
schema: unknown/workflow@0.1
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TASK OPERATIONS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_task_for_each_helpers_with_for_each() {
        let yaml = r#"
id: test_task
for_each: ["en-US", "fr-FR", "de-DE"]
as: locale
concurrency: 3
fail_fast: false
infer: "Generate for {{use.locale}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse task");

        assert!(task.has_for_each());
        assert_eq!(task.for_each_var(), "locale");
        assert_eq!(task.for_each_concurrency(), 3);
        assert!(!task.for_each_fail_fast());
    }

    #[test]
    fn test_task_for_each_helpers_defaults() {
        let yaml = r#"
id: test_task
for_each: ["a", "b"]
infer: "Test {{use.item}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse task");

        assert!(task.has_for_each());
        assert_eq!(task.for_each_var(), "item"); // default
        assert_eq!(task.for_each_concurrency(), 1); // default = sequential
        assert!(task.for_each_fail_fast()); // default = true
    }

    #[test]
    fn test_task_without_for_each() {
        let yaml = r#"
id: simple_task
infer: "Simple test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse task");

        assert!(!task.has_for_each());
        assert_eq!(task.for_each_var(), "item");
        assert_eq!(task.for_each_concurrency(), 1);
    }

    #[test]
    fn test_task_decompose_helpers() {
        let yaml = r#"
id: decompose_task
decompose:
  strategy: semantic
  traverse: HAS_CHILD
  source: "$entity"
infer: "Generate for {{use.item}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse task");

        assert!(task.has_decompose());
        assert!(task.decompose_spec().is_some());
    }

    #[test]
    fn test_task_without_decompose() {
        let yaml = r#"
id: normal_task
infer: "No decompose"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse task");

        assert!(!task.has_decompose());
        assert!(task.decompose_spec().is_none());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FOR_EACH VALIDATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_validate_for_each_with_array() {
        let yaml = r#"
id: test
for_each: ["a", "b", "c"]
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.validate_for_each().is_ok());
    }

    #[test]
    fn test_validate_for_each_with_binding_expression_template() {
        let yaml = r#"
id: test
for_each: "{{use.items}}"
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.validate_for_each().is_ok());
    }

    #[test]
    fn test_validate_for_each_with_binding_expression_dollar() {
        let yaml = r#"
id: test
for_each: "$items"
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.validate_for_each().is_ok());
    }

    #[test]
    fn test_validate_for_each_empty_array_fails() {
        let yaml = r#"
id: test
for_each: []
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let result = task.validate_for_each();

        assert!(result.is_err());
        if let Err(e) = result {
            let error_str = format!("{:?}", e);
            assert!(error_str.contains("for_each array cannot be empty"));
        }
    }

    #[test]
    fn test_validate_for_each_invalid_type_fails() {
        let yaml = r#"
id: test
for_each: 42
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let result = task.validate_for_each();

        assert!(result.is_err());
        if let Err(e) = result {
            let error_str = format!("{:?}", e);
            assert!(error_str.contains("for_each must be an array or binding expression"));
        }
    }

    #[test]
    fn test_validate_for_each_invalid_string_fails() {
        let yaml = r#"
id: test
for_each: "plain_string"
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let result = task.validate_for_each();

        assert!(result.is_err());
    }

    #[test]
    fn test_validate_for_each_none() {
        let yaml = r#"
id: test
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.validate_for_each().is_ok());
    }

    #[test]
    fn test_workflow_validate_for_each_on_all_tasks() {
        let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: task1
    for_each: ["a", "b"]
    infer: "Test"
  - id: task2
    infer: "Normal"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_workflow_validate_fails_with_empty_for_each() {
        let yaml = r#"
schema: nika/workflow@0.3
tasks:
  - id: task1
    for_each: []
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        let result = workflow.validate_schema();

        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TASK ACTION ICONS TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_task_action_icon_infer() {
        let yaml = r#"
id: test
infer: "Generate something"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.action_icon(), "⚡");
    }

    #[test]
    fn test_task_action_icon_exec() {
        let yaml = r#"
id: test
exec: "echo hello"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.action_icon(), "📟");
    }

    #[test]
    fn test_task_action_icon_fetch() {
        let yaml = r#"
id: test
fetch:
  url: "https://example.com"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.action_icon(), "🛰️");
    }

    #[test]
    fn test_task_action_icon_invoke() {
        let yaml = r#"
id: test
invoke:
  mcp: novanet
  tool: novanet_generate
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.action_icon(), "🔌");
    }

    #[test]
    fn test_task_action_icon_agent() {
        let yaml = r#"
id: test
agent:
  prompt: "Generate something"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.action_icon(), "🐔");
    }

    #[test]
    fn test_task_subagent_icon() {
        assert_eq!(Task::subagent_icon(), "🐤");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FLOW ENDPOINT TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_flow_endpoint_single() {
        let yaml = r#"
source: step1
target: step2
"#;
        let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse");

        let source_vec = flow.source.as_vec();
        assert_eq!(source_vec.len(), 1);
        assert_eq!(source_vec[0], "step1");

        let target_vec = flow.target.as_vec();
        assert_eq!(target_vec.len(), 1);
        assert_eq!(target_vec[0], "step2");
    }

    #[test]
    fn test_flow_endpoint_multiple_source() {
        let yaml = r#"
source:
  - step1
  - step2
target: step3
"#;
        let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse");

        let source_vec = flow.source.as_vec();
        assert_eq!(source_vec.len(), 2);
        assert_eq!(source_vec[0], "step1");
        assert_eq!(source_vec[1], "step2");
    }

    #[test]
    fn test_flow_endpoint_multiple_target() {
        let yaml = r#"
source: step1
target:
  - step2
  - step3
  - step4
"#;
        let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse");

        let target_vec = flow.target.as_vec();
        assert_eq!(target_vec.len(), 3);
        assert_eq!(target_vec[0], "step2");
        assert_eq!(target_vec[1], "step3");
        assert_eq!(target_vec[2], "step4");
    }

    #[test]
    fn test_flow_endpoint_multiple_both() {
        let yaml = r#"
source: [step1, step2]
target: [step3, step4]
"#;
        let flow: Flow = serde_yaml::from_str(yaml).expect("Failed to parse");

        assert_eq!(flow.source.as_vec().len(), 2);
        assert_eq!(flow.target.as_vec().len(), 2);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // HASH COMPUTATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_workflow_compute_hash() {
        let yaml = r#"
schema: nika/workflow@0.5
provider: claude
model: claude-sonnet-4-20250514
tasks:
  - id: task1
    infer: "Test"
  - id: task2
    exec: "echo done"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        let hash = workflow.compute_hash();

        // Should be 16-character hex string (64-bit hash)
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_workflow_compute_hash_consistency() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        let hash1 = workflow.compute_hash();
        let hash2 = workflow.compute_hash();

        // Same workflow should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_workflow_compute_hash_differs_with_schema() {
        let yaml_v1 = r#"
schema: nika/workflow@0.1
tasks:
  - id: task1
    infer: "Test"
"#;
        let yaml_v5 = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow_v1: Workflow = serde_yaml::from_str(yaml_v1).expect("Failed to parse");
        let workflow_v5: Workflow = serde_yaml::from_str(yaml_v5).expect("Failed to parse");

        let hash_v1 = workflow_v1.compute_hash();
        let hash_v5 = workflow_v5.compute_hash();

        // Different schema should produce different hash
        assert_ne!(hash_v1, hash_v5);
    }

    #[test]
    fn test_workflow_compute_hash_differs_with_tasks() {
        let yaml_1task = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
"#;
        let yaml_2tasks = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
  - id: task2
    exec: "echo done"
"#;
        let workflow_1: Workflow = serde_yaml::from_str(yaml_1task).expect("Failed to parse");
        let workflow_2: Workflow = serde_yaml::from_str(yaml_2tasks).expect("Failed to parse");

        // Different task count should produce different hash
        assert_ne!(workflow_1.compute_hash(), workflow_2.compute_hash());
    }

    #[test]
    fn test_workflow_compute_hash_differs_with_model() {
        let yaml_claude = r#"
schema: nika/workflow@0.5
model: claude-sonnet-4-20250514
tasks:
  - id: task1
    infer: "Test"
"#;
        let yaml_openai = r#"
schema: nika/workflow@0.5
model: gpt-4-turbo
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow_claude: Workflow = serde_yaml::from_str(yaml_claude).expect("Failed to parse");
        let workflow_openai: Workflow = serde_yaml::from_str(yaml_openai).expect("Failed to parse");

        // Different models should produce different hash
        assert_ne!(
            workflow_claude.compute_hash(),
            workflow_openai.compute_hash()
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EDGE CASES TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_workflow_empty_tasks_list() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(workflow.tasks.len(), 0);
        assert!(workflow.validate_schema().is_ok());
    }

    #[test]
    fn test_workflow_empty_flows_list() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(workflow.flows.len(), 0);
    }

    #[test]
    fn test_task_depends_on_ids_returns_empty() {
        let yaml = r#"
id: task1
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let deps = task.depends_on_ids();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_workflow_with_multiple_flows() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: step1
    infer: "Start"
  - id: step2
    infer: "Middle"
  - id: step3
    infer: "End"
flows:
  - source: step1
    target: step2
  - source: step2
    target: step3
  - source: step1
    target: step3
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(workflow.flows.len(), 3);
    }

    #[test]
    fn test_task_with_use_wiring() {
        let yaml = r#"
id: task1
use:
  input: previous_task.result
infer: "Process {{use.input}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.use_wiring.is_some());
    }

    #[test]
    fn test_task_with_output_policy() {
        let yaml = r#"
id: task1
output:
  format: json
infer: "Generate JSON"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.output.is_some());
    }

    #[test]
    fn test_mcp_config_inline_minimal() {
        let yaml = r#"
schema: nika/workflow@0.2
mcp:
  test_server:
    command: echo
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        let mcp = workflow.mcp.unwrap();
        let server = &mcp["test_server"];

        assert_eq!(server.command, "echo");
        assert!(server.args.is_empty());
        assert!(server.env.is_empty());
        assert!(server.cwd.is_none());
    }

    #[test]
    fn test_mcp_config_inline_full() {
        let yaml = r#"
schema: nika/workflow@0.2
mcp:
  novanet:
    command: cargo
    args: [run, -p, novanet-mcp]
    env:
      NEO4J_URI: bolt://localhost:7687
      NEO4J_USER: neo4j
    cwd: /path/to/workspace
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        let mcp = workflow.mcp.unwrap();
        let server = &mcp["novanet"];

        assert_eq!(server.command, "cargo");
        assert_eq!(server.args.len(), 3);
        assert_eq!(server.env.len(), 2);
        assert_eq!(server.cwd, Some("/path/to/workspace".to_string()));
    }

    #[test]
    fn test_task_concurrency_zero_becomes_one() {
        let yaml = r#"
id: test
for_each: ["a", "b"]
concurrency: 0
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        // max(0, 1) = 1
        assert_eq!(task.for_each_concurrency(), 1);
    }

    #[test]
    fn test_task_concurrency_large_value() {
        let yaml = r#"
id: test
for_each: ["a", "b"]
concurrency: 1000
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.for_each_concurrency(), 1000);
    }

    #[test]
    fn test_workflow_default_provider_is_claude() {
        let yaml = r#"
schema: nika/workflow@0.5
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(workflow.provider, "claude");
    }

    #[test]
    fn test_task_as_field_empty_string() {
        let yaml = r#"
id: test
for_each: ["a", "b"]
as: ""
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        // Empty string should use default "item"
        assert_eq!(task.for_each_var(), "");
    }

    #[test]
    fn test_task_as_field_custom_name() {
        let yaml = r#"
id: test
for_each: ["en-US", "fr-FR"]
as: locale
infer: "Generate {{use.locale}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.for_each_var(), "locale");
    }
}
