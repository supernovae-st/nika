// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Analysis error types with span tracking.
//!
//! These errors are produced during Phase 2 analysis and include
//! precise source locations for IDE integration.
//!
//! ## Rich Diagnostics
//!
//! For terminal display with source code snippets, wrap errors with
//! [`RichAnalyzeError`] which carries the source content:
//!
//! ```ignore
//! let rich = RichAnalyzeError::new(error, source_code, filename);
//! eprintln!("{:?}", miette::Report::new(rich));
//! ```

use crate::source::Span;
use miette::{Diagnostic, SourceSpan};

/// An error that occurred during analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzeError {
    /// The error kind
    pub kind: AnalyzeErrorKind,
    /// The span where the error occurred
    pub span: Span,
    /// Human-readable error message
    pub message: String,
    /// Optional suggestion (for "did you mean?" hints)
    pub suggestion: Option<String>,
    /// Optional note (additional context)
    pub note: Option<String>,
}

impl AnalyzeError {
    /// Create a new error.
    pub fn new(kind: AnalyzeErrorKind, span: Span, message: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            message: message.into(),
            suggestion: None,
            note: None,
        }
    }

    /// Add a "did you mean?" suggestion.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Add a note.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Create an "unknown task" error with optional suggestion.
    pub fn unknown_task(span: Span, name: &str, suggestion: Option<&str>) -> Self {
        let mut err = Self::new(
            AnalyzeErrorKind::UnknownTask,
            span,
            format!("unknown task '{}'", name),
        );
        if let Some(s) = suggestion {
            err = err.with_suggestion(format!("did you mean '{}'?", s));
        }
        err
    }

    /// Create a "duplicate task" error.
    pub fn duplicate_task(span: Span, name: &str, first_span: Span) -> Self {
        Self::new(
            AnalyzeErrorKind::DuplicateTask,
            span,
            format!("duplicate task id '{}'", name),
        )
        .with_note(format!("first defined at {:?}", first_span))
    }

    /// Create an "invalid schema" error.
    pub fn invalid_schema(span: Span, schema: &str, suggestion: Option<&str>) -> Self {
        let mut err = Self::new(
            AnalyzeErrorKind::InvalidSchema,
            span,
            format!("invalid schema version '{}'", schema),
        );
        if let Some(s) = suggestion {
            err = err.with_suggestion(format!("did you mean '{}'?", s));
        }
        err
    }

    /// Create a "cyclic dependency" error.
    pub fn cyclic_dependency(span: Span, cycle: &[&str]) -> Self {
        Self::new(
            AnalyzeErrorKind::CyclicDependency,
            span,
            format!("cyclic dependency detected: {}", cycle.join(" → ")),
        )
    }

    /// Create an "invalid value" error.
    pub fn invalid_value(span: Span, field: &str, value: &str, expected: &str) -> Self {
        Self::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            format!("invalid {} '{}', expected {}", field, value, expected),
        )
    }

    /// Create a "missing field" error.
    pub fn missing_field(span: Span, field: &str) -> Self {
        Self::new(
            AnalyzeErrorKind::MissingField,
            span,
            format!("missing required field '{}'", field),
        )
    }

    /// Create an "unsupported feature" error for schema version gating.
    pub fn unsupported_feature(
        span: Span,
        feature: &str,
        current_version: &str,
        required_version: &str,
    ) -> Self {
        Self::new(
            AnalyzeErrorKind::UnsupportedFeature,
            span,
            format!(
                "'{}' requires schema version {} or later, but workflow uses {}",
                feature, required_version, current_version
            ),
        )
        .with_suggestion(format!("upgrade to schema: {}", required_version))
    }

    /// Create an "invalid binding" error for malformed `with:` expressions.
    pub fn invalid_binding(span: Span, input: &str, reason: &str) -> Self {
        Self::new(
            AnalyzeErrorKind::InvalidBinding,
            span,
            format!("invalid binding expression '{}': {}", input, reason),
        )
    }
}

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, " ({})", suggestion)?;
        }
        Ok(())
    }
}

impl std::error::Error for AnalyzeError {}

/// Kinds of analysis errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzeErrorKind {
    /// Unknown task reference (e.g., in `with:` or `depends_on:`)
    UnknownTask,
    /// Duplicate task ID
    DuplicateTask,
    /// Invalid schema version
    InvalidSchema,
    /// Cyclic dependency between tasks
    CyclicDependency,
    /// Invalid field value
    InvalidValue,
    /// Missing required field
    MissingField,
    /// Feature not available in this schema version
    UnsupportedFeature,
    /// Invalid binding expression in `with:` block
    InvalidBinding,
    /// NIKA-034: model: required when infer/agent verb is used
    MissingModel,
    /// NIKA-290: on_error: fallback references a task that does not exist
    UnknownOnErrorFallback,
}

impl AnalyzeErrorKind {
    /// Get the error code in NIKA-XXX format.
    ///
    /// AST analysis errors use range NIKA-140-150:
    /// - NIKA-034: model: required for LLM verbs
    /// - NIKA-140: Unknown task reference
    /// - NIKA-141: Duplicate task ID
    /// - NIKA-142: Invalid schema version
    /// - NIKA-143: Cyclic dependency
    /// - NIKA-144: Invalid field value
    /// - NIKA-145: Missing required field
    /// - NIKA-149: Feature not available in schema version
    /// - NIKA-151: Invalid binding expression
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownTask => "NIKA-140",
            Self::DuplicateTask => "NIKA-141",
            Self::InvalidSchema => "NIKA-142",
            Self::CyclicDependency => "NIKA-143",
            Self::InvalidValue => "NIKA-144",
            Self::MissingField => "NIKA-145",
            Self::UnsupportedFeature => "NIKA-149",
            Self::InvalidBinding => "NIKA-151",
            Self::MissingModel => "NIKA-034",
            Self::UnknownOnErrorFallback => "NIKA-290",
        }
    }
}

/// Result of analysis - either an analyzed workflow or collected errors.
#[derive(Debug)]
pub struct AnalyzeResult<T> {
    /// The result (if analysis succeeded)
    pub value: Option<T>,
    /// All errors encountered during analysis
    pub errors: Vec<AnalyzeError>,
    /// All warnings (non-fatal issues)
    pub warnings: Vec<AnalyzeError>,
}

impl<T> AnalyzeResult<T> {
    /// Create a successful result.
    pub fn ok(value: T) -> Self {
        Self {
            value: Some(value),
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a failed result with errors.
    pub fn err(errors: Vec<AnalyzeError>) -> Self {
        Self {
            value: None,
            errors,
            warnings: Vec::new(),
        }
    }

    /// Check if analysis succeeded (no errors).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty() && self.value.is_some()
    }

    /// Check if analysis failed.
    pub fn is_err(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Get the value if successful.
    pub fn into_result(self) -> Result<T, Vec<AnalyzeError>> {
        if self.errors.is_empty() {
            self.value.ok_or_else(Vec::new)
        } else {
            Err(self.errors)
        }
    }

    /// Add an error.
    pub fn add_error(&mut self, error: AnalyzeError) {
        self.errors.push(error);
    }

    /// Add a warning.
    pub fn add_warning(&mut self, warning: AnalyzeError) {
        self.warnings.push(warning);
    }
}

// ============================================================================
// Rich Diagnostic Wrapper for Terminal Display
// ============================================================================

/// A wrapper that combines [`AnalyzeError`] with source code for rich terminal display.
///
/// This implements [`miette::Diagnostic`] to show:
/// - Error code (NIKA-140-151)
/// - Source code snippet with highlighted span
/// - "Did you mean?" suggestions
/// - Additional notes
///
/// # Example
///
/// ```ignore
/// use nika::ast::analyzer::errors::{AnalyzeError, RichAnalyzeError};
///
/// let error = AnalyzeError::unknown_task(span, "taks1", Some("task1"));
/// let rich = RichAnalyzeError::new(error, source_code, "workflow.nika.yaml");
///
/// // Display with miette's fancy formatting
/// eprintln!("{:?}", miette::Report::new(rich));
/// ```
#[derive(Debug)]
pub struct RichAnalyzeError {
    /// The underlying analysis error
    error: AnalyzeError,
    /// Source code content for display
    source_code: String,
    /// Filename for display
    filename: String,
    /// Miette source span (derived from error.span)
    label_span: SourceSpan,
}

impl RichAnalyzeError {
    /// Create a rich error from an [`AnalyzeError`] with source context.
    pub fn new(
        error: AnalyzeError,
        source_code: impl Into<String>,
        filename: impl Into<String>,
    ) -> Self {
        let source = source_code.into();
        let label_span = SourceSpan::new(error.span.start.as_usize().into(), error.span.len());
        Self {
            error,
            source_code: source,
            filename: filename.into(),
            label_span,
        }
    }

    /// Get the underlying error.
    pub fn error(&self) -> &AnalyzeError {
        &self.error
    }

    /// Get the source code.
    pub fn source(&self) -> &str {
        &self.source_code
    }

    /// Get the filename.
    pub fn filename(&self) -> &str {
        &self.filename
    }
}

impl std::fmt::Display for RichAnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.error.kind.code(), self.error.message)
    }
}

impl std::error::Error for RichAnalyzeError {}

impl Diagnostic for RichAnalyzeError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(format!(
            "nika::{}",
            self.error.kind.code().to_lowercase().replace('-', "_")
        )))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.error
            .suggestion
            .as_ref()
            .map(|s| Box::new(s.clone()) as Box<dyn std::fmt::Display>)
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(format!(
            "https://nika.sh/errors/{}",
            self.error.kind.code()
        )))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_code)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        let label =
            miette::LabeledSpan::new_with_span(Some(self.error.message.clone()), self.label_span);
        Some(Box::new(std::iter::once(label)))
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        None
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        None
    }
}

/// Convert multiple [`AnalyzeError`]s to rich diagnostics.
///
/// # Example
///
/// ```ignore
/// let rich_errors = to_rich_diagnostics(errors, &source_code, "workflow.nika.yaml");
/// for err in rich_errors {
///     eprintln!("{:?}", miette::Report::new(err));
/// }
/// ```
pub fn to_rich_diagnostics(
    errors: Vec<AnalyzeError>,
    source_code: &str,
    filename: &str,
) -> Vec<RichAnalyzeError> {
    errors
        .into_iter()
        .map(|e| RichAnalyzeError::new(e, source_code.to_string(), filename.to_string()))
        .collect()
}

/// Format a single error for display using miette.
///
/// Returns a formatted string suitable for terminal output.
pub fn format_error(error: &AnalyzeError, source_code: &str, filename: &str) -> String {
    let rich = RichAnalyzeError::new(error.clone(), source_code, filename);
    format!("{:?}", miette::Report::new(rich))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn make_span(start: u32, end: u32) -> Span {
        Span::new(FileId(0), start, end)
    }

    #[test]
    fn test_analyze_error_display() {
        let err = AnalyzeError::unknown_task(make_span(10, 20), "taks1", Some("task1"));
        assert_eq!(
            format!("{}", err),
            "unknown task 'taks1' (did you mean 'task1'?)"
        );
    }

    #[test]
    fn test_analyze_error_duplicate() {
        let err = AnalyzeError::duplicate_task(make_span(100, 110), "my-task", make_span(10, 20));
        assert!(format!("{}", err).contains("duplicate task id 'my-task'"));
    }

    #[test]
    fn test_analyze_error_cyclic() {
        let err = AnalyzeError::cyclic_dependency(make_span(0, 10), &["a", "b", "c", "a"]);
        assert!(format!("{}", err).contains("a → b → c → a"));
    }

    #[test]
    fn test_error_kind_codes() {
        // AST analysis errors use NIKA-140-150 range
        assert_eq!(AnalyzeErrorKind::UnknownTask.code(), "NIKA-140");
        assert_eq!(AnalyzeErrorKind::DuplicateTask.code(), "NIKA-141");
        assert_eq!(AnalyzeErrorKind::InvalidSchema.code(), "NIKA-142");
        assert_eq!(AnalyzeErrorKind::CyclicDependency.code(), "NIKA-143");
        assert_eq!(AnalyzeErrorKind::InvalidValue.code(), "NIKA-144");
        assert_eq!(AnalyzeErrorKind::MissingField.code(), "NIKA-145");
        assert_eq!(AnalyzeErrorKind::UnsupportedFeature.code(), "NIKA-149");
        assert_eq!(AnalyzeErrorKind::InvalidBinding.code(), "NIKA-151");
        assert_eq!(AnalyzeErrorKind::MissingModel.code(), "NIKA-034");
    }

    #[test]
    fn test_analyze_result_ok() {
        let result: AnalyzeResult<i32> = AnalyzeResult::ok(42);
        assert!(result.is_ok());
        assert!(!result.is_err());
        assert_eq!(result.into_result(), Ok(42));
    }

    #[test]
    fn test_analyze_result_err() {
        let errors = vec![AnalyzeError::new(
            AnalyzeErrorKind::UnknownTask,
            make_span(0, 5),
            "test error",
        )];
        let result: AnalyzeResult<i32> = AnalyzeResult::err(errors);
        assert!(!result.is_ok());
        assert!(result.is_err());
        assert!(result.into_result().is_err());
    }

    // =========================================================================
    // RichAnalyzeError Tests
    // =========================================================================

    #[test]
    fn test_rich_error_creation() {
        let source = "tasks:\n  - id: step1\n    with:\n      data: taks1\n";
        let err = AnalyzeError::unknown_task(make_span(40, 45), "taks1", Some("task1"));
        let rich = RichAnalyzeError::new(err, source, "test.nika.yaml");

        assert_eq!(rich.filename(), "test.nika.yaml");
        assert_eq!(rich.source(), source);
        assert_eq!(rich.error().kind, AnalyzeErrorKind::UnknownTask);
    }

    #[test]
    fn test_rich_error_display() {
        let err = AnalyzeError::unknown_task(make_span(10, 15), "taks1", Some("task1"));
        let rich = RichAnalyzeError::new(err, "some source", "test.yaml");

        let display = format!("{}", rich);
        assert!(display.contains("NIKA-140"));
        assert!(display.contains("unknown task 'taks1'"));
    }

    #[test]
    fn test_rich_error_diagnostic_code() {
        let err = AnalyzeError::unknown_task(make_span(0, 5), "x", None);
        let rich = RichAnalyzeError::new(err, "source", "file.yaml");

        let code = Diagnostic::code(&rich).unwrap();
        assert_eq!(format!("{}", code), "nika::nika_140");
    }

    #[test]
    fn test_rich_error_diagnostic_help() {
        let err = AnalyzeError::unknown_task(make_span(0, 5), "taks1", Some("task1"));
        let rich = RichAnalyzeError::new(err, "source", "file.yaml");

        let help = Diagnostic::help(&rich).unwrap();
        assert_eq!(format!("{}", help), "did you mean 'task1'?");
    }

    #[test]
    fn test_rich_error_diagnostic_no_help() {
        let err = AnalyzeError::unknown_task(make_span(0, 5), "x", None);
        let rich = RichAnalyzeError::new(err, "source", "file.yaml");

        assert!(Diagnostic::help(&rich).is_none());
    }

    #[test]
    fn test_rich_error_diagnostic_url() {
        let err = AnalyzeError::new(AnalyzeErrorKind::CyclicDependency, make_span(0, 5), "cycle");
        let rich = RichAnalyzeError::new(err, "source", "file.yaml");

        let url = Diagnostic::url(&rich).unwrap();
        assert_eq!(format!("{}", url), "https://nika.sh/errors/NIKA-143");
    }

    #[test]
    fn test_rich_error_has_source_code() {
        let source = "schema: nika/workflow@0.12\ntasks:\n  - id: test\n";
        let err = AnalyzeError::missing_field(make_span(30, 40), "infer");
        let rich = RichAnalyzeError::new(err, source, "test.yaml");

        assert!(Diagnostic::source_code(&rich).is_some());
    }

    #[test]
    fn test_rich_error_labels() {
        let err = AnalyzeError::unknown_task(make_span(10, 20), "bad_task", None);
        let rich = RichAnalyzeError::new(err, "0123456789bad_task90", "test.yaml");

        let labels: Vec<_> = Diagnostic::labels(&rich).unwrap().collect();
        assert_eq!(labels.len(), 1);

        // The label span should match our error span
        let span = labels[0].inner();
        assert_eq!(span.offset(), 10);
        assert_eq!(span.len(), 10);
    }

    #[test]
    fn test_to_rich_diagnostics() {
        let errors = vec![
            AnalyzeError::unknown_task(make_span(10, 15), "a", None),
            AnalyzeError::duplicate_task(make_span(20, 25), "b", make_span(0, 5)),
        ];
        let source = "0123456789abcdefghijklmnopqrstuvwxyz";

        let rich = to_rich_diagnostics(errors, source, "test.yaml");
        assert_eq!(rich.len(), 2);
        assert_eq!(rich[0].error().kind, AnalyzeErrorKind::UnknownTask);
        assert_eq!(rich[1].error().kind, AnalyzeErrorKind::DuplicateTask);
    }

    #[test]
    fn test_format_error_output() {
        let source = r#"schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      data: taks1
    infer: "Process {{with.data}}"
"#;
        // "taks1" starts at byte ~96 in this source
        let err = AnalyzeError::unknown_task(make_span(96, 101), "taks1", Some("task1"));
        let output = format_error(&err, source, "workflow.nika.yaml");

        // The output should contain the error code and message
        assert!(output.contains("NIKA-140") || output.contains("nika_140"));
        assert!(output.contains("unknown task"));
    }

    #[test]
    fn test_rich_error_all_kinds() {
        // Ensure all error kinds work with RichAnalyzeError
        let kinds = [
            (AnalyzeErrorKind::UnknownTask, "NIKA-140"),
            (AnalyzeErrorKind::DuplicateTask, "NIKA-141"),
            (AnalyzeErrorKind::InvalidSchema, "NIKA-142"),
            (AnalyzeErrorKind::CyclicDependency, "NIKA-143"),
            (AnalyzeErrorKind::InvalidValue, "NIKA-144"),
            (AnalyzeErrorKind::MissingField, "NIKA-145"),
            (AnalyzeErrorKind::UnsupportedFeature, "NIKA-149"),
            (AnalyzeErrorKind::InvalidBinding, "NIKA-151"),
            (AnalyzeErrorKind::MissingModel, "NIKA-034"),
        ];

        for (kind, expected_code) in kinds {
            let err = AnalyzeError::new(kind, make_span(0, 5), "test message");
            let rich = RichAnalyzeError::new(err, "source", "file.yaml");

            let code = Diagnostic::code(&rich).unwrap();
            let code_str = format!("{}", code);
            assert!(
                code_str.contains(&expected_code.to_lowercase().replace('-', "_")),
                "Expected code {} in {}, got {}",
                expected_code,
                kind.code(),
                code_str
            );
        }
    }
}
