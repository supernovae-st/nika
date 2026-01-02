//! Error types with fix suggestions (v0.1)

use thiserror::Error;

/// Trait for errors that provide fix suggestions
pub trait FixSuggestion {
    fn fix_suggestion(&self) -> Option<&str>;
}

/// All error variants are part of the public API.
/// Some variants are only constructed in library code/tests.
#[derive(Error, Debug)]
#[allow(dead_code)] // Variants used in lib, not all in bin
pub enum NikaError {
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    // ─────────────────────────────────────────────────────────────
    // v0.1: Use block errors (NIKA-050 to NIKA-052)
    // ─────────────────────────────────────────────────────────────

    #[error("NIKA-050: Invalid path syntax: {path}")]
    InvalidPath { path: String },

    #[error("NIKA-051: Task '{task_id}' not found in datastore")]
    TaskNotFound { task_id: String },

    #[error("NIKA-052: Path '{path}' not found (task may not have JSON output)")]
    PathNotFound { path: String },

    // ─────────────────────────────────────────────────────────────
    // v0.1: Output errors (NIKA-060 to NIKA-061)
    // ─────────────────────────────────────────────────────────────

    #[error("NIKA-060: Invalid JSON output: {details}")]
    InvalidJson { details: String },

    #[error("NIKA-061: Schema validation failed: {details}")]
    SchemaFailed { details: String },

    // ─────────────────────────────────────────────────────────────
    // v0.1: Use block validation errors (NIKA-070 to NIKA-074)
    // ─────────────────────────────────────────────────────────────

    #[error("NIKA-070: Duplicate alias '{alias}' in use block")]
    DuplicateAlias { alias: String },

    #[error("NIKA-071: Unknown alias '{{{{use.{alias}}}}}' - not declared in use: block")]
    UnknownAlias { alias: String, task_id: String },

    #[error("NIKA-072: Null value at path '{path}' (strict mode)")]
    NullValue { path: String, alias: String },

    #[error("NIKA-073: Cannot traverse '{segment}' on {value_type} (expected object/array)")]
    InvalidTraversal {
        segment: String,
        value_type: String,
        full_path: String,
    },

    #[error("NIKA-074: Template parse error at position {position}: {details}")]
    TemplateParse { position: usize, details: String },

    // ─────────────────────────────────────────────────────────────
    // v0.1: DAG validation errors (NIKA-080 to NIKA-082)
    // ─────────────────────────────────────────────────────────────

    #[error("NIKA-080: use.{alias}.from references unknown task '{from_task}'")]
    UseUnknownTask { alias: String, from_task: String, task_id: String },

    #[error("NIKA-081: use.{alias}.from='{from_task}' is not upstream of task '{task_id}'")]
    UseNotUpstream { alias: String, from_task: String, task_id: String },

    #[error("NIKA-082: use.{alias}.from='{from_task}' creates circular dependency with '{task_id}'")]
    UseCircularDep { alias: String, from_task: String, task_id: String },

    // ─────────────────────────────────────────────────────────────
    // v0.1: JSONPath errors (NIKA-090 to NIKA-092)
    // ─────────────────────────────────────────────────────────────

    #[error("NIKA-090: JSONPath '{path}' is not supported in v0.1 (use $.a.b or $.a[0].b)")]
    JsonPathUnsupported { path: String },

    #[error("NIKA-091: JSONPath '{path}' matched nothing in output")]
    JsonPathNoMatch { path: String, task_id: String },

    #[error("NIKA-092: Cannot apply JSONPath to non-JSON output from task '{task_id}'")]
    JsonPathNonJson { path: String, task_id: String },
}

impl FixSuggestion for NikaError {
    fn fix_suggestion(&self) -> Option<&str> {
        match self {
            NikaError::YamlParse(_) => Some("Check YAML syntax: indentation and quoting"),
            NikaError::Template(_) => Some("Use {{use.alias}} format with use: block"),
            NikaError::Provider(_) => Some("Check API key env var is set (ANTHROPIC_API_KEY or OPENAI_API_KEY)"),
            NikaError::Execution(_) => Some("Check command/URL is valid"),
            NikaError::Io(_) => Some("Check file path and permissions"),

            // v0.1 error suggestions
            NikaError::InvalidPath { .. } => Some("Use format: task_id.field.subfield"),
            NikaError::TaskNotFound { .. } => Some("Verify task_id exists and has run successfully"),
            NikaError::PathNotFound { .. } => Some("Add default: value or ensure task outputs JSON with format: json"),
            NikaError::InvalidJson { .. } => Some("Ensure output is valid JSON (try parsing with jq)"),
            NikaError::SchemaFailed { .. } => Some("Fix output to match declared schema"),
            NikaError::DuplicateAlias { .. } => Some("Use unique alias names in use: block"),
            NikaError::UnknownAlias { .. } => {
                Some("Declare the alias in use: block before referencing it in templates")
            }
            NikaError::NullValue { .. } => {
                Some("Provide a default value or ensure upstream task returns non-null")
            }
            NikaError::InvalidTraversal { .. } => {
                Some("Check the path - you're trying to access a field on a non-object value")
            }
            NikaError::TemplateParse { .. } => {
                Some("Check template syntax: {{use.alias}} or {{use.alias.field}}")
            }
            NikaError::UseUnknownTask { .. } => {
                Some("Verify the task_id exists in your workflow")
            }
            NikaError::UseNotUpstream { .. } => {
                Some("Add a flow from the source task to this task, or use a different source")
            }
            NikaError::UseCircularDep { .. } => {
                Some("Remove the circular dependency - tasks cannot depend on themselves")
            }
            NikaError::JsonPathUnsupported { .. } => {
                Some("Use simple paths like $.field.subfield or $.array[0].field")
            }
            NikaError::JsonPathNoMatch { .. } => {
                Some("Check the path exists in the source task's output")
            }
            NikaError::JsonPathNonJson { .. } => {
                Some("Ensure source task has output: { format: json }")
            }
        }
    }
}
