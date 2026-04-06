//! Template validation for Nika LSP.
//!
//! Validates that `{{with.alias}}` template references in task prompts
//! are defined in the task's `with:` block.

use nika_core::ast::analyzed::AnalyzedWorkflow;
use nika_core::ast::raw::{RawTaskAction, RawWorkflow};
use regex::Regex;
use std::sync::LazyLock;
use tower_lsp_server::ls_types::{Diagnostic, DiagnosticSeverity, Range};

use crate::document::DocumentState;

/// Regex to find `{{with.alias}}` patterns with their positions.
/// Captures the full match and the path (e.g., "alias.field").
/// Also matches legacy `{{use.alias}}` syntax for migration support.
static USE_TEMPLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*(?:with|use)\.(\w+(?:\.\w+)*)(?:\s*\|\s*\w+)?\s*\}\}").unwrap()
});

/// Extracted template reference with position information.
#[derive(Debug)]
struct TemplateRef {
    /// The alias name (first part of the path)
    alias: String,
    /// Full path (e.g., "alias.field")
    #[allow(dead_code)]
    full_path: String,
    /// Start offset within the string
    start: usize,
    /// End offset within the string
    end: usize,
}

/// Extract template references with positions from a string.
fn extract_refs_with_positions(text: &str) -> Vec<TemplateRef> {
    USE_TEMPLATE_RE
        .captures_iter(text)
        .filter_map(|cap| {
            let m = cap.get(0)?;
            let path = cap.get(1)?.as_str();
            let alias = path.split('.').next()?.to_string();
            Some(TemplateRef {
                alias,
                full_path: path.to_string(),
                start: m.start(),
                end: m.end(),
            })
        })
        .collect()
}

/// Validate template references in a workflow.
///
/// Checks that all `{{with.alias}}` references in task prompts
/// are defined in the task's `with:` block.
pub fn validate_templates(
    raw: &RawWorkflow,
    analyzed: &AnalyzedWorkflow,
    doc: &DocumentState,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Build a map of task id -> declared aliases
    let mut task_aliases: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for task in &analyzed.tasks {
        let aliases: Vec<String> = task.with_spec.keys().cloned().collect();
        task_aliases.insert(task.name.clone(), aliases);
    }

    // Check each raw task for template references
    for raw_task in &raw.tasks.value {
        let task_id = raw_task.value.id.value.clone();
        let declared_aliases = task_aliases.get(&task_id).cloned().unwrap_or_default();

        // Get prompt text and span based on action type
        let prompts_to_check = extract_prompts_from_action(&raw_task.value.action);

        for (prompt_text, prompt_span_start) in prompts_to_check {
            // Extract template refs with positions
            let refs = extract_refs_with_positions(&prompt_text);

            for template_ref in refs {
                if !declared_aliases.contains(&template_ref.alias) {
                    // Calculate the position in the document
                    let ref_start = prompt_span_start + template_ref.start;
                    let ref_end = prompt_span_start + template_ref.end;

                    let start_pos = doc.byte_offset_to_position(ref_start);
                    let end_pos = doc.byte_offset_to_position(ref_end);

                    let range = Range {
                        start: start_pos,
                        end: end_pos,
                    };

                    // Build suggestion with available aliases
                    let suggestion = if declared_aliases.is_empty() {
                        format!(
                            "No bindings declared in 'with:' block for task '{}'. \
                             Add a 'with:' block with alias definitions.",
                            task_id
                        )
                    } else {
                        format!("Available aliases: {}", declared_aliases.join(", "))
                    };

                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(tower_lsp_server::ls_types::NumberOrString::String(
                            "NIKA-141".to_string(),
                        )),
                        code_description: None,
                        source: Some("nika".to_string()),
                        message: format!(
                            "Undefined template binding '{{{{with.{}}}}}'\n\n{}",
                            template_ref.alias, suggestion
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            }
        }
    }

    diagnostics
}

/// Extract prompt texts and their starting positions from a task action.
///
/// Returns a list of (prompt_text, span_start_offset) tuples.
fn extract_prompts_from_action(action: &Option<RawTaskAction>) -> Vec<(String, usize)> {
    let mut prompts = Vec::new();

    match action {
        Some(RawTaskAction::Infer(infer)) => {
            // Main prompt
            prompts.push((
                infer.prompt.value.clone(),
                infer.prompt.span.start.as_usize(),
            ));

            // System prompt if present
            if let Some(ref system) = infer.system {
                prompts.push((system.value.clone(), system.span.start.as_usize()));
            }
        }
        Some(RawTaskAction::Agent(agent)) => {
            // Agent prompt
            prompts.push((
                agent.prompt.value.clone(),
                agent.prompt.span.start.as_usize(),
            ));
        }
        Some(RawTaskAction::Exec(exec)) => {
            // Exec command may have templates
            prompts.push((
                exec.command.value.clone(),
                exec.command.span.start.as_usize(),
            ));
        }
        Some(RawTaskAction::Fetch(fetch)) => {
            // URL may have templates
            prompts.push((fetch.url.value.clone(), fetch.url.span.start.as_usize()));

            // Body if present
            if let Some(ref body) = fetch.body {
                // Body is a serde_json::Value, convert to string for template checking
                let body_str = serde_json::to_string(&body.value).unwrap_or_default();
                prompts.push((body_str, body.span.start.as_usize()));
            }
        }
        Some(RawTaskAction::Invoke(invoke)) => {
            // Params may have templates
            if let Some(ref params) = invoke.params {
                let params_str = serde_json::to_string(&params.value).unwrap_or_default();
                prompts.push((params_str, params.span.start.as_usize()));
            }
        }
        None => {}
    }

    prompts
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_core::ast::analyzer::analyze;
    use nika_core::ast::raw;
    use nika_core::source::FileId;

    fn validate_yaml(content: &str) -> Vec<Diagnostic> {
        let doc = DocumentState::new(content.to_string(), 0);
        let raw_workflow = match raw::parse(content, FileId(0)) {
            Ok(w) => w,
            Err(e) => {
                tracing::debug!("Skipping template validation: parse error: {e}");
                return vec![];
            }
        };
        let analyze_result = analyze(raw_workflow.clone());
        if let Some(ref analyzed) = analyze_result.value {
            validate_templates(&raw_workflow, analyzed, &doc)
        } else {
            vec![]
        }
    }

    #[test]
    fn test_valid_template_reference() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Generate something"
  - id: step2
    with:
      data: $step1
    infer: "Process {{with.data}}"
"#;
        let diagnostics = validate_yaml(content);
        assert!(
            diagnostics.is_empty(),
            "Expected no errors, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_undefined_template_reference() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Generate something"
  - id: step2
    with:
      data: $step1
    infer: "Process {{with.unknown}}"
"#;
        let diagnostics = validate_yaml(content);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Undefined template binding"));
        assert!(diagnostics[0].message.contains("unknown"));
        assert!(diagnostics[0].message.contains("Available aliases: data"));
    }

    #[test]
    fn test_no_with_block() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Process {{with.missing}}"
"#;
        let diagnostics = validate_yaml(content);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0]
            .message
            .contains("Undefined template binding"));
        assert!(diagnostics[0].message.contains("No bindings declared"));
    }

    #[test]
    fn test_multiple_undefined_references() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: producer
    infer: "Generate data"
  - id: step1
    with:
      valid: $producer
    infer: "{{with.bad1}} and {{with.bad2}} but {{with.valid}} is ok"
"#;
        let diagnostics = validate_yaml(content);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().any(|d| d.message.contains("bad1")));
        assert!(diagnostics.iter().any(|d| d.message.contains("bad2")));
    }

    #[test]
    fn test_nested_path_validation() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Generate something"
  - id: step2
    with:
      data: $step1
    infer: "Process {{with.data.field.nested}}"
"#;
        let diagnostics = validate_yaml(content);
        // Should be valid - the alias 'data' is declared
        assert!(
            diagnostics.is_empty(),
            "Expected no errors, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_agent_prompt_validation() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Generate data"
  - id: step2
    with:
      context: $step1
    agent:
      prompt: "Research {{with.undefined_context}}"
      max_turns: 5
"#;
        let diagnostics = validate_yaml(content);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("undefined_context"));
    }

    #[test]
    fn test_exec_command_validation() {
        let content = r#"
schema: "nika/workflow@0.12"
model: test-model
workflow: test
tasks:
  - id: step1
    infer: "Generate filename"
  - id: step2
    with:
      filename: $step1
    exec: "cat {{with.wrong_name}}"
"#;
        let diagnostics = validate_yaml(content);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("wrong_name"));
    }

    #[test]
    fn test_extract_refs_with_positions() {
        let text = "Hello {{with.foo}} world {{with.bar.baz}}";
        let refs = extract_refs_with_positions(text);

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].alias, "foo");
        assert_eq!(refs[0].start, 6);
        assert_eq!(refs[0].end, 18);
        assert_eq!(refs[1].alias, "bar");
        assert_eq!(refs[1].start, 25);
        assert_eq!(refs[1].end, 41);
    }
}
