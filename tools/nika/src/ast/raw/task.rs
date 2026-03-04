//! Raw task AST with all fields as Spanned.

use indexmap::IndexMap;

use crate::source::{Span, Spanned};
use super::action::RawTaskAction;

/// A raw task as parsed from YAML.
///
/// All string references (task IDs, aliases) are unresolved.
/// Resolution happens in Phase 2 (analyzed AST).
#[derive(Debug, Clone, Default)]
pub struct RawTask {
    /// Task identifier (must be unique within workflow)
    pub id: Spanned<String>,

    /// Human-readable description
    pub description: Option<Spanned<String>>,

    /// The action this task performs (infer, exec, fetch, invoke, agent)
    pub action: Option<RawTaskAction>,

    /// Task-specific provider override
    pub provider: Option<Spanned<String>>,

    /// Task-specific model override
    pub model: Option<Spanned<String>>,

    /// Dependencies: use: { alias: task_id } or use: { alias: { task: id, path: "..." } }
    pub use_refs: Option<Spanned<IndexMap<Spanned<String>, RawUseTarget>>>,

    /// Execution dependencies: flow: [task_ids]
    pub flow: Option<Spanned<RawFlow>>,

    /// Output configuration
    pub output: Option<Spanned<RawOutputConfig>>,

    /// For-each iteration
    pub for_each: Option<Spanned<RawForEach>>,

    /// Retry configuration
    pub retry: Option<Spanned<RawRetryConfig>>,

    /// The span of the entire task block
    pub span: Span,
}

/// Flow dependencies for a task.
#[derive(Debug, Clone, Default)]
pub enum RawFlow {
    /// Single dependency: flow: task_id
    Single(Spanned<String>),
    /// Multiple dependencies: flow: [task_id1, task_id2]
    Multiple(Vec<Spanned<String>>),
    #[default]
    None,
}

impl RawFlow {
    /// Get all task IDs in this flow.
    pub fn task_ids(&self) -> Vec<&str> {
        match self {
            RawFlow::Single(id) => vec![&id.value],
            RawFlow::Multiple(ids) => ids.iter().map(|s| s.value.as_str()).collect(),
            RawFlow::None => vec![],
        }
    }

    /// Check if this flow is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, RawFlow::None)
    }
}

/// A use reference target.
#[derive(Debug, Clone)]
pub enum RawUseTarget {
    /// Simple reference: alias: task_id
    TaskId(Spanned<String>),
    /// Extended reference with path: { task: task_id, path: "$.field" }
    Extended {
        task: Spanned<String>,
        path: Option<Spanned<String>>,
        span: Span,
    },
}

impl Default for RawUseTarget {
    fn default() -> Self {
        RawUseTarget::TaskId(Spanned::dummy(String::new()))
    }
}

impl RawUseTarget {
    /// Get the target task ID.
    pub fn task_id(&self) -> &str {
        match self {
            RawUseTarget::TaskId(id) => &id.value,
            RawUseTarget::Extended { task, .. } => &task.value,
        }
    }

    /// Get the span of the task ID reference.
    pub fn task_span(&self) -> Span {
        match self {
            RawUseTarget::TaskId(id) => id.span,
            RawUseTarget::Extended { task, .. } => task.span,
        }
    }

    /// Get the path if present.
    pub fn path(&self) -> Option<&str> {
        match self {
            RawUseTarget::TaskId(_) => None,
            RawUseTarget::Extended { path, .. } => path.as_ref().map(|p| p.value.as_str()),
        }
    }
}

/// A use reference: alias -> target
#[derive(Debug, Clone)]
pub struct RawUseRef {
    pub alias: Spanned<String>,
    pub target: RawUseTarget,
}

/// Output configuration for a task.
#[derive(Debug, Clone, Default)]
pub struct RawOutputConfig {
    /// Output format: text, json, yaml
    pub format: Option<Spanned<String>>,
    /// JSON Schema for validation
    pub schema: Option<Spanned<serde_json::Value>>,
    /// Schema reference: file path or inline
    pub schema_ref: Option<Spanned<String>>,
}

/// For-each iteration configuration.
#[derive(Debug, Clone, Default)]
pub struct RawForEach {
    /// Items expression: "use.task_id" or "{{...}}"
    pub items: Spanned<String>,
    /// Loop variable name (default: "item")
    pub as_var: Option<Spanned<String>>,
    /// Maximum parallelism
    pub parallel: Option<Spanned<u32>>,
}

/// Retry configuration.
#[derive(Debug, Clone, Default)]
pub struct RawRetryConfig {
    /// Maximum retry attempts
    pub max_attempts: Option<Spanned<u32>>,
    /// Delay between retries in milliseconds
    pub delay_ms: Option<Spanned<u64>>,
    /// Exponential backoff multiplier
    pub backoff: Option<Spanned<f64>>,
}

impl RawTask {
    /// Create a new raw task with the given ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Spanned::dummy(id.into()),
            ..Default::default()
        }
    }

    /// Check if this task has any dependencies.
    pub fn has_dependencies(&self) -> bool {
        self.use_refs.as_ref().map(|u| !u.value.is_empty()).unwrap_or(false)
            || self.flow.as_ref().map(|f| !f.value.is_empty()).unwrap_or(false)
    }

    /// Get all task IDs this task depends on.
    pub fn dependency_ids(&self) -> Vec<&str> {
        let mut deps = Vec::new();

        // Add use: dependencies
        if let Some(use_refs) = &self.use_refs {
            for target in use_refs.value.values() {
                deps.push(target.task_id());
            }
        }

        // Add flow: dependencies
        if let Some(flow) = &self.flow {
            deps.extend(flow.value.task_ids());
        }

        deps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    #[test]
    fn test_raw_task_new() {
        let task = RawTask::new("my-task");
        assert_eq!(task.id.value, "my-task");
        assert!(!task.has_dependencies());
    }

    #[test]
    fn test_raw_flow_task_ids() {
        let single = RawFlow::Single(Spanned::new("task1".to_string(), make_span(0, 5)));
        assert_eq!(single.task_ids(), vec!["task1"]);

        let multiple = RawFlow::Multiple(vec![
            Spanned::new("task1".to_string(), make_span(0, 5)),
            Spanned::new("task2".to_string(), make_span(7, 12)),
        ]);
        assert_eq!(multiple.task_ids(), vec!["task1", "task2"]);

        let none = RawFlow::None;
        assert!(none.task_ids().is_empty());
    }

    #[test]
    fn test_raw_use_target() {
        let simple = RawUseTarget::TaskId(Spanned::new("source".to_string(), make_span(0, 6)));
        assert_eq!(simple.task_id(), "source");
        assert!(simple.path().is_none());

        let extended = RawUseTarget::Extended {
            task: Spanned::new("source".to_string(), make_span(0, 6)),
            path: Some(Spanned::new("$.result".to_string(), make_span(10, 18))),
            span: make_span(0, 20),
        };
        assert_eq!(extended.task_id(), "source");
        assert_eq!(extended.path(), Some("$.result"));
    }

    #[test]
    fn test_task_dependency_ids() {
        let mut task = RawTask::new("consumer");

        // Add use: dependency
        let mut use_refs = IndexMap::new();
        use_refs.insert(
            Spanned::new("data".to_string(), make_span(0, 4)),
            RawUseTarget::TaskId(Spanned::new("producer".to_string(), make_span(6, 14))),
        );
        task.use_refs = Some(Spanned::new(use_refs, make_span(0, 20)));

        // Add flow: dependency
        task.flow = Some(Spanned::new(
            RawFlow::Single(Spanned::new("setup".to_string(), make_span(0, 5))),
            make_span(0, 10),
        ));

        let deps = task.dependency_ids();
        assert!(deps.contains(&"producer"));
        assert!(deps.contains(&"setup"));
    }
}
