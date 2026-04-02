//! Analyzed AST - Phase 2 of the Two-Phase IR Architecture.
//!
//! This module contains the validated, resolved AST types. All string
//! references have been replaced with interned identifiers (TaskId, etc.),
//! and all semantic validations have passed.
//!
//! # Two-Phase Architecture
//!
//! ```text
//! ┌─────────────────┐
//! │  YAML Source    │  workflow.nika.yaml
//! └────────┬────────┘
//!          │ marked_yaml
//!          ▼
//! ┌─────────────────┐
//! │  raw::Workflow  │  (see ast/raw/)
//! │  ├── Spanned<T> │  All nodes have line:col
//! │  └── strings    │  Unresolved references
//! └────────┬────────┘
//!          │ analyze()  ← Phase 2 happens here
//!          ▼
//! ┌─────────────────────┐
//! │ analyzed::Workflow  │  ← THIS MODULE
//! │  ├── TaskId(u32)    │  Interned identifiers
//! │  ├── resolved refs  │  All references validated
//! │  └── validated      │  No cycles, unique IDs
//! └─────────────────────┘
//! ```
//!
//! # Benefits
//!
//! 1. **O(1) Comparison**: TaskId(u32) vs String comparison
//! 2. **Memory Efficient**: Strings stored once in lookup tables
//! 3. **Validated**: No cycles, no duplicate IDs, valid schema
//! 4. **Ready for Execution**: Can be directly consumed by runtime
//!
//! # Example
//!
//! ```ignore
//! use nika::ast::raw;
//! use nika::ast::analyzed::{AnalyzedWorkflow, analyze};
//! use nika::source::SourceRegistry;
//!
//! let mut sources = SourceRegistry::new();
//! let file_id = sources.add_file("workflow.yaml", content);
//!
//! // Phase 1: Parse to raw AST
//! let raw_workflow = raw::parse(&content, file_id)?;
//!
//! // Phase 2: Analyze to resolved AST
//! let analyzed = analyze(&sources, raw_workflow)?;
//!
//! // Now use analyzed workflow
//! for task in analyzed.iter_tasks() {
//!     println!("Task {}: {:?}", task.name, task.action.verb_name());
//! }
//! ```

mod ids;
mod task;
mod workflow;

pub use ids::{TaskId, TaskTable};
pub use task::{
    AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedForEach,
    AnalyzedInferAction, AnalyzedInvokeAction, AnalyzedOutput, AnalyzedRetry, AnalyzedTask,
    AnalyzedTaskAction, HttpMethod, OutputFormat,
};
pub use workflow::{
    AnalyzedContextFile, AnalyzedIncludeSpec, AnalyzedMcpServer, AnalyzedWorkflow, McpFromSource,
    McpTransport, SchemaVersion,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports() {
        use crate::binding::WithSpec;

        // Basic smoke test that all types are accessible
        let _ = TaskId::new(0);
        let _ = AnalyzedWorkflow::default();
        let _ = SchemaVersion::latest();
        let _ = AnalyzedTask {
            id: TaskId::new(0),
            name: String::new(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,
            base_url: None,
            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
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
            span: crate::source::Span::dummy(),
        };
    }
}
