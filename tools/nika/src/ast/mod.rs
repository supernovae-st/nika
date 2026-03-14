//! AST Module - Abstract Syntax Tree for YAML workflows
//!
//! # Two-Phase IR Architecture (v0.19)
//!
//! Nika uses a two-phase IR similar to rustc:
//!
//! 1. **raw** - Parsed from YAML with full span tracking (line:col)
//! 2. **analyzed** - Validated, references resolved, ready for execution (planned)
//!
//! The existing types (Workflow, Task, etc.) are the "legacy" AST that will
//! gradually migrate to use the new two-phase architecture.
//!
//! # Modules
//!
//! ## New (v0.19 Foundation)
//! - `raw`: Raw AST with `Spanned<T>` fields for precise error locations
//!
//! ## Legacy (being migrated)
//! - `workflow`: Workflow, Task, Flow, FlowEndpoint
//! - `action`: TaskAction, InferParams, ExecParams, FetchParams
//! - `invoke`: InvokeParams (v0.2 - MCP integration)
//! - `agent`: AgentParams (v0.2 - Agentic execution)
//! - `output`: OutputPolicy, OutputFormat
//! - `context`: ContextConfig (v0.9 - File loading at workflow start)
//! - `agent_def`: AgentDef (v0.6 - Reusable agent configurations)
//! - `skill_def`: SkillDef, SkillRef (v0.6 - Prompt augmentation)
//! - `include`: IncludeSpec (v0.9 - DAG fusion)
//! - `artifact`: ArtifactSpec, ArtifactsConfig (v0.18 - File persistence)
//! - `logging`: LogConfig, LogLevel (v0.18 - Level-filtered logging)
//!
//! These types represent the "what" - static structure parsed from YAML.
//! For runtime execution, see the `runtime` module.

// v0.19 Foundation - Two-Phase IR
pub mod analyzed;
pub mod analyzer;
pub mod lower;
pub mod raw;
pub mod schema;

// Security - YAML bomb protection (v0.27.1)
pub mod budget;

// Legacy modules (being migrated)
mod action;
mod agent;
mod agent_def;
pub mod artifact;
pub mod completion;
pub mod context;
pub mod decompose;
pub mod guardrails;
pub mod import_loader;
pub mod include;
pub mod include_loader;
mod invoke;
pub mod limits;
pub mod loader;
pub mod logging;
pub mod output;
pub mod pkg_resolver;
pub mod schema_validator;
pub mod skill_def;
pub mod structured;
mod workflow;

// Re-export all public types
pub use action::{ExecParams, FetchParams, InferParams, TaskAction};
// AgentParams + ToolChoice are defined in agent.rs
pub use agent::{AgentParams, ToolChoice};
// AgentDef is defined in agent_def.rs (v0.6 - Reusable agent configurations)
pub use agent_def::AgentDef;
// InvokeParams is defined in invoke.rs and re-exported here
// (also used by action.rs for TaskAction::Invoke variant)
pub use invoke::InvokeParams;
// ContextConfig is defined in context.rs (v0.9 - File loading at workflow start)
pub use context::ContextConfig;
// IncludeSpec is defined in include.rs (v0.9 - DAG fusion)
pub use include::IncludeSpec;
pub use output::{OutputFormat, OutputPolicy, SchemaRef};
// SkillDef + SkillRef are defined in skill_def.rs (v0.6 - Prompt augmentation)
pub use skill_def::{SkillDef, SkillRef};
// PkgUri is defined in pkg_resolver.rs (v0.15.2 - Skill Ecosystem)
pub use pkg_resolver::PkgUri;
pub use workflow::{Flow, FlowEndpoint, McpConfigInline, Task, Workflow};
// DecomposeSpec is defined in decompose.rs (v0.5 - Runtime DAG expansion)
pub use decompose::{DecomposeSpec, DecomposeStrategy};
// Loader is defined in loader.rs (v0.13 - Multi-format agent/skill loading)
pub use loader::{discover_definitions, load_definition, DefinitionKind, LoadedDefinition};
// Import loader is defined in import_loader.rs (v0.28 - Raw-level import expansion)
pub use import_loader::expand_imports;
// Include loader is defined in include_loader.rs (v0.14.2 - DAG fusion, legacy)
pub use include_loader::expand_includes;
// StructuredOutputSpec is defined in structured.rs (v0.21 - JSON Schema validation)
pub use structured::StructuredOutputSpec;
// CompletionConfig is defined in completion.rs (v0.21 - Agent completion behavior)
pub use completion::{
    CompletionConfig, CompletionMode, ConfidenceConfig, InstructionConfig, LowConfidenceAction,
    PatternConfig, PatternType, SignalConfig, SignalFields,
};
// LimitsConfig is defined in limits.rs (v0.24 - Agent execution limits)
pub use limits::{LimitAction, LimitStatus, LimitType, LimitsConfig, OnLimitReachedConfig};
// Lowering: Analyzed AST → Legacy AST (v0.28 - Bridge Pattern Phase 3)
pub use lower::lower;

// ============================================================================
// Unified Pipeline: YAML → Raw → Analyzed → Legacy (v0.28 - Bridge Pattern Phase 4)
// ============================================================================

use crate::error::NikaError;
use crate::source::FileId;

/// Parse a YAML workflow through the unified two-phase IR pipeline.
///
/// Pipeline: `YAML → raw::parse → analyzer::analyze → lower → Workflow`
///
/// This replaces the legacy `serde_yaml::from_str::<Workflow>()` path with
/// the span-tracking, validating two-phase IR pipeline. The returned Workflow
/// is the legacy type expected by `expand_includes` and `Runner`.
///
/// # Errors
///
/// - `NikaError::ParseError` — YAML syntax or structural errors (Phase 1)
/// - `NikaError::ValidationError` — Semantic validation errors (Phase 2)
pub fn parse_workflow(yaml: &str) -> Result<Workflow, NikaError> {
    // Phase 1: YAML → Raw AST (with span tracking)
    let raw = raw::parse(yaml, FileId(0)).map_err(|e| NikaError::ParseError {
        details: format!("[{}] {}", e.kind.code(), e.message),
    })?;

    // Phase 2: Raw → Analyzed (validation, reference resolution)
    let analyzed = analyzer::analyze(raw).into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        NikaError::ValidationError {
            reason: messages.join("; "),
        }
    })?;

    // Phase 3: Analyzed → Legacy Workflow (for runtime compatibility)
    Ok(lower(analyzed))
}
