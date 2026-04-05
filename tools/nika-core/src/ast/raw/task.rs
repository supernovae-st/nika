//! Raw task AST with all fields as Spanned.

use indexmap::IndexMap;

use super::action::RawTaskAction;
use crate::ast::decompose::DecomposeSpec;
use crate::ast::structured::StructuredOutputSpec;
use crate::source::{Span, Spanned};

/// A raw task as parsed from YAML.
///
/// All string references (task IDs, aliases) are unresolved.
/// Resolution happens in Phase 2 (analyzed AST).
///
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

    /// Task-level base URL override for OpenAI-compatible endpoint.
    /// Takes precedence over workflow-level base_url.
    pub base_url: Option<Spanned<String>>,

    /// Agent preset reference from the workflow's `agents:` block.
    pub preset: Option<Spanned<String>>,

    /// Binding declarations: `with: { alias: "expression" }`
    ///
    /// Values are raw strings that get parsed by `parse_with_entry()` in Phase 2.
    /// Examples:
    ///   - `data: step1` — simple task reference
    ///   - `temp: step1.data.temp ?? 20` — path + default
    ///   - `cfg: $env.API_KEY` — environment binding
    ///   - `val: step1.output | upper | trim` — with transforms
    pub with_refs: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,

    /// Explicit ordering dependencies: `depends_on: [task_id1, task_id2]`
    ///
    /// These are pure ordering edges — no data flows through them.
    /// Data dependencies are expressed via `with:` bindings.
    pub depends_on: Option<Spanned<Vec<Spanned<String>>>>,

    /// Output configuration
    pub output: Option<Spanned<RawOutputConfig>>,

    /// For-each iteration
    pub for_each: Option<Spanned<RawForEach>>,

    /// Retry configuration
    pub retry: Option<Spanned<RawRetryConfig>>,

    /// Decompose modifier for runtime DAG expansion
    pub decompose: Option<Spanned<DecomposeSpec>>,

    /// Standalone concurrency (used with decompose when no for_each)
    pub concurrency: Option<Spanned<u32>>,

    /// Standalone fail_fast (used with decompose when no for_each)
    pub fail_fast: Option<Spanned<bool>>,

    /// Routing override for this task.
    pub routing: Option<Spanned<serde_json::Value>>,

    /// Structured output specification (JSON schema enforcement)
    pub structured: Option<StructuredOutputSpec>,

    /// Artifact output configuration (persists task output to files)
    pub artifact: Option<Spanned<serde_json::Value>>,

    /// Log configuration override for this task
    pub log: Option<Spanned<serde_json::Value>>,

    /// Record compression configuration
    pub record: Option<Spanned<serde_json::Value>>,

    /// Context budget in tokens — limits total binding size passed to LLM
    pub context_budget: Option<Spanned<u32>>,

    /// Conditional execution: template expression evaluated as boolean.
    /// If falsy (false, "false", "", null, 0), the task is skipped.
    pub when: Option<Spanned<String>>,

    /// On-error fallback: `on_error: { ignore: true }` or `on_error: { fallback: task_id }`.
    pub on_error: Option<Spanned<serde_json::Value>>,

    /// The span of the entire task block
    pub span: Span,
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
    /// Maximum retries on validation failure
    pub max_retries: Option<Spanned<u32>>,
}

/// For-each iteration configuration.
#[derive(Debug, Clone, Default)]
pub struct RawForEach {
    /// Items expression: "$task_id" or "{{...}}"
    pub items: Spanned<String>,
    /// Loop variable name (default: "item")
    pub as_var: Option<Spanned<String>>,
    /// Maximum concurrency
    pub concurrency: Option<Spanned<u32>>,
    /// Stop all iterations on first error (default: true)
    pub fail_fast: Option<Spanned<bool>>,
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

    /// Check if this task has any dependencies (with: or depends_on:).
    pub fn has_dependencies(&self) -> bool {
        self.with_refs
            .as_ref()
            .map(|w| !w.value.is_empty())
            .unwrap_or(false)
            || self
                .depends_on
                .as_ref()
                .map(|d| !d.value.is_empty())
                .unwrap_or(false)
    }

    /// Get all explicit depends_on task IDs.
    pub fn depends_on_ids(&self) -> Vec<&str> {
        match &self.depends_on {
            Some(deps) => deps.value.iter().map(|s| s.value.as_str()).collect(),
            None => vec![],
        }
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
    fn test_task_has_dependencies_with() {
        let mut task = RawTask::new("consumer");

        let mut with_refs = IndexMap::new();
        with_refs.insert(
            Spanned::new("data".to_string(), make_span(0, 4)),
            Spanned::new("producer".to_string(), make_span(6, 14)),
        );
        task.with_refs = Some(Spanned::new(with_refs, make_span(0, 20)));

        assert!(task.has_dependencies());
    }

    #[test]
    fn test_task_has_dependencies_depends_on() {
        let mut task = RawTask::new("consumer");

        task.depends_on = Some(Spanned::new(
            vec![
                Spanned::new("setup".to_string(), make_span(0, 5)),
                Spanned::new("init".to_string(), make_span(7, 11)),
            ],
            make_span(0, 15),
        ));

        assert!(task.has_dependencies());
        assert_eq!(task.depends_on_ids(), vec!["setup", "init"]);
    }

    #[test]
    fn test_task_depends_on_empty() {
        let task = RawTask::new("standalone");
        assert!(task.depends_on_ids().is_empty());
        assert!(!task.has_dependencies());
    }
}
