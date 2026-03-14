//! Analyzed workflow AST.
//!
//! This is the resolved, validated workflow structure with:
//! - All task references resolved to TaskId
//! - Dependency graph validated for cycles
//! - Schema version validated

use indexmap::IndexMap;

use super::ids::{TaskId, TaskTable};
use super::task::AnalyzedTask;
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

    /// Default provider for the workflow
    pub provider: Option<String>,

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

    /// Import specifications (v0.28, replaces include: + skills:)
    pub imports: Vec<AnalyzedImportSpec>,

    /// Input parameters with defaults
    pub inputs: IndexMap<String, serde_json::Value>,

    /// Span of the entire workflow
    pub span: Span,
}

impl Default for AnalyzedWorkflow {
    fn default() -> Self {
        Self {
            schema_version: SchemaVersion::V12,
            name: None,
            description: None,
            provider: None,
            model: None,
            task_table: TaskTable::new(),
            tasks: Vec::new(),
            mcp_servers: IndexMap::new(),
            context_files: Vec::new(),
            imports: Vec::new(),
            inputs: IndexMap::new(),
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
}

// SchemaVersion is defined in ast::schema and re-exported here for backwards compatibility.
pub use super::super::schema::SchemaVersion;

/// Analyzed MCP server configuration.
#[derive(Debug, Clone)]
pub struct AnalyzedMcpServer {
    /// Server name
    pub name: String,

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

/// Analyzed import specification (v0.28, replaces include: + skills:).
#[derive(Debug, Clone)]
pub struct AnalyzedImportSpec {
    /// Path to the imported workflow or skill file.
    pub path: String,

    /// Optional task ID prefix for namespace isolation.
    pub prefix: Option<String>,

    /// Span of the import spec
    pub span: Span,
}

#[cfg(test)]
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
