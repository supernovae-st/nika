//! Diagnostics conversion from Nika analyzer to LSP.
//!
//! Converts `AnalyzeError` to LSP `Diagnostic` for real-time error display.

use nika_engine::ast::analyzer::{AnalyzeError, AnalyzeErrorKind};
use nika_engine::source::{SourceRegistry, Span};
use tower_lsp_server::ls_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Range, Uri,
};

use crate::document::DocumentState;

/// Convert a Nika AnalyzeError to an LSP Diagnostic.
pub fn to_lsp_diagnostic(
    error: &AnalyzeError,
    doc: &DocumentState,
    uri: &Uri,
    _registry: Option<&SourceRegistry>,
) -> Diagnostic {
    let range = span_to_range(&error.span, doc);

    let severity = match error.kind {
        // Errors that prevent execution
        AnalyzeErrorKind::UnknownTask
        | AnalyzeErrorKind::DuplicateTask
        | AnalyzeErrorKind::InvalidSchema
        | AnalyzeErrorKind::CyclicDependency
        | AnalyzeErrorKind::MissingField
        | AnalyzeErrorKind::MissingModel => DiagnosticSeverity::ERROR,

        // Errors that might cause runtime issues
        AnalyzeErrorKind::InvalidValue
        | AnalyzeErrorKind::InvalidBinding
        | AnalyzeErrorKind::UnsupportedFeature => DiagnosticSeverity::ERROR,
    };

    let mut message = error.message.clone();

    // Add suggestion if available
    if let Some(ref suggestion) = error.suggestion {
        message.push_str(&format!("\n\n{}", suggestion));
    }

    // Build related information for notes
    let related_information = error.note.as_ref().map(|note: &String| {
        vec![DiagnosticRelatedInformation {
            location: Location {
                uri: uri.clone(),
                range,
            },
            message: note.clone(),
        }]
    });

    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(tower_lsp_server::ls_types::NumberOrString::String(
            error.kind.code().to_string(),
        )),
        code_description: None,
        source: Some("nika".to_string()),
        message,
        related_information,
        tags: None,
        data: None,
    }
}

/// Convert a Nika Span to an LSP Range.
pub fn span_to_range(span: &Span, doc: &DocumentState) -> Range {
    if span.is_dummy() {
        // Dummy span: point to start of document
        return Range {
            start: tower_lsp_server::ls_types::Position {
                line: 0,
                character: 0,
            },
            end: tower_lsp_server::ls_types::Position {
                line: 0,
                character: 1,
            },
        };
    }

    let start = doc.byte_offset_to_position(span.start.as_usize());
    let end = doc.byte_offset_to_position(span.end.as_usize());

    Range { start, end }
}

/// Validate a document and return diagnostics.
pub fn validate_document(content: &str, uri: &Uri) -> Vec<Diagnostic> {
    use nika_engine::ast::analyzer::analyze;
    use nika_engine::ast::raw;
    use nika_engine::source::FileId;

    use crate::template_validation::validate_templates;

    let mut diagnostics = Vec::new();

    // Create a temporary document state for position conversion
    let doc = DocumentState::new(content.to_string(), 0);

    // Phase 1: Parse to raw AST
    let raw_result = raw::parse(content, FileId(0));

    match raw_result {
        Ok(raw_workflow) => {
            // Phase 2: Analyze
            let analyze_result = analyze(raw_workflow.clone());

            // Convert errors to diagnostics
            for error in &analyze_result.errors {
                diagnostics.push(to_lsp_diagnostic(error, &doc, uri, None));
            }

            // Convert warnings to diagnostics
            for warning in &analyze_result.warnings {
                let mut diag = to_lsp_diagnostic(warning, &doc, uri, None);
                diag.severity = Some(DiagnosticSeverity::WARNING);
                diagnostics.push(diag);
            }

            // Phase 3: Template validation ({{use.*}} references)
            if let Some(ref analyzed) = analyze_result.value {
                let template_diagnostics = validate_templates(&raw_workflow, analyzed, &doc);
                diagnostics.extend(template_diagnostics);
            }
        }
        Err(parse_error) => {
            // Parse error - convert to diagnostic
            let range = span_to_range(&parse_error.span, &doc);

            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(tower_lsp_server::ls_types::NumberOrString::String(
                    parse_error.kind.code().to_string(),
                )),
                code_description: None,
                source: Some("nika".to_string()),
                message: parse_error.message,
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_valid_workflow() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri);

        // Should have no errors for valid workflow
        assert!(
            diagnostics.is_empty(),
            "Expected no errors, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_validate_unknown_task_reference() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      data: $unknown_task
    infer: "World"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri);

        // Should have error for unknown task reference
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("unknown"));
    }

    #[test]
    fn test_validate_invalid_schema() {
        let content = r#"
schema: "invalid-schema"
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri);

        // Should have error for invalid schema
        assert!(!diagnostics.is_empty());
    }
}
