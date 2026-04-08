// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! AST Module - Abstract Syntax Tree for YAML workflows
//!
//! # Three-Phase Pipeline
//!
//! Nika uses a three-phase pipeline similar to rustc:
//!
//! ```text
//! YAML → raw::parse → analyzer::analyze → lower → Workflow → Runtime
//! ```
//!
//! 1. **raw** — Parsed from YAML with full span tracking (line:col)
//! 2. **analyzed** — Validated, references resolved, TaskId interning
//! 3. **runtime types** — `Workflow`, `Task`, etc. consumed by the execution engine
//!
//! The `lower` module converts analyzed AST into the runtime types.
//!
//! # Modules
//!
//! ## Pipeline
//! - `raw`: Raw AST with `Spanned<T>` fields for precise error locations
//! - `analyzed`: Validated AST with TaskId interning and semantic checks
//! - `analyzer`: Validation and transformation (raw → analyzed)
//! - `lower`: Lowering (analyzed → runtime types)
//!
//! ## Runtime Types
//! - `workflow`: Workflow, Task, McpConfigInline
//! - `action`: TaskAction, InferParams, ExecParams, FetchParams
//! - `invoke`: InvokeParams (MCP integration)
//! - `agent`: AgentParams (Agentic execution)
//! - `output`: OutputPolicy, OutputFormat
//! - `context`: ContextConfig (File loading at workflow start)
//! - `agent_def`: AgentDef (Reusable agent configurations)
//! - `skill_def`: SkillDef, SkillRef (Prompt augmentation)
//! - `include`: IncludeSpec (DAG fusion)
//! - `artifact`: ArtifactSpec, ArtifactsConfig (File persistence)
//! - `logging`: LogConfig, LogLevel (Level-filtered logging)
//!
//! These types represent the "what" — static structure parsed from YAML.
//! For execution, see the `runtime` module.

// Pipeline stages — re-exported from nika-core (synced in v0.40.0)
pub use nika_core::ast::analyzed;
pub use nika_core::ast::analyzer;
pub mod lower;
pub use nika_core::ast::raw;
pub mod schema {
    pub use nika_core::ast::schema::*;
}

// Re-export from nika-core: identical type modules
// These modules were deleted from nika (identical copies) and are now
// served from nika-core to eliminate duplication.
pub mod budget {
    pub use nika_core::ast::budget::*;
}
pub mod content {
    pub use nika_core::ast::content::*;
}
pub mod agent_def {
    pub use nika_core::ast::agent_def::*;
}
pub mod artifact {
    pub use nika_core::ast::artifact::*;
}
pub mod context {
    pub use nika_core::ast::context::*;
}
pub mod decompose {
    pub use nika_core::ast::decompose::*;
}
pub mod extract {
    pub use nika_core::ast::extract::*;
}
pub mod logging {
    pub use nika_core::ast::logging::*;
}
pub mod output {
    pub use nika_core::ast::output::*;
}
pub mod structured {
    pub use nika_core::ast::structured::*;
}

#[cfg(test)]
mod tests_200_workflows;

// Re-export from nika-core: modules synced with CoreError variants
pub mod completion {
    pub use nika_core::ast::completion::*;
}
pub mod guardrails {
    pub use nika_core::ast::guardrails::*;
}
pub mod include {
    pub use nika_core::ast::include::*;
}
pub mod limits {
    pub use nika_core::ast::limits::*;
}

// Runtime types (consumed by runner/executor — depend on NikaError)
mod action;
mod agent;
pub mod import_loader;
// include_loader removed — expand_raw_include (import_loader.rs) handles DAG fusion
mod invoke;
pub mod loader;
pub mod pkg_resolver;
pub mod schema_validator;
pub mod skill_def;
mod workflow;

// Re-export all public types
pub use action::{ExecParams, FetchParams, InferParams, ResponseFormat, TaskAction};
// AgentParams + ToolChoice are defined in agent.rs
pub use agent::{AgentParams, ToolChoice};
// AgentDef is defined in agent_def.rs
pub use agent_def::AgentDef;
// InvokeParams is defined in invoke.rs and re-exported here
// (also used by action.rs for TaskAction::Invoke variant)
pub use invoke::InvokeParams;
// ContextConfig is defined in context.rs
pub use context::ContextConfig;
// IncludeSpec is defined in include.rs
pub use include::IncludeSpec;
pub use output::{OutputFormat, OutputPolicy, SchemaRef};
// SkillDef + SkillRef are defined in skill_def.rs
pub use skill_def::{SkillDef, SkillRef};
// PkgUri is defined in pkg_resolver.rs
pub use pkg_resolver::PkgUri;
pub use workflow::{McpConfigInline, Task, Workflow};
// DecomposeSpec is defined in decompose.rs
pub use decompose::{DecomposeSpec, DecomposeStrategy};
// Loader is defined in loader.rs
pub use loader::{discover_definitions, load_definition, DefinitionKind, LoadedDefinition};
// Import loader is defined in import_loader.rs
pub use import_loader::expand_raw_include;
// Include loader removed — raw-level expansion via import_loader is the active path
// StructuredOutputSpec is defined in structured.rs
pub use structured::StructuredOutputSpec;
// CompletionConfig is defined in completion.rs
pub use completion::{
    CompletionConfig, CompletionMode, ConfidenceConfig, InstructionConfig, LowConfidenceAction,
    PatternConfig, PatternType, SignalConfig, SignalFields,
};
// LimitsConfig is defined in limits.rs
pub use limits::{LimitAction, LimitStatus, LimitType, LimitsConfig, OnLimitReachedConfig};
pub use lower::{lower, unlower};

// ============================================================================
// Unified Pipeline: YAML → Raw → Analyzed → Workflow
// ============================================================================

use crate::ast::analyzed::AnalyzedWorkflow;
use crate::error::NikaError;
use crate::source::FileId;

/// Parse a YAML workflow through the three-phase pipeline.
///
/// Pipeline: `YAML → raw::parse → analyzer::analyze → lower → Workflow`
///
/// Returns the runtime `Workflow` type consumed by `expand_includes` and `Runner`.
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

    // Phase 3: Analyzed → Runtime Workflow
    lower(analyzed)
}

/// Parse a YAML workflow with include expansion through the full pipeline.
///
/// Pipeline: `YAML → raw::parse → expand_raw_include → analyzer::analyze → lower → Workflow`
///
/// Expands `include:` partials into the raw AST before analysis so cross-file
/// references work. Returns the lowered `Workflow` for DAG validation.
pub fn parse_workflow_with_includes(
    yaml: &str,
    base_path: &std::path::Path,
) -> Result<Workflow, NikaError> {
    let raw = raw::parse(yaml, FileId(0)).map_err(|e| NikaError::ParseError {
        details: format!("[{}] {}", e.kind.code(), e.message),
    })?;

    let raw = expand_raw_include(raw, base_path)?;

    let analyzed = analyzer::analyze(raw).into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        NikaError::ValidationError {
            reason: messages.join("; "),
        }
    })?;

    lower(analyzed)
}

/// Parse a YAML workflow and return the AnalyzedWorkflow directly.
///
/// Pipeline: `YAML → raw::parse → analyzer::analyze → AnalyzedWorkflow`
///
/// Skips the lowering step. The returned `AnalyzedWorkflow` is consumed
/// directly by the `Runner`, which handles bridge conversions at the
/// `TaskExecutor` boundary.
///
/// # Errors
///
/// - `NikaError::ParseError` — YAML syntax or structural errors (Phase 1)
/// - `NikaError::ValidationError` — Semantic validation errors (Phase 2)
pub fn parse_analyzed(yaml: &str) -> Result<AnalyzedWorkflow, NikaError> {
    // Phase 1: YAML → Raw AST (with span tracking)
    let raw = raw::parse(yaml, FileId(0)).map_err(|e| NikaError::ParseError {
        details: format!("[{}] {}", e.kind.code(), e.message),
    })?;

    // Phase 2: Raw → Analyzed (validation, reference resolution)
    analyzer::analyze(raw).into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        NikaError::ValidationError {
            reason: messages.join("; "),
        }
    })
}

/// Parse a YAML workflow with include expansion, returning AnalyzedWorkflow.
///
/// Pipeline: `YAML → raw::parse → expand_raw_include → analyzer::analyze → AnalyzedWorkflow`
///
/// Unlike `parse_analyzed`, this expands `include:` partials into the raw AST
/// BEFORE analysis, so included tasks can reference each other and the main
/// workflow's tasks. This is the correct pipeline for workflows with `include:`.
///
/// # Errors
///
/// - `NikaError::ParseError` — YAML syntax or structural errors (Phase 1)
/// - `NikaError::ValidationError` — Semantic validation errors (Phase 2)
/// - `NikaError::WorkflowNotFound` — Include file not found
pub fn parse_analyzed_with_includes(
    yaml: &str,
    base_path: &std::path::Path,
) -> Result<AnalyzedWorkflow, NikaError> {
    // Phase 1: YAML → Raw AST (with span tracking)
    let raw = raw::parse(yaml, FileId(0)).map_err(|e| NikaError::ParseError {
        details: format!("[{}] {}", e.kind.code(), e.message),
    })?;

    // Phase 1.5: Expand includes (merge partial workflows into raw AST)
    let raw = expand_raw_include(raw, base_path)?;

    // Phase 2: Raw → Analyzed (validation on the merged AST)
    analyzer::analyze(raw).into_result().map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind.code(), e))
            .collect();
        NikaError::ValidationError {
            reason: messages.join("; "),
        }
    })
}
