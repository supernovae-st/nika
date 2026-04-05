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
//! - NIKA-050-059: Path/task/security errors
//! - NIKA-060-069: Output errors
//! - NIKA-070-079: With block validation errors
//! - NIKA-080-089: DAG validation errors
//! - NIKA-090-099: JSONPath/IO errors (+NIKA-096 Execution catch-all)
//! - NIKA-100-109: MCP errors
//! - NIKA-110-119: Agent errors
//! - NIKA-120-129: Resilience errors
//! - NIKA-130-139: TUI errors
//! - NIKA-140-151: AST analysis errors (Phase 2 analyzer)
//! - NIKA-160-164: Parse errors (Phase 1 parser — ParseErrorKind in nika-core, DO NOT REUSE)
//! - NIKA-165-169: Startup/Policy/Boot errors (165=Policy, 166=Boot, 167=Startup)
//! - NIKA-170-179: Runtime errors (171=DecomposeTimeout)
//!
//! Extended ranges:
//! - NIKA-200-209: File Tool errors (ToolErrorCode in src/tools/mod.rs)
//! - NIKA-210-219: Builtin tool errors
//! - NIKA-220-229: Reserved (DAG Panel - not implemented)
//! - NIKA-230-239: Reserved (Session persistence - not implemented)
//! - NIKA-240-249: Reserved (Animation/Export - not implemented)
//! - NIKA-251-259: Media pipeline errors (MIME, CAS, base64, budget — src/media/error.rs)
//! - NIKA-260-269: Package URI errors
//! - NIKA-270-279: Skill errors
//! - NIKA-280-285: Artifact/media errors (path validation, write, size, integrity, cleanup, lock)
//! - NIKA-290-297: Media tool errors (tool, format, deps, timeout, args, pipeline, security)
//! - NIKA-300-309: Structured Output errors (JSON Schema validation, extraction, repair)
//! - NIKA-310-319: Course errors (course system, exercises, progress)
//! - NIKA-320-324: Record compression errors (compression, JSON parse, confidence, tokens, provider)

use crate::mcp::types::McpErrorCode;
use crate::serde_yaml;
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

/// Format structured output validation errors for display
fn format_validation_errors_short(errors: &[String]) -> String {
    if errors.is_empty() {
        return "no errors".to_string();
    }
    if errors.len() == 1 {
        return errors[0].clone();
    }
    format!("{} errors: {}", errors.len(), errors.join("; "))
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
        help("Use 'nika/workflow@0.12' as the schema version")
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

    #[error("[NIKA-006] Could not determine home directory")]
    #[diagnostic(
        code(nika::home_directory_not_found),
        help("Set the NIKA_HOME environment variable to specify the Nika home directory")
    )]
    HomeDirectoryNotFound,

    // ═══════════════════════════════════════════
    // SCHEMA ERRORS (010-019)
    // ═══════════════════════════════════════════
    #[error("[NIKA-013] Schema file not found for task '{task_id}': {path}")]
    #[diagnostic(
        code(nika::schema_file_not_found),
        help("Ensure the schema file exists relative to the workflow file")
    )]
    SchemaFileNotFound { task_id: String, path: String },

    #[error("[NIKA-014] Invalid JSON in schema file for task '{task_id}': {path}: {reason}")]
    #[diagnostic(
        code(nika::schema_file_invalid),
        help("Ensure the schema file contains valid JSON")
    )]
    SchemaFileInvalid {
        task_id: String,
        path: String,
        reason: String,
    },

    // ═══════════════════════════════════════════
    // DAG ERRORS (020-029)
    // ═══════════════════════════════════════════
    #[error("[NIKA-020] Cycle detected in DAG: {cycle}")]
    CycleDetected { cycle: String },

    #[error("[NIKA-021] Missing dependency: task '{task_id}' depends on unknown '{dep_id}'")]
    MissingDependency { task_id: String, dep_id: String },

    #[error("[NIKA-022] Duplicate task ID: '{task_id}' appears multiple times in workflow")]
    #[diagnostic(
        code(nika::duplicate_task_id),
        help("Each task must have a unique ID. Rename one of the duplicate tasks.")
    )]
    DuplicateTaskId { task_id: String },

    #[error("[NIKA-026] Dependency chain failed: {count} task(s) blocked by failed dependencies")]
    #[diagnostic(
        code(nika::dependency_chain_failed),
        help("One or more upstream tasks failed, blocking downstream tasks. Fix the failing tasks first.")
    )]
    DependencyChainFailed {
        /// Number of tasks blocked
        count: usize,
        /// List of blocked task IDs
        blocked_tasks: Vec<String>,
        /// The root failure that caused the chain
        root_failure: Option<String>,
    },

    #[error("[NIKA-027] Task '{task_id}' was cancelled due to fail_fast")]
    #[diagnostic(
        code(nika::task_cancelled),
        help("Another task in the for_each batch failed with fail_fast=true, causing remaining tasks to be cancelled.")
    )]
    TaskCancelled { task_id: String, reason: String },

    #[error("[NIKA-028] Semaphore closed unexpectedly for task '{task_id}'")]
    #[diagnostic(
        code(nika::semaphore_closed),
        help("The concurrency semaphore was closed before the task could acquire a permit. This is an internal error.")
    )]
    SemaphoreClosed { task_id: String },

    // ═══════════════════════════════════════════
    // PROVIDER ERRORS (030-039)
    // ═══════════════════════════════════════════
    #[error("[NIKA-030] Provider '{provider}' not configured")]
    #[diagnostic(
        code(nika::provider_not_configured),
        help("Run: nika provider list — to see available providers and their setup status")
    )]
    ProviderNotConfigured { provider: String },

    #[error("[NIKA-031] Provider API error: {message}")]
    #[diagnostic(
        code(nika::provider_api_error),
        help("Check your API key and network connection. Run: nika provider test <provider>")
    )]
    ProviderApiError { message: String },

    #[error("[NIKA-032] Missing API key for provider '{provider}'")]
    #[diagnostic(
        code(nika::missing_api_key),
        help(
            "Set it now:   nika provider set {provider} <your-key>\n  Or run:       nika setup   (interactive wizard)\n\n  Run:          nika provider list   to see required env vars for each provider"
        )
    )]
    MissingApiKey { provider: String },

    #[error("[NIKA-033] Invalid configuration: {message}")]
    InvalidConfig { message: String },

    #[error("[NIKA-035] Custom endpoint '{name}' not found in config -- add it to [endpoints.{name}] in ~/.config/nika/config.toml")]
    #[diagnostic(
        code(nika::endpoint_not_found),
        help("Add endpoint to config: nika config edit, or use a known provider name")
    )]
    EndpointNotFound { name: String },

    #[error("[NIKA-036] Cannot connect to custom endpoint '{endpoint}': {reason}")]
    #[diagnostic(
        code(nika::endpoint_connection_failed),
        help("Check that the endpoint URL is reachable and the API key is correct")
    )]
    EndpointConnectionFailed { endpoint: String, reason: String },

    /// All providers in the fallback chain failed.
    #[error("[NIKA-037] Fallback chain exhausted: tried {providers} — all failed. Last error: {last_error}")]
    #[diagnostic(
        code(nika::fallback_chain_exhausted),
        help("Check that at least one provider in the fallback chain is available and working")
    )]
    FallbackChainExhausted {
        providers: String,
        last_error: String,
    },

    /// Workflow exceeded its max_duration_secs timeout (NIKA-038)
    #[error(
        "[NIKA-038] Workflow timed out after {duration_secs}s ({} tasks still running)",
        running_tasks.len()
    )]
    WorkflowTimeout {
        duration_secs: u64,
        running_tasks: Vec<String>,
    },

    // ═══════════════════════════════════════════
    // TEMPLATE/BINDING ERRORS (040-049)
    // ═══════════════════════════════════════════
    /// Simple execution error (catch-all for genuinely misc errors)
    #[error("[NIKA-096] Execution error: {0}")]
    Execution(String),

    #[error("[NIKA-041] Template error in '{template}': {reason}")]
    TemplateError { template: String, reason: String },

    /// [NIKA-044] Exec verb command error (spawn, cwd, shell)
    #[error("[NIKA-044] Exec error: {reason}")]
    #[diagnostic(
        code(nika::exec_error),
        help("Check the command, working directory, and shell configuration")
    )]
    ExecError { reason: String },

    /// [NIKA-045] Fetch verb HTTP error (request, response, URL)
    #[error("[NIKA-045] Fetch error: {reason}")]
    #[diagnostic(
        code(nika::fetch_error),
        help("Check the URL, network connectivity, and response size limits")
    )]
    FetchError { reason: String },

    /// [NIKA-046] Fetch extract mode error (readability, feed, CSS, markdown)
    #[error("[NIKA-046] Extract error: {reason}")]
    #[diagnostic(
        code(nika::extract_error),
        help("Check extract mode, selector syntax, and required features")
    )]
    ExtractError { reason: String },

    /// [NIKA-047] Invoke parameter serialization error
    #[error("[NIKA-047] Invoke params error: {reason}")]
    #[diagnostic(
        code(nika::invoke_param_error),
        help("Check invoke params are valid JSON and template bindings resolve correctly")
    )]
    InvokeParamError { reason: String },

    /// [NIKA-097] Workflow cancelled by user
    #[error("[NIKA-097] Workflow cancelled: {phase}")]
    #[diagnostic(
        code(nika::workflow_cancelled),
        help("The workflow was cancelled by user request or external signal")
    )]
    WorkflowCancelled { phase: String },

    /// [NIKA-098] Task panicked during execution
    #[error("[NIKA-098] Task panicked: {reason}")]
    #[diagnostic(
        code(nika::task_panicked),
        help("A task thread panicked unexpectedly. Check for bugs in task logic.")
    )]
    TaskPanicked { reason: String },

    #[error("[NIKA-042] Binding '{alias}' not found")]
    BindingNotFound { alias: String },

    #[error("[NIKA-043] Binding type mismatch at '{path}': expected {expected}, got {actual}")]
    BindingTypeMismatch {
        expected: String,
        actual: String,
        path: String,
    },

    // ═══════════════════════════════════════════
    // PATH/TASK ERRORS (050-059)
    // ═══════════════════════════════════════════
    #[error("[NIKA-050] Invalid path syntax: {path}")]
    InvalidPath { path: String },

    #[error("[NIKA-052] Path '{path}' not found (task may not have JSON output)")]
    PathNotFound { path: String },

    #[error("[NIKA-053] Command blocked: '{command}' - {reason}")]
    #[diagnostic(
        code(nika::blocked_command),
        help("Use shell: true to opt-in to shell execution, or use a different command")
    )]
    BlockedCommand { command: String, reason: String },

    #[error("[NIKA-055] Invalid task ID '{id}': {reason}")]
    InvalidTaskId { id: String, reason: String },

    #[error("[NIKA-056] Invalid default value '{raw}': {reason}")]
    InvalidDefault { raw: String, reason: String },

    // ═══════════════════════════════════════════
    // OUTPUT ERRORS (060-069)
    // ═══════════════════════════════════════════
    #[error("[NIKA-060] Invalid JSON output: {details}")]
    InvalidJson { details: String },

    #[error("[NIKA-061] Schema validation failed: {details}")]
    SchemaFailed { details: String },

    #[error("[NIKA-062] Serialization error: {details}")]
    SerializationError { details: String },

    // ═══════════════════════════════════════════
    // BINDING VALIDATION (070-079)
    // ═══════════════════════════════════════════
    #[error("[NIKA-071] Unknown alias '{{{{with.{alias}}}}}' in task '{task_id}' — not declared in with: block{}", suggestion.as_ref().map(|s| format!(". Did you mean '{s}'?")).unwrap_or_default())]
    UnknownAlias {
        alias: String,
        task_id: String,
        suggestion: Option<String>,
    },

    #[error("[NIKA-072] Null value at path '{path}' (strict mode)")]
    NullValue { path: String, alias: String },

    #[error("[NIKA-075] Vault access failed for $vault.{service}.{field}: {reason}")]
    VaultAccess {
        service: String,
        field: String,
        reason: String,
    },

    #[error("[NIKA-073] Cannot traverse '{segment}' on {value_type} (expected object/array)")]
    InvalidTraversal {
        segment: String,
        value_type: String,
        full_path: String,
    },

    #[error("[NIKA-074] Template parse error at position {position}: {details}")]
    TemplateParse { position: usize, details: String },

    // ═══════════════════════════════════════════
    // DAG VALIDATION (080-089)
    // ═══════════════════════════════════════════
    #[error("[NIKA-080] with.{alias} references unknown task '{from_task}'{}", suggestion.as_ref().map(|s| format!(". Did you mean '{s}'?")).unwrap_or_default())]
    WithUnknownTask {
        alias: String,
        from_task: String,
        task_id: String,
        suggestion: Option<String>,
    },

    #[error("[NIKA-081] with.{alias}='{from_task}' is not upstream of task '{task_id}'")]
    WithNotUpstream {
        alias: String,
        from_task: String,
        task_id: String,
    },

    #[error("[NIKA-082] with.{alias}='{from_task}' creates circular dependency with '{task_id}'")]
    WithCircularDep {
        alias: String,
        from_task: String,
        task_id: String,
    },

    // ═══════════════════════════════════════════
    // JSONPATH / IO ERRORS (090-099)
    // ═══════════════════════════════════════════
    /// [NIKA-083] Runtime deadlock -- no tasks ready but workflow not complete
    #[error("[NIKA-083] Runtime deadlock: {details}")]
    #[diagnostic(
        code(nika::runtime_deadlock),
        help("Check for circular dependencies or unresolvable task graph structures")
    )]
    RuntimeDeadlock { details: String },

    #[error("[NIKA-090] JSONPath '{path}' uses unsupported syntax (use simple paths like $.a.b or $.a[0].b)")]
    JsonPathUnsupported { path: String },

    #[error("[NIKA-093] IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("[NIKA-094] JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("[NIKA-095] YAML parse error: {0}")]
    #[diagnostic(
        code(nika::yaml_parse),
        help(
            "Check YAML syntax: indentation must be consistent, strings with special chars need quoting"
        )
    )]
    YamlParse(#[from] serde_yaml::Error),

    // ═══════════════════════════════════════════
    // MCP ERRORS (100-109)
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
        /// JSON-RPC error code from MCP server
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
    // AGENT ERRORS (110-119)
    // ═══════════════════════════════════════════
    #[error("[NIKA-113] Agent validation failed: {reason}")]
    AgentValidationError { reason: String },

    #[error("[NIKA-115] Agent execution failed for task '{task_id}': {reason}")]
    AgentExecutionError { task_id: String, reason: String },

    #[error("[NIKA-116] Extended thinking capture failed: {reason}")]
    ThinkingCaptureFailed { reason: String },

    #[error("[NIKA-112] Guardrail violation in task '{task_id}': {}", violations.join(", "))]
    GuardrailViolation {
        task_id: String,
        violations: Vec<String>,
    },

    #[error("[NIKA-114] Agent limit exceeded: {limit_type} ({current} >= {maximum})")]
    AgentLimitExceeded {
        limit_type: String,
        current: f64,
        maximum: f64,
    },

    // ═══════════════════════════════════════════
    // RESILIENCE ERRORS (120-129)
    // ═══════════════════════════════════════════
    #[error("[NIKA-121] Operation '{operation}' timed out after {duration_ms}ms")]
    Timeout { operation: String, duration_ms: u64 },

    #[error("[NIKA-110] MCP tool call '{tool}' failed: {reason}")]
    McpToolCallFailed { tool: String, reason: String },

    // ═══════════════════════════════════════════
    // TUI ERRORS (130-139)
    // ═══════════════════════════════════════════
    #[error("[NIKA-130] TUI error: {reason}")]
    TuiError { reason: String },

    // ═══════════════════════════════════════════
    // CONFIG ERRORS (135-139) - Range reassigned to avoid NIKA-140 collision
    // Note: NIKA-140-149 is reserved for AST analyzer errors (see ast/analyzer/errors.rs)
    // ═══════════════════════════════════════════
    #[error("[NIKA-135] Config error: {reason}")]
    ConfigError { reason: String },

    // ═══════════════════════════════════════════
    // STARTUP / POLICY / BOOT ERRORS (165-169)
    // NIKA-160-164 are reserved for ParseErrorKind in nika-core.
    // ═══════════════════════════════════════════
    #[error("[NIKA-165] Policy violation: {reason}")]
    #[diagnostic(
        code(nika::policy_violation),
        help("Check nika.toml [policy] section or use --allow flag")
    )]
    PolicyViolation { reason: String },

    #[error("[NIKA-166] Boot sequence failed in phase '{phase}': {reason}")]
    #[diagnostic(
        code(nika::boot_failed),
        help("Run 'nika doctor' to diagnose boot issues")
    )]
    BootFailed { phase: String, reason: String },

    #[error("[NIKA-167] Startup verification failed in '{phase}': {reason}")]
    StartupError { phase: String, reason: String },

    // ═══════════════════════════════════════════
    // RUNTIME ERRORS (170-179)
    // ═══════════════════════════════════════════
    #[error(
        "[NIKA-171] Decompose expansion timed out for task '{task_id}': exceeded {timeout_secs}s"
    )]
    #[diagnostic(
        code(nika::decompose_timeout),
        help("The decompose operation took too long. Consider reducing max_depth or max_items, or check MCP server performance.")
    )]
    DecomposeTimeout { task_id: String, timeout_secs: u64 },

    // ═══════════════════════════════════════════
    // TOOL ERRORS (200-209)
    // ═══════════════════════════════════════════
    #[error("[{code}] {message}")]
    ToolError { code: String, message: String },

    // ═══════════════════════════════════════════
    // BUILTIN TOOL ERRORS (210-219)
    // ═══════════════════════════════════════════
    #[error("[NIKA-210] Builtin tool '{tool}' error: {reason}")]
    #[diagnostic(
        code(nika::builtin_tool_error),
        help("Check builtin tool parameters and configuration")
    )]
    BuiltinToolError { tool: String, reason: String },

    #[error("[NIKA-212] Builtin tool '{tool}' invalid parameters: {reason}")]
    #[diagnostic(
        code(nika::builtin_invalid_params),
        help("Check the parameter format matches the expected JSON schema")
    )]
    BuiltinInvalidParams { tool: String, reason: String },

    #[error("[NIKA-213] Assertion failed in nika:assert: {message}")]
    #[diagnostic(code(nika::assertion_failed), help("The condition evaluated to false"))]
    AssertionFailed { message: String, condition: String },

    // ═══════════════════════════════════════════
    // CONTEXT ERROR (250)
    // ═══════════════════════════════════════════
    #[error("[NIKA-250] Failed to load context file '{alias}' from '{path}': {reason}")]
    #[diagnostic(
        code(nika::context_load_error),
        help("Check the file path exists and is readable")
    )]
    ContextLoadError {
        alias: String,
        path: String,
        reason: String,
    },

    // ═══════════════════════════════════════════
    // MEDIA ERRORS (251-259)
    // ═══════════════════════════════════════════
    /// Media pipeline error (NIKA-251..259)
    /// Note: miette diagnostic codes are forwarded via MediaError's own Diagnostic derive.
    #[error(transparent)]
    MediaError(#[from] crate::media::error::MediaError),

    // ═══════════════════════════════════════════
    // PKG URI ERRORS (260-269)
    // ═══════════════════════════════════════════
    #[error("[NIKA-260] Invalid pkg: URI '{uri}': {reason}")]
    #[diagnostic(
        code(nika::invalid_pkg_uri),
        help("Format: pkg:@scope/name@version/path or pkg:@scope/name/path")
    )]
    InvalidPkgUri { uri: String, reason: String },

    #[error("[NIKA-261] Package '{name}@{version}' not found in registry")]
    #[diagnostic(
        code(nika::package_not_found),
        help("Install the package with: nika pkg install {name}@{version}")
    )]
    PackageNotFound { name: String, version: String },

    // ═══════════════════════════════════════════
    // SKILL ERRORS (270-279)
    // ═══════════════════════════════════════════
    #[error("[NIKA-270] Failed to load skill '{skill}': {reason}")]
    #[diagnostic(
        code(nika::skill_load_error),
        help("Ensure skill file exists and is readable. Check pkg: URI format if using packages.")
    )]
    SkillLoadError { skill: String, reason: String },

    // ═══════════════════════════════════════════
    // ARTIFACT ERRORS (280-289)
    // ═══════════════════════════════════════════
    #[error("[NIKA-280] Artifact path error for '{path}': {reason}")]
    #[diagnostic(
        code(nika::artifact_path_error),
        help("Check the artifact path is within the workflow directory and does not contain path traversal patterns")
    )]
    ArtifactPathError { path: String, reason: String },

    #[error("[NIKA-281] Artifact write failed for '{path}': {reason}")]
    #[diagnostic(
        code(nika::artifact_write_error),
        help("Check file permissions and disk space")
    )]
    ArtifactWriteError { path: String, reason: String },

    #[error("[NIKA-282] Artifact size exceeds limit: {size} bytes > {max_size} bytes")]
    #[diagnostic(
        code(nika::artifact_size_exceeded),
        help("Increase artifacts.max_size in workflow or reduce output size")
    )]
    ArtifactSizeExceeded {
        path: String,
        size: u64,
        max_size: u64,
    },

    #[error("[NIKA-285] Media store is locked: {reason}")]
    #[diagnostic(
        code(nika::media_store_locked),
        help("A workflow is currently running. Use --force to override or wait for completion")
    )]
    MediaStoreLocked { reason: String },

    // ═══════════════════════════════════════════
    // STRUCTURED OUTPUT ERRORS (300-309)
    // ═══════════════════════════════════════════
    #[error(
        "[NIKA-300] Structured output extraction failed for task '{task_id}' at {layer}: {reason}"
    )]
    #[diagnostic(
        code(nika::structured_output_extraction_failed),
        help("Check the LLM response format matches the expected JSON Schema")
    )]
    StructuredOutputExtractionFailed {
        task_id: String,
        layer: String,
        reason: String,
    },

    #[error("[NIKA-301] Structured output validation failed for task '{task_id}' at {layer} (attempt {attempt}): {}", format_validation_errors_short(.errors))]
    #[diagnostic(
        code(nika::structured_output_validation_failed),
        help("Fix JSON output to match the declared schema")
    )]
    StructuredOutputValidationFailed {
        task_id: String,
        layer: String,
        attempt: u32,
        errors: Vec<String>,
    },

    #[error("[NIKA-302] Structured output repair failed for task '{task_id}': original errors: {original_errors:?}, repair errors: {repair_errors:?}")]
    #[diagnostic(
        code(nika::structured_output_repair_failed),
        help("The LLM could not repair the output. Consider simplifying the schema or providing more context.")
    )]
    StructuredOutputRepairFailed {
        task_id: String,
        original_errors: Vec<String>,
        repair_errors: Vec<String>,
    },

    #[error("[NIKA-303] Structured output failed after {attempts} attempts for task '{task_id}': {}", format_validation_errors_short(.final_errors))]
    #[diagnostic(
        code(nika::structured_output_all_layers_failed),
        help("All layers exhausted (L0: response_format/tool_injection, L2: validate, L3: retry, L4: repair). Suggestions: simplify the schema, add 'required' fields, increase max_retries, or provide clearer prompt context.")
    )]
    StructuredOutputAllLayersFailed {
        task_id: String,
        attempts: u32,
        final_errors: Vec<String>,
    },

    // ═══════════════════════════════════════════
    // COURSE ERRORS (310-319)
    // ═══════════════════════════════════════════
    #[error("[NIKA-310] Course not found at '{path}'")]
    #[diagnostic(
        code(nika::course_not_found),
        help("Run `nika init --course` to create a course")
    )]
    CourseNotFound { path: String },

    #[error("[NIKA-311] Course exercise check failed for '{exercise}': {reason}")]
    #[diagnostic(
        code(nika::course_check_failed),
        help("Review the exercise instructions and fix your workflow")
    )]
    CourseCheckFailed { exercise: String, reason: String },

    #[error("[NIKA-312] Course level '{level}' is locked (complete level {prerequisite} first)")]
    #[diagnostic(
        code(nika::course_level_locked),
        help("Complete all exercises in the prerequisite level before unlocking this one")
    )]
    CourseLevelLocked { level: String, prerequisite: u8 },

    #[error("[NIKA-313] Course progress corrupted: {reason}")]
    #[diagnostic(
        code(nika::course_progress_corrupted),
        help("Delete .nika/course-progress.json and restart the course")
    )]
    CourseProgressCorrupted { reason: String },

    #[error("[NIKA-314] Course watch error: {reason}")]
    #[diagnostic(
        code(nika::course_watch_error),
        help("Check file permissions and that the course directory exists")
    )]
    CourseWatchError { reason: String },

    // ── Record compression (NIKA-320-324) ────────────────────────
    #[error("[NIKA-320] Record compression failed for task '{task_id}': {reason}")]
    #[diagnostic(
        code(nika::record_compression_failed),
        help("Compression is non-fatal — output was truncated instead")
    )]
    RecordCompressionFailed { task_id: String, reason: String },

    #[error("[NIKA-321] Record compression returned invalid JSON for task '{task_id}': {reason}")]
    #[diagnostic(
        code(nika::record_invalid_json),
        help("Ensure the compression model returns valid JSON")
    )]
    RecordInvalidJson { task_id: String, reason: String },

    #[error("[NIKA-322] Record confidence {confidence:.2} below threshold {threshold:.2} for task '{task_id}'")]
    #[diagnostic(
        code(nika::record_low_confidence),
        help("Lower the confidence_threshold or use a more capable compression model")
    )]
    RecordLowConfidence {
        task_id: String,
        confidence: f64,
        threshold: f64,
    },

    #[error("[NIKA-323] Record max_tokens exceeded for task '{task_id}': {tokens} > {max}")]
    #[diagnostic(
        code(nika::record_tokens_exceeded),
        help("Increase max_tokens in the record: spec or use a shorter prompt")
    )]
    RecordTokensExceeded {
        task_id: String,
        tokens: u64,
        max: u32,
    },

    #[error("[NIKA-324] No compression provider available for task '{task_id}'")]
    #[diagnostic(
        code(nika::record_no_provider),
        help("Add a 'summary' agent to the agents: block or configure a default provider")
    )]
    RecordNoProvider { task_id: String },
}

impl From<nika_core::error::CoreError> for NikaError {
    fn from(e: nika_core::error::CoreError) -> Self {
        match e {
            nika_core::error::CoreError::InvalidPath { path } => NikaError::InvalidPath { path },
            nika_core::error::CoreError::InvalidDefault { raw, reason } => {
                NikaError::InvalidDefault { raw, reason }
            }
            nika_core::error::CoreError::ValidationError { reason } => {
                NikaError::ValidationError { reason }
            }
        }
    }
}

impl From<nika_event::EventError> for NikaError {
    fn from(e: nika_event::EventError) -> Self {
        match e {
            nika_event::EventError::TraceWrite(io) => NikaError::IoError(io),
            nika_event::EventError::Serialization(json) => NikaError::JsonError(json),
        }
    }
}

impl From<nika_mcp::McpError> for NikaError {
    fn from(e: nika_mcp::McpError) -> Self {
        match e {
            nika_mcp::McpError::McpNotConnected { name } => NikaError::McpNotConnected { name },
            nika_mcp::McpError::McpStartError { name, reason } => {
                NikaError::McpStartError { name, reason }
            }
            nika_mcp::McpError::McpToolError {
                tool,
                reason,
                error_code,
            } => NikaError::McpToolError {
                tool,
                reason,
                error_code,
            },
            nika_mcp::McpError::McpResourceNotFound { uri } => {
                NikaError::McpResourceNotFound { uri }
            }
            nika_mcp::McpError::McpProtocolError { reason } => {
                NikaError::McpProtocolError { reason }
            }
            nika_mcp::McpError::McpNotConfigured { name } => NikaError::McpNotConfigured { name },
            nika_mcp::McpError::McpInvalidResponse { tool, reason } => {
                NikaError::McpInvalidResponse { tool, reason }
            }
            nika_mcp::McpError::McpValidationFailed {
                tool,
                details,
                missing,
                suggestions,
            } => NikaError::McpValidationFailed {
                tool,
                details,
                missing,
                suggestions,
            },
            nika_mcp::McpError::McpSchemaError { tool, reason } => {
                NikaError::McpSchemaError { tool, reason }
            }
            nika_mcp::McpError::McpTimeout {
                name,
                operation,
                timeout_secs,
            } => NikaError::McpTimeout {
                name,
                operation,
                timeout_secs,
            },
            nika_mcp::McpError::McpToolCallFailed { tool, reason } => {
                NikaError::McpToolCallFailed { tool, reason }
            }
            nika_mcp::McpError::ConfigError { reason } => NikaError::ConfigError { reason },
            nika_mcp::McpError::ValidationError { reason } => NikaError::ValidationError { reason },
            nika_mcp::McpError::ParseError { details } => NikaError::ParseError { details },
            nika_mcp::McpError::Io(e) => NikaError::IoError(e),
            nika_mcp::McpError::Json(e) => NikaError::JsonError(e),
        }
    }
}

impl From<nika_media::tools::error::MediaToolError> for NikaError {
    fn from(e: nika_media::tools::error::MediaToolError) -> Self {
        match e {
            nika_media::tools::error::MediaToolError::ToolError { tool, reason } => {
                NikaError::BuiltinToolError {
                    tool: format!("nika:{tool}"),
                    reason,
                }
            }
            nika_media::tools::error::MediaToolError::UnsupportedFormat { tool, mime } => {
                NikaError::BuiltinToolError {
                    tool: format!("nika:{tool}"),
                    reason: format!("[NIKA-291] unsupported format '{mime}'"),
                }
            }
            nika_media::tools::error::MediaToolError::DependencyMissing { tool, feature } => {
                NikaError::BuiltinToolError {
                    tool: format!("nika:{tool}"),
                    reason: format!("[NIKA-292] feature '{feature}' required"),
                }
            }
            nika_media::tools::error::MediaToolError::Timeout { tool } => {
                NikaError::BuiltinToolError {
                    tool: format!("nika:{tool}"),
                    reason: "[NIKA-293] operation timed out".to_string(),
                }
            }
            nika_media::tools::error::MediaToolError::InvalidArgs { tool, reason } => {
                NikaError::BuiltinInvalidParams {
                    tool: format!("nika:{tool}"),
                    reason: format!("[NIKA-294] {reason}"),
                }
            }
            nika_media::tools::error::MediaToolError::PipelineStepFailed { step, reason } => {
                NikaError::BuiltinToolError {
                    tool: "nika:pipeline".to_string(),
                    reason: format!("[NIKA-295] step {step} failed: {reason}"),
                }
            }
            nika_media::tools::error::MediaToolError::PipelineEmpty => {
                NikaError::BuiltinToolError {
                    tool: "nika:pipeline".to_string(),
                    reason: "[NIKA-296] pipeline has no steps".to_string(),
                }
            }
            nika_media::tools::error::MediaToolError::SecurityViolation { tool, reason } => {
                NikaError::BuiltinToolError {
                    tool: format!("nika:{tool}"),
                    reason: format!("[NIKA-297] security violation: {reason}"),
                }
            }
            nika_media::tools::error::MediaToolError::Media(me) => me.into(),
        }
    }
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
            Self::HomeDirectoryNotFound => "NIKA-006",
            // Schema errors
            Self::SchemaFileNotFound { .. } => "NIKA-013",
            Self::SchemaFileInvalid { .. } => "NIKA-014",
            // DAG errors
            Self::CycleDetected { .. } => "NIKA-020",
            Self::MissingDependency { .. } => "NIKA-021",
            Self::DuplicateTaskId { .. } => "NIKA-022",
            Self::DependencyChainFailed { .. } => "NIKA-026",
            Self::TaskCancelled { .. } => "NIKA-027",
            Self::SemaphoreClosed { .. } => "NIKA-028",
            // Provider errors
            Self::ProviderNotConfigured { .. } => "NIKA-030",
            Self::ProviderApiError { .. } => "NIKA-031",
            Self::MissingApiKey { .. } => "NIKA-032",
            Self::InvalidConfig { .. } => "NIKA-033",
            Self::EndpointNotFound { .. } => "NIKA-035",
            Self::EndpointConnectionFailed { .. } => "NIKA-036",
            Self::FallbackChainExhausted { .. } => "NIKA-037",
            Self::WorkflowTimeout { .. } => "NIKA-038",
            // Binding/Template errors
            Self::Execution(_) => "NIKA-096",
            Self::TemplateError { .. } => "NIKA-041",
            Self::ExecError { .. } => "NIKA-044",
            Self::FetchError { .. } => "NIKA-045",
            Self::ExtractError { .. } => "NIKA-046",
            Self::InvokeParamError { .. } => "NIKA-047",
            Self::WorkflowCancelled { .. } => "NIKA-097",
            Self::TaskPanicked { .. } => "NIKA-098",
            Self::BindingNotFound { .. } => "NIKA-042",
            Self::BindingTypeMismatch { .. } => "NIKA-043",
            // Path/Task errors
            Self::InvalidPath { .. } => "NIKA-050",
            Self::PathNotFound { .. } => "NIKA-052",
            Self::BlockedCommand { .. } => "NIKA-053",
            Self::InvalidTaskId { .. } => "NIKA-055",
            Self::InvalidDefault { .. } => "NIKA-056",
            // Output errors
            Self::InvalidJson { .. } => "NIKA-060",
            Self::SchemaFailed { .. } => "NIKA-061",
            Self::SerializationError { .. } => "NIKA-062",
            // With block errors
            Self::UnknownAlias { .. } => "NIKA-071",
            Self::NullValue { .. } => "NIKA-072",
            Self::VaultAccess { .. } => "NIKA-075",
            Self::InvalidTraversal { .. } => "NIKA-073",
            Self::TemplateParse { .. } => "NIKA-074",
            // DAG validation errors
            Self::WithUnknownTask { .. } => "NIKA-080",
            Self::WithNotUpstream { .. } => "NIKA-081",
            Self::WithCircularDep { .. } => "NIKA-082",
            Self::RuntimeDeadlock { .. } => "NIKA-083",
            // JSONPath/IO errors
            Self::JsonPathUnsupported { .. } => "NIKA-090",
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
            Self::AgentValidationError { .. } => "NIKA-113",
            Self::AgentExecutionError { .. } => "NIKA-115",
            Self::ThinkingCaptureFailed { .. } => "NIKA-116",
            Self::GuardrailViolation { .. } => "NIKA-112",
            Self::AgentLimitExceeded { .. } => "NIKA-114",
            // Resilience errors
            Self::Timeout { .. } => "NIKA-121",
            Self::McpToolCallFailed { .. } => "NIKA-110",
            // TUI errors
            Self::TuiError { .. } => "NIKA-130",
            // Config errors
            Self::ConfigError { .. } => "NIKA-135",
            // Startup errors
            Self::StartupError { .. } => "NIKA-167",
            // Tool errors (code is dynamic)
            Self::ToolError { .. } => "NIKA-200",
            // Builtin tool errors
            Self::BuiltinToolError { .. } => "NIKA-210",
            Self::BuiltinInvalidParams { .. } => "NIKA-212",
            Self::AssertionFailed { .. } => "NIKA-213",
            // Context errors
            Self::ContextLoadError { .. } => "NIKA-250",
            // Media errors
            Self::MediaError(e) => e.code(),
            // Pkg URI errors
            Self::InvalidPkgUri { .. } => "NIKA-260",
            // Package errors
            Self::PackageNotFound { .. } => "NIKA-261",

            // Skill errors
            Self::SkillLoadError { .. } => "NIKA-270",
            // Artifact errors
            Self::ArtifactPathError { .. } => "NIKA-280",
            Self::ArtifactWriteError { .. } => "NIKA-281",
            Self::ArtifactSizeExceeded { .. } => "NIKA-282",
            Self::MediaStoreLocked { .. } => "NIKA-285",
            // Structured Output errors
            Self::StructuredOutputExtractionFailed { .. } => "NIKA-300",
            Self::StructuredOutputValidationFailed { .. } => "NIKA-301",
            Self::StructuredOutputRepairFailed { .. } => "NIKA-302",
            Self::StructuredOutputAllLayersFailed { .. } => "NIKA-303",
            // Course errors
            Self::CourseNotFound { .. } => "NIKA-310",
            Self::CourseCheckFailed { .. } => "NIKA-311",
            Self::CourseLevelLocked { .. } => "NIKA-312",
            Self::CourseProgressCorrupted { .. } => "NIKA-313",
            Self::CourseWatchError { .. } => "NIKA-314",
            // Record compression errors
            Self::RecordCompressionFailed { .. } => "NIKA-320",
            Self::RecordInvalidJson { .. } => "NIKA-321",
            Self::RecordLowConfidence { .. } => "NIKA-322",
            Self::RecordTokensExceeded { .. } => "NIKA-323",
            Self::RecordNoProvider { .. } => "NIKA-324",
            // Policy errors (renumbered from 160/161 to avoid ParseErrorKind collision)
            Self::PolicyViolation { .. } => "NIKA-165",
            Self::BootFailed { .. } => "NIKA-166",
            // Runtime errors
            Self::DecomposeTimeout { .. } => "NIKA-171",
        }
    }

    /// Convenience: look up fix suggestion by error code string (e.g., "NIKA-044").
    ///
    /// Used by the display layer which only has the error code from TaskFailed events.
    pub fn fix_for_code(code: &str) -> Option<&'static str> {
        fix_suggestion_for_code(code)
    }

    /// Check if error is recoverable (can be retried)
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::McpNotConnected { .. }
            | Self::ProviderApiError { .. }
            | Self::EndpointConnectionFailed { .. }
            | Self::McpToolError { .. }
            | Self::Timeout { .. }
            | Self::McpTimeout { .. }
            | Self::McpToolCallFailed { .. }
            // Fetch errors are transient HTTP issues (retryable)
            | Self::FetchError { .. }
            // Structured output errors that can be retried
            | Self::StructuredOutputExtractionFailed { .. }
            | Self::StructuredOutputValidationFailed { .. }
            | Self::StructuredOutputRepairFailed { .. }
            // Record compression failures are non-fatal (fallback to truncation)
            | Self::RecordCompressionFailed { .. }
            | Self::RecordInvalidJson { .. }
            | Self::RecordLowConfidence { .. } => true,
            // Delegate to MediaError's own is_recoverable (only I/O errors)
            Self::MediaError(e) => e.is_recoverable(),
            _ => false,
        }
    }
}

impl FixSuggestion for NikaError {
    fn fix_suggestion(&self) -> Option<&str> {
        match self {
            NikaError::ParseError { .. } => Some("Check YAML syntax: indentation and quoting"),
            NikaError::InvalidSchemaVersion { .. } => {
                Some("Use 'nika/workflow@0.12' as the schema version")
            }
            NikaError::WorkflowNotFound { .. } => Some("Check the file path exists"),
            NikaError::ValidationError { .. } => Some("Check workflow structure matches schema"),
            NikaError::SchemaValidationFailed { .. } => {
                Some("Check YAML against schemas/nika-workflow.schema.json")
            }
            NikaError::HomeDirectoryNotFound => {
                Some("Set NIKA_HOME environment variable to specify Nika home directory")
            }
            NikaError::YamlParse(_) => Some("Check YAML syntax: indentation and quoting"),
            NikaError::SchemaFileNotFound { .. } => {
                Some("Check the schema file path is correct relative to the workflow file")
            }
            NikaError::SchemaFileInvalid { .. } => {
                Some("Ensure the schema file contains valid JSON (not YAML)")
            }
            NikaError::CycleDetected { .. } => {
                Some("Remove circular dependencies from your workflow")
            }
            NikaError::MissingDependency { .. } => {
                Some("Add the missing task or fix the dependency reference")
            }
            NikaError::ProviderNotConfigured { .. } => {
                Some("Add provider configuration to your workflow")
            }
            NikaError::ProviderApiError { .. } => Some("Check API key and provider availability"),
            NikaError::MissingApiKey { .. } => {
                Some("Run `nika setup` to configure a provider, or set the env var (e.g. ANTHROPIC_API_KEY)")
            }
            NikaError::InvalidConfig { .. } => Some("Check configuration value is valid"),
            NikaError::EndpointNotFound { .. } => {
                Some("Add [endpoints.<name>] to ~/.config/nika/config.toml")
            }
            NikaError::EndpointConnectionFailed { .. } => {
                Some("Check endpoint URL, network, and API key")
            }
            NikaError::Execution(_) => Some("Check command/URL is valid"),
            NikaError::ExecError { .. } => {
                Some("Check command syntax, working directory, and shell availability")
            }
            NikaError::FetchError { .. } => {
                Some("Check URL, network connectivity, and response size limits")
            }
            NikaError::ExtractError { .. } => {
                Some("Check extract mode name, CSS selector syntax, or enable required feature")
            }
            NikaError::InvokeParamError { .. } => {
                Some("Check invoke params are valid JSON and template bindings resolve correctly")
            }
            NikaError::WorkflowCancelled { .. } => {
                Some("The workflow was cancelled. No action needed.")
            }
            NikaError::TaskPanicked { .. } => {
                Some("A task panicked unexpectedly. This is likely a bug — check task logic.")
            }
            NikaError::TemplateError { .. } => Some("Use {{with.alias}} format with with: block"),
            NikaError::InvalidPath { .. } => Some("Use format: task_id.field.subfield"),
            NikaError::PathNotFound { .. } => Some("Add '?? default' or ensure task outputs JSON"),
            NikaError::BlockedCommand { .. } => {
                Some("Use shell: true to opt-in to shell execution, or use a different command")
            }
            NikaError::InvalidTaskId { .. } => {
                Some("Task IDs must be snake_case: lowercase letters, digits, underscores")
            }
            NikaError::InvalidDefault { .. } => {
                Some("Default values must be valid JSON. Strings must be quoted.")
            }
            NikaError::InvalidJson { .. } => Some("Ensure output is valid JSON"),
            NikaError::SchemaFailed { .. } => Some("Fix output to match declared schema"),
            NikaError::SerializationError { .. } => Some("Check data structure is serializable"),
            NikaError::UnknownAlias { .. } => {
                Some("Declare the alias in with: block before referencing")
            }
            NikaError::NullValue { .. } => {
                Some("Provide a default value or ensure non-null output")
            }
            NikaError::VaultAccess { .. } => {
                Some("Check: nika provider set <service> or use ?? default. Run: nika doctor --fix")
            }
            NikaError::InvalidTraversal { .. } => {
                Some("Check the path - accessing field on non-object")
            }
            NikaError::TemplateParse { .. } => Some("Check template syntax: {{with.alias}}"),
            NikaError::WithUnknownTask { .. } => Some("Verify the task_id exists in your workflow"),
            NikaError::WithNotUpstream { .. } => {
                Some("Add depends_on: [source_task] to this task")
            }
            NikaError::WithCircularDep { .. } => Some("Remove the circular dependency"),
            NikaError::RuntimeDeadlock { .. } => {
                Some("Check for circular dependencies or unresolvable task graph structures")
            }
            NikaError::DependencyChainFailed { .. } => {
                Some("Fix the root task failure, then re-run the workflow")
            }
            NikaError::JsonPathUnsupported { .. } => Some("Use simple paths like $.field.subfield"),
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
                Some("Verify the binding alias exists in with: block or task outputs")
            }
            NikaError::BindingTypeMismatch { .. } => {
                Some("Check binding value type matches expected type")
            }
            // Agent errors
            NikaError::AgentValidationError { .. } => {
                Some("Check agent prompt is not empty and max_turns is valid (1-100)")
            }
            NikaError::AgentExecutionError { .. } => {
                Some("Check LLM provider API key and network connectivity")
            }
            NikaError::ThinkingCaptureFailed { .. } => {
                Some("Check Claude API response and streaming connection")
            }
            NikaError::GuardrailViolation { .. } => {
                Some("One or more guardrails failed. Check guardrail config or adjust on_failure action")
            }
            // Resilience errors
            NikaError::AgentLimitExceeded { .. } => {
                Some("Increase limits or set on_limit_reached.action to complete_partial")
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
            // Startup errors
            NikaError::StartupError { .. } => Some(
                "Check directory permissions and run 'nika init' to create required directories",
            ),
            // Tool errors
            NikaError::ToolError { .. } => {
                Some("Check file path and permissions. Use Read before Edit.")
            }
            // Builtin tool errors
            NikaError::BuiltinToolError { .. } => {
                Some("Check builtin tool parameters and configuration")
            }
            NikaError::BuiltinInvalidParams { .. } => {
                Some("Check the parameter format matches the expected JSON schema")
            }
            NikaError::AssertionFailed { .. } => Some("The condition evaluated to false"),
            // Context errors
            NikaError::ContextLoadError { .. } => {
                Some("Check the file path exists and is readable")
            }
            // Media errors
            NikaError::MediaError(_) => {
                Some("Check media content and CAS store configuration")
            }
            // Pkg URI errors
            NikaError::InvalidPkgUri { .. } => Some(
                "Use format: pkg:@scope/name@version/path (e.g., pkg:@supernovae/skills@1.0.0/rust.md)",
            ),
            // Package errors
            NikaError::PackageNotFound { .. } => Some(
                "Check package name and version. Run 'nika pkg list' to see installed packages.",
            ),
            // Policy errors
            NikaError::PolicyViolation { .. } => Some(
                "This action was blocked by security policy. Check nika.toml [policy] section.",
            ),
            NikaError::BootFailed { .. } => {
                Some("Boot sequence failed. Run 'nika doctor' to diagnose.")
            }
            // Skill errors
            NikaError::SkillLoadError { .. } => {
                Some("Ensure skill file exists and is readable. Check pkg: URI format if using packages.")
            }
            // Decompose timeout
            NikaError::DecomposeTimeout { .. } => {
                Some("Decompose expansion timed out. Try reducing max_items or check MCP server performance.")
            }
            // Artifact errors
            NikaError::ArtifactPathError { .. } => {
                Some("Check the artifact path is within the workflow directory and does not contain path traversal patterns")
            }
            NikaError::ArtifactWriteError { .. } => {
                Some("Check file permissions and disk space")
            }
            NikaError::ArtifactSizeExceeded { .. } => {
                Some("Increase artifacts.max_size in workflow or reduce output size")
            }
            NikaError::MediaStoreLocked { .. } => {
                Some("A workflow is currently running. Use --force to override or wait for completion")
            }
            // Structured Output errors
            NikaError::StructuredOutputExtractionFailed { .. } => {
                Some("Check the LLM response format matches the expected JSON Schema")
            }
            NikaError::StructuredOutputValidationFailed { .. } => {
                Some("Fix JSON output to match the declared schema. Check required fields and types.")
            }
            NikaError::StructuredOutputRepairFailed { .. } => {
                Some("The LLM could not repair the output. Consider simplifying the schema or providing more context.")
            }
            NikaError::StructuredOutputAllLayersFailed { .. } => {
                Some("All validation layers failed. Check your schema is valid and the prompt provides enough context for the LLM to generate conforming output.")
            }
            NikaError::TaskCancelled { .. } => {
                Some("Task was cancelled. Check workflow execution logs.")
            }
            NikaError::SemaphoreClosed { .. } => {
                Some("Internal error — concurrency semaphore closed unexpectedly. Please report this bug.")
            }
            // Duplicate task ID
            NikaError::DuplicateTaskId { .. } => {
                Some("Each task must have a unique ID. Rename one of the duplicate tasks.")
            }
            // Course errors
            NikaError::CourseNotFound { .. } => {
                Some("Run `nika init --course` to create a course")
            }
            NikaError::CourseCheckFailed { .. } => {
                Some("Review the exercise instructions and fix your workflow")
            }
            NikaError::CourseLevelLocked { .. } => {
                Some("Complete all exercises in the prerequisite level before unlocking this one")
            }
            NikaError::CourseProgressCorrupted { .. } => {
                Some("Delete .nika/course-progress.json and restart the course")
            }
            NikaError::CourseWatchError { .. } => {
                Some("Check file permissions and that the course directory exists")
            }
            NikaError::FallbackChainExhausted { .. } => {
                Some("Add more providers to routing.fallback or check provider health")
            }
            NikaError::WorkflowTimeout { .. } => {
                Some("Increase max_duration_secs in workflow header or optimize slow tasks")
            }
            // Record compression errors
            NikaError::RecordCompressionFailed { .. } => {
                Some("Compression is non-fatal — output was truncated. Check the compression model.")
            }
            NikaError::RecordInvalidJson { .. } => {
                Some("The compression model returned invalid JSON. Try a different model.")
            }
            NikaError::RecordLowConfidence { .. } => {
                Some("Lower confidence_threshold in record: spec or use a more capable model.")
            }
            NikaError::RecordTokensExceeded { .. } => {
                Some("Increase max_tokens in record: spec.")
            }
            NikaError::RecordNoProvider { .. } => {
                Some("Add a 'summary' agent to agents: block or configure a default provider.")
            }
        }
    }
}

/// Look up a fix suggestion by error code string (e.g., "NIKA-044").
///
/// This is a code-only lookup table, separate from the full NikaError enum,
/// for use by the display layer which only has the error code from events.
pub fn fix_suggestion_for_code(code: &str) -> Option<&'static str> {
    match code {
        "NIKA-001" | "NIKA-095" => Some("Check YAML syntax: indentation and quoting"),
        "NIKA-002" => Some("Use schema: \"nika/workflow@0.12\""),
        "NIKA-003" => Some("Check the file path exists"),
        "NIKA-020" => Some("Remove circular dependencies from your workflow"),
        "NIKA-021" => Some("Verify all task IDs in depends_on: exist"),
        "NIKA-030" => Some("Add provider: to your workflow header"),
        "NIKA-032" => Some("Set the API key env var (e.g., ANTHROPIC_API_KEY)"),
        "NIKA-041" | "NIKA-074" => Some("Use {{with.alias}} format with with: block"),
        "NIKA-042" => Some("Add the alias to your with: block"),
        "NIKA-044" => Some("Check command syntax and working directory"),
        "NIKA-045" => Some("Check URL, network connectivity, and response size limits"),
        "NIKA-046" => Some("Check extract mode name or enable required feature"),
        "NIKA-052" => Some("Add '?? default' or ensure task outputs JSON"),
        "NIKA-053" => {
            Some("This command is blocked for security. Use shell: true or a different command")
        }
        "NIKA-072" => Some("Provide a default value or ensure non-null output"),
        "NIKA-080" => Some("Verify the task_id exists in your workflow"),
        "NIKA-081" => Some("Add depends_on: [source_task] to this task"),
        "NIKA-100" => Some("Check MCP server is running and configured correctly"),
        "NIKA-105" => Some("Add mcp_servers: to your workflow header"),
        "NIKA-121" => Some("Increase the timeout: value (in seconds)"),
        "NIKA-165" => Some("Review the security policy — some URLs or commands may be blocked"),
        // Codes from FixSuggestion impl that may appear in TaskFailed events
        "NIKA-004" => Some("Check workflow structure matches schema"),
        "NIKA-005" => Some("Check YAML against schemas/nika-workflow.schema.json"),
        "NIKA-022" => Some("Each task must have a unique ID"),
        "NIKA-026" => Some("Fix the root task failure, then re-run"),
        "NIKA-027" => Some("Task was cancelled due to upstream failure"),
        "NIKA-028" => Some("Internal error — concurrency semaphore closed unexpectedly"),
        "NIKA-033" => Some("Check configuration value is valid"),
        "NIKA-035" => Some("Check endpoint name matches a [endpoints.NAME] in config.toml"),
        "NIKA-036" => Some("Check the endpoint URL is reachable and the API key is correct"),
        "NIKA-037" => Some("Add more providers to routing.fallback or check provider health"),
        "NIKA-043" => Some("Check binding value type matches expected type"),
        "NIKA-047" => Some("Check invoke params are valid JSON"),
        "NIKA-096" => Some("Check command/URL is valid"),
        "NIKA-097" => Some("The workflow was cancelled. No action needed."),
        "NIKA-098" => Some("A task panicked unexpectedly — check task logic"),
        "NIKA-034" => Some(
            "Add model: to your workflow header or task (e.g., model: claude-sonnet-4-20250514)",
        ),
        "NIKA-071" => Some("Check that the alias is declared in the task's with: block"),
        "NIKA-083" => Some("Check for circular dependencies or unresolvable task graph"),
        "NIKA-110" => Some("Check MCP tool parameters and server availability"),
        "NIKA-112" => {
            Some("Check guardrail configuration — output did not meet quality constraints")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

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
    fn test_schema_file_not_found_error() {
        let err = NikaError::SchemaFileNotFound {
            task_id: "extract".to_string(),
            path: "./schemas/user.json".to_string(),
        };
        assert_eq!(err.code(), "NIKA-013");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-013]"));
        assert!(msg.contains("extract"));
        assert!(msg.contains("./schemas/user.json"));
    }

    #[test]
    fn test_schema_file_invalid_error() {
        let err = NikaError::SchemaFileInvalid {
            task_id: "generate".to_string(),
            path: "./schemas/broken.json".to_string(),
            reason: "expected value at line 1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-014");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-014]"));
        assert!(msg.contains("generate"));
        assert!(msg.contains("broken.json"));
        assert!(msg.contains("expected value"));
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
    fn test_execution_error() {
        let err = NikaError::Execution("command not found".to_string());
        assert_eq!(err.code(), "NIKA-096");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-096]"));
        assert!(msg.contains("Execution error"));
    }

    #[test]
    fn test_exec_error() {
        let err = NikaError::ExecError {
            reason: "Failed to spawn command: not found".to_string(),
        };
        assert_eq!(err.code(), "NIKA-044");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-044]"));
        assert!(msg.contains("Exec error"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_fetch_error() {
        let err = NikaError::FetchError {
            reason: "HTTP server error: 503".to_string(),
        };
        assert_eq!(err.code(), "NIKA-045");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-045]"));
        assert!(msg.contains("Fetch error"));
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_extract_error() {
        let err = NikaError::ExtractError {
            reason: "Invalid CSS selector: >>>".to_string(),
        };
        assert_eq!(err.code(), "NIKA-046");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-046]"));
        assert!(msg.contains("Extract error"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_task_panicked() {
        let err = NikaError::TaskPanicked {
            reason: "index out of bounds".to_string(),
        };
        assert_eq!(err.code(), "NIKA-098");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-098]"));
        assert!(msg.contains("Task panicked"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_template_error_with_path() {
        let err = NikaError::TemplateError {
            template: "{{with.result}}".to_string(),
            reason: "alias not in with block".to_string(),
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

    #[test]
    fn test_blocked_command_error() {
        let err = NikaError::BlockedCommand {
            command: "rm -rf /".to_string(),
            reason: "Destructive command blocked by security policy".to_string(),
        };
        assert_eq!(err.code(), "NIKA-053");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-053]"));
        assert!(msg.contains("rm -rf /"));
        assert!(msg.contains("blocked"));
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
    // WITH BLOCK VALIDATION (070-079)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_unknown_alias_error() {
        let err = NikaError::UnknownAlias {
            alias: "undefined".to_string(),
            task_id: "current_task".to_string(),
            suggestion: None,
        };
        assert_eq!(err.code(), "NIKA-071");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-071]"));
        assert!(msg.contains("undefined"));
    }

    #[test]
    fn test_unknown_alias_with_suggestion() {
        let err = NikaError::UnknownAlias {
            alias: "dta".to_string(),
            task_id: "summarize".to_string(),
            suggestion: Some("data".to_string()),
        };
        let msg = err.to_string();
        assert!(msg.contains("Did you mean 'data'?"), "got: {msg}");
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
    fn test_with_unknown_task_error() {
        let err = NikaError::WithUnknownTask {
            alias: "ctx".to_string(),
            from_task: "undefined".to_string(),
            task_id: "current".to_string(),
            suggestion: None,
        };
        assert_eq!(err.code(), "NIKA-080");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-080]"));
        assert!(msg.contains("undefined"));
    }

    #[test]
    fn test_with_not_upstream_error() {
        let err = NikaError::WithNotUpstream {
            alias: "ctx".to_string(),
            from_task: "task2".to_string(),
            task_id: "task1".to_string(),
        };
        assert_eq!(err.code(), "NIKA-081");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-081]"));
    }

    #[test]
    fn test_with_circular_dep_error() {
        let err = NikaError::WithCircularDep {
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
        // Use serde_json::Value as target since serde-saphyr doesn't export Value type
        let yaml_err = serde_yaml::from_str::<serde_json::Value>(yaml_str);
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
            tool: "novanet_context".to_string(),
            reason: "invalid parameters".to_string(),
            error_code: None,
        };
        assert_eq!(err.code(), "NIKA-102");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-102]"));
        assert!(msg.contains("novanet_context"));
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
        // Error code description should be included in display
        // McpErrorCode::InvalidRequest displays as "The JSON sent is not a valid Request object (-32600)"
        assert!(msg.contains("Request") || msg.contains("-32600"));
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
            tool: "novanet_search".to_string(),
            reason: "missing 'result' field".to_string(),
        };
        assert_eq!(err.code(), "NIKA-106");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-106]"));
    }

    #[test]
    fn test_mcp_validation_failed_error() {
        let err = NikaError::McpValidationFailed {
            tool: "novanet_context".to_string(),
            details: "parameter validation failed".to_string(),
            missing: vec!["focus_key".to_string(), "locale".to_string()],
            suggestions: vec!["Check parameter names".to_string()],
        };
        assert_eq!(err.code(), "NIKA-107");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-107]"));
    }

    #[test]
    fn test_mcp_schema_error() {
        let err = NikaError::McpSchemaError {
            tool: "novanet_context".to_string(),
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
            operation: "novanet_context".to_string(),
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
    fn test_agent_validation_error() {
        let err = NikaError::AgentValidationError {
            reason: "empty prompt".to_string(),
        };
        assert_eq!(err.code(), "NIKA-113");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-113]"));
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
    fn test_agent_limit_exceeded_error() {
        let err = NikaError::AgentLimitExceeded {
            limit_type: "turns".to_string(),
            current: 10.0,
            maximum: 5.0,
        };
        assert_eq!(err.code(), "NIKA-114");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-114]"));
        assert!(msg.contains("turns"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // RESILIENCE ERRORS (120-129)
    // ═══════════════════════════════════════════════════════════════════════════

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
            tool: "novanet_audit".to_string(),
            reason: "malformed response".to_string(),
        };
        assert_eq!(err.code(), "NIKA-110");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-110]"));
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
    // CONFIG ERRORS (135-139)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_config_error() {
        let err = NikaError::ConfigError {
            reason: "invalid TOML syntax".to_string(),
        };
        assert_eq!(err.code(), "NIKA-135");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-135]"));
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
        assert_eq!(err.code(), "NIKA-200");
        let msg = err.to_string();
        assert!(msg.contains("TOOL-001"));
        assert!(msg.contains("File not found"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FIX SUGGESTION TRAIT TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_fix_suggestion_for_all_recoverable_errors() {
        let err = NikaError::Timeout {
            operation: "slow_op".to_string(),
            duration_ms: 5000,
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

    // ═══════════════════════════════════════════════════════════════════════════
    // STRUCTURED OUTPUT ERRORS (300-309)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_structured_output_extraction_failed_error() {
        let err = NikaError::StructuredOutputExtractionFailed {
            task_id: "generate_json".to_string(),
            layer: "rig_extractor".to_string(),
            reason: "Failed to parse JSON from response".to_string(),
        };
        assert_eq!(err.code(), "NIKA-300");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-300]"));
        assert!(msg.contains("generate_json"));
        assert!(msg.contains("rig_extractor"));
        assert!(msg.contains("Failed to parse JSON"));
    }

    #[test]
    fn test_structured_output_validation_failed_error() {
        let err = NikaError::StructuredOutputValidationFailed {
            task_id: "validate_output".to_string(),
            layer: "extract_validate".to_string(),
            attempt: 2,
            errors: vec![
                "missing required field 'id'".to_string(),
                "invalid type for 'count': expected integer".to_string(),
            ],
        };
        assert_eq!(err.code(), "NIKA-301");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-301]"));
        assert!(msg.contains("validate_output"));
        assert!(msg.contains("extract_validate"));
        assert!(msg.contains("attempt 2"));
        assert!(msg.contains("2 errors"));
    }

    #[test]
    fn test_structured_output_validation_failed_single_error() {
        let err = NikaError::StructuredOutputValidationFailed {
            task_id: "single_error".to_string(),
            layer: "retry_with_feedback".to_string(),
            attempt: 1,
            errors: vec!["missing required field 'name'".to_string()],
        };
        assert_eq!(err.code(), "NIKA-301");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-301]"));
        assert!(msg.contains("missing required field 'name'"));
        // Should not contain "errors:" prefix for single error
        assert!(!msg.contains("1 errors:"));
    }

    #[test]
    fn test_structured_output_repair_failed_error() {
        let err = NikaError::StructuredOutputRepairFailed {
            task_id: "repair_task".to_string(),
            original_errors: vec!["invalid JSON syntax".to_string()],
            repair_errors: vec!["repair produced invalid output".to_string()],
        };
        assert_eq!(err.code(), "NIKA-302");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-302]"));
        assert!(msg.contains("repair_task"));
        assert!(msg.contains("original errors"));
        assert!(msg.contains("repair errors"));
    }

    #[test]
    fn test_structured_output_all_layers_failed_error() {
        let err = NikaError::StructuredOutputAllLayersFailed {
            task_id: "final_failure".to_string(),
            attempts: 4,
            final_errors: vec!["schema validation failed".to_string()],
        };
        assert_eq!(err.code(), "NIKA-303");
        let msg = err.to_string();
        assert!(msg.contains("[NIKA-303]"));
        assert!(msg.contains("final_failure"));
        assert!(msg.contains("4 attempts"));
    }

    #[test]
    fn test_all_structured_output_errors_have_correct_codes() {
        assert_eq!(
            NikaError::StructuredOutputExtractionFailed {
                task_id: "x".into(),
                layer: "y".into(),
                reason: "z".into()
            }
            .code(),
            "NIKA-300"
        );
        assert_eq!(
            NikaError::StructuredOutputValidationFailed {
                task_id: "x".into(),
                layer: "y".into(),
                attempt: 1,
                errors: vec![]
            }
            .code(),
            "NIKA-301"
        );
        assert_eq!(
            NikaError::StructuredOutputRepairFailed {
                task_id: "x".into(),
                original_errors: vec![],
                repair_errors: vec![]
            }
            .code(),
            "NIKA-302"
        );
        assert_eq!(
            NikaError::StructuredOutputAllLayersFailed {
                task_id: "x".into(),
                attempts: 1,
                final_errors: vec![]
            }
            .code(),
            "NIKA-303"
        );
    }

    #[test]
    fn test_structured_output_errors_fix_suggestions() {
        let extraction_err = NikaError::StructuredOutputExtractionFailed {
            task_id: "t".into(),
            layer: "l".into(),
            reason: "r".into(),
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&extraction_err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("JSON Schema"));

        let validation_err = NikaError::StructuredOutputValidationFailed {
            task_id: "t".into(),
            layer: "l".into(),
            attempt: 1,
            errors: vec![],
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&validation_err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("schema"));

        let repair_err = NikaError::StructuredOutputRepairFailed {
            task_id: "t".into(),
            original_errors: vec![],
            repair_errors: vec![],
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&repair_err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("simplifying"));

        let all_failed_err = NikaError::StructuredOutputAllLayersFailed {
            task_id: "t".into(),
            attempts: 1,
            final_errors: vec![],
        };
        let suggestion = <NikaError as FixSuggestion>::fix_suggestion(&all_failed_err);
        assert!(suggestion.is_some());
        assert!(suggestion.unwrap().contains("validation layers"));
    }

    #[test]
    fn test_structured_output_is_recoverable() {
        // Extraction, validation, and repair errors are recoverable
        let extraction = NikaError::StructuredOutputExtractionFailed {
            task_id: "x".into(),
            layer: "y".into(),
            reason: "z".into(),
        };
        assert!(extraction.is_recoverable());

        let validation = NikaError::StructuredOutputValidationFailed {
            task_id: "x".into(),
            layer: "y".into(),
            attempt: 1,
            errors: vec![],
        };
        assert!(validation.is_recoverable());

        let repair = NikaError::StructuredOutputRepairFailed {
            task_id: "x".into(),
            original_errors: vec![],
            repair_errors: vec![],
        };
        assert!(repair.is_recoverable());

        // All layers failed is NOT recoverable (final failure)
        let all_failed = NikaError::StructuredOutputAllLayersFailed {
            task_id: "x".into(),
            attempts: 4,
            final_errors: vec![],
        };
        assert!(!all_failed.is_recoverable());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // COURSE ERRORS (310-319)
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn test_course_not_found_code_and_display() {
        let err = NikaError::CourseNotFound {
            path: "/tmp/course".to_string(),
        };
        assert_eq!(err.code(), "NIKA-310");
        assert!(err.to_string().contains("NIKA-310"));
        assert!(err.to_string().contains("/tmp/course"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_course_check_failed_code_and_display() {
        let err = NikaError::CourseCheckFailed {
            exercise: "01_hello".to_string(),
            reason: "missing infer: verb".to_string(),
        };
        assert_eq!(err.code(), "NIKA-311");
        assert!(err.to_string().contains("NIKA-311"));
        assert!(err.to_string().contains("01_hello"));
        assert!(err.to_string().contains("missing infer: verb"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_course_level_locked_code_and_display() {
        let err = NikaError::CourseLevelLocked {
            level: "intermediate".to_string(),
            prerequisite: 1,
        };
        assert_eq!(err.code(), "NIKA-312");
        assert!(err.to_string().contains("NIKA-312"));
        assert!(err.to_string().contains("intermediate"));
        assert!(err.to_string().contains("level 1"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_course_progress_corrupted_code_and_display() {
        let err = NikaError::CourseProgressCorrupted {
            reason: "invalid JSON".to_string(),
        };
        assert_eq!(err.code(), "NIKA-313");
        assert!(err.to_string().contains("NIKA-313"));
        assert!(err.to_string().contains("invalid JSON"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_course_watch_error_code_and_display() {
        let err = NikaError::CourseWatchError {
            reason: "notify failed".to_string(),
        };
        assert_eq!(err.code(), "NIKA-314");
        assert!(err.to_string().contains("NIKA-314"));
        assert!(err.to_string().contains("notify failed"));
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_course_errors_fix_suggestions() {
        let not_found = NikaError::CourseNotFound {
            path: "/tmp".to_string(),
        };
        assert!(not_found.fix_suggestion().is_some());

        let check_failed = NikaError::CourseCheckFailed {
            exercise: "ex".to_string(),
            reason: "r".to_string(),
        };
        assert!(check_failed.fix_suggestion().is_some());

        let locked = NikaError::CourseLevelLocked {
            level: "l".to_string(),
            prerequisite: 1,
        };
        assert!(locked.fix_suggestion().is_some());

        let corrupted = NikaError::CourseProgressCorrupted {
            reason: "r".to_string(),
        };
        assert!(corrupted.fix_suggestion().is_some());

        let watch = NikaError::CourseWatchError {
            reason: "r".to_string(),
        };
        assert!(watch.fix_suggestion().is_some());
    }

    #[test]
    fn test_format_validation_errors_short_empty() {
        let result = format_validation_errors_short(&[]);
        assert_eq!(result, "no errors");
    }

    #[test]
    fn test_format_validation_errors_short_single() {
        let result = format_validation_errors_short(&["missing field".to_string()]);
        assert_eq!(result, "missing field");
    }

    #[test]
    fn test_format_validation_errors_short_multiple() {
        let result = format_validation_errors_short(&[
            "error 1".to_string(),
            "error 2".to_string(),
            "error 3".to_string(),
        ]);
        assert!(result.contains("3 errors:"));
        assert!(result.contains("error 1"));
        assert!(result.contains("error 2"));
        assert!(result.contains("error 3"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // ERROR CODE FORMAT VERIFICATION (NIKA-XXX)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Verify that representative error variants from different groups all
    /// produce codes with the "NIKA-" prefix.
    #[test]
    fn test_all_common_errors_have_nika_prefix() {
        let errors: Vec<NikaError> = vec![
            NikaError::ParseError {
                details: "bad".into(),
            },
            NikaError::ValidationError {
                reason: "missing tasks".into(),
            },
            NikaError::CycleDetected {
                cycle: "a -> b -> a".into(),
            },
            NikaError::ProviderNotConfigured {
                provider: "anthropic".into(),
            },
            NikaError::TemplateError {
                template: "{{with.x}}".into(),
                reason: "unresolved".into(),
            },
            NikaError::BlockedCommand {
                command: "rm -rf /".into(),
                reason: "dangerous".into(),
            },
            NikaError::McpNotConnected {
                name: "novanet".into(),
            },
            NikaError::GuardrailViolation {
                task_id: "t".into(),
                violations: vec!["schema invalid".into()],
            },
            NikaError::StructuredOutputExtractionFailed {
                task_id: "t".into(),
                layer: "L2".into(),
                reason: "no json".into(),
            },
            NikaError::WorkflowTimeout {
                duration_secs: 60,
                running_tasks: vec!["t1".into()],
            },
        ];

        for err in &errors {
            let code = err.code();
            assert!(
                code.starts_with("NIKA-"),
                "Error {:?} has code '{}' which does not start with 'NIKA-'",
                std::mem::discriminant(err),
                code
            );
        }
    }

    /// Verify that error codes match the "NIKA-NNN" format (3 digits).
    #[test]
    fn test_error_code_is_numeric_suffix() {
        let errors: Vec<NikaError> = vec![
            NikaError::ParseError {
                details: "x".into(),
            },
            NikaError::InvalidSchemaVersion {
                version: "0.1".into(),
            },
            NikaError::CycleDetected {
                cycle: "a->b".into(),
            },
            NikaError::ProviderApiError {
                message: "server error".into(),
            },
            NikaError::ExecError {
                reason: "fail".into(),
            },
            NikaError::FetchError {
                reason: "timeout".into(),
            },
            NikaError::UnknownAlias {
                alias: "x".into(),
                task_id: "t".into(),
                suggestion: None,
            },
            NikaError::McpToolError {
                tool: "search".into(),
                reason: "fail".into(),
                error_code: None,
            },
            NikaError::AgentValidationError {
                reason: "bad".into(),
            },
            NikaError::ToolError {
                code: "NIKA-200".into(),
                message: "no file".into(),
            },
            NikaError::SkillLoadError {
                skill: "tone".into(),
                reason: "not found".into(),
            },
            NikaError::ArtifactWriteError {
                path: "/out/x".into(),
                reason: "perm".into(),
            },
            NikaError::CourseNotFound {
                path: "/c".into(),
            },
        ];

        let re = regex::Regex::new(r"^NIKA-\d{3}$").unwrap();
        for err in &errors {
            let code = err.code();
            assert!(
                re.is_match(code),
                "Error code '{}' does not match NIKA-NNN format",
                code
            );
        }
    }

    /// WorkflowTimeout must map to NIKA-038 specifically.
    #[test]
    fn test_workflow_timeout_has_nika_038() {
        let err = NikaError::WorkflowTimeout {
            duration_secs: 120,
            running_tasks: vec!["step1".into(), "step2".into()],
        };
        assert_eq!(err.code(), "NIKA-038");
    }

    /// All common error variants must have NIKA-NNN format codes.
    #[test]
    fn test_error_codes_have_nika_prefix() {
        let errors: Vec<NikaError> = vec![
            NikaError::ParseError { details: "x".into() },
            NikaError::CycleDetected { cycle: "a→b→a".into() },
            NikaError::Execution("x".into()),
            NikaError::WorkflowTimeout { duration_secs: 1, running_tasks: vec![] },
            NikaError::ValidationError { reason: "x".into() },
        ];
        for err in &errors {
            let code = err.code();
            assert!(
                code.starts_with("NIKA-"),
                "Error {:?} has code '{}' without NIKA- prefix",
                std::mem::discriminant(err),
                code,
            );
            // Verify numeric suffix (3 digits)
            let suffix = &code[5..];
            assert!(
                suffix.len() == 3 && suffix.chars().all(|c| c.is_ascii_digit()),
                "Error code '{}' should have 3-digit suffix, got '{}'",
                code,
                suffix,
            );
        }
    }

    /// WorkflowTimeout display now includes NIKA-038 prefix.
    #[test]
    fn test_workflow_timeout_display_includes_prefix() {
        let err = NikaError::WorkflowTimeout {
            duration_secs: 60,
            running_tasks: vec!["task1".into()],
        };
        let msg = err.to_string();
        assert!(
            msg.contains("[NIKA-038]"),
            "WorkflowTimeout display should include [NIKA-038], got: {}",
            msg,
        );
    }
}
