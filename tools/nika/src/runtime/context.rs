//! WorkflowMeta — read-only workflow metadata for execution.
//!
//! Inspired by rustc's Session pattern. Created once from AnalyzedWorkflow,
//! shared via Arc across all runtime components.
//!
//! This provides bidirectional TaskId ↔ name mapping and workflow-level
//! defaults (provider, model) without requiring mutable access.

use std::sync::Arc;

use crate::ast::analyzed::{AnalyzedWorkflow, TaskId, TaskTable};

/// Read-only workflow metadata — immutable after construction.
///
/// Created once from `AnalyzedWorkflow` and wrapped in `Arc` for sharing
/// across runner, executor, and spawned tasks.
#[derive(Debug)]
pub struct WorkflowMeta {
    /// Bidirectional TaskId ↔ name mapping.
    task_table: TaskTable,

    /// Default provider for the workflow.
    provider: Option<String>,

    /// Default model for the workflow.
    model: Option<String>,
}

impl WorkflowMeta {
    /// Create from an AnalyzedWorkflow, wrapped in Arc for sharing.
    pub fn from_workflow(wf: &AnalyzedWorkflow) -> Arc<Self> {
        Arc::new(Self {
            task_table: wf.task_table.clone(),
            provider: wf.provider.clone(),
            model: wf.model.clone(),
        })
    }

    /// Resolve TaskId to human-readable name. Panics if ID is invalid.
    ///
    /// This should only be called with IDs that originated from the analyzer,
    /// which guarantees validity.
    pub fn task_name(&self, id: TaskId) -> &str {
        self.task_table
            .get_name(id)
            .expect("TaskId must be valid — created by analyzer")
    }

    /// Resolve name to TaskId. Returns None for unknown names.
    pub fn task_id(&self, name: &str) -> Option<TaskId> {
        self.task_table.get_id(name)
    }

    /// Default provider for the workflow.
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    /// Default model for the workflow.
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// Access the task table directly.
    pub fn task_table(&self) -> &TaskTable {
        &self.task_table
    }

    /// Number of tasks in the workflow.
    pub fn task_count(&self) -> usize {
        self.task_table.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::AnalyzedWorkflow;

    fn make_workflow(
        tasks: &[&str],
        provider: Option<&str>,
        model: Option<&str>,
    ) -> AnalyzedWorkflow {
        let mut wf = AnalyzedWorkflow::default();
        for name in tasks {
            wf.task_table.insert(name);
        }
        wf.provider = provider.map(String::from);
        wf.model = model.map(String::from);
        wf
    }

    #[test]
    fn from_workflow_roundtrip() {
        let wf = make_workflow(
            &["step1", "step2", "step3"],
            Some("anthropic"),
            Some("claude-sonnet-4-6"),
        );
        let ctx = WorkflowMeta::from_workflow(&wf);

        assert_eq!(ctx.task_count(), 3);
        assert_eq!(ctx.provider(), Some("anthropic"));
        assert_eq!(ctx.model(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn task_name_roundtrip() {
        let wf = make_workflow(&["alpha", "beta"], None, None);
        let ctx = WorkflowMeta::from_workflow(&wf);

        let id_alpha = ctx.task_id("alpha").unwrap();
        let id_beta = ctx.task_id("beta").unwrap();

        assert_eq!(ctx.task_name(id_alpha), "alpha");
        assert_eq!(ctx.task_name(id_beta), "beta");
    }

    #[test]
    fn unknown_name_returns_none() {
        let wf = make_workflow(&["task1"], None, None);
        let ctx = WorkflowMeta::from_workflow(&wf);

        assert!(ctx.task_id("nonexistent").is_none());
    }

    #[test]
    fn provider_and_model_none() {
        let wf = make_workflow(&["task1"], None, None);
        let ctx = WorkflowMeta::from_workflow(&wf);

        assert!(ctx.provider().is_none());
        assert!(ctx.model().is_none());
    }

    #[test]
    fn task_table_accessible() {
        let wf = make_workflow(&["a", "b", "c"], None, None);
        let ctx = WorkflowMeta::from_workflow(&wf);

        let table = ctx.task_table();
        assert_eq!(table.len(), 3);
        assert!(table.contains("a"));
        assert!(table.contains("b"));
        assert!(table.contains("c"));
    }

    #[test]
    fn arc_sharing() {
        let wf = make_workflow(&["task1"], Some("openai"), None);
        let ctx = WorkflowMeta::from_workflow(&wf);

        let ctx2 = Arc::clone(&ctx);
        assert_eq!(ctx2.task_name(TaskId::new(0)), "task1");
        assert_eq!(ctx2.provider(), Some("openai"));
    }
}
