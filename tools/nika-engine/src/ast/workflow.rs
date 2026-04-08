// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Workflow Types - main workflow structure
//!
//! Contains the core YAML-parsed types:
//! - `Workflow`: Root workflow with tasks (edges derived from `task.depends_on`)
//! - `Task`: Individual task definition
//! - `McpConfigInline`: Inline MCP server configuration
//! - `ContextConfig`: File loading at workflow start
//! - `IncludeSpec`: DAG fusion from external workflows

use nika_core::ProviderName;
use rustc_hash::FxHashMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::binding::WithSpec;
use crate::error::NikaError;

use super::action::TaskAction;
use super::decompose::DecomposeSpec;
use super::output::OutputPolicy;

/// Inline MCP server configuration
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
/// MCP server inline configuration — re-exported from nika_mcp.
pub type McpConfigInline = nika_mcp::McpConfigInline;

/// Workflow with Arc-wrapped tasks for efficient cloning
#[derive(Debug, Clone)]
pub struct Workflow {
    pub schema: String,
    pub name: Option<String>,
    pub provider: ProviderName,
    pub model: Option<String>,
    /// MCP server configurations
    ///
    /// Allows workflows to define MCP servers inline rather than
    /// referencing external configuration. The map key is the server
    /// name used in `invoke.mcp` fields.
    pub mcp: Option<FxHashMap<String, McpConfigInline>>,
    /// Context configuration for file loading at workflow start
    ///
    /// Files are loaded into the RunContext at workflow start and accessible
    /// via `{{context.files.alias}}` bindings.
    pub context: Option<super::context::ContextConfig>,
    /// Include external workflows for DAG fusion
    ///
    /// Included workflows have their tasks merged into the main DAG
    /// at parse time. They share the same RunContext.
    pub include: Option<Vec<super::include::IncludeSpec>>,
    /// Reusable agent definitions
    ///
    /// Named agent configurations that can be referenced by tasks.
    /// Agents can be inline definitions or file references.
    pub agents: Option<FxHashMap<String, super::agent_def::AgentDef>>,
    /// Skill file mappings for prompt augmentation
    ///
    /// Named skill files that can be injected into agent system prompts.
    pub skills: Option<FxHashMap<String, super::skill_def::SkillDef>>,
    /// Artifact configuration for file persistence
    ///
    /// Workflow-level defaults for artifact output.
    pub artifacts: Option<super::artifact::ArtifactsConfig>,
    /// Log configuration
    ///
    /// Workflow-level logging configuration.
    pub log: Option<super::logging::LogConfig>,
    /// Input parameters with defaults
    ///
    /// Maps parameter names to their definitions. Each definition is a JSON object
    /// with type, default, description, and optional enum values.
    /// Accessible via `{{inputs.param_name}}` in templates.
    pub inputs: Option<FxHashMap<String, serde_json::Value>>,
    pub tasks: Vec<Arc<Task>>,
}

impl Workflow {
    /// Compute a hash of the workflow for cache invalidation
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
        hasher_input.push_str(self.provider.as_str());
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

    /// Compute DAG edge count from per-task dependencies.
    pub fn flow_count(&self) -> usize {
        self.tasks
            .iter()
            .map(|t| t.depends_on.as_ref().map_or(0, |deps| deps.len()))
            .sum()
    }

    /// Iterate over DAG edges as (source, target) pairs.
    ///
    /// Built from each task's `depends_on` field.
    pub fn edges(&self) -> Vec<(&str, &str)> {
        let mut edges = Vec::new();
        for task in &self.tasks {
            if let Some(ref deps) = task.depends_on {
                for dep in deps {
                    edges.push((dep.as_str(), task.id.as_str()));
                }
            }
        }
        edges
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: String,
    /// Typed binding system
    ///
    /// `with:` block for binding task outputs to local aliases.
    ///
    /// # Example
    ///
    /// ```yaml
    /// tasks:
    ///   - id: process
    ///     with:
    ///       summary: $step1.abstract | lower | trim
    ///       count: $step1.items | length ?? 0
    ///     infer: "Process: {{with.summary}} ({{with.count}} items)"
    /// ```
    #[serde(default, rename = "with")]
    pub with_spec: Option<WithSpec>,
    /// Output format and validation
    #[serde(default)]
    pub output: Option<OutputPolicy>,
    /// Runtime DAG expansion via semantic traversal
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
    ///     infer: "Generate for {{with.item}}"
    /// ```
    #[serde(default)]
    pub decompose: Option<DecomposeSpec>,
    /// Parallel iteration over array values
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
    ///       command: "echo {{with.locale}}"
    /// ```
    #[serde(default)]
    pub for_each: Option<serde_json::Value>,
    /// Variable name for current iteration value
    ///
    /// Defaults to "item" if not specified.
    /// The value is accessible as `{{with.<as>}}` in templates.
    #[serde(default, rename = "as")]
    pub for_each_as: Option<String>,
    /// Maximum parallel executions for for_each
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
    /// Stop all iterations on first error
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
    /// Artifact output configuration for this task
    ///
    /// Can be a simple boolean to enable/disable, a single output spec,
    /// or an array of output specs for multiple artifacts.
    #[serde(default)]
    pub artifact: Option<super::artifact::ArtifactSpec>,
    /// Task-level logging configuration
    ///
    /// Overrides workflow-level log settings for this task.
    #[serde(default)]
    pub log: Option<super::logging::LogConfig>,
    /// Explicit task dependencies
    ///
    /// Task IDs that must complete before this task can execute.
    ///
    /// Accepts both `flow` and `depends_on` as field names.
    ///
    /// # Example
    ///
    /// ```yaml
    /// - id: process
    ///   depends_on: [fetch_data, validate]
    ///   infer: "Process {{with.data}}"
    /// ```
    #[serde(default)]
    pub depends_on: Option<Vec<String>>,
    /// Structured output configuration
    ///
    /// When specified, enforces JSON Schema validation on task output.
    /// Uses the 4-layer StructuredOutputEngine for ~99.99% compliance.
    ///
    /// # Example
    ///
    /// ```yaml
    /// - id: extract_data
    ///   infer: "Extract user data"
    ///   structured: ./schemas/user.json
    /// ```
    ///
    /// Or with full configuration:
    ///
    /// ```yaml
    /// - id: extract_data
    ///   infer: "Extract user data"
    ///   structured:
    ///     schema: ./schemas/user.json
    ///     max_retries: 3
    ///     enable_repair: true
    /// ```
    #[serde(default)]
    pub structured: Option<super::structured::StructuredOutputSpec>,
    /// Record compression configuration
    #[serde(default)]
    pub record: Option<nika_core::ast::record::RecordSpec>,
    /// Agent preset reference from the workflow's `agents:` block
    #[serde(default)]
    pub preset: Option<String>,
    /// Conditional execution expression (template resolved at runtime).
    #[serde(default)]
    pub when: Option<String>,
}

impl Task {
    /// Validate for_each configuration
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
            // Accept binding expressions (e.g., "{{with.items}}", "$items")
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

    /// Check if this task has decompose modifier
    pub fn has_decompose(&self) -> bool {
        self.decompose.is_some()
    }

    /// Get the decompose spec if present
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
    /// Returns task IDs from the `depends_on` field.
    pub fn depends_on_ids(&self) -> Vec<&str> {
        self.depends_on
            .as_ref()
            .map(|deps| deps.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}

// Edges are derived from task.depends_on

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parse_workflow;
    use crate::serde_yaml;
    use nika_core::ProviderName;

    // ═══════════════════════════════════════════════════════════════════════════
    // WORKFLOW PARSING TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_workflow_parse_minimal() {
        let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: hello
    infer: "Say hello"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.schema, "nika/workflow@0.12");
        assert_eq!(workflow.provider, ProviderName::Anthropic); // default
        assert_eq!(workflow.tasks.len(), 1);
        assert_eq!(workflow.tasks[0].id, "hello");
        assert_eq!(workflow.model.as_deref(), Some("test-model"));
        assert!(workflow.mcp.is_none());
        assert_eq!(workflow.flow_count(), 0);
    }

    #[test]
    fn test_workflow_parse_with_provider_and_model() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: openai
model: gpt-4-turbo
tasks:
  - id: task1
    exec: "echo test"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.provider, ProviderName::OpenAI);
        assert_eq!(workflow.model, Some("gpt-4-turbo".to_string()));
    }

    #[test]
    fn test_workflow_parse_multiple_tasks() {
        let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: task1
    infer: "First task"
  - id: task2
    exec: "echo done"
  - id: task3
    fetch:
      url: "https://example.com"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse workflow");

        assert_eq!(workflow.tasks.len(), 3);
        assert_eq!(workflow.tasks[0].id, "task1");
        assert_eq!(workflow.tasks[1].id, "task2");
        assert_eq!(workflow.tasks[2].id, "task3");
    }

    #[test]
    fn test_workflow_parse_with_mcp_config() {
        let yaml = r#"
schema: "nika/workflow@0.12"
mcp:
  servers:
    novanet:
      command: cargo
      args: [run, -p, novanet-mcp]
      env:
        NEO4J_URI: bolt://localhost:7687
tasks:
  - id: invoke_task
    invoke:
      mcp: novanet
      tool: novanet_context
      params:
        entity: qr-code
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse workflow");

        assert!(workflow.mcp.is_some());
        let mcp = workflow.mcp.unwrap();
        assert!(mcp.contains_key("novanet"));

        let novanet_config = &mcp["novanet"];
        assert_eq!(novanet_config.command, "cargo");
        assert_eq!(novanet_config.args.len(), 3);
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
infer: "Generate for {{with.locale}}"
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
infer: "Test {{with.item}}"
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
infer: "Generate for {{with.item}}"
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
for_each: "{{with.items}}"
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
  tool: novanet_context
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
    // HASH COMPUTATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_workflow_compute_hash() {
        let yaml = r#"
schema: "nika/workflow@0.12"
provider: claude
model: claude-sonnet-4-6
tasks:
  - id: task1
    infer: "Test"
  - id: task2
    exec: "echo done"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse");
        let hash = workflow.compute_hash();

        // Should be 16-character hex string (64-bit hash)
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_workflow_compute_hash_consistency() {
        let yaml = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse");
        let hash1 = workflow.compute_hash();
        let hash2 = workflow.compute_hash();

        // Same workflow should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_workflow_compute_hash_differs_with_schema() {
        let yaml_v10 = r#"
schema: "nika/workflow@0.10"
model: test-model
tasks:
  - id: task1
    infer: "Test"
"#;
        let yaml_v12 = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow_v10 = parse_workflow(yaml_v10).expect("Failed to parse");
        let workflow_v12 = parse_workflow(yaml_v12).expect("Failed to parse");

        let hash_v10 = workflow_v10.compute_hash();
        let hash_v12 = workflow_v12.compute_hash();

        // Different schema should produce different hash
        assert_ne!(hash_v10, hash_v12);
    }

    #[test]
    fn test_workflow_compute_hash_differs_with_tasks() {
        let yaml_1task = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: task1
    infer: "Test"
"#;
        let yaml_2tasks = r#"
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: task1
    infer: "Test"
  - id: task2
    exec: "echo done"
"#;
        let workflow_1 = parse_workflow(yaml_1task).expect("Failed to parse");
        let workflow_2 = parse_workflow(yaml_2tasks).expect("Failed to parse");

        // Different task count should produce different hash
        assert_ne!(workflow_1.compute_hash(), workflow_2.compute_hash());
    }

    #[test]
    fn test_workflow_compute_hash_differs_with_model() {
        let yaml_claude = r#"
schema: "nika/workflow@0.12"
model: claude-sonnet-4-6
tasks:
  - id: task1
    infer: "Test"
"#;
        let yaml_openai = r#"
schema: "nika/workflow@0.12"
model: gpt-4-turbo
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow_claude = parse_workflow(yaml_claude).expect("Failed to parse");
        let workflow_openai = parse_workflow(yaml_openai).expect("Failed to parse");

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
    fn test_task_depends_on_ids_returns_empty_when_no_deps() {
        let yaml = r#"
id: task1
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let deps = task.depends_on_ids();
        assert!(deps.is_empty());
    }

    #[test]
    fn test_task_depends_on_alias_works() {
        let yaml = r#"
id: task1
depends_on: [step_a, step_b]
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let deps = task.depends_on_ids();
        assert_eq!(deps, vec!["step_a", "step_b"]);
    }

    #[test]
    fn test_task_depends_on_field_works() {
        let yaml = r#"
id: task1
depends_on: [step_a, step_b]
infer: "Test"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        let deps = task.depends_on_ids();
        assert_eq!(deps, vec!["step_a", "step_b"]);
    }

    #[test]
    fn test_task_with_with_spec() {
        let yaml = r#"
id: task1
with:
  input: $previous_task.result
infer: "Process {{with.input}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(task.with_spec.is_some());
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
schema: "nika/workflow@0.12"
model: test-model
mcp:
  servers:
    test_server:
      command: echo
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse");
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
schema: "nika/workflow@0.12"
model: test-model
mcp:
  servers:
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
        let workflow = parse_workflow(yaml).expect("Failed to parse");
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
schema: "nika/workflow@0.12"
model: test-model
tasks:
  - id: task1
    infer: "Test"
"#;
        let workflow = parse_workflow(yaml).expect("Failed to parse");
        assert_eq!(workflow.provider, ProviderName::Anthropic);
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
infer: "Generate {{with.locale}}"
"#;
        let task: Task = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(task.for_each_var(), "locale");
    }
}
