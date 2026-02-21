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

    #[test]
    fn test_error_code_extraction() {
        let err = NikaError::McpNotConnected {
            name: "novanet".to_string(),
        };
        assert_eq!(err.code(), "NIKA-100");
    }

    #[test]
    fn test_error_display_includes_code() {
        let err = NikaError::TaskFailed {
            task_id: "gen".to_string(),
            reason: "timeout".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-011]"));
        assert!(msg.contains("gen"));
    }

    #[test]
    fn test_mcp_errors_have_codes() {
        assert_eq!(
            NikaError::McpNotConnected { name: "x".into() }.code(),
            "NIKA-100"
        );
        assert_eq!(
            NikaError::McpStartError {
                name: "x".into(),
                reason: "y".into()
            }
            .code(),
            "NIKA-101"
        );
        assert_eq!(
            NikaError::McpToolError {
                tool: "x".into(),
                reason: "y".into(),
                error_code: None,
            }
            .code(),
            "NIKA-102"
        );
    }

    #[test]
    fn test_agent_errors_have_codes() {
        assert_eq!(
            NikaError::AgentMaxTurns { max_turns: 10 }.code(),
            "NIKA-110"
        );
        assert_eq!(
            NikaError::AgentStopConditionFailed {
                condition: "x".into()
            }
            .code(),
            "NIKA-111"
        );
        assert_eq!(
            NikaError::InvalidToolName { name: "x".into() }.code(),
            "NIKA-112"
        );
    }

    #[test]
    fn test_is_recoverable() {
        assert!(NikaError::TaskTimeout {
            task_id: "x".into(),
            timeout_ms: 1000
        }
        .is_recoverable());
        assert!(NikaError::McpNotConnected { name: "x".into() }.is_recoverable());
        assert!(!NikaError::ParseError {
            details: "x".into()
        }
        .is_recoverable());
    }
}
