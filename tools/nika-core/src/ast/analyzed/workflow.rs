//! Analyzed workflow AST.
//!
//! This is the resolved, validated workflow structure with:
//! - All task references resolved to TaskId
//! - Dependency graph validated for cycles
//! - Schema version validated

use indexmap::IndexMap;

use super::ids::{TaskId, TaskTable};
use super::task::AnalyzedTask;
use crate::ast::agent_def::AgentDef;
use crate::ast::artifact::ArtifactsConfig;
use crate::ast::logging::LogConfig;
use crate::source::Span;

/// An analyzed workflow - validated and ready for execution.
///
/// Unlike RawWorkflow, this has:
/// - References resolved to interned IDs (TaskId)
/// - Validated schema version
/// - Validated dependency graph (no cycles)
/// - Validated task IDs (no duplicates)
#[derive(Debug, Clone)]
pub struct AnalyzedWorkflow {
    /// Schema version (validated)
    pub schema_version: SchemaVersion,

    /// Optional workflow name
    pub name: Option<String>,

    /// Optional description
    pub description: Option<String>,

    /// Orchestration goal — when present, enables orchestrator mode.
    pub goal: Option<String>,

    /// Default provider for the workflow (typed enum)
    pub provider: Option<crate::ProviderName>,

    /// Default model for the workflow
    pub model: Option<String>,

    /// Task lookup table (TaskId → name)
    pub task_table: TaskTable,

    /// Tasks in execution order (topologically sorted)
    pub tasks: Vec<AnalyzedTask>,

    /// MCP server configurations (name → config)
    pub mcp_servers: IndexMap<String, AnalyzedMcpServer>,

    /// Context file configurations
    pub context_files: Vec<AnalyzedContextFile>,

    /// Include specifications
    pub include: Vec<AnalyzedIncludeSpec>,

    /// Input parameters with defaults
    pub inputs: IndexMap<String, serde_json::Value>,

    /// Artifact configuration for file persistence
    pub artifacts: Option<ArtifactsConfig>,

    /// Log configuration
    pub log: Option<LogConfig>,

    /// Reusable agent definitions (name → config)
    pub agents: Option<IndexMap<String, AgentDef>>,

    /// Workflow-level skills mapping (alias -> file path)
    ///
    /// Carried from the `skills:` block in the workflow YAML.
    /// Used by `TaskExecutor` to wire skill injection into agent loops.
    pub skills_map: std::collections::HashMap<String, String>,

    /// Orchestrate configuration (goal-driven agent loop).
    pub orchestrate: Option<crate::ast::orchestrate::OrchestrateConfig>,

    /// Routing configuration (fallback chains, smart routing).
    pub routing: Option<crate::ast::routing::RoutingConfig>,

    /// Schedule configuration for recurring execution.
    pub schedule: Option<crate::ast::schedule::ScheduleConfig>,

    /// Global workflow timeout in seconds (default: 3600s = 1h).
    pub max_duration_secs: u64,

    /// Span of the entire workflow
    pub span: Span,
}

impl Default for AnalyzedWorkflow {
    fn default() -> Self {
        Self {
            schema_version: SchemaVersion::V12,
            name: None,
            description: None,
            goal: None,
            provider: None,
            model: None,
            task_table: TaskTable::new(),
            tasks: Vec::new(),
            mcp_servers: IndexMap::new(),
            context_files: Vec::new(),
            include: Vec::new(),
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            schedule: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        }
    }
}

impl AnalyzedWorkflow {
    /// Get a task by its ID.
    pub fn get_task(&self, id: TaskId) -> Option<&AnalyzedTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Get a task by name.
    pub fn get_task_by_name(&self, name: &str) -> Option<&AnalyzedTask> {
        let id = self.task_table.get_id(name)?;
        self.get_task(id)
    }

    /// Get the number of tasks.
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    /// Iterate over all tasks.
    pub fn iter_tasks(&self) -> impl Iterator<Item = &AnalyzedTask> {
        self.tasks.iter()
    }

    /// Check if a task exists by name.
    pub fn has_task(&self, name: &str) -> bool {
        self.task_table.get_id(name).is_some()
    }

    /// Compute a hash of the workflow for trace identification.
    ///
    /// Uses xxhash3 for fast hashing. The hash is computed from:
    /// - Schema version
    /// - Provider + model
    /// - Task count and task names
    ///
    /// Returns a 16-character hex string (64-bit hash).
    pub fn compute_hash(&self) -> String {
        use xxhash_rust::xxh3::xxh3_64;

        let mut input = String::new();
        input.push_str(self.schema_version.as_str());
        if let Some(ref provider) = self.provider {
            input.push_str(provider.as_str());
        }
        if let Some(ref model) = self.model {
            input.push_str(model);
        }
        input.push_str(&self.tasks.len().to_string());
        for task in &self.tasks {
            input.push_str(&task.name);
        }

        let hash = xxh3_64(input.as_bytes());
        format!("{:016x}", hash)
    }
}

// SchemaVersion is defined in ast::schema and re-exported here.
pub use super::super::schema::SchemaVersion;

/// Source for MCP server resolution via `from:` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpFromSource {
    /// Resolve from .mcp.json > .nika/mcp.yaml > ~/.nika/mcp.yaml
    Config,
    /// Resolve from project only (.mcp.json or .nika/mcp.yaml)
    Project,
    /// Resolve from global only (~/.nika/mcp.yaml)
    Global,
}

/// Analyzed MCP server configuration.
#[derive(Debug, Clone)]
pub struct AnalyzedMcpServer {
    /// Server name
    pub name: String,

    /// Reference source (None = inline, Some = resolve from config)
    pub from: Option<McpFromSource>,

    /// Command to spawn (for stdio transport)
    pub command: Option<String>,

    /// Command arguments
    pub args: Vec<String>,

    /// Environment variables
    pub env: IndexMap<String, String>,

    /// Working directory
    pub cwd: Option<String>,

    /// URL (for SSE transport)
    pub url: Option<String>,

    /// Transport type (stdio or sse)
    pub transport: McpTransport,

    /// Span of the server config
    pub span: Span,
}

/// MCP transport type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpTransport {
    /// stdio transport (default)
    #[default]
    Stdio,
    /// SSE transport
    Sse,
}

/// Analyzed context file configuration.
#[derive(Debug, Clone)]
pub struct AnalyzedContextFile {
    /// File path (may contain globs)
    pub path: String,

    /// Optional alias for the file in context
    pub alias: Option<String>,

    /// Maximum bytes to load
    pub max_bytes: Option<u64>,

    /// Span of the config
    pub span: Span,
}

/// Analyzed include specification.
#[derive(Debug, Clone)]
pub struct AnalyzedIncludeSpec {
    /// Path to the included workflow or skill file.
    pub path: String,

    /// Optional task ID prefix for namespace isolation.
    pub prefix: Option<String>,

    /// Span of the include spec
    pub span: Span,
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn test_analyzed_workflow_default() {
        let workflow = AnalyzedWorkflow::default();
        assert_eq!(workflow.task_count(), 0);
        assert!(workflow.name.is_none());
        assert!(workflow.span.is_dummy());
    }

    #[test]
    fn test_analyzed_workflow_provider_is_typed() {
        use crate::ProviderName;

        let mut workflow = AnalyzedWorkflow::default();
        workflow.provider = Some(ProviderName::Anthropic);

        // Provider should be a typed enum, not a raw string
        assert_eq!(workflow.provider, Some(ProviderName::Anthropic));
        assert!(workflow.provider.as_ref().unwrap().requires_api_key());
        assert!(workflow.provider.as_ref().unwrap().supports_vision());
    }

    #[test]
    fn test_analyzed_workflow_provider_alias_resolved() {
        use crate::ProviderName;

        // "claude" alias should resolve to Anthropic variant
        let provider = ProviderName::parse("claude");
        let mut workflow = AnalyzedWorkflow::default();
        workflow.provider = Some(provider);

        assert_eq!(workflow.provider, Some(ProviderName::Anthropic));
        assert_eq!(workflow.provider.as_ref().unwrap().as_str(), "anthropic");
    }

    #[test]
    fn test_analyzed_workflow_hash_with_typed_provider() {
        use crate::ProviderName;

        let mut workflow = AnalyzedWorkflow::default();
        workflow.provider = Some(ProviderName::Anthropic);
        workflow.model = Some("claude-sonnet-4-6".to_string());

        let hash = workflow.compute_hash();
        assert_eq!(hash.len(), 16); // 64-bit hex
    }

    #[test]
    fn test_analyzed_workflow_task_lookup() {
        use crate::binding::WithSpec;

        let mut workflow = AnalyzedWorkflow::default();

        // Insert some tasks
        let id1 = workflow.task_table.insert("task1");
        let id2 = workflow.task_table.insert("task2");

        workflow.tasks.push(AnalyzedTask {
            id: id1,
            name: "task1".to_string(),
            description: None,
            action: super::super::task::AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            span: Span::dummy(),
        });

        workflow.tasks.push(AnalyzedTask {
            id: id2,
            name: "task2".to_string(),
            description: None,
            action: super::super::task::AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            span: Span::dummy(),
        });

        assert_eq!(workflow.task_count(), 2);
        assert!(workflow.has_task("task1"));
        assert!(workflow.has_task("task2"));
        assert!(!workflow.has_task("unknown"));

        assert!(workflow.get_task(id1).is_some());
        assert_eq!(workflow.get_task(id1).unwrap().name, "task1");
        assert!(workflow.get_task_by_name("task2").is_some());
    }
}
