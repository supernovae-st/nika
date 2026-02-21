// The #[error] attribute from thiserror uses struct fields via string interpolation,
// but Rust's unused_assignments lint doesn't recognize this.
#![allow(unused_assignments)]

//! Nika Error Types with Error Codes
//!
//! Error code ranges:
//! - NIKA-000-009: Workflow errors
//! - NIKA-010-019: Schema/validation errors
//! - NIKA-020-029: DAG errors
//! - NIKA-030-039: Provider errors
//! - NIKA-040-049: Template/binding errors
//! - NIKA-050-059: Path/task errors
//! - NIKA-060-069: Output errors
//! - NIKA-070-079: Use block validation errors
//! - NIKA-080-089: DAG validation errors
//! - NIKA-090-099: JSONPath/IO errors
//! - NIKA-100-109: MCP errors (v0.2, v0.5.1: +validation, v0.5.3: +error_code)
//! - NIKA-110-119: Agent errors (v0.2)
//! - NIKA-120-129: Resilience errors (v0.2) [122-124 deprecated in v0.4]
//! - NIKA-130-139: TUI errors (v0.2)
//!
//! v0.6.1: Added miette for fancy error display with source spans

use crate::mcp::types::McpErrorCode;
use miette::Diagnostic;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, NikaError>;

/// Format schema validation errors for display
fn format_schema_errors(errors: &[crate::ast::schema_validator::SchemaError]) -> String {
    if errors.is_empty() {
        return "no errors".to_string();
    }
    if errors.len() == 1 {
        return errors[0].message.clone();
    }
    format!(
        "{} errors: {}",
        errors.len(),
        errors
            .iter()
            .map(|e| format!("[{}] {}", e.path, e.message))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

/// Trait for errors that provide fix suggestions
pub trait FixSuggestion {
    fn fix_suggestion(&self) -> Option<&str>;
}

/// All error variants are part of the public API.
///
/// Implements both `thiserror::Error` for std error compatibility
/// and `miette::Diagnostic` for fancy terminal error display.
#[derive(Error, Debug, Diagnostic)]
#[diagnostic(url(docsrs))]
pub enum NikaError {
    // ═══════════════════════════════════════════
    // WORKFLOW ERRORS (000-009)
    // ═══════════════════════════════════════════
    #[error("[NIKA-001] Failed to parse workflow: {details}")]
    #[diagnostic(
        code(nika::parse_error),
        help("Check YAML syntax: indentation and quoting")
    )]
    ParseError { details: String },

    #[error("[NIKA-002] Invalid schema version: {version}")]
    #[diagnostic(
        code(nika::invalid_schema_version),
        help("Use 'nika/workflow@0.5' as the schema version")
    )]
    InvalidSchemaVersion { version: String },

    #[error("[NIKA-003] Workflow file not found: {path}")]
    #[diagnostic(code(nika::workflow_not_found), help("Check the file path exists"))]
    WorkflowNotFound { path: String },

    #[error("[NIKA-004] Workflow validation failed: {reason}")]
    #[diagnostic(
        code(nika::validation_error),
        help("Check workflow structure matches schema")
    )]
    ValidationError { reason: String },

    #[error("[NIKA-005] Schema validation failed: {}", format_schema_errors(.errors))]
    #[diagnostic(
        code(nika::schema_validation_failed),
        help("Check YAML against schemas/nika-workflow.schema.json")
    )]
    SchemaValidationFailed {
        errors: Vec<crate::ast::schema_validator::SchemaError>,
    },

    // ═══════════════════════════════════════════
    // SCHEMA ERRORS (010-019) - v0.1 compat
    // ═══════════════════════════════════════════
    #[error("[NIKA-010] Invalid schema version: expected '{expected}', got '{actual}'")]
    InvalidSchema { expected: String, actual: String },

    #[error("[NIKA-011] Task '{task_id}' failed: {reason}")]
    TaskFailed { task_id: String, reason: String },

    #[error("[NIKA-012] Task '{task_id}' timed out after {timeout_ms}ms")]
    TaskTimeout { task_id: String, timeout_ms: u64 },

    // ═══════════════════════════════════════════
    // DAG ERRORS (020-029)
    // ═══════════════════════════════════════════
    #[error("[NIKA-020] Cycle detected in DAG: {cycle}")]
    CycleDetected { cycle: String },

    #[error("[NIKA-021] Missing dependency: task '{task_id}' depends on unknown '{dep_id}'")]
    MissingDependency { task_id: String, dep_id: String },

    // ═══════════════════════════════════════════
    // PROVIDER ERRORS (030-039)
    // ═══════════════════════════════════════════
    /// Legacy: simple provider error (v0.1 compat)
    #[error("Provider error: {0}")]
    Provider(String),

    #[error("[NIKA-030] Provider '{provider}' not configured")]
    ProviderNotConfigured { provider: String },

    #[error("[NIKA-031] Provider API error: {message}")]
    ProviderApiError { message: String },

    #[error("[NIKA-032] Missing API key for provider '{provider}'")]
    MissingApiKey { provider: String },

    #[error("[NIKA-033] Invalid configuration: {message}")]
    InvalidConfig { message: String },

    // ═══════════════════════════════════════════
    // TEMPLATE/BINDING ERRORS (040-049)
    // ═══════════════════════════════════════════
    /// Legacy: simple template error (v0.1 compat)
    #[error("Template error: {0}")]
    Template(String),

    /// Legacy: simple execution error (v0.1 compat)
    #[error("Execution error: {0}")]
    Execution(String),

    #[error("[NIKA-040] Binding resolution failed: {reason}")]
    BindingError { reason: String },

    #[error("[NIKA-041] Template error in '{template}': {reason}")]
    TemplateError { template: String, reason: String },

    #[error("[NIKA-042] Binding '{alias}' not found")]
    BindingNotFound { alias: String },

    #[error("[NIKA-043] Binding type mismatch at '{path}': expected {expected}, got {actual}")]
    BindingTypeMismatch {
        expected: String,
        actual: String,
        path: String,
    },

    // ═══════════════════════════════════════════
    // PATH/TASK ERRORS (050-059) - v0.1
    // ═══════════════════════════════════════════
    #[error("[NIKA-050] Invalid path syntax: {path}")]
    InvalidPath { path: String },

    #[error("[NIKA-051] Task '{task_id}' not found in datastore")]
    TaskNotFound { task_id: String },

    #[error("[NIKA-052] Path '{path}' not found (task may not have JSON output)")]
    PathNotFound { path: String },

    #[error("[NIKA-055] Invalid task ID '{id}': {reason}")]
    InvalidTaskId { id: String, reason: String },

    #[error("[NIKA-056] Invalid default value '{raw}': {reason}")]
    InvalidDefault { raw: String, reason: String },

    // ═══════════════════════════════════════════
    // OUTPUT ERRORS (060-069) - v0.1
    // ═══════════════════════════════════════════
    #[error("[NIKA-060] Invalid JSON output: {details}")]
    InvalidJson { details: String },

    #[error("[NIKA-061] Schema validation failed: {details}")]
    SchemaFailed { details: String },

    // ═══════════════════════════════════════════
    // USE BLOCK VALIDATION (070-079) - v0.1
    // ═══════════════════════════════════════════
    #[error("[NIKA-070] Duplicate alias '{alias}' in use block")]
    DuplicateAlias { alias: String },

    #[error("[NIKA-071] Unknown alias '{{{{use.{alias}}}}}' - not declared in use: block")]
    UnknownAlias { alias: String, task_id: String },

    #[error("[NIKA-072] Null value at path '{path}' (strict mode)")]
    NullValue { path: String, alias: String },

    #[error("[NIKA-073] Cannot traverse '{segment}' on {value_type} (expected object/array)")]
    InvalidTraversal {
        segment: String,
        value_type: String,
        full_path: String,
    },

    #[error("[NIKA-074] Template parse error at position {position}: {details}")]
    TemplateParse { position: usize, details: String },

    // ═══════════════════════════════════════════
    // DAG VALIDATION (080-089) - v0.1
    // ═══════════════════════════════════════════
    #[error("[NIKA-080] use.{alias}.from references unknown task '{from_task}'")]
    UseUnknownTask {
        alias: String,
        from_task: String,
        task_id: String,
    },

    #[error("[NIKA-081] use.{alias}.from='{from_task}' is not upstream of task '{task_id}'")]
    UseNotUpstream {
        alias: String,
        from_task: String,
        task_id: String,
    },

    #[error(
        "[NIKA-082] use.{alias}.from='{from_task}' creates circular dependency with '{task_id}'"
    )]
    UseCircularDep {
        alias: String,
        from_task: String,
        task_id: String,
    },

    // ═══════════════════════════════════════════
    // JSONPATH / IO ERRORS (090-099) - v0.1
    // ═══════════════════════════════════════════
    #[error("[NIKA-090] JSONPath '{path}' is not supported in v0.1 (use $.a.b or $.a[0].b)")]
    JsonPathUnsupported { path: String },

    #[error("[NIKA-091] JSONPath '{path}' matched nothing in output")]
    JsonPathNoMatch { path: String, task_id: String },

    #[error("[NIKA-092] Cannot apply JSONPath to non-JSON output from task '{task_id}'")]
    JsonPathNonJson { path: String, task_id: String },

    #[error("[NIKA-093] IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("[NIKA-094] JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("[NIKA-095] YAML parse error: {0}")]
    #[diagnostic(
        code(nika::yaml_parse),
        help("Check YAML syntax: indentation must be consistent, strings with special chars need quoting")
    )]
    YamlParse(#[from] serde_yaml::Error),

    // ═══════════════════════════════════════════
    // MCP ERRORS (100-109) - NEW v0.2
    // ═══════════════════════════════════════════
    #[error("[NIKA-100] MCP server '{name}' not connected")]
    #[diagnostic(
        code(nika::mcp_not_connected),
        help("Check MCP server is running and configured correctly")
    )]
    McpNotConnected { name: String },

    #[error("[NIKA-101] MCP server '{name}' failed to start: {reason}")]
    #[diagnostic(
        code(nika::mcp_start_error),
        help("Check MCP command and args in workflow config")
    )]
    McpStartError { name: String, reason: String },

    #[error("[NIKA-102] MCP tool '{tool}' call failed{}: {reason}", error_code.map(|c| format!(" ({})", c)).unwrap_or_default())]
    #[diagnostic(
        code(nika::mcp_tool_error),
        help("Check tool parameters and MCP server logs")
    )]
    McpToolError {
        tool: String,
        reason: String,
        /// JSON-RPC error code from MCP server (v0.5.3)
        error_code: Option<McpErrorCode>,
    },

    #[error("[NIKA-103] MCP resource '{uri}' not found")]
    McpResourceNotFound { uri: String },

    #[error("[NIKA-104] MCP protocol error: {reason}")]
    McpProtocolError { reason: String },

    #[error("[NIKA-105] MCP server '{name}' not configured in workflow")]
    McpNotConfigured { name: String },

    #[error("[NIKA-106] MCP tool '{tool}' returned invalid response: {reason}")]
    McpInvalidResponse { tool: String, reason: String },

    #[error("[NIKA-107] MCP parameter validation failed for '{tool}': {details}")]
    McpValidationFailed {
        tool: String,
        details: String,
        /// Required fields that are missing
        missing: Vec<String>,
        /// Suggested corrections
        suggestions: Vec<String>,
    },

    #[error("[NIKA-108] MCP schema error for '{tool}': {reason}")]
    McpSchemaError { tool: String, reason: String },

    #[error(
        "[NIKA-109] MCP operation timed out for '{name}' ({operation}): exceeded {timeout_secs}s"
    )]
    McpTimeout {
        name: String,
        operation: String,
        timeout_secs: u64,
    },

    // ═══════════════════════════════════════════
    // AGENT ERRORS (110-119) - NEW v0.2
    // ═══════════════════════════════════════════
    #[error("[NIKA-110] Agent loop exceeded max turns ({max_turns})")]
    AgentMaxTurns { max_turns: u32 },

    #[error("[NIKA-111] Agent stop condition not met: {condition}")]
    AgentStopConditionFailed { condition: String },

    #[error("[NIKA-112] Invalid tool name format: {name}")]
    InvalidToolName { name: String },

    #[error("[NIKA-113] Agent validation failed: {reason}")]
    AgentValidationError { reason: String },

    #[error("[NIKA-114] Feature not implemented: {feature}. {suggestion}")]
    NotImplemented { feature: String, suggestion: String },

    #[error("[NIKA-115] Agent execution failed for task '{task_id}': {reason}")]
    AgentExecutionError { task_id: String, reason: String },

    #[error("[NIKA-116] Extended thinking capture failed: {reason}")]
    ThinkingCaptureFailed { reason: String },

    #[error("[NIKA-117] Extended thinking not supported for provider '{provider}'")]
    ThinkingNotSupported { provider: String },

    // ═══════════════════════════════════════════
    // RESILIENCE ERRORS (120-129) - NEW v0.2
    // ═══════════════════════════════════════════
    #[error("[NIKA-120] Provider '{provider}' error: {reason}")]
    ProviderError { provider: String, reason: String },

    #[error("[NIKA-121] Operation '{operation}' timed out after {duration_ms}ms")]
    Timeout { operation: String, duration_ms: u64 },

    #[error("[NIKA-125] MCP tool call '{tool}' failed: {reason}")]
    McpToolCallFailed { tool: String, reason: String },

    // ═══════════════════════════════════════════
    // TUI ERRORS (130-139) - NEW v0.2
    // ═══════════════════════════════════════════
    #[error("[NIKA-130] TUI error: {reason}")]
    TuiError { reason: String },

    // ═══════════════════════════════════════════
    // CONFIG ERRORS (140-149) - NEW v0.5
    // ═══════════════════════════════════════════
    #[error("[NIKA-140] Config error: {reason}")]
    ConfigError { reason: String },

    // ═══════════════════════════════════════════
    // TOOL ERRORS (200-219) - NEW v0.6
    // ═══════════════════════════════════════════
    #[error("[{code}] {message}")]
    ToolError { code: String, message: String },
}

impl NikaError {
    /// Get the error code (e.g., "NIKA-001")
    pub fn code(&self) -> &'static str {
        match self {
            // Workflow errors
            Self::ParseError { .. } => "NIKA-001",
            Self::InvalidSchemaVersion { .. } => "NIKA-002",
            Self::WorkflowNotFound { .. } => "NIKA-003",
            Self::ValidationError { .. } => "NIKA-004",
            Self::SchemaValidationFailed { .. } => "NIKA-005",
            // Schema errors
            Self::InvalidSchema { .. } => "NIKA-010",
            Self::TaskFailed { .. } => "NIKA-011",
            Self::TaskTimeout { .. } => "NIKA-012",
            // DAG errors
            Self::CycleDetected { .. } => "NIKA-020",
            Self::MissingDependency { .. } => "NIKA-021",
            // Provider errors
            Self::Provider(_) => "NIKA-030", // legacy
            Self::ProviderNotConfigured { .. } => "NIKA-030",
            Self::ProviderApiError { .. } => "NIKA-031",
            Self::MissingApiKey { .. } => "NIKA-032",
            Self::InvalidConfig { .. } => "NIKA-033",
            // Binding/Template errors
            Self::Template(_) => "NIKA-040",  // legacy
            Self::Execution(_) => "NIKA-041", // legacy
            Self::BindingError { .. } => "NIKA-040",
            Self::TemplateError { .. } => "NIKA-041",
            Self::BindingNotFound { .. } => "NIKA-042",
            Self::BindingTypeMismatch { .. } => "NIKA-043",
            // Path/Task errors
            Self::InvalidPath { .. } => "NIKA-050",
            Self::TaskNotFound { .. } => "NIKA-051",
            Self::PathNotFound { .. } => "NIKA-052",
            Self::InvalidTaskId { .. } => "NIKA-055",
            Self::InvalidDefault { .. } => "NIKA-056",
            // Output errors
            Self::InvalidJson { .. } => "NIKA-060",
            Self::SchemaFailed { .. } => "NIKA-061",
            // Use block errors
            Self::DuplicateAlias { .. } => "NIKA-070",
            Self::UnknownAlias { .. } => "NIKA-071",
            Self::NullValue { .. } => "NIKA-072",
            Self::InvalidTraversal { .. } => "NIKA-073",
            Self::TemplateParse { .. } => "NIKA-074",
            // DAG validation errors
            Self::UseUnknownTask { .. } => "NIKA-080",
            Self::UseNotUpstream { .. } => "NIKA-081",
            Self::UseCircularDep { .. } => "NIKA-082",
            // JSONPath/IO errors
            Self::JsonPathUnsupported { .. } => "NIKA-090",
            Self::JsonPathNoMatch { .. } => "NIKA-091",
            Self::JsonPathNonJson { .. } => "NIKA-092",
            Self::IoError(_) => "NIKA-093",
            Self::JsonError(_) => "NIKA-094",
            Self::YamlParse(_) => "NIKA-095",
            // MCP errors
            Self::McpNotConnected { .. } => "NIKA-100",
            Self::McpStartError { .. } => "NIKA-101",
            Self::McpToolError { .. } => "NIKA-102",
            Self::McpResourceNotFound { .. } => "NIKA-103",
            Self::McpProtocolError { .. } => "NIKA-104",
            Self::McpNotConfigured { .. } => "NIKA-105",
            Self::McpInvalidResponse { .. } => "NIKA-106",
            Self::McpValidationFailed { .. } => "NIKA-107",
            Self::McpSchemaError { .. } => "NIKA-108",
            Self::McpTimeout { .. } => "NIKA-109",
            // Agent errors
            Self::AgentMaxTurns { .. } => "NIKA-110",
            Self::AgentStopConditionFailed { .. } => "NIKA-111",
            Self::InvalidToolName { .. } => "NIKA-112",
            Self::AgentValidationError { .. } => "NIKA-113",
            Self::NotImplemented { .. } => "NIKA-114",
            Self::AgentExecutionError { .. } => "NIKA-115",
            Self::ThinkingCaptureFailed { .. } => "NIKA-116",
            Self::ThinkingNotSupported { .. } => "NIKA-117",
            // Resilience errors
            Self::ProviderError { .. } => "NIKA-120",
            Self::Timeout { .. } => "NIKA-121",
            Self::McpToolCallFailed { .. } => "NIKA-125",
            // TUI errors
            Self::TuiError { .. } => "NIKA-130",
            // Config errors
            Self::ConfigError { .. } => "NIKA-140",
            // Tool errors (code is dynamic)
            Self::ToolError { .. } => "NIKA-2XX",
        }
    }

    /// Check if error is recoverable (can be retried)
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::TaskTimeout { .. }
                | Self::McpNotConnected { .. }
                | Self::ProviderApiError { .. }
                | Self::McpToolError { .. }
                | Self::ProviderError { .. }
                | Self::Timeout { .. }
                | Self::McpTimeout { .. }
                | Self::McpToolCallFailed { .. }
        )
    }
}

impl FixSuggestion for NikaError {
    fn fix_suggestion(&self) -> Option<&str> {
        match self {
            NikaError::ParseError { .. } => Some("Check YAML syntax: indentation and quoting"),
            NikaError::InvalidSchemaVersion { .. } => {
                Some("Use 'nika/workflow@0.5' as the schema version")
            }
            NikaError::WorkflowNotFound { .. } => Some("Check the file path exists"),
            NikaError::ValidationError { .. } => Some("Check workflow structure matches schema"),
            NikaError::SchemaValidationFailed { .. } => {
                Some("Check YAML against schemas/nika-workflow.schema.json")
            }
            NikaError::YamlParse(_) => Some("Check YAML syntax: indentation and quoting"),
            NikaError::InvalidSchema { .. } => {
                Some("Use 'nika/workflow@0.5' as the schema version")
            }
            NikaError::TaskFailed { .. } => Some("Check task configuration and dependencies"),
            NikaError::TaskTimeout { .. } => Some("Increase timeout or optimize the task"),
            NikaError::CycleDetected { .. } => {
                Some("Remove circular dependencies from your workflow")
            }
            NikaError::MissingDependency { .. } => {
                Some("Add the missing task or fix the dependency reference")
            }
            NikaError::Provider(_) => Some("Check API key env var is set"),
            NikaError::ProviderNotConfigured { .. } => {
                Some("Add provider configuration to your workflow")
            }
            NikaError::ProviderApiError { .. } => Some("Check API key and provider availability"),
            NikaError::MissingApiKey { .. } => {
                Some("Set the API key env var (ANTHROPIC_API_KEY or OPENAI_API_KEY)")
            }
            NikaError::InvalidConfig { .. } => Some("Check configuration value is valid"),
            NikaError::Template(_) => Some("Use {{use.alias}} format with use: block"),
            NikaError::Execution(_) => Some("Check command/URL is valid"),
            NikaError::BindingError { .. } => Some("Check binding syntax and source task output"),
            NikaError::TemplateError { .. } => Some("Use {{use.alias}} format with use: block"),
            NikaError::InvalidPath { .. } => Some("Use format: task_id.field.subfield"),
            NikaError::TaskNotFound { .. } => {
                Some("Verify task_id exists and has run successfully")
            }
            NikaError::PathNotFound { .. } => Some("Add '?? default' or ensure task outputs JSON"),
            NikaError::InvalidTaskId { .. } => {
                Some("Task IDs must be snake_case: lowercase letters, digits, underscores")
            }
            NikaError::InvalidDefault { .. } => {
                Some("Default values must be valid JSON. Strings must be quoted.")
            }
            NikaError::InvalidJson { .. } => Some("Ensure output is valid JSON"),
            NikaError::SchemaFailed { .. } => Some("Fix output to match declared schema"),
            NikaError::DuplicateAlias { .. } => Some("Use unique alias names in use: block"),
            NikaError::UnknownAlias { .. } => {
                Some("Declare the alias in use: block before referencing")
            }
            NikaError::NullValue { .. } => {
                Some("Provide a default value or ensure non-null output")
            }
            NikaError::InvalidTraversal { .. } => {
                Some("Check the path - accessing field on non-object")
            }
            NikaError::TemplateParse { .. } => Some("Check template syntax: {{use.alias}}"),
            NikaError::UseUnknownTask { .. } => Some("Verify the task_id exists in your workflow"),
            NikaError::UseNotUpstream { .. } => {
                Some("Add a flow from the source task to this task")
            }
            NikaError::UseCircularDep { .. } => Some("Remove the circular dependency"),
            NikaError::JsonPathUnsupported { .. } => Some("Use simple paths like $.field.subfield"),
            NikaError::JsonPathNoMatch { .. } => {
                Some("Check the path exists in source task output")
            }
            NikaError::JsonPathNonJson { .. } => {
                Some("Ensure source task has output: { format: json }")
            }
            NikaError::IoError(_) => Some("Check file path and permissions"),
            NikaError::JsonError(_) => Some("Check JSON syntax"),
            // MCP errors
            NikaError::McpNotConnected { .. } => {
                Some("Check MCP server is running and configured correctly")
            }
            NikaError::McpStartError { .. } => {
                Some("Check MCP command and args in workflow config")
            }
            NikaError::McpToolError { .. } => Some("Check tool parameters and MCP server logs"),
            NikaError::McpResourceNotFound { .. } => Some("Verify the resource URI exists"),
            NikaError::McpProtocolError { .. } => Some("Check MCP server compatibility"),
            NikaError::McpNotConfigured { .. } => {
                Some("Add MCP server config to workflow 'mcp:' section")
            }
            NikaError::McpInvalidResponse { .. } => {
                Some("Check MCP server is returning valid JSON responses")
            }
            NikaError::McpValidationFailed {
                missing,
                suggestions,
                ..
            } => {
                if !missing.is_empty() {
                    Some("Add the required fields to your params")
                } else if !suggestions.is_empty() {
                    Some("Check spelling of field names")
                } else {
                    Some("Review the tool's parameter schema")
                }
            }
            NikaError::McpSchemaError { .. } => Some("Check MCP server's tool schema definitions"),
            // Binding errors (decompose)
            NikaError::BindingNotFound { .. } => {
                Some("Verify the binding alias exists in use: block or task outputs")
            }
            NikaError::BindingTypeMismatch { .. } => {
                Some("Check binding value type matches expected type")
            }
            // Agent errors
            NikaError::AgentMaxTurns { .. } => Some("Increase max_turns or simplify the task"),
            NikaError::AgentStopConditionFailed { .. } => {
                Some("Check stop condition is achievable")
            }
            NikaError::InvalidToolName { .. } => {
                Some("Tool names must be mcp_server.tool_name format")
            }
            NikaError::AgentValidationError { .. } => {
                Some("Check agent prompt is not empty and max_turns is valid (1-100)")
            }
            NikaError::AgentExecutionError { .. } => {
                Some("Check LLM provider API key and network connectivity")
            }
            NikaError::NotImplemented { .. } => {
                Some("This feature is planned for a future release")
            }
            NikaError::ThinkingCaptureFailed { .. } => {
                Some("Check Claude API response and streaming connection")
            }
            NikaError::ThinkingNotSupported { .. } => {
                Some("Extended thinking is only supported with Claude provider")
            }
            // Resilience errors
            NikaError::ProviderError { .. } => {
                Some("Check provider configuration and network connectivity")
            }
            NikaError::Timeout { .. } => Some("Increase timeout or check for slow operations"),
            NikaError::McpTimeout { .. } => {
                Some("MCP server is slow or unresponsive. Check network and server health.")
            }
            NikaError::McpToolCallFailed { .. } => {
                Some("Check MCP tool parameters and server logs")
            }
            // TUI errors
            NikaError::TuiError { .. } => Some("Check terminal compatibility and size"),
            // Config errors
            NikaError::ConfigError { .. } => {
                Some("Check ~/.config/nika/config.toml for syntax errors")
            }
            // Tool errors
            NikaError::ToolError { .. } => {
                Some("Check file path and permissions. Use Read before Edit.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════════════════
    // WORKFLOW ERRORS (000-009)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_parse_error_code_and_display() {
        let err = NikaError::ParseError {
            details: "unexpected token at line 5".to_string(),
        };
        assert_eq!(err.code(), "NIKA-001");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-001]"));
        assert!(msg.contains("unexpected token"));
    }

    #[test]
    fn test_parse_error_fix_suggestion() {
        let err = NikaError::ParseError {
            details: "bad yaml".to_string(),
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("YAML syntax"));
    }

    #[test]
    fn test_invalid_schema_version_error() {
        let err = NikaError::InvalidSchemaVersion {
            version: "0.1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-002");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-002]"));
        assert!(msg.contains("0.1"));
    }

    #[test]
    fn test_workflow_not_found_error() {
        let err = NikaError::WorkflowNotFound {
            path: "/path/to/missing.yaml".to_string(),
        };
        assert_eq!(err.code(), "NIKA-003");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-003]"));
        assert!(msg.contains("missing.yaml"));
    }

    #[test]
    fn test_validation_error() {
        let err = NikaError::ValidationError {
            reason: "missing required field 'tasks'".to_string(),
        };
        assert_eq!(err.code(), "NIKA-004");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-004]"));
    }

    #[test]
    fn test_schema_validation_failed_error_empty() {
        let err = NikaError::SchemaValidationFailed { errors: vec![] };
        assert_eq!(err.code(), "NIKA-005");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-005]"));
        assert!(msg.contains("no errors"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SCHEMA ERRORS (010-019)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_invalid_schema_error() {
        let err = NikaError::InvalidSchema {
            expected: "nika/workflow@0.5".to_string(),
            actual: "nika/workflow@0.1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-010");
        let msg = err.to_string();
        assert!(msg.contains("0.5"));
        assert!(msg.contains("0.1"));
    }

    #[test]
    fn test_task_failed_error() {
        let err = NikaError::TaskFailed {
            task_id: "gen".to_string(),
            reason: "timeout".to_string(),
        };
        assert_eq!(err.code(), "NIKA-011");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-011]"));
        assert!(msg.contains("gen"));
        assert!(msg.contains("timeout"));
    }

    #[test]
    fn test_task_timeout_error() {
        let err = NikaError::TaskTimeout {
            task_id: "slow_task".to_string(),
            timeout_ms: 5000,
        };
        assert_eq!(err.code(), "NIKA-012");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-012]"));
        assert!(msg.contains("5000"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // DAG ERRORS (020-029)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_cycle_detected_error() {
        let err = NikaError::CycleDetected {
            cycle: "task1 -> task2 -> task1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-020");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-020]"));
        assert!(msg.contains("task1"));
    }

    #[test]
    fn test_missing_dependency_error() {
        let err = NikaError::MissingDependency {
            task_id: "step2".to_string(),
            dep_id: "step1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-021");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-021]"));
        assert!(msg.contains("step2"));
        assert!(msg.contains("step1"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PROVIDER ERRORS (030-039)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_provider_legacy_error() {
        let err = NikaError::Provider("Connection failed".to_string());
        assert_eq!(err.code(), "NIKA-030");
        let msg = err.to_string();
        assert!(msg.contains("Provider error"));
    }

    #[test]
    fn test_provider_not_configured_error() {
        let err = NikaError::ProviderNotConfigured {
            provider: "openai".to_string(),
        };
        assert_eq!(err.code(), "NIKA-030");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-030]"));
    }

    #[test]
    fn test_provider_api_error() {
        let err = NikaError::ProviderApiError {
            message: "Rate limit exceeded".to_string(),
        };
        assert_eq!(err.code(), "NIKA-031");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-031]"));
    }

    #[test]
    fn test_missing_api_key_error() {
        let err = NikaError::MissingApiKey {
            provider: "anthropic".to_string(),
        };
        assert_eq!(err.code(), "NIKA-032");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-032]"));
        assert!(msg.contains("anthropic"));
    }

    #[test]
    fn test_invalid_config_error() {
        let err = NikaError::InvalidConfig {
            message: "port must be > 0".to_string(),
        };
        assert_eq!(err.code(), "NIKA-033");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-033]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TEMPLATE/BINDING ERRORS (040-049)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_template_legacy_error() {
        let err = NikaError::Template("unmatched {{".to_string());
        assert_eq!(err.code(), "NIKA-040");
        let msg = err.to_string();
        assert!(msg.contains("Template error"));
    }

    #[test]
    fn test_execution_legacy_error() {
        let err = NikaError::Execution("command not found".to_string());
        assert_eq!(err.code(), "NIKA-041");
        let msg = err.to_string();
        assert!(msg.contains("Execution error"));
    }

    #[test]
    fn test_binding_error() {
        let err = NikaError::BindingError {
            reason: "undefined reference".to_string(),
        };
        assert_eq!(err.code(), "NIKA-040");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-040]"));
    }

    #[test]
    fn test_template_error_with_path() {
        let err = NikaError::TemplateError {
            template: "{{use.result}}".to_string(),
            reason: "alias not in use block".to_string(),
        };
        assert_eq!(err.code(), "NIKA-041");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-041]"));
        assert!(msg.contains("result"));
    }

    #[test]
    fn test_binding_not_found_error() {
        let err = NikaError::BindingNotFound {
            alias: "entity_data".to_string(),
        };
        assert_eq!(err.code(), "NIKA-042");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-042]"));
        assert!(msg.contains("entity_data"));
    }

    #[test]
    fn test_binding_type_mismatch_error() {
        let err = NikaError::BindingTypeMismatch {
            expected: "string".to_string(),
            actual: "array".to_string(),
            path: "use.field.subfield".to_string(),
        };
        assert_eq!(err.code(), "NIKA-043");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-043]"));
        assert!(msg.contains("string"));
        assert!(msg.contains("array"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // PATH/TASK ERRORS (050-059)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_invalid_path_error() {
        let err = NikaError::InvalidPath {
            path: "task1..field".to_string(),
        };
        assert_eq!(err.code(), "NIKA-050");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-050]"));
    }

    #[test]
    fn test_task_not_found_error() {
        let err = NikaError::TaskNotFound {
            task_id: "missing_task".to_string(),
        };
        assert_eq!(err.code(), "NIKA-051");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-051]"));
        assert!(msg.contains("missing_task"));
    }

    #[test]
    fn test_path_not_found_error() {
        let err = NikaError::PathNotFound {
            path: "task.deeply.nested.field".to_string(),
        };
        assert_eq!(err.code(), "NIKA-052");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-052]"));
    }

    #[test]
    fn test_invalid_task_id_error() {
        let err = NikaError::InvalidTaskId {
            id: "Invalid-Task-ID".to_string(),
            reason: "contains uppercase or hyphens".to_string(),
        };
        assert_eq!(err.code(), "NIKA-055");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-055]"));
    }

    #[test]
    fn test_invalid_default_error() {
        let err = NikaError::InvalidDefault {
            raw: "not_quoted_string".to_string(),
            reason: "strings must be quoted".to_string(),
        };
        assert_eq!(err.code(), "NIKA-056");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-056]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // OUTPUT ERRORS (060-069)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_invalid_json_error() {
        let err = NikaError::InvalidJson {
            details: "trailing comma in object".to_string(),
        };
        assert_eq!(err.code(), "NIKA-060");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-060]"));
    }

    #[test]
    fn test_schema_failed_error() {
        let err = NikaError::SchemaFailed {
            details: "missing required property 'id'".to_string(),
        };
        assert_eq!(err.code(), "NIKA-061");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-061]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // USE BLOCK VALIDATION (070-079)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_duplicate_alias_error() {
        let err = NikaError::DuplicateAlias {
            alias: "result".to_string(),
        };
        assert_eq!(err.code(), "NIKA-070");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-070]"));
        assert!(msg.contains("result"));
    }

    #[test]
    fn test_unknown_alias_error() {
        let err = NikaError::UnknownAlias {
            alias: "undefined".to_string(),
            task_id: "current_task".to_string(),
        };
        assert_eq!(err.code(), "NIKA-071");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-071]"));
        assert!(msg.contains("undefined"));
    }

    #[test]
    fn test_null_value_error() {
        let err = NikaError::NullValue {
            path: "task.field".to_string(),
            alias: "myalias".to_string(),
        };
        assert_eq!(err.code(), "NIKA-072");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-072]"));
    }

    #[test]
    fn test_invalid_traversal_error() {
        let err = NikaError::InvalidTraversal {
            segment: "field".to_string(),
            value_type: "string".to_string(),
            full_path: "task.value.field".to_string(),
        };
        assert_eq!(err.code(), "NIKA-073");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-073]"));
        assert!(msg.contains("string"));
    }

    #[test]
    fn test_template_parse_error() {
        let err = NikaError::TemplateParse {
            position: 10,
            details: "unexpected closing brace".to_string(),
        };
        assert_eq!(err.code(), "NIKA-074");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-074]"));
        assert!(msg.contains("10"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // DAG VALIDATION (080-089)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_use_unknown_task_error() {
        let err = NikaError::UseUnknownTask {
            alias: "ctx".to_string(),
            from_task: "undefined".to_string(),
            task_id: "current".to_string(),
        };
        assert_eq!(err.code(), "NIKA-080");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-080]"));
        assert!(msg.contains("undefined"));
    }

    #[test]
    fn test_use_not_upstream_error() {
        let err = NikaError::UseNotUpstream {
            alias: "ctx".to_string(),
            from_task: "task2".to_string(),
            task_id: "task1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-081");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-081]"));
    }

    #[test]
    fn test_use_circular_dep_error() {
        let err = NikaError::UseCircularDep {
            alias: "ctx".to_string(),
            from_task: "task1".to_string(),
            task_id: "task2".to_string(),
        };
        assert_eq!(err.code(), "NIKA-082");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-082]"));
        assert!(msg.contains("circular"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // JSONPATH / IO ERRORS (090-099)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_jsonpath_unsupported_error() {
        let err = NikaError::JsonPathUnsupported {
            path: "$.deeply[*].nested.path".to_string(),
        };
        assert_eq!(err.code(), "NIKA-090");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-090]"));
    }

    #[test]
    fn test_jsonpath_no_match_error() {
        let err = NikaError::JsonPathNoMatch {
            path: "$.missing.field".to_string(),
            task_id: "source_task".to_string(),
        };
        assert_eq!(err.code(), "NIKA-091");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-091]"));
    }

    #[test]
    fn test_jsonpath_non_json_error() {
        let err = NikaError::JsonPathNonJson {
            path: "$.field".to_string(),
            task_id: "text_task".to_string(),
        };
        assert_eq!(err.code(), "NIKA-092");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-092]"));
    }

    #[test]
    fn test_io_error_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: NikaError = io_err.into();
        assert_eq!(err.code(), "NIKA-093");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-093]"));
    }

    #[test]
    fn test_json_error_from_serde() {
        let json_str = "{invalid json";
        let json_err: serde_json::Result<serde_json::Value> = serde_json::from_str(json_str);
        if let Err(e) = json_err {
            let err: NikaError = e.into();
            assert_eq!(err.code(), "NIKA-094");
            let msg = err.to_string();
            assert!(msg.contains("[NIKA-094]"));
        }
    }

    #[test]
    fn test_yaml_parse_error_from_serde() {
        let yaml_str = "invalid: yaml: syntax:";
        let yaml_err: serde_yaml::Result<serde_yaml::Value> = serde_yaml::from_str(yaml_str);
        if let Err(e) = yaml_err {
            let err: NikaError = e.into();
            assert_eq!(err.code(), "NIKA-095");
            let msg = err.to_string();
            assert!(msg.contains("[NIKA-095]"));
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MCP ERRORS (100-109)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_mcp_not_connected_error() {
        let err = NikaError::McpNotConnected {
            name: "novanet".to_string(),
        };
        assert_eq!(err.code(), "NIKA-100");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-100]"));
        assert!(msg.contains("novanet"));
    }

    #[test]
    fn test_mcp_start_error() {
        let err = NikaError::McpStartError {
            name: "novanet".to_string(),
            reason: "port already in use".to_string(),
        };
        assert_eq!(err.code(), "NIKA-101");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-101]"));
    }

    #[test]
    fn test_mcp_tool_error_without_code() {
        let err = NikaError::McpToolError {
            tool: "novanet_generate".to_string(),
            reason: "invalid parameters".to_string(),
            error_code: None,
        };
        assert_eq!(err.code(), "NIKA-102");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-102]"));
        assert!(msg.contains("novanet_generate"));
    }

    #[test]
    fn test_mcp_tool_error_with_code() {
        let err = NikaError::McpToolError {
            tool: "novanet_describe".to_string(),
            reason: "entity not found".to_string(),
            error_code: Some(McpErrorCode::InvalidRequest),
        };
        assert_eq!(err.code(), "NIKA-102");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-102]"));
        // Error code should be included in display
        assert!(msg.contains("InvalidRequest") || msg.contains("error"));
    }

    #[test]
    fn test_mcp_resource_not_found_error() {
        let err = NikaError::McpResourceNotFound {
            uri: "novanet://entity/qr-code".to_string(),
        };
        assert_eq!(err.code(), "NIKA-103");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-103]"));
    }

    #[test]
    fn test_mcp_protocol_error() {
        let err = NikaError::McpProtocolError {
            reason: "JSON-RPC version mismatch".to_string(),
        };
        assert_eq!(err.code(), "NIKA-104");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-104]"));
    }

    #[test]
    fn test_mcp_not_configured_error() {
        let err = NikaError::McpNotConfigured {
            name: "novanet".to_string(),
        };
        assert_eq!(err.code(), "NIKA-105");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-105]"));
    }

    #[test]
    fn test_mcp_invalid_response_error() {
        let err = NikaError::McpInvalidResponse {
            tool: "novanet_traverse".to_string(),
            reason: "missing 'result' field".to_string(),
        };
        assert_eq!(err.code(), "NIKA-106");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-106]"));
    }

    #[test]
    fn test_mcp_validation_failed_error() {
        let err = NikaError::McpValidationFailed {
            tool: "novanet_generate".to_string(),
            details: "parameter validation failed".to_string(),
            missing: vec!["entity".to_string(), "locale".to_string()],
            suggestions: vec!["Check parameter names".to_string()],
        };
        assert_eq!(err.code(), "NIKA-107");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-107]"));
    }

    #[test]
    fn test_mcp_schema_error() {
        let err = NikaError::McpSchemaError {
            tool: "novanet_assemble".to_string(),
            reason: "invalid property type in schema".to_string(),
        };
        assert_eq!(err.code(), "NIKA-108");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-108]"));
    }

    #[test]
    fn test_mcp_timeout_error() {
        let err = NikaError::McpTimeout {
            name: "novanet".to_string(),
            operation: "novanet_generate".to_string(),
            timeout_secs: 30,
        };
        assert_eq!(err.code(), "NIKA-109");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-109]"));
        assert!(msg.contains("30"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // AGENT ERRORS (110-119)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_agent_max_turns_error() {
        let err = NikaError::AgentMaxTurns { max_turns: 10 };
        assert_eq!(err.code(), "NIKA-110");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-110]"));
        assert!(msg.contains("10"));
    }

    #[test]
    fn test_agent_stop_condition_failed_error() {
        let err = NikaError::AgentStopConditionFailed {
            condition: "generate complete landing page".to_string(),
        };
        assert_eq!(err.code(), "NIKA-111");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-111]"));
    }

    #[test]
    fn test_invalid_tool_name_error() {
        let err = NikaError::InvalidToolName {
            name: "invalid-format".to_string(),
        };
        assert_eq!(err.code(), "NIKA-112");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-112]"));
    }

    #[test]
    fn test_agent_validation_error() {
        let err = NikaError::AgentValidationError {
            reason: "empty prompt".to_string(),
        };
        assert_eq!(err.code(), "NIKA-113");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-113]"));
    }

    #[test]
    fn test_not_implemented_error() {
        let err = NikaError::NotImplemented {
            feature: "dynamic schema validation".to_string(),
            suggestion: "Use static schema for now".to_string(),
        };
        assert_eq!(err.code(), "NIKA-114");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-114]"));
    }

    #[test]
    fn test_agent_execution_error() {
        let err = NikaError::AgentExecutionError {
            task_id: "agent_task".to_string(),
            reason: "provider unreachable".to_string(),
        };
        assert_eq!(err.code(), "NIKA-115");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-115]"));
    }

    #[test]
    fn test_thinking_capture_failed_error() {
        let err = NikaError::ThinkingCaptureFailed {
            reason: "streaming connection lost".to_string(),
        };
        assert_eq!(err.code(), "NIKA-116");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-116]"));
    }

    #[test]
    fn test_thinking_not_supported_error() {
        let err = NikaError::ThinkingNotSupported {
            provider: "openai".to_string(),
        };
        assert_eq!(err.code(), "NIKA-117");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-117]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // RESILIENCE ERRORS (120-129)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_provider_error() {
        let err = NikaError::ProviderError {
            provider: "claude".to_string(),
            reason: "API key invalid".to_string(),
        };
        assert_eq!(err.code(), "NIKA-120");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-120]"));
    }

    #[test]
    fn test_timeout_error() {
        let err = NikaError::Timeout {
            operation: "fetch_data".to_string(),
            duration_ms: 5000,
        };
        assert_eq!(err.code(), "NIKA-121");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-121]"));
        assert!(msg.contains("5000"));
    }

    #[test]
    fn test_mcp_tool_call_failed_error() {
        let err = NikaError::McpToolCallFailed {
            tool: "novanet_atoms".to_string(),
            reason: "malformed response".to_string(),
        };
        assert_eq!(err.code(), "NIKA-125");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-125]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TUI ERRORS (130-139)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_tui_error() {
        let err = NikaError::TuiError {
            reason: "terminal size too small".to_string(),
        };
        assert_eq!(err.code(), "NIKA-130");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-130]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONFIG ERRORS (140-149)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_config_error() {
        let err = NikaError::ConfigError {
            reason: "invalid TOML syntax".to_string(),
        };
        assert_eq!(err.code(), "NIKA-140");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-140]"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // TOOL ERRORS (200-219)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_tool_error() {
        let err = NikaError::ToolError {
            code: "TOOL-001".to_string(),
            message: "File not found".to_string(),
        };
        assert_eq!(err.code(), "NIKA-2XX");
        let msg = err.to_string();
        assert!(msg.contains("TOOL-001"));
        assert!(msg.contains("File not found"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FIX SUGGESTION TRAIT TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_fix_suggestion_for_all_recoverable_errors() {
        let err = NikaError::TaskTimeout {
            task_id: "slow".to_string(),
            timeout_ms: 5000,
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("timeout"));
    }

    #[test]
    fn test_fix_suggestion_for_mcp_validation_with_missing_fields() {
        let err = NikaError::McpValidationFailed {
            tool: "test_tool".to_string(),
            details: "missing required fields".to_string(),
            missing: vec!["field1".to_string()],
            suggestions: vec![],
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("required fields"));
    }

    #[test]
    fn test_fix_suggestion_for_mcp_validation_with_suggestions() {
        let err = NikaError::McpValidationFailed {
            tool: "test_tool".to_string(),
            details: "field mismatch".to_string(),
            missing: vec![],
            suggestions: vec!["Did you mean 'entity'?".to_string()],
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("spelling"));
    }

    #[test]
    fn test_fix_suggestion_for_mcp_validation_default() {
        let err = NikaError::McpValidationFailed {
            tool: "test_tool".to_string(),
            details: "unknown issue".to_string(),
            missing: vec![],
            suggestions: vec![],
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("parameter schema"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // IS_RECOVERABLE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_is_recoverable_task_timeout() {
        let err = NikaError::TaskTimeout {
            task_id: "x".into(),
            timeout_ms: 1000,
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_mcp_not_connected() {
        let err = NikaError::McpNotConnected { name: "x".into() };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_provider_api_error() {
        let err = NikaError::ProviderApiError {
            message: "x".into(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_mcp_tool_error() {
        let err = NikaError::McpToolError {
            tool: "x".into(),
            reason: "y".into(),
            error_code: None,
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_provider_error() {
        let err = NikaError::ProviderError {
            provider: "x".into(),
            reason: "y".into(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_timeout() {
        let err = NikaError::Timeout {
            operation: "x".into(),
            duration_ms: 1000,
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_mcp_timeout() {
        let err = NikaError::McpTimeout {
            name: "x".into(),
            operation: "y".into(),
            timeout_secs: 30,
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_recoverable_mcp_tool_call_failed() {
        let err = NikaError::McpToolCallFailed {
            tool: "x".into(),
            reason: "y".into(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_is_not_recoverable_parse_error() {
        let err = NikaError::ParseError {
            details: "x".into(),
        };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_not_recoverable_validation_error() {
        let err = NikaError::ValidationError { reason: "x".into() };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_is_not_recoverable_cycle_detected() {
        let err = NikaError::CycleDetected { cycle: "x".into() };
        assert!(!err.is_recoverable());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ERROR CODE CONSISTENCY TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_all_workflow_errors_have_correct_codes() {
        assert_eq!(
            NikaError::ParseError {
                details: "x".into()
            }
            .code(),
            "NIKA-001"
        );
        assert_eq!(
            NikaError::InvalidSchemaVersion {
                version: "x".into()
            }
            .code(),
            "NIKA-002"
        );
        assert_eq!(
            NikaError::WorkflowNotFound { path: "x".into() }.code(),
            "NIKA-003"
        );
        assert_eq!(
            NikaError::ValidationError { reason: "x".into() }.code(),
            "NIKA-004"
        );
    }

    #[test]
    fn test_all_dag_errors_have_correct_codes() {
        assert_eq!(
            NikaError::CycleDetected { cycle: "x".into() }.code(),
            "NIKA-020"
        );
        assert_eq!(
            NikaError::MissingDependency {
                task_id: "x".into(),
                dep_id: "y".into()
            }
            .code(),
            "NIKA-021"
        );
    }

    #[test]
    fn test_all_provider_errors_have_correct_codes() {
        assert_eq!(
            NikaError::ProviderNotConfigured {
                provider: "x".into()
            }
            .code(),
            "NIKA-030"
        );
        assert_eq!(
            NikaError::ProviderApiError {
                message: "x".into()
            }
            .code(),
            "NIKA-031"
        );
        assert_eq!(
            NikaError::MissingApiKey {
                provider: "x".into()
            }
            .code(),
            "NIKA-032"
        );
    }

    #[test]
    fn test_all_binding_errors_have_correct_codes() {
        assert_eq!(
            NikaError::BindingError { reason: "x".into() }.code(),
            "NIKA-040"
        );
        assert_eq!(
            NikaError::BindingNotFound { alias: "x".into() }.code(),
            "NIKA-042"
        );
        assert_eq!(
            NikaError::BindingTypeMismatch {
                expected: "x".into(),
                actual: "y".into(),
                path: "z".into()
            }
            .code(),
            "NIKA-043"
        );
    }
}
