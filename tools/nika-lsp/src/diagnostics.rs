//! Diagnostics conversion from Nika analyzer to LSP.
//!
//! Converts `AnalyzeError` to LSP `Diagnostic` for real-time error display.

use nika_core::ast::analyzer::{AnalyzeError, AnalyzeErrorKind};
use nika_core::source::{SourceRegistry, Span};
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
        | AnalyzeErrorKind::MissingModel
        | AnalyzeErrorKind::UnknownOnErrorFallback => DiagnosticSeverity::ERROR,

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
pub fn validate_document(
    content: &str,
    uri: &Uri,
    daemon_providers: Option<&[nika_core::catalogs::ProviderStatusInfo]>,
) -> Vec<Diagnostic> {
    use nika_core::ast::analyzer::analyze;
    use nika_core::ast::raw;
    use nika_core::source::FileId;

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

            // Phase 4: Empty tasks warning
            if let Some(ref analyzed) = analyze_result.value {
                if analyzed.tasks.is_empty() {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: tower_lsp_server::ls_types::Position {
                                line: 0,
                                character: 0,
                            },
                            end: tower_lsp_server::ls_types::Position {
                                line: 0,
                                character: 1,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(tower_lsp_server::ls_types::NumberOrString::String(
                            "NIKA-145".to_string(),
                        )),
                        code_description: None,
                        source: Some("nika".to_string()),
                        message: "workflow has no tasks. Add at least one task under 'tasks:'"
                            .to_string(),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            }

            // Phase 5: Provider API key availability (daemon-aware)
            if let Some(ref provider_spanned) = raw_workflow.provider {
                let provider_str = provider_spanned.value.as_str();

                // Check daemon first (covers vault + env), fallback to env-only check
                let has_key = if let Some(dp) = daemon_providers {
                    if let Some(info) = dp.iter().find(|p| p.id == provider_str) {
                        info.has_key
                    } else {
                        // Unknown provider in daemon data — skip warning
                        true
                    }
                } else {
                    // No daemon — env-only check (original behavior)
                    let env_var = match provider_str {
                        "anthropic" => Some("ANTHROPIC_API_KEY"),
                        "openai" => Some("OPENAI_API_KEY"),
                        "mistral" => Some("MISTRAL_API_KEY"),
                        "groq" => Some("GROQ_API_KEY"),
                        "deepseek" => Some("DEEPSEEK_API_KEY"),
                        "gemini" => Some("GEMINI_API_KEY"),
                        "xai" => Some("XAI_API_KEY"),
                        _ => None,
                    };
                    match env_var {
                        Some(var) => !std::env::var(var).map(|v| v.is_empty()).unwrap_or(true),
                        None => true, // mock, native — no key needed
                    }
                };

                if !has_key {
                    let env_var =
                        nika_core::catalogs::provider_to_env_var(provider_str).unwrap_or("API_KEY");
                    let range = span_to_range(&provider_spanned.span, &doc);
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(tower_lsp_server::ls_types::NumberOrString::String(
                            "NIKA-031".to_string(),
                        )),
                        code_description: None,
                        source: Some("nika".to_string()),
                        message: format!(
                            "no API key found for provider '{}'. Set {} or run `nika keys set {}`",
                            provider_str, env_var, provider_str
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
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
        let diagnostics = validate_document(content, &uri, None);

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
        let diagnostics = validate_document(content, &uri, None);

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
        let diagnostics = validate_document(content, &uri, None);

        // Should have error for invalid schema
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_validate_empty_tasks_warning() {
        let content = r#"schema: "nika/workflow@0.12"
workflow: test
provider: mock
tasks: []
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri, None);
        // The analyzer catches empty tasks as NIKA-144 error.
        // Our Phase 4 NIKA-145 warning is a defensive fallback for edge cases
        // where the analyzer returns a value with 0 tasks.
        // Either NIKA-144 or NIKA-145 should appear.
        let has_empty_tasks_diagnostic = diagnostics
            .iter()
            .any(|d| d.message.contains("must not be empty") || d.message.contains("no tasks"));
        assert!(
            has_empty_tasks_diagnostic,
            "Expected empty tasks diagnostic, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_validate_missing_provider_key_warning() {
        // Use xai provider which is unlikely to have a key in test env
        let content = r#"schema: "nika/workflow@0.12"
workflow: test
provider: xai
tasks:
  - id: step1
    infer: "test"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri, None);
        // Should not crash regardless of env state
        let _ = diagnostics;
    }

    #[test]
    fn test_validate_mock_provider_no_key_warning() {
        let content = r#"schema: "nika/workflow@0.12"
workflow: test
provider: mock
tasks:
  - id: step1
    infer: "test"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri, None);
        // mock provider should NOT trigger a key warning
        assert!(
            !diagnostics.iter().any(|d| d.message.contains("API key")),
            "mock provider should not trigger API key warning, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_validate_native_provider_no_key_warning() {
        let content = r#"schema: "nika/workflow@0.12"
workflow: test
provider: native
tasks:
  - id: step1
    infer: "test"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostics = validate_document(content, &uri, None);
        // native provider should NOT trigger a key warning
        assert!(
            !diagnostics.iter().any(|d| d.message.contains("API key")),
            "native provider should not trigger API key warning, got: {:?}",
            diagnostics
        );
    }
}
