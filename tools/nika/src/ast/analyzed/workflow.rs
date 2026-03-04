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

    /// Named flow definitions (flows: section)
    pub flow_defs: IndexMap<String, AnalyzedFlowDef>,

    /// Span of the entire workflow
    pub span: Span,
}

impl Default for AnalyzedWorkflow {
    fn default() -> Self {
        Self {
            schema_version: SchemaVersion::V10,
            name: None,
            description: None,
            provider: None,
            model: None,
            task_table: TaskTable::new(),
            tasks: Vec::new(),
            mcp_servers: IndexMap::new(),
            context_files: Vec::new(),
            flow_defs: IndexMap::new(),
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

/// Validated schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaVersion {
    /// nika/workflow@0.1
    V01,
    /// nika/workflow@0.2
    V02,
    /// nika/workflow@0.3
    V03,
    /// nika/workflow@0.4
    V04,
    /// nika/workflow@0.5
    V05,
    /// nika/workflow@0.6
    V06,
    /// nika/workflow@0.7
    V07,
    /// nika/workflow@0.8
    V08,
    /// nika/workflow@0.9
    V09,
    /// nika/workflow@0.10
    V10,
}

impl SchemaVersion {
    /// Parse a schema version string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "nika/workflow@0.1" => Some(Self::V01),
            "nika/workflow@0.2" => Some(Self::V02),
            "nika/workflow@0.3" => Some(Self::V03),
            "nika/workflow@0.4" => Some(Self::V04),
            "nika/workflow@0.5" => Some(Self::V05),
            "nika/workflow@0.6" => Some(Self::V06),
            "nika/workflow@0.7" => Some(Self::V07),
            "nika/workflow@0.8" => Some(Self::V08),
            "nika/workflow@0.9" => Some(Self::V09),
            "nika/workflow@0.10" => Some(Self::V10),
            _ => None,
        }
    }

    /// Get the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V01 => "nika/workflow@0.1",
            Self::V02 => "nika/workflow@0.2",
            Self::V03 => "nika/workflow@0.3",
            Self::V04 => "nika/workflow@0.4",
            Self::V05 => "nika/workflow@0.5",
            Self::V06 => "nika/workflow@0.6",
            Self::V07 => "nika/workflow@0.7",
            Self::V08 => "nika/workflow@0.8",
            Self::V09 => "nika/workflow@0.9",
            Self::V10 => "nika/workflow@0.10",
        }
    }

    /// Get all valid schema versions.
    pub fn all() -> &'static [Self] {
        &[
            Self::V01,
            Self::V02,
            Self::V03,
            Self::V04,
            Self::V05,
            Self::V06,
            Self::V07,
            Self::V08,
            Self::V09,
            Self::V10,
        ]
    }

    /// Get the latest schema version.
    pub fn latest() -> Self {
        Self::V10
    }

    /// Get the numeric version for comparison (e.g., V03 returns 3).
    pub fn version_number(&self) -> u32 {
        match self {
            Self::V01 => 1,
            Self::V02 => 2,
            Self::V03 => 3,
            Self::V04 => 4,
            Self::V05 => 5,
            Self::V06 => 6,
            Self::V07 => 7,
            Self::V08 => 8,
            Self::V09 => 9,
            Self::V10 => 10,
        }
    }

    /// Check if a version supports a minimum required version.
    pub fn supports(&self, min_version: Self) -> bool {
        self.version_number() >= min_version.version_number()
    }

    /// Check if MCP servers are supported (v0.2+).
    pub fn supports_mcp(&self) -> bool {
        self.supports(Self::V02)
    }

    /// Check if invoke/agent verbs are supported (v0.2+).
    pub fn supports_invoke_agent(&self) -> bool {
        self.supports(Self::V02)
    }

    /// Check if for_each is supported (v0.3+).
    pub fn supports_for_each(&self) -> bool {
        self.supports(Self::V03)
    }

    /// Check if skills are supported (v0.6+).
    pub fn supports_skills(&self) -> bool {
        self.supports(Self::V06)
    }

    /// Check if agent definitions are supported (v0.6+).
    pub fn supports_agent_defs(&self) -> bool {
        self.supports(Self::V06)
    }

    /// Check if context files are supported (v0.9+).
    pub fn supports_context(&self) -> bool {
        self.supports(Self::V09)
    }

    /// Check if include/DAG fusion is supported (v0.9+).
    pub fn supports_include(&self) -> bool {
        self.supports(Self::V09)
    }

    /// Check if inputs are supported (v0.10+).
    pub fn supports_inputs(&self) -> bool {
        self.supports(Self::V10)
    }

    /// Check if artifacts are supported (v0.10+).
    pub fn supports_artifacts(&self) -> bool {
        self.supports(Self::V10)
    }

    /// Check if retry configuration is supported (v0.3+).
    pub fn supports_retry(&self) -> bool {
        self.supports(Self::V03)
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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

/// Analyzed flow definition.
#[derive(Debug, Clone)]
pub struct AnalyzedFlowDef {
    /// Flow name
    pub name: String,

    /// Tasks in this flow (resolved to TaskIds)
    pub tasks: Vec<TaskId>,

    /// Span of the flow definition
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_parse() {
        assert_eq!(
            SchemaVersion::parse("nika/workflow@0.1"),
            Some(SchemaVersion::V01)
        );
        assert_eq!(
            SchemaVersion::parse("nika/workflow@0.10"),
            Some(SchemaVersion::V10)
        );
        assert_eq!(SchemaVersion::parse("invalid"), None);
        assert_eq!(SchemaVersion::parse("nika/workflow@0.99"), None);
    }

    #[test]
    fn test_schema_version_latest() {
        assert_eq!(SchemaVersion::latest(), SchemaVersion::V10);
        assert_eq!(SchemaVersion::latest().as_str(), "nika/workflow@0.10");
    }

    #[test]
    fn test_analyzed_workflow_default() {
        let workflow = AnalyzedWorkflow::default();
        assert_eq!(workflow.task_count(), 0);
        assert!(workflow.name.is_none());
        assert!(workflow.span.is_dummy());
    }

    #[test]
    fn test_analyzed_workflow_task_lookup() {
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
            use_refs: IndexMap::new(),
            flow_deps: Vec::new(),
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
            use_refs: IndexMap::new(),
            flow_deps: Vec::new(),
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

    #[test]
    fn test_schema_version_number() {
        assert_eq!(SchemaVersion::V01.version_number(), 1);
        assert_eq!(SchemaVersion::V05.version_number(), 5);
        assert_eq!(SchemaVersion::V10.version_number(), 10);
    }

    #[test]
    fn test_schema_version_supports() {
        // V01 only supports V01
        assert!(SchemaVersion::V01.supports(SchemaVersion::V01));
        assert!(!SchemaVersion::V01.supports(SchemaVersion::V02));

        // V10 supports all versions
        assert!(SchemaVersion::V10.supports(SchemaVersion::V01));
        assert!(SchemaVersion::V10.supports(SchemaVersion::V10));
    }

    #[test]
    fn test_schema_version_feature_gates() {
        // MCP requires v0.2+
        assert!(!SchemaVersion::V01.supports_mcp());
        assert!(SchemaVersion::V02.supports_mcp());
        assert!(SchemaVersion::V10.supports_mcp());

        // for_each requires v0.3+
        assert!(!SchemaVersion::V01.supports_for_each());
        assert!(!SchemaVersion::V02.supports_for_each());
        assert!(SchemaVersion::V03.supports_for_each());
        assert!(SchemaVersion::V10.supports_for_each());

        // skills requires v0.6+
        assert!(!SchemaVersion::V05.supports_skills());
        assert!(SchemaVersion::V06.supports_skills());
        assert!(SchemaVersion::V10.supports_skills());

        // context requires v0.9+
        assert!(!SchemaVersion::V08.supports_context());
        assert!(SchemaVersion::V09.supports_context());
        assert!(SchemaVersion::V10.supports_context());

        // inputs requires v0.10+
        assert!(!SchemaVersion::V09.supports_inputs());
        assert!(SchemaVersion::V10.supports_inputs());
    }
}
