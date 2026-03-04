//! Raw workflow AST - directly parsed from YAML with spans.

use indexmap::IndexMap;

use crate::source::{Span, Spanned};
use super::task::RawTask;
use super::mcp::RawMcpConfig;

/// Raw workflow as parsed from YAML.
///
/// All fields preserve source positions via `Spanned<T>`.
/// This is Phase 1 of the Two-Phase IR - no validation, just structure.
#[derive(Debug, Clone, Default)]
pub struct RawWorkflow {
    /// Schema version: "nika/workflow@0.10"
    pub schema: Spanned<String>,

    /// Workflow identifier (optional in YAML, defaults to filename)
    pub workflow: Option<Spanned<String>>,

    /// Optional description
    pub description: Option<Spanned<String>>,

    /// Default provider (e.g., "claude", "openai")
    pub provider: Option<Spanned<String>>,

    /// Default model (e.g., "claude-sonnet-4-6")
    pub model: Option<Spanned<String>>,

    /// MCP server configuration
    pub mcp: Option<Spanned<RawMcpConfig>>,

    /// Package includes: pkg: { include: [...] }
    pub pkg: Option<Spanned<RawPkgConfig>>,

    /// Context files configuration
    pub context: Option<Spanned<RawContextConfig>>,

    /// Include external workflows
    pub include: Option<Spanned<Vec<Spanned<String>>>>,

    /// Input parameters with defaults
    pub inputs: Option<Spanned<IndexMap<Spanned<String>, Spanned<serde_json::Value>>>>,

    /// Task definitions (order matters for implicit flow)
    pub tasks: Spanned<Vec<Spanned<RawTask>>>,

    /// Explicit flow definitions
    pub flows: Option<Spanned<Vec<Spanned<RawFlowDef>>>>,

    /// The span of the entire workflow document
    pub span: Span,
}

/// Raw package configuration.
#[derive(Debug, Clone, Default)]
pub struct RawPkgConfig {
    /// Included package specs: ["github:user/repo", "local:./path"]
    pub include: Vec<Spanned<String>>,
}

/// Raw context configuration for file loading.
#[derive(Debug, Clone, Default)]
pub struct RawContextConfig {
    /// File bindings: alias -> path
    pub files: Option<IndexMap<Spanned<String>, Spanned<String>>>,
}

/// Raw explicit flow definition.
#[derive(Debug, Clone, Default)]
pub struct RawFlowDef {
    /// Source task(s)
    pub from: Spanned<Vec<Spanned<String>>>,
    /// Target task(s)
    pub to: Spanned<Vec<Spanned<String>>>,
    /// Span of the entire flow definition
    pub span: Span,
}

impl RawWorkflow {
    /// Create a new empty raw workflow.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the workflow name, falling back to "unnamed".
    pub fn name(&self) -> &str {
        self.workflow.as_ref().map(|s| s.value.as_str()).unwrap_or("unnamed")
    }

    /// Get the number of tasks.
    pub fn task_count(&self) -> usize {
        self.tasks.value.len()
    }

    /// Iterate over tasks.
    pub fn iter_tasks(&self) -> impl Iterator<Item = &Spanned<RawTask>> {
        self.tasks.value.iter()
    }

    /// Get a task by ID (linear search - Phase 2 uses interned IDs).
    pub fn get_task(&self, id: &str) -> Option<&Spanned<RawTask>> {
        self.tasks.value.iter().find(|t| t.value.id.value == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FileId, Span};

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    #[test]
    fn test_raw_workflow_default() {
        let workflow = RawWorkflow::default();
        assert_eq!(workflow.task_count(), 0);
        assert_eq!(workflow.name(), "unnamed");
    }

    #[test]
    fn test_raw_workflow_with_tasks() {
        use super::super::task::RawTask;

        let mut workflow = RawWorkflow::new();
        workflow.workflow = Some(Spanned::new("test-workflow".to_string(), make_span(0, 13)));
        workflow.tasks = Spanned::new(
            vec![
                Spanned::new(
                    RawTask {
                        id: Spanned::new("task1".to_string(), make_span(20, 25)),
                        ..Default::default()
                    },
                    make_span(18, 50),
                ),
                Spanned::new(
                    RawTask {
                        id: Spanned::new("task2".to_string(), make_span(60, 65)),
                        ..Default::default()
                    },
                    make_span(58, 90),
                ),
            ],
            make_span(15, 95),
        );

        assert_eq!(workflow.task_count(), 2);
        assert_eq!(workflow.name(), "test-workflow");
        assert!(workflow.get_task("task1").is_some());
        assert!(workflow.get_task("task2").is_some());
        assert!(workflow.get_task("task3").is_none());
    }
}
