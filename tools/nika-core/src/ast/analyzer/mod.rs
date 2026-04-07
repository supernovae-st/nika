//! Analyzer module - Phase 2 transformation.
//!
//! This module provides the core analysis functionality that transforms
//! raw::Workflow (parsed from YAML) into analyzed::Workflow (validated,
//! references resolved).
//!
//! # Two-Phase Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────┐
//! │  Phase 1: Parsing (ast::raw)                                       │
//! │  - marked_yaml parses YAML with Span tracking                      │
//! │  - All values are strings, unresolved                              │
//! │  - Output: raw::Workflow with Spanned<T> fields                    │
//! └──────────────────────────┬─────────────────────────────────────────┘
//!                            │
//!                            │ analyze()
//!                            │
//!                            ▼
//! ┌────────────────────────────────────────────────────────────────────┐
//! │  Phase 2: Analysis (this module)                                   │
//! │  - Validates schema version                                        │
//! │  - Builds task table (TaskId interning)                            │
//! │  - Resolves all references (with:, depends_on:)                     │
//! │  - Detects cyclic dependencies                                     │
//! │  - Collects errors with precise spans                              │
//! │  - Output: analyzed::Workflow with TaskId fields                   │
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Error Handling
//!
//! The analyzer collects ALL errors in a single pass (not fail-fast).
//! This allows IDEs to show all problems at once.
//!
//! ```ignore
//! let result = analyze(raw_workflow);
//! if result.is_err() {
//!     for error in &result.errors {
//!         // error.span has precise location
//!         // error.suggestion may have "did you mean?"
//!         eprintln!("{}: {}", error.kind.code(), error);
//!     }
//! }
//! ```
//!
//! # Suggestion Engine
//!
//! Uses Jaro-Winkler similarity (strsim) for "did you mean?" suggestions:
//!
//! - Unknown task "taks1" → did you mean "task1"?
//! - Invalid schema "nika/workfow@0.10" → did you mean "nika/workflow@0.12"?

mod analyze;
mod errors;
pub mod suggestions;
pub mod taint;

pub use analyze::{analyze, validate};
pub use errors::{
    format_error, to_rich_diagnostics, AnalyzeError, AnalyzeErrorKind, AnalyzeResult,
    RichAnalyzeError,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::raw::RawWorkflow;
    use crate::source::{FileId, Span, Spanned};

    #[test]
    fn test_module_exports() {
        // Smoke test that all types are accessible
        let _ = AnalyzeErrorKind::UnknownTask;
        let _ = AnalyzeError::new(AnalyzeErrorKind::UnknownTask, Span::dummy(), "test");
    }

    #[test]
    fn test_analyze_empty_workflow_is_rejected() {
        let raw = RawWorkflow {
            schema: Spanned::new(
                "nika/workflow@0.12".to_string(),
                Span::new(FileId(0), 0, 18),
            ),
            ..Default::default()
        };

        let result = analyze(raw);
        assert!(result.is_err(), "empty tasks array should be rejected");
    }
}
