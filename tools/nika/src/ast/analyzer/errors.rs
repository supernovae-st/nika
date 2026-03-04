//! Analysis error types with span tracking.
//!
//! These errors are produced during Phase 2 analysis and include
//! precise source locations for IDE integration.

use crate::source::Span;

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
    /// Unknown task reference (e.g., in `use:` or `flow:`)
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
    /// Invalid template expression
    InvalidTemplate,
    /// Unknown flow definition
    UnknownFlow,
    /// Unknown MCP server
    UnknownMcpServer,
    /// Feature not available in this schema version
    UnsupportedFeature,
}

impl AnalyzeErrorKind {
    /// Get the error code in NIKA-XXX format.
    ///
    /// AST analysis errors use range NIKA-140-149:
    /// - NIKA-140: Unknown task reference
    /// - NIKA-141: Duplicate task ID
    /// - NIKA-142: Invalid schema version
    /// - NIKA-143: Cyclic dependency
    /// - NIKA-144: Invalid field value
    /// - NIKA-145: Missing required field
    /// - NIKA-146: Invalid template expression
    /// - NIKA-147: Unknown flow definition
    /// - NIKA-148: Unknown MCP server
    /// - NIKA-149: Feature not available in schema version
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownTask => "NIKA-140",
            Self::DuplicateTask => "NIKA-141",
            Self::InvalidSchema => "NIKA-142",
            Self::CyclicDependency => "NIKA-143",
            Self::InvalidValue => "NIKA-144",
            Self::MissingField => "NIKA-145",
            Self::InvalidTemplate => "NIKA-146",
            Self::UnknownFlow => "NIKA-147",
            Self::UnknownMcpServer => "NIKA-148",
            Self::UnsupportedFeature => "NIKA-149",
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
        // AST analysis errors use NIKA-140-149 range
        assert_eq!(AnalyzeErrorKind::UnknownTask.code(), "NIKA-140");
        assert_eq!(AnalyzeErrorKind::DuplicateTask.code(), "NIKA-141");
        assert_eq!(AnalyzeErrorKind::InvalidSchema.code(), "NIKA-142");
        assert_eq!(AnalyzeErrorKind::CyclicDependency.code(), "NIKA-143");
        assert_eq!(AnalyzeErrorKind::InvalidValue.code(), "NIKA-144");
        assert_eq!(AnalyzeErrorKind::MissingField.code(), "NIKA-145");
        assert_eq!(AnalyzeErrorKind::InvalidTemplate.code(), "NIKA-146");
        assert_eq!(AnalyzeErrorKind::UnknownFlow.code(), "NIKA-147");
        assert_eq!(AnalyzeErrorKind::UnknownMcpServer.code(), "NIKA-148");
        assert_eq!(AnalyzeErrorKind::UnsupportedFeature.code(), "NIKA-149");
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
}
