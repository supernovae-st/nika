//! AST Module - Abstract Syntax Tree for YAML workflows
//!
//! # Three-Phase Pipeline
//!
//! ```text
//! YAML -> raw::parse -> analyzer::analyze -> AnalyzedWorkflow
//! ```
//!
//! 1. **raw** -- Parsed from YAML with full span tracking (line:col)
//! 2. **analyzed** -- Validated, references resolved, TaskId interning
//!
//! The lowering step (analyzed -> runtime types) lives in the `nika` crate,
//! since it depends on runtime types not available here.
//!
//! # Modules
//!
//! ## Pipeline
//! - `raw`: Raw AST with `Spanned<T>` fields for precise error locations
//! - `analyzed`: Validated AST with TaskId interning and semantic checks
//! - `analyzer`: Validation and transformation (raw -> analyzed)
//!
//! ## Type Modules
//! - `schema`: SchemaVersion enum
//! - `budget`: YAML bomb protection (Budget, from_str_with_budget)
//! - `output`: OutputPolicy, OutputFormat, SchemaRef
//! - `decompose`: DecomposeSpec, DecomposeStrategy
//! - `context`: ContextConfig (file loading at workflow start)
//! - `logging`: LogConfig, LogLevel, LogFormat
//! - `structured`: StructuredOutputSpec (JSON schema enforcement)
//! - `agent_def`: AgentDef (reusable agent configurations)
//! - `artifact`: ArtifactsConfig, ArtifactSpec (file persistence)
//! - `content`: ContentPart types (multimodal vision support)
//! - `limits`: LimitsConfig (agent resource limits)
//! - `include`: IncludeSpec (DAG fusion)

// Pipeline stages
pub mod analyzed;
pub mod analyzer;
pub mod raw;
pub mod schema;

// Security - YAML bomb protection
pub mod budget;

// Vision/multimodal content parts
pub mod content;

// Template-aware typed values
pub mod templatable;

// Type modules
pub mod agent_def;
pub mod artifact;
pub mod completion;
pub mod context;
pub mod decompose;
pub mod extract;
pub mod guardrails;
pub mod include;
pub mod limits;
pub mod logging;
pub mod orchestrate;
pub mod output;
pub mod record;
pub mod routing;
pub mod schedule;
pub mod structured;

// Re-export key types for convenient access
pub use agent_def::AgentDef;
pub use context::ContextConfig;
pub use decompose::{DecomposeSpec, DecomposeStrategy};
pub use extract::{ExtractMode, ResponseMode};
pub use include::IncludeSpec;
pub use limits::{LimitAction, LimitStatus, LimitType, LimitsConfig, OnLimitReachedConfig};
pub use logging::{LogConfig, LogFormat, LogLevel};
pub use orchestrate::OrchestrateConfig;
pub use output::{OutputFormat, OutputPolicy, SchemaRef};
pub use record::RecordSpec;
pub use routing::RoutingConfig;
pub use schema::SchemaVersion;
pub use structured::StructuredOutputSpec;
pub use templatable::Templatable;

// ============================================================================
// Unified Pipeline: YAML -> Raw -> Analyzed
// ============================================================================

use crate::ast::analyzed::AnalyzedWorkflow;
use crate::error::CoreError;
use crate::source::FileId;

/// Parse a YAML workflow and return the AnalyzedWorkflow directly.
///
/// Pipeline: `YAML -> raw::parse -> analyzer::analyze -> AnalyzedWorkflow`
///
/// # Errors
///
/// - `CoreError::ValidationError` -- YAML syntax or structural errors (Phase 1)
/// - `CoreError::ValidationError` -- Semantic validation errors (Phase 2)
pub fn parse_analyzed(yaml: &str) -> Result<AnalyzedWorkflow, CoreError> {
    // Phase 1: YAML -> Raw AST (with span tracking)
    let raw = raw::parse(yaml, FileId(0)).map_err(|e| CoreError::ValidationError {
        reason: format!("[{}] {}", e.kind.code(), e.message),
    })?;

    // Phase 2: Raw -> Analyzed (validation, reference resolution)
    analyzer::analyze(raw).into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        CoreError::ValidationError {
            reason: messages.join("; "),
        }
    })
}
